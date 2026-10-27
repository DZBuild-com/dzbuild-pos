//! What comes back from DZBuild (spec §4.7, research 06 §5.5–5.6): the events feed (long-poll
//! `GET /v1/events?cursor=&wait=25`) or, while DZBuild has not shipped it, pulls by
//! `updated_since`. Each event is applied once (the inbox), in one transaction with the cursor;
//! a detail fetched for it (a product, an order) is fetched before the transaction.
//! Rules: for the fields the owner edits online — the name in the store's language, the price in
//! a TTC shop, active/draft — the online edit wins, and a local edit of the same field waiting to
//! be pushed loses with a `conflict` notice; everything else is the POS's. Stock moved on DZBuild
//! by the dashboard, the API, or an order the POS does not take in becomes a local adjustment
//! (`ref_type = 'dzbuild'`) acknowledged in the baseline; our own moves (`source = pos`) are
//! echoes. Blacklist flags are salted phone hashes. A revocation is reported to the engine.
//! One item the catalogue or the mirror refuses (a name longer than the catalogue takes, a
//! negative fee) is undone alone and recorded `failed`: it never blocks the feed behind it. An
//! event whose detail DZBuild answers in a shape this version cannot read holds its page up for
//! the engine's next try; its last try records it `failed` with one notice.

use super::api::Remote;
use super::http::{Call, CallError, Endpoint, Lane, Page};
use super::orders::{self, RemoteOrder, UpsertOutcome};
use super::payloads::ProductSnapshot;
use super::store::{self, CloudLink, CloudNoticeKind, CloudSettings, MapEntity};
use super::wire::{ApiMoney, ApiTime, IdString, de_id, iso_time, parse_time};
use crate::audit::{self, Actor};
use crate::catalog::products::{self, Barcode, Product, ProductInput, ProductKind};
use crate::company::PriceMode;
use crate::db::Db;
use crate::error::AppError;
use crate::money::{Money, Qty, Rate};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_json::value::RawValue;
use sha2::{Digest, Sha256};
use specta::Type;
use std::collections::BTreeMap;

pub const FLAG_THRESHOLD: i64 = 70;
const EVENTS: &str = "cloud:events";
const PRODUCTS: &str = "cloud:products";
const ORDERS: &str = "cloud:orders";
const EPOCH: &str = "1970-01-01T01:00:00+01:00";

#[derive(Debug, Clone, Deserialize)]
pub struct EventItem {
    #[serde(deserialize_with = "de_id")]
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub data: Box<RawValue>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemotePricing {
    pub price: ApiMoney,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RemoteInventory {
    #[serde(default)]
    pub track_stock: bool,
    #[serde(default)]
    pub stock_quantity: Option<i64>,
    #[serde(default)]
    pub sku: Option<String>,
    #[serde(default)]
    pub barcode: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteProduct {
    #[serde(deserialize_with = "de_id")]
    pub id: String,
    #[serde(default)]
    pub external_id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub sku: Option<String>,
    #[serde(default)]
    pub barcode: Option<String>,
    pub pricing: RemotePricing,
    #[serde(default)]
    pub inventory: Option<RemoteInventory>,
    pub status: String,
    #[serde(default)]
    pub has_variants: bool,
    #[serde(default)]
    pub updated_at: Option<ApiTime>,
    /// The variant family (contract v1.12); a block this version cannot read is none.
    #[serde(default, deserialize_with = "de_variants")]
    pub variants: Option<RemoteVariants>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteVariants {
    pub axes: Vec<RemoteAxis>,
    #[serde(default)]
    pub stock_mode: String,
    pub items: Vec<RemoteVariantItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteAxis {
    pub name: String,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteVariantItem {
    pub key: String,
    #[serde(default)]
    pub external_id: Option<String>,
    #[serde(default)]
    pub sku: Option<String>,
    #[serde(default)]
    pub barcode: Option<String>,
    pub price: ApiMoney,
    pub attributes: BTreeMap<String, String>,
    #[serde(default)]
    pub stock: Option<i64>,
    #[serde(default = "active_by_default")]
    pub active: bool,
}

fn active_by_default() -> bool {
    true
}

/// Read from the raw text (amounts are exact decimals), so a malformed block fails alone.
fn de_variants<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<RemoteVariants>, D::Error> {
    let raw: Option<Box<RawValue>> = Option::deserialize(d)?;
    Ok(raw.and_then(|r| match serde_json::from_str(r.get()) {
        Ok(variants) => Some(variants),
        Err(e) => {
            tracing::warn!(target: "cloud", error = %e, "a variants block this version cannot read");
            None
        }
    }))
}

impl RemoteProduct {
    /// The family the till can hold: one to three axes and 1 to 200 items (`variants::generate`).
    fn family(&self) -> Option<&RemoteVariants> {
        self.variants
            .as_ref()
            .filter(|v| (1..=3).contains(&v.axes.len()) && (1..=200).contains(&v.items.len()))
    }
    fn sku(&self) -> Option<&str> {
        self.sku
            .as_deref()
            .or(self.inventory.as_ref().and_then(|i| i.sku.as_deref()))
            .filter(|s| !s.trim().is_empty())
    }
    fn barcode(&self) -> Option<&str> {
        self.barcode
            .as_deref()
            .or(self.inventory.as_ref().and_then(|i| i.barcode.as_deref()))
            .filter(|s| !s.trim().is_empty())
    }
}

#[derive(Debug, Deserialize)]
struct StockEvent {
    #[serde(default)]
    product_id: Option<IdString>,
    #[serde(default)]
    external_id: Option<String>,
    #[serde(default)]
    old: Option<i64>,
    #[serde(default)]
    new: Option<i64>,
    #[serde(default)]
    delta: Option<i64>,
    #[serde(default)]
    source: String,
    /// The online order that moved the stock (`source` `order`), when DZBuild names it.
    #[serde(default)]
    order_number: Option<IdString>,
    /// The variant whose stock moved (contract v1.12); none at product level.
    #[serde(default)]
    variant_key: Option<String>,
    #[serde(default)]
    variant_external_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RefEvent {
    #[serde(default)]
    id: Option<IdString>,
    #[serde(default)]
    external_id: Option<String>,
    #[serde(default)]
    device_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CustomerEvent {
    #[serde(deserialize_with = "de_id")]
    id: String,
    #[serde(default)]
    phone: Option<String>,
    #[serde(default)]
    is_banned: bool,
    #[serde(default)]
    fraud_score: Option<i64>,
}

pub struct PullContext<'a> {
    pub remote: &'a Remote,
    pub db: &'a Db,
    pub link: &'a CloudLink,
    pub settings: &'a CloudSettings,
    pub actor: Actor,
    pub now: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PullReport {
    pub events: usize,
    pub applied: usize,
    pub products: usize,
    pub conflicts: usize,
    pub stock_moves: usize,
    pub orders: usize,
    pub flags: usize,
    /// DZBuild revoked this device or the link: the engine suspends it.
    pub revoked: bool,
    /// Events recorded `failed` because their detail could not be read ([`Unreadable::Record`]).
    pub unreadable: usize,
}

/// What a try does with an event whose detail DZBuild answered in a shape this version cannot
/// read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unreadable {
    /// The page waits for the next try, and the events behind it with it.
    Wait,
    /// The last try: the event is recorded `failed` in the inbox with one
    /// [`UNREADABLE_NOTICE`], and the rest of the page applies.
    Record,
}

/// The notice of the events recorded `failed` because their detail could not be read.
pub const UNREADABLE_NOTICE: &str = "cloud_event_unreadable";

fn db_stop(e: AppError) -> CallError {
    tracing::error!(target: "cloud", code = %e.code(), "database error while pulling");
    CallError::Invalid(format!("database: {}", e.code()))
}

/// This item's own refusal (a rule of the catalogue, a check of the mirror refused what DZBuild
/// sent), as opposed to a database that could not work.
fn refused_item(e: &AppError) -> bool {
    match e {
        AppError::Validation { .. } | AppError::NotFound { .. } | AppError::Rejected { .. } => true,
        AppError::Conflict { code } => {
            !matches!(code.as_str(), "database_busy" | "database_corrupt")
        }
        _ => false,
    }
}

/// The notices of a DZBuild item this PC refused (see [`refused_item`]): the feed goes on
/// without it, and the owner reads in Settings › DZBuild which item it was (the subject: a
/// product's online name, an order's number, else the event's type) and why (`details.reason`,
/// the refusal's code).
pub const PRODUCT_REFUSED_NOTICE: &str = "cloud_product_refused";
pub const ORDER_REFUSED_NOTICE: &str = "cloud_order_refused";
pub const EVENT_REFUSED_NOTICE: &str = "cloud_event_refused";

/// A notice's subject is cut to this many characters: an online name may be what was refused
/// (too long).
const SUBJECT_CHARS: usize = 60;

/// What a refused item tells the owner: one [`CloudNoticeKind::FailedItems`] notice naming it.
struct Refusal<'a> {
    notice: &'static str,
    subject: &'a str,
}

impl<'a> Refusal<'a> {
    fn product(remote: &'a RemoteProduct) -> Refusal<'a> {
        let name = remote.name.trim();
        Refusal {
            notice: PRODUCT_REFUSED_NOTICE,
            subject: if name.is_empty() { &remote.id } else { name },
        }
    }

    fn order(remote: &'a RemoteOrder) -> Refusal<'a> {
        Refusal {
            notice: ORDER_REFUSED_NOTICE,
            subject: &remote.order_number,
        }
    }

    fn event(kind: &'a str) -> Refusal<'a> {
        Refusal {
            notice: EVENT_REFUSED_NOTICE,
            subject: kind,
        }
    }

    fn raise(&self, tx: &Transaction<'_>, e: &AppError, now: i64) -> Result<(), AppError> {
        let subject = self.subject.trim();
        let subject = if subject.chars().count() > SUBJECT_CHARS {
            let cut: String = subject.chars().take(SUBJECT_CHARS).collect();
            format!("{}…", cut.trim_end())
        } else {
            subject.to_owned()
        };
        store::raise_notice(
            tx,
            CloudNoticeKind::FailedItems,
            self.notice,
            &subject,
            json!({ "reason": e.code() }),
            now,
        )?;
        Ok(())
    }
}

/// Runs one remote item in a savepoint. A refused item (see [`refused_item`]) has its writes
/// undone, raises its `refusal` notice (none for an item a dated pull was sent again unchanged:
/// it was told already) and gives `Ok(None)`, so one odd item never blocks the feed behind it;
/// any other error aborts the whole transaction, which is retried.
fn isolated<T>(
    tx: &Transaction<'_>,
    item: &str,
    refusal: Option<Refusal<'_>>,
    now: i64,
    apply: impl FnOnce() -> Result<T, AppError>,
) -> Result<Option<T>, AppError> {
    tx.execute_batch("SAVEPOINT pull_item")?;
    match apply() {
        Ok(value) => {
            tx.execute_batch("RELEASE pull_item")?;
            Ok(Some(value))
        }
        Err(e) if refused_item(&e) => {
            tx.execute_batch("ROLLBACK TO pull_item; RELEASE pull_item")?;
            tracing::warn!(target: "cloud", item, code = %e.code(), "a DZBuild item was refused here");
            if let Some(refusal) = refusal {
                refusal.raise(tx, &e, now)?;
            }
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

pub fn long_poll(
    remote: &Remote,
    cursor: Option<&str>,
    wait_secs: u64,
) -> Result<Page<EventItem>, CallError> {
    let mut call = Call::get(Endpoint::Events, "/events")
        .query("wait", wait_secs.to_string())
        .query("limit", "200")
        .timeout(std::time::Duration::from_secs(wait_secs.saturating_add(10)));
    if let Some(cursor) = cursor {
        call = call.query("cursor", cursor);
    }
    remote.data(call)
}

/// A detail fetched before the transaction.
enum Fetched {
    Product(RemoteProduct),
    Order(RemoteOrder),
    Profile(super::api::StoreProfile),
    Nothing,
    /// DZBuild answered in a shape this version cannot read (the decoding error).
    Unreadable(CallError),
    /// DZBuild refused the detail of an order or a product (a 403 or a 404 with a code): the
    /// event is recorded `failed` with a notice naming the remote id, never `ignored` in silence.
    Refused {
        code: String,
        remote_id: String,
    },
    /// The store's plan refuses the detail: recorded `failed` with one `plan_cap` notice, instead
    /// of holding the page forever.
    PlanCapped {
        code: String,
    },
}

/// `GET` of a detail: no answer or a refusal is the error; an answer this version cannot read is
/// `Ok(Err(..))`, so a try tells a dead network from a shape it will never read.
fn detail<T: DeserializeOwned>(
    remote: &Remote,
    call: Call,
) -> Result<Result<T, CallError>, CallError> {
    Ok(remote.send(call)?.data())
}

fn fetch(ctx: &PullContext<'_>, event: &EventItem) -> Result<Fetched, CallError> {
    let reference: Option<RefEvent> = serde_json::from_str(event.data.get()).ok();
    let id = reference
        .as_ref()
        .and_then(|r| r.id.as_ref())
        .map(|i| i.0.clone());
    let remote_id = id.clone().unwrap_or_default();
    let detailed = event.kind.starts_with("order.") || event.kind.starts_with("product.");
    let result = match (event.kind.as_str(), id) {
        ("product.created" | "product.updated", Some(id)) => detail(
            ctx.remote,
            Call::get(Endpoint::ProductGet, &format!("/products/{id}")),
        )
        .map(|read| read.map_or_else(Fetched::Unreadable, Fetched::Product)),
        (kind, Some(id)) if kind.starts_with("order.") && ctx.settings.orders_intake => detail(
            ctx.remote,
            Call::get(Endpoint::OrderGet, &format!("/orders/{id}")),
        )
        .map(|read| read.map_or_else(Fetched::Unreadable, Fetched::Order)),
        // `api::store_profile`, with the decoding apart
        ("store.plan_changed", _) => detail(ctx.remote, Call::get(Endpoint::Store, "/store"))
            .map(|read| read.map_or_else(Fetched::Unreadable, Fetched::Profile)),
        _ => Ok(Fetched::Nothing),
    };
    match result {
        Err(CallError::PlanCap { code }) => Ok(Fetched::PlanCapped { code }),
        Err(CallError::Rejected { code, .. }) if detailed && !remote_id.is_empty() => {
            Ok(Fetched::Refused { code, remote_id })
        }
        Err(CallError::Rejected { .. } | CallError::NotReady { .. }) => Ok(Fetched::Nothing),
        other => other,
    }
}

/// An error the last try never records: no answer, DZBuild down, a refused token.
/// Another call later may get the detail.
fn retried_later(e: &CallError) -> bool {
    matches!(
        e,
        CallError::Offline(_)
            | CallError::Server { .. }
            | CallError::Relink
            | CallError::Unauthorized
    )
}

/// The event is in the inbox: it was applied (or recorded) once already.
fn inboxed(conn: &Connection, store_id: &str, event_id: &str) -> Result<bool, AppError> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM cloud_inbox WHERE store_id = ?1 AND event_id = ?2)",
        params![store_id, event_id],
        |r| r.get(0),
    )?)
}

/// Applies a page of events and moves the cursor, in one transaction. An event whose detail
/// cannot be read holds the page up ([`Unreadable::Wait`]).
pub fn apply_events(
    ctx: &PullContext<'_>,
    page: &Page<EventItem>,
) -> Result<PullReport, CallError> {
    apply_events_with(ctx, page, Unreadable::Wait)
}

/// [`apply_events`]; with [`Unreadable::Record`] (the engine's last try) an event whose
/// detail cannot be read is recorded `failed` and the rest of the page applies.
pub fn apply_events_with(
    ctx: &PullContext<'_>,
    page: &Page<EventItem>,
    unreadable: Unreadable,
) -> Result<PullReport, CallError> {
    let store_id = ctx.link.store_id.clone();
    let mut fresh: Vec<(String, String, String, Fetched)> = Vec::new();
    for event in &page.items {
        if fresh.iter().any(|(id, ..)| *id == event.id) {
            continue;
        }
        // only spares the detail fetch of an event already applied; the transaction checks again
        let (s, id) = (store_id.clone(), event.id.clone());
        let seen = ctx.db.read(move |c| inboxed(c, &s, &id)).map_err(db_stop)?;
        if seen {
            continue;
        }
        let detail = match fetch(ctx, event) {
            Ok(Fetched::Unreadable(e)) if unreadable == Unreadable::Wait => return Err(e),
            Ok(detail) => detail,
            // the last try records any error a later call would meet again
            Err(e) if unreadable == Unreadable::Record && !retried_later(&e) => {
                Fetched::Unreadable(e)
            }
            Err(e) => return Err(e),
        };
        fresh.push((
            event.id.clone(),
            event.kind.clone(),
            event.data.get().to_owned(),
            detail,
        ));
    }
    let (link, settings, actor, now, next) = (
        ctx.link.clone(),
        ctx.settings.clone(),
        ctx.actor.clone(),
        ctx.now,
        page.next_cursor.clone(),
    );
    let total = page.items.len();
    ctx.db
        .write(move |tx| {
            let tx: &Transaction<'_> = tx;
            let mut report = PullReport {
                events: total,
                ..PullReport::default()
            };
            for (id, kind, data, detail) in &fresh {
                // checked again under the writer: another pull of the same page (the engine and
                // an owner's "sync now") may have applied it since the check above
                if inboxed(tx, &link.store_id, id)? {
                    continue;
                }
                if let Fetched::Unreadable(e) = detail {
                    tracing::warn!(target: "cloud", event_type = %kind, code = e.code(),
                        "an event whose detail cannot be read is recorded failed; the feed moves on");
                    tx.execute(
                        "INSERT INTO cloud_inbox (store_id, event_id, event_type, received_at, outcome)
                         VALUES (?1, ?2, ?3, ?4, 'failed')",
                        params![link.store_id, id, kind, now],
                    )?;
                    store::raise_notice(
                        tx,
                        CloudNoticeKind::FailedItems,
                        UNREADABLE_NOTICE,
                        "events",
                        json!({ "type": kind }),
                        now,
                    )?;
                    report.unreadable += 1;
                    continue;
                }
                if let Fetched::Refused { code, remote_id } = detail {
                    tracing::warn!(target: "cloud", event_type = %kind, code = %code,
                        "DZBuild refused the detail of an event; it is recorded failed");
                    tx.execute(
                        "INSERT INTO cloud_inbox (store_id, event_id, event_type, received_at, outcome)
                         VALUES (?1, ?2, ?3, ?4, 'failed')",
                        params![link.store_id, id, kind, now],
                    )?;
                    let notice = if kind.starts_with("order.") {
                        ORDER_REFUSED_NOTICE
                    } else {
                        EVENT_REFUSED_NOTICE
                    };
                    store::raise_notice(
                        tx,
                        CloudNoticeKind::FailedItems,
                        notice,
                        remote_id,
                        json!({ "reason": code, "type": kind }),
                        now,
                    )?;
                    continue;
                }
                if let Fetched::PlanCapped { code } = detail {
                    tracing::warn!(target: "cloud", event_type = %kind, code = %code,
                        "the store's plan refuses the detail of an event; it is recorded failed");
                    tx.execute(
                        "INSERT INTO cloud_inbox (store_id, event_id, event_type, received_at, outcome)
                         VALUES (?1, ?2, ?3, ?4, 'failed')",
                        params![link.store_id, id, kind, now],
                    )?;
                    store::raise_notice(
                        tx,
                        CloudNoticeKind::PlanCap,
                        "cloud_plan_cap",
                        "",
                        json!({ "code": code, "type": kind }),
                        now,
                    )?;
                    continue;
                }
                let refusal = match detail {
                    Fetched::Product(remote) => Refusal::product(remote),
                    Fetched::Order(remote) => Refusal::order(remote),
                    _ => Refusal::event(kind),
                };
                let applied = isolated(tx, kind, Some(refusal), now, || {
                    apply_one(
                        tx,
                        &link,
                        &settings,
                        &actor,
                        kind,
                        data,
                        detail,
                        id,
                        &mut report,
                        now,
                    )
                })?;
                let outcome = applied.unwrap_or("failed");
                tx.execute(
                    "INSERT OR IGNORE INTO cloud_inbox (store_id, event_id, event_type, received_at, outcome)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![link.store_id, id, kind, now, outcome],
                )?;
                report.applied += usize::from(outcome == "applied");
            }
            if let Some(next) = &next {
                store::set_remote_cursor(tx, EVENTS, next, now)?;
            }
            summary(tx, &actor, &report)?;
            Ok(report)
        })
        .map_err(db_stop)
}

/// An order DZBuild sent again with nothing newer changes nothing here: it is not counted, so it
/// writes no `cloud.pulled` row.
fn changed_mirror(outcome: UpsertOutcome) -> bool {
    match outcome {
        UpsertOutcome::New | UpsertOutcome::Updated | UpsertOutcome::Closed => true,
        UpsertOutcome::Unchanged => false,
    }
}

fn summary(tx: &Transaction<'_>, actor: &Actor, report: &PullReport) -> Result<(), AppError> {
    if report.products + report.stock_moves + report.orders == 0 {
        return Ok(());
    }
    audit::append(
        tx,
        &actor.event(
            "cloud.pulled",
            "cloud_link",
            None,
            json!({
                "products": report.products,
                "conflicts": report.conflicts,
                "stockMoves": report.stock_moves,
                "orders": report.orders
            }),
        ),
    )?;
    Ok(())
}

/// Applies one event. Every arm counts in `report` only after its last fallible write, so an
/// event undone by [`isolated`] leaves the report as it was.
#[allow(clippy::too_many_arguments)]
fn apply_one(
    tx: &Transaction<'_>,
    link: &CloudLink,
    settings: &CloudSettings,
    actor: &Actor,
    kind: &str,
    data: &str,
    detail: &Fetched,
    event_id: &str,
    report: &mut PullReport,
    now: i64,
) -> Result<&'static str, AppError> {
    match (kind, detail) {
        ("product.created" | "product.updated", Fetched::Product(remote)) => {
            match apply_remote_product(tx, link, remote, actor, settings.prices_pull, now)? {
                ProductOutcome::Updated { conflicts } => {
                    report.products += 1;
                    report.conflicts += conflicts;
                    Ok("applied")
                }
                _ => Ok("ignored"),
            }
        }
        ("product.deleted", _) => {
            let reference: Option<RefEvent> = serde_json::from_str(data).ok();
            let local = local_product(
                tx,
                link,
                reference.as_ref().and_then(|r| r.external_id.as_deref()),
                reference
                    .as_ref()
                    .and_then(|r| r.id.as_ref())
                    .map(|i| i.0.as_str()),
            )?;
            let Some(local) = local else {
                return Ok("ignored");
            };
            tx.execute(
                "INSERT INTO cloud_publish (product_id, publish, updated_at) VALUES (?1, 0, ?2)
                 ON CONFLICT (product_id) DO UPDATE SET publish = 0, updated_at = excluded.updated_at",
                params![local, now],
            )?;
            store::map_forget(tx, &link.store_id, MapEntity::Product, &local)?;
            let product = products::get(tx, &local)?;
            store::raise_notice(
                tx,
                CloudNoticeKind::Conflict,
                "online_product_deleted",
                &local,
                json!({ "product": super::payloads::name_for(&product, link.store_language) }),
                now,
            )?;
            Ok("applied")
        }
        ("inventory.level_changed", _) => {
            let Ok(event) = serde_json::from_str::<StockEvent>(data) else {
                return Ok("ignored");
            };
            if event.source == "pos" {
                return Ok("echo");
            }
            if !settings.stock_pull || (event.source == "order" && settings.orders_intake) {
                return Ok("ignored");
            }
            let remote_id = event.product_id.as_ref().map(|i| i.0.as_str());
            let local = match (&event.variant_external_id, &event.variant_key) {
                (None, None) => local_product(tx, link, event.external_id.as_deref(), remote_id)?,
                (child, key) => match child.as_deref().filter(|id| products::get(tx, id).is_ok()) {
                    Some(id) => Some(id.to_owned()),
                    None => match (remote_id, key) {
                        (Some(remote), Some(key)) => {
                            super::variants::map_local(tx, &link.store_id, remote, key)?
                        }
                        _ => None,
                    },
                },
            };
            let Some(local) = local else {
                return Ok("ignored");
            };
            if products::get(tx, &local)?.kind == ProductKind::VariantParent {
                tracing::debug!(target: "cloud", "a stock event of a variant model: its children hold the stock");
                return Ok("ignored");
            }
            let delta = event
                .delta
                .or_else(|| event.new?.checked_sub(event.old?))
                .unwrap_or(0);
            if delta == 0 {
                return Ok("ignored");
            }
            // an online order the POS does not take in: named in the stock history, never an
            // anonymous correction
            let order = event.source == "order";
            let note = order.then(|| {
                event
                    .order_number
                    .as_ref()
                    .map_or_else(|| "DZBuild".to_owned(), |n| format!("DZBuild #{}", n.0))
            });
            crate::stock::append(
                tx,
                actor,
                &crate::stock::NewMovement {
                    product_id: local.clone(),
                    location_id: crate::stock::default_location(tx)?.id,
                    qty_delta: Qty(delta.saturating_mul(1_000)),
                    reason: crate::stock::MovementReason::Adjustment,
                    reason_code: Some(crate::stock::AdjustReason::Correction),
                    ref_type: Some(if order { "dzbuild_order" } else { "dzbuild" }.into()),
                    ref_id: Some(event_id.to_owned()),
                    unit_cost: None,
                    note,
                },
                now,
            )?;
            super::stock::acknowledge(tx, &link.store_id, &local, delta, now)?;
            report.stock_moves += 1;
            Ok("applied")
        }
        (k, Fetched::Order(order)) if k.starts_with("order.") => {
            if !changed_mirror(orders::upsert_remote(tx, link, order, now)?) {
                return Ok("ignored");
            }
            report.orders += 1;
            Ok("applied")
        }
        ("customer.created" | "customer.updated", _) => {
            let Ok(event) = serde_json::from_str::<CustomerEvent>(data) else {
                return Ok("ignored");
            };
            if !settings.customers_flags {
                return Ok("ignored");
            }
            let Some(phone) =
                crate::party::clean_phone(event.phone.as_deref(), "phone").unwrap_or(None)
            else {
                return Ok("ignored");
            };
            let hash = flag_hash(&link.store_id, &phone);
            let risky = event.is_banned || event.fraud_score.is_some_and(|s| s >= FLAG_THRESHOLD);
            if risky {
                let score = event.fraud_score.map(|s| s.clamp(0, 100));
                tx.execute(
                    "INSERT INTO remote_customer_flags (store_id, phone_hash, remote_customer_id, is_banned, fraud_score, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT (store_id, phone_hash) DO UPDATE SET is_banned = excluded.is_banned,
                         fraud_score = excluded.fraud_score, updated_at = excluded.updated_at",
                    params![link.store_id, hash, event.id, event.is_banned, score, now],
                )?;
                report.flags += 1;
            } else {
                tx.execute(
                    "DELETE FROM remote_customer_flags WHERE store_id = ?1 AND phone_hash = ?2",
                    params![link.store_id, hash],
                )?;
            }
            Ok("applied")
        }
        ("store.plan_changed", Fetched::Profile(profile)) => {
            store::update_profile(
                tx,
                &link.id,
                &store::ProfileUpdate {
                    store_plan: profile.plan.clone(),
                    active_products_cap: profile
                        .plan_limits
                        .as_ref()
                        .and_then(|l| l.active_products),
                    ..store::ProfileUpdate::default()
                },
            )?;
            Ok("applied")
        }
        ("device.revoked", _) => {
            let reference: Option<RefEvent> = serde_json::from_str(data).ok();
            let ours = reference
                .and_then(|r| r.device_id)
                .is_none_or(|d| Some(d) == link.device_id);
            report.revoked |= ours;
            Ok(if ours { "applied" } else { "ignored" })
        }
        ("link.revoked" | "app.uninstalled", _) => {
            report.revoked = true;
            Ok("applied")
        }
        ("pos.sale.created" | "pos.sale.refunded", _) => Ok("echo"),
        _ => Ok("ignored"),
    }
}

fn local_product(
    conn: &Connection,
    link: &CloudLink,
    external_id: Option<&str>,
    remote_id: Option<&str>,
) -> Result<Option<String>, AppError> {
    if let Some(id) = external_id
        && products::get(conn, id).is_ok()
    {
        return Ok(Some(id.to_owned()));
    }
    match remote_id {
        Some(remote) => store::map_local(conn, &link.store_id, MapEntity::Product, remote),
        None => Ok(None),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductOutcome {
    Updated { conflicts: usize },
    Unchanged,
    Unmapped,
}

/// Applies the online-owned fields of a DZBuild product to its local twin (see the module doc).
/// Its price only with `prices_pull` (`cloud.prices.pull`): otherwise the owner is told
/// (`online_price_changed`) and the till keeps its price.
pub fn apply_remote_product(
    tx: &Transaction<'_>,
    link: &CloudLink,
    remote: &RemoteProduct,
    actor: &Actor,
    prices_pull: bool,
    now: i64,
) -> Result<ProductOutcome, AppError> {
    let Some(local_id) = local_product(tx, link, remote.external_id.as_deref(), Some(&remote.id))?
    else {
        return Ok(ProductOutcome::Unmapped);
    };
    let product = products::get(tx, &local_id)?;
    let family = remote.family();
    if family.is_some() && product.kind != ProductKind::VariantParent {
        tracing::info!(target: "cloud", kind = product.kind.as_str(),
            "a product with variants online is not a variant model here; the owner converts it");
        return Ok(ProductOutcome::Unchanged);
    }
    let children = match family {
        Some(family) => reconcile_children(
            tx,
            link,
            actor,
            &product,
            &remote.id,
            family,
            prices_pull,
            now,
        )?,
        None => 0,
    };
    let online = ProductSnapshot {
        name: remote.name.clone(),
        price: remote.pricing.price.0,
        status: remote.status.clone(),
    };
    let last: Option<ProductSnapshot> =
        store::map_get(tx, &link.store_id, MapEntity::Product, &local_id)?
            .and_then(|m| m.last_payload)
            .and_then(|p| serde_json::from_value(p).ok());
    let remember = |tx: &Transaction<'_>| -> Result<(), AppError> {
        let value = serde_json::to_value(&online)?;
        store::map_put(
            tx,
            &link.store_id,
            MapEntity::Product,
            &local_id,
            &remote.id,
            Some(&value),
            None,
            now,
        )
    };
    let Some(last) = last else {
        remember(tx)?;
        return Ok(if children > 0 {
            ProductOutcome::Updated { conflicts: 0 }
        } else {
            ProductOutcome::Unchanged
        });
    };
    let waiting: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM sync_outbox WHERE target = 'cloud' AND kind = 'product_upsert' AND entity_id = ?1
            AND status = 'pending' AND attempts = 0)",
        [&local_id],
        |r| r.get(0),
    )?;
    let ar = link.store_language == crate::lang::Lang::Ar;
    // the name as the push would send it now (store language, with the same fallback)
    let local_name = super::payloads::name_for(&product, link.store_language);
    let mut conflicts: Vec<&str> = Vec::new();
    let mut changed = children > 0;
    if online.name != last.name && !online.name.trim().is_empty() {
        if waiting && local_name != last.name {
            conflicts.push("name");
        }
        let mut input = ProductInput::from(&product);
        if ar {
            input.name_ar = online.name.clone();
        } else {
            input.name_fr = online.name.clone();
        }
        products::update(tx, actor, &local_id, input)?;
        changed = true;
    }
    if online.price != last.price {
        let company =
            crate::company::get(tx)?.ok_or_else(|| AppError::conflict("setup_required"))?;
        if !prices_pull {
            store::raise_notice(
                tx,
                CloudNoticeKind::Conflict,
                "online_price_changed",
                &local_id,
                json!({ "price": online.price, "product": local_name }),
                now,
            )?;
        } else if company.price_mode == crate::company::PriceMode::Ttc {
            let current = products::get(tx, &local_id)?.sell_price;
            if waiting && current != last.price {
                conflicts.push("price");
            }
            let level = crate::catalog::price_levels::default_level(tx)?;
            products::set_price(tx, actor, &local_id, &level.id, online.price)?;
            changed = true;
        } else {
            store::raise_notice(
                tx,
                CloudNoticeKind::Conflict,
                "online_price_ht_shop",
                &local_id,
                json!({ "price": online.price, "product": local_name }),
                now,
            )?;
        }
    }
    if online.status != last.status {
        match online.status.as_str() {
            "active" | "draft" => tx.execute(
                "INSERT INTO cloud_publish (product_id, publish, online_status, updated_at) VALUES (?1, 1, ?2, ?3)
                 ON CONFLICT (product_id) DO UPDATE SET publish = 1, online_status = excluded.online_status, updated_at = excluded.updated_at",
                params![local_id, online.status, now],
            )?,
            _ => tx.execute(
                "INSERT INTO cloud_publish (product_id, publish, updated_at) VALUES (?1, 0, ?2)
                 ON CONFLICT (product_id) DO UPDATE SET publish = 0, updated_at = excluded.updated_at",
                params![local_id, now],
            )?,
        };
        changed = true;
    }
    if !conflicts.is_empty() {
        let name = if online.name.is_empty() {
            local_name
        } else {
            online.name.clone()
        };
        store::raise_notice(
            tx,
            CloudNoticeKind::Conflict,
            "cloud_conflict_online_wins",
            &local_id,
            json!({ "fields": conflicts, "product": name }),
            now,
        )?;
    }
    remember(tx)?;
    Ok(if changed {
        ProductOutcome::Updated {
            conflicts: conflicts.len(),
        }
    } else {
        ProductOutcome::Unchanged
    })
}

/// Saves a dated walk's newest `updated_at` as the mark, never behind the mark already saved.
fn advance_mark(tx: &Transaction<'_>, name: &str, mark: &str, now: i64) -> Result<(), AppError> {
    let saved = store::remote_cursor(tx, name)?;
    let next =
        newest(saved, parse_time(mark).unwrap_or(i64::MIN)).unwrap_or_else(|| mark.to_owned());
    store::set_remote_cursor(tx, name, &next, now)
}

fn newest(current: Option<String>, candidate: i64) -> Option<String> {
    let current_ms = current.as_deref().and_then(parse_time).unwrap_or(i64::MIN);
    Some(if candidate > current_ms {
        iso_time(candidate)
    } else {
        current.unwrap_or_else(|| iso_time(candidate))
    })
}

/// `GET /v1/products?updated_since=` (all pages) while there is no events feed. The mark moves
/// once the last page is in, never page by page: DZBuild does not promise an order, and a walk
/// that breaks after a page of the newest items would otherwise skip the older ones on the
/// pages after it. A broken walk starts again from the old mark (items are applied by id).
pub fn pull_products(ctx: &PullContext<'_>) -> Result<PullReport, CallError> {
    let since = ctx
        .db
        .read(|c| store::remote_cursor(c, PRODUCTS))
        .map_err(db_stop)?
        .unwrap_or_else(|| EPOCH.to_owned());
    // `updated_since` is inclusive: an item at the mark was sent (and told, if refused) before
    let since_ms = parse_time(&since).unwrap_or(i64::MIN);
    let mut report = PullReport::default();
    let mut cursor: Option<String> = None;
    // the newest `updated_at` seen by this walk, saved with its last page
    let mut walked: Option<String> = None;
    loop {
        let mut call = Call::get(Endpoint::Products, "/products")
            .query("updated_since", since.clone())
            .query("limit", "200");
        if let Some(c) = &cursor {
            call = call.query("cursor", c.clone());
        }
        let page: Page<RemoteProduct> = ctx.remote.data(call)?;
        let last = !page.has_more || page.next_cursor.is_none();
        let (link, actor, now, items, seen, prices) = (
            ctx.link.clone(),
            ctx.actor.clone(),
            ctx.now,
            page.items.clone(),
            walked.clone(),
            ctx.settings.prices_pull,
        );
        let (step, seen) = ctx
            .db
            .write(move |tx| {
                let tx: &Transaction<'_> = tx;
                let mut step = PullReport::default();
                let mut mark = seen;
                for remote in &items {
                    let fresh = remote.updated_at.is_none_or(|at| at.0 > since_ms);
                    let refusal = fresh.then(|| Refusal::product(remote));
                    let outcome = isolated(tx, "product", refusal, now, || {
                        apply_remote_product(tx, &link, remote, &actor, prices, now)
                    })?;
                    if let Some(ProductOutcome::Updated { conflicts }) = outcome {
                        step.products += 1;
                        step.conflicts += conflicts;
                    }
                    if let Some(at) = remote.updated_at {
                        mark = newest(mark, at.0);
                    }
                }
                if last && let Some(mark) = &mark {
                    advance_mark(tx, PRODUCTS, mark, now)?;
                }
                summary(tx, &actor, &step)?;
                Ok((step, mark))
            })
            .map_err(db_stop)?;
        walked = seen;
        report.products += step.products;
        report.conflicts += step.conflicts;
        if !page.has_more || page.next_cursor.is_none() {
            return Ok(report);
        }
        cursor = page.next_cursor;
    }
}

/// `GET /v1/orders?updated_since=` (all pages) while there is no events feed.
pub fn pull_orders(ctx: &PullContext<'_>) -> Result<PullReport, CallError> {
    if !ctx.settings.orders_intake {
        return Ok(PullReport::default());
    }
    let since = ctx
        .db
        .read(|c| store::remote_cursor(c, ORDERS))
        .map_err(db_stop)?
        .unwrap_or_else(|| EPOCH.to_owned());
    // `updated_since` is inclusive: an order at the mark was sent (and told, if refused) before
    let since_ms = parse_time(&since).unwrap_or(i64::MIN);
    let mut report = PullReport::default();
    let mut cursor: Option<String> = None;
    // the newest `updated_at` seen by this walk, saved with its last page (as `pull_products`)
    let mut walked: Option<String> = None;
    loop {
        let mut call = Call::get(Endpoint::Orders, "/orders")
            .query("updated_since", since.clone())
            .query("limit", "200");
        if let Some(c) = &cursor {
            call = call.query("cursor", c.clone());
        }
        let page: Page<RemoteOrder> = ctx.remote.data(call)?;
        let last = !page.has_more || page.next_cursor.is_none();
        let (link, actor, now, items, seen) = (
            ctx.link.clone(),
            ctx.actor.clone(),
            ctx.now,
            page.items.clone(),
            walked.clone(),
        );
        let (applied, seen) = ctx
            .db
            .write(move |tx| {
                let tx: &Transaction<'_> = tx;
                let mut mark = seen;
                let mut applied = 0;
                for order in &items {
                    // the newest order comes back on every poll (`updated_since` is inclusive)
                    let refusal = (order.updated_at.0 > since_ms).then(|| Refusal::order(order));
                    if isolated(tx, "order", refusal, now, || {
                        orders::upsert_remote(tx, &link, order, now)
                    })?
                    .is_some_and(changed_mirror)
                    {
                        applied += 1;
                    }
                    // seen, even when refused: an edit online brings it back
                    mark = newest(mark, order.updated_at.0);
                }
                if last && let Some(mark) = &mark {
                    advance_mark(tx, ORDERS, mark, now)?;
                }
                let step = PullReport {
                    orders: applied,
                    ..PullReport::default()
                };
                summary(tx, &actor, &step)?;
                Ok((applied, mark))
            })
            .map_err(db_stop)?;
        walked = seen;
        report.orders += applied;
        if !page.has_more || page.next_cursor.is_none() {
            return Ok(report);
        }
        cursor = page.next_cursor;
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub imported: usize,
    pub matched: usize,
    /// Products with variants the till cannot hold: no readable family, more than three axes,
    /// none listed, a family refused here, or a simple product here.
    pub skipped_variants: usize,
}

/// The owner's "Importer les produits de ma boutique en ligne" after linking a store that already
/// sells online: every online product becomes (or is matched by SKU or barcode to) a local one,
/// published, its online stock as opening stock and as the confirmed baseline.
pub fn import_online(ctx: &PullContext<'_>) -> Result<ImportReport, CallError> {
    let mut report = ImportReport::default();
    let mut cursor: Option<String> = None;
    loop {
        let mut call = Call::get(Endpoint::Products, "/products")
            .query("updated_since", EPOCH)
            .query("limit", "200");
        if let Some(c) = &cursor {
            call = call.query("cursor", c.clone());
        }
        let page: Page<RemoteProduct> = ctx.remote.data(call)?;
        let (link, actor, now, items) = (
            ctx.link.clone(),
            ctx.actor.clone(),
            ctx.now,
            page.items.clone(),
        );
        let step = ctx
            .db
            .write(move |tx| import_page(tx, &link, &actor, &items, now))
            .map_err(db_stop)?;
        report.imported += step.imported;
        report.matched += step.matched;
        report.skipped_variants += step.skipped_variants;
        if !page.has_more || page.next_cursor.is_none() {
            return Ok(report);
        }
        cursor = page.next_cursor;
    }
}

fn import_page(
    tx: &Transaction<'_>,
    link: &CloudLink,
    actor: &Actor,
    items: &[RemoteProduct],
    now: i64,
) -> Result<ImportReport, AppError> {
    let mut report = ImportReport::default();
    let company = crate::company::get(tx)?.ok_or_else(|| AppError::conflict("setup_required"))?;
    let rate = crate::catalog::tax_rates::by_code(tx, "TVA19")?
        .ok_or_else(|| AppError::not_found("tax_rate"))?;
    for remote in items {
        if let Some(family) = remote.family() {
            let outcome = isolated(tx, "product", Some(Refusal::product(remote)), now, || {
                import_family(
                    tx,
                    link,
                    actor,
                    company.price_mode,
                    &rate,
                    remote,
                    family,
                    now,
                )
            })?;
            match outcome {
                Some(FamilyImport::Imported) => report.imported += 1,
                Some(FamilyImport::Matched) => report.matched += 1,
                Some(FamilyImport::Known) => {}
                Some(FamilyImport::Skipped) | None => report.skipped_variants += 1,
            }
            continue;
        }
        if remote.has_variants {
            report.skipped_variants += 1;
            continue;
        }
        if local_product(tx, link, remote.external_id.as_deref(), Some(&remote.id))?.is_some() {
            continue;
        }
        let by_sku: Option<String> = match remote.sku() {
            Some(sku) => tx
                .query_row(
                    "SELECT id FROM products WHERE reference = ?1 AND archived_at IS NULL",
                    [sku],
                    |r| r.get(0),
                )
                .optional()?,
            None => None,
        };
        let by_code: Option<String> = match (by_sku, remote.barcode()) {
            (Some(id), _) => Some(id),
            (None, Some(code)) => tx
                .query_row(
                    "SELECT product_id FROM product_barcodes WHERE barcode = ?1",
                    [code],
                    |r| r.get(0),
                )
                .optional()?,
            (None, None) => None,
        };
        let local_id = match by_code {
            Some(id) => {
                report.matched += 1;
                id
            }
            None => {
                let mut input = online_input(link, &rate, company.price_mode, remote);
                input.barcodes = remote
                    .barcode()
                    .map(|b| {
                        vec![Barcode {
                            barcode: b.to_owned(),
                            pack_qty: Qty::ONE,
                            label: None,
                        }]
                    })
                    .unwrap_or_default();
                let created = products::create(tx, actor, input.clone()).or_else(|e| {
                    // a barcode another product holds, or an odd one: import without it
                    if matches!(
                        e.code().as_str(),
                        "barcode_taken" | "barcode_invalid" | "reference_taken"
                    ) {
                        products::create(
                            tx,
                            actor,
                            ProductInput {
                                barcodes: Vec::new(),
                                reference: None,
                                ..input
                            },
                        )
                    } else {
                        Err(e)
                    }
                })?;
                let stock = remote
                    .inventory
                    .as_ref()
                    .and_then(|i| i.stock_quantity)
                    .filter(|q| *q > 0);
                if let Some(pieces) = stock {
                    crate::stock::adjust(
                        tx,
                        actor,
                        crate::stock::AdjustInput {
                            product_id: created.id.clone(),
                            location_id: None,
                            qty_delta: Qty(pieces.saturating_mul(1_000)),
                            reason: crate::stock::AdjustReason::Opening,
                            unit_cost: None,
                            note: None,
                        },
                        now,
                    )?;
                }
                tx.execute(
                    "INSERT INTO cloud_stock (store_id, product_id, confirmed, updated_at) VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT (store_id, product_id) DO UPDATE SET confirmed = excluded.confirmed",
                    params![link.store_id, created.id, stock.unwrap_or(0), now],
                )?;
                report.imported += 1;
                created.id
            }
        };
        adopt(tx, link, remote, &local_id, now)?;
    }
    let step = PullReport {
        products: report.imported + report.matched,
        ..PullReport::default()
    };
    summary(tx, actor, &step)?;
    Ok(report)
}

/// Publishes an imported product as it is online and maps it, with the online fields as the
/// last snapshot.
fn adopt(
    tx: &Transaction<'_>,
    link: &CloudLink,
    remote: &RemoteProduct,
    local_id: &str,
    now: i64,
) -> Result<(), AppError> {
    let snapshot = serde_json::to_value(ProductSnapshot {
        name: remote.name.clone(),
        price: remote.pricing.price.0,
        status: remote.status.clone(),
    })?;
    if remote.status == "archived" {
        // off the storefront online: not published here either, and the mapping below makes
        // `publish_decision` keep it archived instead of listing it again
        tx.execute(
            "INSERT INTO cloud_publish (product_id, publish, updated_at) VALUES (?1, 0, ?2)
             ON CONFLICT (product_id) DO UPDATE SET publish = 0, updated_at = excluded.updated_at",
            params![local_id, now],
        )?;
    } else {
        tx.execute(
            "INSERT INTO cloud_publish (product_id, publish, online_status, updated_at) VALUES (?1, 1, ?2, ?3)
             ON CONFLICT (product_id) DO UPDATE SET publish = 1, online_status = excluded.online_status",
            params![local_id, if remote.status == "draft" { "draft" } else { "active" }, now],
        )?;
    }
    store::map_put(
        tx,
        &link.store_id,
        MapEntity::Product,
        local_id,
        &remote.id,
        Some(&snapshot),
        None,
        now,
    )?;
    Ok(())
}

/// A local product for an online one: its name in the store's language, its SKU, its price in
/// the shop's price mode.
fn online_input(
    link: &CloudLink,
    rate: &crate::catalog::tax_rates::TaxRate,
    mode: PriceMode,
    remote: &RemoteProduct,
) -> ProductInput {
    let mut input = ProductInput::simple(
        &remote.name,
        crate::catalog::UNIT_PIECE_ID,
        &rate.id,
        shop_price(mode, rate.rate_bp, remote.pricing.price.0),
    );
    if link.store_language == crate::lang::Lang::Ar {
        input.name_ar = remote.name.clone();
        input.name_fr = String::new();
    }
    input.reference = remote.sku().map(str::to_owned);
    input
}

/// An online price (TTC) in the shop's price mode.
fn shop_price(mode: PriceMode, rate: Rate, online: Money) -> Money {
    match mode {
        PriceMode::Ttc => online,
        PriceMode::Ht => online
            .mul_div(10_000, 10_000 + i64::from(rate.0))
            .unwrap_or(online),
    }
}

enum FamilyImport {
    Imported,
    Matched,
    /// Mapped already: its children were brought in line.
    Known,
    Skipped,
}

/// One online family: a new model and its children, or the shop's model with the same reference
/// (its children matched by their values), or the mapped model brought in line.
#[allow(clippy::too_many_arguments)]
fn import_family(
    tx: &Transaction<'_>,
    link: &CloudLink,
    actor: &Actor,
    mode: PriceMode,
    rate: &crate::catalog::tax_rates::TaxRate,
    remote: &RemoteProduct,
    family: &RemoteVariants,
    now: i64,
) -> Result<FamilyImport, AppError> {
    let local = match local_product(tx, link, remote.external_id.as_deref(), Some(&remote.id))? {
        Some(id) => Some((products::get(tx, &id)?, FamilyImport::Known)),
        None => match remote.sku() {
            Some(sku) => tx
                .query_row(
                    "SELECT id FROM products WHERE reference = ?1 AND archived_at IS NULL",
                    [sku],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
                .map(|id| products::get(tx, &id).map(|p| (p, FamilyImport::Matched)))
                .transpose()?,
            None => None,
        },
    };
    let (parent, outcome) = match local {
        Some((parent, _)) if parent.kind != ProductKind::VariantParent => {
            tracing::info!(target: "cloud", kind = parent.kind.as_str(),
                "an online family whose product here is not a variant model; left out");
            return Ok(FamilyImport::Skipped);
        }
        Some(found) => found,
        None => {
            let mut input = online_input(link, rate, mode, remote);
            input.kind = ProductKind::VariantParent;
            input.track_stock = false;
            let parent = products::create(tx, actor, input.clone()).or_else(|e| {
                if e.code() == "reference_taken" {
                    products::create(
                        tx,
                        actor,
                        ProductInput {
                            reference: None,
                            ..input
                        },
                    )
                } else {
                    Err(e)
                }
            })?;
            (parent, FamilyImport::Imported)
        }
    };
    reconcile_children(tx, link, actor, &parent, &remote.id, family, false, now)?;
    if !matches!(outcome, FamilyImport::Known) {
        adopt(tx, link, remote, &parent.id, now)?;
    }
    Ok(outcome)
}

/// Brings the children of a mapped model in line with its online items: a key not mapped yet
/// adopts the shop's child with the same values or becomes a new child, a mapped key takes a
/// changed online price only with `prices_pull` in a TTC shop, and a key gone online keeps its
/// child and loses its map row. Gives the number of children created or repriced.
#[allow(clippy::too_many_arguments)]
fn reconcile_children(
    tx: &Transaction<'_>,
    link: &CloudLink,
    actor: &Actor,
    parent: &Product,
    remote_id: &str,
    family: &RemoteVariants,
    prices_pull: bool,
    now: i64,
) -> Result<usize, AppError> {
    let mode = crate::company::get(tx)?
        .ok_or_else(|| AppError::conflict("setup_required"))?
        .price_mode;
    let mapped = super::variants::map_children(tx, &link.store_id, &parent.id)?;
    let own: Vec<(String, String)> = {
        let mut stmt = tx.prepare(
            "SELECT id FROM products WHERE parent_id = ?1 AND archived_at IS NULL ORDER BY rowid",
        )?;
        let ids = stmt
            .query_map([&parent.id], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        ids.into_iter()
            .map(|id| {
                let key = super::variants::variant_key(&super::variants::attributes_of(tx, &id)?);
                Ok((key, id))
            })
            .collect::<Result<_, AppError>>()?
    };
    let mut changed = 0;
    for item in &family.items {
        let known = mapped.iter().find(|r| r.variant_key == item.key);
        let local_id = match (known, own.iter().find(|(key, _)| *key == item.key)) {
            (Some(row), _) => {
                if prices_pull && mode == PriceMode::Ttc && row.price_online != Some(item.price.0) {
                    let level = crate::catalog::price_levels::default_level(tx)?;
                    products::set_price(tx, actor, &row.local_id, &level.id, item.price.0)?;
                    changed += 1;
                }
                row.local_id.clone()
            }
            (None, Some((_, id))) => id.clone(),
            (None, None) => {
                changed += 1;
                create_child(tx, link, actor, parent, family, item, mode, now)?
            }
        };
        super::variants::map_put(
            tx,
            &link.store_id,
            &super::variants::VariantMapRow {
                local_id,
                parent_id: parent.id.clone(),
                remote_product_id: remote_id.to_owned(),
                variant_key: item.key.clone(),
                price_online: Some(item.price.0),
                updated_at: now,
            },
        )?;
    }
    for gone in mapped
        .iter()
        .filter(|r| !family.items.iter().any(|i| i.key == r.variant_key))
    {
        tracing::info!(target: "cloud", key = %gone.variant_key,
            "a variant gone online keeps its product here, unmapped");
        super::variants::map_forget(tx, &link.store_id, &gone.local_id)?;
    }
    Ok(changed)
}

/// A child for an online item, made as `variants::generate` makes one: the model's name and the
/// item's values in axis order, the online SKU (else a generated reference) and barcode (a taken
/// one is left out), the online price, the axis values as attributes, the online stock as opening
/// stock and as the confirmed baseline when the item has one.
#[allow(clippy::too_many_arguments)]
fn create_child(
    tx: &Transaction<'_>,
    link: &CloudLink,
    actor: &Actor,
    parent: &Product,
    family: &RemoteVariants,
    item: &RemoteVariantItem,
    mode: PriceMode,
    now: i64,
) -> Result<String, AppError> {
    let values: Vec<String> = family
        .axes
        .iter()
        .filter_map(|a| item.attributes.get(&a.name).cloned())
        .collect();
    let generated = || -> Result<Option<String>, AppError> {
        match &parent.reference {
            Some(r) => Ok(Some(crate::catalog::variants::reference(tx, r, &values)?)),
            None => Ok(None),
        }
    };
    let mut input = ProductInput::from(parent);
    input.kind = ProductKind::Variant;
    input.parent_id = Some(parent.id.clone());
    input.track_stock = true;
    input.name_ar = crate::catalog::variants::named(&parent.name_ar, &values)?;
    input.name_fr = crate::catalog::variants::named(&parent.name_fr, &values)?;
    input.name_en = crate::catalog::variants::named(&parent.name_en, &values)?;
    input.sell_price = shop_price(mode, parent.tax_rate_bp, item.price.0);
    input.plu = None;
    input.image = None;
    input.components = Vec::new();
    input.reference = match item.sku.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(sku) => Some(sku.to_owned()),
        None => generated()?,
    };
    input.barcodes = item
        .barcode
        .as_deref()
        .map(str::trim)
        .filter(|b| !b.is_empty())
        .map(|b| {
            vec![Barcode {
                barcode: b.to_owned(),
                pack_qty: Qty::ONE,
                label: None,
            }]
        })
        .unwrap_or_default();
    let child = match products::create(tx, actor, input.clone()) {
        Err(e)
            if matches!(
                e.code().as_str(),
                "barcode_taken" | "barcode_invalid" | "reference_taken"
            ) =>
        {
            products::create(
                tx,
                actor,
                ProductInput {
                    barcodes: Vec::new(),
                    reference: generated()?,
                    ..input
                },
            )?
        }
        other => other?,
    };
    crate::catalog::extras::set_quiet(
        tx,
        &child.id,
        crate::catalog::extras::ProductExtrasInput {
            attributes: item.attributes.clone(),
            ..crate::catalog::extras::ProductExtrasInput::default()
        },
    )?;
    if let Some(pieces) = item.stock.filter(|q| *q > 0) {
        crate::stock::adjust(
            tx,
            actor,
            crate::stock::AdjustInput {
                product_id: child.id.clone(),
                location_id: None,
                qty_delta: Qty(pieces.saturating_mul(1_000)),
                reason: crate::stock::AdjustReason::Opening,
                unit_cost: None,
                note: None,
            },
            now,
        )?;
    }
    if let Some(stock) = item.stock {
        tx.execute(
            "INSERT INTO cloud_stock (store_id, product_id, confirmed, updated_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (store_id, product_id) DO UPDATE SET confirmed = excluded.confirmed",
            params![link.store_id, child.id, stock.max(0), now],
        )?;
    }
    Ok(child.id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CustomerFlag {
    pub is_banned: bool,
    pub fraud_score: Option<i64>,
}

/// SHA-256 hex of `store id ␟ normalised phone`: a flag is found by the phone typed at the till,
/// yet no phone is stored.
pub fn flag_hash(store_id: &str, phone: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(store_id.as_bytes());
    hasher.update([0x1f_u8]);
    hasher.update(phone.as_bytes());
    hex::encode(hasher.finalize().as_slice())
}

pub fn lookup_flag(
    conn: &Connection,
    store_id: &str,
    raw_phone: &str,
) -> Result<Option<CustomerFlag>, AppError> {
    let Some(phone) = crate::party::clean_phone(Some(raw_phone), "phone").unwrap_or(None) else {
        return Ok(None);
    };
    Ok(conn
        .query_row(
            "SELECT is_banned, fraud_score FROM remote_customer_flags WHERE store_id = ?1 AND phone_hash = ?2",
            params![store_id, flag_hash(store_id, &phone)],
            |r| Ok(CustomerFlag { is_banned: r.get(0)?, fraud_score: r.get(1)? }),
        )
        .optional()?)
}

#[derive(Debug, Deserialize)]
struct RemoteCustomer {
    #[serde(deserialize_with = "de_id")]
    id: String,
    #[serde(default)]
    is_banned: bool,
    #[serde(default)]
    fraud_score: Option<i64>,
}

/// Asks DZBuild about a phone typed at the till (`GET /v1/customers?phone=`): sends the phone,
/// so only when the owner turned `cloud.customers.sync` on. Updates the flag table.
pub fn refresh_flag(
    remote: &Remote,
    db: &Db,
    link: &CloudLink,
    raw_phone: &str,
    now: i64,
) -> Result<Option<CustomerFlag>, CallError> {
    let Some(phone) = crate::party::clean_phone(Some(raw_phone), "phone").unwrap_or(None) else {
        return Ok(None);
    };
    let page: Page<RemoteCustomer> = remote.data(
        Call::get(Endpoint::Customers, "/customers")
            .query("phone", phone.clone())
            .lane(Lane::Interactive)
            .timeout(std::time::Duration::from_secs(3)),
    )?;
    let found = page.items.into_iter().next();
    let (store_id, hash) = (link.store_id.clone(), flag_hash(&link.store_id, &phone));
    let flag = found.as_ref().and_then(|c| {
        (c.is_banned || c.fraud_score.is_some_and(|s| s >= FLAG_THRESHOLD)).then_some(
            CustomerFlag {
                is_banned: c.is_banned,
                fraud_score: c.fraud_score.map(|s| s.clamp(0, 100)),
            },
        )
    });
    let (stored, id) = (flag, found.map(|c| c.id));
    db.write(move |tx| {
        match (stored, id) {
            (Some(f), Some(id)) => tx.execute(
                "INSERT INTO remote_customer_flags (store_id, phone_hash, remote_customer_id, is_banned, fraud_score, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT (store_id, phone_hash) DO UPDATE SET
                     is_banned = excluded.is_banned, fraud_score = excluded.fraud_score, updated_at = excluded.updated_at",
                params![store_id, hash, id, f.is_banned, f.fraud_score, now],
            )?,
            _ => tx.execute("DELETE FROM remote_customer_flags WHERE store_id = ?1 AND phone_hash = ?2", params![store_id, hash])?,
        };
        Ok(())
    })
    .map_err(db_stop)?;
    Ok(flag)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::derive::{OnlineStatus, derive, publish_decision};
    use crate::cloud::http::ManualClock;
    use crate::cloud::payloads::ProductSnapshot;
    use crate::cloud::store::cloud_settings;
    use crate::cloud::testkit::{
        MockDz, linked_ht_shop, linked_shop, ok, publish, remote_for_test,
    };
    use crate::money::Money;
    use crate::testkit::{NOON, Shop};
    use serde_json::json;
    use wiremock::matchers::{path, query_param};
    use wiremock::{Mock, ResponseTemplate};

    fn ctx<'a>(
        shop: &'a Shop,
        link: &'a CloudLink,
        remote: &'a Remote,
        settings: &'a CloudSettings,
    ) -> PullContext<'a> {
        PullContext {
            remote,
            db: &shop.db,
            link,
            settings,
            actor: shop.owner.actor(),
            now: NOON,
        }
    }

    fn page(events: serde_json::Value, next: &str) -> Page<EventItem> {
        // `data` is kept as raw JSON text: parse text, as the transport does
        serde_json::from_str(
            &json!({ "items": events, "next_cursor": next, "has_more": false }).to_string(),
        )
        .unwrap()
    }

    fn run_derive(shop: &Shop, link: &CloudLink) -> crate::cloud::derive::DeriveReport {
        let link = link.clone();
        shop.db
            .write(move |tx| {
                let settings = cloud_settings(tx)?;
                derive(tx, &link, &settings, NOON)
            })
            .unwrap()
    }

    /// The product as the POS last pushed it.
    fn pushed(shop: &Shop, product_id: &str, name: &str, price: i64) {
        let snapshot = serde_json::to_value(ProductSnapshot {
            name: name.into(),
            price: Money(price),
            status: "active".into(),
        })
        .unwrap();
        let id = product_id.to_owned();
        shop.db
            .write(move |tx| {
                store::map_put(
                    tx,
                    "141",
                    MapEntity::Product,
                    &id,
                    "88121",
                    Some(&snapshot),
                    None,
                    NOON,
                )
            })
            .unwrap();
    }

    fn remote_product(
        external_id: &str,
        name: &str,
        price: i64,
        status: &str,
    ) -> serde_json::Value {
        json!({ "data": { "id": 88121, "external_id": external_id, "name": name, "pricing": { "price": price },
                           "inventory": { "track_stock": true, "stock_quantity": 10 }, "status": status, "has_variants": false } })
    }

    #[test]
    fn a_dashboard_edit_is_applied_once_and_never_echoed() {
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        // the link's first derive (its catalogue walk, which starts the stock baseline again)
        // runs right after linking, long before DZBuild sends an event
        run_derive(&shop, &link);
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        publish(&shop, &milk.id);
        shop.stock_in(&milk.id, 10_000);
        shop.db
            .write(|tx| {
                tx.execute("INSERT INTO cloud_stock (store_id, product_id, confirmed, updated_at) SELECT '141', id, 10, 0 FROM products", [])?;
                Ok(())
            })
            .unwrap();
        pushed(&shop, &milk.id, "Lait", 10_000);
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        let settings = shop.db.read(cloud_settings).unwrap();
        let events = page(
            json!([
                { "id": "evt_1", "type": "inventory.level_changed",
                  "data": { "external_id": milk.id, "target": "product", "old": 10, "new": 8, "source": "dashboard" } },
                { "id": "evt_2", "type": "inventory.level_changed",
                  "data": { "external_id": milk.id, "target": "product", "old": 8, "new": 7, "source": "pos" } }
            ]),
            "c1",
        );
        let report = apply_events(&ctx(&shop, &link, &remote, &settings), &events).unwrap();
        assert_eq!(
            report.stock_moves, 1,
            "the dashboard edit; our own echo is ignored"
        );
        apply_events(&ctx(&shop, &link, &remote, &settings), &events).unwrap(); // delivered twice
        let id = milk.id.clone();
        assert_eq!(
            shop.db
                .read(move |c| crate::stock::on_hand(c, &id))
                .unwrap(),
            crate::money::Qty(8_000)
        );
        let moves: i64 =
            shop.count("SELECT COUNT(*) FROM stock_movements WHERE ref_type = 'dzbuild'");
        assert_eq!(moves, 1, "applied once");
        assert_eq!(run_derive(&shop, &link).stock_items, 0, "never pushed back");
        assert_eq!(
            shop.db
                .read(|c| store::remote_cursor(c, "cloud:events"))
                .unwrap()
                .as_deref(),
            Some("c1")
        );
    }

    /// A product deleted online is unpublished here, and the owner reads which one.
    #[test]
    fn a_product_deleted_online_is_named_in_its_notice() {
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        let bag = shop.product("Sac", Money(450_000), &shop.tva19);
        pushed(&shop, &bag.id, "Sac", 450_000);
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        let settings = shop.db.read(cloud_settings).unwrap();
        let events = page(
            json!([{ "id": "evt_d", "type": "product.deleted", "data": { "id": 88121, "external_id": bag.id } }]),
            "c2",
        );
        apply_events(&ctx(&shop, &link, &remote, &settings), &events).unwrap();
        let notices = shop.db.read(|c| store::notices(c, false)).unwrap();
        let told = notices
            .iter()
            .find(|n| n.code == "online_product_deleted")
            .unwrap();
        assert_eq!(told.details["product"], json!("Sac"), "{told:?}");
    }

    /// A shop showing its prices HT never takes an online (TTC) price: the owner is told which
    /// product's price changed.
    #[test]
    fn a_price_changed_online_in_an_ht_shop_names_the_product() {
        let dz = MockDz::start();
        let (shop, link) = linked_ht_shop();
        shop.set("cloud.prices.pull", json!(true));
        let bag = shop.product("Sac", Money(450_000), &shop.tva19);
        pushed(&shop, &bag.id, "Sac", 450_000);
        dz.mount(Mock::given(path("/v1/products/88121")).respond_with(
            ResponseTemplate::new(200).set_body_json(remote_product(
                &bag.id,
                "Sac",
                470_000 / 100,
                "active",
            )),
        ));
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        let settings = shop.db.read(cloud_settings).unwrap();
        let events = page(
            json!([{ "id": "evt_h", "type": "product.updated", "data": { "id": 88121, "external_id": bag.id } }]),
            "c2",
        );
        apply_events(&ctx(&shop, &link, &remote, &settings), &events).unwrap();
        let notices = shop.db.read(|c| store::notices(c, false)).unwrap();
        let told = notices
            .iter()
            .find(|n| n.code == "online_price_ht_shop")
            .unwrap();
        assert_eq!(told.details["product"], json!("Sac"), "{told:?}");
    }

    /// `cloud.prices.pull` off (the default): a price changed online is told, the till keeps
    /// its own; the name still follows.
    #[test]
    fn a_price_changed_online_is_told_not_applied_unless_the_owner_says_so() {
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        let bag = shop.product("Sac", Money(450_000), &shop.tva19);
        pushed(&shop, &bag.id, "Sac", 450_000);
        dz.mount(Mock::given(path("/v1/products/88121")).respond_with(
            ResponseTemplate::new(200).set_body_json(remote_product(
                &bag.id,
                "Sac cuir",
                470_000 / 100,
                "active",
            )),
        ));
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        let settings = shop.db.read(cloud_settings).unwrap();
        assert!(!settings.prices_pull);
        let events = page(
            json!([{ "id": "evt_9", "type": "product.updated", "data": { "id": 88121, "external_id": bag.id } }]),
            "c2",
        );
        apply_events(&ctx(&shop, &link, &remote, &settings), &events).unwrap();
        let id = bag.id.clone();
        let now = shop
            .db
            .read(move |c| crate::catalog::products::get(c, &id))
            .unwrap();
        assert_eq!(now.sell_price, Money(450_000), "the shop's price stays");
        let notices = shop.db.read(|c| store::notices(c, false)).unwrap();
        let told = notices
            .iter()
            .find(|n| n.code == "online_price_changed")
            .unwrap();
        assert_eq!(told.details["price"], json!(470_000));
        assert!(told.details["product"].as_str().is_some(), "{told:?}");
    }

    #[test]
    fn an_online_edit_wins_and_a_simultaneous_local_edit_is_reported() {
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        shop.set("cloud.prices.pull", json!(true));
        let bag = shop.product("Sac", Money(450_000), &shop.tva19);
        pushed(&shop, &bag.id, "Sac", 450_000);
        // the owner changed the price at the shop (not pushed yet) and online at the same time
        let owner = shop.owner.actor();
        let (id, level) = (bag.id.clone(), crate::catalog::RETAIL_LEVEL_ID);
        shop.db
            .write(move |tx| {
                crate::catalog::products::set_price(tx, &owner, &id, level, Money(460_000))
                    .map(drop)
            })
            .unwrap();
        shop.db
            .write(|tx| {
                tx.execute(
                    "INSERT INTO sync_outbox (id, target, store_id, kind, entity_id, idempotency_key, created_at)
                     SELECT '00000000-0000-7000-8000-00000000a0a0', 'cloud', '141', 'product_upsert', id, 'product:x', 0 FROM products",
                    [],
                )?;
                Ok(())
            })
            .unwrap();
        dz.mount(Mock::given(path("/v1/products/88121")).respond_with(
            ResponseTemplate::new(200).set_body_json(remote_product(
                &bag.id,
                "Sac cuir",
                470_000 / 100,
                "active",
            )),
        ));
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        let settings = shop.db.read(cloud_settings).unwrap();
        let events = page(
            json!([{ "id": "evt_9", "type": "product.updated", "data": { "id": 88121, "external_id": bag.id } }]),
            "c2",
        );
        let report = apply_events(&ctx(&shop, &link, &remote, &settings), &events).unwrap();
        assert_eq!((report.products, report.conflicts), (1, 1));
        let id = bag.id.clone();
        let now = shop
            .db
            .read(move |c| crate::catalog::products::get(c, &id))
            .unwrap();
        assert_eq!(now.sell_price, Money(470_000), "DZBuild wins on the price");
        assert_eq!(
            now.name_ar, "Sac cuir",
            "the name in the store's language (ar)"
        );
        let notices = shop.db.read(|c| store::notices(c, false)).unwrap();
        assert!(
            notices
                .iter()
                .any(|n| n.code == "cloud_conflict_online_wins"
                    && n.details["fields"] == json!(["price"]))
        );
    }

    #[test]
    fn order_events_feed_the_mirror_and_a_revocation_is_reported() {
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        dz.mount(Mock::given(path("/v1/orders/9001")).respond_with(ok(json!({
            "id": 9001, "order_number": 1042, "status": "confirmed", "payment_method": "cod", "delivery_type": "pickup",
            "subtotal": 200, "total": 200, "created_at": "2026-09-28 10:00:00", "updated_at": "2026-09-28 10:00:00",
            "items": [{ "product_name": "Lait", "price": 100, "quantity": 2, "total": 200 }]
        }))));
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        let settings = shop.db.read(cloud_settings).unwrap();
        let events = page(
            json!([
                { "id": "e1", "type": "order.created", "data": { "id": 9001 } },
                { "id": "e2", "type": "device.revoked", "data": { "device_id": "dev_1" } }
            ]),
            "c3",
        );
        let report = apply_events(&ctx(&shop, &link, &remote, &settings), &events).unwrap();
        assert_eq!((report.orders, report.revoked), (1, true));
        assert_eq!(shop.count("SELECT COUNT(*) FROM online_orders"), 1);
    }

    #[test]
    fn flags_are_salted_hashes_and_only_risky_buyers_are_kept() {
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        let settings = shop.db.read(cloud_settings).unwrap();
        let events = page(
            json!([
                { "id": "f1", "type": "customer.updated", "data": { "id": 1, "phone": "+213 550 12 34 56", "is_banned": true } },
                { "id": "f2", "type": "customer.updated", "data": { "id": 2, "phone": "0661 00 00 00", "is_banned": false, "fraud_score": 12 } }
            ]),
            "c4",
        );
        assert_eq!(
            apply_events(&ctx(&shop, &link, &remote, &settings), &events)
                .unwrap()
                .flags,
            1
        );
        let flag = shop
            .db
            .read(|c| lookup_flag(c, "141", "0550123456"))
            .unwrap()
            .unwrap();
        assert!(flag.is_banned);
        assert!(
            shop.db
                .read(|c| lookup_flag(c, "141", "0661000000"))
                .unwrap()
                .is_none()
        );
        let stored: Vec<(String, String)> = shop
            .db
            .read(|c| {
                let mut stmt =
                    c.prepare("SELECT phone_hash, remote_customer_id FROM remote_customer_flags")?;
                let rows = stmt
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .unwrap();
        assert_eq!(
            stored,
            [(flag_hash("141", "0550123456"), "1".to_owned())],
            "the salted hash and DZBuild's id, no phone"
        );
        assert_ne!(
            flag_hash("141", "0550123456"),
            flag_hash("152", "0550123456"),
            "salted per store"
        );
    }

    #[test]
    fn without_the_events_feed_orders_and_products_are_pulled_by_date() {
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        dz.mount(
            Mock::given(path("/v1/orders"))
                .and(query_param("updated_since", "1970-01-01T01:00:00+01:00"))
                .respond_with(ok(json!({ "items": [{
                    "id": 9001, "order_number": 1042, "status": "pending", "subtotal": 100, "total": 100,
                    "created_at": "2026-09-28 10:00:00", "updated_at": "2026-09-28 11:00:00", "items": []
                }], "next_cursor": null, "has_more": false }))),
        );
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        let settings = shop.db.read(cloud_settings).unwrap();
        assert_eq!(
            pull_orders(&ctx(&shop, &link, &remote, &settings))
                .unwrap()
                .orders,
            1
        );
        assert_eq!(
            shop.db
                .read(|c| store::remote_cursor(c, "cloud:orders"))
                .unwrap()
                .as_deref(),
            Some("2026-09-28T11:00:00+01:00"),
            "the next pull starts at the newest order seen"
        );
    }

    #[test]
    fn the_first_link_imports_simple_online_products_and_matches_by_sku() {
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        let mut known = crate::catalog::products::ProductInput::simple(
            "Café 250 g",
            crate::catalog::UNIT_PIECE_ID,
            &shop.tva9,
            Money(35_000),
        );
        known.reference = Some("CAFE-250G".into());
        let known = shop.product_with(known);
        let mut tea = crate::catalog::products::ProductInput::simple(
            "Thé 100 g",
            crate::catalog::UNIT_PIECE_ID,
            &shop.tva9,
            Money(20_000),
        );
        tea.reference = Some("THE-100G".into());
        shop.product_with(tea);
        dz.mount(Mock::given(path("/v1/products")).respond_with(ok(json!({ "items": [
            { "id": 1, "name": "قهوة", "sku": "CAFE-250G", "pricing": { "price": 350 }, "status": "active", "has_variants": false },
            { "id": 2, "name": "حقيبة", "sku": "SAC-1", "barcode": "6130000012346", "pricing": { "price": 4500 },
              "inventory": { "track_stock": true, "stock_quantity": 4 }, "status": "active", "has_variants": false },
            { "id": 3, "name": "حذاء", "pricing": { "price": 6000 }, "status": "active", "has_variants": true },
            // archived online: one new here, one matched by SKU
            { "id": 4, "name": "قبعة", "sku": "HAT-1", "pricing": { "price": 900 },
              "inventory": { "track_stock": true, "stock_quantity": 2 }, "status": "archived", "has_variants": false },
            { "id": 5, "name": "شاي", "sku": "THE-100G", "pricing": { "price": 200 }, "status": "archived", "has_variants": false }
        ], "next_cursor": null, "has_more": false }))));
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        let settings = shop.db.read(cloud_settings).unwrap();
        let report = import_online(&ctx(&shop, &link, &remote, &settings)).unwrap();
        assert_eq!(
            report,
            ImportReport {
                imported: 2,
                matched: 2,
                skipped_variants: 1
            }
        );
        let known_id = known.id.clone();
        assert_eq!(
            shop.db
                .read(move |c| store::map_get(c, "141", MapEntity::Product, &known_id))
                .unwrap()
                .unwrap()
                .remote_id,
            "1"
        );
        let bag: (String, i64) = shop
            .db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT id, sell_price FROM products WHERE reference = 'SAC-1'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?)
            })
            .unwrap();
        assert_eq!(bag.1, 450_000);
        let id = bag.0.clone();
        assert_eq!(
            shop.db
                .read(move |c| crate::stock::on_hand(c, &id))
                .unwrap(),
            crate::money::Qty(4_000)
        );
        let confirmed: i64 = shop.count("SELECT confirmed FROM cloud_stock WHERE product_id IN (SELECT id FROM products WHERE reference = 'SAC-1')");
        assert_eq!(
            confirmed, 4,
            "DZBuild already holds it: nothing to push back"
        );
        // an archived online product stays off the storefront: not published, still mapped, so
        // the push archives it instead of listing it again
        run_derive(&shop, &link);
        let state = |reference: &'static str| -> (String, i64, bool, Option<OnlineStatus>) {
            shop.db
                .read(move |c| {
                    let (id, publish): (String, i64) = c.query_row(
                        "SELECT p.id, cp.publish FROM products p JOIN cloud_publish cp ON cp.product_id = p.id
                         WHERE p.reference = ?1",
                        [reference],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )?;
                    let mapped = store::map_get(c, "141", MapEntity::Product, &id)?.is_some();
                    let product = crate::catalog::products::get(c, &id)?;
                    let decision = publish_decision(c, "141", &product, &cloud_settings(c)?)?;
                    Ok((id, publish, mapped, decision))
                })
                .unwrap()
        };
        for (reference, remote_id) in [("HAT-1", "4"), ("THE-100G", "5")] {
            let (id, publish, mapped, decision) = state(reference);
            assert_eq!(
                (publish, mapped, decision),
                (0, true, Some(OnlineStatus::Archived)),
                "{reference}: archived online, never pushed back as active"
            );
            assert_eq!(
                shop.db
                    .read(move |c| store::map_get(c, "141", MapEntity::Product, &id))
                    .unwrap()
                    .unwrap()
                    .remote_id,
                remote_id
            );
        }
        for reference in ["CAFE-250G", "SAC-1"] {
            let (_, publish, mapped, decision) = state(reference);
            assert_eq!(
                (publish, mapped, decision),
                (1, true, Some(OnlineStatus::Active)),
                "{reference}"
            );
        }
    }

    // Every other HTTP path of the pull, against wiremock (binding rule), and the feed's
    // resilience to one item the catalogue or the mirror refuses.

    #[test]
    fn the_events_feed_is_long_polled_from_the_cursor() {
        let dz = MockDz::start();
        let (_shop, link) = linked_shop();
        dz.mount(Mock::given(path("/v1/events")).respond_with(ok(json!({
            "items": [{ "id": "evt_1", "type": "order.created", "data": { "id": 9001 } }],
            "next_cursor": "c8", "has_more": true
        }))));
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        let first = long_poll(&remote, None, 25).unwrap();
        let event = &first.items[0];
        assert_eq!(
            (event.id.as_str(), event.kind.as_str()),
            ("evt_1", "order.created")
        );
        assert_eq!(
            event.data.get(),
            r#"{"id":9001}"#,
            "the data is kept as raw JSON text"
        );
        assert_eq!(
            (first.next_cursor.as_deref(), first.has_more),
            (Some("c8"), true)
        );
        long_poll(&remote, Some("c8"), 25).unwrap();
        let queries: Vec<String> = dz
            .requests()
            .iter()
            .map(|r| r.url.query().unwrap_or_default().to_owned())
            .collect();
        assert_eq!(
            queries,
            ["wait=25&limit=200", "wait=25&limit=200&cursor=c8"]
        );
    }

    #[test]
    fn without_the_events_feed_online_edits_are_pulled_by_date_page_by_page() {
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        shop.set("cloud.prices.pull", json!(true));
        let bag = shop.product("Sac", Money(450_000), &shop.tva19);
        pushed(&shop, &bag.id, "Sac", 450_000);
        let item = |price: i64, updated: &str| {
            json!({ "id": 88121, "external_id": bag.id, "name": "Sac", "pricing": { "price": price },
                    "status": "active", "has_variants": false, "updated_at": updated })
        };
        dz.mount(
            Mock::given(path("/v1/products"))
                .and(query_param("updated_since", EPOCH))
                .and(query_param("cursor", "p2"))
                .respond_with(ok(json!({ "items": [item(4_800, "2026-09-28 11:30:00")], "next_cursor": null, "has_more": false }))),
        );
        dz.mount(
            Mock::given(path("/v1/products"))
                .and(query_param("updated_since", EPOCH))
                .respond_with(ok(json!({ "items": [item(4_700, "2026-09-28 11:00:00")], "next_cursor": "p2", "has_more": true }))),
        );
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        let settings = shop.db.read(cloud_settings).unwrap();
        assert_eq!(
            pull_products(&ctx(&shop, &link, &remote, &settings))
                .unwrap()
                .products,
            2
        );
        let id = bag.id.clone();
        assert_eq!(
            shop.db
                .read(move |c| crate::catalog::products::get(c, &id))
                .unwrap()
                .sell_price,
            Money(480_000)
        );
        assert_eq!(
            shop.db
                .read(|c| store::remote_cursor(c, "cloud:products"))
                .unwrap()
                .as_deref(),
            Some("2026-09-28T11:30:00+01:00")
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'cloud.pulled'"),
            2,
            "one per page"
        );
        dz.mount(
            Mock::given(path("/v1/products"))
                .and(query_param("updated_since", "2026-09-28T11:30:00+01:00"))
                .respond_with(ok(
                    json!({ "items": [], "next_cursor": null, "has_more": false }),
                ))
                .expect(1),
        );
        assert_eq!(
            pull_products(&ctx(&shop, &link, &remote, &settings)).unwrap(),
            PullReport::default()
        );
        dz.verify();
    }

    #[test]
    fn a_phone_typed_at_the_till_is_checked_with_dzbuild_and_only_its_hash_is_kept() {
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        let answer = |score: i64| {
            ok(
                json!({ "items": [{ "id": 7, "is_banned": false, "fraud_score": score }], "next_cursor": null, "has_more": false }),
            )
        };
        dz.mount(
            Mock::given(path("/v1/customers"))
                .and(query_param("phone", "0550123456"))
                .respond_with(answer(85))
                .up_to_n_times(1),
        );
        dz.mount(
            Mock::given(path("/v1/customers"))
                .and(query_param("phone", "0550123456"))
                .respond_with(answer(20)),
        );
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        let flag = refresh_flag(&remote, &shop.db, &link, "+213 550 12 34 56", NOON).unwrap();
        assert_eq!(
            flag,
            Some(CustomerFlag {
                is_banned: false,
                fraud_score: Some(85)
            })
        );
        assert_eq!(
            shop.db
                .read(|c| lookup_flag(c, "141", "0550 12 34 56"))
                .unwrap(),
            flag
        );
        let stored: Vec<(String, String)> = shop
            .db
            .read(|c| {
                let mut stmt =
                    c.prepare("SELECT phone_hash, remote_customer_id FROM remote_customer_flags")?;
                let rows = stmt
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .unwrap();
        assert_eq!(stored, [(flag_hash("141", "0550123456"), "7".to_owned())]);
        // DZBuild's score went down: the flag goes
        assert_eq!(
            refresh_flag(&remote, &shop.db, &link, "0550123456", NOON).unwrap(),
            None
        );
        assert_eq!(
            shop.db
                .read(|c| lookup_flag(c, "141", "0550123456"))
                .unwrap(),
            None
        );
        assert_eq!(shop.count("SELECT COUNT(*) FROM remote_customer_flags"), 0);
        // not a phone: nothing is asked
        assert_eq!(
            refresh_flag(&remote, &shop.db, &link, "12", NOON).unwrap(),
            None
        );
        assert_eq!(dz.requests().len(), 2);
    }

    #[test]
    fn a_plan_change_reads_the_store_profile_again() {
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        dz.mount(Mock::given(path("/v1/store")).respond_with(ok(json!({
            "name": "Boutique Amel", "plan": "business", "plan_limits": { "active_products": 1000 }
        }))));
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        let settings = shop.db.read(cloud_settings).unwrap();
        let events = page(
            json!([{ "id": "s1", "type": "store.plan_changed", "data": {} }]),
            "c6",
        );
        assert_eq!(
            apply_events(&ctx(&shop, &link, &remote, &settings), &events)
                .unwrap()
                .applied,
            1
        );
        let now = shop.db.read(store::active_link).unwrap().unwrap();
        assert_eq!(
            (now.store_plan.as_deref(), now.active_products_cap),
            (Some("business"), Some(1_000))
        );
    }

    #[test]
    fn an_event_the_catalogue_refuses_is_recorded_failed_and_the_feed_goes_on() {
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        let bag = shop.product("Sac", Money(450_000), &shop.tva19);
        pushed(&shop, &bag.id, "Sac", 450_000);
        // a name longer than the catalogue takes (120 characters), with a new price
        let long = "حقيبة جلد ".repeat(20);
        dz.mount(
            Mock::given(path("/v1/products/88121")).respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(remote_product(&bag.id, &long, 4_700, "active")),
            ),
        );
        dz.mount(Mock::given(path("/v1/orders/9001")).respond_with(ok(json!({
            "id": 9001, "order_number": 1042, "status": "confirmed", "subtotal": 100, "total": 100,
            "created_at": "2026-09-28 10:00:00", "updated_at": "2026-09-28 10:00:00", "items": []
        }))));
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        let settings = shop.db.read(cloud_settings).unwrap();
        let events = page(
            json!([
                { "id": "evt_a", "type": "product.updated", "data": { "id": 88121, "external_id": bag.id } },
                { "id": "evt_b", "type": "order.created", "data": { "id": 9001 } }
            ]),
            "c5",
        );
        let report = apply_events(&ctx(&shop, &link, &remote, &settings), &events).unwrap();
        assert_eq!(
            (report.products, report.orders, report.applied),
            (0, 1, 1),
            "the order behind it still comes in"
        );
        let id = bag.id.clone();
        let product = shop
            .db
            .read(move |c| crate::catalog::products::get(c, &id))
            .unwrap();
        assert_eq!(
            (product.name_ar.as_str(), product.sell_price),
            ("", Money(450_000)),
            "the whole event is undone"
        );
        let outcomes: Vec<(String, String)> = shop
            .db
            .read(|c| {
                let mut stmt =
                    c.prepare("SELECT event_id, outcome FROM cloud_inbox ORDER BY event_id")?;
                let rows = stmt
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .unwrap();
        assert_eq!(
            outcomes,
            [
                ("evt_a".to_owned(), "failed".to_owned()),
                ("evt_b".to_owned(), "applied".to_owned())
            ]
        );
        assert_eq!(
            shop.db
                .read(|c| store::remote_cursor(c, "cloud:events"))
                .unwrap()
                .as_deref(),
            Some("c5")
        );
    }

    #[test]
    fn an_order_the_mirror_refuses_is_skipped_in_a_dated_pull() {
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        let order = |id: i64, shipping: i64, updated: &str| {
            json!({ "id": id, "order_number": id, "status": "pending", "subtotal": 100, "shipping_cost": shipping, "total": 100,
                    "created_at": "2026-09-28 10:00:00", "updated_at": updated, "items": [] })
        };
        dz.mount(Mock::given(path("/v1/orders")).respond_with(ok(json!({
            "items": [order(9001, -1, "2026-09-28 11:30:00"), order(9002, 0, "2026-09-28 11:00:00")],
            "next_cursor": null, "has_more": false
        }))));
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        let settings = shop.db.read(cloud_settings).unwrap();
        assert_eq!(
            pull_orders(&ctx(&shop, &link, &remote, &settings))
                .unwrap()
                .orders,
            1
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM online_orders WHERE remote_id = '9002'"),
            1
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM online_orders"),
            1,
            "a negative fee is refused, alone"
        );
        assert_eq!(
            shop.db
                .read(|c| store::remote_cursor(c, "cloud:orders"))
                .unwrap()
                .as_deref(),
            Some("2026-09-28T11:30:00+01:00"),
            "seen: an edit online brings it back"
        );
    }

    /// DZBuild may page newest first: a walk broken after page 1 keeps the old mark, so the older
    /// orders of page 2 are pulled by the next walk instead of being skipped for good.
    #[test]
    fn a_dated_walk_broken_after_a_page_keeps_the_old_mark() {
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        let order = |id: i64, updated: &str| {
            json!({ "id": id, "order_number": id, "status": "pending", "subtotal": 100, "total": 100,
                    "created_at": "2026-09-28 10:00:00", "updated_at": updated, "items": [] })
        };
        // page 2 fails once (a refusal, not retried), then answers
        dz.mount(
            Mock::given(path("/v1/orders"))
                .and(query_param("cursor", "p2"))
                .respond_with(
                    ResponseTemplate::new(400)
                        .set_body_json(json!({ "error": { "code": "busy" } })),
                )
                .up_to_n_times(1),
        );
        dz.mount(
            Mock::given(path("/v1/orders"))
                .and(query_param("cursor", "p2"))
                .respond_with(ok(json!({
                    "items": [order(9001, "2026-09-28 09:00:00")],
                    "next_cursor": null, "has_more": false
                }))),
        );
        dz.mount(Mock::given(path("/v1/orders")).respond_with(ok(json!({
            "items": [order(9003, "2026-09-28 12:00:00")],
            "next_cursor": "p2", "has_more": true
        }))));
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        let settings = shop.db.read(cloud_settings).unwrap();
        assert!(pull_orders(&ctx(&shop, &link, &remote, &settings)).is_err());
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM online_orders WHERE remote_id = '9003'"),
            1,
            "page 1 is applied"
        );
        assert_eq!(
            shop.db
                .read(|c| store::remote_cursor(c, "cloud:orders"))
                .unwrap(),
            None,
            "the mark waits for the last page"
        );
        // the next walk gets every page: the older order of page 2 arrives, then the mark moves
        pull_orders(&ctx(&shop, &link, &remote, &settings)).unwrap();
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM online_orders WHERE remote_id = '9001'"),
            1,
            "never skipped"
        );
        assert_eq!(
            shop.db
                .read(|c| store::remote_cursor(c, "cloud:orders"))
                .unwrap()
                .as_deref(),
            Some("2026-09-28T12:00:00+01:00"),
            "the newest of the whole walk"
        );
    }

    #[test]
    fn an_order_pulled_again_unchanged_is_neither_counted_nor_audited() {
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        let order = |id: i64| {
            json!({ "id": id, "order_number": id, "status": "pending", "subtotal": 100, "total": 100,
                    "created_at": "2026-09-28 10:00:00", "updated_at": "2026-09-28 11:00:00", "items": [] })
        };
        // `updated_since` is inclusive: the newest order comes back on every dated poll
        dz.mount(Mock::given(path("/v1/orders")).respond_with(ok(json!({
            "items": [order(9001)], "next_cursor": null, "has_more": false
        }))));
        dz.mount(Mock::given(path("/v1/orders/9002")).respond_with(ok(order(9002))));
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        let settings = shop.db.read(cloud_settings).unwrap();
        let audited = || shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'cloud.pulled'");
        let first = pull_orders(&ctx(&shop, &link, &remote, &settings)).unwrap();
        let second = pull_orders(&ctx(&shop, &link, &remote, &settings)).unwrap();
        assert_eq!((first.orders, second.orders), (1, 0), "nothing changed");
        assert_eq!(
            audited(),
            1,
            "one cloud.pulled, for the pull that changed the mirror"
        );
        // the feed: an order created then updated in one page (one detail, fetched twice)
        let events = page(
            json!([
                { "id": "o1", "type": "order.created", "data": { "id": 9002 } },
                { "id": "o2", "type": "order.updated", "data": { "id": 9002 } }
            ]),
            "c7",
        );
        let report = apply_events(&ctx(&shop, &link, &remote, &settings), &events).unwrap();
        assert_eq!((report.orders, report.applied), (1, 1), "counted once");
        let outcomes: Vec<(String, String)> = shop
            .db
            .read(|c| {
                let mut stmt =
                    c.prepare("SELECT event_id, outcome FROM cloud_inbox ORDER BY event_id")?;
                let rows = stmt
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .unwrap();
        assert_eq!(
            outcomes,
            [
                ("o1".to_owned(), "applied".to_owned()),
                ("o2".to_owned(), "ignored".to_owned())
            ]
        );
        assert_eq!(audited(), 2);
        // a later event for it, with nothing new behind it
        let again = page(
            json!([{ "id": "o3", "type": "order.updated", "data": { "id": 9002 } }]),
            "c8",
        );
        let report = apply_events(&ctx(&shop, &link, &remote, &settings), &again).unwrap();
        assert_eq!((report.orders, report.applied), (0, 0));
        assert_eq!(audited(), 2, "no audit row when nothing changed");
    }

    #[test]
    fn an_event_applied_by_another_pull_meanwhile_is_not_applied_twice() {
        use std::time::{Duration, Instant};
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        shop.stock_in(&milk.id, 10_000);
        // the store profile answers late: the engine's pull waits between its inbox check and
        // its transaction
        dz.mount(
            Mock::given(path("/v1/store")).respond_with(
                ok(json!({ "name": "Boutique Amel", "plan": "business" }))
                    .set_delay(Duration::from_secs(3)),
            ),
        );
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        let settings = shop.db.read(cloud_settings).unwrap();
        let stock = json!({ "id": "evt_1", "type": "inventory.level_changed",
                            "data": { "external_id": milk.id, "old": 10, "new": 8, "source": "dashboard" } });
        let engine_page = page(
            json!([stock.clone(), { "id": "evt_2", "type": "store.plan_changed", "data": {} }]),
            "c1",
        );
        let owner_page = page(json!([stock]), "c1");
        let engine = std::thread::scope(|s| {
            let engine =
                s.spawn(|| apply_events(&ctx(&shop, &link, &remote, &settings), &engine_page));
            // the engine asks for the profile once it has checked evt_1's inbox row
            let deadline = Instant::now() + Duration::from_secs(20);
            while !dz.requests().iter().any(|r| r.url.path() == "/v1/store") {
                assert!(Instant::now() < deadline, "the engine never asked");
                std::thread::sleep(Duration::from_millis(5));
            }
            // the owner's "sync now" applies the same event meanwhile
            let owner = apply_events(&ctx(&shop, &link, &remote, &settings), &owner_page).unwrap();
            assert_eq!(owner.stock_moves, 1);
            engine.join().unwrap().unwrap()
        });
        assert_eq!(
            (engine.stock_moves, engine.applied),
            (0, 1),
            "the plan change only"
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM stock_movements WHERE ref_type = 'dzbuild'"),
            1,
            "applied once"
        );
        let id = milk.id.clone();
        assert_eq!(
            shop.db
                .read(move |c| crate::stock::on_hand(c, &id))
                .unwrap(),
            crate::money::Qty(8_000)
        );
    }

    // Variant families (contract v1.12): the import and the feed.

    fn import_items(shop: &Shop, link: &CloudLink, items: serde_json::Value) -> ImportReport {
        let dz = MockDz::start();
        dz.mount(Mock::given(path("/v1/products")).respond_with(ok(
            json!({ "items": items, "next_cursor": null, "has_more": false }),
        )));
        let remote = remote_for_test(&dz, link, ManualClock::at(NOON));
        let settings = shop.db.read(cloud_settings).unwrap();
        import_online(&ctx(shop, link, &remote, &settings)).unwrap()
    }

    /// Shoes in sizes: (size, barcode, price in dinars, online stock) per item.
    fn shoes(sizes: &[(&str, &str, i64, Option<i64>)]) -> serde_json::Value {
        let items: Vec<serde_json::Value> = sizes
            .iter()
            .map(|(size, barcode, price, stock)| {
                json!({ "key": format!("taille={size}"), "external_id": null, "sku": format!("SHOE-1-{size}"),
                        "barcode": barcode, "price": price, "attributes": { "Taille": size }, "stock": stock,
                        "active": true })
            })
            .collect();
        json!({ "id": 10, "name": "حذاء", "sku": "SHOE-1", "pricing": { "price": 6000 }, "status": "active",
                "has_variants": true, "variants": {
                    "axes": [{ "name": "Taille", "values": sizes.iter().map(|s| s.0).collect::<Vec<_>>() }],
                    "stock_mode": "variant", "items": items } })
    }

    /// A T-shirt in two colours by two sizes, the white M without online stock.
    fn shirts(colours: &[&str]) -> serde_json::Value {
        let mut items = Vec::new();
        for colour in colours {
            for size in ["S", "M"] {
                let stock = if *colour == "Blanc" && size == "M" {
                    None
                } else {
                    Some(2)
                };
                items.push(json!({
                    "key": format!("couleur={}|taille={}", colour.to_lowercase(), size.to_lowercase()),
                    "sku": null, "barcode": null, "price": 1500, "stock": stock,
                    "attributes": { "Couleur": colour, "Taille": size }
                }));
            }
        }
        json!({ "id": 20, "name": "قميص", "sku": "TS01", "pricing": { "price": 1500 }, "status": "active",
                "has_variants": true, "variants": {
                    "axes": [{ "name": "Couleur", "values": colours }, { "name": "Taille", "values": ["S", "M"] }],
                    "stock_mode": "variant", "items": items } })
    }

    /// The children of the local product referenced `reference`: (id, name, reference, price,
    /// on hand in pieces), by reference.
    fn children_of(
        shop: &Shop,
        reference: &'static str,
    ) -> Vec<(String, String, Option<String>, i64, i64)> {
        shop.db
            .read(move |c| {
                let mut stmt = c.prepare(
                    "SELECT p.id, p.name_ar, p.reference, p.sell_price FROM products p
                     JOIN products parent ON parent.id = p.parent_id
                     WHERE parent.reference = ?1 AND p.kind = 'variant' ORDER BY p.rowid",
                )?;
                let rows = stmt
                    .query_map([reference], |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                    })?
                    .collect::<Result<Vec<(String, String, Option<String>, i64)>, _>>()?;
                rows.into_iter()
                    .map(|(id, name, reference, price)| {
                        let pieces = crate::stock::on_hand(c, &id)?.0 / 1_000;
                        Ok((id, name, reference, price, pieces))
                    })
                    .collect()
            })
            .unwrap()
    }

    fn parent_id(shop: &Shop, reference: &'static str) -> String {
        shop.db
            .read(move |c| {
                Ok(c.query_row(
                    "SELECT id FROM products WHERE reference = ?1 AND kind = 'variant_parent'",
                    [reference],
                    |r| r.get(0),
                )?)
            })
            .unwrap()
    }

    fn mapped_keys(shop: &Shop, parent: &str) -> Vec<(String, String, Option<Money>)> {
        let parent = parent.to_owned();
        shop.db
            .read(move |c| crate::cloud::variants::map_children(c, "141", &parent))
            .unwrap()
            .into_iter()
            .map(|r| (r.variant_key, r.local_id, r.price_online))
            .collect()
    }

    #[test]
    fn the_variants_block_reads_tolerantly() {
        let read = |variants: &str| -> RemoteProduct {
            serde_json::from_str(&format!(
                r#"{{"id": 1, "name": "x", "pricing": {{"price": 10}}, "status": "active", "has_variants": true, "variants": {variants}}}"#
            ))
            .unwrap()
        };
        assert!(read("null").variants.is_none());
        assert!(read(r#""oops""#).variants.is_none(), "garbage is no family");
        assert!(read(r#"{"axes": 3}"#).variants.is_none());
        let family = read(
            r#"{"axes": [{"name": "Taille", "values": ["41"]}], "stock_mode": "none", "later": 1,
                "items": [{"key": "taille=41", "price": "4500.50", "attributes": {"Taille": "41"}, "stock": null}]}"#,
        )
        .variants
        .unwrap();
        let item = &family.items[0];
        assert_eq!(
            (item.price.0, item.stock, item.active, item.sku.as_deref()),
            (Money(450_050), None, true, None),
            "an unknown field is ignored, absent fields take their defaults"
        );
    }

    #[test]
    fn a_one_axis_family_is_imported_as_a_parent_and_its_children() {
        let (shop, link) = linked_shop();
        let report = import_items(
            &shop,
            &link,
            json!([
                shoes(&[
                    ("40", "6130000000401", 6000, Some(3)),
                    ("41", "6130000000418", 6500, Some(0))
                ]),
                { "id": 2, "name": "حقيبة", "sku": "SAC-1", "pricing": { "price": 4500 }, "status": "active",
                  "has_variants": false, "variants": null }
            ]),
        );
        assert_eq!(
            report,
            ImportReport {
                imported: 2,
                matched: 0,
                skipped_variants: 0
            }
        );
        let parent = parent_id(&shop, "SHOE-1");
        let id = parent.clone();
        let model = shop
            .db
            .read(move |c| crate::catalog::products::get(c, &id))
            .unwrap();
        assert_eq!(
            (
                model.name_ar.as_str(),
                model.sell_price,
                model.track_stock,
                model.barcodes.len()
            ),
            ("حذاء", Money(600_000), false, 0),
            "the model holds no stock and no barcode"
        );
        let children = children_of(&shop, "SHOE-1");
        let shown: Vec<(&str, Option<&str>, i64, i64)> = children
            .iter()
            .map(|(_, name, reference, price, pieces)| {
                (name.as_str(), reference.as_deref(), *price, *pieces)
            })
            .collect();
        assert_eq!(
            shown,
            [
                ("حذاء 40", Some("SHOE-1-40"), 600_000, 3),
                ("حذاء 41", Some("SHOE-1-41"), 650_000, 0)
            ]
        );
        let first = children[0].0.clone();
        let (barcodes, attributes) = shop
            .db
            .read(move |c| {
                let p = crate::catalog::products::get(c, &first)?;
                Ok((
                    p.barcodes,
                    crate::cloud::variants::attributes_of(c, &first)?,
                ))
            })
            .unwrap();
        assert_eq!(
            barcodes,
            [Barcode {
                barcode: "6130000000401".into(),
                pack_qty: Qty::ONE,
                label: None
            }]
        );
        assert_eq!(attributes["Taille"], "40");
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM cloud_stock WHERE product_id IN (SELECT id FROM products WHERE kind = 'variant')"),
            2
        );
        assert_eq!(
            shop.count("SELECT confirmed FROM cloud_stock WHERE product_id IN (SELECT id FROM products WHERE reference = 'SHOE-1-40')"),
            3
        );
        assert_eq!(
            mapped_keys(&shop, &parent),
            [
                (
                    "taille=40".to_owned(),
                    children[0].0.clone(),
                    Some(Money(600_000))
                ),
                (
                    "taille=41".to_owned(),
                    children[1].0.clone(),
                    Some(Money(650_000))
                )
            ]
        );
        let id = parent.clone();
        assert_eq!(
            shop.db
                .read(move |c| store::map_get(c, "141", MapEntity::Product, &id))
                .unwrap()
                .unwrap()
                .remote_id,
            "10"
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM cloud_publish WHERE publish = 1 AND product_id IN (SELECT id FROM products WHERE kind = 'variant_parent')"),
            1
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM cloud_publish WHERE product_id IN (SELECT id FROM products WHERE kind = 'variant')"),
            0,
            "the children go online with their parent"
        );
    }

    #[test]
    fn a_two_axis_family_imports_once_and_a_key_gone_online_keeps_its_child() {
        let (shop, link) = linked_shop();
        let page = json!([shirts(&["Noir", "Blanc"])]);
        assert_eq!(import_items(&shop, &link, page.clone()).imported, 1);
        let children = children_of(&shop, "TS01");
        let names: Vec<(&str, Option<&str>)> = children
            .iter()
            .map(|(_, name, reference, ..)| (name.as_str(), reference.as_deref()))
            .collect();
        assert_eq!(
            names,
            [
                ("قميص Noir S", Some("TS01-NOIR-S")),
                ("قميص Noir M", Some("TS01-NOIR-M")),
                ("قميص Blanc S", Some("TS01-BLANC-S")),
                ("قميص Blanc M", Some("TS01-BLANC-M"))
            ],
            "the values in axis order; no online SKU: the generated reference"
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM cloud_stock WHERE product_id IN (SELECT id FROM products WHERE kind = 'variant')"),
            3,
            "the white M has no online stock to follow"
        );
        assert_eq!(
            import_items(&shop, &link, page),
            ImportReport::default(),
            "the same page again"
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM products WHERE kind = 'variant'"),
            4
        );
        // the white is gone online
        import_items(&shop, &link, json!([shirts(&["Noir"])]));
        assert_eq!(children_of(&shop, "TS01").len(), 4, "every child stays");
        let parent = parent_id(&shop, "TS01");
        let keys: Vec<String> = mapped_keys(&shop, &parent)
            .into_iter()
            .map(|(key, ..)| key)
            .collect();
        assert_eq!(keys, ["couleur=noir|taille=m", "couleur=noir|taille=s"]);
    }

    #[test]
    fn a_local_family_is_matched_by_its_reference_and_its_children_by_their_values() {
        let (shop, link) = linked_shop();
        let mut model = crate::catalog::products::ProductInput::simple(
            "T-shirt",
            crate::catalog::UNIT_PIECE_ID,
            &shop.tva19,
            Money(150_000),
        );
        model.kind = crate::catalog::products::ProductKind::VariantParent;
        model.track_stock = false;
        model.reference = Some("TS01".into());
        let model = shop.product_with(model);
        let (actor, id) = (shop.owner.actor(), model.id.clone());
        shop.db
            .write(move |tx| {
                crate::catalog::variants::generate(
                    tx,
                    &actor,
                    &id,
                    crate::catalog::variants::VariantsInput {
                        axes: vec![
                            crate::catalog::variants::VariantAxis {
                                name: "Taille".into(),
                                values: vec!["S".into(), "M".into()],
                            },
                            crate::catalog::variants::VariantAxis {
                                name: "Couleur".into(),
                                values: vec!["Noir".into()],
                            },
                        ],
                        sell_price: None,
                        cost_price: None,
                        internal_barcodes: false,
                    },
                )
            })
            .unwrap();
        let report = import_items(&shop, &link, json!([shirts(&["Noir", "Blanc"])]));
        assert_eq!(
            report,
            ImportReport {
                imported: 0,
                matched: 1,
                skipped_variants: 0
            }
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM products WHERE kind = 'variant'"),
            4,
            "the two black ones are the shop's, the two white ones are new"
        );
        assert_eq!(mapped_keys(&shop, &model.id).len(), 4);
    }

    #[test]
    fn a_family_this_version_cannot_take_is_skipped_and_the_page_goes_on() {
        let (shop, link) = linked_shop();
        let four_axes: Vec<serde_json::Value> = ["A", "B", "C", "D"]
            .iter()
            .map(|a| json!({ "name": a, "values": ["1"] }))
            .collect();
        // more than the till's 200 combinations
        let too_many: Vec<serde_json::Value> = (0..201)
            .map(|n| json!({ "key": format!("n={n}"), "price": 100, "attributes": { "N": n.to_string() } }))
            .collect();
        let report = import_items(
            &shop,
            &link,
            json!([
                { "id": 31, "name": "a", "pricing": { "price": 100 }, "status": "active", "has_variants": true,
                  "variants": { "axes": "garbage" } },
                { "id": 32, "name": "b", "pricing": { "price": 100 }, "status": "active", "has_variants": true,
                  "variants": { "axes": four_axes, "stock_mode": "none", "items": [
                      { "key": "a=1|b=1|c=1|d=1", "price": 100, "attributes": { "A": "1", "B": "1", "C": "1", "D": "1" } }
                  ] } },
                { "id": 33, "name": "c", "pricing": { "price": 100 }, "status": "active", "has_variants": true,
                  "variants": { "axes": [{ "name": "Taille", "values": [] }], "stock_mode": "none", "items": [] } },
                { "id": 34, "name": "d", "pricing": { "price": 100 }, "status": "active", "has_variants": true },
                { "id": 36, "name": "f", "pricing": { "price": 100 }, "status": "active", "has_variants": true,
                  "variants": { "axes": [{ "name": "N", "values": [] }], "stock_mode": "none", "items": too_many } },
                { "id": 35, "name": "e", "sku": "E-1", "pricing": { "price": 100 }, "status": "active", "has_variants": false }
            ]),
        );
        assert_eq!(
            report,
            ImportReport {
                imported: 1,
                matched: 0,
                skipped_variants: 5
            }
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM products WHERE kind != 'simple'"),
            0
        );
    }

    fn feed(
        shop: &Shop,
        link: &CloudLink,
        dz: &MockDz,
        events: serde_json::Value,
        cursor: &str,
    ) -> PullReport {
        let remote = remote_for_test(dz, link, ManualClock::at(NOON));
        let settings = shop.db.read(cloud_settings).unwrap();
        apply_events(&ctx(shop, link, &remote, &settings), &page(events, cursor)).unwrap()
    }

    #[test]
    fn a_family_updated_online_gets_its_new_children_and_prices() {
        let (shop, link) = linked_shop();
        shop.set("cloud.prices.pull", json!(true));
        import_items(
            &shop,
            &link,
            json!([shoes(&[("40", "6130000000401", 6000, Some(3))])]),
        );
        let dz = MockDz::start();
        dz.mount(
            Mock::given(path("/v1/products/10")).respond_with(ok(shoes(&[
                ("40", "6130000000401", 6200, Some(3)),
                ("42", "6130000000425", 6000, Some(5)),
            ]))),
        );
        let report = feed(
            &shop,
            &link,
            &dz,
            json!([{ "id": "u1", "type": "product.updated", "data": { "id": 10 } }]),
            "c1",
        );
        assert_eq!(report.products, 1);
        let shown: Vec<(String, i64, i64)> = children_of(&shop, "SHOE-1")
            .into_iter()
            .map(|(_, name, _, price, pieces)| (name, price, pieces))
            .collect();
        assert_eq!(
            shown,
            [
                ("حذاء 40".to_owned(), 620_000, 3),
                ("حذاء 42".to_owned(), 600_000, 5)
            ],
            "the new size comes in with its stock; the online price follows (cloud.prices.pull)"
        );
        let parent = parent_id(&shop, "SHOE-1");
        assert_eq!(mapped_keys(&shop, &parent).len(), 2);
    }

    #[test]
    fn a_simple_product_that_became_a_family_online_is_left_alone() {
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        let bag = shop.product("Sac", Money(450_000), &shop.tva19);
        pushed(&shop, &bag.id, "Sac", 450_000);
        let mut family = shoes(&[("40", "6130000000401", 6000, Some(3))]);
        family["id"] = json!(88121);
        family["name"] = json!("Sac cuir");
        dz.mount(Mock::given(path("/v1/products/88121")).respond_with(ok(family)));
        let report = feed(
            &shop,
            &link,
            &dz,
            json!([{ "id": "u2", "type": "product.updated", "data": { "id": 88121 } }]),
            "c2",
        );
        assert_eq!(report.products, 0);
        let id = bag.id.clone();
        let now = shop
            .db
            .read(move |c| crate::catalog::products::get(c, &id))
            .unwrap();
        assert_eq!(
            (now.name_ar.as_str(), now.kind),
            ("", crate::catalog::products::ProductKind::Simple),
            "the owner turns it into a family by hand"
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM products WHERE kind = 'variant'"),
            0
        );
    }

    #[test]
    fn a_stock_event_of_a_variant_moves_the_child_and_the_parent_level_one_is_ignored() {
        let (shop, link) = linked_shop();
        import_items(
            &shop,
            &link,
            json!([shoes(&[
                ("40", "6130000000401", 6000, Some(3)),
                ("41", "6130000000418", 6000, Some(0))
            ])]),
        );
        let children = children_of(&shop, "SHOE-1");
        let (forty, forty_one) = (children[0].0.clone(), children[1].0.clone());
        let dz = MockDz::start();
        let report = feed(
            &shop,
            &link,
            &dz,
            json!([
                { "id": "s1", "type": "inventory.level_changed",
                  "data": { "product_id": 10, "external_id": null, "variant_key": "taille=40",
                            "variant_external_id": null, "old": 3, "new": 1, "source": "dashboard" } },
                { "id": "s2", "type": "inventory.level_changed",
                  "data": { "product_id": 10, "variant_key": "taille=41", "variant_external_id": forty_one,
                            "old": 0, "new": 5, "source": "dashboard" } },
                { "id": "s3", "type": "inventory.level_changed",
                  "data": { "product_id": 10, "old": 6, "new": 4, "source": "dashboard" } }
            ]),
            "c3",
        );
        assert_eq!(report.stock_moves, 2);
        let pieces: Vec<i64> = children_of(&shop, "SHOE-1")
            .into_iter()
            .map(|c| c.4)
            .collect();
        assert_eq!(pieces, [1, 5]);
        let confirmed = |id: String| {
            shop.db
                .read(move |c| {
                    Ok(c.query_row(
                        "SELECT confirmed FROM cloud_stock WHERE product_id = ?1",
                        [id],
                        |r| r.get::<_, i64>(0),
                    )?)
                })
                .unwrap()
        };
        assert_eq!((confirmed(forty), confirmed(forty_one)), (1, 5));
        let parent = parent_id(&shop, "SHOE-1");
        assert_eq!(
            shop.db
                .read(move |c| Ok(c.query_row(
                    "SELECT COUNT(*) FROM stock_movements WHERE product_id = ?1",
                    [parent],
                    |r| r.get::<_, i64>(0)
                )?))
                .unwrap(),
            0,
            "the parent holds no stock"
        );
        assert_eq!(
            shop.count(
                "SELECT COUNT(*) FROM cloud_inbox WHERE event_id = 's3' AND outcome = 'ignored'"
            ),
            1
        );
    }

    #[test]
    fn a_family_imported_in_an_ht_shop_takes_its_prices_ht() {
        let (shop, link) = linked_ht_shop();
        import_items(
            &shop,
            &link,
            json!([shoes(&[("40", "6130000000401", 5950, None)])]),
        );
        let id = parent_id(&shop, "SHOE-1");
        let model = shop
            .db
            .read(move |c| crate::catalog::products::get(c, &id))
            .unwrap();
        let child = &children_of(&shop, "SHOE-1")[0];
        assert_eq!(
            (model.sell_price, child.3),
            (Money(504_202), 500_000),
            "6000 and 5950 TTC at 19 %"
        );
    }
}

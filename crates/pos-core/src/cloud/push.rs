//! Sends the due outbox items to DZBuild, kind by kind in the push order (a product before its
//! photo and stock, a sale before its refund). One-shot writes (sales, refunds, closures, order
//! updates) carry their item's key; product batches the hash of their body and their rows' keys
//! (an unchanged product is skipped without a request); stock batches their frozen body. A Brief
//! endpoint not shipped yet defers its kind for a day with one notice; a plan cap drafts what
//! exceeds it; a 4xx fails the item for the owner; a network error on DZBuild's API stops the
//! cycle without charging an attempt (on a photo's storage host it only reschedules the photo).

use super::api::{BatchAnswer, CreatedRecord, MediaTicket, Remote, VariantAnswer};
use super::derive::{OnlineStatus, publish_decision};
use super::http::{Call, CallError, Endpoint, Lane};
use super::outbox::{self, OutboxItem, OutboxKind};
use super::payloads::{self, PayloadContext, ProductSnapshot};
use super::stock::{self, ItemOutcome, StockItem};
use super::store::{self, CloudLink, CloudNoticeKind, CloudSettings, MapEntity};
use super::variants::{self, VariantMapRow};
use crate::db::Db;
use crate::error::AppError;
use rusqlite::{Connection, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;

/// Products per `POST /v1/products/batch`.
pub const PRODUCT_BATCH: usize = 100;
const PER_KIND_PER_CYCLE: usize = 200;
/// A one-shot write DZBuild already holds (a lost answer, then a key past its 24 h): done.
const ALREADY_THERE: &[&str] = &[
    "already_exists",
    "duplicate_external_id",
    "external_id_taken",
];

pub struct PushContext<'a> {
    pub remote: &'a Remote,
    pub db: &'a Db,
    pub link: &'a CloudLink,
    pub settings: &'a CloudSettings,
    /// The media folder the product photos live in (`None`: photos wait).
    pub media_dir: Option<&'a Path>,
    pub now: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PushReport {
    pub sent: usize,
    pub unchanged: usize,
    pub failed: usize,
    pub rescheduled: usize,
    pub drafted: usize,
    pub not_ready: Vec<Endpoint>,
    /// Kinds the store's plan refused, waiting a day (`cloud_plan_cap`).
    pub plan_capped: Vec<Endpoint>,
}

/// The conditions that stop a whole cycle: no item is to blame.
fn stops(e: &CallError) -> bool {
    matches!(
        e,
        CallError::Offline(_) | CallError::Relink | CallError::Unauthorized
    )
}

/// AppError from the database while pushing: stop the kind, it is not the item's fault.
fn db_stop(e: AppError) -> CallError {
    tracing::error!(target: "cloud", code = %e.code(), "database error while pushing");
    CallError::Invalid(format!("database: {}", e.code()))
}

/// Pushes every kind's due items (at most [`PER_KIND_PER_CYCLE`] each). `Err` only for what
/// stops the cycle (`Offline`, `Relink`, `Unauthorized`); everything else is recorded per item.
pub fn push_due(ctx: &PushContext<'_>) -> Result<PushReport, CallError> {
    let mut report = PushReport::default();
    let store_id = ctx.link.store_id.clone();
    for kind in OutboxKind::ALL {
        let endpoint = kind.endpoint();
        let (s, now) = (store_id.clone(), ctx.now);
        if !ctx
            .db
            .read(move |c| store::endpoint_due(c, &s, endpoint, now))
            .map_err(db_stop)?
        {
            continue;
        }
        let (s, now) = (store_id.clone(), ctx.now);
        let items = ctx
            .db
            .read(move |c| outbox::due(c, &s, kind, now, PER_KIND_PER_CYCLE))
            .map_err(db_stop)?;
        if items.is_empty() {
            continue;
        }
        let outcome = match kind {
            OutboxKind::ProductUpsert => push_products(ctx, &items, &mut report),
            OutboxKind::MediaUpload => push_media(ctx, &items, &mut report),
            OutboxKind::StockBatch => push_stock(ctx, &items, &mut report),
            _ => push_one_by_one(ctx, &items, &mut report),
        };
        match outcome {
            Ok(()) => {
                let (s, now) = (store_id.clone(), ctx.now);
                ctx.db
                    .write(move |tx| store::set_ready(tx, &s, endpoint, now))
                    .map_err(db_stop)?;
            }
            Err(CallError::NotReady { .. }) => {
                let (s, now) = (store_id.clone(), ctx.now);
                ctx.db
                    .write(move |tx| {
                        store::set_not_ready(tx, &s, endpoint, now)?;
                        outbox::defer_kind(tx, &s, kind, now + store::NOT_READY_RETRY_MS)
                    })
                    .map_err(db_stop)?;
                report.not_ready.push(endpoint);
            }
            Err(CallError::PlanCap { code }) => {
                // The store's plan refuses this kind (POS sales on a plan without them…): the
                // kind waits a day like an endpoint not shipped, and the owner reads why, once.
                let (s, now) = (store_id.clone(), ctx.now);
                ctx.db
                    .write(move |tx| {
                        store::raise_notice(
                            tx,
                            CloudNoticeKind::PlanCap,
                            "cloud_plan_cap",
                            endpoint.name(),
                            json!({ "code": code }),
                            now,
                        )?;
                        outbox::defer_kind(tx, &s, kind, now + store::NOT_READY_RETRY_MS)
                    })
                    .map_err(db_stop)?;
                report.plan_capped.push(endpoint);
            }
            Err(e) if stops(&e) => return Err(e),
            Err(e) => {
                tracing::warn!(target: "cloud", kind = kind.as_str(), code = e.code(), "push of a kind stopped early");
            }
        }
    }
    Ok(report)
}

/// What becomes of one item after its answer: sent, failed for the owner (a 4xx), or later (a
/// transient failure). What stops the cycle or the kind is handed back.
fn settle(
    ctx: &PushContext<'_>,
    item: &OutboxItem,
    result: Result<Option<String>, CallError>,
    report: &mut PushReport,
) -> Result<(), CallError> {
    let (id, store_id, now) = (item.id.clone(), ctx.link.store_id.clone(), ctx.now);
    match result {
        Ok(remote_id) => {
            ctx.db
                .write(move |tx| outbox::mark_sent(tx, &id, remote_id.as_deref(), now))
                .map_err(db_stop)?;
            report.sent += 1;
            Ok(())
        }
        Err(CallError::Rejected { status: 409, .. }) if item.kind == OutboxKind::OrderClaim => {
            // Another device prepares the order: the local claim goes (an order already sold or
            // shipped here keeps its document), the owner is told, and the order's updates
            // queued behind the claim are skipped with it (`depends_on`).
            let (order_id, id) = (item.entity_id.clone(), item.id.clone());
            ctx.db
                .write(move |tx| {
                    super::orders::claim_refused(tx, &order_id, now)?;
                    outbox::skip(tx, &id, "claimed_elsewhere")
                })
                .map_err(db_stop)?;
            report.failed += 1;
            Ok(())
        }
        Err(CallError::Rejected {
            status,
            code,
            message,
            details,
        }) => {
            let failed = item.clone();
            let details = json!({ "status": status, "message": message, "details": details });
            ctx.db
                .write(move |tx| {
                    outbox::mark_failed(tx, &store_id, &failed, &code, Some(details), now)
                })
                .map_err(db_stop)?;
            report.failed += 1;
            Ok(())
        }
        Err(e)
            if stops(&e) || matches!(e, CallError::NotReady { .. } | CallError::PlanCap { .. }) =>
        {
            Err(e)
        }
        Err(_) => {
            let attempts = item.attempts;
            ctx.db
                .write(move |tx| outbox::reschedule(tx, &id, attempts, now))
                .map_err(db_stop)?;
            report.rescheduled += 1;
            Ok(())
        }
    }
}

/// Nothing built yet: `{}`, or a refund that only names its sale (`{"sale": …}`, its body not
/// frozen beside it yet).
fn unbuilt(item: &OutboxItem) -> bool {
    item.is_unbuilt() || (item.kind == OutboxKind::PosRefund && item.payload.get("body").is_none())
}

/// A refund's frozen row: its sale (the path names it, never the body) and its body as the exact
/// text frozen, never parsed into a `Value`.
#[derive(Deserialize)]
struct FrozenRefund<'a> {
    #[serde(borrow)]
    body: &'a RawValue,
}

/// The text of an item's body: built once, frozen before the first request, resent as is. A
/// refund's row freezes `{"sale": …, "body": <body>}`; the bytes sent are `<body>` alone.
fn body_of(
    ctx: &PushContext<'_>,
    item: &OutboxItem,
    build: impl FnOnce(&Connection) -> Result<String, AppError> + Send + 'static,
) -> Result<String, CallError> {
    let frozen = if unbuilt(item) {
        let id = item.id.clone();
        ctx.db
            .write(move |tx| {
                let text = build(tx)?;
                outbox::freeze(tx, &id, &text)?;
                Ok(text)
            })
            .map_err(db_stop)?
    } else {
        item.raw.clone()
    };
    if item.kind != OutboxKind::PosRefund {
        return Ok(frozen);
    }
    let refund: FrozenRefund<'_> = serde_json::from_str(&frozen)
        .map_err(|e| CallError::Invalid(format!("frozen refund: {:?}", e.classify())))?;
    Ok(refund.body.get().to_owned())
}

fn payload_context(ctx: &PushContext<'_>) -> Result<PayloadContext, CallError> {
    let (link, settings) = (ctx.link.clone(), ctx.settings.clone());
    ctx.db
        .read(move |c| PayloadContext::load(c, &link, &settings))
        .map_err(db_stop)
}

fn push_one_by_one(
    ctx: &PushContext<'_>,
    items: &[OutboxItem],
    report: &mut PushReport,
) -> Result<(), CallError> {
    let pctx = payload_context(ctx)?;
    for item in items {
        let result = send_one(ctx, &pctx, item);
        settle(ctx, item, result, report)?;
    }
    Ok(())
}

/// A sale, a refund, a Z or an order update: one request under the item's own key.
fn send_one(
    ctx: &PushContext<'_>,
    pctx: &PayloadContext,
    item: &OutboxItem,
) -> Result<Option<String>, CallError> {
    let store_id = ctx.link.store_id.clone();
    let (entity, p) = (item.entity_id.clone(), pctx.clone());
    let sale = item
        .payload
        .get("sale")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let (path, entity_kind) = match item.kind {
        OutboxKind::PosSale => ("/pos/sales".to_owned(), MapEntity::Sale),
        OutboxKind::Closure => ("/pos/closures".to_owned(), MapEntity::Closure),
        OutboxKind::PosRefund => {
            let (s, local_sale) = (store_id.clone(), sale.clone());
            let remote_sale = ctx
                .db
                .read(move |c| store::map_get(c, &s, MapEntity::Sale, &local_sale))
                .map_err(db_stop)?
                .map(|m| m.remote_id);
            let Some(remote_sale) = remote_sale else {
                // Its sale is sent but not mapped yet (a 409 without an id): transient, retried.
                return Err(CallError::Invalid("sale_not_mapped".into()));
            };
            (
                format!("/pos/sales/{remote_sale}/refunds"),
                MapEntity::Refund,
            )
        }
        OutboxKind::OrderClaim | OutboxKind::OrderStatus | OutboxKind::OrderCancel => {
            return send_order(ctx, item);
        }
        OutboxKind::ProductUpsert | OutboxKind::MediaUpload | OutboxKind::StockBatch => {
            return Ok(None);
        }
    };
    let kind = item.kind;
    // Serialized straight to text (amounts as `wire::money_wire` writes them), never through a
    // `Value`, frozen and sent as those bytes.
    let body = body_of(ctx, item, move |c| {
        Ok(match kind {
            OutboxKind::PosSale => serde_json::to_string(&payloads::sale_payload(
                c,
                &crate::documents::get(c, &entity)?,
                &p,
            )?)?,
            OutboxKind::PosRefund => {
                let refund = payloads::refund_payload(c, &crate::documents::get(c, &entity)?, &p)?;
                // Kept in the row so every attempt finds its sale; the path names it, the body not.
                format!(
                    "{{\"sale\":{},\"body\":{}}}",
                    serde_json::to_string(&sale)?,
                    serde_json::to_string(&refund)?
                )
            }
            _ => serde_json::to_string(&payloads::closure_payload(
                c,
                &crate::cash::closing::z_report(c, &entity)?,
                &p,
            )?)?,
        })
    })?;
    let call = Call::post(item.kind.endpoint(), &path)
        .json_body(body.into_bytes())
        .key(&item.idempotency_key);
    let created = match ctx.remote.data::<CreatedRecord>(call) {
        Err(CallError::Rejected {
            status: 409,
            code,
            details,
            ..
        }) if ALREADY_THERE.contains(&code.as_str()) => {
            let id = details
                .as_ref()
                .and_then(|d| d.get("id"))
                .map(|v| v.to_string().trim_matches('"').to_owned());
            CreatedRecord {
                id: id.unwrap_or_default(),
                stock_applied: None,
            }
        }
        other => other?,
    };
    let (local, remote_id, now) = (item.entity_id.clone(), created.id.clone(), ctx.now);
    let double = item.kind == OutboxKind::PosSale && created.stock_applied == Some(true);
    ctx.db
        .write(move |tx| {
            if !remote_id.is_empty() {
                store::map_put(
                    tx,
                    &store_id,
                    entity_kind,
                    &local,
                    &remote_id,
                    None,
                    None,
                    now,
                )?;
            }
            if double {
                // DZBuild moved its stock for this sale although the POS asked it not to: the
                // stock batches did too, so the products are set again from the local level.
                let lines: Vec<String> = crate::documents::get(tx, &local)?
                    .lines
                    .into_iter()
                    .filter_map(|l| l.product_id)
                    .collect();
                stock::request_reset(tx, &store_id, Some(&lines), now)?;
                store::raise_notice(
                    tx,
                    CloudNoticeKind::Stock,
                    "stock_double_applied",
                    Endpoint::PosSales.name(),
                    json!({}),
                    now,
                )?;
            }
            Ok(())
        })
        .map_err(db_stop)?;
    Ok(Some(created.id).filter(|id| !id.is_empty()))
}

/// Order updates carry their own request (built by `cloud::orders`): `{verb, path, body}`.
fn send_order(ctx: &PushContext<'_>, item: &OutboxItem) -> Result<Option<String>, CallError> {
    let path = item
        .payload
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let body = item.payload.get("body").cloned().unwrap_or(json!({}));
    let call = match item.payload.get("verb").and_then(Value::as_str) {
        Some("patch") => Call::patch(item.kind.endpoint(), path),
        _ => Call::post(item.kind.endpoint(), path),
    }
    .json_body(body.to_string().into_bytes())
    .key(&item.idempotency_key);
    ctx.remote.send(call).map(|_| None)
}

/// The active products online now (the last pushed status), for the plan cap.
fn active_online(conn: &Connection, store_id: &str) -> Result<i64, AppError> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM cloud_map WHERE store_id = ?1 AND entity_type = 'product'
           AND json_extract(last_payload, '$.status') = 'active'",
        [store_id],
        |r| r.get(0),
    )?)
}

struct Built {
    item: OutboxItem,
    product: payloads::ProductItem,
    hash: String,
    /// Active online for the first time: what the store's plan may refuse.
    newly_active: bool,
}

/// Builds each product from its current state, skips the unchanged ones (same hash as last
/// pushed), drafts the new actives beyond the plan's cap, and sends the rest [`PRODUCT_BATCH`]
/// at a time under [`products_key`].
fn push_products(
    ctx: &PushContext<'_>,
    items: &[OutboxItem],
    report: &mut PushReport,
) -> Result<(), CallError> {
    let pctx = payload_context(ctx)?;
    let store_id = ctx.link.store_id.clone();
    let cap = ctx.link.active_products_cap;
    for chunk in items.chunks(PRODUCT_BATCH) {
        let (chunk_items, s, p, settings, now) = (
            chunk.to_vec(),
            store_id.clone(),
            pctx.clone(),
            ctx.settings.clone(),
            ctx.now,
        );
        let (built, unchanged, drafted): (Vec<Built>, usize, usize) = ctx
            .db
            .write(move |tx| {
                let mut active = active_online(tx, &s)?;
                let (mut out, mut unchanged, mut drafted) = (Vec::new(), 0, 0);
                for item in chunk_items {
                    let Ok(product) = crate::catalog::products::get(tx, &item.entity_id) else {
                        outbox::skip(tx, &item.id, "product_gone")?;
                        continue;
                    };
                    let Some(mut status) = publish_decision(tx, &s, &product, &settings)? else {
                        outbox::mark_sent(tx, &item.id, None, now)?;
                        unchanged += 1;
                        continue;
                    };
                    let mapped = store::map_get(tx, &s, MapEntity::Product, &product.id)?;
                    let was_active = mapped
                        .as_ref()
                        .and_then(|m| m.last_payload.as_ref())
                        .and_then(|p| p.get("status"))
                        .and_then(Value::as_str)
                        == Some("active");
                    let mut newly_active = false;
                    if status == OnlineStatus::Active && !was_active {
                        if cap.is_some_and(|c| active >= c) {
                            status = OnlineStatus::Draft;
                            drafted += 1;
                            tx.execute(
                                "INSERT INTO cloud_publish (product_id, publish, online_status, updated_at)
                                 VALUES (?1, 1, 'draft', ?2)
                                 ON CONFLICT (product_id) DO UPDATE SET online_status = 'draft',
                                     updated_at = excluded.updated_at",
                                rusqlite::params![product.id, now],
                            )?;
                        } else {
                            active += 1;
                            newly_active = true;
                        }
                    }
                    let built = payloads::product_item(tx, &product, status, &p)?;
                    let hash = payloads::body_hash(&built)?;
                    if mapped.as_ref().and_then(|m| m.last_hash.as_deref()) == Some(hash.as_str()) {
                        outbox::mark_sent(tx, &item.id, None, now)?;
                        unchanged += 1;
                        continue;
                    }
                    out.push(Built {
                        item,
                        product: built,
                        hash,
                        newly_active,
                    });
                }
                if drafted > 0 {
                    store::raise_notice(
                        tx,
                        CloudNoticeKind::PlanCap,
                        "cloud_plan_cap",
                        "",
                        json!({ "cap": cap, "drafted": drafted }),
                        now,
                    )?;
                }
                Ok((out, unchanged, drafted))
            })
            .map_err(db_stop)?;
        report.unchanged += unchanged;
        report.drafted += drafted;
        if built.is_empty() {
            continue;
        }
        let mut built = built;
        let mut answered = send_products(ctx, &built);
        if let Err(CallError::PlanCap { code }) = &answered
            && built.iter().any(|b| b.newly_active)
        {
            // The store's plan refuses more active products: the new ones go as drafts (the
            // owner reads why once), the batch once more under its new key.
            let drafts = to_drafts(&mut built)?;
            report.drafted += drafts.len();
            let (code, now) = (code.clone(), ctx.now);
            ctx.db
                .write(move |tx| {
                    for product in &drafts {
                        tx.execute(
                            "INSERT INTO cloud_publish (product_id, publish, online_status, updated_at)
                             VALUES (?1, 1, 'draft', ?2)
                             ON CONFLICT (product_id) DO UPDATE SET online_status = 'draft',
                                 updated_at = excluded.updated_at",
                            rusqlite::params![product, now],
                        )?;
                    }
                    store::raise_notice(
                        tx,
                        CloudNoticeKind::PlanCap,
                        "cloud_plan_cap",
                        "",
                        json!({ "code": code, "drafted": drafts.len() }),
                        now,
                    )
                    .map(drop)
                })
                .map_err(db_stop)?;
            answered = send_products(ctx, &built);
        }
        match answered {
            Ok(answer) => {
                let (s, now) = (store_id.clone(), ctx.now);
                let (sent, failed, later) = ctx
                    .db
                    .write(move |tx| apply_product_answer(tx, &s, &built, &answer, now))
                    .map_err(db_stop)?;
                report.sent += sent;
                report.failed += failed;
                report.rescheduled += later;
            }
            // the whole kind waits, as for an endpoint not shipped (`push_due`)
            Err(e @ CallError::PlanCap { .. }) => return Err(e),
            Err(e) => {
                for b in &built {
                    settle(ctx, &b.item, Err(e.clone()), report)?;
                }
            }
        }
    }
    Ok(())
}

/// One `POST /v1/products/batch` of `built`, its body serialized straight to bytes (the prices as
/// `wire::money_wire` writes them) under [`products_key`].
fn send_products(ctx: &PushContext<'_>, built: &[Built]) -> Result<BatchAnswer, CallError> {
    let body = serde_json::to_vec(&ProductsBody {
        items: built.iter().map(|b| &b.product).collect(),
    })
    .map_err(|e| CallError::Invalid(format!("products body: {:?}", e.classify())))?;
    let key = products_key(&body, built);
    let call = Call::post(Endpoint::ProductsBatch, "/products/batch")
        .json_body(body)
        .key(&key);
    ctx.remote.data::<BatchAnswer>(call)
}

/// The newly active products of a batch become drafts (their hash follows): the ids changed.
fn to_drafts(built: &mut [Built]) -> Result<Vec<String>, CallError> {
    let mut drafted = Vec::new();
    for b in built.iter_mut().filter(|b| b.newly_active) {
        b.product.status = OnlineStatus::Draft.as_str();
        b.hash = payloads::body_hash(&b.product)
            .map_err(|e| CallError::Invalid(format!("products hash: {}", e.code())))?;
        b.newly_active = false;
        drafted.push(b.product.external_id.clone());
    }
    Ok(drafted)
}

/// `POST /v1/products/batch`'s body.
#[derive(Serialize)]
struct ProductsBody<'a> {
    items: Vec<&'a payloads::ProductItem>,
}

/// `products:<first 32 hex of SHA-256(body ‖ the rows' own keys, sorted)>`. The body alone
/// repeats: a product edited A → B → A within DZBuild's 24 h window sends a body DZBuild already
/// holds, which it answers from its store and never applies (the storefront keeps B), and an
/// owner's retry would get the stored refusal back. Every edit is its own row
/// (`product:<row id>`) and a retry renames it (`…:r<n>`), so both take a new key, while a resend
/// of the same rows after a transient failure keeps its key.
fn products_key(body: &[u8], built: &[Built]) -> String {
    let mut rows: Vec<&str> = built
        .iter()
        .map(|b| b.item.idempotency_key.as_str())
        .collect();
    rows.sort_unstable();
    let mut hasher = Sha256::new();
    hasher.update(body);
    for row in rows {
        hasher.update(b"\n");
        hasher.update(row.as_bytes());
    }
    let digest = hex::encode(hasher.finalize().as_slice());
    format!("products:{}", digest.get(..32).unwrap_or(&digest))
}

/// Each product's answer by its `external_id`: mapped and sent, failed for the owner, or (no
/// answer of its own) later. By position only when no answer names a product and there is one
/// answer per product: a missing or reordered answer never gives a product another one's remote
/// id or refusal. Returns (sent, failed, rescheduled).
fn apply_product_answer(
    tx: &Transaction<'_>,
    store_id: &str,
    built: &[Built],
    answer: &BatchAnswer,
    now: i64,
) -> Result<(usize, usize, usize), AppError> {
    let (mut sent, mut failed, mut later) = (0, 0, 0);
    let by_position =
        answer.items.len() == built.len() && answer.items.iter().all(|a| a.external_id.is_none());
    for (index, b) in built.iter().enumerate() {
        let found = if by_position {
            answer.items.get(index)
        } else {
            answer
                .items
                .iter()
                .find(|a| a.external_id.as_deref() == Some(b.product.external_id.as_str()))
        };
        match found {
            Some(a) if a.error.is_none() => {
                let remote_id = a.id.as_ref().map(|i| i.0.clone()).unwrap_or_default();
                let snapshot: ProductSnapshot = b.product.snapshot();
                let snapshot = serde_json::to_value(snapshot)?;
                store::map_put(
                    tx,
                    store_id,
                    MapEntity::Product,
                    &b.product.external_id,
                    &remote_id,
                    Some(&snapshot),
                    Some(&b.hash),
                    now,
                )?;
                outbox::mark_sent(tx, &b.item.id, Some(&remote_id), now)?;
                apply_variant_answers(
                    tx,
                    store_id,
                    b,
                    Some(&remote_id),
                    a.variants.as_deref(),
                    now,
                )?;
                sent += 1;
            }
            Some(a) => {
                apply_variant_answers(tx, store_id, b, None, a.variants.as_deref(), now)?;
                let error = a.error.clone();
                let code = error
                    .as_ref()
                    .map_or("rejected", |e| e.code.as_str())
                    .to_owned();
                let details = error.map(|e| json!({ "message": e.message, "details": e.details }));
                outbox::mark_failed(tx, store_id, &b.item, &code, details, now)?;
                failed += 1;
            }
            None => {
                outbox::reschedule(tx, &b.item.id, b.item.attempts, now)?;
                later += 1;
            }
        }
    }
    Ok((sent, failed, later))
}

/// A family's children as DZBuild answered them. Refused ones raise one notice per family; once
/// the family is applied (`remote_id`), each child it holds is mapped by its key and a child no
/// longer sent loses its row (the family online was replaced without it).
fn apply_variant_answers(
    tx: &Transaction<'_>,
    store_id: &str,
    b: &Built,
    remote_id: Option<&str>,
    answers: Option<&[VariantAnswer]>,
    now: i64,
) -> Result<(), AppError> {
    let Some(sent) = &b.product.variants else {
        return Ok(());
    };
    let answers = answers.unwrap_or_default();
    let refused = |v: &&VariantAnswer| v.error.is_some() || v.status.as_deref() == Some("error");
    let rejected: Vec<&VariantAnswer> = answers.iter().filter(refused).collect();
    if let Some(first) = rejected.first() {
        store::raise_notice(
            tx,
            CloudNoticeKind::FailedItems,
            "cloud_variant_refused",
            &b.product.external_id,
            json!({
                "product": b.product.name,
                "count": rejected.len(),
                "reason": first.error.as_ref().map(|e| e.code.as_str()),
            }),
            now,
        )?;
    }
    let Some(remote_id) = remote_id else {
        return Ok(());
    };
    let is_sent = |id: &str| sent.items.iter().any(|i| i.external_id == id);
    for v in answers.iter().filter(|v| !refused(v)) {
        // only a child this item sent: an answer never maps another product
        let (Some(local), Some(key)) = (v.external_id.as_deref(), v.key.as_deref()) else {
            continue;
        };
        if is_sent(local) {
            variants::map_put(
                tx,
                store_id,
                &VariantMapRow {
                    local_id: local.to_owned(),
                    parent_id: b.product.external_id.clone(),
                    remote_product_id: remote_id.to_owned(),
                    variant_key: key.to_owned(),
                    price_online: v.price_online.map(|p| p.0),
                    updated_at: now,
                },
            )?;
        }
    }
    for row in variants::map_children(tx, store_id, &b.product.external_id)? {
        if !is_sent(&row.local_id) {
            variants::map_forget(tx, store_id, &row.local_id)?;
        }
    }
    Ok(())
}

/// Each stock batch as frozen at derive time, under `stock:<row id>`. Every item's answer by its
/// `ref`, else by position; no answer means applied. A refused item (or a 4xx on the whole
/// batch) releases its share and asks the next derive for a `set`.
fn push_stock(
    ctx: &PushContext<'_>,
    items: &[OutboxItem],
    report: &mut PushReport,
) -> Result<(), CallError> {
    for item in items {
        let batch: Vec<StockItem> = item
            .payload
            .get("items")
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        let call = Call::post(
            Endpoint::InventoryAdjustments,
            "/inventory/adjustments/batch",
        )
        .json_body(item.payload.to_string().into_bytes())
        .key(&item.idempotency_key);
        let (store_id, now, row) = (ctx.link.store_id.clone(), ctx.now, item.clone());
        match ctx.remote.data::<BatchAnswer>(call) {
            Ok(answer) => {
                let outcomes: Vec<ItemOutcome> = batch
                    .iter()
                    .enumerate()
                    .map(|(index, i)| {
                        let answer_for = answer
                            .items
                            .iter()
                            .find(|a| a.reference.as_deref() == Some(i.reference.as_str()))
                            .or_else(|| answer.items.get(index));
                        let refused = answer_for.is_some_and(|a| {
                            a.error.is_some() || a.status.as_deref() == Some("error")
                        });
                        if refused {
                            ItemOutcome::Refused
                        } else {
                            ItemOutcome::Applied
                        }
                    })
                    .collect();
                ctx.db
                    .write(move |tx| {
                        stock::on_answer(tx, &store_id, &batch, &outcomes, now)?;
                        outbox::mark_sent(tx, &row.id, None, now)
                    })
                    .map_err(db_stop)?;
                report.sent += 1;
            }
            Err(CallError::Rejected { status, code, .. }) => {
                ctx.db
                    .write(move |tx| {
                        stock::release(tx, &store_id, &batch, now)?;
                        outbox::mark_failed(
                            tx,
                            &store_id,
                            &row,
                            &code,
                            Some(json!({ "status": status })),
                            now,
                        )
                    })
                    .map_err(db_stop)?;
                report.failed += 1;
            }
            Err(e) => settle(ctx, item, Err(e), report)?,
        }
    }
    Ok(())
}

/// Photo upload in three steps (research 06 G9): ask for a ticket, `PUT` the bytes to the
/// pre-signed URL (no bearer, 10 a minute per store), attach the media to the product. A photo
/// waits for its product's mapping; a photo whose file is gone has nothing to send.
fn push_media(
    ctx: &PushContext<'_>,
    items: &[OutboxItem],
    report: &mut PushReport,
) -> Result<(), CallError> {
    let Some(media_dir) = ctx.media_dir else {
        return Ok(());
    };
    for item in items {
        let store_id = ctx.link.store_id.clone();
        let file = item
            .payload
            .get("image")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let (s, product_id) = (store_id.clone(), item.entity_id.clone());
        let product_remote = ctx
            .db
            .read(move |c| store::map_get(c, &s, MapEntity::Product, &product_id))
            .map_err(db_stop)?
            .map(|m| m.remote_id);
        let Some(product_remote) = product_remote else {
            let (id, attempts, now) = (item.id.clone(), item.attempts, ctx.now);
            ctx.db
                .write(move |tx| outbox::reschedule(tx, &id, attempts, now))
                .map_err(db_stop)?;
            report.rescheduled += 1;
            continue;
        };
        // Only a name the catalogue writes (`products/<id>-<n>.jpg`): no path leaves the folder.
        let bytes = crate::catalog::images::is_valid_name(&file)
            .then(|| std::fs::read(media_dir.join(&file)).ok())
            .flatten();
        let Some(bytes) = bytes else {
            let id = item.id.clone();
            ctx.db
                .write(move |tx| outbox::skip(tx, &id, "media_missing"))
                .map_err(db_stop)?;
            continue;
        };
        let filename = file
            .strip_prefix(crate::catalog::images::DIR)
            .and_then(|rest| rest.strip_prefix('/'))
            .unwrap_or(&file);
        let sha = hex::encode(Sha256::digest(&bytes).as_slice());
        let ask = json!({ "filename": filename, "content_type": "image/jpeg", "size": bytes.len(), "sha256": sha });
        let ticket_for = |key: &str| {
            ctx.remote.data::<MediaTicket>(
                Call::post(Endpoint::MediaUploads, "/media/uploads")
                    .json_body(ask.to_string().into_bytes())
                    .key(key),
            )
        };
        let result = ticket_for(&item.idempotency_key).and_then(|ticket| {
            let ticket = match ctx.remote.http().send(&upload_call(&ticket, bytes.clone())) {
                Ok(_) => ticket,
                // The pre-signed URL expired: DZBuild replays the first ticket under the
                // item's key, so a fresh ticket is asked once, under a key of this attempt.
                Err(CallError::Rejected {
                    status: 400 | 403, ..
                }) => {
                    let fresh = ticket_for(&fresh_ticket_key(item))?;
                    ctx.remote
                        .http()
                        .send(&upload_call(&fresh, bytes))
                        .map_err(upload_trouble)?;
                    fresh
                }
                Err(e) => return Err(upload_trouble(e)),
            };
            let attach = json!({ "media_id": ticket.media_id, "position": 0 });
            ctx.remote.send(
                Call::post(
                    Endpoint::ProductImages,
                    &format!("/products/{product_remote}/images"),
                )
                .json_body(attach.to_string().into_bytes())
                .key(&super::outbox_key(
                    "media-attach",
                    &format!("{}:{}", item.entity_id, ticket.media_id),
                )),
            )?;
            Ok(ticket.media_id)
        });
        if let Ok(media_id) = &result {
            let (s, local, media_id, file, now) = (
                store_id.clone(),
                item.entity_id.clone(),
                media_id.clone(),
                file.clone(),
                ctx.now,
            );
            ctx.db
                .write(move |tx| {
                    store::map_put(
                        tx,
                        &s,
                        MapEntity::Media,
                        &local,
                        &media_id,
                        Some(&json!({ "file": file })),
                        None,
                        now,
                    )
                })
                .map_err(db_stop)?;
        }
        settle(ctx, item, result.map(Some), report)?;
    }
    Ok(())
}

/// The key of a fresh upload ticket for `item` (its pre-signed URL expired): the item's key and
/// the attempt, so each attempt asks DZBuild for a new URL once.
fn fresh_ticket_key(item: &OutboxItem) -> String {
    format!(
        "{}:t{}",
        item.idempotency_key,
        item.attempts.saturating_add(1)
    )
}

/// The pre-signed `PUT` goes to the storage host, not to DZBuild's API: a dead uplink, a timeout
/// on a large photo or a refused signature (401) there is this photo's trouble, rescheduled with
/// backoff like any transient failure. It never stops the cycle, or one photo would hold back
/// every sale, refund and Z behind it; only DZBuild's own endpoints (ticket, attach) do.
fn upload_trouble(e: CallError) -> CallError {
    if stops(&e) || matches!(e, CallError::NotReady { .. }) {
        CallError::Invalid(format!("upload: {}", e.code()))
    } else {
        e
    }
}

/// The pre-signed `PUT`: an absolute URL on the storage host, without the bearer or a key, on
/// the media lane (10 a minute per store).
fn upload_call(ticket: &MediaTicket, bytes: Vec<u8>) -> Call {
    Call::put(Endpoint::Upload, "")
        .absolute(&ticket.upload_url)
        .bytes(bytes, "image/jpeg")
        .lane(Lane::Media)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::derive::derive;
    use crate::cloud::http::ManualClock;
    use crate::cloud::store::cloud_settings;
    use crate::cloud::testkit::{
        MockDz, UNREACHABLE, created, header, linked_shop, ok, outbox_rows, publish, remote_at,
        remote_for_test,
    };
    use crate::money::{Money, Qty};
    use crate::payments::PaymentInput;
    use crate::sales::{Cart, CartLine, SaleInput, create_sale};
    use crate::testkit::{NOON, Shop};
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use wiremock::matchers::{body_partial_json, method, path, path_regex};
    use wiremock::{Mock, Request, Respond, ResponseTemplate};

    fn derive_now(shop: &Shop, link: &CloudLink) {
        let link = link.clone();
        shop.db
            .write(move |tx| {
                let settings = cloud_settings(tx)?;
                derive(tx, &link, &settings, NOON).map(drop)
            })
            .unwrap();
    }

    fn push_at(
        shop: &Shop,
        link: &CloudLink,
        remote: &Remote,
        now: i64,
    ) -> Result<PushReport, CallError> {
        push_with(shop, link, remote, now, None)
    }

    fn push_with(
        shop: &Shop,
        link: &CloudLink,
        remote: &Remote,
        now: i64,
        media_dir: Option<&Path>,
    ) -> Result<PushReport, CallError> {
        let settings = shop.db.read(cloud_settings).unwrap();
        push_due(&PushContext {
            remote,
            db: &shop.db,
            link,
            settings: &settings,
            media_dir,
            now,
        })
    }

    /// A real photo of `product_id` under `media` (`products/<id>-1.jpg`), set on the product.
    fn photo(shop: &Shop, product_id: &str, media: &Path) -> (String, Vec<u8>) {
        let name = crate::catalog::images::file_name(product_id, 1);
        let bytes = b"\xFF\xD8\xFF\xE0 a small jpeg \xFF\xD9".to_vec();
        std::fs::create_dir_all(media.join(crate::catalog::images::DIR)).unwrap();
        std::fs::write(media.join(&name), &bytes).unwrap();
        let (actor, id, set) = (shop.owner.actor(), product_id.to_owned(), name.clone());
        shop.db
            .write(move |tx| crate::catalog::images::set(tx, &actor, &id, Some(&set)).map(drop))
            .unwrap();
        (name, bytes)
    }

    fn mount_ticket(dz: &MockDz, upload_url: &str) {
        dz.mount(
            Mock::given(method("POST"))
                .and(path("/v1/media/uploads"))
                .respond_with(created(
                    json!({ "media_id": "m1", "upload_url": upload_url }),
                ))
                .expect(1),
        );
    }

    fn media_row(shop: &Shop, image: &str) -> (String, Option<String>, i64, i64) {
        let image = image.to_owned();
        shop.db
            .read(move |c| {
                Ok(c.query_row(
                    "SELECT status, last_error, attempts, next_attempt_at FROM sync_outbox
                     WHERE kind = 'media_upload' AND json_extract(payload, '$.image') = ?1",
                    [image],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )?)
            })
            .unwrap()
    }

    fn batch_keys(dz: &MockDz) -> Vec<String> {
        dz.requests()
            .iter()
            .filter(|r| r.url.path() == "/v1/products/batch")
            .map(|r| header(r, "idempotency-key").unwrap())
            .collect()
    }

    /// DZBuild's 24 h idempotency store in front of `POST /v1/products/batch`: a new key applies
    /// the batch to the storefront; a key seen before replays its first answer and applies nothing.
    #[derive(Clone, Default)]
    struct IdempotentBatches {
        seen: Arc<Mutex<HashMap<String, ResponseTemplate>>>,
        storefront: Arc<Mutex<Vec<Value>>>,
    }

    impl Respond for IdempotentBatches {
        fn respond(&self, request: &Request) -> ResponseTemplate {
            let key = header(request, "idempotency-key").unwrap_or_default();
            let mut seen = self.seen.lock().unwrap();
            if let Some(first) = seen.get(&key) {
                return first.clone().insert_header("Idempotency-Replay", "1");
            }
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            let items = body["items"].as_array().cloned().unwrap_or_default();
            let answers: Vec<Value> = items
                .iter()
                .map(|i| json!({ "external_id": i["external_id"], "id": 88121, "status": i["status"], "error": null }))
                .collect();
            *self.storefront.lock().unwrap() = items;
            let answer = ok(json!({ "items": answers }));
            seen.insert(key, answer.clone());
            answer
        }
    }

    fn sell_one(shop: &Shop, product_id: &str) -> String {
        let till = shop.till();
        let input = SaleInput {
            cart: Cart::new(
                crate::lang::Lang::Fr,
                vec![CartLine::product(product_id, Qty(1_000))],
            ),
            payments: vec![PaymentInput::cash(Money(10_000))],
        };
        shop.db
            .write(move |tx| create_sale(tx, &till, input))
            .unwrap()
            .document
            .id
    }

    /// One answer naming no product: taken by position for a one-product batch.
    fn mount_products_ok(dz: &MockDz) {
        dz.mount(
            Mock::given(path("/v1/products/batch")).respond_with(ok(json!({
                "items": [{ "id": 88121, "status": "active", "error": null }]
            }))),
        );
    }

    #[test]
    fn a_retried_sale_reuses_its_key_and_a_4xx_is_never_retried() {
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        shop.stock_in(&milk.id, 10_000);
        shop.open_till(0);
        let first = sell_one(&shop, &milk.id);
        derive_now(&shop, &link);
        dz.mount(
            Mock::given(path("/v1/pos/sales"))
                .respond_with(ResponseTemplate::new(503))
                .up_to_n_times(4),
        );
        dz.mount(
            Mock::given(method("POST"))
                .and(path("/v1/pos/sales"))
                .and(body_partial_json(json!({ "external_id": first })))
                .respond_with(created(
                    json!({ "id": 99120, "external_id": first, "stock_applied": false }),
                )),
        );
        let clock = ManualClock::at(NOON);
        let remote = remote_for_test(&dz, &link, clock.clone());
        let report = push_at(&shop, &link, &remote, NOON).unwrap();
        assert_eq!(
            (report.sent, report.rescheduled),
            (0, 1),
            "4 attempts in the transport, then later"
        );
        let later = NOON + crate::cloud::outbox::backoff_ms(1);
        assert_eq!(push_at(&shop, &link, &remote, later).unwrap().sent, 1);
        let keys: Vec<String> = dz
            .requests()
            .iter()
            .filter(|r| r.url.path() == "/v1/pos/sales")
            .map(|r| header(r, "idempotency-key").unwrap())
            .collect();
        assert_eq!(keys.len(), 5);
        assert!(
            keys.iter().all(|k| *k == format!("sale:{first}")),
            "one key for every retry: {keys:?}"
        );
        let mapped = shop
            .db
            .read(move |c| {
                crate::cloud::store::map_get(c, "141", crate::cloud::store::MapEntity::Sale, &first)
            })
            .unwrap()
            .unwrap();
        assert_eq!(mapped.remote_id, "99120");

        let second = sell_one(&shop, &milk.id);
        derive_now(&shop, &link);
        dz.rt.block_on(dz.server.reset());
        dz.mount(
            Mock::given(path("/v1/pos/sales"))
                .respond_with(
                    ResponseTemplate::new(422)
                        .set_body_json(json!({ "error": { "code": "validation_failed" } })),
                )
                .expect(1),
        );
        let report = push_at(&shop, &link, &remote, later + 1).unwrap();
        assert_eq!(report.failed, 1);
        push_at(&shop, &link, &remote, later + 10 * 3_600_000).unwrap();
        dz.verify(); // exactly one request: a 4xx waits for the owner
        assert!(outbox_rows(&shop).contains(&("pos_sale".into(), second, "failed".into())));
    }

    /// Every amount reaches DZBuild as the exact decimal string `wire::money_wire` writes, from
    /// a body serialized once and frozen as text: 0.07 DA and 12 345 678.91 DA in the products, the
    /// sales, their refunds and the Z, never through a float.
    #[test]
    fn amounts_reach_dzbuild_as_exact_decimal_strings() {
        use crate::payments::PaymentMethod;
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        let cent = shop.product("Bonbon", Money(7), &shop.tva19);
        let big = shop.product("Machine", Money(1_234_567_891), &shop.tva19);
        for p in [&cent, &big] {
            publish(&shop, &p.id);
            shop.stock_in(&p.id, 5_000);
        }
        shop.open_till(0);
        let sell = |product: &str, payment: PaymentInput| {
            let till = shop.till();
            let input = SaleInput {
                cart: Cart::new(
                    crate::lang::Lang::Fr,
                    vec![CartLine::product(product, Qty(1_000))],
                ),
                payments: vec![payment],
            };
            shop.db
                .write(move |tx| create_sale(tx, &till, input))
                .unwrap()
                .document
                .id
        };
        let small = sell(&cent.id, PaymentInput::cash(Money(7)));
        let large = sell(
            &big.id,
            PaymentInput {
                method: PaymentMethod::CardCib,
                amount: Money(1_234_567_891),
                reference: Some("AUTH42".into()),
                cheque_bank: None,
                cheque_date: None,
            },
        );
        crate::cloud::testkit::return_all(&shop, &small);
        let ctx = crate::till::TillContext::new(shop.owner.clone(), shop.pack.clone(), NOON);
        let original = large.clone();
        shop.db
            .write(move |tx| {
                let lines = crate::returns::returnable(tx, &original)?
                    .into_iter()
                    .map(|l| crate::returns::ReturnLineInput {
                        origin_line_id: Some(l.line.id.clone()),
                        product_id: None,
                        qty: l.remaining,
                        unit_price: None,
                        restock: true,
                        reason: crate::returns::ReturnReason::ChangedMind,
                        note: None,
                    })
                    .collect();
                crate::returns::create_return(
                    tx,
                    &ctx,
                    crate::returns::ReturnInput {
                        original_id: Some(original.clone()),
                        lines,
                        refund_method: PaymentMethod::CardCib,
                        refund_reference: Some("AUTH43".into()),
                        lang: crate::lang::Lang::Fr,
                        note: None,
                    },
                )
                .map(drop)
            })
            .unwrap();
        let till = shop.owner_till();
        shop.db
            .write(move |tx| {
                crate::cash::closing::close_session(
                    tx,
                    &till,
                    crate::cash::closing::CloseInput {
                        counts: vec![],
                        notes: None,
                    },
                )
                .map(drop)
            })
            .unwrap();
        derive_now(&shop, &link);
        dz.mount(
            Mock::given(path("/v1/products/batch")).respond_with(ok(json!({
                "items": [
                    { "id": 1, "status": "active", "error": null },
                    { "id": 2, "status": "active", "error": null }
                ]
            }))),
        );
        dz.mount(
            Mock::given(path("/v1/inventory/adjustments/batch"))
                .respond_with(ok(json!({ "items": [] }))),
        );
        let counter = Arc::new(Mutex::new(99_120));
        dz.mount(
            Mock::given(path("/v1/pos/sales")).respond_with(move |_: &Request| {
                let mut next = counter.lock().unwrap();
                *next += 1;
                created(json!({ "id": *next, "stock_applied": false }))
            }),
        );
        dz.mount(
            Mock::given(path_regex(r"^/v1/pos/sales/\d+/refunds$"))
                .respond_with(created(json!({ "id": 5 }))),
        );
        dz.mount(Mock::given(path("/v1/pos/closures")).respond_with(created(json!({ "id": 3 }))));
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        for minute in 0..4 {
            push_at(&shop, &link, &remote, NOON + minute * 60_000).unwrap();
        }
        let bodies = |p: &str| -> Vec<String> {
            dz.requests()
                .iter()
                .filter(|r| {
                    r.url.path() == p || (p == "refunds" && r.url.path().ends_with("/refunds"))
                })
                .map(|r| String::from_utf8(r.body.clone()).unwrap())
                .collect()
        };
        let products = bodies("/v1/products/batch").join("\n");
        assert!(products.contains(r#""price":"0.07""#), "{products}");
        assert!(products.contains(r#""price":"12345678.91""#), "{products}");
        let sales = bodies("/v1/pos/sales").join("\n");
        assert!(sales.contains(r#""unit_price":"0.07""#), "{sales}");
        assert!(sales.contains(r#""unit_price":"12345678.91""#), "{sales}");
        let refunds = bodies("refunds");
        assert_eq!(refunds.len(), 2, "both refunds sent");
        let refunds = refunds.join("\n");
        assert!(refunds.contains(r#""0.07""#), "{refunds}");
        assert!(refunds.contains(r#""12345678.91""#), "{refunds}");
        assert!(
            !refunds.contains(r#""sale":"#),
            "the path names the sale, the body not"
        );
        let closures = bodies("/v1/pos/closures").join("\n");
        assert!(closures.contains(r#""12345678.98""#), "{closures}");
        for body in [&products, &sales, &refunds, &closures] {
            assert!(
                !body.contains("12345678.91000") && !body.contains("e+") && !body.contains("E+"),
                "no float on the wire: {body}"
            );
        }
    }

    /// DZBuild's plan refuses more active products (402 `product_limit_reached`) though the POS
    /// knew no cap: the batch's new products go as drafts, the owner is told once.
    #[test]
    fn a_plan_refusing_new_products_drafts_them_and_says_so_once() {
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        let a = shop.product("A", Money(1_000), &shop.tva19).id;
        publish(&shop, &a);
        derive_now(&shop, &link);
        dz.mount(
            Mock::given(path("/v1/products/batch"))
                .and(body_partial_json(
                    json!({ "items": [{ "status": "active" }] }),
                ))
                .respond_with(
                    ResponseTemplate::new(402)
                        .set_body_json(json!({ "error": { "code": "product_limit_reached" } })),
                )
                .expect(1),
        );
        dz.mount(
            Mock::given(path("/v1/products/batch"))
                .and(body_partial_json(
                    json!({ "items": [{ "status": "draft" }] }),
                ))
                .respond_with(ok(
                    json!({ "items": [{ "external_id": a, "id": 1, "status": "draft" }] }),
                ))
                .expect(1),
        );
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        let report = push_at(&shop, &link, &remote, NOON).unwrap();
        dz.verify();
        assert_eq!((report.sent, report.drafted), (1, 1));
        let notices = shop
            .db
            .read(|c| crate::cloud::store::notices(c, false))
            .unwrap();
        let caps: Vec<_> = notices
            .iter()
            .filter(|n| n.code == "cloud_plan_cap")
            .collect();
        assert_eq!(caps.len(), 1, "{notices:?}");
        assert_eq!(caps[0].details["code"], "product_limit_reached");
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM cloud_publish WHERE online_status = 'draft'"),
            1
        );
    }

    /// A plan without POS sales (403 `app_plan_required`): the sales wait a day like an endpoint
    /// not shipped, with one notice naming the endpoint, never rescheduled in silence.
    #[test]
    fn a_plan_refusing_a_kind_defers_it_with_one_notice() {
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        shop.stock_in(&milk.id, 5_000);
        shop.open_till(0);
        sell_one(&shop, &milk.id);
        derive_now(&shop, &link);
        dz.mount(
            Mock::given(path("/v1/inventory/adjustments/batch"))
                .respond_with(ok(json!({ "items": [] }))),
        );
        dz.mount(
            Mock::given(path("/v1/pos/sales"))
                .respond_with(
                    ResponseTemplate::new(403)
                        .set_body_json(json!({ "error": { "code": "app_plan_required" } })),
                )
                .expect(1),
        );
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        let report = push_at(&shop, &link, &remote, NOON).unwrap();
        assert_eq!(report.plan_capped, [Endpoint::PosSales]);
        push_at(&shop, &link, &remote, NOON + 3_600_000).unwrap();
        dz.verify(); // not asked again the same day
        let notices = shop
            .db
            .read(|c| crate::cloud::store::notices(c, false))
            .unwrap();
        assert_eq!(notices.len(), 1, "{notices:?}");
        assert_eq!(
            (notices[0].code.as_str(), notices[0].subject.as_str()),
            ("cloud_plan_cap", "pos.sales")
        );
        assert!(
            outbox_rows(&shop)
                .iter()
                .any(|r| r.0 == "pos_sale" && r.2 == "pending"),
            "the sale waits"
        );
    }

    #[test]
    fn a_not_ready_endpoint_keeps_its_items_and_raises_one_notice() {
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        shop.open_till(0);
        let till = shop.till();
        shop.db
            .write(move |tx| {
                crate::cash::closing::close_session(
                    tx,
                    &till,
                    crate::cash::closing::CloseInput {
                        counts: vec![],
                        notes: None,
                    },
                )
            })
            .unwrap();
        derive_now(&shop, &link);
        dz.mount(
            Mock::given(path("/v1/pos/closures"))
                .respond_with(ResponseTemplate::new(404))
                .expect(1),
        );
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        let report = push_at(&shop, &link, &remote, NOON).unwrap();
        assert_eq!(report.not_ready, [Endpoint::PosClosures]);
        push_at(&shop, &link, &remote, NOON + 60_000).unwrap();
        dz.verify(); // not asked again the same day
        assert!(
            outbox_rows(&shop)
                .iter()
                .any(|r| r.0 == "closure" && r.2 == "pending"),
            "the Z waits"
        );
        let notices = shop
            .db
            .read(|c| crate::cloud::store::notices(c, false))
            .unwrap();
        assert_eq!(notices.len(), 1);
        assert_eq!(
            (notices[0].code.as_str(), notices[0].subject.as_str()),
            ("cloud_not_ready", "pos.closures")
        );
    }

    /// Answers are matched by `external_id`: one left out is sent again later, reordered ones
    /// keep their own remote id, and a list of errors only fails those it names.
    #[test]
    fn product_answers_are_matched_by_external_id_never_by_position() {
        let remote_of = |shop: &Shop, link: &CloudLink, id: &str| {
            let (store, id) = (link.store_id.clone(), id.to_owned());
            shop.db
                .read(move |c| crate::cloud::store::map_get(c, &store, MapEntity::Product, &id))
                .unwrap()
                .map(|m| m.remote_id)
        };
        let state_of = |shop: &Shop, id: &str| -> String {
            let id = id.to_owned();
            shop.db
                .read(move |c| {
                    Ok(c.query_row(
                        "SELECT status FROM sync_outbox WHERE kind = 'product_upsert' AND entity_id = ?1
                         ORDER BY created_at DESC LIMIT 1",
                        [&id],
                        |r| r.get(0),
                    )?)
                })
                .unwrap()
        };
        for case in ["omits", "reorders", "errors only"] {
            let dz = MockDz::start();
            let (shop, link) = linked_shop();
            let a = shop.product("A", Money(1_000), &shop.tva19).id;
            let b = shop.product("B", Money(1_000), &shop.tva19).id;
            publish(&shop, &a);
            publish(&shop, &b);
            derive_now(&shop, &link);
            let items = match case {
                "omits" => {
                    json!([{ "external_id": b, "id": 2, "status": "active", "error": null }])
                }
                "reorders" => json!([
                    { "external_id": b, "id": 2, "status": "active", "error": null },
                    { "external_id": a, "id": 1, "status": "active", "error": null }
                ]),
                _ => {
                    json!([{ "external_id": b, "error": { "code": "invalid_price", "message": "no" } }])
                }
            };
            dz.mount(
                Mock::given(path("/v1/products/batch"))
                    .respond_with(ok(json!({ "items": items })))
                    .expect(1),
            );
            let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
            let report = push_at(&shop, &link, &remote, NOON).unwrap();
            match case {
                "omits" => {
                    assert_eq!((report.sent, report.rescheduled), (1, 1), "{case}");
                    assert_eq!(remote_of(&shop, &link, &b).as_deref(), Some("2"));
                    assert_eq!(remote_of(&shop, &link, &a), None, "never B's id");
                    assert_eq!(state_of(&shop, &a), "pending", "sent again later");
                }
                "reorders" => {
                    assert_eq!(report.sent, 2, "{case}");
                    assert_eq!(remote_of(&shop, &link, &a).as_deref(), Some("1"));
                    assert_eq!(remote_of(&shop, &link, &b).as_deref(), Some("2"));
                }
                _ => {
                    assert_eq!((report.failed, report.rescheduled), (1, 1), "{case}");
                    assert_eq!(state_of(&shop, &b), "failed");
                    assert_eq!(state_of(&shop, &a), "pending", "not failed for B's refusal");
                }
            }
            dz.verify();
        }
    }

    #[test]
    fn products_are_batched_skipped_when_unchanged_and_drafted_beyond_the_plan_cap() {
        let dz = MockDz::start();
        let (shop, mut link) = linked_shop();
        link.active_products_cap = Some(1);
        for name in ["A", "B"] {
            let p = shop.product(name, Money(1_000), &shop.tva19);
            publish(&shop, &p.id);
        }
        derive_now(&shop, &link);
        dz.mount(
            Mock::given(path("/v1/products/batch"))
                .respond_with(ok(json!({ "items": [
                    { "id": 1, "status": "active", "error": null },
                    { "id": 2, "status": "draft", "error": null }
                ] })))
                .expect(1),
        );
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        let report = push_at(&shop, &link, &remote, NOON).unwrap();
        assert_eq!((report.sent, report.drafted), (2, 1));
        let body: serde_json::Value = serde_json::from_slice(&dz.requests()[0].body).unwrap();
        let statuses: Vec<&str> = body["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["status"].as_str().unwrap())
            .collect();
        assert_eq!(statuses, ["active", "draft"]);
        assert!(
            header(&dz.requests()[0], "idempotency-key")
                .unwrap()
                .starts_with("products:")
        );
        let notices = shop
            .db
            .read(|c| crate::cloud::store::notices(c, false))
            .unwrap();
        assert!(notices.iter().any(|n| n.code == "cloud_plan_cap"));
        // nothing changed: the next derive + push sends nothing
        shop.db
            .write(|tx| {
                tx.execute("UPDATE products SET updated_at = updated_at + 1", [])?;
                Ok(())
            })
            .unwrap();
        derive_now(&shop, &link);
        let again = push_at(&shop, &link, &remote, NOON + 1).unwrap();
        assert_eq!((again.sent, again.unchanged), (0, 2));
        dz.verify();
    }

    #[test]
    fn a_product_changed_back_within_a_day_takes_a_new_batch_key() {
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        publish(&shop, &milk.id);
        let batches = IdempotentBatches::default();
        dz.mount(Mock::given(path("/v1/products/batch")).respond_with(batches.clone()));
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        let price = |centimes: i64| {
            let id = milk.id.clone();
            shop.db
                .write(move |tx| {
                    tx.execute(
                        "UPDATE products SET sell_price = ?2 WHERE id = ?1",
                        rusqlite::params![id, centimes],
                    )?;
                    Ok(())
                })
                .unwrap();
            derive_now(&shop, &link);
        };
        derive_now(&shop, &link);
        assert_eq!(push_at(&shop, &link, &remote, NOON).unwrap().sent, 1);
        let first = batches.storefront.lock().unwrap().clone();
        price(12_000); // a typo …
        assert_eq!(
            push_at(&shop, &link, &remote, NOON + 60_000).unwrap().sent,
            1
        );
        price(10_000); // … fixed within the hour: the same body as the first push
        assert_eq!(
            push_at(&shop, &link, &remote, NOON + 120_000).unwrap().sent,
            1
        );
        let keys = batch_keys(&dz);
        assert_eq!(keys.len(), 3);
        assert!(
            keys.iter()
                .all(|k| k.starts_with("products:") && k.len() == 41),
            "{keys:?}"
        );
        assert_ne!(keys[2], keys[0], "a new edit is a new request: {keys:?}");
        assert_eq!(
            *batches.storefront.lock().unwrap(),
            first,
            "DZBuild applied the fix, not a replay of the first answer"
        );
    }

    #[test]
    fn an_owner_retry_of_a_refused_product_takes_a_new_batch_key() {
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        publish(&shop, &milk.id);
        derive_now(&shop, &link);
        dz.mount(
            Mock::given(path("/v1/products/batch"))
                .respond_with(
                    ResponseTemplate::new(422)
                        .set_body_json(json!({ "error": { "code": "validation_failed" } })),
                )
                .up_to_n_times(1),
        );
        mount_products_ok(&dz);
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        assert_eq!(push_at(&shop, &link, &remote, NOON).unwrap().failed, 1);
        let row: String = shop
            .db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT id FROM sync_outbox WHERE kind = 'product_upsert'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        shop.db
            .write(move |tx| outbox::retry(tx, &row, NOON + 1))
            .unwrap();
        // The same product, the same body: under the old key DZBuild would replay its 422.
        assert_eq!(push_at(&shop, &link, &remote, NOON + 2).unwrap().sent, 1);
        let keys = batch_keys(&dz);
        assert_eq!(keys.len(), 2);
        assert_ne!(keys[0], keys[1], "{keys:?}");
    }

    #[test]
    fn a_photo_goes_up_in_three_steps_and_the_storage_host_never_sees_the_bearer() {
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        publish(&shop, &milk.id);
        let dir = tempfile::tempdir().unwrap();
        let media = dir.path().join("media");
        let (name, bytes) = photo(&shop, &milk.id, &media);
        derive_now(&shop, &link);
        mount_products_ok(&dz);
        dz.mount(
            Mock::given(path("/v1/inventory/adjustments/batch"))
                .respond_with(ok(json!({ "items": [] }))),
        );
        mount_ticket(&dz, &format!("{}/upload/m1", dz.uri()));
        dz.mount(
            Mock::given(method("PUT"))
                .and(path("/upload/m1"))
                .respond_with(ResponseTemplate::new(200))
                .expect(1),
        );
        dz.mount(
            Mock::given(method("POST"))
                .and(path("/v1/products/88121/images"))
                .and(body_partial_json(json!({ "media_id": "m1" })))
                .respond_with(created(json!({ "id": 7 })))
                .expect(1),
        );
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        push_with(&shop, &link, &remote, NOON, Some(&media)).unwrap();
        dz.verify();
        let requests = dz.requests();
        let find = |p: &str| requests.iter().find(|r| r.url.path() == p).unwrap();

        let ticket = find("/v1/media/uploads");
        let ask: Value = serde_json::from_slice(&ticket.body).unwrap();
        assert_eq!(
            ask["filename"],
            json!(format!("{}-1.jpg", milk.id)),
            "no folder in the name"
        );
        assert_eq!(ask["size"], json!(bytes.len()));
        assert_eq!(
            header(ticket, "idempotency-key"),
            Some(crate::cloud::outbox_key(
                "media",
                &format!("{}:{name}", milk.id)
            ))
        );
        assert!(
            header(ticket, "authorization").is_some(),
            "DZBuild's own endpoint"
        );
        let put = find("/upload/m1");
        assert_eq!(put.body, bytes, "the file's bytes");
        assert_eq!(header(put, "content-type").as_deref(), Some("image/jpeg"));
        assert_eq!(
            header(put, "authorization"),
            None,
            "no bearer on the storage host"
        );
        let attach = find("/v1/products/88121/images");
        assert!(
            header(attach, "idempotency-key")
                .unwrap()
                .starts_with("media-attach:")
        );
        assert!(outbox_rows(&shop).contains(&(
            "media_upload".into(),
            milk.id.clone(),
            "sent".into()
        )));
        let id = milk.id.clone();
        let mapped = shop
            .db
            .read(move |c| store::map_get(c, "141", MapEntity::Media, &id))
            .unwrap()
            .unwrap();
        assert_eq!(mapped.remote_id, "m1");
        assert_eq!(mapped.last_payload, Some(json!({ "file": name })));

        // A name the catalogue never writes is not read, even when the file is there.
        std::fs::write(dir.path().join("x.jpg"), &bytes).unwrap();
        let id = milk.id.clone();
        shop.db
            .write(move |tx| {
                outbox::enqueue(
                    tx,
                    outbox::NewItem {
                        store_id: "141".into(),
                        kind: OutboxKind::MediaUpload,
                        entity_id: id.clone(),
                        payload: json!({ "image": "../x.jpg" }),
                        key: crate::cloud::outbox_key("media", &format!("{id}:../x.jpg")),
                        depends_on: None,
                    },
                    NOON,
                )
                .map(drop)
            })
            .unwrap();
        let before = dz.requests().len();
        push_with(&shop, &link, &remote, NOON + 1, Some(&media)).unwrap();
        assert_eq!(dz.requests().len(), before, "no request for it");
        let (status, reason, _, _) = media_row(&shop, "../x.jpg");
        assert_eq!(
            (status.as_str(), reason.as_deref()),
            ("skipped", Some("media_missing"))
        );

        // The pre-signed URL is a capability: `Debug` never prints it. Its PUT rides the media lane.
        let ticket: MediaTicket = serde_json::from_value(json!({
            "media_id": "m1", "upload_url": "https://storage.example/upload/m1?X-Amz-Signature=s3cr3t"
        }))
        .unwrap();
        let shown = format!("{ticket:?}");
        assert!(
            !shown.contains("storage.example") && !shown.contains("s3cr3t"),
            "{shown}"
        );
        let call = format!("{:?}", upload_call(&ticket, Vec::new()));
        assert!(
            call.contains("lane: Media") && call.contains("\"upload\""),
            "{call}"
        );
    }

    /// An expired pre-signed URL (the storage host answers 403, DZBuild replays the first ticket
    /// under the item's key): a fresh ticket is asked once under a new key and the photo goes up.
    #[test]
    fn an_expired_upload_url_gets_a_fresh_ticket() {
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        publish(&shop, &milk.id);
        let dir = tempfile::tempdir().unwrap();
        let media = dir.path().join("media");
        let (name, _) = photo(&shop, &milk.id, &media);
        derive_now(&shop, &link);
        mount_products_ok(&dz);
        dz.mount(
            Mock::given(path("/v1/inventory/adjustments/batch"))
                .respond_with(ok(json!({ "items": [] }))),
        );
        let key = crate::cloud::outbox_key("media", &format!("{}:{name}", milk.id));
        dz.mount(
            Mock::given(method("POST"))
                .and(path("/v1/media/uploads"))
                .and(wiremock::matchers::header("idempotency-key", key.as_str()))
                .respond_with(created(
                    json!({ "media_id": "m1", "upload_url": format!("{}/upload/expired", dz.uri()) }),
                ))
                .expect(1),
        );
        dz.mount(
            Mock::given(method("POST"))
                .and(path("/v1/media/uploads"))
                .and(wiremock::matchers::header(
                    "idempotency-key",
                    format!("{key}:t1").as_str(),
                ))
                .respond_with(created(
                    json!({ "media_id": "m2", "upload_url": format!("{}/upload/fresh", dz.uri()) }),
                ))
                .expect(1),
        );
        dz.mount(
            Mock::given(method("PUT"))
                .and(path("/upload/expired"))
                .respond_with(ResponseTemplate::new(403))
                .expect(1),
        );
        dz.mount(
            Mock::given(method("PUT"))
                .and(path("/upload/fresh"))
                .respond_with(ResponseTemplate::new(200))
                .expect(1),
        );
        dz.mount(
            Mock::given(method("POST"))
                .and(path("/v1/products/88121/images"))
                .and(body_partial_json(json!({ "media_id": "m2" })))
                .respond_with(created(json!({ "id": 7 })))
                .expect(1),
        );
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        push_with(&shop, &link, &remote, NOON, Some(&media)).unwrap();
        dz.verify();
        let (status, _, _, _) = media_row(&shop, &name);
        assert_eq!(status, "sent");
    }

    #[test]
    fn a_storage_host_down_or_refusing_reschedules_the_photo_and_the_sale_still_goes() {
        for refused in [true, false] {
            let dz = MockDz::start();
            let (shop, link) = linked_shop();
            let milk = shop.product("Lait", Money(10_000), &shop.tva19);
            publish(&shop, &milk.id);
            shop.stock_in(&milk.id, 5_000);
            let dir = tempfile::tempdir().unwrap();
            let (name, _) = photo(&shop, &milk.id, dir.path());
            shop.open_till(0);
            let sale = sell_one(&shop, &milk.id);
            derive_now(&shop, &link);
            mount_products_ok(&dz);
            dz.mount(
                Mock::given(path("/v1/inventory/adjustments/batch"))
                    .respond_with(ok(json!({ "items": [] }))),
            );
            let upload = if refused {
                // an expired or foreign signature: the storage host answers 401
                dz.mount(
                    Mock::given(method("PUT"))
                        .and(path("/upload/m1"))
                        .respond_with(ResponseTemplate::new(401)),
                );
                format!("{}/upload/m1", dz.uri())
            } else {
                format!("{UNREACHABLE}/upload/m1")
            };
            mount_ticket(&dz, &upload);
            dz.mount(
                Mock::given(path_regex(r"^/v1/products/\d+/images$"))
                    .respond_with(created(json!({ "id": 7 })))
                    .expect(0),
            );
            dz.mount(
                Mock::given(path("/v1/pos/sales"))
                    .respond_with(created(json!({ "id": 99120, "stock_applied": false })))
                    .expect(1),
            );
            let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
            let report = push_with(&shop, &link, &remote, NOON, Some(dir.path()))
                .unwrap_or_else(|e| panic!("refused={refused}: the cycle stopped: {e:?}"));
            dz.verify();
            assert_eq!(report.rescheduled, 1, "refused={refused}: {report:?}");
            assert!(
                outbox_rows(&shop).contains(&("pos_sale".into(), sale, "sent".into())),
                "refused={refused}: the sale went"
            );
            let (status, _, attempts, next) = media_row(&shop, &name);
            assert_eq!(
                (status.as_str(), attempts, next),
                ("pending", 1, NOON + outbox::backoff_ms(1)),
                "refused={refused}: the photo is tried again later"
            );
        }
    }

    #[test]
    fn a_failed_stock_batch_is_derived_again() {
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        publish(&shop, &milk.id);
        shop.stock_in(&milk.id, 7_000);
        derive_now(&shop, &link);
        mount_products_ok(&dz);
        dz.mount(
            Mock::given(path("/v1/inventory/adjustments/batch"))
                .respond_with(ok(json!({ "items": [
                    { "ref": "r", "status": "error", "error": { "code": "product_not_found" } }
                ] })))
                .up_to_n_times(1),
        );
        dz.mount(
            Mock::given(path("/v1/inventory/adjustments/batch")).respond_with(ok(
                json!({ "items": [{ "ref": "r", "status": "applied" }] }),
            )),
        );
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        push_at(&shop, &link, &remote, NOON).unwrap();
        derive_now(&shop, &link); // the refused set is planned again
        push_at(&shop, &link, &remote, NOON + 1).unwrap();
        let bodies: Vec<serde_json::Value> = dz
            .requests()
            .iter()
            .filter(|r| r.url.path() == "/v1/inventory/adjustments/batch")
            .map(|r| serde_json::from_slice(&r.body).unwrap())
            .collect();
        assert_eq!(bodies.len(), 2);
        assert_eq!(
            bodies[1]["items"][0]["set"], 7,
            "a reset sends the level again"
        );
        let confirmed: Option<i64> = shop
            .db
            .read(|c| Ok(c.query_row("SELECT confirmed FROM cloud_stock", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(confirmed, Some(7));
    }

    #[test]
    fn offline_stops_the_cycle_without_counting_an_attempt() {
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        shop.stock_in(&milk.id, 1_000);
        shop.open_till(0);
        sell_one(&shop, &milk.id);
        derive_now(&shop, &link);
        let remote = remote_at(UNREACHABLE, &link, ManualClock::at(NOON));
        assert!(
            push_at(&shop, &link, &remote, NOON)
                .unwrap_err()
                .is_offline()
        );
        let attempts: i64 = shop
            .db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT attempts FROM sync_outbox WHERE kind = 'pos_sale'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(attempts, 0, "a dead network is not the item's fault");
    }

    #[test]
    fn a_refund_goes_to_its_sale_and_a_double_stock_deduction_asks_for_a_reset() {
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        publish(&shop, &milk.id);
        shop.stock_in(&milk.id, 5_000);
        shop.open_till(0);
        let ticket = sell_one(&shop, &milk.id);
        derive_now(&shop, &link);
        mount_products_ok(&dz);
        dz.mount(
            Mock::given(path("/v1/inventory/adjustments/batch"))
                .respond_with(ok(json!({ "items": [] }))),
        );
        dz.mount(
            Mock::given(path("/v1/pos/sales"))
                .respond_with(created(json!({ "id": 99120, "stock_applied": true }))),
        );
        dz.mount(
            Mock::given(path("/v1/pos/sales/99120/refunds"))
                .respond_with(created(json!({ "id": 5 })))
                .expect(1),
        );
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        push_at(&shop, &link, &remote, NOON).unwrap();
        let notices = shop
            .db
            .read(|c| crate::cloud::store::notices(c, false))
            .unwrap();
        assert!(
            notices.iter().any(|n| n.code == "stock_double_applied"),
            "{notices:?}"
        );
        let refund = crate::cloud::testkit::return_all(&shop, &ticket);
        derive_now(&shop, &link);
        push_at(&shop, &link, &remote, NOON + 1).unwrap();
        dz.verify();
        let request = dz
            .requests()
            .into_iter()
            .find(|r| r.url.path() == "/v1/pos/sales/99120/refunds")
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["external_id"], json!(refund.id), "the refund itself");
        assert!(body.get("sale").is_none(), "the path names the sale");
        assert_eq!(
            header(&request, "idempotency-key"),
            Some(format!("refund:{}", refund.id))
        );
    }

    #[test]
    fn a_queued_claim_refused_by_dzbuild_puts_the_order_back_in_the_queue() {
        use crate::cloud::orders::{self, OrderState, RemoteOrder};
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        let remote_order: RemoteOrder = serde_json::from_str(
            &json!({
                "id": 9001, "order_number": 1042, "status": "confirmed", "subtotal": 200, "total": 200,
                "created_at": "2026-09-28 10:00:00", "updated_at": "2026-09-28 10:00:00",
                "items": [{ "product_name": "Lait", "price": 100, "quantity": 2, "total": 200 }]
            })
            .to_string(),
        )
        .unwrap();
        let (till, l) = (shop.till(), link.clone());
        let id = shop
            .db
            .write(move |tx| {
                orders::upsert_remote(tx, &l, &remote_order, NOON)?;
                let id: String = tx.query_row(
                    "SELECT id FROM online_orders WHERE remote_id = '9001'",
                    [],
                    |r| r.get(0),
                )?;
                orders::claim(tx, &till, &l, &id, false).map(|o| o.id)
            })
            .unwrap();
        dz.mount(
            Mock::given(method("POST"))
                .and(path("/v1/orders/9001/claim"))
                .respond_with(ResponseTemplate::new(409).set_body_json(json!({
                    "error": { "code": "already_claimed", "message": "claimed by another device" }
                })))
                .expect(1),
        );
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        let report = push_at(&shop, &link, &remote, NOON).unwrap();
        assert_eq!((report.sent, report.failed), (0, 1));
        push_at(&shop, &link, &remote, NOON + 3_600_000).unwrap();
        dz.verify(); // skipped, never sent again
        assert_eq!(
            outbox_rows(&shop),
            [("order_claim".into(), id.clone(), "skipped".into())],
            "nothing waits for the owner"
        );
        let terminal = shop.terminal.id.clone();
        let order = shop
            .db
            .read(move |c| orders::get(c, &id, &terminal))
            .unwrap();
        assert_eq!(
            (order.local_state, order.claimed_by_me),
            (OrderState::New, false)
        );
        let notices = shop
            .db
            .read(|c| crate::cloud::store::notices(c, false))
            .unwrap();
        assert!(
            notices.iter().any(|n| (n.code.as_str(), n.subject.as_str())
                == ("cloud_order_claimed_elsewhere", "1042")),
            "{notices:?}"
        );
    }

    /// Order 9001 (#1042) mirrored: 2 × Lait at 100 DA, its product `external_id` when given.
    fn online_order(shop: &Shop, link: &CloudLink, external_id: Option<&str>) -> String {
        let remote_order: crate::cloud::orders::RemoteOrder = serde_json::from_str(
            &json!({
                "id": 9001, "order_number": 1042, "status": "confirmed", "subtotal": 200, "total": 200,
                "created_at": "2026-09-28 10:00:00", "updated_at": "2026-09-28 10:00:00",
                "items": [{ "external_id": external_id, "product_name": "Lait", "price": 100, "quantity": 2, "total": 200 }]
            })
            .to_string(),
        )
        .unwrap();
        let l = link.clone();
        shop.db
            .write(move |tx| {
                crate::cloud::orders::upsert_remote(tx, &l, &remote_order, NOON)?;
                Ok(tx.query_row(
                    "SELECT id FROM online_orders WHERE remote_id = '9001'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap()
    }

    fn order_request(dz: &MockDz, verb: &str, url: &str) -> (Value, Option<String>) {
        let request = dz
            .requests()
            .into_iter()
            .find(|r| r.method.as_str() == verb && r.url.path() == url)
            .unwrap_or_else(|| panic!("no {verb} {url}"));
        (
            serde_json::from_slice(&request.body).unwrap(),
            header(&request, "idempotency-key"),
        )
    }

    fn mount_claim_refused(dz: &MockDz) {
        dz.mount(
            Mock::given(method("POST"))
                .and(path("/v1/orders/9001/claim"))
                .respond_with(ResponseTemplate::new(409).set_body_json(json!({
                    "error": { "code": "already_claimed", "message": "claimed by another device" }
                })))
                .expect(1),
        );
        dz.mount(
            Mock::given(method("PATCH"))
                .and(path("/v1/orders/9001"))
                .respond_with(ok(json!({ "id": 9001 })))
                .expect(0),
        );
    }

    #[test]
    fn a_claim_refused_after_the_order_was_prepared_offline_takes_it_back_and_sends_nothing_else() {
        use crate::cloud::orders::{self, OrderState};
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        let id = online_order(&shop, &link, None);
        let (till, l, i) = (shop.till(), link.clone(), id.clone());
        shop.db
            .write(move |tx| {
                orders::claim(tx, &till, &l, &i, false)?;
                orders::prepare(tx, &till, &l, &i).map(drop)
            })
            .unwrap();
        mount_claim_refused(&dz);
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        let report = push_at(&shop, &link, &remote, NOON).unwrap();
        assert_eq!((report.sent, report.failed), (0, 1));
        push_at(&shop, &link, &remote, NOON + 3_600_000).unwrap();
        dz.verify(); // one claim, no "processing" for an order another device prepares
        assert_eq!(
            outbox_rows(&shop),
            [
                ("order_claim".into(), id.clone(), "skipped".into()),
                ("order_status".into(), id.clone(), "skipped".into())
            ]
        );
        let terminal = shop.terminal.id.clone();
        let order = shop
            .db
            .read(move |c| orders::get(c, &id, &terminal))
            .unwrap();
        assert_eq!(
            (order.local_state, order.claimed_by_me),
            (OrderState::New, false),
            "the till no longer sells or ships it"
        );
        let notices = shop
            .db
            .read(|c| crate::cloud::store::notices(c, false))
            .unwrap();
        assert!(
            notices.iter().any(|n| (n.code.as_str(), n.subject.as_str())
                == ("cloud_order_claimed_elsewhere", "1042")),
            "{notices:?}"
        );
    }

    #[test]
    fn a_claim_refused_after_the_order_was_sold_keeps_the_sale_and_says_so() {
        use crate::cloud::orders::{self, OrderState};
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        shop.stock_in(&milk.id, 10_000);
        shop.open_till(0);
        let id = online_order(&shop, &link, Some(&milk.id));
        let (till, l, i) = (shop.till(), link.clone(), id.clone());
        shop.db
            .write(move |tx| {
                orders::claim(tx, &till, &l, &i, false)?;
                orders::sell(
                    tx,
                    &till,
                    &l,
                    &i,
                    "Livraison".into(),
                    vec![PaymentInput::cash(Money(20_000))],
                )
                .map(drop)
            })
            .unwrap();
        mount_claim_refused(&dz);
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        push_at(&shop, &link, &remote, NOON).unwrap();
        dz.verify(); // no "delivered" for an order DZBuild gave to another device
        assert_eq!(
            outbox_rows(&shop),
            [
                ("order_claim".into(), id.clone(), "skipped".into()),
                ("order_status".into(), id.clone(), "skipped".into())
            ]
        );
        let terminal = shop.terminal.id.clone();
        let order = shop
            .db
            .read(move |c| orders::get(c, &id, &terminal))
            .unwrap();
        assert_eq!(
            order.local_state,
            OrderState::Fulfilled,
            "a validated ticket stays"
        );
        let notices = shop
            .db
            .read(|c| crate::cloud::store::notices(c, false))
            .unwrap();
        let codes: Vec<(&str, &str)> = notices
            .iter()
            .map(|n| (n.code.as_str(), n.subject.as_str()))
            .collect();
        assert_eq!(
            codes,
            [("online_order_fulfilled_claimed_elsewhere", "1042")],
            "the owner is told the order may be prepared twice"
        );
    }

    #[test]
    fn a_queued_claim_goes_first_and_the_order_updates_follow_it_once_accepted() {
        use crate::cloud::orders;
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        let id = online_order(&shop, &link, None);
        let (till, l, i) = (shop.till(), link.clone(), id.clone());
        shop.db
            .write(move |tx| {
                orders::claim(tx, &till, &l, &i, false)?;
                orders::prepare(tx, &till, &l, &i).map(drop)
            })
            .unwrap();
        dz.mount(
            Mock::given(method("POST"))
                .and(path("/v1/orders/9001/claim"))
                .respond_with(ResponseTemplate::new(503))
                .up_to_n_times(4),
        );
        dz.mount(
            Mock::given(method("POST"))
                .and(path("/v1/orders/9001/claim"))
                .respond_with(ok(json!({ "id": 9001, "claimed_by": "dev_1" }))),
        );
        dz.mount(
            Mock::given(method("PATCH"))
                .and(path("/v1/orders/9001"))
                .respond_with(ok(json!({ "id": 9001, "status": "processing" })))
                .expect(1),
        );
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        let report = push_at(&shop, &link, &remote, NOON).unwrap();
        assert_eq!(
            (report.sent, report.rescheduled),
            (0, 1),
            "the claim is retried later and the update waits for it"
        );
        let later = NOON + crate::cloud::outbox::backoff_ms(1);
        assert_eq!(push_at(&shop, &link, &remote, later).unwrap().sent, 2);
        dz.verify();
        let (claim, claim_key) = order_request(&dz, "POST", "/v1/orders/9001/claim");
        assert_eq!(claim, json!({ "device_id": "dev_1", "terminal": "C1" }));
        assert_eq!(claim_key, Some(format!("order:{id}:claim")));
        let (update, update_key) = order_request(&dz, "PATCH", "/v1/orders/9001");
        assert_eq!(update, json!({ "status": "processing" }));
        assert_eq!(update_key, Some(format!("order:{id}:processing")));
        let last = dz.requests().last().map(|r| r.method.as_str().to_owned());
        assert_eq!(last.as_deref(), Some("PATCH"), "after the claim");
        assert_eq!(
            outbox_rows(&shop),
            [
                ("order_claim".into(), id.clone(), "sent".into()),
                ("order_status".into(), id, "sent".into())
            ]
        );
    }

    #[test]
    fn a_refusal_cancels_the_order_on_dzbuild_under_its_own_key() {
        use crate::cloud::orders::{self, RefuseReason};
        let dz = MockDz::start();
        let (shop, link) = linked_shop();
        let id = online_order(&shop, &link, None);
        let (till, l, i) = (shop.till(), link.clone(), id.clone());
        shop.db
            .write(move |tx| {
                orders::refuse(
                    tx,
                    &till,
                    &l,
                    &i,
                    RefuseReason::OutOfStock,
                    Some("Plus de lait".into()),
                )
                .map(drop)
            })
            .unwrap();
        dz.mount(
            Mock::given(method("POST"))
                .and(path("/v1/orders/9001/cancel"))
                .respond_with(ok(json!({ "id": 9001, "status": "cancelled" })))
                .expect(1),
        );
        let remote = remote_for_test(&dz, &link, ManualClock::at(NOON));
        assert_eq!(push_at(&shop, &link, &remote, NOON).unwrap().sent, 1);
        push_at(&shop, &link, &remote, NOON + 3_600_000).unwrap();
        dz.verify(); // sent once
        let (body, key) = order_request(&dz, "POST", "/v1/orders/9001/cancel");
        assert_eq!(
            body,
            json!({ "reason": "out_of_stock", "note": "Plus de lait" })
        );
        assert_eq!(key, Some(format!("order:{id}:cancel")));
        assert_eq!(
            outbox_rows(&shop),
            [("order_cancel".into(), id, "sent".into())]
        );
    }
}

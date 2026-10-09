//! Online orders (DZB-014, OMN): a local mirror of the store's orders with the till's claim, and
//! the till's workflow — claim, prepare, then either sell at the till (a ticket through the M1a
//! sale, at the online prices: a price that differs from the shop's asks a manager like any
//! price change, and so do the delivery fee (an open item) and the order's discount for an
//! operator without those rights; cash on delivery or at pickup), or ship with a BL (M3; the
//! courier collects), or refuse. Every step tells DZBuild through the outbox, behind this till's
//! queued claim; the stock DZBuild took for the order is acknowledged, never deducted twice.
//! Buyer details are purged `cloud.orders.retention_days` after the order is closed (loi 18-07).

use super::outbox::{self, NewItem, OutboxKind};
use super::store::{self, CloudLink, CloudNoticeKind, MapEntity};
use super::wire::{ApiMoney, ApiTime, IdString, de_id};
use crate::audit::{self, Actor};
use crate::catalog::products;
use crate::company::PriceMode;
use crate::documents::Document;
use crate::error::AppError;
use crate::ids::new_id;
use crate::money::{Money, Qty, Rate};
use crate::payments::PaymentInput;
use crate::sales::{self, Cart, CartLine, PaymentPreview, SaleInput, ValidatedSale};
use crate::sqlenum::sql_text_enum;
use crate::tax::tva::Discount;
use crate::till::TillContext;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OrderState {
    New,
    Claimed,
    Prepared,
    Fulfilled,
    Refused,
    Closed,
}

impl OrderState {
    pub const fn as_str(self) -> &'static str {
        match self {
            OrderState::New => "new",
            OrderState::Claimed => "claimed",
            OrderState::Prepared => "prepared",
            OrderState::Fulfilled => "fulfilled",
            OrderState::Refused => "refused",
            OrderState::Closed => "closed",
        }
    }
    pub fn parse(s: &str) -> Option<OrderState> {
        [
            OrderState::New,
            OrderState::Claimed,
            OrderState::Prepared,
            OrderState::Fulfilled,
            OrderState::Refused,
            OrderState::Closed,
        ]
        .into_iter()
        .find(|o| o.as_str() == s)
    }
    const fn open(self) -> bool {
        matches!(
            self,
            OrderState::New | OrderState::Claimed | OrderState::Prepared
        )
    }
}

sql_text_enum!(OrderState);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RefuseReason {
    OutOfStock,
    CustomerUnreachable,
    Duplicate,
    Other,
}

impl RefuseReason {
    const fn as_str(self) -> &'static str {
        match self {
            RefuseReason::OutOfStock => "out_of_stock",
            RefuseReason::CustomerUnreachable => "customer_unreachable",
            RefuseReason::Duplicate => "duplicate",
            RefuseReason::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteOrderItem {
    #[serde(default)]
    pub product_id: Option<IdString>,
    #[serde(default)]
    pub external_id: Option<String>,
    pub product_name: String,
    #[serde(default)]
    pub variant_name: Option<String>,
    #[serde(default)]
    pub sku: Option<String>,
    pub price: ApiMoney,
    pub quantity: i64,
    pub total: ApiMoney,
}

/// A DZBuild order (webhook payload fields, research 06 §3.1.6).
#[derive(Debug, Clone, Deserialize)]
pub struct RemoteOrder {
    #[serde(deserialize_with = "de_id")]
    pub id: String,
    #[serde(deserialize_with = "de_id")]
    pub order_number: String,
    pub status: String,
    #[serde(default)]
    pub payment_method: Option<String>,
    /// DZBuild's payment status (`pending`, `paid`…), kept as sent.
    #[serde(default)]
    pub payment_status: Option<String>,
    #[serde(default)]
    pub delivery_type: Option<String>,
    #[serde(default)]
    pub customer_name: Option<String>,
    #[serde(default)]
    pub customer_phone: Option<String>,
    #[serde(default)]
    pub wilaya_id: Option<i64>,
    #[serde(default)]
    pub commune: Option<String>,
    #[serde(default)]
    pub address: Option<String>,
    #[serde(default)]
    pub customer_notes: Option<String>,
    pub subtotal: ApiMoney,
    #[serde(default)]
    pub shipping_cost: Option<ApiMoney>,
    #[serde(default)]
    pub discount: Option<ApiMoney>,
    pub total: ApiMoney,
    pub created_at: ApiTime,
    pub updated_at: ApiTime,
    #[serde(default)]
    pub items: Vec<RemoteOrderItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpsertOutcome {
    New,
    Updated,
    Unchanged,
    Closed,
}

const REMOTE_STATUSES: [&str; 7] = [
    "pending",
    "confirmed",
    "processing",
    "shipped",
    "delivered",
    "cancelled",
    "returned",
];

fn resolve_product(
    conn: &Connection,
    store_id: &str,
    item: &RemoteOrderItem,
) -> Result<Option<String>, AppError> {
    if let Some(id) = item.external_id.as_deref()
        && products::get(conn, id).is_ok()
    {
        return Ok(Some(id.to_owned()));
    }
    if let Some(remote) = &item.product_id
        && let Some(local) = store::map_local(conn, store_id, MapEntity::Product, &remote.0)?
    {
        return Ok(Some(local));
    }
    match item.sku.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(sku) => Ok(conn
            .query_row(
                "SELECT id FROM products WHERE reference = ?1 AND archived_at IS NULL",
                [sku],
                |r| r.get(0),
            )
            .optional()?),
        None => Ok(None),
    }
}

fn replace_lines(
    tx: &Transaction<'_>,
    store_id: &str,
    order_id: &str,
    remote: &RemoteOrder,
) -> Result<(), AppError> {
    tx.execute(
        "DELETE FROM online_order_lines WHERE order_id = ?1",
        [order_id],
    )?;
    for (index, item) in remote.items.iter().filter(|i| i.quantity > 0).enumerate() {
        let product_id = resolve_product(tx, store_id, item)?;
        tx.execute(
            "INSERT INTO online_order_lines (order_id, line_no, remote_product_id, product_id, product_name,
                 variant_name, sku, unit_price, qty, total)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                order_id,
                i64::try_from(index + 1).unwrap_or(i64::MAX),
                item.product_id.as_ref().map(|i| i.0.clone()),
                product_id,
                item.product_name,
                item.variant_name,
                item.sku,
                item.price.0,
                Qty(item.quantity.saturating_mul(1_000)),
                item.total.0
            ],
        )?;
    }
    Ok(())
}

fn lines_signature(conn: &Connection, order_id: &str) -> Result<Vec<(String, i64, i64)>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT product_name, qty, unit_price FROM online_order_lines WHERE order_id = ?1 ORDER BY line_no",
    )?;
    let rows = stmt
        .query_map([order_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Mirrors an order (events or delta pull). A cancelled, returned or delivered-elsewhere order
/// that the till has not fulfilled is closed; items changed after the till prepared the order
/// send it back to "claimed" with a notice. While the order is open, the buyer's details, the
/// delivery type and the payment method follow DZBuild's latest copy (the merchant's
/// confirmation call often corrects the address or the phone): the till shows them, chooses
/// between selling and shipping by them, and names the consignee of a BL from them. A row whose
/// buyer was purged never gets them back.
pub fn upsert_remote(
    tx: &Transaction<'_>,
    link: &CloudLink,
    remote: &RemoteOrder,
    now: i64,
) -> Result<UpsertOutcome, AppError> {
    let status = if REMOTE_STATUSES.contains(&remote.status.as_str()) {
        remote.status.as_str()
    } else {
        "pending"
    };
    let known_delivery = match remote.delivery_type.as_deref() {
        Some(d @ ("home" | "desk" | "pickup" | "digital")) => Some(d),
        _ => None,
    };
    let delivery = known_delivery.unwrap_or("home").to_owned();
    let phone =
        crate::party::clean_phone(remote.customer_phone.as_deref(), "phone").unwrap_or(None);
    let wilaya = remote.wilaya_id.filter(|w| (1..=69).contains(w));
    // Blank or longer than the column takes: not a status, the known one stays.
    let payment_status = remote
        .payment_status
        .as_deref()
        .map(str::trim)
        .filter(|p| (1..=40).contains(&p.chars().count()));
    let existing: Option<(String, OrderState, i64)> = tx
        .query_row(
            "SELECT id, local_state, remote_updated_at FROM online_orders WHERE store_id = ?1 AND remote_id = ?2",
            params![link.store_id, remote.id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let money = |m: Option<ApiMoney>| m.map_or(Money::ZERO, |m| m.0);
    let Some((id, state, updated_at)) = existing else {
        let id = new_id();
        tx.execute(
            "INSERT INTO online_orders (id, store_id, remote_id, order_number, remote_status, payment_method, delivery_type,
                 customer_name, customer_phone, wilaya_id, commune, address, customer_notes, subtotal, shipping_cost,
                 discount, total, local_state, remote_created_at, remote_updated_at, created_at, updated_at,
                 payment_status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, 'new', ?18, ?19, ?20, ?20,
                     ?21)",
            params![
                id,
                link.store_id,
                remote.id,
                remote.order_number,
                status,
                remote.payment_method.clone().unwrap_or_else(|| "cod".into()),
                delivery,
                remote.customer_name,
                phone,
                wilaya,
                remote.commune,
                remote.address,
                remote.customer_notes,
                remote.subtotal.0,
                money(remote.shipping_cost),
                money(remote.discount),
                remote.total.0,
                remote.created_at.0,
                remote.updated_at.0,
                now,
                payment_status
            ],
        )?;
        replace_lines(tx, &link.store_id, &id, remote)?;
        if matches!(status, "cancelled" | "returned" | "delivered") {
            tx.execute(
                "UPDATE online_orders SET local_state = 'closed', closed_at = ?2 WHERE id = ?1",
                params![id, now],
            )?;
            return Ok(UpsertOutcome::Closed);
        }
        return Ok(UpsertOutcome::New);
    };
    if remote.updated_at.0 <= updated_at {
        return Ok(UpsertOutcome::Unchanged);
    }
    tx.execute(
        "UPDATE online_orders SET remote_status = ?2, subtotal = ?3, shipping_cost = ?4, discount = ?5, total = ?6,
             remote_updated_at = ?7, updated_at = ?8, payment_status = COALESCE(?9, payment_status) WHERE id = ?1",
        params![
            id,
            status,
            remote.subtotal.0,
            money(remote.shipping_cost),
            money(remote.discount),
            remote.total.0,
            remote.updated_at.0,
            now,
            payment_status
        ],
    )?;
    if state.open() {
        // A type or method DZBuild does not name keeps the one the row has.
        tx.execute(
            "UPDATE online_orders SET customer_name = ?2, customer_phone = ?3, wilaya_id = ?4, commune = ?5,
                 address = ?6, customer_notes = ?7, delivery_type = COALESCE(?8, delivery_type),
                 payment_method = COALESCE(?9, payment_method)
             WHERE id = ?1 AND personal_data_purged_at IS NULL",
            params![
                id,
                remote.customer_name,
                phone,
                wilaya,
                remote.commune,
                remote.address,
                remote.customer_notes,
                known_delivery,
                remote.payment_method
            ],
        )?;
    }
    if state.open() && matches!(status, "cancelled" | "returned" | "delivered") {
        tx.execute(
            "UPDATE online_orders SET local_state = 'closed', closed_at = ?2 WHERE id = ?1",
            params![id, now],
        )?;
        if state != OrderState::New {
            store::raise_notice(
                tx,
                CloudNoticeKind::Order,
                "online_order_cancelled",
                &remote.order_number,
                json!({ "status": status }),
                now,
            )?;
        }
        return Ok(UpsertOutcome::Closed);
    }
    if state.open() {
        let before = lines_signature(tx, &id)?;
        replace_lines(tx, &link.store_id, &id, remote)?;
        if state == OrderState::Prepared && lines_signature(tx, &id)? != before {
            tx.execute(
                "UPDATE online_orders SET local_state = 'claimed' WHERE id = ?1",
                [&id],
            )?;
            store::raise_notice(
                tx,
                CloudNoticeKind::Order,
                "online_order_changed",
                &remote.order_number,
                json!({}),
                now,
            )?;
        }
    }
    Ok(UpsertOutcome::Updated)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OrderFilter {
    /// New, claimed or prepared.
    Open,
    /// Claimed or prepared by this till.
    Mine,
    /// Fulfilled, refused or closed.
    Done,
    All,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OrderQuery {
    pub filter: OrderFilter,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OnlineOrderSummary {
    pub id: String,
    pub order_number: String,
    pub remote_status: String,
    pub local_state: OrderState,
    pub delivery_type: String,
    pub payment_method: String,
    /// DZBuild's payment status as sent (`pending`, `paid`…); none before migration 0009.
    pub payment_status: Option<String>,
    pub customer_name: Option<String>,
    pub wilaya_id: Option<i64>,
    pub total: Money,
    pub items: i64,
    pub claimed_by_me: bool,
    pub claimed_elsewhere: bool,
    pub claim_confirmed: bool,
    pub remote_created_at: i64,
    pub seen: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OnlineOrderLine {
    pub line_no: i64,
    pub product_id: Option<String>,
    pub product_name: String,
    pub variant_name: Option<String>,
    pub sku: Option<String>,
    pub unit_price: Money,
    pub qty: Qty,
    pub total: Money,
    /// The shop's price today, to show a difference before selling.
    pub local_price: Option<Money>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OnlineOrder {
    pub id: String,
    pub remote_id: String,
    pub order_number: String,
    pub remote_status: String,
    pub local_state: OrderState,
    pub delivery_type: String,
    pub payment_method: String,
    /// DZBuild's payment status as sent (`pending`, `paid`…); none before migration 0009.
    pub payment_status: Option<String>,
    pub customer_name: Option<String>,
    pub customer_phone: Option<String>,
    pub wilaya_id: Option<i64>,
    pub commune: Option<String>,
    pub address: Option<String>,
    pub customer_notes: Option<String>,
    pub subtotal: Money,
    pub shipping_cost: Money,
    pub discount: Money,
    pub total: Money,
    pub claimed_by_me: bool,
    pub claimed_elsewhere: bool,
    pub claim_confirmed: bool,
    pub local_document_id: Option<String>,
    pub local_document_number: Option<String>,
    pub refuse_reason: Option<String>,
    pub remote_created_at: i64,
    pub lines: Vec<OnlineOrderLine>,
}

const SUMMARY_SELECT: &str = "SELECT o.id, o.order_number, o.remote_status, o.local_state, o.delivery_type, o.payment_method,
        o.customer_name, o.wilaya_id, o.total, (SELECT COUNT(*) FROM online_order_lines l WHERE l.order_id = o.id),
        o.claimed_by_terminal, o.claim_confirmed, o.remote_created_at, o.seen_at IS NOT NULL, o.payment_status
     FROM online_orders o";

fn map_summary(r: &Row<'_>, terminal_id: &str) -> rusqlite::Result<OnlineOrderSummary> {
    let claimed_by: Option<String> = r.get(10)?;
    Ok(OnlineOrderSummary {
        id: r.get(0)?,
        order_number: r.get(1)?,
        remote_status: r.get(2)?,
        local_state: r.get(3)?,
        delivery_type: r.get(4)?,
        payment_method: r.get(5)?,
        payment_status: r.get(14)?,
        customer_name: r.get(6)?,
        wilaya_id: r.get(7)?,
        total: r.get(8)?,
        items: r.get(9)?,
        claimed_by_me: claimed_by.as_deref() == Some(terminal_id),
        claimed_elsewhere: claimed_by.as_deref().is_some_and(|t| t != terminal_id),
        claim_confirmed: r.get(11)?,
        remote_created_at: r.get(12)?,
        seen: r.get(13)?,
    })
}

pub fn list(
    conn: &Connection,
    query: &OrderQuery,
    terminal_id: &str,
) -> Result<Vec<OnlineOrderSummary>, AppError> {
    // constant clauses only; values are bound
    let clause = match query.filter {
        OrderFilter::Open => "o.local_state IN ('new', 'claimed', 'prepared')",
        OrderFilter::Mine => {
            "o.local_state IN ('claimed', 'prepared') AND o.claimed_by_terminal = ?2"
        }
        OrderFilter::Done => "o.local_state IN ('fulfilled', 'refused', 'closed')",
        OrderFilter::All => "1 = 1",
    };
    let sql = format!("{SUMMARY_SELECT} WHERE {clause} ORDER BY o.remote_created_at DESC LIMIT ?1");
    let limit = i64::from(query.limit.unwrap_or(200).clamp(1, 500));
    let mut stmt = conn.prepare(&sql)?;
    let map = |r: &Row<'_>| map_summary(r, terminal_id);
    let rows = if query.filter == OrderFilter::Mine {
        stmt.query_map(params![limit, terminal_id], map)?
            .collect::<Result<Vec<_>, _>>()?
    } else {
        stmt.query_map(params![limit], map)?
            .collect::<Result<Vec<_>, _>>()?
    };
    Ok(rows)
}

pub fn get(conn: &Connection, id: &str, terminal_id: &str) -> Result<OnlineOrder, AppError> {
    let order = conn
        .query_row(
            "SELECT o.id, o.remote_id, o.order_number, o.remote_status, o.local_state, o.delivery_type, o.payment_method,
                    o.customer_name, o.customer_phone, o.wilaya_id, o.commune, o.address, o.customer_notes, o.subtotal,
                    o.shipping_cost, o.discount, o.total, o.claimed_by_terminal, o.claim_confirmed, o.local_document_id,
                    d.number, o.refuse_reason, o.remote_created_at, o.payment_status
             FROM online_orders o LEFT JOIN documents d ON d.id = o.local_document_id WHERE o.id = ?1",
            [id],
            |r| {
                let claimed_by: Option<String> = r.get(17)?;
                Ok(OnlineOrder {
                    id: r.get(0)?,
                    remote_id: r.get(1)?,
                    order_number: r.get(2)?,
                    remote_status: r.get(3)?,
                    local_state: r.get(4)?,
                    delivery_type: r.get(5)?,
                    payment_method: r.get(6)?,
                    payment_status: r.get(23)?,
                    customer_name: r.get(7)?,
                    customer_phone: r.get(8)?,
                    wilaya_id: r.get(9)?,
                    commune: r.get(10)?,
                    address: r.get(11)?,
                    customer_notes: r.get(12)?,
                    subtotal: r.get(13)?,
                    shipping_cost: r.get(14)?,
                    discount: r.get(15)?,
                    total: r.get(16)?,
                    claimed_by_me: claimed_by.as_deref() == Some(terminal_id),
                    claimed_elsewhere: claimed_by.as_deref().is_some_and(|t| t != terminal_id),
                    claim_confirmed: r.get(18)?,
                    local_document_id: r.get(19)?,
                    local_document_number: r.get(20)?,
                    refuse_reason: r.get(21)?,
                    remote_created_at: r.get(22)?,
                    lines: Vec::new(),
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("cloud_order"))?;
    let mut stmt = conn.prepare(
        "SELECT l.line_no, l.product_id, l.product_name, l.variant_name, l.sku, l.unit_price, l.qty, l.total, p.sell_price
         FROM online_order_lines l LEFT JOIN products p ON p.id = l.product_id WHERE l.order_id = ?1 ORDER BY l.line_no",
    )?;
    let lines = stmt
        .query_map([id], |r| {
            Ok(OnlineOrderLine {
                line_no: r.get(0)?,
                product_id: r.get(1)?,
                product_name: r.get(2)?,
                variant_name: r.get(3)?,
                sku: r.get(4)?,
                unit_price: r.get(5)?,
                qty: r.get(6)?,
                total: r.get(7)?,
                local_price: r.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(OnlineOrder { lines, ..order })
}

pub fn unseen_count(conn: &Connection) -> Result<i64, AppError> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM online_orders WHERE seen_at IS NULL AND local_state = 'new'",
        [],
        |r| r.get(0),
    )?)
}

pub fn mark_seen(tx: &Transaction<'_>, ids: &[String], now: i64) -> Result<usize, AppError> {
    let mut n = 0;
    for id in ids {
        n += tx.execute(
            "UPDATE online_orders SET seen_at = ?2 WHERE id = ?1 AND seen_at IS NULL",
            params![id, now],
        )?;
    }
    Ok(n)
}

fn load(tx: &Transaction<'_>, ctx: &TillContext, id: &str) -> Result<OnlineOrder, AppError> {
    get(tx, id, &ctx.session.terminal_id)
}

fn require(order: &OnlineOrder, states: &[OrderState], mine: bool) -> Result<(), AppError> {
    if order.claimed_elsewhere && mine {
        return Err(AppError::sync("cloud_order_claimed_elsewhere"));
    }
    if !states.contains(&order.local_state) || (mine && !order.claimed_by_me) {
        return Err(AppError::sync("cloud_order_state_invalid"));
    }
    Ok(())
}

/// One order update for DZBuild, sent by the push in order. An update queued behind this till's
/// own claim (pending, or failed and waiting for the owner) depends on it: it goes only once
/// DZBuild accepted the claim, and it is skipped with the claim when DZBuild gives the order to
/// another device (`sync_outbox_skip_dependants`), so this till never tells DZBuild it prepares,
/// delivers, ships or cancels an order another device prepares.
fn tell(
    tx: &Transaction<'_>,
    link: &CloudLink,
    kind: OutboxKind,
    order: &OnlineOrder,
    step: &str,
    request: Value,
    now: i64,
) -> Result<(), AppError> {
    let claim: Option<String> = if kind == OutboxKind::OrderClaim {
        None
    } else {
        tx.query_row(
            "SELECT id FROM sync_outbox
             WHERE target = 'cloud' AND kind = ?1 AND entity_id = ?2 AND status IN ('pending', 'failed')
             ORDER BY created_at DESC, rowid DESC LIMIT 1",
            params![OutboxKind::OrderClaim, order.id],
            |r| r.get(0),
        )
        .optional()?
    };
    outbox::enqueue(
        tx,
        NewItem {
            store_id: link.store_id.clone(),
            kind,
            entity_id: order.id.clone(),
            payload: request,
            key: super::outbox_key("order", &format!("{}:{step}", order.id)),
            depends_on: claim,
        },
        now,
    )?;
    Ok(())
}

fn journal(
    tx: &Transaction<'_>,
    actor: &Actor,
    action: &str,
    order: &OnlineOrder,
    extra: Value,
) -> Result<(), AppError> {
    let mut payload = json!({ "orderNumber": order.order_number, "remoteId": order.remote_id });
    if let (Some(map), Value::Object(more)) = (payload.as_object_mut(), extra) {
        map.extend(more);
    }
    audit::append(
        tx,
        &actor.event(action, "online_order", Some(&order.id), payload),
    )?;
    Ok(())
}

/// Takes the order for this till. `confirmed_online`: DZBuild already accepted the claim (the
/// command tried first); otherwise the claim is queued and a refusal reverts it.
pub fn claim(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    link: &CloudLink,
    id: &str,
    confirmed_online: bool,
) -> Result<OnlineOrder, AppError> {
    let order = load(tx, ctx, id)?;
    require(&order, &[OrderState::New], false)?;
    if order.claimed_elsewhere {
        return Err(AppError::sync("cloud_order_claimed_elsewhere"));
    }
    tx.execute(
        "UPDATE online_orders SET local_state = 'claimed', claimed_by_terminal = ?2, claimed_by_user = ?3,
             claimed_at = ?4, claim_confirmed = ?5, seen_at = COALESCE(seen_at, ?4), updated_at = ?4 WHERE id = ?1",
        params![
            id,
            ctx.session.terminal_id,
            ctx.session.user_id,
            ctx.now,
            confirmed_online
        ],
    )?;
    if !confirmed_online {
        let request = json!({
            "verb": "post",
            "path": format!("/orders/{}/claim", order.remote_id),
            "body": { "device_id": link.device_id, "terminal": ctx.session.terminal_code },
        });
        tell(
            tx,
            link,
            OutboxKind::OrderClaim,
            &order,
            "claim",
            request,
            ctx.now,
        )?;
    }
    journal(
        tx,
        &ctx.actor(),
        "cloud.order_claimed",
        &order,
        json!({ "confirmed": confirmed_online }),
    )?;
    load(tx, ctx, id)
}

/// DZBuild refused a queued claim (another device prepares it). A claimed or prepared order goes
/// back to new: this till no longer sells or ships it. One this till already sold or shipped
/// keeps its validated document, and the owner gets a notice of its own: the other device may
/// prepare it too. Either way the order's updates queued behind the claim are skipped with it
/// (see [`tell`]).
pub fn claim_refused(tx: &Transaction<'_>, id: &str, now: i64) -> Result<(), AppError> {
    let Some((number, state, confirmed)) = tx
        .query_row(
            "SELECT order_number, local_state, claim_confirmed FROM online_orders WHERE id = ?1",
            [id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, OrderState>(1)?,
                    r.get::<_, bool>(2)?,
                ))
            },
        )
        .optional()?
    else {
        return Ok(());
    };
    let code = match state {
        OrderState::Claimed | OrderState::Prepared if !confirmed => {
            tx.execute(
                "UPDATE online_orders SET local_state = 'new', claimed_by_terminal = NULL, claimed_by_user = NULL,
                     claimed_at = NULL, updated_at = ?2 WHERE id = ?1",
                params![id, now],
            )?;
            "cloud_order_claimed_elsewhere"
        }
        OrderState::Fulfilled if !confirmed => "online_order_fulfilled_claimed_elsewhere",
        _ => "cloud_order_claimed_elsewhere",
    };
    store::raise_notice(tx, CloudNoticeKind::Order, code, &number, json!({}), now)?;
    Ok(())
}

/// Gives a claimed order back to the queue (local; DZBuild keeps its claim until the order moves).
pub fn release(tx: &Transaction<'_>, ctx: &TillContext, id: &str) -> Result<OnlineOrder, AppError> {
    let order = load(tx, ctx, id)?;
    require(&order, &[OrderState::Claimed, OrderState::Prepared], true)?;
    tx.execute(
        "UPDATE online_orders SET local_state = 'new', claimed_by_terminal = NULL, claimed_by_user = NULL,
             claimed_at = NULL, updated_at = ?2 WHERE id = ?1",
        params![id, ctx.now],
    )?;
    journal(tx, &ctx.actor(), "cloud.order_released", &order, json!({}))?;
    load(tx, ctx, id)
}

pub fn prepare(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    link: &CloudLink,
    id: &str,
) -> Result<OnlineOrder, AppError> {
    let order = load(tx, ctx, id)?;
    require(&order, &[OrderState::Claimed], true)?;
    tx.execute(
        "UPDATE online_orders SET local_state = 'prepared', updated_at = ?2 WHERE id = ?1",
        params![id, ctx.now],
    )?;
    let request = json!({ "verb": "patch", "path": format!("/orders/{}", order.remote_id), "body": { "status": "processing" } });
    tell(
        tx,
        link,
        OutboxKind::OrderStatus,
        &order,
        "processing",
        request,
        ctx.now,
    )?;
    journal(tx, &ctx.actor(), "cloud.order_prepared", &order, json!({}))?;
    load(tx, ctx, id)
}

/// A consumer amount from DZBuild (TTC) in the shop's price mode: as is in a TTC shop, back to HT
/// at `rate` in an HT shop.
fn in_shop_mode(amount: Money, mode: PriceMode, rate: Rate) -> Money {
    match mode {
        PriceMode::Ttc => amount,
        PriceMode::Ht => amount
            .mul_div(10_000, 10_000 + i64::from(rate.0))
            .unwrap_or(amount),
    }
}

/// PRM-002's rule for the order's discount (TTC, as DZBuild gives it): each line's share in
/// proportion to its amount, rounded on the running total, so the shares add up to the discount
/// exactly and none exceeds its line; `discount_invalid` when the discount exceeds the lines.
fn spread(amount: Money, grosses: &[Money]) -> Result<Vec<Money>, AppError> {
    let invalid = || AppError::validation("discount", "discount_invalid");
    let total = Money::checked_sum(grosses.iter().copied()).ok_or_else(invalid)?;
    if amount > total {
        return Err(invalid());
    }
    let (mut running, mut given) = (Money::ZERO, Money::ZERO);
    let mut shares = Vec::with_capacity(grosses.len());
    for gross in grosses {
        running = running.checked_add(*gross).ok_or_else(invalid)?;
        let upto = amount.mul_div(running.0, total.0).ok_or_else(invalid)?;
        shares.push(upto.checked_sub(given).ok_or_else(invalid)?);
        given = upto;
    }
    Ok(shares)
}

/// The order as a cart at the online prices: an override only where the online price differs
/// from what the shop charges a consumer (compared in TTC); the order's discount spread over the
/// lines in proportion to their amounts ([`spread`]); the delivery fee as an open line labelled
/// `shipping_label` (the till's words, in the till's language) at 19 %. In an HT shop the online
/// TTC amounts go back to HT at each line's rate.
///
/// The till's rules apply to these amounts as to any cart: a price that differs needs
/// `sales.price_override`, and so does the delivery fee (an open item); the discount needs
/// `sales.discount` within the discount limit. An operator without the right gets the till's
/// usual refusal (`price_override_not_allowed`, `free_line_not_allowed`, `discount_not_allowed`,
/// `discount_above_limit`) and a manager approves it through the till's approval flow (the
/// command keeps the grant in the `TillContext`; the screen asks for a discount, then a price
/// override), exactly as for a price or a discount typed at the till.
pub fn cart_for(
    conn: &Connection,
    ctx: &TillContext,
    order: &OnlineOrder,
    shipping_label: &str,
) -> Result<Cart, AppError> {
    let unmapped: Vec<&str> = order
        .lines
        .iter()
        .filter(|l| l.product_id.is_none())
        .map(|l| l.product_name.as_str())
        .collect();
    if !unmapped.is_empty() {
        return Err(AppError::rejected(
            json!({ "products": unmapped }),
            "cloud_order_unmapped_product",
        ));
    }
    let mode = crate::company::get(conn)?
        .ok_or_else(|| AppError::conflict("setup_required"))?
        .price_mode;
    let too_large = || AppError::validation("amount", "amount_invalid");
    let mut lines: Vec<CartLine> = Vec::new();
    // Per line: the online (TTC) gross, the gross in the shop's mode, the rate.
    let mut grosses: Vec<(Money, Money, Rate)> = Vec::new();
    for l in &order.lines {
        let Some(product_id) = l.product_id.as_deref() else {
            continue;
        };
        let product = products::get(conn, product_id)?;
        let mut line = CartLine::product(product_id, l.qty);
        let unit = in_shop_mode(l.unit_price, mode, product.tax_rate_bp);
        if super::payloads::consumer_price(&product, mode)? != l.unit_price {
            line.unit_price = Some(unit);
        }
        grosses.push((
            l.unit_price.mul_qty(l.qty).ok_or_else(too_large)?,
            unit.mul_qty(l.qty).ok_or_else(too_large)?,
            product.tax_rate_bp,
        ));
        lines.push(line);
    }
    if order.discount.is_positive() {
        let online: Vec<Money> = grosses.iter().map(|g| g.0).collect();
        let shares = spread(order.discount, &online)?;
        for (line, (share, (_, gross, rate))) in
            lines.iter_mut().zip(shares.into_iter().zip(&grosses))
        {
            if share.is_positive() {
                // back to HT, never more than the line (a rounding at 100 %)
                line.discount = Discount::Amount(in_shop_mode(share, mode, *rate).min(*gross));
            }
        }
    }
    if order.shipping_cost.is_positive() {
        let rate = crate::catalog::tax_rates::by_code(conn, "TVA19")?
            .ok_or_else(|| AppError::not_found("tax_rate"))?;
        let fee = in_shop_mode(order.shipping_cost, mode, rate.rate_bp);
        lines.push(CartLine::open_item(shipping_label, Qty::ONE, fee, &rate.id));
    }
    let lang = ctx.session.lang;
    Ok(Cart {
        note: Some(format!("#{}", order.order_number)),
        ..Cart::new(lang, lines)
    })
}

/// The consignee of a delivery note: the customer chosen at the till, else the one with the
/// order's phone, else a new customer made from the order (name, phone, address, wilaya). Only
/// the cashier's "ship" creates it: the shop's own record of whom it delivers to.
fn consignee(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    order: &OnlineOrder,
    chosen: Option<String>,
) -> Result<String, AppError> {
    use crate::customers::{self, CustomerInput};
    if let Some(id) = chosen {
        return customers::get(tx, &id).map(|c| c.id);
    }
    if let Some(phone) = order.customer_phone.as_deref()
        && let Some(found) = customers::find_by_phone(tx, phone)?
    {
        return Ok(found.id);
    }
    let name = order
        .customer_name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| format!("DZBuild #{}", order.order_number));
    let input = CustomerInput {
        phone: order.customer_phone.clone(),
        address: order.address.clone(),
        wilaya: order.wilaya_id.map(|w| format!("{w:02}")),
        ..CustomerInput::named(&name)
    };
    Ok(customers::create(tx, &ctx.actor(), &ctx.pack, input)?.id)
}

pub fn quote(
    conn: &Connection,
    ctx: &TillContext,
    id: &str,
    shipping_label: &str,
    tenders: &[PaymentInput],
) -> Result<PaymentPreview, AppError> {
    let order = get(conn, id, &ctx.session.terminal_id)?;
    let cart = cart_for(conn, ctx, &order, shipping_label)?;
    sales::preview_payment(conn, ctx, &cart, tenders)
}

/// DZBuild already took the stock of this order: acknowledge it for every line's product.
fn acknowledge_stock(
    tx: &Transaction<'_>,
    link: &CloudLink,
    order: &OnlineOrder,
    now: i64,
) -> Result<(), AppError> {
    for line in &order.lines {
        if let (Some(product_id), Some(pieces)) = (
            line.product_id.as_deref(),
            super::wire::whole_pieces(line.qty),
        ) {
            super::stock::acknowledge(tx, &link.store_id, product_id, -pieces, now)?;
        }
    }
    Ok(())
}

fn fulfilled(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    link: &CloudLink,
    order: &OnlineOrder,
    document: &Document,
    remote_status: &str,
) -> Result<(), AppError> {
    tx.execute(
        "UPDATE online_orders SET local_state = 'fulfilled', local_document_id = ?2, closed_at = ?3, updated_at = ?3 WHERE id = ?1",
        params![order.id, document.id, ctx.now],
    )?;
    acknowledge_stock(tx, link, order, ctx.now)?;
    let request = json!({ "verb": "patch", "path": format!("/orders/{}", order.remote_id), "body": { "status": remote_status } });
    tell(
        tx,
        link,
        OutboxKind::OrderStatus,
        order,
        remote_status,
        request,
        ctx.now,
    )?;
    journal(
        tx,
        &ctx.actor(),
        "cloud.order_fulfilled",
        order,
        json!({ "document": document.number, "status": remote_status }),
    )
}

/// The buyer paid DZBuild online (any method but cash on delivery): the till never takes their
/// money again.
pub fn is_prepaid(order: &OnlineOrder) -> bool {
    !order.payment_method.trim().eq_ignore_ascii_case("cod")
}

/// `sell` and `ship` take payment (a ticket) or leave it to collect (a BL): never for an order
/// already paid online (`cloud_order_prepaid`), which is handed over ([`hand_over`]).
fn refuse_prepaid(order: &OnlineOrder) -> Result<(), AppError> {
    if is_prepaid(order) {
        return Err(AppError::rejected(
            json!({ "paymentMethod": order.payment_method }),
            "cloud_order_prepaid",
        ));
    }
    Ok(())
}

/// What the buyer paid online for this order, as the tender of its ticket: a transfer (the money
/// reaches the shop from DZBuild's payment provider) referencing the order, so no cash enters
/// the drawer and no droit de timbre is due (it is on cash only).
fn paid_online(order: &OnlineOrder, amount: Money) -> PaymentInput {
    PaymentInput {
        method: crate::payments::PaymentMethod::Transfer,
        amount,
        reference: Some(format!("DZBuild #{}", order.order_number)),
        cheque_bank: None,
        cheque_date: None,
    }
}

/// An order paid online is handed over, at the shop (`courier` false: DZBuild is told
/// `delivered`) or to a courier (`shipped`): a ticket at the online prices settled by what the
/// buyer already paid ([`paid_online`]), never cash. A free order has no tender.
pub fn hand_over(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    link: &CloudLink,
    id: &str,
    shipping_label: String,
    courier: bool,
) -> Result<ValidatedSale, AppError> {
    let order = load(tx, ctx, id)?;
    require(&order, &[OrderState::Claimed, OrderState::Prepared], true)?;
    if !is_prepaid(&order) {
        // cash on delivery: sold at the till or shipped with a BL
        return Err(AppError::sync("cloud_order_state_invalid"));
    }
    let cart = cart_for(tx, ctx, &order, &shipping_label)?;
    let total = sales::preview_payment(tx, ctx, &cart, &[])?
        .settlement
        .total_ttc;
    let payments = if total.is_positive() {
        vec![paid_online(&order, total)]
    } else {
        Vec::new()
    };
    let sale = sales::create_sale(tx, ctx, SaleInput { cart, payments })?;
    let status = if courier { "shipped" } else { "delivered" };
    fulfilled(tx, ctx, link, &order, &sale.document, status)?;
    Ok(sale)
}

/// Picked up at the shop, or cash brought back by the courier: a ticket (droit de timbre on the
/// cash part, like any sale), then DZBuild is told `delivered`. Cash on delivery only.
pub fn sell(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    link: &CloudLink,
    id: &str,
    shipping_label: String,
    payments: Vec<PaymentInput>,
) -> Result<ValidatedSale, AppError> {
    sell_with(
        tx,
        ctx,
        link,
        id,
        shipping_label,
        payments,
        &sales::SaleExtras::default(),
    )
}

/// [`sell`] with the till's pack extras of the sale (register entries, serials, buyer, payment
/// types): the order's ticket refuses or records them exactly like a till sale.
pub fn sell_with(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    link: &CloudLink,
    id: &str,
    shipping_label: String,
    payments: Vec<PaymentInput>,
    extras: &sales::SaleExtras,
) -> Result<ValidatedSale, AppError> {
    let order = load(tx, ctx, id)?;
    require(&order, &[OrderState::Claimed, OrderState::Prepared], true)?;
    refuse_prepaid(&order)?;
    let cart = cart_for(tx, ctx, &order, &shipping_label)?;
    let sale = sales::create_sale_with(tx, ctx, SaleInput { cart, payments }, extras)?;
    fulfilled(tx, ctx, link, &order, &sale.document, "delivered")?;
    Ok(sale)
}

/// Handed to a courier: a validated delivery note (M3) to the consignee, no payment now (cash
/// on delivery, settled later on the customer's account or by an invoice from the BL), then
/// DZBuild is told `shipped`. Cash on delivery only: an order paid online is handed over.
pub fn ship(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    link: &CloudLink,
    id: &str,
    shipping_label: String,
    customer_id: Option<String>,
) -> Result<Document, AppError> {
    use crate::billing::drafts::{DraftInput, save_draft};
    use crate::billing::validate::{ValidateInput, validate};
    let order = load(tx, ctx, id)?;
    require(&order, &[OrderState::Claimed, OrderState::Prepared], true)?;
    refuse_prepaid(&order)?;
    let mut cart = cart_for(tx, ctx, &order, &shipping_label)?;
    cart.customer_id = Some(consignee(tx, ctx, &order, customer_id)?);
    let draft = save_draft(
        tx,
        ctx,
        None,
        DraftInput {
            doc_type: crate::documents::DocType::DeliveryNote,
            cart,
            valid_until: None,
            due_date: None,
            second_lang: None,
            draft_id: None,
        },
    )?;
    let validated = validate(
        tx,
        ctx,
        ValidateInput {
            id: draft.id.clone(),
            payments: Vec::new(),
        },
    )?;
    fulfilled(tx, ctx, link, &order, &validated.document, "shipped")?;
    Ok(validated.document)
}

pub fn refuse(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    link: &CloudLink,
    id: &str,
    reason: RefuseReason,
    note: Option<String>,
) -> Result<OnlineOrder, AppError> {
    let order = load(tx, ctx, id)?;
    require(
        &order,
        &[OrderState::New, OrderState::Claimed, OrderState::Prepared],
        false,
    )?;
    if order.claimed_elsewhere {
        return Err(AppError::sync("cloud_order_claimed_elsewhere"));
    }
    let note = crate::catalog::clean_opt(note.as_deref(), "note", 200)?;
    tx.execute(
        "UPDATE online_orders SET local_state = 'refused', refuse_reason = ?2, closed_at = ?3, updated_at = ?3 WHERE id = ?1",
        params![id, reason.as_str(), ctx.now],
    )?;
    let request = json!({
        "verb": "post",
        "path": format!("/orders/{}/cancel", order.remote_id),
        "body": { "reason": reason.as_str(), "note": note },
    });
    tell(
        tx,
        link,
        OutboxKind::OrderCancel,
        &order,
        "cancel",
        request,
        ctx.now,
    )?;
    journal(
        tx,
        &ctx.actor(),
        "cloud.order_refused",
        &order,
        json!({ "reason": reason.as_str() }),
    )?;
    load(tx, ctx, id)
}

/// An order never closed here (claimed and forgotten, its store unlinked) loses its buyer's
/// details this many days past the retention, counted from DZBuild's last change of it.
pub const OPEN_ORDER_MARGIN_DAYS: i64 = 30;

const DAY_MS: i64 = 86_400_000;

/// loi 18-07: the buyer's name, phone, address and notes go `retention_days` after the order
/// closed, or [`OPEN_ORDER_MARGIN_DAYS`] later after DZBuild's last change of an order that never
/// closed; the order itself (number, amounts, lines, the document) stays. Each run that purged
/// something is journaled with its count (PDP-003).
pub fn purge_personal_data(
    tx: &Transaction<'_>,
    retention_days: i64,
    now: i64,
) -> Result<usize, AppError> {
    let purged = tx.execute(
        "UPDATE online_orders SET customer_name = NULL, customer_phone = NULL, address = NULL, commune = NULL,
             customer_notes = NULL, personal_data_purged_at = ?1
         WHERE personal_data_purged_at IS NULL
           AND ((closed_at IS NOT NULL AND closed_at < ?2) OR remote_updated_at < ?3)",
        params![
            now,
            now - retention_days * DAY_MS,
            now - (retention_days + OPEN_ORDER_MARGIN_DAYS) * DAY_MS
        ],
    )?;
    if purged > 0 {
        audit::append(
            tx,
            &Actor::system(None).event(
                "cloud.personal_data_purged",
                "online_order",
                None,
                json!({ "orders": purged, "retentionDays": retention_days }),
            ),
        )?;
    }
    Ok(purged)
}

/// The store is unlinked: its orders still open here are closed (no till can take them any
/// more) so their buyers' details follow the retention, and DZBuild's buyer flags for it go.
pub fn close_unlinked_store(
    tx: &Transaction<'_>,
    store_id: &str,
    now: i64,
) -> Result<usize, AppError> {
    let closed = tx.execute(
        "UPDATE online_orders SET local_state = 'closed', closed_at = ?2, updated_at = ?2
         WHERE store_id = ?1 AND closed_at IS NULL AND local_state IN ('new', 'claimed', 'prepared')",
        params![store_id, now],
    )?;
    tx.execute(
        "DELETE FROM remote_customer_flags WHERE store_id = ?1",
        [store_id],
    )?;
    Ok(closed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::derive::derive;
    use crate::cloud::store::cloud_settings;
    use crate::cloud::testkit::{linked_ht_shop, linked_shop, outbox_rows, publish};
    use crate::money::{Money, Qty};
    use crate::testkit::{NOON, Shop};
    use serde_json::json;

    fn order_json(external_id: &str, price: i64, status: &str) -> serde_json::Value {
        json!({
            "id": 9001, "order_number": 1042, "status": status, "payment_method": "cod", "delivery_type": "pickup",
            "customer_name": "Samira B.", "customer_phone": "0550 12 34 56", "wilaya_id": 16, "commune": "Bab Ezzouar",
            "address": "Cité 5 juillet", "customer_notes": "après 17h",
            "subtotal": price * 2, "shipping_cost": 0, "discount": 0, "total": price * 2,
            "created_at": "2026-09-28 10:00:00", "updated_at": "2026-09-28 10:00:00",
            "items": [
                { "product_id": 88121, "external_id": external_id, "product_name": "Lait", "price": price, "quantity": 2, "total": price * 2 }
            ]
        })
    }

    /// Amounts are read from the raw JSON text (ApiMoney), so parse text, not a Value.
    fn parse(value: &serde_json::Value) -> RemoteOrder {
        serde_json::from_str(&value.to_string()).unwrap()
    }

    fn remote_order(external_id: &str, price: i64, status: &str) -> RemoteOrder {
        parse(&order_json(external_id, price, status))
    }

    /// The cashier's till with a manager's (the owner's) approval of each of `permissions`.
    fn approved(shop: &Shop, permissions: &[crate::auth::Permission]) -> TillContext {
        permissions.iter().fold(shop.till(), |till, permission| {
            till.with_approval(crate::auth::session::OverrideGrant {
                approver_id: shop.owner.user_id.clone(),
                approver_name: shop.owner.display_name.clone(),
                permission: *permission,
                granted_at: NOON,
                discount_limit_bp: 10_000,
            })
        })
    }

    fn sell_as(
        shop: &Shop,
        link: &CloudLink,
        till: TillContext,
        id: &str,
    ) -> Result<ValidatedSale, AppError> {
        let (l, i) = (link.clone(), id.to_owned());
        shop.db.write(move |tx| {
            sell(
                tx,
                &till,
                &l,
                &i,
                "Livraison".into(),
                vec![crate::payments::PaymentInput::cash(Money(1_000_000))],
            )
        })
    }

    fn claimed(shop: &Shop, link: &CloudLink, order: RemoteOrder) -> String {
        let (id, _) = mirror(shop, link, order);
        let (till, l, i) = (shop.till(), link.clone(), id.clone());
        shop.db
            .write(move |tx| claim(tx, &till, &l, &i, true).map(drop))
            .unwrap();
        id
    }

    #[test]
    fn a_delivery_fee_is_an_open_line_a_manager_approves_like_any_open_item() {
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        shop.stock_in(&milk.id, 10_000);
        shop.open_till(0);
        let mut value = order_json(&milk.id, 100, "confirmed");
        value["delivery_type"] = json!("home");
        value["shipping_cost"] = json!(400);
        value["total"] = json!(600);
        let id = claimed(&shop, &link, parse(&value));
        let refused = sell_as(&shop, &link, shop.till(), &id).unwrap_err();
        assert_eq!(
            refused.code(),
            "free_line_not_allowed",
            "DZBuild's fee is an open line: the till asks a manager, as for any open item"
        );
        let till = approved(&shop, &[crate::auth::Permission::SalesPriceOverride]);
        let (ctx, i) = (till.clone(), id.clone());
        let preview = shop
            .db
            .read(move |c| quote(c, &ctx, &i, "Livraison", &[]))
            .unwrap();
        assert_eq!(preview.cart.totals.total_ttc, Money(60_000));
        let sale = sell_as(&shop, &link, till, &id).unwrap();
        assert_eq!(sale.document.total_ttc, Money(60_000), "the order's total");
        let fee = sale
            .document
            .lines
            .iter()
            .find(|l| l.product_id.is_none())
            .expect("the delivery line");
        assert_eq!(
            (fee.description.as_str(), fee.total_ttc, fee.tax_rate_bp),
            ("Livraison", Money(40_000), Rate(1_900))
        );
    }

    #[test]
    fn the_orders_discount_is_spread_over_its_lines_and_a_manager_approves_it_like_any_discount() {
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        let bread = shop.product("Pain", Money(5_000), &shop.tva9);
        shop.stock_in(&milk.id, 10_000);
        shop.stock_in(&bread.id, 10_000);
        shop.open_till(0);
        // 225 DA off 300 DA: more than the largest line (200 DA) alone can take
        let mut value = order_json(&milk.id, 100, "confirmed");
        value["items"] = json!([
            { "product_id": 88121, "external_id": milk.id, "product_name": "Lait", "price": 100, "quantity": 2, "total": 200 },
            { "product_id": 88122, "external_id": bread.id, "product_name": "Pain", "price": 50, "quantity": 2, "total": 100 }
        ]);
        value["subtotal"] = json!(300);
        value["discount"] = json!(225);
        value["total"] = json!(75);
        let id = claimed(&shop, &link, parse(&value));
        let refused = sell_as(&shop, &link, shop.till(), &id).unwrap_err();
        assert_eq!(
            refused.code(),
            "discount_not_allowed",
            "the till asks a manager, as for any discount"
        );
        let till = approved(&shop, &[crate::auth::Permission::SalesDiscount]);
        let sale = sell_as(&shop, &link, till, &id).unwrap();
        assert_eq!(sale.document.total_ttc, Money(7_500), "the order's total");
        let discounts: Vec<Money> = sale
            .document
            .lines
            .iter()
            .map(|l| l.discount_amount)
            .collect();
        assert_eq!(
            discounts,
            [Money(15_000), Money(7_500)],
            "in proportion to the lines, adding up to the order's discount"
        );
    }

    #[test]
    fn in_an_ht_shop_the_ticket_comes_to_the_orders_total() {
        let (shop, link) = linked_ht_shop();
        // 100.00 DA HT: 119.00 DA to a consumer, the price DZBuild shows
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        shop.stock_in(&milk.id, 10_000);
        shop.open_till(0);
        let mut value = order_json(&milk.id, 119, "confirmed");
        value["shipping_cost"] = json!(500);
        value["discount"] = json!(19);
        value["total"] = json!(238 + 500 - 19);
        let id = claimed(&shop, &link, parse(&value));
        let till = approved(
            &shop,
            &[
                crate::auth::Permission::SalesPriceOverride,
                crate::auth::Permission::SalesDiscount,
            ],
        );
        let sale = sell_as(&shop, &link, till, &id).unwrap();
        assert_eq!(
            sale.document.total_ttc,
            Money(71_900),
            "the order's total, from HT amounts"
        );
        assert_eq!(
            (
                sale.document.lines[0].unit_price,
                sale.document.lines[0].price_overridden
            ),
            (Money(10_000), false),
            "the shop's HT price: no override where the consumer prices agree"
        );
    }

    #[test]
    fn an_open_order_takes_the_buyers_corrected_details_and_a_purged_one_never_gets_them_back() {
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        let (id, _) = mirror(&shop, &link, remote_order(&milk.id, 100, "pending"));
        // the merchant's confirmation call corrected the address, the phone and the delivery
        let mut value = order_json(&milk.id, 100, "confirmed");
        value["updated_at"] = json!("2026-09-28 10:30:00");
        value["customer_name"] = json!("Samira Benali");
        value["customer_phone"] = json!("0661 22 33 44");
        value["wilaya_id"] = json!(31);
        value["commune"] = json!("Bir El Djir");
        value["address"] = json!("Cité 400 logements");
        value["customer_notes"] = json!("avant midi");
        value["delivery_type"] = json!("home");
        value["payment_method"] = json!("free_digital");
        assert_eq!(
            mirror(&shop, &link, parse(&value)).1,
            UpsertOutcome::Updated
        );
        let (terminal, i) = (shop.terminal.id.clone(), id.clone());
        let order = shop.db.read(move |c| get(c, &i, &terminal)).unwrap();
        assert_eq!(
            (
                order.customer_name.as_deref(),
                order.customer_phone.as_deref(),
                order.wilaya_id,
                order.commune.as_deref(),
                order.address.as_deref(),
                order.customer_notes.as_deref(),
                order.delivery_type.as_str(),
                order.payment_method.as_str(),
            ),
            (
                Some("Samira Benali"),
                Some("0661223344"),
                Some(31),
                Some("Bir El Djir"),
                Some("Cité 400 logements"),
                Some("avant midi"),
                "home",
                "free_digital",
            )
        );
        // purged: a later copy of the order never brings the buyer back, whatever the row's
        // state (the row is left showing as open so that only the purge guard stands in the way)
        let purged = shop
            .db
            .write(move |tx| {
                tx.execute("UPDATE online_orders SET closed_at = ?1", [NOON])?;
                purge_personal_data(tx, 0, NOON + 1)
            })
            .unwrap();
        assert_eq!(purged, 1);
        value["updated_at"] = json!("2026-09-28 11:00:00");
        mirror(&shop, &link, parse(&value));
        let (terminal, i) = (shop.terminal.id.clone(), id);
        let order = shop.db.read(move |c| get(c, &i, &terminal)).unwrap();
        assert_eq!(
            (order.customer_name, order.customer_phone, order.address),
            (None, None, None)
        );
    }

    /// Migration 0009: DZBuild's `payment_status` is kept as sent and follows its
    /// later copies; a copy without it keeps the one known, and a value too long is not kept.
    #[test]
    fn the_payment_status_follows_dzbuild_and_shows_on_the_order() {
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        let (id, _) = mirror(&shop, &link, remote_order(&milk.id, 100, "pending"));
        let status = |shop: &Shop| {
            let (terminal, i) = (shop.terminal.id.clone(), id.clone());
            let order = shop.db.read(move |c| get(c, &i, &terminal)).unwrap();
            let terminal = shop.terminal.id.clone();
            let query = OrderQuery {
                filter: OrderFilter::All,
                limit: None,
            };
            let listed = shop.db.read(move |c| list(c, &query, &terminal)).unwrap();
            assert_eq!(listed[0].payment_status, order.payment_status);
            order.payment_status
        };
        assert_eq!(status(&shop), None, "a payload without one");
        let mut value = order_json(&milk.id, 100, "confirmed");
        for (minute, sent, kept) in [
            (10, Some("pending"), Some("pending")),
            (20, Some("paid"), Some("paid")),
            (30, None, Some("paid")),
            (40, Some(" "), Some("paid")),
        ] {
            value["updated_at"] = json!(format!("2026-09-28 10:{minute}:00"));
            match sent {
                Some(s) => value["payment_status"] = json!(s),
                None => drop(value.as_object_mut().unwrap().remove("payment_status")),
            }
            assert_eq!(
                mirror(&shop, &link, parse(&value)).1,
                UpsertOutcome::Updated
            );
            assert_eq!(status(&shop).as_deref(), kept, "{sent:?}");
        }
        let mut fresh = order_json(&milk.id, 100, "pending");
        fresh["id"] = json!(9002);
        fresh["payment_status"] = json!("x".repeat(41));
        let l = link.clone();
        let kept: Option<String> = shop
            .db
            .write(move |tx| {
                upsert_remote(tx, &l, &parse(&fresh), NOON)?;
                Ok(tx.query_row(
                    "SELECT payment_status FROM online_orders WHERE remote_id = '9002'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(kept, None, "never more than 40 characters");
    }

    /// An order the buyer paid online is never charged again: no cash ticket, no BL left to
    /// collect. It is handed over with a ticket settled by the online payment (a transfer
    /// referencing the order): no drawer movement, no droit de timbre, no customer balance.
    #[test]
    fn an_order_paid_online_is_handed_over_never_charged_again() {
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        shop.stock_in(&milk.id, 10_000);
        shop.open_till(0);
        let mut value = order_json(&milk.id, 100, "confirmed");
        value["payment_method"] = json!("digital_payment");
        let id = claimed(&shop, &link, parse(&value));
        assert_eq!(
            sell_as(&shop, &link, shop.till(), &id).unwrap_err().code(),
            "cloud_order_prepaid"
        );
        let (till, l, i) = (shop.owner_till(), link.clone(), id.clone());
        assert_eq!(
            shop.db
                .write(move |tx| ship(tx, &till, &l, &i, "Livraison".into(), None))
                .unwrap_err()
                .code(),
            "cloud_order_prepaid",
            "no BL leaving the buyer owing what they paid"
        );
        let drawer = "SELECT COALESCE(SUM(amount), 0) FROM cash_movements";
        let drawer_before = shop.count(drawer);
        let (till, l, i) = (shop.till(), link.clone(), id.clone());
        let sale = shop
            .db
            .write(move |tx| hand_over(tx, &till, &l, &i, "Livraison".into(), false))
            .unwrap();
        assert_eq!(sale.document.total_ttc, Money(20_000));
        assert_eq!(
            sale.settlement.stamp_duty,
            Money::ZERO,
            "no cash, no droit de timbre"
        );
        assert_eq!(sale.settlement.cash_tendered, Money::ZERO);
        assert_eq!(
            sale.document
                .payments
                .iter()
                .map(|p| (p.method, p.amount, p.reference.clone()))
                .collect::<Vec<_>>(),
            [(
                crate::payments::PaymentMethod::Transfer,
                Money(20_000),
                Some("DZBuild #1042".to_owned())
            )]
        );
        assert_eq!(
            shop.count(drawer),
            drawer_before,
            "nothing entered the drawer"
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM customer_ledger"),
            0,
            "no balance for the buyer"
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM online_orders WHERE local_state = 'fulfilled'"),
            1
        );
        // a cash-on-delivery order is not handed over: it is sold or shipped
        let mut cod = order_json(&milk.id, 100, "confirmed");
        cod["id"] = json!(9002);
        cod["order_number"] = json!(1043);
        let cod_id = {
            let (l, order) = (link.clone(), parse(&cod));
            shop.db
                .write(move |tx| {
                    upsert_remote(tx, &l, &order, NOON)?;
                    Ok(tx.query_row(
                        "SELECT id FROM online_orders WHERE remote_id = '9002'",
                        [],
                        |r| r.get::<_, String>(0),
                    )?)
                })
                .unwrap()
        };
        let (till, l, i) = (shop.till(), link.clone(), cod_id.clone());
        shop.db
            .write(move |tx| claim(tx, &till, &l, &i, true).map(drop))
            .unwrap();
        let (till, l, i) = (shop.till(), link.clone(), cod_id);
        assert_eq!(
            shop.db
                .write(move |tx| hand_over(tx, &till, &l, &i, "Livraison".into(), false))
                .unwrap_err()
                .code(),
            "cloud_order_state_invalid"
        );
    }

    /// loi 18-07: an order never closed here still loses its buyer's details, the retention
    /// plus a margin after DZBuild's last change of it, and every purge run is journaled with
    /// its count. An unlinked store's open orders close and its buyer flags go.
    #[test]
    fn buyers_details_go_from_orders_never_closed_and_each_purge_is_journaled() {
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        let (id, _) = mirror(&shop, &link, remote_order(&milk.id, 100, "pending"));
        let day = 86_400_000;
        let soon = NOON + 90 * day;
        assert_eq!(
            shop.db
                .write(move |tx| purge_personal_data(tx, 90, soon))
                .unwrap(),
            0,
            "an open order within the retention keeps its buyer"
        );
        let later = NOON + (90 + OPEN_ORDER_MARGIN_DAYS + 1) * day;
        assert_eq!(
            shop.db
                .write(move |tx| purge_personal_data(tx, 90, later))
                .unwrap(),
            1
        );
        let (terminal, i) = (shop.terminal.id.clone(), id.clone());
        let order = shop.db.read(move |c| get(c, &i, &terminal)).unwrap();
        assert_eq!((order.customer_name, order.customer_phone), (None, None));
        assert_eq!(
            shop.count(
                "SELECT COUNT(*) FROM audit_log WHERE action = 'cloud.personal_data_purged'"
            ),
            1
        );
        assert_eq!(
            shop.db
                .write(move |tx| purge_personal_data(tx, 90, later + 1))
                .unwrap(),
            0
        );
        assert_eq!(
            shop.count(
                "SELECT COUNT(*) FROM audit_log WHERE action = 'cloud.personal_data_purged'"
            ),
            1,
            "a run that purged nothing is not journaled"
        );
        // an unlinked store: its open orders close, its buyer flags go
        let mut other = order_json(&milk.id, 100, "confirmed");
        other["id"] = json!(9003);
        let (l, order) = (link.clone(), parse(&other));
        shop.db
            .write(move |tx| {
                upsert_remote(tx, &l, &order, NOON)?;
                tx.execute(
                    "INSERT INTO remote_customer_flags (store_id, phone_hash, remote_customer_id, is_banned, fraud_score, updated_at)
                     VALUES ('141', ?1, 'c1', 1, NULL, 0)",
                    ["a".repeat(64)],
                )?;
                close_unlinked_store(tx, "141", NOON + 5).map(drop)
            })
            .unwrap();
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM online_orders WHERE remote_id = '9003' AND local_state = 'closed' AND closed_at IS NOT NULL"),
            1
        );
        assert_eq!(shop.count("SELECT COUNT(*) FROM remote_customer_flags"), 0);
    }

    fn mirror(shop: &Shop, link: &CloudLink, order: RemoteOrder) -> (String, UpsertOutcome) {
        let link = link.clone();
        shop.db
            .write(move |tx| {
                let outcome = upsert_remote(tx, &link, &order, NOON)?;
                let id: String = tx.query_row(
                    "SELECT id FROM online_orders WHERE remote_id = '9001'",
                    [],
                    |r| r.get(0),
                )?;
                Ok((id, outcome))
            })
            .unwrap()
    }

    #[test]
    fn an_order_is_mirrored_once_with_its_products_resolved() {
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        let (id, first) = mirror(&shop, &link, remote_order(&milk.id, 100, "confirmed"));
        assert_eq!(first, UpsertOutcome::New);
        let (again, second) = mirror(&shop, &link, remote_order(&milk.id, 100, "confirmed"));
        assert_eq!((again, second), (id.clone(), UpsertOutcome::Unchanged));
        let terminal = shop.terminal.id.clone();
        let order = shop.db.read(move |c| get(c, &id, &terminal)).unwrap();
        assert_eq!(order.lines[0].product_id.as_deref(), Some(milk.id.as_str()));
        assert_eq!(
            (order.lines[0].qty, order.lines[0].unit_price),
            (Qty(2_000), Money(10_000))
        );
        assert_eq!(
            order.customer_phone.as_deref(),
            Some("0550123456"),
            "normalised"
        );
        assert_eq!(shop.db.read(unseen_count).unwrap(), 1);
    }

    #[test]
    fn claim_prepare_and_sell_at_the_till_updates_dzbuild_through_the_outbox() {
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        shop.stock_in(&milk.id, 10_000);
        shop.open_till(0);
        let (id, _) = mirror(&shop, &link, remote_order(&milk.id, 100, "confirmed"));
        let (till, l, i) = (shop.till(), link.clone(), id.clone());
        shop.db
            .write(move |tx| claim(tx, &till, &l, &i, false).map(drop))
            .unwrap();
        let (till, l, i) = (shop.till(), link.clone(), id.clone());
        shop.db
            .write(move |tx| prepare(tx, &till, &l, &i).map(drop))
            .unwrap();
        let (till, l, i) = (shop.till(), link.clone(), id.clone());
        let sale = shop
            .db
            .write(move |tx| {
                sell(
                    tx,
                    &till,
                    &l,
                    &i,
                    "Livraison".into(),
                    vec![crate::payments::PaymentInput::cash(Money(20_000))],
                )
            })
            .unwrap();
        assert_eq!(
            sale.document.total_ttc,
            Money(20_000),
            "the online price, which equals the shop's"
        );
        let terminal = shop.terminal.id.clone();
        let order = shop.db.read(move |c| get(c, &id, &terminal)).unwrap();
        assert_eq!(order.local_state, OrderState::Fulfilled);
        assert_eq!(
            order.local_document_id.as_deref(),
            Some(sale.document.id.as_str())
        );
        let kinds: Vec<String> = outbox_rows(&shop).into_iter().map(|r| r.0).collect();
        assert_eq!(
            kinds,
            ["order_claim", "order_status", "order_status"],
            "claim offline, processing, delivered"
        );
    }

    #[test]
    fn an_online_order_sold_at_the_till_is_not_deducted_twice_online() {
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        publish(&shop, &milk.id);
        shop.stock_in(&milk.id, 10_000);
        let run = |shop: &Shop| {
            let link = link.clone();
            shop.db
                .write(move |tx| {
                    let settings = cloud_settings(tx)?;
                    derive(tx, &link, &settings, NOON)
                })
                .unwrap()
        };
        run(&shop);
        // DZBuild confirmed the set of 10 pieces
        shop.db
            .write(|tx| {
                tx.execute(
                    "UPDATE cloud_stock SET confirmed = set_in_flight, set_in_flight = NULL",
                    [],
                )?;
                Ok(())
            })
            .unwrap();
        shop.open_till(0);
        let (id, _) = mirror(&shop, &link, remote_order(&milk.id, 100, "confirmed"));
        let (till, l, i) = (shop.till(), link.clone(), id.clone());
        shop.db
            .write(move |tx| {
                claim(tx, &till, &l, &i, true)?;
                sell(
                    tx,
                    &till,
                    &l,
                    &i,
                    "Livraison".into(),
                    vec![crate::payments::PaymentInput::cash(Money(20_000))],
                )
            })
            .unwrap();
        let report = run(&shop);
        assert_eq!(
            report.stock_items, 0,
            "DZBuild took the 2 pieces at confirmation"
        );
        assert_eq!(report.sales, 0, "an online order is not an in-store sale");
        let confirmed: i64 = shop
            .db
            .read(|c| Ok(c.query_row("SELECT confirmed FROM cloud_stock", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(confirmed, 8);
    }

    #[test]
    fn a_different_online_price_needs_the_price_override_like_any_price_change() {
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        shop.stock_in(&milk.id, 10_000);
        shop.open_till(0);
        let (id, _) = mirror(&shop, &link, remote_order(&milk.id, 90, "confirmed"));
        let (till, l, i) = (shop.till(), link.clone(), id.clone());
        shop.db
            .write(move |tx| claim(tx, &till, &l, &i, true).map(drop))
            .unwrap();
        let (till, l, i) = (shop.till(), link.clone(), id.clone());
        let refused = shop
            .db
            .write(move |tx| {
                sell(
                    tx,
                    &till,
                    &l,
                    &i,
                    "Livraison".into(),
                    vec![crate::payments::PaymentInput::cash(Money(18_000))],
                )
            })
            .unwrap_err();
        assert_eq!(
            refused.code(),
            "price_override_not_allowed",
            "the till asks a manager, as for any price change"
        );
        let grant = crate::auth::session::OverrideGrant {
            approver_id: shop.owner.user_id.clone(),
            approver_name: shop.owner.display_name.clone(),
            permission: crate::auth::Permission::SalesPriceOverride,
            granted_at: NOON,
            discount_limit_bp: 10_000,
        };
        let (till, l, i) = (shop.till().with_approval(grant), link.clone(), id);
        let sale = shop
            .db
            .write(move |tx| {
                sell(
                    tx,
                    &till,
                    &l,
                    &i,
                    "Livraison".into(),
                    vec![crate::payments::PaymentInput::cash(Money(18_000))],
                )
            })
            .unwrap();
        assert_eq!(sale.document.total_ttc, Money(18_000));
    }

    #[test]
    fn a_cancelled_order_closes_and_a_changed_prepared_order_goes_back_to_claimed() {
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        let (id, _) = mirror(&shop, &link, remote_order(&milk.id, 100, "confirmed"));
        let (till, l, i) = (shop.till(), link.clone(), id.clone());
        shop.db
            .write(move |tx| {
                claim(tx, &till, &l, &i, true)?;
                prepare(tx, &till, &l, &i)
            })
            .unwrap();
        let mut changed = remote_order(&milk.id, 100, "confirmed");
        changed.items[0].quantity = 3;
        changed.updated_at = crate::cloud::wire::ApiTime(NOON + 1_000);
        assert_eq!(mirror(&shop, &link, changed).1, UpsertOutcome::Updated);
        let (terminal, i) = (shop.terminal.id.clone(), id.clone());
        assert_eq!(
            shop.db
                .read(move |c| get(c, &i, &terminal))
                .unwrap()
                .local_state,
            OrderState::Claimed
        );
        let mut cancelled = remote_order(&milk.id, 100, "cancelled");
        cancelled.updated_at = crate::cloud::wire::ApiTime(NOON + 2_000);
        assert_eq!(mirror(&shop, &link, cancelled).1, UpsertOutcome::Closed);
        let notices = shop
            .db
            .read(|c| crate::cloud::store::notices(c, false))
            .unwrap();
        assert!(notices.iter().any(|n| n.code == "online_order_changed"));
        assert!(notices.iter().any(|n| n.code == "online_order_cancelled"));
    }

    #[test]
    fn refusing_cancels_online_and_the_buyer_is_forgotten_after_the_retention() {
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        let (id, _) = mirror(&shop, &link, remote_order(&milk.id, 100, "confirmed"));
        let (till, l, i) = (shop.till(), link.clone(), id.clone());
        shop.db
            .write(move |tx| refuse(tx, &till, &l, &i, RefuseReason::OutOfStock, None).map(drop))
            .unwrap();
        assert_eq!(outbox_rows(&shop)[0].0, "order_cancel");
        let day = 86_400_000;
        assert_eq!(
            shop.db
                .write(move |tx| purge_personal_data(tx, 90, NOON + 89 * day))
                .unwrap(),
            0
        );
        assert_eq!(
            shop.db
                .write(move |tx| purge_personal_data(tx, 90, NOON + 91 * day))
                .unwrap(),
            1
        );
        let (terminal, i) = (shop.terminal.id.clone(), id);
        let order = shop.db.read(move |c| get(c, &i, &terminal)).unwrap();
        assert_eq!(
            (order.customer_name, order.customer_phone, order.address),
            (None, None, None)
        );
        assert_eq!(order.order_number, "1042", "the order itself stays");
    }

    #[test]
    fn a_courier_order_ships_with_a_delivery_note_to_the_buyer() {
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        shop.stock_in(&milk.id, 10_000);
        let mut value = order_json(&milk.id, 100, "confirmed");
        value["delivery_type"] = json!("home");
        value["shipping_cost"] = json!(400);
        value["total"] = json!(600);
        let (id, _) = mirror(&shop, &link, parse(&value));
        let ctx = crate::till::TillContext::new(shop.owner.clone(), shop.pack.clone(), NOON);
        let (l, i) = (link.clone(), id.clone());
        let bl = shop
            .db
            .write(move |tx| {
                claim(tx, &ctx, &l, &i, true)?;
                ship(tx, &ctx, &l, &i, "Livraison".into(), None)
            })
            .unwrap();
        assert_eq!(bl.doc_type, crate::documents::DocType::DeliveryNote);
        assert!(bl.number.as_deref().is_some_and(|n| n.starts_with("BL")));
        assert_eq!(
            bl.total_ttc,
            Money(60_000),
            "the goods and the delivery fee the courier collects"
        );
        let buyer = shop
            .db
            .read(|c| crate::customers::find_by_phone(c, "0550123456"))
            .unwrap()
            .expect("the consignee the BL names");
        assert_eq!(
            (buyer.name.as_str(), buyer.wilaya.as_deref()),
            ("Samira B.", Some("16"))
        );
        assert_eq!(bl.customer_id.as_deref(), Some(buyer.id.as_str()));
        let status: serde_json::Value = shop
            .db
            .read(|c| {
                let text: String = c.query_row(
                    "SELECT payload FROM sync_outbox WHERE kind = 'order_status' ORDER BY created_at DESC, rowid DESC LIMIT 1",
                    [],
                    |r| r.get(0),
                )?;
                Ok(serde_json::from_str(&text)?)
            })
            .unwrap();
        assert_eq!(status["body"]["status"], "shipped");
    }

    #[test]
    fn a_discount_larger_than_the_lines_is_refused() {
        assert_eq!(
            spread(Money(301), &[Money(200), Money(100)])
                .unwrap_err()
                .code(),
            "discount_invalid"
        );
        assert_eq!(
            spread(Money(300), &[Money(200), Money(100)]).unwrap(),
            [Money(200), Money(100)]
        );
    }

    mod spread_props {
        use super::super::spread;
        use crate::money::Money;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn shares_add_up_to_the_discount_and_none_exceeds_its_line(
                grosses in proptest::collection::vec(0_i64..5_000_000, 1..12),
                part in 0_u32..=10_000,
            ) {
                let grosses: Vec<Money> = grosses.into_iter().map(Money).collect();
                let total: i64 = grosses.iter().map(|g| g.0).sum();
                let amount = Money(total * i64::from(part) / 10_000);
                prop_assume!(total > 0);
                let shares = spread(amount, &grosses).unwrap();
                prop_assert_eq!(shares.iter().map(|s| s.0).sum::<i64>(), amount.0);
                for (share, gross) in shares.iter().zip(&grosses) {
                    prop_assert!(share.0 >= 0 && share.0 <= gross.0, "{share:?} of {gross:?}");
                }
            }
        }
    }
}

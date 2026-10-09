//! The cloud tables of migration 0006: the link, the id map, notices, endpoint readiness, remote
//! cursors, and one typed snapshot of the `cloud.*` settings. No token ever reaches these rows.

use super::http::Endpoint;
use crate::error::AppError;
use crate::ids::new_id;
use crate::json::JsonValue;
use crate::lang::Lang;
use crate::sqlenum::sql_text_enum;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;

/// A Brief endpoint answered "not shipped": retried once a day (spec §4.7).
pub const NOT_READY_RETRY_MS: i64 = 24 * 3_600_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum LinkStatus {
    Linked,
    /// The refresh token was refused: the tokens are gone, the outbox and the map wait.
    RelinkRequired,
    /// DZBuild revoked this device or the link.
    Revoked,
    Unlinked,
}

impl LinkStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            LinkStatus::Linked => "linked",
            LinkStatus::RelinkRequired => "relink_required",
            LinkStatus::Revoked => "revoked",
            LinkStatus::Unlinked => "unlinked",
        }
    }
    pub fn parse(s: &str) -> Option<LinkStatus> {
        [
            LinkStatus::Linked,
            LinkStatus::RelinkRequired,
            LinkStatus::Revoked,
            LinkStatus::Unlinked,
        ]
        .into_iter()
        .find(|l| l.as_str() == s)
    }
}

sql_text_enum!(LinkStatus);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudLink {
    pub id: String,
    pub account_sub: String,
    pub account_name: Option<String>,
    pub store_id: String,
    pub store_name: String,
    pub store_language: Lang,
    pub store_plan: Option<String>,
    pub public_url: Option<String>,
    pub location_id: Option<String>,
    pub device_id: Option<String>,
    pub scopes: Vec<String>,
    pub stock_deduction: Option<String>,
    pub active_products_cap: Option<i64>,
    pub status: LinkStatus,
    pub linked_at: i64,
    pub last_sync_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewLink {
    pub id: String,
    pub account_sub: String,
    pub account_name: Option<String>,
    pub store_id: String,
    pub store_name: String,
    pub store_language: Lang,
    pub store_plan: Option<String>,
    pub public_url: Option<String>,
    pub location_id: Option<String>,
    pub device_id: Option<String>,
    pub scopes: Vec<String>,
    pub stock_deduction: Option<String>,
    pub active_products_cap: Option<i64>,
}

const LINK_SELECT: &str = "SELECT id, account_sub, account_name, store_id, store_name, store_language,
        store_plan, public_url, location_id, device_id, scopes, stock_deduction, active_products_cap,
        status, linked_at, last_sync_at FROM cloud_links";

fn map_link(r: &Row<'_>) -> rusqlite::Result<CloudLink> {
    let scopes: String = r.get(10)?;
    Ok(CloudLink {
        id: r.get(0)?,
        account_sub: r.get(1)?,
        account_name: r.get(2)?,
        store_id: r.get(3)?,
        store_name: r.get(4)?,
        store_language: r.get(5)?,
        store_plan: r.get(6)?,
        public_url: r.get(7)?,
        location_id: r.get(8)?,
        device_id: r.get(9)?,
        scopes: serde_json::from_str(&scopes).unwrap_or_default(),
        stock_deduction: r.get(11)?,
        active_products_cap: r.get(12)?,
        status: r.get(13)?,
        linked_at: r.get(14)?,
        last_sync_at: r.get(15)?,
    })
}

/// The link that is not unlinked (linked, waiting for a relink, or revoked), if any.
pub fn active_link(conn: &Connection) -> Result<Option<CloudLink>, AppError> {
    Ok(conn
        .query_row(
            &format!("{LINK_SELECT} WHERE status <> 'unlinked'"),
            [],
            map_link,
        )
        .optional()?)
}

/// A PC linked to DZBuild does not join a main PC as a till: the copy it would install drops the
/// link without revoking its token or removing its device on DZBuild, and archives its unsent
/// uploads (sales, Z, stock). The owner unlinks it first (Settings › DZBuild), which does both;
/// `cloud_unlink_first` names the store and the uploads still to send.
pub fn refuse_while_linked(conn: &Connection) -> Result<(), AppError> {
    let Some(link) = active_link(conn)? else {
        return Ok(());
    };
    let pending: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sync_outbox WHERE target = 'cloud' AND store_id = ?1
           AND status IN ('pending', 'failed')",
        [&link.store_id],
        |r| r.get(0),
    )?;
    Err(AppError::rejected(
        serde_json::json!({ "store": link.store_name, "pending": pending }),
        "cloud_unlink_first",
    ))
}

pub fn link_by_id(conn: &Connection, id: &str) -> Result<CloudLink, AppError> {
    conn.query_row(&format!("{LINK_SELECT} WHERE id = ?1"), [id], map_link)
        .optional()?
        .ok_or_else(|| AppError::sync("cloud_not_linked"))
}

pub fn insert_link(tx: &Transaction<'_>, link: &NewLink, now: i64) -> Result<CloudLink, AppError> {
    tx.execute(
        "INSERT INTO cloud_links (id, account_sub, account_name, store_id, store_name, store_language,
             store_plan, public_url, location_id, device_id, scopes, stock_deduction, active_products_cap,
             status, linked_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 'linked', ?14)",
        params![
            link.id,
            link.account_sub,
            link.account_name,
            link.store_id,
            link.store_name,
            link.store_language,
            link.store_plan,
            link.public_url,
            link.location_id,
            link.device_id,
            serde_json::to_string(&link.scopes)?,
            link.stock_deduction,
            link.active_products_cap,
            now
        ],
    )?;
    mirror_store_url(tx)?;
    link_by_id(tx, &link.id)
}

/// The setting the tills read for the "Commandez en ligne" QR (`cloud.store_public_url`): the
/// public address of the live link, null otherwise. Written with every change of the link, in
/// its transaction; the link's own journal rows say what changed.
fn mirror_store_url(tx: &Transaction<'_>) -> Result<(), AppError> {
    let url = active_link(tx)?
        .filter(|l| l.status == LinkStatus::Linked)
        .and_then(|l| l.public_url)
        .filter(|u| u.starts_with("https://"));
    let value = url.map_or(Value::Null, Value::String);
    if crate::settings::get_value(tx, STORE_URL_SETTING)? == value {
        return Ok(());
    }
    tx.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![STORE_URL_SETTING, value.to_string(), crate::time::now_ms()],
    )?;
    Ok(())
}

/// The shared setting that mirrors the live link's public address (see [`mirror_store_url`]).
pub const STORE_URL_SETTING: &str = "cloud.store_public_url";

/// A link waiting for a relink (or revoked) comes back with a new grant: same row, same outbox.
pub fn relink(tx: &Transaction<'_>, link: &NewLink, now: i64) -> Result<CloudLink, AppError> {
    tx.execute(
        "UPDATE cloud_links SET account_sub = ?2, account_name = ?3, store_name = ?4, store_language = ?5,
             store_plan = ?6, public_url = ?7, location_id = ?8, device_id = ?9, scopes = ?10,
             stock_deduction = ?11, active_products_cap = ?12, status = 'linked', linked_at = ?13
         WHERE id = ?1 AND status IN ('relink_required', 'revoked')",
        params![
            link.id,
            link.account_sub,
            link.account_name,
            link.store_name,
            link.store_language,
            link.store_plan,
            link.public_url,
            link.location_id,
            link.device_id,
            serde_json::to_string(&link.scopes)?,
            link.stock_deduction,
            link.active_products_cap,
            now
        ],
    )?;
    mirror_store_url(tx)?;
    link_by_id(tx, &link.id)
}

pub fn set_status(
    tx: &Transaction<'_>,
    id: &str,
    status: LinkStatus,
    now: i64,
) -> Result<(), AppError> {
    let unlinked_at = (status == LinkStatus::Unlinked).then_some(now);
    tx.execute(
        "UPDATE cloud_links SET status = ?2, unlinked_at = ?3 WHERE id = ?1",
        params![id, status, unlinked_at],
    )?;
    mirror_store_url(tx)
}

pub fn touch_sync(tx: &Transaction<'_>, id: &str, now: i64) -> Result<(), AppError> {
    tx.execute(
        "UPDATE cloud_links SET last_sync_at = ?2 WHERE id = ?1",
        params![id, now],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileUpdate {
    pub store_name: Option<String>,
    pub store_language: Option<Lang>,
    pub store_plan: Option<String>,
    pub public_url: Option<String>,
    pub stock_deduction: Option<String>,
    pub active_products_cap: Option<i64>,
}

/// The daily `GET /v1/store` and `store.plan_changed` (values absent from the answer are kept).
pub fn update_profile(tx: &Transaction<'_>, id: &str, p: &ProfileUpdate) -> Result<(), AppError> {
    tx.execute(
        "UPDATE cloud_links SET store_name = COALESCE(?2, store_name), store_language = COALESCE(?3, store_language),
             store_plan = COALESCE(?4, store_plan), public_url = COALESCE(?5, public_url),
             stock_deduction = COALESCE(?6, stock_deduction), active_products_cap = COALESCE(?7, active_products_cap)
         WHERE id = ?1",
        params![
            id,
            p.store_name,
            p.store_language,
            p.store_plan,
            p.public_url,
            p.stock_deduction,
            p.active_products_cap
        ],
    )?;
    mirror_store_url(tx)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapEntity {
    Product,
    Category,
    Sale,
    Refund,
    Closure,
    Media,
    Location,
    Device,
}

impl MapEntity {
    pub const fn as_str(self) -> &'static str {
        match self {
            MapEntity::Product => "product",
            MapEntity::Category => "category",
            MapEntity::Sale => "sale",
            MapEntity::Refund => "refund",
            MapEntity::Closure => "closure",
            MapEntity::Media => "media",
            MapEntity::Location => "location",
            MapEntity::Device => "device",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapRow {
    pub remote_id: String,
    pub last_payload: Option<Value>,
    pub last_hash: Option<String>,
    pub updated_at: i64,
}

pub fn map_get(
    conn: &Connection,
    store: &str,
    entity: MapEntity,
    local_id: &str,
) -> Result<Option<MapRow>, AppError> {
    Ok(conn
        .query_row(
            "SELECT remote_id, last_payload, last_hash, updated_at FROM cloud_map
             WHERE store_id = ?1 AND entity_type = ?2 AND local_id = ?3",
            params![store, entity.as_str(), local_id],
            |r| {
                let payload: Option<String> = r.get(1)?;
                Ok(MapRow {
                    remote_id: r.get(0)?,
                    last_payload: payload.and_then(|p| serde_json::from_str(&p).ok()),
                    last_hash: r.get(2)?,
                    updated_at: r.get(3)?,
                })
            },
        )
        .optional()?)
}

#[allow(clippy::too_many_arguments)]
pub fn map_put(
    tx: &Transaction<'_>,
    store: &str,
    entity: MapEntity,
    local_id: &str,
    remote_id: &str,
    last_payload: Option<&Value>,
    last_hash: Option<&str>,
    now: i64,
) -> Result<(), AppError> {
    let payload = last_payload.map(Value::to_string);
    tx.execute(
        "INSERT INTO cloud_map (store_id, entity_type, local_id, remote_id, last_payload, last_hash, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT (store_id, entity_type, local_id) DO UPDATE SET remote_id = excluded.remote_id,
             last_payload = COALESCE(excluded.last_payload, last_payload),
             last_hash = COALESCE(excluded.last_hash, last_hash), updated_at = excluded.updated_at",
        params![store, entity.as_str(), local_id, remote_id, payload, last_hash, now],
    )?;
    Ok(())
}

pub fn map_local(
    conn: &Connection,
    store: &str,
    entity: MapEntity,
    remote_id: &str,
) -> Result<Option<String>, AppError> {
    Ok(conn
        .query_row(
            "SELECT local_id FROM cloud_map WHERE store_id = ?1 AND entity_type = ?2 AND remote_id = ?3",
            params![store, entity.as_str(), remote_id],
            |r| r.get(0),
        )
        .optional()?)
}

pub fn map_forget(
    tx: &Transaction<'_>,
    store: &str,
    entity: MapEntity,
    local_id: &str,
) -> Result<(), AppError> {
    tx.execute(
        "DELETE FROM cloud_map WHERE store_id = ?1 AND entity_type = ?2 AND local_id = ?3",
        params![store, entity.as_str(), local_id],
    )?;
    Ok(())
}

/// What a DZBuild notice is about (`CloudNotice::kind`). Named apart from the shop network's
/// `NoticeKind` (M5): `src/bindings.ts` gives every exported type one name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CloudNoticeKind {
    NotReady,
    PlanCap,
    Conflict,
    Relink,
    Revoked,
    FailedItems,
    Order,
    Stock,
    /// The automatic online backup of 0.7–0.9, removed in 1.0.0 (free edition, no cloud
    /// storage). Never raised any more; kept so a `cloud_notices` row of an older version still
    /// reads (`notices` leaves those rows out).
    Backup,
}

impl CloudNoticeKind {
    pub const ALL: [CloudNoticeKind; 9] = [
        CloudNoticeKind::NotReady,
        CloudNoticeKind::PlanCap,
        CloudNoticeKind::Conflict,
        CloudNoticeKind::Relink,
        CloudNoticeKind::Revoked,
        CloudNoticeKind::FailedItems,
        CloudNoticeKind::Order,
        CloudNoticeKind::Stock,
        CloudNoticeKind::Backup,
    ];
    pub const fn as_str(self) -> &'static str {
        match self {
            CloudNoticeKind::NotReady => "not_ready",
            CloudNoticeKind::PlanCap => "plan_cap",
            CloudNoticeKind::Conflict => "conflict",
            CloudNoticeKind::Relink => "relink",
            CloudNoticeKind::Revoked => "revoked",
            CloudNoticeKind::FailedItems => "failed_items",
            CloudNoticeKind::Order => "order",
            CloudNoticeKind::Stock => "stock",
            CloudNoticeKind::Backup => "backup",
        }
    }
    pub fn parse(s: &str) -> Option<CloudNoticeKind> {
        CloudNoticeKind::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

sql_text_enum!(CloudNoticeKind);

/// Something the owner should know about the link, said once and counted (never a toast storm).
/// `code` is translated by the UI (`sync:notices.<code>`), `subject` names what it is about (an
/// endpoint, a product, an order), `details` fill the sentence.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudNotice {
    pub id: String,
    pub kind: CloudNoticeKind,
    pub code: String,
    pub subject: String,
    #[specta(type = JsonValue)]
    pub details: Value,
    pub count: i64,
    pub first_at: i64,
    pub last_at: i64,
    pub dismissed: bool,
}

/// Raises (or counts again) the open notice `(kind, code, subject)`; `true` when it is new.
pub fn raise_notice(
    tx: &Transaction<'_>,
    kind: CloudNoticeKind,
    code: &str,
    subject: &str,
    details: Value,
    now: i64,
) -> Result<bool, AppError> {
    let updated = tx.execute(
        "UPDATE cloud_notices SET count = count + 1, last_at = ?4, details = ?5
         WHERE kind = ?1 AND code = ?2 AND subject = ?3 AND dismissed_at IS NULL",
        params![kind, code, subject, now, details.to_string()],
    )?;
    if updated > 0 {
        return Ok(false);
    }
    tx.execute(
        "INSERT INTO cloud_notices (id, kind, code, subject, details, first_at, last_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        params![new_id(), kind, code, subject, details.to_string(), now],
    )?;
    Ok(true)
}

/// The situation is over (the endpoint shipped, the link came back): the open notice goes.
pub fn clear_notice(
    tx: &Transaction<'_>,
    kind: CloudNoticeKind,
    code: &str,
    subject: &str,
    now: i64,
) -> Result<(), AppError> {
    tx.execute(
        "UPDATE cloud_notices SET dismissed_at = ?4
         WHERE kind = ?1 AND code = ?2 AND subject = ?3 AND dismissed_at IS NULL",
        params![kind, code, subject, now],
    )?;
    Ok(())
}

/// The online backup endpoints of 0.7–0.9 (removed in 1.0.0): an older version may have
/// recorded them "not ready", with their notice; the POS no longer calls them, so neither shows.
const REMOVED_ENDPOINTS_JSON: &str = r#"["backups","backups.complete","backups.download"]"#;

pub fn notices(conn: &Connection, include_dismissed: bool) -> Result<Vec<CloudNotice>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, code, subject, details, count, first_at, last_at, dismissed_at FROM cloud_notices
         WHERE (?1 OR dismissed_at IS NULL)
           AND kind <> 'backup'
           AND NOT (kind = 'not_ready' AND subject IN (SELECT value FROM json_each(?2)))
         ORDER BY last_at DESC LIMIT 200",
    )?;
    let rows = stmt
        .query_map(params![include_dismissed, REMOVED_ENDPOINTS_JSON], |r| {
            let details: String = r.get(4)?;
            let dismissed: Option<i64> = r.get(8)?;
            Ok(CloudNotice {
                id: r.get(0)?,
                kind: r.get(1)?,
                code: r.get(2)?,
                subject: r.get(3)?,
                details: serde_json::from_str(&details).unwrap_or(Value::Null),
                count: r.get(5)?,
                first_at: r.get(6)?,
                last_at: r.get(7)?,
                dismissed: dismissed.is_some(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn dismiss_notice(tx: &Transaction<'_>, id: &str, now: i64) -> Result<(), AppError> {
    tx.execute(
        "UPDATE cloud_notices SET dismissed_at = ?2 WHERE id = ?1 AND dismissed_at IS NULL",
        params![id, now],
    )?;
    Ok(())
}

/// Ready, or not ready and its daily retry is due.
pub fn endpoint_due(
    conn: &Connection,
    store: &str,
    endpoint: Endpoint,
    now: i64,
) -> Result<bool, AppError> {
    let retry_at: Option<Option<i64>> = conn
        .query_row(
            "SELECT retry_at FROM cloud_endpoints WHERE store_id = ?1 AND endpoint = ?2 AND state = 'not_ready'",
            params![store, endpoint.name()],
            |r| r.get(0),
        )
        .optional()?;
    Ok(retry_at.is_none_or(|at| at.is_none_or(|t| t <= now)))
}

/// Marks `endpoint` not ready (retry in a day); the first time raises the one notice. `true`
/// when this is news. A repeat report before the retry is due keeps that retry: another item of
/// the same cycle never postpones it.
pub fn set_not_ready(
    tx: &Transaction<'_>,
    store: &str,
    endpoint: Endpoint,
    now: i64,
) -> Result<bool, AppError> {
    let was: Option<String> = tx
        .query_row(
            "SELECT state FROM cloud_endpoints WHERE store_id = ?1 AND endpoint = ?2",
            params![store, endpoint.name()],
            |r| r.get(0),
        )
        .optional()?;
    tx.execute(
        "INSERT INTO cloud_endpoints (store_id, endpoint, state, since, retry_at) VALUES (?1, ?2, 'not_ready', ?3, ?4)
         ON CONFLICT (store_id, endpoint) DO UPDATE SET state = 'not_ready',
             retry_at = CASE WHEN state = 'not_ready' AND retry_at > excluded.since THEN retry_at
                             ELSE excluded.retry_at END,
             since = CASE WHEN state = 'not_ready' THEN since ELSE excluded.since END",
        params![store, endpoint.name(), now, now + NOT_READY_RETRY_MS],
    )?;
    let news = was.as_deref() != Some("not_ready");
    if news {
        raise_notice(
            tx,
            CloudNoticeKind::NotReady,
            "cloud_not_ready",
            endpoint.name(),
            serde_json::json!({}),
            now,
        )?;
    }
    Ok(news)
}

pub fn set_ready(
    tx: &Transaction<'_>,
    store: &str,
    endpoint: Endpoint,
    now: i64,
) -> Result<(), AppError> {
    tx.execute(
        "INSERT INTO cloud_endpoints (store_id, endpoint, state, since) VALUES (?1, ?2, 'ready', ?3)
         ON CONFLICT (store_id, endpoint) DO UPDATE SET state = 'ready', retry_at = NULL,
             since = CASE WHEN state = 'ready' THEN since ELSE excluded.since END",
        params![store, endpoint.name(), now],
    )?;
    clear_notice(
        tx,
        CloudNoticeKind::NotReady,
        "cloud_not_ready",
        endpoint.name(),
        now,
    )
}

pub fn not_ready_endpoints(conn: &Connection, store: &str) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT endpoint FROM cloud_endpoints WHERE store_id = ?1 AND state = 'not_ready'
           AND endpoint NOT IN (SELECT value FROM json_each(?2)) ORDER BY endpoint",
    )?;
    let rows = stmt
        .query_map(params![store, REMOVED_ENDPOINTS_JSON], |r| r.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    Ok(rows)
}

pub fn remote_cursor(conn: &Connection, name: &str) -> Result<Option<String>, AppError> {
    Ok(conn
        .query_row(
            "SELECT cursor FROM sync_cursors WHERE name = ?1 AND kind = 'remote'",
            [name],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten())
}

pub fn set_remote_cursor(
    tx: &Transaction<'_>,
    name: &str,
    cursor: &str,
    now: i64,
) -> Result<(), AppError> {
    tx.execute(
        "INSERT INTO sync_cursors (name, kind, cursor, updated_at) VALUES (?1, 'remote', ?2, ?3)
         ON CONFLICT (name) DO UPDATE SET cursor = excluded.cursor, updated_at = excluded.updated_at",
        params![name, cursor, now],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OrderDocument {
    Ticket,
    DeliveryNote,
}

/// The `cloud.*` settings, read once per sync cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudSettings {
    pub sync_enabled: bool,
    pub publish_all: bool,
    pub images_push: bool,
    pub stock_push: bool,
    pub stock_pull: bool,
    /// A price changed online becomes the till's price (`cloud.prices.pull`); off, the owner is
    /// told instead.
    pub prices_pull: bool,
    pub sales_push: bool,
    pub orders_intake: bool,
    pub orders_document: OrderDocument,
    pub orders_retention_days: i64,
    pub customers_sync: bool,
    pub customers_flags: bool,
    pub receipt_qr: bool,
}

pub fn cloud_settings(conn: &Connection) -> Result<CloudSettings, AppError> {
    use crate::settings::get;
    Ok(CloudSettings {
        sync_enabled: get(conn, "cloud.sync.enabled")?,
        publish_all: get::<String>(conn, "cloud.publish.mode")? == "all",
        images_push: get(conn, "cloud.images.push")?,
        stock_push: get(conn, "cloud.stock.push")?,
        stock_pull: get(conn, "cloud.stock.pull")?,
        prices_pull: get(conn, "cloud.prices.pull")?,
        sales_push: get(conn, "cloud.sales.push")?,
        orders_intake: get(conn, "cloud.orders.intake")?,
        orders_document: get(conn, "cloud.orders.document")?,
        orders_retention_days: get(conn, "cloud.orders.retention_days")?,
        customers_sync: get(conn, "cloud.customers.sync")?,
        customers_flags: get(conn, "cloud.customers.flags")?,
        receipt_qr: get(conn, "cloud.receipt_qr")?,
    })
}

/// The online store printed on receipts and shown on the customer display ("Commandez en
/// ligne"): only with `cloud.receipt_qr` on and a live link that has a public URL. Read from the
/// shared settings the main PC keeps in step with its link, so a till prints it too.
pub fn receipt_url(conn: &Connection) -> Result<Option<String>, AppError> {
    if !crate::settings::get::<bool>(conn, "cloud.receipt_qr")? {
        return Ok(None);
    }
    Ok(
        crate::settings::get::<Option<String>>(conn, STORE_URL_SETTING)?
            .filter(|u| u.starts_with("https://")),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::http::Endpoint;
    use crate::db::Db;
    use serde_json::json;

    fn new_link() -> NewLink {
        NewLink {
            id: "00000000-0000-7000-8000-00000000c001".into(),
            account_sub: "84211".into(),
            account_name: Some("Amel".into()),
            store_id: "141".into(),
            store_name: "Boutique Amel".into(),
            store_language: Lang::Ar,
            store_plan: Some("pro".into()),
            public_url: Some("https://amel.dzbuild.app".into()),
            location_id: Some("loc_1".into()),
            device_id: Some("dev_1".into()),
            scopes: vec!["offline_access".into()],
            stock_deduction: Some("on_confirm".into()),
            active_products_cap: Some(300),
        }
    }

    #[test]
    fn a_link_is_inserted_read_back_and_unlinked() {
        let db = Db::open_memory().unwrap();
        assert!(db.read(active_link).unwrap().is_none());
        let link = db.write(|tx| insert_link(tx, &new_link(), 10)).unwrap();
        assert_eq!(
            (link.status, link.store_language),
            (LinkStatus::Linked, Lang::Ar)
        );
        assert_eq!(db.read(active_link).unwrap().unwrap(), link);
        let id = link.id.clone();
        db.write(move |tx| set_status(tx, &id, LinkStatus::Unlinked, 20))
            .unwrap();
        assert!(db.read(active_link).unwrap().is_none());
    }

    #[test]
    fn notices_coalesce_until_dismissed() {
        let db = Db::open_memory().unwrap();
        let raise = |n: i64| {
            db.write(move |tx| {
                raise_notice(
                    tx,
                    CloudNoticeKind::FailedItems,
                    "cloud_request_invalid",
                    "pos_sale",
                    json!({ "n": n }),
                    n,
                )
            })
            .unwrap()
        };
        assert!(raise(1), "first time");
        assert!(!raise(2), "the same notice again");
        let open = db.read(|c| notices(c, false)).unwrap();
        assert_eq!(
            (open.len(), open[0].count, open[0].details["n"].clone()),
            (1, 2, json!(2))
        );
        let id = open[0].id.clone();
        db.write(move |tx| dismiss_notice(tx, &id, 3)).unwrap();
        assert!(db.read(|c| notices(c, false)).unwrap().is_empty());
        assert!(raise(4), "a new occurrence after a dismissal is news");
        assert_eq!(db.read(|c| notices(c, true)).unwrap().len(), 2);
    }

    /// What 0.7–0.9 recorded about the online backup removed in 1.0.0 (its notices, its
    /// endpoints "not ready") still reads, and is no longer shown; other notices are.
    #[test]
    fn the_removed_online_backup_leaves_no_notice_behind() {
        let db = Db::open_memory().unwrap();
        db.write(|tx| {
            raise_notice(
                tx,
                CloudNoticeKind::Backup,
                "cloud_backup_no_key",
                "auto",
                json!({}),
                1,
            )?;
            for endpoint in ["backups", "backups.complete", "backups.download"] {
                tx.execute(
                    "INSERT INTO cloud_endpoints (store_id, endpoint, state, since, retry_at)
                     VALUES ('141', ?1, 'not_ready', 1, 2)",
                    [endpoint],
                )?;
                raise_notice(
                    tx,
                    CloudNoticeKind::NotReady,
                    "cloud_not_ready",
                    endpoint,
                    json!({}),
                    1,
                )?;
            }
            set_not_ready(tx, "141", Endpoint::PosClosures, 3).map(drop)
        })
        .unwrap();
        let kinds = db.read(|c| {
            let mut stmt = c.prepare("SELECT kind FROM cloud_notices")?;
            let kinds = stmt
                .query_map([], |r| r.get::<_, CloudNoticeKind>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(kinds)
        });
        assert_eq!(kinds.unwrap().len(), 5, "every row still reads");
        let shown = db.read(|c| notices(c, true)).unwrap();
        assert_eq!(
            shown
                .iter()
                .map(|n| (n.kind, n.subject.as_str()))
                .collect::<Vec<_>>(),
            [(CloudNoticeKind::NotReady, "pos.closures")]
        );
        assert_eq!(
            db.read(|c| not_ready_endpoints(c, "141")).unwrap(),
            ["pos.closures"]
        );
    }

    #[test]
    fn a_not_ready_endpoint_raises_one_notice_and_is_retried_a_day_later() {
        let db = Db::open_memory().unwrap();
        let day = NOT_READY_RETRY_MS;
        assert!(
            db.read(|c| endpoint_due(c, "141", Endpoint::PosClosures, 0))
                .unwrap()
        );
        assert!(
            db.write(|tx| set_not_ready(tx, "141", Endpoint::PosClosures, 0))
                .unwrap()
        );
        assert!(
            !db.write(|tx| set_not_ready(tx, "141", Endpoint::PosClosures, 5))
                .unwrap(),
            "already known"
        );
        assert!(
            !db.read(move |c| endpoint_due(c, "141", Endpoint::PosClosures, day - 1))
                .unwrap()
        );
        assert!(
            db.read(move |c| endpoint_due(c, "141", Endpoint::PosClosures, day))
                .unwrap()
        );
        assert!(
            !db.write(move |tx| set_not_ready(tx, "141", Endpoint::PosClosures, day))
                .unwrap()
        );
        assert!(
            !db.read(move |c| endpoint_due(c, "141", Endpoint::PosClosures, day + 1))
                .unwrap(),
            "still not shipped at the retry: the next one is a day later"
        );
        assert_eq!(db.read(|c| notices(c, false)).unwrap().len(), 1);
        assert_eq!(
            db.read(|c| not_ready_endpoints(c, "141")).unwrap(),
            ["pos.closures"]
        );
        db.write(move |tx| set_ready(tx, "141", Endpoint::PosClosures, day))
            .unwrap();
        assert!(
            db.read(|c| notices(c, false)).unwrap().is_empty(),
            "the notice goes when DZBuild ships it"
        );
    }

    #[test]
    fn the_map_and_the_remote_cursors_round_trip() {
        let db = Db::open_memory().unwrap();
        db.write(|tx| {
            map_put(
                tx,
                "141",
                MapEntity::Product,
                "p1",
                "88121",
                Some(&json!({ "price": 450000 })),
                None,
                1,
            )
        })
        .unwrap();
        let row = db
            .read(|c| map_get(c, "141", MapEntity::Product, "p1"))
            .unwrap()
            .unwrap();
        assert_eq!(
            (row.remote_id.as_str(), row.last_payload),
            ("88121", Some(json!({ "price": 450000 })))
        );
        assert_eq!(
            db.read(|c| map_local(c, "141", MapEntity::Product, "88121"))
                .unwrap()
                .as_deref(),
            Some("p1")
        );
        assert!(
            db.read(|c| map_get(c, "152", MapEntity::Product, "p1"))
                .unwrap()
                .is_none(),
            "per store"
        );
        db.write(|tx| set_remote_cursor(tx, "cloud:events", "c_42", 2))
            .unwrap();
        assert_eq!(
            db.read(|c| remote_cursor(c, "cloud:events"))
                .unwrap()
                .as_deref(),
            Some("c_42")
        );
    }

    #[test]
    fn the_cloud_settings_are_read_in_one_place() {
        let db = Db::open_memory().unwrap();
        let s = db.read(cloud_settings).unwrap();
        assert!(s.sync_enabled && !s.publish_all && !s.customers_sync && !s.receipt_qr);
        assert_eq!(
            (s.orders_document, s.orders_retention_days),
            (OrderDocument::Ticket, 90)
        );
    }

    #[test]
    fn the_receipt_url_needs_the_setting_and_a_live_link() {
        let (shop, _) = crate::cloud::testkit::linked_shop();
        assert_eq!(shop.db.read(receipt_url).unwrap(), None, "off by default");
        shop.set("cloud.receipt_qr", serde_json::json!(true));
        assert_eq!(
            shop.db.read(receipt_url).unwrap().as_deref(),
            Some("https://amel.dzbuild.app")
        );
        shop.db
            .write(|tx| {
                let id: String = tx.query_row("SELECT id FROM cloud_links", [], |r| r.get(0))?;
                set_status(tx, &id, LinkStatus::RelinkRequired, 20)
            })
            .unwrap();
        assert_eq!(shop.db.read(receipt_url).unwrap(), None);
    }

    /// A till has no `cloud_links` (the main PC's own): it prints the store's QR from the shared
    /// setting the main PC keeps in step with its link.
    #[test]
    fn a_till_prints_the_stores_qr_from_the_main_pcs_link() {
        let lan = crate::sync::testkit::lan_pair();
        lan.host.set("cloud.receipt_qr", serde_json::json!(true));
        lan.host
            .db
            .write(|tx| insert_link(tx, &new_link(), 10).map(drop))
            .unwrap();
        lan.pull();
        assert_eq!(
            lan.count_till("SELECT COUNT(*) FROM cloud_links"),
            0,
            "the link is the main PC's"
        );
        assert_eq!(
            lan.till.read(receipt_url).unwrap().as_deref(),
            Some("https://amel.dzbuild.app")
        );
        lan.host
            .db
            .write(|tx| {
                set_status(
                    tx,
                    "00000000-0000-7000-8000-00000000c001",
                    LinkStatus::Unlinked,
                    20,
                )
            })
            .unwrap();
        lan.pull();
        assert_eq!(lan.till.read(receipt_url).unwrap(), None, "unlinked: no QR");
    }
}

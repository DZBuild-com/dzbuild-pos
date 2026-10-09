//! The change-feed reader of the DZBuild link (spec §4.7, §5.1): in one transaction, it reads the
//! rows committed since its position, turns them into outbox items — product upserts and photos
//! of published products, stock batches planned on the baseline, in-store sales, refunds after
//! their sale, Z closures — and moves its position. A crash before the commit loses nothing and
//! repeats nothing; every item's key is deterministic, so even a replayed feed adds no twin.

use super::outbox::{self, NewItem, OutboxKind};
use super::stock::{self, StockItem, StockReason};
use super::store::{self, CloudLink, CloudNoticeKind, CloudSettings, MapEntity};
use crate::catalog::products::{self, Product, ProductKind};
use crate::documents::{self, DocStatus, DocType};
use crate::error::AppError;
use crate::ids::new_id;
use crate::sync::changelog::{self, CLOUD_CONSUMER};
use rusqlite::{Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};

pub const DERIVE_BATCH: usize = 2_000;
pub const STOCK_BATCH_ITEMS: usize = 500;
/// Products an epoch looks at again per derive, so the single writer is never held long: about
/// 40 ms for 100 products on a development PC (each one loaded, decided, queued, its stock
/// planned), what a sale may wait behind a chunk. The engine derives again while `more`.
pub const EPOCH_BATCH: usize = 100;
const PUBLISH_EPOCH: &str = "cloud:publish_mode";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OnlineStatus {
    Active,
    /// Kept off the storefront (the plan's cap, or the owner's choice online).
    Draft,
    /// Was published, is not any more: leaves the storefront.
    Archived,
}

impl OnlineStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            OnlineStatus::Active => "active",
            OnlineStatus::Draft => "draft",
            OnlineStatus::Archived => "archived",
        }
    }
}

/// `None`: the product stays home. Variant families are not published in M7 (deferred row).
pub fn publish_decision(
    conn: &Connection,
    store_id: &str,
    product: &Product,
    settings: &CloudSettings,
) -> Result<Option<OnlineStatus>, AppError> {
    let flag: Option<(bool, String)> = conn
        .query_row(
            "SELECT publish, online_status FROM cloud_publish WHERE product_id = ?1",
            [&product.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let wanted = flag.as_ref().map_or(settings.publish_all, |(p, _)| *p);
    let eligible = !product.archived
        && matches!(
            product.kind,
            ProductKind::Simple | ProductKind::Composite | ProductKind::Service
        );
    if wanted && eligible {
        let draft = flag.is_some_and(|(_, s)| s == "draft");
        return Ok(Some(if draft {
            OnlineStatus::Draft
        } else {
            OnlineStatus::Active
        }));
    }
    let mapped = store::map_get(conn, store_id, MapEntity::Product, &product.id)?.is_some();
    Ok(mapped.then_some(OnlineStatus::Archived))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeriveReport {
    pub read: usize,
    pub products: usize,
    pub media: usize,
    pub stock_items: usize,
    pub sales: usize,
    pub refunds: usize,
    pub closures: usize,
    pub skipped: usize,
    /// Work is left (a full change-feed batch, or a catalogue walk under way): derive again in a
    /// new transaction, so sales commit in between.
    pub more: bool,
}

fn products_in_category(conn: &Connection, category_id: &str) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare("SELECT id FROM products WHERE category_id = ?1")?;
    let rows = stmt
        .query_map([category_id], |r| r.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    Ok(rows)
}

/// A walk of the catalogue, kept in the remote cursor `cloud:publish_mode`: which link and mode
/// it serves, and the last product id it passed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Epoch {
    link: String,
    mode: String,
    after: Option<String>,
    done: bool,
}

/// The next products of the walk after `after`, by id: every product in 'all' mode; in
/// 'selected' mode only the published ones and the ones this store has (to archive them).
fn epoch_chunk(
    conn: &Connection,
    store_id: &str,
    all: bool,
    after: &str,
) -> Result<Vec<String>, AppError> {
    let limit = i64::try_from(EPOCH_BATCH).unwrap_or(i64::MAX);
    let rows = if all {
        let mut stmt =
            conn.prepare_cached("SELECT id FROM products WHERE id > ?1 ORDER BY id LIMIT ?2")?;
        stmt.query_map(rusqlite::params![after, limit], |r| r.get(0))?
            .collect::<Result<Vec<String>, _>>()?
    } else {
        let mut stmt = conn.prepare_cached(
            "SELECT product_id FROM cloud_publish WHERE publish = 1 AND product_id > ?1
             UNION
             SELECT local_id FROM cloud_map WHERE store_id = ?3 AND entity_type = 'product' AND local_id > ?1
             ORDER BY 1 LIMIT ?2",
        )?;
        stmt.query_map(rusqlite::params![after, limit, store_id], |r| r.get(0))?
            .collect::<Result<Vec<String>, _>>()?
    };
    Ok(rows)
}

/// The products to look at again in this derive. The first derive of a link (a new link after an
/// unlink too: nothing was fed while no reader was registered) and a change of publish mode start
/// a walk of the catalogue, [`EPOCH_BATCH`] products per derive; `true` while it goes on. A new
/// link knows nothing of the stock DZBuild holds for its store: the baseline starts again, and
/// each product walked gets its level `set`.
fn publish_epoch(
    tx: &Transaction<'_>,
    link: &CloudLink,
    settings: &CloudSettings,
    now: i64,
) -> Result<(Vec<String>, bool), AppError> {
    let mode = if settings.publish_all {
        "all"
    } else {
        "selected"
    };
    let saved: Option<Epoch> = store::remote_cursor(tx, PUBLISH_EPOCH)?
        .and_then(|cursor| serde_json::from_str(&cursor).ok());
    let mut epoch = match saved {
        Some(epoch) if epoch.link == link.id && epoch.mode == mode => epoch,
        saved => {
            if saved.is_none_or(|epoch| epoch.link != link.id) {
                tx.execute(
                    "DELETE FROM cloud_stock WHERE store_id = ?1",
                    [&link.store_id],
                )?;
            }
            Epoch {
                link: link.id.clone(),
                mode: mode.to_owned(),
                after: None,
                done: false,
            }
        }
    };
    if epoch.done {
        return Ok((Vec::new(), false));
    }
    let ids = epoch_chunk(
        tx,
        &link.store_id,
        settings.publish_all,
        epoch.after.as_deref().unwrap_or(""),
    )?;
    epoch.done = ids.len() < EPOCH_BATCH;
    if let Some(last) = ids.last() {
        epoch.after = Some(last.clone());
    }
    store::set_remote_cursor(tx, PUBLISH_EPOCH, &serde_json::to_string(&epoch)?, now)?;
    Ok((ids, !epoch.done))
}

fn enqueue_product(
    tx: &Transaction<'_>,
    link: &CloudLink,
    settings: &CloudSettings,
    id: &str,
    report: &mut DeriveReport,
    now: i64,
) -> Result<(), AppError> {
    let Ok(product) = products::get(tx, id) else {
        return Ok(());
    };
    let Some(status) = publish_decision(tx, &link.store_id, &product, settings)? else {
        return Ok(());
    };
    let row = new_id();
    let queued = outbox::enqueue(
        tx,
        NewItem {
            store_id: link.store_id.clone(),
            kind: OutboxKind::ProductUpsert,
            entity_id: product.id.clone(),
            payload: json!({}),
            key: super::outbox_key("product", &row),
            depends_on: None,
        },
        now,
    )?;
    report.products += usize::from(queued.is_some());
    if status == OnlineStatus::Archived {
        return Ok(());
    }
    if stock::online_pieces(tx, &product)?.is_some() && settings.stock_push {
        stock::track(tx, &link.store_id, &product.id, now)?;
    }
    if let (true, Some(image)) = (settings.images_push, product.image.as_deref()) {
        let sent = store::map_get(tx, &link.store_id, MapEntity::Media, &product.id)?
            .and_then(|m| m.last_payload)
            .and_then(|p| p.get("file").and_then(Value::as_str).map(str::to_owned));
        if sent.as_deref() != Some(image) {
            let queued = outbox::enqueue(
                tx,
                NewItem {
                    store_id: link.store_id.clone(),
                    kind: OutboxKind::MediaUpload,
                    entity_id: product.id.clone(),
                    payload: json!({ "image": image }),
                    key: super::outbox_key("media", &format!("{}:{image}", product.id)),
                    depends_on: None,
                },
                now,
            )?;
            report.media += usize::from(queued.is_some());
        }
    }
    Ok(())
}

fn plan_stock(
    tx: &Transaction<'_>,
    link: &CloudLink,
    settings: &CloudSettings,
    changed: &BTreeMap<String, BTreeSet<StockReason>>,
    report: &mut DeriveReport,
    now: i64,
) -> Result<(), AppError> {
    if !settings.stock_push {
        return Ok(());
    }
    let mut todo = changed.clone();
    for id in stock::needing_plan(tx, &link.store_id)? {
        todo.entry(id).or_default();
    }
    let mut items: Vec<StockItem> = Vec::new();
    let batch_id = new_id();
    for (id, reasons) in &todo {
        let Ok(product) = products::get(tx, id) else {
            continue;
        };
        if !matches!(
            publish_decision(tx, &link.store_id, &product, settings)?,
            Some(OnlineStatus::Active | OnlineStatus::Draft)
        ) {
            continue;
        }
        let reason = match reasons.len() {
            1 => reasons.iter().next().copied().unwrap_or(StockReason::Count),
            _ => StockReason::Count,
        };
        let reference = format!("{batch_id}:{}", items.len());
        if let Some(item) = stock::plan(
            tx,
            &link.store_id,
            &product,
            reason,
            &reference,
            link.location_id.as_deref(),
        )? {
            items.push(item);
        }
    }
    report.stock_items = items.len();
    for chunk in items.chunks(STOCK_BATCH_ITEMS) {
        let row = new_id();
        outbox::enqueue(
            tx,
            NewItem {
                store_id: link.store_id.clone(),
                kind: OutboxKind::StockBatch,
                entity_id: row.clone(),
                payload: json!({ "items": chunk }),
                key: super::outbox_key("stock", &row),
                depends_on: None,
            },
            now,
        )?;
    }
    Ok(())
}

fn is_online_order_document(conn: &Connection, document_id: &str) -> Result<bool, AppError> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM online_orders WHERE local_document_id = ?1)",
        [document_id],
        |r| r.get(0),
    )?)
}

/// Where a refund of `document_id` (the returned or credited document) goes on DZBuild.
enum RefundOf {
    /// The pushed sale: the document itself or, for a credit note on an invoice made from a
    /// ticket or from one delivery note (M3), that ticket or note (the invoice is not pushed).
    Sale(String),
    /// A credit note on an invoice of several delivery notes, each pushed as its own sale: the
    /// refund cannot be told apart between them (ids and numbers).
    Notes(Vec<(String, Option<String>)>),
}

fn refund_of(conn: &Connection, document_id: &str) -> Result<RefundOf, AppError> {
    use crate::documents::links::{LinkKind, sources};
    let sources = sources(conn, document_id)?;
    if let Some(ticket) = sources
        .iter()
        .find(|s| s.link.kind == LinkKind::InvoicedTicket)
    {
        return Ok(RefundOf::Sale(ticket.link.from_id.clone()));
    }
    let mut notes: Vec<(String, Option<String>)> = sources
        .into_iter()
        .filter(|s| s.link.kind == LinkKind::Delivered)
        .map(|s| (s.link.from_id, s.number))
        .collect();
    Ok(match notes.len() {
        0 => RefundOf::Sale(document_id.to_owned()),
        1 => RefundOf::Sale(notes.remove(0).0),
        _ => RefundOf::Notes(notes),
    })
}

/// How `store_id` knows `sale`: `Some(Some(row))` while its outbox row is kept (a refund waits
/// for it), `Some(None)` once that row is pruned but DZBuild answered it (the mapping stays),
/// `None` when DZBuild never had it in this store (sold before the link, or in another store).
fn known_sale(
    conn: &Connection,
    store_id: &str,
    sale: &str,
) -> Result<Option<Option<String>>, AppError> {
    let row: Option<String> = conn
        .query_row(
            "SELECT id FROM sync_outbox WHERE target = 'cloud' AND store_id = ?1 AND kind = 'pos_sale'
               AND entity_id = ?2 AND status IN ('pending', 'sent', 'failed')",
            [store_id, sale],
            |r| r.get(0),
        )
        .optional()?;
    if row.is_some() {
        return Ok(Some(row));
    }
    let mapped = store::map_get(conn, store_id, MapEntity::Sale, sale)?.is_some();
    Ok(mapped.then_some(None))
}

fn enqueue_document(
    tx: &Transaction<'_>,
    link: &CloudLink,
    id: &str,
    report: &mut DeriveReport,
    now: i64,
) -> Result<(), AppError> {
    let doc = documents::get(tx, id)?;
    if doc.status != DocStatus::Validated || is_online_order_document(tx, &doc.id)? {
        report.skipped += 1;
        return Ok(());
    }
    let store_id = link.store_id.clone();
    match doc.doc_type {
        DocType::Ticket | DocType::Invoice | DocType::DeliveryNote => {
            use crate::documents::links::{LinkKind, sources};
            let counted_elsewhere = sources(tx, &doc.id)?
                .iter()
                .any(|s| matches!(s.link.kind, LinkKind::InvoicedTicket | LinkKind::Delivered));
            if counted_elsewhere {
                report.skipped += 1;
                return Ok(());
            }
            let queued = outbox::enqueue(
                tx,
                NewItem {
                    store_id,
                    kind: OutboxKind::PosSale,
                    entity_id: doc.id.clone(),
                    payload: json!({}),
                    key: super::outbox_key("sale", &doc.id),
                    depends_on: None,
                },
                now,
            )?;
            report.sales += usize::from(queued.is_some());
        }
        DocType::ReturnNote | DocType::CreditNote => {
            let Some(parent) = doc.parent_id.as_deref() else {
                report.skipped += 1;
                return Ok(());
            };
            if is_online_order_document(tx, parent)? {
                store::raise_notice(
                    tx,
                    CloudNoticeKind::Order,
                    "online_order_returned_in_shop",
                    parent,
                    json!({ "document": doc.number }),
                    now,
                )?;
                report.skipped += 1;
                return Ok(());
            }
            let sale = match refund_of(tx, parent)? {
                RefundOf::Sale(sale) => sale,
                RefundOf::Notes(notes) => {
                    let mut pushed = false;
                    for (note, _) in &notes {
                        pushed |= known_sale(tx, &store_id, note)?.is_some();
                    }
                    if pushed {
                        let subject = doc.number.clone().unwrap_or_else(|| doc.id.clone());
                        let deliveries: Vec<_> = notes.into_iter().map(|(_, n)| n).collect();
                        store::raise_notice(
                            tx,
                            CloudNoticeKind::FailedItems,
                            "cloud_refund_unmatched",
                            &subject,
                            json!({ "document": doc.number, "deliveries": deliveries }),
                            now,
                        )?;
                    }
                    report.skipped += 1;
                    return Ok(());
                }
            };
            let Some(depends_on) = known_sale(tx, &store_id, &sale)? else {
                report.skipped += 1; // its sale predates the link, or went to another store
                return Ok(());
            };
            let queued = outbox::enqueue(
                tx,
                NewItem {
                    store_id,
                    kind: OutboxKind::PosRefund,
                    entity_id: doc.id.clone(),
                    payload: json!({ "sale": sale }),
                    key: super::outbox_key("refund", &doc.id),
                    depends_on,
                },
                now,
            )?;
            report.refunds += usize::from(queued.is_some());
        }
        _ => report.skipped += 1,
    }
    Ok(())
}

pub fn derive(
    tx: &Transaction<'_>,
    link: &CloudLink,
    settings: &CloudSettings,
    now: i64,
) -> Result<DeriveReport, AppError> {
    let from = changelog::position(tx, CLOUD_CONSUMER)?
        .ok_or_else(|| AppError::sync("cloud_not_linked"))?;
    let rows = changelog::read_after(tx, from, DERIVE_BATCH)?;
    let mut report = DeriveReport {
        read: rows.len(),
        ..DeriveReport::default()
    };
    let mut touched: BTreeSet<String> = BTreeSet::new();
    let mut stock_moves: BTreeMap<String, BTreeSet<StockReason>> = BTreeMap::new();
    let (mut documents, mut closures) = (Vec::new(), Vec::new());
    let (walked, walking) = publish_epoch(tx, link, settings, now)?;
    touched.extend(walked);
    report.more = walking || rows.len() == DERIVE_BATCH;
    for row in &rows {
        match row.table_name.as_str() {
            "products" | "cloud_publish" => {
                touched.insert(row.row_id.clone());
            }
            "product_barcodes" => {
                if let Some(p) = row.field("product_id") {
                    touched.insert(p.to_owned());
                }
            }
            "categories" => touched.extend(products_in_category(tx, &row.row_id)?),
            "stock_movements" if row.field("ref_type") != Some("dzbuild") => {
                if let Some(p) = row.field("product_id") {
                    let reason = StockReason::of_movement(row.field("reason").unwrap_or(""));
                    stock_moves.entry(p.to_owned()).or_default().insert(reason);
                }
            }
            "documents" => documents.push(row.row_id.clone()),
            "cash_closures" => closures.push(row.row_id.clone()),
            _ => {}
        }
    }
    for id in &touched {
        enqueue_product(tx, link, settings, id, &mut report, now)?;
    }
    plan_stock(tx, link, settings, &stock_moves, &mut report, now)?;
    if settings.sales_push {
        for id in &documents {
            enqueue_document(tx, link, id, &mut report, now)?;
        }
        for session in &closures {
            let queued = outbox::enqueue(
                tx,
                NewItem {
                    store_id: link.store_id.clone(),
                    kind: OutboxKind::Closure,
                    entity_id: session.clone(),
                    payload: json!({}),
                    key: super::outbox_key("closure", session),
                    depends_on: None,
                },
                now,
            )?;
            report.closures += usize::from(queued.is_some());
        }
    }
    if let Some(last) = rows.last() {
        changelog::advance(tx, CLOUD_CONSUMER, last.seq, now)?;
    }
    changelog::prune(tx)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::store::cloud_settings;
    use crate::cloud::testkit::{linked_shop, outbox_rows, publish, return_all};
    use crate::money::{Money, Qty};
    use crate::payments::PaymentInput;
    use crate::sales::{Cart, CartLine, SaleInput, create_sale};
    use crate::testkit::{NOON, Shop};

    fn run(shop: &Shop, link: &CloudLink) -> DeriveReport {
        let link = link.clone();
        shop.db
            .write(move |tx| {
                let settings = cloud_settings(tx)?;
                derive(tx, &link, &settings, NOON)
            })
            .unwrap()
    }

    fn sell(shop: &Shop, product_id: &str, pieces: i64, cash: i64) -> String {
        let ctx = shop.till();
        let input = SaleInput {
            cart: Cart::new(
                crate::lang::Lang::Fr,
                vec![CartLine::product(product_id, Qty(pieces * 1_000))],
            ),
            payments: vec![PaymentInput::cash(Money(cash))],
        };
        shop.db
            .write(move |tx| create_sale(tx, &ctx, input))
            .unwrap()
            .document
            .id
    }

    #[test]
    fn deriving_twice_enqueues_each_sale_once() {
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        publish(&shop, &milk.id);
        shop.stock_in(&milk.id, 10_000);
        shop.open_till(0);
        let ticket = sell(&shop, &milk.id, 2, 20_000);
        let report = run(&shop, &link);
        assert_eq!((report.sales, report.products), (1, 1));
        run(&shop, &link);
        shop.db
            .write(|tx| {
                tx.execute(
                    "UPDATE sync_cursors SET last_seq = 0 WHERE name = 'changelog:cloud'",
                    [],
                )?;
                Ok(())
            })
            .unwrap();
        run(&shop, &link); // the feed replayed from the start
        let sales: Vec<_> = outbox_rows(&shop)
            .into_iter()
            .filter(|r| r.0 == "pos_sale")
            .collect();
        assert_eq!(
            sales,
            [("pos_sale".to_owned(), ticket.clone(), "pending".to_owned())]
        );
        let key: String = shop
            .db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT idempotency_key FROM sync_outbox WHERE kind = 'pos_sale'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(key, format!("sale:{ticket}"));
    }

    #[test]
    fn the_cursor_moves_with_the_outbox_in_one_transaction() {
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        publish(&shop, &milk.id);
        let failing = link.clone();
        let err = shop
            .db
            .write(move |tx| {
                let settings = cloud_settings(tx)?;
                derive(tx, &failing, &settings, NOON)?;
                Err::<(), _>(AppError::Internal("power cut".into()))
            })
            .unwrap_err();
        assert_eq!(err.code(), "internal_error");
        assert!(outbox_rows(&shop).is_empty(), "rolled back together");
        assert_eq!(
            shop.db
                .read(|c| changelog::position(c, changelog::CLOUD_CONSUMER))
                .unwrap(),
            Some(0),
            "the cursor did not move either"
        );
        assert_eq!(
            run(&shop, &link).products,
            1,
            "the next derive does the work"
        );
    }

    #[test]
    fn unpublished_products_stay_home_and_weights_publish_without_stock() {
        let (shop, link) = linked_shop();
        let secret = shop.product("Pas en ligne", Money(1_000), &shop.tva19);
        shop.stock_in(&secret.id, 5_000);
        let mut cheese = crate::catalog::products::ProductInput::simple(
            "Fromage",
            crate::catalog::UNIT_KG_ID,
            &shop.tva9,
            Money(150_000),
        );
        cheese.is_weighed = true;
        let cheese = shop.product_with(cheese);
        publish(&shop, &cheese.id);
        shop.stock_in(&cheese.id, 2_500);
        let report = run(&shop, &link);
        assert_eq!(
            (report.products, report.stock_items),
            (1, 0),
            "the cheese only, and no stock for a weight"
        );
        assert!(outbox_rows(&shop).iter().all(|r| r.1 != secret.id));
    }

    #[test]
    fn a_published_product_gets_a_set_then_deltas_and_moves_dzbuild_made_are_not_pushed() {
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        publish(&shop, &milk.id);
        shop.stock_in(&milk.id, 12_000);
        assert_eq!(run(&shop, &link).stock_items, 1);
        let batch = |shop: &Shop| -> serde_json::Value {
            shop.db
                .read(|c| {
                    let text: String = c.query_row(
                        "SELECT payload FROM sync_outbox WHERE kind = 'stock_batch' ORDER BY created_at DESC, rowid DESC LIMIT 1",
                        [],
                        |r| r.get(0),
                    )?;
                    Ok(serde_json::from_str(&text)?)
                })
                .unwrap()
        };
        let first = batch(&shop);
        assert_eq!(
            first["items"][0]["set"], 12,
            "first publication: set the level"
        );
        assert_eq!(first["items"][0]["product_external_id"], milk.id);
        assert_eq!(first["items"][0]["location_id"], "loc_1");
        shop.open_till(0);
        sell(&shop, &milk.id, 3, 30_000);
        assert_eq!(run(&shop, &link).stock_items, 1);
        let second = batch(&shop);
        assert_eq!(
            (
                second["items"][0]["delta"].clone(),
                second["items"][0]["reason"].clone()
            ),
            (serde_json::json!(-3), serde_json::json!("pos_sale"))
        );
        // DZBuild's own move, applied locally and acknowledged in the same transaction
        let (store, id) = (link.store_id.clone(), milk.id.clone());
        let actor = shop.owner.actor();
        shop.db
            .write(move |tx| {
                crate::stock::append(
                    tx,
                    &actor,
                    &crate::stock::NewMovement {
                        product_id: id.clone(),
                        location_id: crate::stock::SHOP_LOCATION_ID.into(),
                        qty_delta: Qty(-1_000),
                        reason: crate::stock::MovementReason::Adjustment,
                        reason_code: Some(crate::stock::AdjustReason::Correction),
                        ref_type: Some("dzbuild".into()),
                        ref_id: Some("evt_1".into()),
                        unit_cost: None,
                        note: None,
                    },
                    NOON,
                )?;
                crate::cloud::stock::acknowledge(tx, &store, &id, -1, NOON)
            })
            .unwrap();
        assert_eq!(run(&shop, &link).stock_items, 0, "never echoed back");
    }

    #[test]
    fn a_return_waits_for_its_sale_and_a_ticket_from_before_the_link_is_skipped() {
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        shop.stock_in(&milk.id, 10_000);
        shop.open_till(0);
        let ticket = sell(&shop, &milk.id, 1, 10_000);
        run(&shop, &link);
        let sale_row: String = shop
            .db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT id FROM sync_outbox WHERE kind = 'pos_sale'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        return_all(&shop, &ticket);
        run(&shop, &link);
        let depends: Option<String> = shop
            .db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT depends_on FROM sync_outbox WHERE kind = 'pos_refund'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(depends, Some(sale_row));

        // a ticket sold while no reader was registered (before the link): its return refunds
        // nothing DZBuild knows
        shop.db
            .write(|tx| changelog::unregister(tx, changelog::CLOUD_CONSUMER))
            .unwrap();
        let unlinked_ticket = sell(&shop, &milk.id, 1, 10_000);
        shop.db
            .write(|tx| changelog::register(tx, changelog::CLOUD_CONSUMER, NOON).map(drop))
            .unwrap();
        return_all(&shop, &unlinked_ticket);
        let report = run(&shop, &link);
        assert_eq!((report.refunds, report.skipped), (0, 1));
    }

    #[test]
    fn switching_to_publish_all_enqueues_the_catalogue_once() {
        let (shop, link) = linked_shop();
        for name in ["A", "B", "C"] {
            shop.product(name, Money(1_000), &shop.tva19);
        }
        assert_eq!(
            run(&shop, &link).products,
            0,
            "selected mode, nothing selected"
        );
        shop.set("cloud.publish.mode", serde_json::json!("all"));
        assert_eq!(run(&shop, &link).products, 3);
        assert_eq!(run(&shop, &link).products, 0, "the switch is applied once");
    }

    /// What `link::unlink` writes: the link ends, its reader goes, its open items are skipped.
    fn unlink(shop: &Shop, link: &CloudLink) {
        let id = link.id.clone();
        shop.db
            .write(move |tx| {
                store::set_status(tx, &id, store::LinkStatus::Unlinked, NOON)?;
                changelog::unregister(tx, CLOUD_CONSUMER)?;
                changelog::prune(tx)?;
                tx.execute(
                    "UPDATE sync_outbox SET status = 'skipped', last_error = 'unlinked'
                     WHERE target = 'cloud' AND status IN ('pending', 'failed')",
                    [],
                )?;
                Ok(())
            })
            .unwrap();
    }

    /// A new link after an unlink (`link::finish` gives it a new id), to `store_id`.
    fn link_to(shop: &Shop, id: &str, store_id: &str) -> CloudLink {
        let new = store::NewLink {
            id: id.into(),
            account_sub: "84211".into(),
            account_name: None,
            store_id: store_id.into(),
            store_name: "Boutique Amel".into(),
            store_language: crate::lang::Lang::Fr,
            store_plan: None,
            public_url: None,
            location_id: Some("loc_2".into()),
            device_id: Some("dev_2".into()),
            scopes: vec!["offline_access".into()],
            stock_deduction: None,
            active_products_cap: None,
        };
        shop.db
            .write(move |tx| {
                let link = store::insert_link(tx, &new, NOON)?;
                changelog::register(tx, CLOUD_CONSUMER, NOON)?;
                Ok(link)
            })
            .unwrap()
    }

    fn strings(shop: &Shop, sql: &'static str) -> Vec<String> {
        shop.db
            .read(move |c| {
                let mut stmt = c.prepare(sql)?;
                let rows = stmt
                    .query_map([], |r| r.get(0))?
                    .collect::<Result<Vec<String>, _>>()?;
                Ok(rows)
            })
            .unwrap()
    }

    fn json_of(text: &str) -> serde_json::Value {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn a_link_after_an_unlink_publishes_the_catalogue_and_its_stock_to_the_new_store() {
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        publish(&shop, &milk.id);
        shop.stock_in(&milk.id, 12_000);
        assert_eq!(run(&shop, &link).products, 1);
        unlink(&shop, &link);
        // published while unlinked: no reader, so nothing in the change feed
        let bread = shop.product("Pain", Money(1_500), &shop.tva9);
        publish(&shop, &bread.id);
        shop.stock_in(&bread.id, 4_000);
        let other = link_to(&shop, "00000000-0000-7000-8000-00000000c002", "142");
        let report = run(&shop, &other);
        assert_eq!(
            (report.products, report.stock_items),
            (2, 2),
            "the new store gets the catalogue and its stock"
        );
        let mut expected = vec![milk.id.clone(), bread.id.clone()];
        expected.sort();
        assert_eq!(
            strings(
                &shop,
                "SELECT entity_id FROM sync_outbox WHERE store_id = '142' AND kind = 'product_upsert'
                   AND status = 'pending' ORDER BY entity_id"
            ),
            expected
        );
        assert_eq!(
            strings(
                &shop,
                "SELECT product_id FROM cloud_stock WHERE store_id = '142' AND set_in_flight IS NOT NULL
                 ORDER BY product_id"
            ),
            expected,
            "both levels are set on store 142"
        );
        assert_eq!(
            run(&shop, &other).products,
            0,
            "the new link's walk is done"
        );
    }

    #[test]
    fn a_link_to_the_same_store_after_an_unlink_sets_its_stock_again() {
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        publish(&shop, &milk.id);
        shop.stock_in(&milk.id, 12_000);
        assert_eq!(
            run(&shop, &link).stock_items,
            1,
            "a set of 12, never answered"
        );
        unlink(&shop, &link);
        shop.stock_in(&milk.id, 3_000);
        let again = link_to(&shop, "00000000-0000-7000-8000-00000000c003", "141");
        assert_eq!(run(&shop, &again).stock_items, 1);
        let batch = strings(
            &shop,
            "SELECT payload FROM sync_outbox WHERE kind = 'stock_batch' AND status = 'pending'",
        );
        assert_eq!(batch.len(), 1, "the skipped batch is not waited for");
        assert_eq!(
            json_of(&batch[0])["items"][0]["set"],
            15,
            "the level is set again"
        );
    }

    /// DZBuild answered the sale: its row is sent and the sale is mapped in `store_id`.
    fn sent_and_mapped(shop: &Shop, store_id: &str, sale: &str, remote: &str) {
        let (store_id, sale, remote) = (store_id.to_owned(), sale.to_owned(), remote.to_owned());
        shop.db
            .write(move |tx| {
                let row: String = tx.query_row(
                    "SELECT id FROM sync_outbox WHERE kind = 'pos_sale' AND entity_id = ?1",
                    [&sale],
                    |r| r.get(0),
                )?;
                outbox::mark_sent(tx, &row, Some(&remote), NOON)?;
                store::map_put(
                    tx,
                    &store_id,
                    MapEntity::Sale,
                    &sale,
                    &remote,
                    None,
                    None,
                    NOON,
                )
            })
            .unwrap();
    }

    fn refund_row(shop: &Shop) -> (Option<String>, serde_json::Value) {
        shop.db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT depends_on, payload FROM sync_outbox WHERE kind = 'pos_refund'",
                    [],
                    |r| Ok((r.get(0)?, json_of(&r.get::<_, String>(1)?))),
                )?)
            })
            .unwrap()
    }

    #[test]
    fn a_refund_after_its_sale_row_was_pruned_follows_the_mapping_of_its_own_store() {
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        shop.stock_in(&milk.id, 10_000);
        shop.open_till(0);
        let ticket = sell(&shop, &milk.id, 1, 10_000);
        run(&shop, &link);
        sent_and_mapped(&shop, "141", &ticket, "99120");
        let pruned = shop
            .db
            .write(|tx| outbox::prune_sent(tx, NOON + outbox::PRUNE_AFTER_MS + 1))
            .unwrap();
        assert_eq!(pruned, 1, "the sale's row is gone 30 days on");
        return_all(&shop, &ticket);
        assert_eq!(run(&shop, &link).refunds, 1);
        let (depends, payload) = refund_row(&shop);
        assert_eq!(depends, None, "the sale is on DZBuild: nothing to wait for");
        assert_eq!(payload["sale"], ticket);

        // a sale DZBuild holds in another store refunds nothing in this one
        let second = sell(&shop, &milk.id, 1, 10_000);
        run(&shop, &link);
        sent_and_mapped(&shop, "141", &second, "99121");
        unlink(&shop, &link);
        let other = link_to(&shop, "00000000-0000-7000-8000-00000000c002", "142");
        run(&shop, &other);
        return_all(&shop, &second);
        let report = run(&shop, &other);
        assert_eq!((report.refunds, report.skipped), (0, 1));
    }

    fn delivery(
        shop: &Shop,
        who: &crate::customers::Customer,
        product: &crate::catalog::products::Product,
        qty: i64,
    ) -> documents::Document {
        use crate::billing::drafts::tests::{cart, input, save};
        use crate::billing::validate::{ValidateInput, validate};
        let draft = save(
            shop,
            None,
            input(
                DocType::DeliveryNote,
                cart(Some(who), vec![CartLine::product(&product.id, Qty(qty))]),
            ),
        )
        .unwrap();
        let ctx = shop.owner_till();
        shop.db
            .write(move |tx| {
                validate(
                    tx,
                    &ctx,
                    ValidateInput {
                        id: draft.id,
                        payments: Vec::new(),
                    },
                )
            })
            .unwrap()
            .document
    }

    fn invoice_notes(shop: &Shop, notes: &[&documents::Document]) -> documents::Document {
        use crate::billing::convert::{DeliveriesInput, invoice_deliveries};
        let total: i64 = notes.iter().map(|n| n.total_ttc.0).sum();
        let ids = notes.iter().map(|n| n.id.clone()).collect();
        let ctx = shop.owner_till();
        shop.db
            .write(move |tx| {
                invoice_deliveries(
                    tx,
                    &ctx,
                    DeliveriesInput {
                        delivery_ids: ids,
                        payments: vec![PaymentInput::electronic(
                            crate::payments::PaymentMethod::Transfer,
                            Money(total),
                            Some("VIR-2026-0931"),
                        )],
                        due_date: None,
                        second_lang: None,
                        note: None,
                    },
                )
            })
            .unwrap()
            .document
    }

    fn credit_one(shop: &Shop, invoice: &documents::Document) -> documents::Document {
        use crate::billing::credit_notes::{
            CreditLineInput, CreditNoteInput, CreditReason, create_credit_note,
        };
        let input = CreditNoteInput {
            invoice_id: invoice.id.clone(),
            kind: crate::documents::CreditKind::Goods,
            lines: vec![CreditLineInput {
                origin_line_id: invoice.lines[0].id.clone(),
                qty: Qty(1_000),
                restock: true,
            }],
            amount: None,
            reason: CreditReason::Defective,
            refund_method: crate::payments::PaymentMethod::Transfer,
            refund_reference: Some("VIR-2026-0932".into()),
            note: None,
            second_lang: None,
        };
        let ctx = shop.owner_till();
        shop.db
            .write(move |tx| create_credit_note(tx, &ctx, input))
            .unwrap()
            .document
    }

    #[test]
    fn a_credit_note_on_an_invoice_of_delivery_notes_refunds_the_note_dzbuild_has() {
        let (shop, link) = linked_shop();
        let (amine, cement, sand) = crate::billing::drafts::tests::fixture(&shop);
        shop.stock_in(&cement.id, 100_000);
        shop.stock_in(&sand.id, 100_000);
        let bl = delivery(&shop, &amine, &cement, 3_000);
        assert_eq!(run(&shop, &link).sales, 1, "the note is the sale");
        let bl_row = strings(&shop, "SELECT id FROM sync_outbox WHERE kind = 'pos_sale'");
        let invoice = invoice_notes(&shop, &[&bl]);
        let report = run(&shop, &link);
        assert_eq!(
            (report.sales, report.skipped),
            (0, 1),
            "the invoice repeats the note"
        );
        credit_one(&shop, &invoice);
        assert_eq!(run(&shop, &link).refunds, 1);
        let (depends, payload) = refund_row(&shop);
        assert_eq!(depends.as_deref(), bl_row.first().map(String::as_str));
        assert_eq!(payload["sale"], bl.id);

        // an invoice of two notes: the refund cannot be told apart between them, the owner is told
        let first = delivery(&shop, &amine, &cement, 1_000);
        let second = delivery(&shop, &amine, &sand, 1_000);
        assert_eq!(run(&shop, &link).sales, 2);
        let both = invoice_notes(&shop, &[&first, &second]);
        run(&shop, &link);
        let av = credit_one(&shop, &both);
        let report = run(&shop, &link);
        assert_eq!((report.refunds, report.skipped), (0, 1));
        let notices = shop.db.read(|c| store::notices(c, false)).unwrap();
        let unmatched: Vec<_> = notices
            .iter()
            .filter(|n| n.code == "cloud_refund_unmatched")
            .collect();
        assert_eq!(unmatched.len(), 1, "{notices:?}");
        assert_eq!(Some(unmatched[0].subject.clone()), av.number);
        assert_eq!(
            unmatched[0].details["deliveries"],
            serde_json::json!([first.number, second.number])
        );
    }

    #[test]
    fn a_newer_photo_replaces_the_one_still_waiting() {
        let (shop, link) = linked_shop();
        let milk = shop.product("Lait", Money(10_000), &shop.tva19);
        publish(&shop, &milk.id);
        let photo = |version: i64| {
            let name = crate::catalog::images::file_name(&milk.id, version);
            let (actor, id, set) = (shop.owner.actor(), milk.id.clone(), name.clone());
            shop.db
                .write(move |tx| crate::catalog::images::set(tx, &actor, &id, Some(&set)).map(drop))
                .unwrap();
            name
        };
        photo(1);
        assert_eq!(run(&shop, &link).media, 1);
        let newer = photo(2);
        run(&shop, &link);
        let waiting = strings(
            &shop,
            "SELECT payload FROM sync_outbox WHERE kind = 'media_upload' AND status = 'pending'",
        );
        assert_eq!(waiting.len(), 1, "one photo waits");
        assert_eq!(json_of(&waiting[0])["image"], newer, "the newer one");
        assert_eq!(
            strings(
                &shop,
                "SELECT idempotency_key FROM sync_outbox WHERE kind = 'media_upload'"
            ),
            [crate::cloud::outbox_key(
                "media",
                &format!("{}:{newer}", milk.id)
            )]
        );
    }

    #[test]
    fn the_catalogue_is_walked_a_chunk_per_derive_and_selected_mode_reads_only_its_products() {
        let (shop, link) = linked_shop();
        // a catalogue from before the link: nothing of it is in the change feed
        shop.db
            .write(|tx| changelog::unregister(tx, CLOUD_CONSUMER))
            .unwrap();
        let rate = shop.tva19.clone();
        let ids = shop
            .db
            .write(move |tx| {
                (0..EPOCH_BATCH + 3)
                    .map(|n| {
                        let input = crate::catalog::products::ProductInput::simple(
                            &format!("P{n}"),
                            crate::catalog::UNIT_PIECE_ID,
                            &rate,
                            Money(1_000),
                        );
                        products::create(tx, &crate::audit::Actor::system(None), input)
                            .map(|p| p.id)
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .unwrap();
        let (mapped, kept_home) = (ids[8].clone(), ids[9].clone());
        publish(&shop, &ids[7]);
        shop.db
            .write(move |tx| {
                tx.execute(
                    "INSERT INTO cloud_publish (product_id, publish, updated_at) VALUES (?1, 0, 0)",
                    [&kept_home],
                )?;
                store::map_put(
                    tx,
                    "141",
                    MapEntity::Product,
                    &mapped,
                    "p_9",
                    None,
                    None,
                    NOON,
                )?;
                changelog::register(tx, CLOUD_CONSUMER, NOON).map(drop)
            })
            .unwrap();
        let selected = run(&shop, &link);
        assert_eq!(
            (selected.products, selected.more),
            (2, false),
            "selected: the published and the mapped product, read in one go"
        );
        shop.set("cloud.publish.mode", serde_json::json!("all"));
        let walk = run(&shop, &link);
        assert!(walk.more, "the walk goes on in the next derive");
        assert!(walk.products <= EPOCH_BATCH && walk.stock_items <= EPOCH_BATCH);
        let rest = run(&shop, &link);
        assert!(!rest.more, "the walk is over");
        assert_eq!(
            selected.products + walk.products + rest.products,
            EPOCH_BATCH + 3 - 1,
            "every product once, except the one the owner keeps home"
        );
        let again = run(&shop, &link);
        assert_eq!((again.products, again.more), (0, false));
    }

    #[test]
    fn a_z_close_is_enqueued_once() {
        let (shop, link) = linked_shop();
        shop.open_till(10_000);
        let ctx = shop.till();
        shop.db
            .write(move |tx| {
                crate::cash::closing::close_session(
                    tx,
                    &ctx,
                    crate::cash::closing::CloseInput {
                        counts: vec![crate::cash::closing::CountLine {
                            denomination: Money(10_000),
                            count: 1,
                        }],
                        notes: None,
                    },
                )
            })
            .unwrap();
        assert_eq!(run(&shop, &link).closures, 1);
        run(&shop, &link);
        assert_eq!(
            outbox_rows(&shop)
                .iter()
                .filter(|r| r.0 == "closure")
                .count(),
            1
        );
    }
}

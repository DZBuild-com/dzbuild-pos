//! Lots with expiry dates (PRD-051/052/053, STK-004, décret 13-378): created when goods come in,
//! their stock per location cached in `lot_levels` by the movement trigger (and rebuilt by
//! `stock::rebuild_levels`), consumed first-expired first-out. A sale never takes an expired lot
//! before a fresh one; with `stock.expired_lots = block` a sale that only expired lots could
//! cover is refused. Stock leaving for any other reason takes expired lots first.

use super::{MovementReason, NewMovement, append, append_with_lot, level};
use crate::audit::Actor;
use crate::catalog::{extras, products};
use crate::error::AppError;
use crate::ids::new_id;
use crate::money::Qty;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LotInput {
    pub lot_number: String,
    pub expiry_date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Lot {
    pub id: String,
    pub product_id: String,
    pub lot_number: String,
    pub expiry_date: Option<String>,
    pub supplier_id: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LotStock {
    pub lot: Lot,
    pub location_id: String,
    pub qty: Qty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ExpiredPolicy {
    Warn,
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LotTake {
    pub lot_id: Option<String>,
    pub lot_number: Option<String>,
    pub qty: Qty,
    pub expired: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ExpiryStatus {
    Expired,
    Soon,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExpiryRow {
    pub product_id: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub lot_id: String,
    pub lot_number: String,
    pub expiry_date: String,
    pub location_id: String,
    pub qty: Qty,
    pub days_left: i64,
    pub status: ExpiryStatus,
}

const SELECT: &str =
    "SELECT id, product_id, lot_number, expiry_date, supplier_id, created_at FROM lots";

fn map(r: &Row<'_>) -> rusqlite::Result<Lot> {
    Ok(Lot {
        id: r.get(0)?,
        product_id: r.get(1)?,
        lot_number: r.get(2)?,
        expiry_date: r.get(3)?,
        supplier_id: r.get(4)?,
        created_at: r.get(5)?,
    })
}

pub fn find(
    conn: &Connection,
    product_id: &str,
    lot_number: &str,
) -> Result<Option<Lot>, AppError> {
    Ok(conn
        .prepare_cached(&format!(
            "{SELECT} WHERE product_id = ?1 AND lot_number = ?2"
        ))?
        .query_row(params![product_id, lot_number.trim()], map)
        .optional()?)
}

pub fn get_or_create(
    tx: &Transaction<'_>,
    product_id: &str,
    lot: &LotInput,
    supplier_id: Option<&str>,
    now: i64,
) -> Result<Lot, AppError> {
    let number = crate::catalog::clean_opt(Some(lot.lot_number.as_str()), "lotNumber", 40)?
        .ok_or_else(|| AppError::validation("lotNumber", "required"))?;
    let expiry = crate::party::clean_day(lot.expiry_date.as_deref(), "expiryDate")?;
    if let Some(existing) = find(tx, product_id, &number)? {
        if existing.expiry_date.is_none() && expiry.is_some() {
            tx.execute(
                "UPDATE lots SET expiry_date = ?2, updated_at = ?3 WHERE id = ?1",
                params![existing.id, expiry, now],
            )?;
        }
        return find(tx, product_id, &number)?.ok_or_else(|| AppError::not_found("lot"));
    }
    let id = new_id();
    tx.execute(
        "INSERT INTO lots (id, product_id, lot_number, expiry_date, supplier_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        params![id, product_id, number, expiry, supplier_id, now],
    )?;
    find(tx, product_id, &number)?.ok_or_else(|| AppError::not_found("lot"))
}

pub fn stock_of(conn: &Connection, product_id: &str) -> Result<Vec<LotStock>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT l.id, l.product_id, l.lot_number, l.expiry_date, l.supplier_id, l.created_at,
                ll.location_id, ll.qty
         FROM lot_levels ll JOIN lots l ON l.id = ll.lot_id
         WHERE l.product_id = ?1 AND ll.qty <> 0
         ORDER BY l.expiry_date IS NULL, l.expiry_date, l.created_at",
    )?;
    let rows = stmt
        .query_map([product_id], |r| {
            Ok(LotStock {
                lot: map(r)?,
                location_id: r.get(6)?,
                qty: r.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn expired_policy(conn: &Connection) -> Result<ExpiredPolicy, AppError> {
    crate::settings::get(conn, "stock.expired_lots")
}

fn part(m: &NewMovement, qty: Qty) -> NewMovement {
    NewMovement {
        product_id: m.product_id.clone(),
        location_id: m.location_id.clone(),
        qty_delta: qty,
        reason: m.reason,
        reason_code: m.reason_code,
        ref_type: m.ref_type.clone(),
        ref_id: m.ref_id.clone(),
        unit_cost: m.unit_cost,
        note: m.note.clone(),
    }
}

/// Takes `-m.qty_delta` out of the lots of `m.location_id`, then from its stock without a lot.
///
/// A sale (PRD-053) takes fresh lots first, the soonest expiry first, and expired lots only under
/// `stock.expired_lots = warn`; under `block` it is refused (`lot_expired`) when fresh lots and
/// stock without a lot cannot cover it. Every other reason (a write-off, a count, a transfer, a
/// supplier return) takes every lot first-expired first-out, so expired lots leave first, and is
/// never refused: expired stock must always be able to leave. Products without lot tracking get
/// one plain movement and an empty list.
pub fn take(
    tx: &Transaction<'_>,
    actor: &Actor,
    m: &NewMovement,
    today: &str,
    now: i64,
) -> Result<Vec<LotTake>, AppError> {
    take_rows(tx, actor, m, today, now).map(|(takes, _)| takes)
}

/// [`take`], with the ids of the ledger rows it appended, in order (one plain row for a product
/// without lot tracking).
pub(crate) fn take_rows(
    tx: &Transaction<'_>,
    actor: &Actor,
    m: &NewMovement,
    today: &str,
    now: i64,
) -> Result<(Vec<LotTake>, Vec<String>), AppError> {
    if !m.qty_delta.is_negative() {
        return Err(AppError::validation("qtyDelta", "qty_invalid"));
    }
    if !extras::get(tx, &m.product_id)?.track_lots {
        let id = append(tx, actor, m, now)?;
        return Ok((Vec::new(), vec![id]));
    }
    let mut need = m
        .qty_delta
        .checked_neg()
        .ok_or_else(|| AppError::validation("qtyDelta", "qty_invalid"))?;
    // (id, number, qty, expired), first-expired first: expired lots come before fresh ones.
    let lots: Vec<(String, String, Qty, bool)> = {
        let mut stmt = tx.prepare_cached(
            "SELECT l.id, l.lot_number, l.expiry_date, ll.qty
             FROM lot_levels ll JOIN lots l ON l.id = ll.lot_id
             WHERE l.product_id = ?1 AND ll.location_id = ?2 AND ll.qty > 0
             ORDER BY l.expiry_date IS NULL, l.expiry_date, l.created_at",
        )?;
        stmt.query_map(params![m.product_id, m.location_id], |r| {
            let expiry: Option<String> = r.get(2)?;
            let expired = expiry.as_deref().is_some_and(|e| e < today);
            Ok((r.get(0)?, r.get(1)?, r.get(3)?, expired))
        })?
        .collect::<Result<Vec<_>, _>>()?
    };
    let order = if m.reason == MovementReason::Sale {
        sale_order(tx, m, lots, need)?
    } else {
        lots
    };
    let mut takes = Vec::new();
    let mut ids = Vec::new();
    for (lot_id, number, available, was_expired) in order {
        if !need.is_positive() {
            break;
        }
        let qty = need.min(available);
        ids.push(append_with_lot(
            tx,
            actor,
            &part(m, Qty(-qty.0)),
            Some(&lot_id),
            now,
        )?);
        takes.push(LotTake {
            lot_id: Some(lot_id),
            lot_number: Some(number),
            qty,
            expired: was_expired,
        });
        need = need.checked_sub(qty).unwrap_or(Qty::ZERO);
    }
    if need.is_positive() {
        ids.push(append(tx, actor, &part(m, Qty(-need.0)), now)?);
        takes.push(LotTake {
            lot_id: None,
            lot_number: None,
            qty: need,
            expired: false,
        });
    }
    Ok((takes, ids))
}

/// The lots a sale may take, in order (PRD-053): fresh lots, then expired ones under `warn` only.
/// Under `block`, refuses when fresh lots and stock without a lot cannot cover `need`.
fn sale_order(
    tx: &Transaction<'_>,
    m: &NewMovement,
    lots: Vec<(String, String, Qty, bool)>,
    need: Qty,
) -> Result<Vec<(String, String, Qty, bool)>, AppError> {
    let (expired, fresh): (Vec<_>, Vec<_>) = lots.into_iter().partition(|l| l.3);
    let lotted = Qty(fresh.iter().chain(&expired).map(|l| l.2.0).sum());
    let unlotted = level(tx, &m.product_id, &m.location_id)?
        .checked_sub(lotted)
        .unwrap_or(Qty::ZERO)
        .max(Qty::ZERO);
    let fresh_total = Qty(fresh.iter().map(|l| l.2.0).sum());
    let policy = expired_policy(tx)?;
    let usable_without_expired = fresh_total.checked_add(unlotted).unwrap_or(fresh_total);
    if policy == ExpiredPolicy::Block && usable_without_expired < need && !expired.is_empty() {
        let product = products::get(tx, &m.product_id)?;
        return Err(AppError::rejected(
            json!({ "products": [{ "productId": product.id, "nameFr": product.name_fr, "nameAr": product.name_ar,
                                    "nameEn": product.name_en, "lots": expired.iter().map(|l| &l.1).collect::<Vec<_>>() }] }),
            "lot_expired",
        ));
    }
    let mut order = fresh;
    if policy == ExpiredPolicy::Warn {
        order.extend(expired);
    }
    Ok(order)
}

/// Lots in stock that are expired or expire within `within_days` of `today` (RPT-026).
pub fn expiry_list(
    conn: &Connection,
    today: &str,
    within_days: i64,
) -> Result<Vec<ExpiryRow>, AppError> {
    let today_day = chrono::NaiveDate::parse_from_str(today, "%Y-%m-%d")
        .map_err(|_| AppError::validation("today", "date_invalid"))?;
    // Checked: a horizon past chrono's calendar is refused instead of panicking.
    let limit = chrono::TimeDelta::try_days(within_days)
        .and_then(|d| today_day.checked_add_signed(d))
        .ok_or_else(|| AppError::validation("withinDays", "value_invalid"))?
        .format("%Y-%m-%d")
        .to_string();
    let mut stmt = conn.prepare_cached(
        "SELECT p.id, p.name_ar, p.name_fr, p.name_en, l.id, l.lot_number, l.expiry_date, ll.location_id, ll.qty
         FROM lot_levels ll JOIN lots l ON l.id = ll.lot_id JOIN products p ON p.id = l.product_id
         WHERE ll.qty > 0 AND l.expiry_date IS NOT NULL AND l.expiry_date <= ?1
         ORDER BY l.expiry_date, p.name_fr",
    )?;
    let rows = stmt
        .query_map([&limit], |r| {
            let expiry: String = r.get(6)?;
            let days_left = chrono::NaiveDate::parse_from_str(&expiry, "%Y-%m-%d")
                .map(|d| (d - today_day).num_days())
                .unwrap_or(0);
            Ok(ExpiryRow {
                product_id: r.get(0)?,
                name_ar: r.get(1)?,
                name_fr: r.get(2)?,
                name_en: r.get(3)?,
                lot_id: r.get(4)?,
                lot_number: r.get(5)?,
                expiry_date: expiry,
                location_id: r.get(7)?,
                qty: r.get(8)?,
                days_left,
                status: if days_left < 0 {
                    ExpiryStatus::Expired
                } else {
                    ExpiryStatus::Soon
                },
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::extras::{self, ProductExtrasInput};
    use crate::lang::Lang;
    use crate::money::Money;
    use crate::payments::{PaymentInput, PaymentMethod};
    use crate::sales::{Cart, CartLine, SaleInput, create_sale};
    use crate::stock::{
        AdjustInput, AdjustReason, DAMAGED_LOCATION_ID, MovementReason, ReceiveLine,
        SHOP_LOCATION_ID, StockMovement, adjust, rebuild_levels, receive,
    };
    use crate::testkit::{NOON, Shop, shop};
    use proptest::prelude::*;

    fn lotted(shop: &Shop) -> String {
        let p = shop.product("Doliprane 500", Money(25_000), &shop.tva9);
        let actor = shop.owner.actor();
        let id = p.id.clone();
        shop.db
            .write(move |tx| {
                extras::set(
                    tx,
                    &actor,
                    &id,
                    ProductExtrasInput {
                        track_lots: true,
                        ..ProductExtrasInput::default()
                    },
                )
            })
            .unwrap();
        p.id
    }

    fn receive_lot(shop: &Shop, product: &str, lot: &str, expiry: &str, qty: i64) {
        let line = ReceiveLine {
            product_id: product.into(),
            location_id: SHOP_LOCATION_ID.into(),
            qty: Qty(qty),
            unit_cost: Money(18_000),
            lot: Some(LotInput {
                lot_number: lot.into(),
                expiry_date: Some(expiry.into()),
            }),
            serials: Vec::new(),
            supplier_id: None,
            ref_type: "document".into(),
            ref_id: "rc".into(),
            note: None,
        };
        let actor = shop.owner.actor();
        shop.db
            .write(move |tx| receive(tx, &actor, &line, 1))
            .unwrap();
    }

    fn sale_of(product: &str, qty: i64) -> NewMovement {
        NewMovement {
            product_id: product.into(),
            location_id: SHOP_LOCATION_ID.into(),
            qty_delta: Qty(-qty),
            reason: MovementReason::Sale,
            reason_code: None,
            ref_type: Some("document".into()),
            ref_id: Some("tk".into()),
            unit_cost: None,
            note: None,
        }
    }

    fn leaving(product: &str, reason: MovementReason, qty: i64) -> NewMovement {
        NewMovement {
            reason,
            ..sale_of(product, qty)
        }
    }

    fn adjust_by(
        shop: &Shop,
        product: &str,
        qty: i64,
        reason: AdjustReason,
    ) -> Result<StockMovement, AppError> {
        let input = AdjustInput {
            product_id: product.into(),
            location_id: None,
            qty_delta: Qty(qty),
            reason,
            unit_cost: None,
            note: None,
        };
        let actor = shop.owner.actor();
        shop.db.write(move |tx| adjust(tx, &actor, input, NOON))
    }

    fn summary(takes: Vec<LotTake>) -> Vec<(String, i64, bool)> {
        takes
            .into_iter()
            .map(|t| (t.lot_number.unwrap_or_default(), t.qty.0, t.expired))
            .collect()
    }

    #[test]
    fn a_lotted_receipt_needs_its_lot() {
        let shop = shop();
        let product = lotted(&shop);
        let line = ReceiveLine {
            product_id: product,
            location_id: SHOP_LOCATION_ID.into(),
            qty: Qty(1_000),
            unit_cost: Money(18_000),
            lot: None,
            serials: Vec::new(),
            supplier_id: None,
            ref_type: "document".into(),
            ref_id: "rc".into(),
            note: None,
        };
        let actor = shop.owner.actor();
        assert_eq!(
            shop.db
                .write(move |tx| receive(tx, &actor, &line, 1))
                .unwrap_err(),
            AppError::validation("lot", "lot_required")
        );
    }

    #[test]
    fn fefo_takes_the_earliest_expiry_and_never_an_expired_lot_when_blocked() {
        let shop = shop();
        let product = lotted(&shop);
        receive_lot(&shop, &product, "L-OLD", "2026-09-01", 5_000);
        receive_lot(&shop, &product, "L-SOON", "2026-11-30", 4_000);
        receive_lot(&shop, &product, "L-LATE", "2027-06-30", 10_000);
        let actor = shop.owner.actor();
        let m = sale_of(&product, 6_000);
        let takes = shop
            .db
            .write(move |tx| take(tx, &actor, &m, "2026-09-28", 2))
            .unwrap();
        assert_eq!(
            takes
                .iter()
                .map(|t| (t.lot_number.clone().unwrap(), t.qty.0, t.expired))
                .collect::<Vec<_>>(),
            vec![
                ("L-SOON".into(), 4_000, false),
                ("L-LATE".into(), 2_000, false)
            ],
            "fresh lots first, the soonest expiry first; the expired lot is left"
        );
        shop.set("stock.expired_lots", serde_json::json!("block"));
        let actor = shop.owner.actor();
        let too_much = sale_of(&product, 9_000);
        let err = shop
            .db
            .write(move |tx| take(tx, &actor, &too_much, "2026-09-28", 3))
            .unwrap_err();
        assert_eq!(err.code(), "lot_expired");
        shop.set("stock.expired_lots", serde_json::json!("warn"));
        let actor = shop.owner.actor();
        let rest = sale_of(&product, 9_000);
        let takes = shop
            .db
            .write(move |tx| take(tx, &actor, &rest, "2026-09-28", 4))
            .unwrap();
        assert_eq!(
            takes.last().map(|t| (t.lot_number.clone(), t.expired)),
            Some((Some("L-OLD".into()), true))
        );
    }

    #[test]
    fn the_expiry_list_shows_expired_and_soon_lots_in_stock() {
        let shop = shop();
        let product = lotted(&shop);
        receive_lot(&shop, &product, "L-OLD", "2026-09-01", 5_000);
        receive_lot(&shop, &product, "L-SOON", "2026-10-15", 4_000);
        receive_lot(&shop, &product, "L-LATE", "2027-06-30", 10_000);
        let rows = shop.db.read(|c| expiry_list(c, "2026-09-28", 30)).unwrap();
        assert_eq!(
            rows.iter()
                .map(|r| (r.lot_number.as_str(), r.status, r.days_left))
                .collect::<Vec<_>>(),
            vec![
                ("L-OLD", ExpiryStatus::Expired, -27),
                ("L-SOON", ExpiryStatus::Soon, 17)
            ]
        );
    }

    #[test]
    fn stock_that_is_not_sold_takes_expired_lots_first_and_is_never_refused() {
        let shop = shop();
        let product = lotted(&shop);
        receive_lot(&shop, &product, "L-OLD", "2026-09-01", 5_000);
        receive_lot(&shop, &product, "L-OLDER", "2026-08-01", 2_000);
        receive_lot(&shop, &product, "L-NEW", "2027-01-31", 1_000);
        shop.set("stock.expired_lots", serde_json::json!("block"));
        let taken = |reason: MovementReason, qty: i64| {
            let actor = shop.owner.actor();
            let m = leaving(&product, reason, qty);
            shop.db
                .write(move |tx| take(tx, &actor, &m, "2026-09-28", 2))
        };
        assert_eq!(
            taken(MovementReason::Sale, 2_000).unwrap_err().code(),
            "lot_expired",
            "the expired-lot rule still refuses a sale"
        );
        // A count that finds expired boxes gone, then a transfer of expired stock: expired lots
        // leave first (FEFO), and neither is refused.
        assert_eq!(
            summary(taken(MovementReason::Count, 3_000).unwrap()),
            vec![
                ("L-OLDER".into(), 2_000, true),
                ("L-OLD".into(), 1_000, true)
            ]
        );
        assert_eq!(
            summary(taken(MovementReason::TransferOut, 4_000).unwrap()),
            vec![("L-OLD".into(), 4_000, true)]
        );
        assert_eq!(
            shop.count("SELECT COALESCE(SUM(qty), 0) FROM lot_levels"),
            1_000,
            "only the fresh lot is left"
        );
    }

    #[test]
    fn an_expiry_write_off_empties_the_expired_lot_and_later_sales_go_on() {
        let shop = shop();
        let product = lotted(&shop);
        receive_lot(&shop, &product, "L-OLD", "2026-09-01", 5_000);
        receive_lot(&shop, &product, "L-LATE", "2027-06-30", 3_000);
        // Stock put in by an adjustment carries no lot: it is stock without a lot.
        adjust_by(&shop, &product, 2_000, AdjustReason::Opening).unwrap();
        assert_eq!(
            shop.count("SELECT COALESCE(SUM(qty), 0) FROM lot_levels"),
            8_000
        );
        let written_off = adjust_by(&shop, &product, -5_000, AdjustReason::Expiry).unwrap();
        assert_eq!(
            (written_off.qty_delta, written_off.reason_code),
            (Qty(-5_000), Some(AdjustReason::Expiry))
        );
        assert_eq!(
            shop.count(
                "SELECT COUNT(*) FROM stock_movements m JOIN lots l ON l.id = m.lot_id
                 WHERE m.reason = 'loss' AND l.lot_number = 'L-OLD'"
            ),
            1,
            "the write-off leaves the expired lot"
        );
        let rows = shop.db.read(|c| expiry_list(c, "2026-09-28", 365)).unwrap();
        assert_eq!(
            rows.iter()
                .map(|r| r.lot_number.as_str())
                .collect::<Vec<_>>(),
            vec!["L-LATE"],
            "the written-off lot is off the expiry list"
        );
        // 3 boxes of L-LATE and 2 without a lot are left: selling 4 is not refused and takes no
        // expired lot.
        shop.set("stock.expired_lots", serde_json::json!("block"));
        shop.open_till(0);
        let ctx = shop.till();
        let input = SaleInput {
            cart: Cart::new(Lang::Fr, vec![CartLine::product(&product, Qty(4_000))]),
            payments: vec![PaymentInput::electronic(
                PaymentMethod::CardCib,
                Money(100_000),
                Some("A1"),
            )],
        };
        shop.db
            .write(move |tx| create_sale(tx, &ctx, input))
            .unwrap();
        assert_eq!(
            shop.count(
                "SELECT COUNT(*) FROM stock_movements m JOIN lots l ON l.id = m.lot_id
                 WHERE m.reason = 'sale' AND l.lot_number = 'L-OLD'"
            ),
            0
        );
        assert_eq!(
            shop.count(
                "SELECT json_extract(payload, '$.expiredLots') FROM audit_log
                 WHERE action = 'sale.created'"
            ),
            0
        );
        assert_eq!(
            shop.count("SELECT COALESCE(SUM(qty), 0) FROM lot_levels"),
            0
        );
        assert_eq!(shop.count("SELECT SUM(qty) FROM stock_levels"), 1_000);
    }

    #[test]
    fn a_write_off_across_lots_appends_one_row_per_lot() {
        let shop = shop();
        let product = lotted(&shop);
        receive_lot(&shop, &product, "L-A", "2027-01-31", 2_000);
        receive_lot(&shop, &product, "L-B", "2027-03-31", 3_000);
        let first = adjust_by(&shop, &product, -4_000, AdjustReason::Damage).unwrap();
        assert_eq!(first.qty_delta, Qty(-2_000), "the first row: all of L-A");
        let id = product.clone();
        let left = shop.db.read(move |c| stock_of(c, &id)).unwrap();
        assert_eq!(
            left.iter()
                .map(|s| (s.lot.lot_number.as_str(), s.qty))
                .collect::<Vec<_>>(),
            vec![("L-B", Qty(1_000))]
        );
        assert_eq!(
            shop.count(
                "SELECT COUNT(*) FROM audit_log WHERE action = 'stock.adjusted'
                   AND json_extract(payload, '$.qtyDelta') = -4000
                   AND json_array_length(payload, '$.movementIds') = 2
                   AND json_array_length(payload, '$.lots') = 2"
            ),
            1
        );
    }

    #[test]
    fn rebuild_levels_repairs_a_drifted_lot_cache() {
        let shop = shop();
        let product = lotted(&shop);
        receive_lot(&shop, &product, "L-A", "2027-01-31", 5_000);
        receive_lot(&shop, &product, "L-B", "2027-03-31", 4_000);
        let actor = shop.owner.actor();
        let m = sale_of(&product, 2_000);
        shop.db
            .write(move |tx| take(tx, &actor, &m, "2026-09-28", 2))
            .unwrap();
        // The cache drifts: a wrong lot quantity, and a lot row that no movement explains.
        shop.db
            .write(|tx| {
                tx.execute(
                    "UPDATE lot_levels SET qty = 999
                     WHERE lot_id = (SELECT id FROM lots WHERE lot_number = 'L-A')",
                    [],
                )?;
                tx.execute(
                    "INSERT INTO lot_levels (lot_id, location_id, qty, updated_at)
                     SELECT id, ?1, 7, 0 FROM lots WHERE lot_number = 'L-B'",
                    [DAMAGED_LOCATION_ID],
                )?;
                Ok(())
            })
            .unwrap();
        let actor = shop.owner.actor();
        let report = shop.db.write(move |tx| rebuild_levels(tx, &actor)).unwrap();
        assert_eq!(
            report.corrected, 2,
            "the wrong lot quantity and the stray row"
        );
        let id = product.clone();
        let restored = shop.db.read(move |c| stock_of(c, &id)).unwrap();
        assert_eq!(
            restored
                .iter()
                .map(|s| (s.lot.lot_number.as_str(), s.location_id.as_str(), s.qty))
                .collect::<Vec<_>>(),
            vec![
                ("L-A", SHOP_LOCATION_ID, Qty(3_000)),
                ("L-B", SHOP_LOCATION_ID, Qty(4_000))
            ]
        );
        assert_eq!(
            shop.count(
                "SELECT COUNT(*) FROM audit_log WHERE action = 'stock.levels_rebuilt'
                   AND json_extract(payload, '$.lotCorrected') = 2
                   AND json_extract(payload, '$.lotLevels') = 2"
            ),
            1
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(24))]
        #[test]
        fn lot_levels_always_add_up_to_the_lotted_movements(
            receipts in proptest::collection::vec(1_i64..50, 1..6),
            sales in proptest::collection::vec(1_i64..40, 0..8)
        ) {
            let shop = shop();
            let product = lotted(&shop);
            for (i, qty) in receipts.iter().enumerate() {
                receive_lot(&shop, &product, &format!("L{i}"), &format!("2027-0{}-01", i + 1), qty * 1_000);
            }
            for qty in sales {
                let actor = shop.owner.actor();
                let m = sale_of(&product, qty * 1_000);
                shop.db.write(move |tx| take(tx, &actor, &m, "2026-09-28", 5)).unwrap();
            }
            let by_lots = shop.count("SELECT COALESCE(SUM(qty), 0) FROM lot_levels");
            let lotted_moves = shop.count("SELECT COALESCE(SUM(qty_delta), 0) FROM stock_movements WHERE lot_id IS NOT NULL");
            prop_assert_eq!(by_lots, lotted_moves);
            let level = shop.count("SELECT SUM(qty) FROM stock_levels");
            let all_moves = shop.count("SELECT SUM(qty_delta) FROM stock_movements");
            prop_assert_eq!(level, all_moves);
            prop_assert!(shop.count("SELECT COUNT(*) FROM lot_levels WHERE qty < 0") == 0, "FEFO never drives a lot below zero");
            let actor = shop.owner.actor();
            let report = shop.db.write(move |tx| rebuild_levels(tx, &actor)).unwrap();
            prop_assert_eq!(report.corrected, 0, "the rebuild agrees with the triggers");
        }
    }
}

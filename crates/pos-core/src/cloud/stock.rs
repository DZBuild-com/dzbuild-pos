//! What DZBuild holds of each published product's stock, as far as this till knows (`cloud_stock`,
//! whole pieces): `confirmed` (acknowledged by DZBuild), `set_in_flight` (a `set` not yet
//! answered), `in_flight` (deltas not yet answered). A plan compares the local on-hand (storage
//! locations, damaged goods excluded) with `expected = (set_in_flight ?? confirmed) + in_flight`
//! and emits the difference, so the online level always converges on the shop's. A refused item
//! requests a reset (the next plan sends a `set`). Moves DZBuild already made — an online order
//! sold at the till, a dashboard edit applied here — are acknowledged, never pushed back.

use crate::catalog::products::{Product, ProductKind};
use crate::error::AppError;
use crate::money::QTY_SCALE;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StockReason {
    PosSale,
    PosReturn,
    Restock,
    Count,
    Loss,
}

impl StockReason {
    /// The Backend Brief's reason for a local movement reason.
    pub fn of_movement(reason: &str) -> StockReason {
        match reason {
            "sale" => StockReason::PosSale,
            "return" => StockReason::PosReturn,
            "purchase" | "opening" | "production" => StockReason::Restock,
            "loss" => StockReason::Loss,
            _ => StockReason::Count,
        }
    }
}

/// One item of `POST /v1/inventory/adjustments/batch`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockItem {
    pub product_external_id: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta: Option<i64>,
    pub reason: StockReason,
    #[serde(rename = "ref")]
    pub reference: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemOutcome {
    Applied,
    Refused,
}

/// The online level of a product in whole pieces, or `None` when it has none online (a weight,
/// an untracked product, a kit, a service).
pub fn online_pieces(conn: &Connection, product: &Product) -> Result<Option<i64>, AppError> {
    if !product.track_stock || product.unit_decimals != 0 || product.kind != ProductKind::Simple {
        return Ok(None);
    }
    let on_hand = crate::stock::on_hand(conn, &product.id)?;
    Ok(Some(on_hand.0.max(0) / QTY_SCALE))
}

/// Starts following a published product (the first plan sends a `set`).
pub fn track(
    tx: &Transaction<'_>,
    store: &str,
    product_id: &str,
    now: i64,
) -> Result<(), AppError> {
    tx.execute(
        "INSERT INTO cloud_stock (store_id, product_id, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT (store_id, product_id) DO NOTHING",
        params![store, product_id, now],
    )?;
    Ok(())
}

/// Products whose level must be planned even without a new movement: never set, or reset.
pub fn needing_plan(conn: &Connection, store: &str) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT product_id FROM cloud_stock WHERE store_id = ?1
           AND (reset_requested = 1 OR (confirmed IS NULL AND set_in_flight IS NULL))",
    )?;
    let rows = stmt
        .query_map([store], |r| r.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    Ok(rows)
}

struct Row {
    confirmed: Option<i64>,
    set_in_flight: Option<i64>,
    in_flight: i64,
    reset_requested: bool,
}

fn row(conn: &Connection, store: &str, product_id: &str) -> Result<Option<Row>, AppError> {
    Ok(conn
        .query_row(
            "SELECT confirmed, set_in_flight, in_flight, reset_requested FROM cloud_stock
             WHERE store_id = ?1 AND product_id = ?2",
            params![store, product_id],
            |r| {
                Ok(Row {
                    confirmed: r.get(0)?,
                    set_in_flight: r.get(1)?,
                    in_flight: r.get(2)?,
                    reset_requested: r.get(3)?,
                })
            },
        )
        .optional()?)
}

/// The item that brings DZBuild to the local level, booked as in flight; `None` when nothing
/// needs to be sent (or the product has no online level).
pub fn plan(
    tx: &Transaction<'_>,
    store: &str,
    product: &Product,
    reason: StockReason,
    reference: &str,
    location_id: Option<&str>,
) -> Result<Option<StockItem>, AppError> {
    let Some(target) = online_pieces(tx, product)? else {
        return Ok(None);
    };
    track(tx, store, &product.id, 0)?;
    let Some(mut r) = row(tx, store, &product.id)? else {
        return Ok(None);
    };
    if r.reset_requested && r.in_flight == 0 && r.set_in_flight.is_none() {
        tx.execute(
            "UPDATE cloud_stock SET confirmed = NULL, reset_requested = 0 WHERE store_id = ?1 AND product_id = ?2",
            params![store, product.id],
        )?;
        r.confirmed = None;
    }
    let item = |set: Option<i64>, delta: Option<i64>| StockItem {
        product_external_id: product.id.clone(),
        target: "product".into(),
        set,
        delta,
        reason,
        reference: reference.to_owned(),
        location_id: location_id.map(str::to_owned),
    };
    match (r.confirmed, r.set_in_flight) {
        (None, None) => {
            tx.execute(
                "UPDATE cloud_stock SET set_in_flight = ?3 WHERE store_id = ?1 AND product_id = ?2",
                params![store, product.id, target],
            )?;
            Ok(Some(item(Some(target), None)))
        }
        (confirmed, set) => {
            let expected = set.or(confirmed).unwrap_or(0) + r.in_flight;
            let delta = target - expected;
            if delta == 0 {
                return Ok(None);
            }
            tx.execute(
                "UPDATE cloud_stock SET in_flight = in_flight + ?3 WHERE store_id = ?1 AND product_id = ?2",
                params![store, product.id, delta],
            )?;
            Ok(Some(item(None, Some(delta))))
        }
    }
}

/// DZBuild answered a batch: applied items move `confirmed`, refused ones release their share and
/// request a reset.
pub fn on_answer(
    tx: &Transaction<'_>,
    store: &str,
    items: &[StockItem],
    outcomes: &[ItemOutcome],
    now: i64,
) -> Result<(), AppError> {
    for (item, outcome) in items.iter().zip(outcomes) {
        let id = &item.product_external_id;
        match (outcome, item.set, item.delta) {
            (ItemOutcome::Applied, Some(v), _) => tx.execute(
                "UPDATE cloud_stock SET confirmed = ?3, set_in_flight = NULL, updated_at = ?4
                 WHERE store_id = ?1 AND product_id = ?2",
                params![store, id, v, now],
            )?,
            (ItemOutcome::Applied, None, Some(d)) => tx.execute(
                "UPDATE cloud_stock SET confirmed = COALESCE(confirmed, set_in_flight, 0) + ?3,
                     in_flight = in_flight - ?3, updated_at = ?4 WHERE store_id = ?1 AND product_id = ?2",
                params![store, id, d, now],
            )?,
            (ItemOutcome::Refused, Some(_), _) => tx.execute(
                "UPDATE cloud_stock SET set_in_flight = NULL, reset_requested = 1, updated_at = ?3
                 WHERE store_id = ?1 AND product_id = ?2",
                params![store, id, now],
            )?,
            (ItemOutcome::Refused, None, Some(d)) => tx.execute(
                "UPDATE cloud_stock SET in_flight = in_flight - ?3, reset_requested = 1, updated_at = ?4
                 WHERE store_id = ?1 AND product_id = ?2",
                params![store, id, d, now],
            )?,
            _ => 0,
        };
    }
    Ok(())
}

/// The whole batch was refused (a 4xx): every item releases its share.
pub fn release(
    tx: &Transaction<'_>,
    store: &str,
    items: &[StockItem],
    now: i64,
) -> Result<(), AppError> {
    let refused = vec![ItemOutcome::Refused; items.len()];
    on_answer(tx, store, items, &refused, now)
}

/// DZBuild already moved its stock by `pieces` (an online order this till sold or shipped, a
/// dashboard edit applied here): our knowledge follows, nothing is pushed.
pub fn acknowledge(
    tx: &Transaction<'_>,
    store: &str,
    product_id: &str,
    pieces: i64,
    now: i64,
) -> Result<(), AppError> {
    tx.execute(
        "UPDATE cloud_stock SET confirmed = confirmed + ?3, updated_at = ?4
         WHERE store_id = ?1 AND product_id = ?2 AND confirmed IS NOT NULL",
        params![store, product_id, pieces, now],
    )?;
    tx.execute(
        "UPDATE cloud_stock SET set_in_flight = set_in_flight + ?3, updated_at = ?4
         WHERE store_id = ?1 AND product_id = ?2 AND set_in_flight IS NOT NULL",
        params![store, product_id, pieces, now],
    )?;
    Ok(())
}

/// The owner's "Recaler le stock en ligne" (or a double deduction detected): the next plan of
/// each product sends a `set` of the local level.
pub fn request_reset(
    tx: &Transaction<'_>,
    store: &str,
    product_ids: Option<&[String]>,
    now: i64,
) -> Result<usize, AppError> {
    match product_ids {
        None => Ok(tx.execute(
            "UPDATE cloud_stock SET reset_requested = 1, updated_at = ?2 WHERE store_id = ?1",
            params![store, now],
        )?),
        Some(ids) => {
            let mut n = 0;
            for id in ids {
                n += tx.execute(
                    "UPDATE cloud_stock SET reset_requested = 1, updated_at = ?3 WHERE store_id = ?1 AND product_id = ?2",
                    params![store, id, now],
                )?;
            }
            Ok(n)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::testkit::{linked_shop, publish};
    use crate::money::Money;
    use crate::testkit::NOON;
    use proptest::prelude::*;

    #[derive(Debug, Clone)]
    enum Op {
        Move(i64),
        Plan,
        Answer(bool),
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![
            (-5_i64..=5)
                .prop_filter("a move", |n| *n != 0)
                .prop_map(Op::Move),
            Just(Op::Plan),
            any::<bool>().prop_map(Op::Answer),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]
        #[test]
        fn stock_pushes_converge_on_the_local_level(ops in proptest::collection::vec(op(), 1..40)) {
            let (shop, link) = linked_shop();
            let milk = shop.product("Lait", Money(10_000), &shop.tva19);
            publish(&shop, &milk.id);
            shop.stock_in(&milk.id, 50_000);
            let store = link.store_id.clone();
            let mut remote: Option<i64> = None; // what DZBuild holds
            let mut in_flight: Vec<Vec<StockItem>> = Vec::new();
            let mut step = 0;
            let apply = |ok: bool, items: &[StockItem], remote: &mut Option<i64>| {
                if ok {
                    for item in items {
                        match (item.set, item.delta) {
                            (Some(v), _) => *remote = Some(v),
                            (None, Some(d)) => *remote = Some(remote.unwrap_or(0) + d),
                            _ => {}
                        }
                    }
                }
                let outcomes = vec![if ok { ItemOutcome::Applied } else { ItemOutcome::Refused }; items.len()];
                let (store, items) = (store.clone(), items.to_vec());
                shop.db.write(move |tx| on_answer(tx, &store, &items, &outcomes, NOON)).unwrap();
            };
            for op in ops {
                match op {
                    Op::Move(n) => {
                        let input = crate::stock::AdjustInput {
                            product_id: milk.id.clone(),
                            location_id: None,
                            qty_delta: crate::money::Qty(n * 1_000),
                            reason: crate::stock::AdjustReason::Correction,
                            unit_cost: None,
                            note: None,
                        };
                        let actor = shop.owner.actor();
                        shop.db.write(move |tx| crate::stock::adjust(tx, &actor, input, NOON)).unwrap();
                    }
                    Op::Plan => {
                        step += 1;
                        let (store, product) = (store.clone(), milk.clone());
                        let r = format!("b{step}:0");
                        if let Some(item) = shop.db.write(move |tx| plan(tx, &store, &product, StockReason::Count, &r, None)).unwrap() {
                            in_flight.push(vec![item]);
                        }
                    }
                    Op::Answer(ok) => {
                        if !in_flight.is_empty() {
                            let items = in_flight.remove(0);
                            apply(ok, &items, &mut remote);
                        }
                    }
                }
            }
            // then the network is good: answer everything, plan until nothing is left to send
            for items in std::mem::take(&mut in_flight) {
                apply(true, &items, &mut remote);
            }
            for round in 0..4 {
                let (store, product) = (store.clone(), milk.clone());
                let r = format!("final{round}:0");
                match shop.db.write(move |tx| plan(tx, &store, &product, StockReason::Count, &r, None)).unwrap() {
                    Some(item) => apply(true, &[item], &mut remote),
                    None => break,
                }
            }
            let id = milk.id.clone();
            let local = shop.db.read(move |c| crate::stock::on_hand(c, &id)).unwrap().0.max(0) / 1_000;
            prop_assert_eq!(remote, Some(local));
        }
    }
}

//! A traced carcass (Lots module, `track_lots`): the cutting run takes it out of its lots through
//! `stock::lots::take`, so `lot_levels` drop with `stock_levels` and no lot keeps meat that was
//! already cut up.

use super::yields::{self, CostMethod, RunCut, RunInput};
use crate::auth::Permission;
use crate::catalog::UNIT_KG_ID;
use crate::catalog::extras::{self, ProductExtrasInput};
use crate::catalog::products::ProductInput;
use crate::money::{Money, Qty};
use crate::packs::{self, Module};
use crate::stock::lots::LotInput;
use crate::stock::{ReceiveLine, SHOP_LOCATION_ID, receive};
use crate::testkit::{NOON, Shop, shop};

fn by_kg(shop: &Shop, name: &str, price: i64, cost: i64) -> String {
    let mut input = ProductInput::simple(name, UNIT_KG_ID, &shop.tva19, Money(price));
    input.is_weighed = true;
    input.cost_price = Money(cost);
    shop.product_with(input).id
}

/// A 20 kg lamb carcass at 1 500 DA/kg received into lot `A-0928`, and its lot id.
fn traced_carcass(shop: &Shop) -> (String, String) {
    let carcass = by_kg(shop, "Carcasse d'agneau", 0, 150_000);
    let (actor, pid) = (shop.owner.actor(), carcass.clone());
    let lot_id = shop
        .db
        .write(move |tx| {
            packs::set_modules(tx, &actor, &[Module::Yield, Module::Waste, Module::Lots])?;
            extras::set(
                tx,
                &actor,
                &pid,
                ProductExtrasInput {
                    track_lots: true,
                    ..ProductExtrasInput::default()
                },
            )?;
            let line = ReceiveLine {
                product_id: pid.clone(),
                location_id: SHOP_LOCATION_ID.into(),
                qty: Qty(20_000),
                unit_cost: Money(150_000),
                lot: Some(LotInput {
                    lot_number: "A-0928".into(),
                    expiry_date: Some("2026-10-05".into()),
                }),
                serials: Vec::new(),
                supplier_id: None,
                ref_type: "document".into(),
                ref_id: "rc".into(),
                note: None,
            };
            Ok(receive(tx, &actor, &line, NOON)?.lot_id)
        })
        .unwrap()
        .unwrap();
    (carcass, lot_id)
}

#[test]
fn a_traced_carcass_leaves_its_lot_when_it_is_cut_up() {
    let shop = shop();
    let (carcass, lot_id) = traced_carcass(&shop);
    let gigot = by_kg(&shop, "Gigot", 280_000, 0);
    let cotes = by_kg(&shop, "Côtelettes", 260_000, 0);
    let ctx = shop.approved_till(Permission::StockAdjust);
    let input = RunInput {
        template_id: None,
        source_product_id: carcass.clone(),
        location_id: None,
        input_qty: Qty(20_000),
        input_cost: None,
        method: CostMethod::Value,
        cuts: vec![
            RunCut {
                product_id: gigot,
                qty: Qty(9_000),
            },
            RunCut {
                product_id: cotes,
                qty: Qty(8_000),
            },
        ],
        note: None,
    };
    let run = shop
        .db
        .write(move |tx| yields::run(tx, &ctx, input, crate::lang::Lang::Fr))
        .unwrap();
    assert_eq!(run.loss_qty, Qty(3_000));

    let lot = lot_id.clone();
    let lot_qty: i64 = shop
        .db
        .read(move |c| {
            Ok(c.query_row(
                "SELECT qty FROM lot_levels WHERE lot_id = ?1 AND location_id = ?2",
                rusqlite::params![lot, SHOP_LOCATION_ID],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(lot_qty, 0, "the cut-up carcass is no longer in its lot");
    let c = carcass.clone();
    assert!(
        shop.db
            .read(move |conn| crate::stock::lots::stock_of(conn, &c))
            .unwrap()
            .is_empty()
    );
    let c = carcass.clone();
    assert_eq!(
        shop.db
            .read(move |conn| crate::stock::on_hand(conn, &c))
            .unwrap(),
        Qty::ZERO
    );

    let (c, r) = (carcass.clone(), run.id.clone());
    let out_rows: Vec<(Option<String>, i64)> = shop
        .db
        .read(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT lot_id, qty_delta FROM stock_movements
                 WHERE reason = 'production' AND product_id = ?1 AND ref_type = 'yield_run' AND ref_id = ?2",
            )?;
            Ok(stmt
                .query_map(rusqlite::params![c, r], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()?)
        })
        .unwrap();
    assert_eq!(
        out_rows,
        vec![(Some(lot_id), -20_000)],
        "the carcass leaves through its lot, in one production row"
    );
}

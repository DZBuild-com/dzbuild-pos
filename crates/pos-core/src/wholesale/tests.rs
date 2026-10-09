//! Wholesale and distribution (Task 11).

use super::rounds::{self, RoundInput, StopReport, StopStatus};
use super::{self as ws, PackInput};
use crate::billing::drafts::{DraftInput, save_draft};
use crate::billing::validate::{ValidateInput, validate};
use crate::catalog::UNIT_CARTON_ID;
use crate::documents::{DocType, Document};
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::packs::{self, Module};
use crate::payments::{PaymentInput, PaymentMethod};
use crate::sales::{Cart, CartLine, SaleExtras, SaleInput, create_sale_with, price_cart};
use crate::stock::transfers::TransferLine;
use crate::testkit::{Shop, shop};
use serde_json::json;

fn wholesaler() -> Shop {
    let shop = shop();
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| {
            packs::set_modules(tx, &actor, &[Module::Wholesale, Module::DeliveryRounds])
        })
        .unwrap();
    shop.open_till(1_000_000);
    shop
}

/// The wholesaler's shop, for the delivery rounds report test.
pub(crate) fn wholesaler_for_reports() -> Shop {
    wholesaler()
}

fn on_account(amount: Money) -> Vec<PaymentInput> {
    vec![PaymentInput {
        method: PaymentMethod::Credit,
        amount,
        reference: None,
        cheque_bank: None,
        cheque_date: None,
    }]
}

/// Cash for `amount` of goods with the droit de timbre the customer is charged on it.
fn cash_with_duty(shop: &Shop, amount: Money) -> Vec<PaymentInput> {
    let duty = crate::tax::timbre::for_cash_amount(&shop.pack, amount);
    vec![PaymentInput::cash(amount.checked_add(duty).unwrap())]
}

fn validated(shop: &Shop, doc_type: DocType, customer: &str, product: &str, qty: i64) -> Document {
    let ctx = shop.owner_till();
    let mut cart = Cart::new(Lang::Fr, vec![CartLine::product(product, Qty(qty))]);
    cart.customer_id = Some(customer.into());
    shop.db
        .write(move |tx| {
            let draft = save_draft(
                tx,
                &ctx,
                None,
                DraftInput {
                    doc_type,
                    cart,
                    valid_until: None,
                    due_date: None,
                    second_lang: None,
                    draft_id: None,
                },
            )?;
            // M3: a delivery note takes no payment; an invoice here is sold on account.
            let payments = match doc_type {
                DocType::Invoice => on_account(draft.total_ttc),
                _ => Vec::new(),
            };
            Ok(validate(
                tx,
                &ctx,
                ValidateInput {
                    id: draft.id,
                    payments,
                },
            )?
            .document)
        })
        .unwrap()
}

/// A validated delivery note (M3: no payment and no debt until it is invoiced).
pub(crate) fn bl(shop: &Shop, customer: &str, product: &str, qty: i64) -> Document {
    validated(shop, DocType::DeliveryNote, customer, product, qty)
}

/// A validated invoice sold on the customer's account.
pub(crate) fn invoice_on_account(shop: &Shop, customer: &str, product: &str, qty: i64) -> Document {
    validated(shop, DocType::Invoice, customer, product, qty)
}

#[test]
fn an_agreed_customer_price_replaces_the_level_price_and_takes_no_automatic_discount() {
    let shop = wholesaler();
    let water = shop
        .product("Eau minérale 1,5 L", Money(4_000), &shop.tva19)
        .id;
    let cafe = shop.customer("Café du Port").id;
    let actor = shop.owner.actor();
    let id = cafe.clone();
    shop.db
        .write(move |tx| {
            Ok(tx.execute(
                "UPDATE customers SET discount_bp = 500 WHERE id = ?1",
                [&id],
            )?)
        })
        .unwrap();
    let (c, w) = (cafe.clone(), water.clone());
    shop.db
        .write(move |tx| ws::set_price(tx, &actor, &c, &w, Some(Money(3_500))))
        .unwrap();
    let ctx = shop.till();
    let mut cart = Cart::new(Lang::Fr, vec![CartLine::product(&water, Qty(6_000))]);
    cart.customer_id = Some(cafe.clone());
    let priced = shop.db.read(move |c| price_cart(c, &ctx, &cart)).unwrap();
    let line = &priced.lines[0];
    assert_eq!(
        (
            line.unit_price,
            line.customer_price,
            line.auto_discount.is_none()
        ),
        (Money(3_500), true, true)
    );
    let ctx = shop.till();
    let walk_in = Cart::new(Lang::Fr, vec![CartLine::product(&water, Qty(6_000))]);
    let priced = shop
        .db
        .read(move |c| price_cart(c, &ctx, &walk_in))
        .unwrap();
    assert_eq!(
        (priced.lines[0].unit_price, priced.lines[0].customer_price),
        (Money(4_000), false)
    );
    let actor = shop.owner.actor();
    let (c, w) = (cafe.clone(), water.clone());
    assert_eq!(
        shop.db
            .write(move |tx| ws::set_price(tx, &actor, &c, &w, Some(Money(-1))))
            .unwrap_err()
            .code(),
        "customer_price_invalid"
    );
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| packs::set_modules(tx, &actor, &[]))
        .unwrap();
    let ctx = shop.till();
    let mut cart = Cart::new(Lang::Fr, vec![CartLine::product(&water, Qty(6_000))]);
    cart.customer_id = Some(cafe);
    let priced = shop.db.read(move |c| price_cart(c, &ctx, &cart)).unwrap();
    assert!(
        !priced.lines[0].customer_price,
        "the module off, the agreed prices sleep"
    );
}

#[test]
fn a_line_below_the_minimum_order_is_flagged_or_refused_as_set() {
    let shop = wholesaler();
    let sugar = shop.product("Sucre 1 kg", Money(9_000), &shop.tva9).id;
    let actor = shop.owner.actor();
    let s = sugar.clone();
    shop.db
        .write(move |tx| ws::set_min_order(tx, &actor, &s, Some(Qty(10_000))))
        .unwrap();
    let ctx = shop.till();
    let cart = Cart::new(Lang::Fr, vec![CartLine::product(&sugar, Qty(4_000))]);
    let priced = shop.db.read(move |c| price_cart(c, &ctx, &cart)).unwrap();
    assert_eq!(priced.lines[0].min_order, Some(Qty(10_000)));
    shop.set("wholesale.min_order_policy", json!("block"));
    let ctx = shop.till();
    let cart = Cart::new(Lang::Fr, vec![CartLine::product(&sugar, Qty(4_000))]);
    let err = shop
        .db
        .read(move |c| price_cart(c, &ctx, &cart))
        .unwrap_err();
    assert_eq!(
        (err.field(), err.code().as_str()),
        (Some("lines.0.qty"), "below_min_order")
    );
    let ctx = shop.till();
    let cart = Cart::new(Lang::Fr, vec![CartLine::product(&sugar, Qty(10_000))]);
    assert_eq!(
        shop.db
            .read(move |c| price_cart(c, &ctx, &cart))
            .unwrap()
            .lines[0]
            .min_order,
        None
    );
}

#[test]
fn a_pack_product_sells_its_units_from_the_unit_stock() {
    let shop = wholesaler();
    let water = shop
        .product("Eau minérale 1,5 L", Money(4_000), &shop.tva19)
        .id;
    shop.stock_in(&water, 60_000);
    let actor = shop.owner.actor();
    let input = PackInput {
        unit_product_id: water.clone(),
        units_per_pack: Qty(6_000),
        unit_id: UNIT_CARTON_ID.into(),
        name_ar: String::new(),
        name_fr: "Eau minérale 1,5 L — pack de 6".into(),
        name_en: String::new(),
        price: Money(22_000),
        barcode: Some("6130234000112".into()),
    };
    let pack = shop
        .db
        .write(move |tx| ws::create_pack(tx, &actor, input))
        .unwrap();
    let ctx = shop.till();
    let sale = SaleInput {
        cart: Cart::new(Lang::Fr, vec![CartLine::product(&pack.id, Qty(2_000))]),
        payments: cash_with_duty(&shop, Money(44_000)),
    };
    shop.db
        .write(move |tx| create_sale_with(tx, &ctx, sale, &SaleExtras::default()))
        .unwrap();
    let w = water.clone();
    assert_eq!(
        shop.db.read(move |c| crate::stock::on_hand(c, &w)).unwrap(),
        Qty(48_000)
    );
    let actor = shop.owner.actor();
    let bad = PackInput {
        units_per_pack: Qty(500),
        ..PackInput::sample(&water)
    };
    assert_eq!(
        shop.db
            .write(move |tx| ws::create_pack(tx, &actor, bad))
            .unwrap_err()
            .code(),
        "pack_units_invalid"
    );
}

#[test]
fn a_closed_round_books_every_collection_on_the_ledger_and_its_cash_in_the_drawer() {
    let shop = wholesaler();
    let flour = shop.product("Farine 25 kg", Money(250_000), &shop.tva9).id;
    shop.stock_in(&flour, 100_000);
    shop.complete_company();
    let (a, b) = (
        shop.b2b_customer("Épicerie Amine").id,
        shop.b2b_customer("Supérette Nour").id,
    );
    // Stop 1 an invoice on account, stop 2 a delivery note not invoiced yet.
    let bl_a = invoice_on_account(&shop, &a, &flour, 2_000); // 2 sacks: 5 000 DA owed
    let bl_b = bl(&shop, &b, &flour, 4_000); // 4 sacks: 10 000 DA to collect on delivery
    let ctx = shop.owner_till();
    let round = shop
        .db
        .write(move |tx| {
            rounds::create(
                tx,
                &ctx,
                RoundInput {
                    business_date: None,
                    driver_name: "Rachid".into(),
                    driver_user_id: None,
                    vehicle_plate: Some("01234-116-16".into()),
                    van_location_id: None,
                    note: None,
                },
            )
        })
        .unwrap();
    assert_eq!(round.number, "TR2026-000001");
    let (ctx, id, docs) = (
        shop.owner_till(),
        round.id.clone(),
        vec![bl_a.id.clone(), bl_b.id.clone()],
    );
    let planned = shop
        .db
        .write(move |tx| rounds::set_stops(tx, &ctx, &id, docs))
        .unwrap();
    assert_eq!(planned.total_due, Money(1_500_000));
    let (ctx, id) = (shop.owner_till(), round.id.clone());
    shop.db
        .write(move |tx| rounds::start(tx, &ctx, &id))
        .unwrap();
    let report = |seq: u32, status: StopStatus, collected: i64, method: Option<PaymentMethod>| {
        let (ctx, id) = (shop.owner_till(), round.id.clone());
        shop.db.write(move |tx| {
            rounds::report_stop(
                tx,
                &ctx,
                &id,
                StopReport {
                    seq,
                    status,
                    collected: Money(collected),
                    method,
                    reference: method
                        .filter(|m| *m == PaymentMethod::Cheque)
                        .map(|_| "0012345".into()),
                    cheque_bank: method
                        .filter(|m| *m == PaymentMethod::Cheque)
                        .map(|_| "BNA".into()),
                    cheque_date: method
                        .filter(|m| *m == PaymentMethod::Cheque)
                        .map(|_| "2026-09-28".into()),
                    note: None,
                },
            )
        })
    };
    assert_eq!(
        report(
            2,
            StopStatus::Delivered,
            1_000_001,
            Some(PaymentMethod::Cash)
        )
        .unwrap_err()
        .code(),
        "collection_above_due"
    );
    report(1, StopStatus::Delivered, 500_000, Some(PaymentMethod::Cash)).unwrap();
    report(
        2,
        StopStatus::Delivered,
        300_000,
        Some(PaymentMethod::Cheque),
    )
    .unwrap();
    let cash_before = shop.count("SELECT COALESCE(SUM(amount), 0) FROM cash_movements");
    let (ctx, id) = (shop.owner_till(), round.id.clone());
    let closed = shop
        .db
        .write(move |tx| rounds::close(tx, &ctx, &id))
        .unwrap();
    assert_eq!(closed.versements.len(), 2);
    let cash = &closed.versements[0];
    assert_eq!(
        (cash.quote.stamp_duty, cash.quote.to_receive),
        (Money(5_000), Money(505_000)),
        "5 000 DA in cash: 50 tranches at 1 DA"
    );
    assert_eq!(
        closed.versements[1].quote.stamp_duty,
        Money::ZERO,
        "a cheque pays no duty"
    );
    assert_eq!(
        cash.allocations[0].debit_number.as_deref(),
        bl_a.number.as_deref()
    );
    assert_eq!(
        shop.count("SELECT COALESCE(SUM(amount), 0) FROM cash_movements") - cash_before,
        505_000,
        "the cash is in the drawer"
    );
    let balance = |id: &str| {
        let id = id.to_owned();
        shop.db
            .read(move |c| crate::customers::get(c, &id))
            .unwrap()
            .balance
    };
    // B's cheque waits on the account as an advance until the delivery note is invoiced.
    assert_eq!((balance(&a), balance(&b)), (Money::ZERO, Money(-300_000)));
    let (ctx, bl_id) = (shop.owner_till(), bl_b.id.clone());
    shop.db
        .write(move |tx| {
            crate::billing::convert::invoice_deliveries(
                tx,
                &ctx,
                crate::billing::convert::DeliveriesInput {
                    delivery_ids: vec![bl_id],
                    payments: on_account(Money(1_000_000)),
                    due_date: None,
                    second_lang: None,
                    note: None,
                },
            )
        })
        .unwrap();
    assert_eq!(balance(&b), Money(700_000), "the invoice takes the advance");
    assert_eq!(closed.round.total_collected, Money(800_000));
    assert_eq!(closed.round.cash_collected, Money(500_000));
    assert!(
        shop.db
            .write(|tx| Ok(tx.execute_batch("UPDATE delivery_round_stops SET collected = 0")?))
            .is_err(),
        "a closed round is final"
    );
    let (ctx, id) = (shop.owner_till(), round.id.clone());
    assert_eq!(
        shop.db
            .write(move |tx| rounds::close(tx, &ctx, &id))
            .unwrap_err()
            .code(),
        "round_status_invalid"
    );
}

#[test]
fn a_document_rides_one_open_round_at_a_time() {
    let shop = wholesaler();
    let flour = shop.product("Farine 25 kg", Money(250_000), &shop.tva9).id;
    shop.stock_in(&flour, 10_000);
    let a = shop.customer("Épicerie Amine").id;
    let doc = bl(&shop, &a, &flour, 1_000);
    let make = || {
        let ctx = shop.owner_till();
        shop.db
            .write(move |tx| {
                rounds::create(
                    tx,
                    &ctx,
                    RoundInput {
                        driver_name: "Rachid".into(),
                        ..RoundInput::default()
                    },
                )
            })
            .unwrap()
    };
    let (r1, r2) = (make(), make());
    let (ctx, id, docs) = (shop.owner_till(), r1.id.clone(), vec![doc.id.clone()]);
    shop.db
        .write(move |tx| rounds::set_stops(tx, &ctx, &id, docs))
        .unwrap();
    let (ctx, id, docs) = (shop.owner_till(), r2.id.clone(), vec![doc.id.clone()]);
    assert_eq!(
        shop.db
            .write(move |tx| rounds::set_stops(tx, &ctx, &id, docs))
            .unwrap_err()
            .code(),
        "round_stop_invalid"
    );
}

#[test]
fn a_van_sale_takes_its_stock_from_the_van() {
    let shop = wholesaler();
    let soda = shop.product("Hamoud 1 L", Money(12_000), &shop.tva19).id;
    shop.stock_in(&soda, 100_000);
    let actor = shop.owner.actor();
    let van = shop
        .db
        .write(move |tx| rounds::create_van(tx, &actor, "VAN1", ("شاحنة 1", "Camion 1", "Van 1")))
        .unwrap();
    let ctx = shop.owner_till();
    let v = van.id.clone();
    let round = shop
        .db
        .write(move |tx| {
            rounds::create(
                tx,
                &ctx,
                RoundInput {
                    driver_name: "Rachid".into(),
                    van_location_id: Some(v),
                    ..RoundInput::default()
                },
            )
        })
        .unwrap();
    let (ctx, id, s) = (shop.owner_till(), round.id.clone(), soda.clone());
    shop.db
        .write(move |tx| {
            rounds::load(
                tx,
                &ctx,
                &id,
                vec![TransferLine {
                    product_id: s,
                    qty: Qty(30_000),
                    lot_number: None,
                    serials: Vec::new(),
                }],
            )
        })
        .unwrap();
    let (ctx, id) = (shop.owner_till(), round.id.clone());
    shop.db
        .write(move |tx| rounds::start(tx, &ctx, &id))
        .unwrap();
    let ctx = shop.till();
    let extras = SaleExtras {
        location_id: Some(van.id.clone()),
        ..SaleExtras::default()
    };
    let sale = SaleInput {
        cart: Cart::new(Lang::Fr, vec![CartLine::product(&soda, Qty(12_000))]),
        payments: cash_with_duty(&shop, Money(144_000)),
    };
    shop.db
        .write(move |tx| create_sale_with(tx, &ctx, sale, &extras))
        .unwrap();
    let level = |loc: String| {
        let s = soda.clone();
        shop.db
            .read(move |c| crate::stock::level(c, &s, &loc))
            .unwrap()
    };
    let shop_loc = shop.db.read(crate::stock::default_location).unwrap().id;
    assert_eq!(
        (level(van.id.clone()), level(shop_loc.clone())),
        (Qty(18_000), Qty(70_000))
    );
    let (ctx, id) = (shop.owner_till(), round.id.clone());
    shop.db
        .write(move |tx| rounds::close(tx, &ctx, &id))
        .unwrap();
    let (ctx, id) = (shop.owner_till(), round.id.clone());
    shop.db
        .write(move |tx| rounds::unload(tx, &ctx, &id))
        .unwrap();
    assert_eq!(
        (level(van.id.clone()), level(shop_loc)),
        (Qty::ZERO, Qty(88_000))
    );
    let ctx = shop.till();
    let damaged = shop.db.read(crate::stock::damaged_location).unwrap().id;
    let extras = SaleExtras {
        location_id: Some(damaged),
        ..SaleExtras::default()
    };
    let sale = SaleInput {
        cart: Cart::new(Lang::Fr, vec![CartLine::product(&soda, Qty(1_000))]),
        payments: vec![PaymentInput::cash(Money(12_000))],
    };
    assert_eq!(
        shop.db
            .write(move |tx| create_sale_with(tx, &ctx, sale, &extras))
            .unwrap_err()
            .code(),
        "van_location_invalid"
    );
}

#[test]
fn agreed_prices_are_listed_and_removed_the_bl_mode_needs_its_decision_and_rounds_list() {
    let shop = wholesaler();
    let water = shop
        .product("Eau minérale 1,5 L", Money(4_000), &shop.tva19)
        .id;
    let cafe = shop.customer("Café du Port").id;
    let price = |p: Option<Money>| {
        let (actor, c, w) = (shop.owner.actor(), cafe.clone(), water.clone());
        shop.db
            .write(move |tx| ws::set_price(tx, &actor, &c, &w, p))
            .unwrap()
    };
    let agreed = price(Some(Money(3_500))).unwrap();
    assert_eq!(
        (agreed.price, agreed.list_price),
        (Money(3_500), Money(4_000))
    );
    assert_eq!(price(None), None);
    let c = cafe.clone();
    assert!(
        shop.db
            .read(move |conn| ws::prices_of(conn, &c, Lang::Ar))
            .unwrap()
            .is_empty()
    );

    let authorisation = || shop.db.read(ws::bl_invoice_authorisation).unwrap();
    assert_eq!(authorisation(), None, "off by default");
    shop.set(
        "documents.bl_as_invoice",
        json!({ "number": " 117/2026 ", "date": "2026-01-15" }),
    );
    assert_eq!(
        authorisation(),
        Some(ws::BlInvoiceAuthorisation {
            number: "117/2026".into(),
            date: "2026-01-15".into()
        })
    );
    for bad in [
        json!({ "number": "117/2026" }),
        json!({ "number": "", "date": "2026-01-15" }),
        json!({ "number": "117/2026", "date": "2026-02-30" }),
        json!({ "number": "117/2026", "date": "2026-01-15", "extra": 1 }),
    ] {
        let actor = shop.owner.actor();
        assert!(
            shop.db
                .write(move |tx| crate::settings::set_value(
                    tx,
                    &actor,
                    "documents.bl_as_invoice",
                    bad
                ))
                .is_err()
        );
    }

    let ctx = shop.owner_till();
    let round = shop
        .db
        .write(move |tx| {
            rounds::create(
                tx,
                &ctx,
                RoundInput {
                    driver_name: "Rachid".into(),
                    ..RoundInput::default()
                },
            )
        })
        .unwrap();
    let filter = rounds::RoundFilter {
        status: Some(rounds::RoundStatus::Planned),
        from: "2026-09-01".into(),
        to: "2026-09-30".into(),
    };
    let listed = shop.db.read(move |c| rounds::list(c, &filter)).unwrap();
    assert_eq!(
        listed.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        [round.id.as_str()]
    );
    let ctx = shop.owner_till();
    assert_eq!(
        shop.db
            .write(move |tx| rounds::create(tx, &ctx, RoundInput::default()))
            .unwrap_err()
            .code(),
        "required",
        "a round needs its driver"
    );
}

#[test]
fn every_round_write_is_refused_while_delivery_rounds_is_off() {
    let shop = wholesaler();
    let flour = shop.product("Farine 25 kg", Money(250_000), &shop.tva9).id;
    shop.stock_in(&flour, 100_000);
    shop.complete_company();
    let amine = shop.b2b_customer("Épicerie Amine").id;
    let note = bl(&shop, &amine, &flour, 2_000);
    let ctx = shop.owner_till();
    let round = shop
        .db
        .write(move |tx| {
            rounds::create(
                tx,
                &ctx,
                RoundInput {
                    driver_name: "Rachid".into(),
                    ..RoundInput::default()
                },
            )
        })
        .unwrap();
    let switch = |modules: &'static [Module]| {
        let actor = shop.owner.actor();
        shop.db
            .write(move |tx| packs::set_modules(tx, &actor, modules))
            .unwrap();
    };
    let refused = |err: crate::error::AppError, write: &str| {
        assert_eq!(err.code(), "module_disabled", "{write}");
        assert_eq!(
            serde_json::to_value(&err).unwrap()["details"],
            json!({ "module": "delivery_rounds" }),
            "{write}"
        );
    };

    switch(&[Module::Wholesale]);
    let (ctx, id, docs) = (shop.owner_till(), round.id.clone(), vec![note.id.clone()]);
    refused(
        shop.db
            .write(move |tx| rounds::set_stops(tx, &ctx, &id, docs))
            .unwrap_err(),
        "set_stops",
    );
    let (ctx, id) = (shop.owner_till(), round.id.clone());
    refused(
        shop.db
            .write(move |tx| rounds::start(tx, &ctx, &id))
            .unwrap_err(),
        "start",
    );

    switch(&[Module::Wholesale, Module::DeliveryRounds]);
    let (ctx, id, docs) = (shop.owner_till(), round.id.clone(), vec![note.id.clone()]);
    shop.db
        .write(move |tx| rounds::set_stops(tx, &ctx, &id, docs))
        .unwrap();
    let (ctx, id) = (shop.owner_till(), round.id.clone());
    shop.db
        .write(move |tx| rounds::start(tx, &ctx, &id))
        .unwrap();

    switch(&[Module::Wholesale]);
    let (ctx, id) = (shop.owner_till(), round.id.clone());
    refused(
        shop.db
            .write(move |tx| {
                rounds::report_stop(
                    tx,
                    &ctx,
                    &id,
                    StopReport {
                        seq: 1,
                        status: StopStatus::Delivered,
                        collected: Money(100_000),
                        method: Some(PaymentMethod::Cash),
                        reference: None,
                        cheque_bank: None,
                        cheque_date: None,
                        note: None,
                    },
                )
            })
            .unwrap_err(),
        "report_stop",
    );
    let (ctx, id) = (shop.owner_till(), round.id.clone());
    refused(
        shop.db
            .write(move |tx| rounds::close(tx, &ctx, &id))
            .unwrap_err(),
        "close",
    );
    let id = round.id.clone();
    let still = shop.db.read(move |c| rounds::get(c, &id)).unwrap();
    assert_eq!(
        (still.status, still.stops[0].collected),
        (rounds::RoundStatus::Out, Money(0)),
        "nothing was written while the module was off"
    );
}

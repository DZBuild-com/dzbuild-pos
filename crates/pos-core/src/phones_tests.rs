//! Phones and kiosks (Task 8): serials at the sale and on returns, the warranted item's buyer,
//! flexy top-ups and their float.

use crate::catalog::extras::{self, ProductExtrasInput};
use crate::flexy::{self, FloatPurchase};
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::packs::{self, Module};
use crate::payments::PaymentInput;
use crate::returns::{ReturnInput, ReturnLineInput, ReturnReason, create_return_with};
use crate::sales::extensions::{ReturnExtras, ReturnSerial, SaleSerial};
use crate::sales::{Cart, CartLine, SaleExtras, SaleInput, create_sale_with};
use crate::stock::serials::{SerialStatus, imei_ok};
use crate::testkit::{NOON, Shop, shop};

fn phones() -> Shop {
    let shop = shop();
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| {
            packs::set_modules(tx, &actor, &[Module::Serials, Module::Flexy, Module::Staff])
        })
        .unwrap();
    shop.open_till(500_000);
    shop
}

fn phone_with_imeis(shop: &Shop, imeis: &[&'static str]) -> String {
    let id = shop
        .product("Redmi Note 14", Money(3_200_000), &shop.tva19)
        .id;
    let pid = id.clone();
    let pieces = 1_000 * i64::try_from(imeis.len()).unwrap();
    let imeis: Vec<String> = imeis.iter().map(|s| (*s).to_owned()).collect();
    shop.db
        .write(move |tx| {
            extras::set_quiet(
                tx,
                &pid,
                ProductExtrasInput {
                    track_serials: true,
                    ..Default::default()
                },
            )?;
            for imei in &imeis {
                crate::stock::serials::register(
                    tx,
                    &pid,
                    imei,
                    Some(crate::stock::SHOP_LOCATION_ID),
                    None,
                    NOON,
                )?;
            }
            Ok(())
        })
        .unwrap();
    shop.stock_in(&id, pieces);
    id
}

fn sell(
    shop: &Shop,
    lines: Vec<CartLine>,
    customer: Option<String>,
    extras: SaleExtras,
) -> Result<crate::sales::ValidatedSale, crate::error::AppError> {
    let ctx = shop.till();
    let mut cart = Cart::new(Lang::Fr, lines);
    cart.customer_id = customer;
    let input = SaleInput {
        cart,
        payments: vec![PaymentInput::cash(Money(100_000_000))],
    };
    shop.db
        .write(move |tx| create_sale_with(tx, &ctx, input, &extras))
}

const IMEI_A: &str = "356938035643809";
const IMEI_B: &str = "490154203237518";

#[test]
fn imeis_are_checked_by_luhn() {
    assert!(imei_ok(IMEI_A) && imei_ok(IMEI_B));
    assert!(!imei_ok("356938035643808"));
    assert!(!imei_ok("35693803564380"));
}

#[test]
fn a_serial_is_sold_once_and_comes_back_with_its_line() {
    let shop = phones();
    let redmi = phone_with_imeis(&shop, &[IMEI_A, IMEI_B]);
    let none = sell(
        &shop,
        vec![CartLine::product(&redmi, Qty(1_000))],
        None,
        SaleExtras::default(),
    )
    .unwrap_err();
    assert_eq!(none.code(), "serials_required");
    let one = SaleExtras {
        serials: vec![SaleSerial {
            line: 0,
            serial_no: IMEI_A.into(),
        }],
        ..SaleExtras::default()
    };
    let sale = sell(
        &shop,
        vec![CartLine::product(&redmi, Qty(1_000))],
        None,
        one.clone(),
    )
    .unwrap();
    let again = sell(
        &shop,
        vec![CartLine::product(&redmi, Qty(1_000))],
        None,
        one,
    )
    .unwrap_err();
    assert_eq!(again.code(), "serial_status_invalid", "sold once");
    let ctx = shop.approved_till(crate::auth::Permission::SalesReturn);
    let line_id = sale.document.lines[0].id.clone();
    let input = ReturnInput {
        original_id: Some(sale.document.id.clone()),
        lines: vec![ReturnLineInput {
            origin_line_id: Some(line_id),
            product_id: None,
            qty: Qty(1_000),
            unit_price: None,
            restock: true,
            reason: ReturnReason::Defective,
            note: None,
        }],
        refund_method: crate::payments::PaymentMethod::CardCib,
        refund_reference: Some("R1".into()),
        lang: Lang::Fr,
        note: None,
    };
    let (i2, c2) = (input.clone(), ctx.clone());
    let wrong = ReturnExtras {
        serials: vec![ReturnSerial {
            line: 0,
            serial_no: IMEI_B.into(),
        }],
        ..ReturnExtras::default()
    };
    let err = shop
        .db
        .write(move |tx| create_return_with(tx, &c2, i2, None, &wrong))
        .unwrap_err();
    assert_eq!(err.code(), "serial_not_on_line");
    let right = ReturnExtras {
        serials: vec![ReturnSerial {
            line: 0,
            serial_no: IMEI_A.into(),
        }],
        ..ReturnExtras::default()
    };
    shop.db
        .write(move |tx| create_return_with(tx, &ctx, input, None, &right))
        .unwrap();
    let back = shop
        .db
        .read(|c| crate::stock::serials::find(c, IMEI_A))
        .unwrap();
    assert_eq!(back[0].status, SerialStatus::InStock);
}

#[test]
fn a_bad_imei_and_a_warranted_phone_without_buyer_are_refused() {
    let shop = phones();
    let redmi = phone_with_imeis(&shop, &[IMEI_A]);
    let (actor, pid) = (shop.owner.actor(), redmi.clone());
    shop.db
        .write(move |tx| {
            let mut e = ProductExtrasInput::from(&extras::get(tx, &pid)?);
            e.warranty_months = Some(12);
            extras::set(tx, &actor, &pid, e)
        })
        .unwrap();
    let buyer = shop.customer("Walid Mansouri").id;
    let bad = SaleExtras {
        serials: vec![SaleSerial {
            line: 0,
            serial_no: "356938035643808".into(),
        }],
        ..SaleExtras::default()
    };
    let err = sell(
        &shop,
        vec![CartLine::product(&redmi, Qty(1_000))],
        Some(buyer.clone()),
        bad,
    )
    .unwrap_err();
    assert_eq!(err.code(), "imei_invalid");
    let good = SaleExtras {
        serials: vec![SaleSerial {
            line: 0,
            serial_no: IMEI_A.into(),
        }],
        ..SaleExtras::default()
    };
    assert_eq!(
        sell(
            &shop,
            vec![CartLine::product(&redmi, Qty(1_000))],
            None,
            good.clone()
        )
        .unwrap_err()
        .code(),
        "buyer_required"
    );
    assert!(
        sell(
            &shop,
            vec![CartLine::product(&redmi, Qty(1_000))],
            Some(buyer),
            good
        )
        .is_ok()
    );
}

#[test]
fn top_ups_draw_on_the_operators_float_and_are_never_returned() {
    let shop = phones();
    let (actor, pack) = (shop.owner.actor(), shop.pack.clone());
    assert_eq!(
        shop.db
            .write(move |tx| flexy::seed(tx, &actor, &pack))
            .unwrap(),
        3
    );
    let pack = shop.pack.clone();
    assert_eq!(
        flexy::guess(&pack, "0661 23 45 67").as_deref(),
        Some("mobilis")
    );
    assert_eq!(
        flexy::guess(&pack, "+213 770 12 34 56").as_deref(),
        Some("djezzy")
    );
    let pack = shop.pack.clone();
    let line = shop
        .db
        .read(move |c| flexy::line(c, &pack, "mobilis", "0661234567", Money(100_000)))
        .unwrap();
    let dry = sell(&shop, vec![line.clone()], None, SaleExtras::default()).unwrap_err();
    assert_eq!(dry.code(), "flexy_float_insufficient");
    let (ctx, pack) = (shop.till(), shop.pack.clone());
    let buy = FloatPurchase {
        operator: "mobilis".into(),
        amount: Money(1_000_000),
        cost: Money(960_000),
        from_till: true,
        note: None,
    };
    shop.db
        .write(move |tx| flexy::purchase(tx, &ctx, &pack, buy))
        .unwrap();
    assert_eq!(
        shop.count("SELECT SUM(amount) FROM cash_movements WHERE kind = 'paid_out'"),
        -960_000,
        "the float was paid from the drawer"
    );
    let sale = sell(&shop, vec![line], None, SaleExtras::default()).unwrap();
    assert_eq!(
        sale.document.lines[0].note.as_deref(),
        Some("0661 23 45 67")
    );
    let pack = shop.pack.clone();
    let balances = shop.db.read(move |c| flexy::balances(c, &pack)).unwrap();
    assert!(balances.contains(&("mobilis".to_owned(), Money(900_000))));
    assert_eq!(
        shop.count("SELECT CAST(phone_tail AS INTEGER) FROM flexy_float WHERE kind = 'sale'"),
        4567
    );
    let ctx = shop.approved_till(crate::auth::Permission::SalesReturn);
    let input = ReturnInput {
        original_id: Some(sale.document.id.clone()),
        lines: vec![ReturnLineInput {
            origin_line_id: Some(sale.document.lines[0].id.clone()),
            product_id: None,
            qty: Qty(1_000),
            unit_price: None,
            restock: false,
            reason: ReturnReason::Other,
            note: None,
        }],
        refund_method: crate::payments::PaymentMethod::Cash,
        refund_reference: None,
        lang: Lang::Fr,
        note: None,
    };
    let err = shop
        .db
        .write(move |tx| create_return_with(tx, &ctx, input, None, &ReturnExtras::default()))
        .unwrap_err();
    assert_eq!(err.code(), "flexy_not_returnable");
}

/// A defective phone swapped for another (RET-004): the exchange names the serial coming back and
/// the one sold, in the same transaction; without them it is refused like a return or a sale.
#[test]
fn an_exchange_brings_back_one_serial_and_sells_another() {
    let shop = phones();
    let redmi = phone_with_imeis(&shop, &[IMEI_A, IMEI_B]);
    let one = SaleExtras {
        serials: vec![SaleSerial {
            line: 0,
            serial_no: IMEI_A.into(),
        }],
        ..SaleExtras::default()
    };
    let sale = sell(
        &shop,
        vec![CartLine::product(&redmi, Qty(1_000))],
        None,
        one,
    )
    .unwrap();
    let ret = ReturnInput {
        original_id: Some(sale.document.id.clone()),
        lines: vec![ReturnLineInput {
            origin_line_id: Some(sale.document.lines[0].id.clone()),
            product_id: None,
            qty: Qty(1_000),
            unit_price: None,
            restock: false,
            reason: ReturnReason::Defective,
            note: None,
        }],
        refund_method: crate::payments::PaymentMethod::Cash,
        refund_reference: None,
        lang: Lang::Fr,
        note: None,
    };
    let swap = SaleInput {
        cart: Cart::new(Lang::Fr, vec![CartLine::product(&redmi, Qty(1_000))]),
        payments: vec![],
    };
    let (ctx, r, s) = (shop.till(), ret.clone(), swap.clone());
    let bare = shop
        .db
        .write(move |tx| crate::returns::exchange(tx, &ctx, r, s))
        .unwrap_err();
    assert_eq!(bare.code(), "serials_required");
    let back = ReturnExtras {
        serials: vec![ReturnSerial {
            line: 0,
            serial_no: IMEI_A.into(),
        }],
        ..ReturnExtras::default()
    };
    let out = SaleExtras {
        serials: vec![SaleSerial {
            line: 0,
            serial_no: IMEI_B.into(),
        }],
        ..SaleExtras::default()
    };
    let ctx = shop.till();
    let ex = shop
        .db
        .write(move |tx| crate::returns::exchange_with(tx, &ctx, ret, swap, &back, &out))
        .unwrap();
    assert_eq!(ex.net_to_pay, Money::ZERO);
    let status = |imei: &'static str| {
        shop.db
            .read(move |c| crate::stock::serials::find(c, imei))
            .unwrap()[0]
            .clone()
    };
    assert_eq!(
        status(IMEI_A).status,
        SerialStatus::Returned,
        "the defective one is set aside"
    );
    let sold = status(IMEI_B);
    assert_eq!(sold.status, SerialStatus::Sold);
    assert_eq!(
        sold.sale_line_id.as_deref(),
        Some(ex.sale.document.lines[0].id.as_str())
    );
}

/// RPT-074: per day and operator, the top-ups sold, the commission at the operator's rate and the
/// float bought; the report needs the flexy module.
#[test]
fn the_flexy_report_shows_top_ups_commission_and_float_per_operator() {
    use crate::reports::{ReportKind, ReportRequest, run};
    use crate::table::Cell;
    let shop = phones();
    let (actor, pack) = (shop.owner.actor(), shop.pack.clone());
    shop.db
        .write(move |tx| flexy::seed(tx, &actor, &pack))
        .unwrap();
    let mut set = shop
        .db
        .read(|c| crate::settings::get_value(c, "flexy.operators"))
        .unwrap();
    for op in set.as_array_mut().unwrap() {
        if op["code"] == "mobilis" {
            op["commissionBp"] = serde_json::json!(300);
        }
    }
    shop.set("flexy.operators", set);
    let (ctx, pack) = (shop.till(), shop.pack.clone());
    let buy = FloatPurchase {
        operator: "mobilis".into(),
        amount: Money(1_000_000),
        cost: Money(960_000),
        from_till: false,
        note: Some("Distributeur".into()),
    };
    shop.db
        .write(move |tx| flexy::purchase(tx, &ctx, &pack, buy))
        .unwrap();
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM cash_movements WHERE kind = 'paid_out'"),
        0,
        "paid outside the till"
    );
    for (phone, amount) in [
        ("0661234567", 100_000),
        ("0555123456", 20_000),
        ("0662000000", 50_000),
    ] {
        let pack = shop.pack.clone();
        let operator = flexy::guess(&pack, phone).unwrap();
        let line = shop
            .db
            .read(move |c| flexy::line(c, &pack, &operator, phone, Money(amount)))
            .unwrap_or_else(|e| panic!("{phone}: {e}"));
        if phone.starts_with("05") {
            // Ooredoo has no float yet.
            let err = sell(&shop, vec![line], None, SaleExtras::default()).unwrap_err();
            assert_eq!(err.code(), "flexy_float_insufficient");
        } else {
            sell(&shop, vec![line], None, SaleExtras::default()).unwrap();
        }
    }
    let entries = shop
        .db
        .read(|c| flexy::entries(c, Some("mobilis"), "2026-09-28", "2026-09-28"))
        .unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].kind, flexy::FloatKind::Purchase);
    let req = ReportRequest::new(ReportKind::FlexySales, "2026-09-28", "2026-09-28");
    let report = shop.db.read(move |c| run(c, &req)).unwrap();
    assert_eq!(
        report.table.rows,
        vec![vec![
            Cell::text("2026-09-28"),
            Cell::text("mobilis"),
            Cell::int(2),
            Cell::money(Money(150_000)),
            Cell::money(Money(4_500)),
            Cell::money(Money(1_000_000)),
            Cell::money(Money(960_000)),
        ]],
        "3 % of 1 500 DA; Ooredoo sold nothing"
    );
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| packs::set_modules(tx, &actor, &[Module::Serials]))
        .unwrap();
    let req = ReportRequest::new(ReportKind::FlexySales, "2026-09-28", "2026-09-28");
    assert_eq!(
        shop.db.read(move |c| run(c, &req)).unwrap_err().code(),
        "module_disabled"
    );
}

/// The float is the credit sent on the network: a discount the shop gives on a top-up comes out
/// of its own margin, never out of the operator's float, so the ledger stays the operator's
/// balance.
#[test]
fn a_discounted_top_up_spends_its_face_value_of_float() {
    let shop = phones();
    let (actor, pack) = (shop.owner.actor(), shop.pack.clone());
    shop.db
        .write(move |tx| flexy::seed(tx, &actor, &pack))
        .unwrap();
    let (ctx, pack) = (shop.till(), shop.pack.clone());
    let buy = FloatPurchase {
        operator: "djezzy".into(),
        amount: Money(500_000),
        cost: Money(480_000),
        from_till: false,
        note: None,
    };
    shop.db
        .write(move |tx| flexy::purchase(tx, &ctx, &pack, buy))
        .unwrap();
    let pack = shop.pack.clone();
    let mut line = shop
        .db
        .read(move |c| flexy::line(c, &pack, "djezzy", "0770123456", Money(20_000)))
        .unwrap();
    line.discount = crate::tax::tva::Discount::Percent(crate::money::Rate(500));
    let ctx = shop.approved_till(crate::auth::Permission::SalesDiscount);
    let input = SaleInput {
        cart: Cart::new(Lang::Fr, vec![line]),
        payments: vec![PaymentInput::cash(Money(100_000))],
    };
    let sale = shop
        .db
        .write(move |tx| create_sale_with(tx, &ctx, input, &SaleExtras::default()))
        .unwrap();
    assert_eq!(sale.document.lines[0].total_ttc, Money(19_000));
    let pack = shop.pack.clone();
    let balances = shop.db.read(move |c| flexy::balances(c, &pack)).unwrap();
    assert!(
        balances.contains(&("djezzy".to_owned(), Money(480_000))),
        "{balances:?}"
    );
}

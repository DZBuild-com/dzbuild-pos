//! Jewellery (Task 14).

use crate::jewellery::{self as jw, BuyerIdKind, BuyerIdentity, Metal, PieceInput};
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::packs::{self, Module};
use crate::payments::{PaymentInput, PaymentMethod};
use crate::sales::{
    Cart, CartLine, SaleExtras, SaleInput, ValidatedSale, create_sale_with, price_cart,
};
use crate::testkit::{Shop, shop};
use serde_json::json;

fn bijouterie() -> (Shop, String) {
    let shop = shop();
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| packs::set_modules(tx, &actor, &[Module::Jewellery]))
        .unwrap();
    shop.open_till(100_000);
    let ctx = shop.owner_till();
    shop.db
        .write(move |tx| jw::set_metal_price(tx, &ctx, Metal::Gold, 750, Money(1_450_000)))
        .unwrap();
    let ring = shop.product("Bague or 18 carats", Money(0), &shop.tva19).id;
    let actor = shop.owner.actor();
    let r = ring.clone();
    shop.db
        .write(move |tx| {
            jw::set_piece(
                tx,
                &actor,
                &r,
                PieceInput {
                    metal: Metal::Gold,
                    titre_permille: 750,
                    weight_mg: 5_200,
                    facon: Money(300_000),
                    stones: None,
                    hallmark: Some("Tête de Massinissa".into()),
                    priced_by_weight: true,
                },
            )
        })
        .unwrap();
    shop.stock_in(&ring, 3_000);
    (shop, ring)
}

fn buyer() -> BuyerIdentity {
    BuyerIdentity {
        name: "Samira Belkacem".into(),
        id_kind: BuyerIdKind::Cni,
        id_number: "109876543210987654".into(),
        address: "12 rue Didouche Mourad, Alger".into(),
    }
}

fn sell(
    shop: &Shop,
    product: &str,
    customer: Option<&str>,
    payments: Vec<PaymentInput>,
    buyer: Option<BuyerIdentity>,
) -> Result<ValidatedSale, crate::error::AppError> {
    let ctx = shop.till();
    let mut cart = Cart::new(Lang::Fr, vec![CartLine::product(product, Qty(1_000))]);
    cart.customer_id = customer.map(str::to_owned);
    let extras = SaleExtras {
        buyer,
        ..SaleExtras::default()
    };
    shop.db
        .write(move |tx| create_sale_with(tx, &ctx, SaleInput { cart, payments }, &extras))
}

fn card(amount: i64) -> Vec<PaymentInput> {
    vec![PaymentInput::electronic(
        PaymentMethod::CardCib,
        Money(amount),
        Some("A7"),
    )]
}

/// The jewellery shop of these tests, for the report tests.
pub(crate) fn bijouterie_for_reports() -> (Shop, String) {
    bijouterie()
}

/// One ring sold by card to the identified buyer, for the report tests.
pub(crate) fn sell_ring_for_reports(shop: &Shop, ring: &str) {
    sell(shop, ring, None, card(7_840_000), Some(buyer())).unwrap();
}

#[test]
fn a_piece_priced_by_weight_follows_the_gold_board() {
    let (shop, ring) = bijouterie();
    let price = |shop: &Shop| {
        let ctx = shop.till();
        let cart = Cart::new(Lang::Fr, vec![CartLine::product(&ring, Qty(1_000))]);
        shop.db
            .read(move |c| price_cart(c, &ctx, &cart))
            .unwrap()
            .lines[0]
            .unit_price
    };
    assert_eq!(
        price(&shop),
        Money(7_840_000),
        "5.200 g × 14 500 DA + 3 000 DA making charge"
    );
    let ctx = shop.owner_till();
    shop.db
        .write(move |tx| jw::set_metal_price(tx, &ctx, Metal::Gold, 750, Money(1_500_000)))
        .unwrap();
    assert_eq!(price(&shop), Money(8_100_000));
    let ctx = shop.owner_till();
    let report = shop.db.write(move |tx| jw::reprice(tx, &ctx)).unwrap();
    assert_eq!((report.pieces, report.changed), (1, 1));
    let r = ring.clone();
    assert_eq!(
        shop.db
            .read(move |c| crate::catalog::products::get(c, &r))
            .unwrap()
            .sell_price,
        Money(8_100_000)
    );
    let ctx = shop.owner_till();
    let err = shop
        .db
        .write(move |tx| jw::set_metal_price(tx, &ctx, Metal::Gold, 1_001, Money(1)))
        .unwrap_err();
    assert_eq!(err.code(), "jewel_piece_invalid");
}

#[test]
fn the_jeweller_register_is_append_only_gap_free_and_needs_the_buyer() {
    let (shop, ring) = bijouterie();
    assert_eq!(
        sell(&shop, &ring, None, card(7_840_000), None)
            .unwrap_err()
            .code(),
        "buyer_identity_required"
    );
    sell(&shop, &ring, None, card(7_840_000), Some(buyer())).unwrap();
    sell(&shop, &ring, None, card(7_840_000), Some(buyer())).unwrap();
    let entries = shop.db.read(|c| jw::entries(c, 2026, 1, 100)).unwrap();
    assert_eq!(entries.iter().map(|e| e.seq).collect::<Vec<_>>(), [1, 2]);
    let e = &entries[0];
    assert_eq!(
        (e.weight_mg, e.titre_permille, e.price, e.buyer_id_kind),
        (5_200, 750, Money(7_840_000), BuyerIdKind::Cni)
    );
    assert_eq!(e.nature, "Bague or 18 carats");
    assert!(
        shop.db
            .write(|tx| Ok(tx.execute_batch("UPDATE jeweller_register SET buyer_name = 'X'")?))
            .is_err()
    );
    assert!(
        shop.db
            .write(|tx| Ok(tx.execute_batch("DELETE FROM jeweller_register")?))
            .is_err()
    );
    let soap = shop.product("Écrin", Money(50_000), &shop.tva19).id;
    sell(
        &shop,
        &soap,
        None,
        // Cash carries the droit de timbre on top: 600 DA cover it, the rest is change.
        vec![PaymentInput::cash(Money(60_000))],
        None,
    )
    .unwrap();
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM jeweller_register"),
        2,
        "a box is not a precious-metal piece"
    );
    let journal: String = shop
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT payload FROM audit_log WHERE action = 'sale.created' ORDER BY seq DESC LIMIT 1 OFFSET 1",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert!(
        !journal.contains("109876543210987654"),
        "the journal never copies the identity number"
    );
}

#[test]
fn the_buyer_comes_from_a_complete_customer_file() {
    let (shop, ring) = bijouterie();
    let c = shop.customer("Samira Belkacem").id;
    let id = c.clone();
    shop.db
        .write(move |tx| {
            Ok(tx.execute(
                "UPDATE customers SET nin = '109876543210987654', address = 'Alger' WHERE id = ?1",
                [&id],
            )?)
        })
        .unwrap();
    sell(&shop, &ring, Some(&c), card(7_840_000), None).unwrap();
    let e = shop
        .db
        .read(|c| jw::entries(c, 2026, 1, 1))
        .unwrap()
        .remove(0);
    assert_eq!(
        (
            e.buyer_name.as_str(),
            e.buyer_id_number.as_str(),
            e.buyer_address.as_str()
        ),
        ("Samira Belkacem", "109876543210987654", "Alger")
    );
}

#[test]
fn cash_above_the_set_limit_is_refused_for_precious_metal() {
    let (shop, ring) = bijouterie();
    shop.set("jewellery.max_cash", json!(5_000_000));
    // The cash part carries the droit de timbre on top: the cash handed over covers it and the
    // rest comes back as change. What the ticket takes in cash is the cash kept: tendered −
    // change, duty included.
    let payments = vec![
        PaymentInput::cash(Money(6_100_000)),
        PaymentInput::electronic(PaymentMethod::CardCib, Money(1_840_000), Some("A8")),
    ];
    let err = sell(&shop, &ring, None, payments, Some(buyer())).unwrap_err();
    assert_eq!(err.code(), "cash_above_limit");
    let payments = vec![
        PaymentInput::cash(Money(5_000_000)),
        PaymentInput::electronic(PaymentMethod::CardCib, Money(2_940_000), Some("A9")),
    ];
    let sale = sell(&shop, &ring, None, payments, Some(buyer())).unwrap();
    assert!(
        sale.document.stamp_duty.is_positive() && sale.document.change_given.is_positive(),
        "the duty is paid from the 50 000 DA in cash, with change"
    );
}

#[test]
fn the_label_carries_metal_fineness_weight_and_hallmark() {
    let (shop, ring) = bijouterie();
    let r = ring.clone();
    let data = shop
        .db
        .read(move |c| {
            crate::labels::build::preview_label(
                c,
                &crate::labels::template::builtin_jewel(),
                Some(&r),
                None,
                "2026-09-28",
            )
        })
        .unwrap();
    let jewel = data.jewel.unwrap();
    assert_eq!(
        (
            jewel.metal,
            jewel.titre_permille,
            jewel.weight_mg,
            jewel.hallmark.as_deref()
        ),
        (Metal::Gold, 750, 5_200, Some("Tête de Massinissa"))
    );
    assert_eq!(data.price, Money(7_840_000));
}

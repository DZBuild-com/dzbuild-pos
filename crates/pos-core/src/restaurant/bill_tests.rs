use super::bill::{self, PayInput, SplitBy};
use super::kitchen::{self, KitchenKind, StationInput};
use super::tests::{add, dine_in, restaurant};
use super::tickets::{CompReason, TicketStatus, VoidReason};
use crate::auth::Permission;
use crate::money::{Money, Qty};
use crate::payments::PaymentInput;
use crate::testkit::{NOON, Shop};
use proptest::prelude::*;

fn station(shop: &Shop, code: &str, category: &str) -> String {
    let actor = shop.owner.actor();
    let input = StationInput {
        code: code.into(),
        name_ar: "المطبخ".into(),
        name_fr: code.into(),
        name_en: code.into(),
        sort_order: 1,
        category_ids: vec![category.into()],
    };
    shop.db
        .write(move |tx| kitchen::save_station(tx, &actor, None, input))
        .unwrap()
        .id
}

fn category(shop: &Shop, fr: &str) -> String {
    let actor = shop.owner.actor();
    let input = crate::catalog::categories::CategoryInput {
        name_fr: fr.into(),
        ..Default::default()
    };
    shop.db
        .write(move |tx| crate::catalog::categories::create(tx, &actor, input))
        .unwrap()
        .id
}

fn product_in(shop: &Shop, name: &str, price: i64, category: &str) -> String {
    let mut input = crate::catalog::products::ProductInput::simple(
        name,
        crate::catalog::UNIT_PIECE_ID,
        &shop.tva9,
        Money(price),
    );
    input.category_id = Some(category.into());
    input.track_stock = false;
    shop.product_with(input).id
}

fn manager_ctx(shop: &Shop, permission: Permission) -> crate::till::TillContext {
    shop.approved_till(permission)
}

#[test]
fn lines_go_to_their_station_once() {
    let (shop, t1, _) = restaurant();
    let (plats, boissons) = (category(&shop, "Plats"), category(&shop, "Boissons"));
    let cuisine = station(&shop, "CUISINE", &plats);
    let bar = station(&shop, "BAR", &boissons);
    let chakhchoukha = product_in(&shop, "Chakhchoukha", 70_000, &plats);
    let citronnade = product_in(&shop, "Citronnade", 12_000, &boissons);
    let ticket = dine_in(&shop, &t1, 2);
    add(&shop, &ticket.id, &chakhchoukha, 2_000, Some(1), vec![]);
    add(&shop, &ticket.id, &citronnade, 2_000, None, vec![]);
    let ctx = shop.till();
    let id = ticket.id.clone();
    let sent = shop
        .db
        .write(move |tx| kitchen::send(tx, &ctx, &id))
        .unwrap();
    let mut routed: Vec<(Option<String>, String, Qty)> = sent
        .iter()
        .map(|k| {
            (
                k.station_id.clone(),
                k.lines[0].name.clone(),
                k.lines[0].qty,
            )
        })
        .collect();
    routed.sort();
    let mut expected = vec![
        (Some(bar), "Citronnade".to_owned(), Qty(2_000)),
        (Some(cuisine), "Chakhchoukha".to_owned(), Qty(2_000)),
    ];
    expected.sort();
    assert_eq!(routed, expected);
    assert_eq!(sent[0].table_code.as_deref(), Some("T1"));
    let ctx = shop.till();
    let id = ticket.id.clone();
    let again = shop
        .db
        .write(move |tx| kitchen::send(tx, &ctx, &id))
        .unwrap_err();
    assert_eq!(again.code(), "kitchen_nothing_to_send");
    add(&shop, &ticket.id, &chakhchoukha, 1_000, None, vec![]);
    let ctx = shop.till();
    let id = ticket.id.clone();
    let more = shop
        .db
        .write(move |tx| kitchen::send(tx, &ctx, &id))
        .unwrap();
    assert_eq!(
        (more.len(), more[0].seq),
        (1, 3),
        "only the new dish, numbered after the first two"
    );
}

#[test]
fn a_void_after_the_kitchen_prints_a_cancellation_and_is_journaled() {
    let (shop, t1, _) = restaurant();
    let plats = category(&shop, "Plats");
    let cuisine = station(&shop, "CUISINE", &plats);
    let pizza = product_in(&shop, "Pizza margherita", 60_000, &plats);
    let ticket = dine_in(&shop, &t1, 1);
    let ticket = add(&shop, &ticket.id, &pizza, 2_000, None, vec![]);
    let ctx = shop.till();
    let id = ticket.id.clone();
    shop.db
        .write(move |tx| kitchen::send(tx, &ctx, &id))
        .unwrap();
    let ctx = shop.till();
    let line = ticket.lines[0].id.clone();
    let refused = shop
        .db
        .write(move |tx| {
            bill::void_line(
                tx,
                &ctx,
                &line,
                Some(Qty(1_000)),
                VoidReason::CustomerChanged,
            )
        })
        .unwrap_err();
    assert_eq!(
        refused.code(),
        "insufficient_permission",
        "a cashier needs the manager"
    );
    let ctx = manager_ctx(&shop, Permission::SalesVoidLine);
    let line = ticket.lines[0].id.clone();
    let out = shop
        .db
        .write(move |tx| {
            bill::void_line(
                tx,
                &ctx,
                &line,
                Some(Qty(1_000)),
                VoidReason::CustomerChanged,
            )
        })
        .unwrap();
    assert_eq!(out.ticket.total_ttc, Money(60_000));
    assert_eq!(out.cancels.len(), 1);
    let cancel = &out.cancels[0];
    assert_eq!(
        (
            cancel.kind,
            cancel.station_id.as_deref(),
            cancel.lines[0].qty
        ),
        (KitchenKind::Cancel, Some(cuisine.as_str()), Qty(1_000))
    );
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'dining.line_voided'"),
        1
    );
    let payload: String = shop
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT payload FROM audit_log WHERE action = 'dining.line_voided'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert!(
        payload.contains("\"reason\":\"customer_changed\"") && payload.contains("\"value\":60000")
    );
}

#[test]
fn a_comped_line_is_free_with_its_reason_and_needs_the_discount_right() {
    let (shop, t1, _) = restaurant();
    let dessert = shop.product("Tamina", Money(20_000), &shop.tva9).id;
    let ticket = dine_in(&shop, &t1, 1);
    let ticket = add(&shop, &ticket.id, &dessert, 1_000, None, vec![]);
    let ctx = shop.till();
    let line = ticket.lines[0].id.clone();
    let err = shop
        .db
        .write(move |tx| bill::comp_line(tx, &ctx, &line, CompReason::Offered))
        .unwrap_err();
    assert_eq!(
        err.code(),
        "discount_not_allowed",
        "a cashier does not give discounts"
    );
    let ctx = manager_ctx(&shop, Permission::SalesDiscount);
    let line = ticket.lines[0].id.clone();
    let comped = shop
        .db
        .write(move |tx| bill::comp_line(tx, &ctx, &line, CompReason::Offered))
        .unwrap();
    assert_eq!(
        (comped.total_ttc, comped.lines[0].comp_reason),
        (Money::ZERO, Some(CompReason::Offered))
    );
}

#[test]
fn the_addition_marks_the_table_billed_and_shows_the_cash_duty() {
    let (shop, t1, _) = restaurant();
    let grill = shop
        .product("Grillade mixte", Money(150_000), &shop.tva9)
        .id;
    let ticket = dine_in(&shop, &t1, 3);
    add(&shop, &ticket.id, &grill, 3_000, None, vec![]);
    let ctx = shop.till();
    let id = ticket.id.clone();
    let bill = shop
        .db
        .write(move |tx| bill::prebill(tx, &ctx, &id, Some(3)))
        .unwrap();
    assert_eq!(bill.ticket.status, TicketStatus::Billed);
    assert_eq!(bill.priced.totals.total_ttc, Money(450_000));
    assert_eq!(
        bill.stamp_duty,
        Money(4_500),
        "1 DA per 100 DA on 4 500 DA paid in cash"
    );
    assert_eq!(bill.cash_due, Money(454_500));
    assert_eq!(
        bill.shares,
        [Money(150_000), Money(150_000), Money(150_000)]
    );
    let board = shop
        .db
        .read(|c| super::floor::board(c, None, NOON))
        .unwrap();
    assert_eq!(board[0].state, super::floor::TableState::Billed);
    let the = shop.product("Thé", Money(8_000), &shop.tva9).id;
    let reopened = add(&shop, &ticket.id, &the, 1_000, None, vec![]);
    assert_eq!(
        reopened.status,
        TicketStatus::Open,
        "a new line makes the addition stale"
    );
}

#[test]
fn a_split_by_seat_moves_exactly_that_seats_lines() {
    let (shop, t1, _) = restaurant();
    let (a, b) = (
        shop.product("Plat A", Money(50_000), &shop.tva9).id,
        shop.product("Plat B", Money(40_000), &shop.tva9).id,
    );
    let ticket = dine_in(&shop, &t1, 2);
    add(&shop, &ticket.id, &a, 1_000, Some(1), vec![]);
    add(&shop, &ticket.id, &b, 1_000, Some(2), vec![]);
    add(&shop, &ticket.id, &a, 1_000, Some(2), vec![]);
    let ctx = shop.till();
    let id = ticket.id.clone();
    let done = shop
        .db
        .write(move |tx| bill::split(tx, &ctx, &id, SplitBy::Seat { seat: 2 }))
        .unwrap();
    assert_eq!(
        (done.from.total_ttc, done.to.total_ttc),
        (Money(50_000), Money(90_000))
    );
    assert_eq!(done.to.parent_id.as_deref(), Some(ticket.id.as_str()));
    assert_eq!(done.to.table_id, done.from.table_id);
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM dining_lines WHERE status = 'active'"),
        3,
        "moved, never copied"
    );
    let ctx = shop.till();
    let id = ticket.id.clone();
    let err = shop
        .db
        .write(move |tx| bill::split(tx, &ctx, &id, SplitBy::Seat { seat: 1 }))
        .unwrap_err();
    assert_eq!(
        err.code(),
        "split_invalid",
        "the last lines never leave their ticket empty"
    );
}

#[test]
fn a_paid_ticket_is_one_validated_sale_priced_like_the_till() {
    let (shop, t1, _) = restaurant();
    let couscous = shop.product("Couscous", Money(90_000), &shop.tva9).id;
    shop.open_till(100_000);
    let ticket = dine_in(&shop, &t1, 2);
    add(&shop, &ticket.id, &couscous, 2_000, None, vec![]);
    let ctx = shop.till();
    let id = ticket.id.clone();
    let paid = shop
        .db
        .write(move |tx| {
            bill::pay(
                tx,
                &ctx,
                PayInput {
                    ticket_id: id,
                    payments: vec![PaymentInput::cash(Money(200_000))],
                },
            )
        })
        .unwrap();
    let doc = &paid.sale.document;
    assert_eq!(
        (doc.total_ttc, doc.stamp_duty, doc.total_due),
        (Money(180_000), Money(1_800), Money(181_800))
    );
    assert_eq!(paid.ticket.status, TicketStatus::Paid);
    assert_eq!(paid.ticket.document_id.as_deref(), Some(doc.id.as_str()));
    assert_eq!(
        paid.kitchen.len(),
        1,
        "unsent lines went to the kitchen at the payment"
    );
    let info = shop
        .db
        .read(move |c| bill::dining_of_document(c, &paid.sale.document.id))
        .unwrap()
        .unwrap();
    assert_eq!((info.table_code.as_deref(), info.covers), (Some("T1"), 2));
}

#[test]
fn a_ticket_is_paid_once() {
    let (shop, t1, _) = restaurant();
    let the = shop.product("Thé", Money(8_000), &shop.tva9).id;
    shop.open_till(100_000);
    let ticket = dine_in(&shop, &t1, 1);
    add(&shop, &ticket.id, &the, 1_000, None, vec![]);
    for expect_ok in [true, false] {
        let ctx = shop.till();
        let id = ticket.id.clone();
        let res = shop.db.write(move |tx| {
            bill::pay(
                tx,
                &ctx,
                PayInput {
                    ticket_id: id,
                    payments: vec![PaymentInput::cash(Money(8_000))],
                },
            )
        });
        assert_eq!(res.is_ok(), expect_ok);
        if !expect_ok {
            assert_eq!(res.unwrap_err().code(), "ticket_not_open");
        }
    }
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM documents WHERE doc_type = 'ticket'"),
        1
    );
}

#[test]
fn a_cover_charge_applies_only_when_set() {
    let (shop, t1, _) = restaurant();
    let cover = shop.product("Couvert", Money(5_000), &shop.tva9).id;
    let plat = shop.product("Plat", Money(50_000), &shop.tva9).id;
    shop.set("restaurant.cover_product_id", serde_json::json!(cover));
    shop.open_till(100_000);
    let ticket = dine_in(&shop, &t1, 4);
    add(&shop, &ticket.id, &plat, 1_000, None, vec![]);
    let ctx = shop.till();
    let id = ticket.id.clone();
    let paid = shop
        .db
        .write(move |tx| {
            bill::pay(
                tx,
                &ctx,
                PayInput {
                    ticket_id: id,
                    payments: vec![PaymentInput::cash(Money(71_000))],
                },
            )
        })
        .unwrap();
    assert_eq!(
        paid.sale.document.total_ttc,
        Money(70_000),
        "4 covers × 50 DA + the dish"
    );
}

/// The addition tells the amount the payment will take, the charge per cover included.
#[test]
fn the_addition_carries_the_cover_charge_the_payment_takes() {
    let (shop, t1, _) = restaurant();
    let cover = shop.product("Couvert", Money(5_000), &shop.tva9).id;
    let plat = shop.product("Plat", Money(50_000), &shop.tva9).id;
    shop.set("restaurant.cover_product_id", serde_json::json!(cover));
    let ticket = dine_in(&shop, &t1, 4);
    add(&shop, &ticket.id, &plat, 1_000, None, vec![]);
    let ctx = shop.till();
    let id = ticket.id.clone();
    let bill = shop
        .db
        .write(move |tx| bill::prebill(tx, &ctx, &id, None))
        .unwrap();
    assert_eq!(bill.priced.totals.total_ttc, Money(70_000));
    assert_eq!(bill.priced.lines.len(), 2, "the dish, then the covers");
    assert_eq!(bill.priced.lines[1].qty, Qty(4_000));
    assert!(bill.shares.is_empty(), "one payer");
}

/// A whole ticket voided cancels, per station, what the kitchen had; a kitchen ticket is printed
/// once and the till retries only the unprinted ones; an archived station takes no category.
#[test]
fn a_voided_ticket_cancels_what_the_kitchen_had_and_prints_are_marked_once() {
    let (shop, t1, _) = restaurant();
    let plats = category(&shop, "Plats");
    let cuisine = station(&shop, "CUISINE", &plats);
    let pizza = product_in(&shop, "Pizza margherita", 60_000, &plats);
    let ticket = dine_in(&shop, &t1, 2);
    add(&shop, &ticket.id, &pizza, 2_000, None, vec![]);
    let ctx = shop.till();
    let id = ticket.id.clone();
    let sent = shop
        .db
        .write(move |tx| kitchen::send(tx, &ctx, &id))
        .unwrap();
    let terminal = shop.terminal.id.clone();
    let pending = shop
        .db
        .read(move |c| kitchen::pending(c, &terminal))
        .unwrap();
    assert_eq!(pending, sent);
    let ids = vec![sent[0].id.clone()];
    let twice = ids.clone();
    assert_eq!(
        shop.db
            .write(move |tx| kitchen::mark_printed(tx, &ids, NOON))
            .unwrap(),
        1
    );
    assert_eq!(
        shop.db
            .write(move |tx| kitchen::mark_printed(tx, &twice, NOON + 1))
            .unwrap(),
        0,
        "printed_at is set once"
    );
    let terminal = shop.terminal.id.clone();
    assert!(
        shop.db
            .read(move |c| kitchen::pending(c, &terminal))
            .unwrap()
            .is_empty()
    );
    let ctx = shop.till();
    let id = ticket.id.clone();
    let refused = shop
        .db
        .write(move |tx| bill::void_ticket(tx, &ctx, &id, VoidReason::Other))
        .unwrap_err();
    assert_eq!(refused.code(), "insufficient_permission");
    let ctx = manager_ctx(&shop, Permission::SalesVoidLine);
    let id = ticket.id.clone();
    let out = shop
        .db
        .write(move |tx| bill::void_ticket(tx, &ctx, &id, VoidReason::Other))
        .unwrap();
    assert_eq!(
        (
            out.ticket.status,
            out.ticket.total_ttc,
            out.ticket.lines.len()
        ),
        (TicketStatus::Void, Money::ZERO, 0)
    );
    assert_eq!(out.cancels.len(), 1);
    assert_eq!(
        (
            out.cancels[0].kind,
            out.cancels[0].station_id.as_deref(),
            out.cancels[0].lines[0].qty
        ),
        (KitchenKind::Cancel, Some(cuisine.as_str()), Qty(2_000))
    );
    let id = ticket.id.clone();
    let all = shop.db.read(move |c| kitchen::of_ticket(c, &id)).unwrap();
    assert_eq!(
        all.iter().map(|k| (k.seq, k.kind)).collect::<Vec<_>>(),
        [(1, KitchenKind::Order), (2, KitchenKind::Cancel)]
    );
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'dining.ticket_voided'"),
        1
    );
    let actor = shop.owner.actor();
    let archived = cuisine.clone();
    shop.db
        .write(move |tx| kitchen::archive_station(tx, &actor, &archived))
        .unwrap();
    assert!(
        shop.db
            .read(|c| kitchen::stations(c, false))
            .unwrap()
            .is_empty()
    );
    assert_eq!(shop.count("SELECT COUNT(*) FROM category_stations"), 0);
    let actor = shop.owner.actor();
    let again = shop
        .db
        .write(move |tx| kitchen::archive_station(tx, &actor, &cuisine))
        .unwrap_err();
    assert_eq!(again.code(), "kitchen_station_not_found");
}

/// An offered line changes what the table owes: the printed addition no longer tells the truth.
#[test]
fn a_comp_after_the_addition_reopens_the_ticket() {
    let (shop, t1, _) = restaurant();
    let dessert = shop.product("Tamina", Money(20_000), &shop.tva9).id;
    let ticket = dine_in(&shop, &t1, 1);
    let ticket = add(&shop, &ticket.id, &dessert, 1_000, None, vec![]);
    let ctx = shop.till();
    let id = ticket.id.clone();
    let billed = shop
        .db
        .write(move |tx| bill::prebill(tx, &ctx, &id, None))
        .unwrap();
    assert_eq!(billed.ticket.status, TicketStatus::Billed);
    let ctx = manager_ctx(&shop, Permission::SalesDiscount);
    let line = ticket.lines[0].id.clone();
    let comped = shop
        .db
        .write(move |tx| bill::comp_line(tx, &ctx, &line, CompReason::Staff))
        .unwrap();
    assert_eq!(
        (comped.status, comped.billed_at),
        (TicketStatus::Open, None)
    );
}

/// A station code fits the kitchen ticket (12 characters) and names one live station.
#[test]
fn a_station_code_is_short_and_unique() {
    let (shop, _, _) = restaurant();
    let plats = category(&shop, "Plats");
    station(&shop, "GRILL", &plats);
    let try_code = |code: &'static str| {
        let actor = shop.owner.actor();
        let input = StationInput {
            code: code.into(),
            name_ar: String::new(),
            name_fr: "Poste".into(),
            name_en: String::new(),
            sort_order: 2,
            category_ids: vec![],
        };
        shop.db
            .write(move |tx| kitchen::save_station(tx, &actor, None, input))
    };
    assert_eq!(try_code("GRILL-ET-PIZZA").unwrap_err().code(), "too_long");
    assert_eq!(try_code("GRILL").unwrap_err().code(), "code_taken");
    assert_eq!(try_code("PIZZA").unwrap().code, "PIZZA");
}

proptest! {
    #[test]
    fn even_shares_always_sum_to_the_amount_due(total in 1_i64..100_000_000, parts in 2_u8..=20) {
        let shares = bill::even_shares(Money(total), parts).unwrap();
        prop_assert_eq!(shares.len(), usize::from(parts));
        prop_assert_eq!(shares.iter().map(|m| m.0).sum::<i64>(), total);
        let (min, max) = (shares.iter().min().unwrap().0, shares.iter().max().unwrap().0);
        prop_assert!(max - min < i64::from(parts), "fair to the centime");
        prop_assert!(min >= 0);
    }
}

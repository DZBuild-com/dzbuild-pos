//! Task 23: what the floor and the ticket screen need of the backend. An unsent line of a table
//! ticket moves to another seat or course and takes a note for the kitchen, its priced options
//! with it (a line the kitchen received is voided instead); the floor tells which tickets have a
//! kitchen ticket that did not print.

use super::floor;
use super::kitchen;
use super::modifiers::{self, ModifierGroupInput, ModifierOptionInput};
use super::tests::{add, dine_in, restaurant};
use super::tickets::{self, LineEdit};
use crate::money::Money;

#[test]
fn an_unsent_line_changes_seat_course_and_note_with_its_options() {
    let (shop, t1, _) = restaurant();
    let burger = shop.product("Burger", Money(45_000), &shop.tva9).id;
    let cheese = shop
        .product("Supplément fromage", Money(5_000), &shop.tva9)
        .id;
    let actor = shop.owner.actor();
    let group = shop
        .db
        .write(move |tx| {
            modifiers::save_group(
                tx,
                &actor,
                None,
                ModifierGroupInput {
                    name_ar: "إضافات".into(),
                    name_fr: "Suppléments".into(),
                    name_en: "Extras".into(),
                    min_select: 0,
                    max_select: 1,
                    sort_order: 1,
                    options: vec![ModifierOptionInput {
                        id: None,
                        name_ar: "جبن".into(),
                        name_fr: "Fromage".into(),
                        name_en: "Cheese".into(),
                        product_id: Some(cheese),
                        sort_order: 1,
                    }],
                },
            )
        })
        .unwrap();
    let (actor, bid, gid) = (shop.owner.actor(), burger.clone(), group.id.clone());
    shop.db
        .write(move |tx| modifiers::set_product_groups(tx, &actor, &bid, &[gid]))
        .unwrap();
    let ticket = dine_in(&shop, &t1, 2);
    let ticket = add(
        &shop,
        &ticket.id,
        &burger,
        1_000,
        None,
        vec![group.options[0].id.clone()],
    );
    let total = ticket.total_ttc;
    let main = ticket.lines[0].id.clone();

    let ctx = shop.till();
    let id = main.clone();
    let edited = shop
        .db
        .write(move |tx| {
            tickets::edit_unsent(
                tx,
                &ctx,
                &id,
                LineEdit {
                    seat: Some(2),
                    course: Some(2),
                    note: Some("  sans oignons ".into()),
                },
            )
        })
        .unwrap();
    assert_eq!(edited.total_ttc, total, "nothing priced changes");
    let line = &edited.lines[0];
    assert_eq!(
        (line.seat, line.course, line.cart_line.note.as_deref()),
        (Some(2), Some(2), Some("sans oignons"))
    );
    let option = &edited.lines[1];
    assert_eq!(option.parent_line_id.as_deref(), Some(main.as_str()));
    assert_eq!(
        (option.seat, option.course),
        (Some(2), Some(2)),
        "the priced option follows its dish"
    );

    let ctx = shop.till();
    let id = option.id.clone();
    let own = shop.db.write(move |tx| {
        tickets::edit_unsent(
            tx,
            &ctx,
            &id,
            LineEdit {
                seat: None,
                course: None,
                note: None,
            },
        )
    });
    assert_eq!(
        own.unwrap_err().code(),
        "value_invalid",
        "an option moves with its dish only"
    );

    let ctx = shop.till();
    let id = main.clone();
    let out_of_range = shop.db.write(move |tx| {
        tickets::edit_unsent(
            tx,
            &ctx,
            &id,
            LineEdit {
                seat: Some(0),
                course: None,
                note: None,
            },
        )
    });
    assert_eq!(out_of_range.unwrap_err().code(), "value_invalid");

    let ctx = shop.till();
    let tid = ticket.id.clone();
    shop.db
        .write(move |tx| kitchen::send(tx, &ctx, &tid))
        .unwrap();
    let ctx = shop.till();
    let id = main.clone();
    let sent = shop.db.write(move |tx| {
        tickets::edit_unsent(
            tx,
            &ctx,
            &id,
            LineEdit {
                seat: None,
                course: None,
                note: None,
            },
        )
    });
    assert_eq!(sent.unwrap_err().code(), "line_already_sent");
}

#[test]
fn the_floor_counts_the_kitchen_tickets_that_did_not_print() {
    let (shop, t1, _) = restaurant();
    let plat = shop.product("Chorba", Money(35_000), &shop.tva9).id;
    let ticket = dine_in(&shop, &t1, 2);
    add(&shop, &ticket.id, &plat, 1_000, None, vec![]);
    let board = |shop: &crate::testkit::Shop| {
        shop.db
            .read(move |c| floor::board(c, None, crate::testkit::NOON))
            .unwrap()
    };
    assert_eq!(board(&shop)[0].tickets[0].unprinted_kitchen, 0);
    assert_eq!(
        board(&shop)[0].tickets[0].unsent_lines,
        1,
        "final review: the dish is not sent yet"
    );
    let ctx = shop.till();
    let tid = ticket.id.clone();
    let sent = shop
        .db
        .write(move |tx| kitchen::send(tx, &ctx, &tid))
        .unwrap();
    assert_eq!(
        board(&shop)[0].tickets[0].unprinted_kitchen,
        1,
        "sent, not printed yet"
    );
    assert_eq!(
        board(&shop)[0].tickets[0].unsent_lines,
        0,
        "nothing left to send"
    );
    let ids: Vec<String> = sent.iter().map(|k| k.id.clone()).collect();
    shop.db
        .write(move |tx| kitchen::mark_printed(tx, &ids, crate::testkit::NOON))
        .unwrap();
    assert_eq!(board(&shop)[0].tickets[0].unprinted_kitchen, 0);
}

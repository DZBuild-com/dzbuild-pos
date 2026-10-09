use super::floor::{self, RoomInput, TableInput, TableShape, TableState};
use super::menu;
use super::modifiers::{self, ModifierGroupInput, ModifierOptionInput};
use super::tickets::{self, AddLineInput, DiningMode, OpenInput, TicketStatus};
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::packs::{self, Module};
use crate::sales::CartLine;
use crate::testkit::{NOON, Shop, shop};

pub(crate) fn restaurant() -> (Shop, String, String) {
    let shop = shop();
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| packs::set_modules(tx, &actor, &[Module::Restaurant, Module::Staff]))
        .unwrap();
    let actor = shop.owner.actor();
    let room = shop
        .db
        .write(move |tx| {
            floor::save_room(
                tx,
                &actor,
                None,
                RoomInput {
                    code: "S1".into(),
                    name_ar: "القاعة".into(),
                    name_fr: "Salle".into(),
                    name_en: "Dining room".into(),
                    grid_cols: 4,
                    grid_rows: 3,
                    sort_order: 1,
                },
            )
        })
        .unwrap();
    let mut ids = Vec::new();
    for (i, code) in ["T1", "T2"].into_iter().enumerate() {
        let actor = shop.owner.actor();
        let input = TableInput {
            room_id: room.id.clone(),
            code: code.into(),
            seats: 4,
            shape: TableShape::Square,
            grid_col: u8::try_from(i).unwrap(),
            grid_row: 0,
        };
        ids.push(
            shop.db
                .write(move |tx| floor::save_table(tx, &actor, None, input))
                .unwrap()
                .id,
        );
    }
    (shop, ids[0].clone(), ids[1].clone())
}

pub(crate) fn dine_in(shop: &Shop, table: &str, covers: u8) -> tickets::Ticket {
    let ctx = shop.till();
    let input = OpenInput {
        mode: DiningMode::DineIn,
        table_id: Some(table.into()),
        label: None,
        covers,
        customer_id: None,
        delivery_address: None,
        lang: Lang::Fr,
        price_level_id: None,
    };
    shop.db
        .write(move |tx| tickets::open(tx, &ctx, input))
        .unwrap()
}

pub(crate) fn add(
    shop: &Shop,
    ticket: &str,
    product: &str,
    qty: i64,
    seat: Option<u8>,
    modifiers: Vec<String>,
) -> tickets::Ticket {
    let ctx = shop.till();
    let (ticket, input) = (
        ticket.to_owned(),
        AddLineInput {
            cart_line: CartLine::product(product, Qty(qty)),
            seat,
            course: None,
            modifiers,
        },
    );
    shop.db
        .write(move |tx| tickets::add_line(tx, &ctx, &ticket, input))
        .unwrap()
}

#[test]
fn the_module_must_be_on() {
    let shop = shop();
    let ctx = shop.till();
    let input = OpenInput {
        mode: DiningMode::Takeaway,
        table_id: None,
        label: None,
        covers: 0,
        customer_id: None,
        delivery_address: None,
        lang: Lang::Fr,
        price_level_id: None,
    };
    let err = shop
        .db
        .write(move |tx| tickets::open(tx, &ctx, input))
        .unwrap_err();
    assert_eq!(err.code(), "module_disabled");
}

#[test]
fn tables_live_on_free_cells_of_their_room_with_unique_codes() {
    let (shop, _t1, _t2) = restaurant();
    let room = shop.db.read(|c| floor::rooms(c, false)).unwrap()[0]
        .id
        .clone();
    let try_table = |code: &'static str, col: u8, row: u8| {
        let actor = shop.owner.actor();
        let input = TableInput {
            room_id: room.clone(),
            code: code.into(),
            seats: 2,
            shape: TableShape::Round,
            grid_col: col,
            grid_row: row,
        };
        shop.db
            .write(move |tx| floor::save_table(tx, &actor, None, input))
    };
    assert_eq!(
        try_table("T3", 4, 0).unwrap_err().code(),
        "table_cell_invalid",
        "outside the 4 × 3 grid"
    );
    assert_eq!(
        try_table("T3", 0, 0).unwrap_err().code(),
        "table_cell_taken"
    );
    assert_eq!(
        try_table("t1", 3, 2).unwrap_err().code(),
        "table_code_taken"
    );
    assert_eq!(try_table("T3", 3, 2).unwrap().code, "T3");
}

#[test]
fn a_dine_in_ticket_needs_a_place_and_takeaway_tickets_are_numbered_per_day() {
    let (shop, t1, _) = restaurant();
    let ctx = shop.till();
    let nowhere = OpenInput {
        mode: DiningMode::DineIn,
        table_id: None,
        label: None,
        covers: 2,
        customer_id: None,
        delivery_address: None,
        lang: Lang::Fr,
        price_level_id: None,
    };
    let err = shop
        .db
        .write(move |tx| tickets::open(tx, &ctx, nowhere))
        .unwrap_err();
    assert_eq!(err.code(), "ticket_needs_place");
    let at_table = dine_in(&shop, &t1, 2);
    assert_eq!(
        (at_table.order_no, at_table.table_code.as_deref()),
        (None, Some("T1"))
    );
    let mut numbers = Vec::new();
    for _ in 0..2 {
        let ctx = shop.till();
        let input = OpenInput {
            mode: DiningMode::Takeaway,
            table_id: None,
            label: None,
            covers: 0,
            customer_id: None,
            delivery_address: None,
            lang: Lang::Ar,
            price_level_id: None,
        };
        numbers.push(
            shop.db
                .write(move |tx| tickets::open(tx, &ctx, input))
                .unwrap()
                .order_no,
        );
    }
    assert_eq!(numbers, [Some(1), Some(2)]);
}

#[test]
fn lines_are_priced_like_the_till_and_the_total_is_kept_on_the_ticket() {
    let (shop, t1, _) = restaurant();
    let couscous = shop.product("Couscous royal", Money(90_000), &shop.tva9);
    let the = shop.product("Thé à la menthe", Money(8_000), &shop.tva9);
    let ticket = dine_in(&shop, &t1, 2);
    add(&shop, &ticket.id, &couscous.id, 2_000, Some(1), vec![]);
    let ticket = add(&shop, &ticket.id, &the.id, 2_000, None, vec![]);
    assert_eq!(ticket.total_ttc, Money(196_000));
    assert_eq!(
        ticket
            .lines
            .iter()
            .map(|l| l.name.as_str())
            .collect::<Vec<_>>(),
        ["Couscous royal", "Thé à la menthe"]
    );
    let ctx = shop.till();
    let cart = tickets::cart_of(&ticket);
    let priced = shop
        .db
        .read(move |c| crate::sales::cart::price_cart(c, &ctx, &cart))
        .unwrap();
    assert_eq!(priced.totals.total_ttc, ticket.total_ttc);
}

#[test]
fn modifiers_respect_min_and_max_and_priced_options_become_lines() {
    let (shop, t1, _) = restaurant();
    let burger = shop.product("Burger", Money(45_000), &shop.tva9);
    let cheese = shop.product("Supplément fromage", Money(5_000), &shop.tva9);
    let actor = shop.owner.actor();
    let cheese_id = cheese.id.clone();
    let group = shop
        .db
        .write(move |tx| {
            modifiers::save_group(
                tx,
                &actor,
                None,
                ModifierGroupInput {
                    name_ar: "الصلصات".into(),
                    name_fr: "Sauces".into(),
                    name_en: "Sauces".into(),
                    min_select: 1,
                    max_select: 2,
                    sort_order: 1,
                    options: vec![
                        ModifierOptionInput {
                            id: None,
                            name_ar: "هريسة".into(),
                            name_fr: "Harissa".into(),
                            name_en: "Harissa".into(),
                            product_id: None,
                            sort_order: 1,
                        },
                        ModifierOptionInput {
                            id: None,
                            name_ar: "مايونيز".into(),
                            name_fr: "Mayonnaise".into(),
                            name_en: "Mayonnaise".into(),
                            product_id: None,
                            sort_order: 2,
                        },
                        ModifierOptionInput {
                            id: None,
                            name_ar: "جبن".into(),
                            name_fr: "Fromage".into(),
                            name_en: "Cheese".into(),
                            product_id: Some(cheese_id),
                            sort_order: 3,
                        },
                    ],
                },
            )
        })
        .unwrap();
    let (actor, burger_id, gid) = (shop.owner.actor(), burger.id.clone(), group.id.clone());
    shop.db
        .write(move |tx| modifiers::set_product_groups(tx, &actor, &burger_id, &[gid]))
        .unwrap();
    let options: Vec<String> = group.options.iter().map(|o| o.id.clone()).collect();
    let ticket = dine_in(&shop, &t1, 1);
    let ctx = shop.till();
    let (tid, bid) = (ticket.id.clone(), burger.id.clone());
    let none = shop.db.write(move |tx| {
        tickets::add_line(
            tx,
            &ctx,
            &tid,
            AddLineInput {
                cart_line: CartLine::product(&bid, Qty(1_000)),
                seat: None,
                course: None,
                modifiers: vec![],
            },
        )
    });
    assert_eq!(
        none.unwrap_err().code(),
        "modifier_choice_invalid",
        "the sauce is forced"
    );
    let ctx = shop.till();
    let (tid, bid, three) = (ticket.id.clone(), burger.id.clone(), options.clone());
    let too_many = shop.db.write(move |tx| {
        tickets::add_line(
            tx,
            &ctx,
            &tid,
            AddLineInput {
                cart_line: CartLine::product(&bid, Qty(1_000)),
                seat: None,
                course: None,
                modifiers: three,
            },
        )
    });
    assert_eq!(
        too_many.unwrap_err().code(),
        "modifier_choice_invalid",
        "two at most"
    );
    let ticket = add(
        &shop,
        &ticket.id,
        &burger.id,
        1_000,
        None,
        vec![options[0].clone(), options[2].clone()],
    );
    assert_eq!(ticket.lines.len(), 2, "the burger and its cheese");
    assert_eq!(ticket.lines[0].cart_line.note.as_deref(), Some("Harissa"));
    assert_eq!(
        ticket.lines[1].parent_line_id.as_deref(),
        Some(ticket.lines[0].id.as_str())
    );
    assert_eq!(ticket.total_ttc, Money(50_000));
}

#[test]
fn a_line_moved_by_transfer_or_merge_is_never_duplicated() {
    let (shop, t1, t2) = restaurant();
    let pizza = shop.product("Pizza", Money(70_000), &shop.tva9);
    let soda = shop.product("Soda", Money(10_000), &shop.tva9);
    let a = dine_in(&shop, &t1, 2);
    add(&shop, &a.id, &pizza.id, 1_000, None, vec![]);
    let b = dine_in(&shop, &t2, 3);
    add(&shop, &b.id, &soda.id, 3_000, None, vec![]);
    let ctx = shop.till();
    let (aid, t2c) = (a.id.clone(), t2.clone());
    let moved = shop
        .db
        .write(move |tx| tickets::transfer(tx, &ctx, &aid, &t2c))
        .unwrap();
    assert_eq!(moved.table_code.as_deref(), Some("T2"));
    let ctx = shop.till();
    let (from, into) = (a.id.clone(), b.id.clone());
    let merged = shop
        .db
        .write(move |tx| tickets::merge(tx, &ctx, &from, &into))
        .unwrap();
    assert_eq!((merged.covers, merged.total_ttc), (5, Money(100_000)));
    assert_eq!(merged.lines.len(), 2);
    assert_eq!(shop.count("SELECT COUNT(*) FROM dining_lines"), 2);
    let gone = shop.db.read(move |c| tickets::get(c, &a.id)).unwrap();
    assert_eq!((gone.status, gone.lines.len()), (TicketStatus::Void, 0));
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'dining.tickets_merged'"),
        1
    );
}

#[test]
fn the_board_shows_free_and_occupied_tables_with_their_minutes() {
    let (shop, t1, _) = restaurant();
    let tagine = shop.product("Tajine", Money(80_000), &shop.tva9);
    let ticket = dine_in(&shop, &t1, 2);
    add(&shop, &ticket.id, &tagine.id, 1_000, None, vec![]);
    let later = NOON + 25 * 60_000;
    let board = shop.db.read(move |c| floor::board(c, None, later)).unwrap();
    let states: Vec<(String, TableState)> = board
        .iter()
        .map(|s| (s.table.code.clone(), s.state))
        .collect();
    assert_eq!(
        states,
        [
            ("T1".to_owned(), TableState::Occupied),
            ("T2".to_owned(), TableState::Free)
        ]
    );
    assert_eq!(board[0].tickets[0].minutes_open, 25);
    assert_eq!(board[0].tickets[0].total_ttc, Money(80_000));
}

#[test]
fn an_unsent_line_is_removed_and_journaled() {
    let (shop, t1, _) = restaurant();
    let jus = shop.product("Jus d'orange", Money(15_000), &shop.tva9);
    let ticket = dine_in(&shop, &t1, 1);
    let ticket = add(&shop, &ticket.id, &jus.id, 1_000, None, vec![]);
    let ctx = shop.till();
    let line = ticket.lines[0].id.clone();
    let after = shop
        .db
        .write(move |tx| tickets::remove_unsent(tx, &ctx, &line))
        .unwrap();
    assert!(after.lines.is_empty() && after.total_ttc == Money::ZERO);
    assert_eq!(
        shop.count(
            "SELECT COUNT(*) FROM dining_lines WHERE status = 'voided' AND void_reason = 'mistake'"
        ),
        1
    );
}

#[test]
fn the_daily_menu_is_set_per_day() {
    let (shop, _, _) = restaurant();
    let chorba = shop.product("Chorba", Money(25_000), &shop.tva9);
    let (actor, id) = (shop.owner.actor(), chorba.id.clone());
    shop.db
        .write(move |tx| menu::set_day(tx, &actor, "2026-09-28", &[id]))
        .unwrap();
    assert_eq!(
        shop.db.read(|c| menu::of_day(c, "2026-09-28")).unwrap(),
        [chorba.id]
    );
    assert!(
        shop.db
            .read(|c| menu::of_day(c, "2026-09-29"))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn the_restaurant_template_seeds_a_room_and_modifiers_once() {
    let shop = shop();
    let actor = shop.owner.actor();
    assert!(shop.db.write(move |tx| super::seed(tx, &actor)).unwrap() > 0);
    let actor = shop.owner.actor();
    assert_eq!(shop.db.write(move |tx| super::seed(tx, &actor)).unwrap(), 0);
    assert_eq!(shop.count("SELECT COUNT(*) FROM dining_tables"), 8);
    assert_eq!(shop.count("SELECT COUNT(*) FROM modifier_groups"), 3);
}

/// What the schema would refuse with a bare `constraint_violation` is refused on its field first,
/// a busy table stays, and a voided ticket takes no line.
#[test]
fn the_floor_and_tickets_refuse_on_the_field_before_the_schema_does() {
    let (shop, t1, t2) = restaurant();
    let room_input = |code: &str| RoomInput {
        code: code.into(),
        name_ar: String::new(),
        name_fr: "Terrasse".into(),
        name_en: String::new(),
        grid_cols: 2,
        grid_rows: 2,
        sort_order: 2,
    };
    let save_room = |input: RoomInput| {
        let actor = shop.owner.actor();
        shop.db
            .write(move |tx| floor::save_room(tx, &actor, None, input))
    };
    let err = save_room(room_input("TERRASSE-ETE-1")).unwrap_err();
    assert_eq!(
        (err.code(), err.payload().field.as_deref()),
        ("too_long".to_owned(), Some("code"))
    );
    let err = save_room(room_input("S1")).unwrap_err();
    assert_eq!(
        (err.code(), err.payload().field.as_deref()),
        ("code_taken".to_owned(), Some("code"))
    );
    let terrace = save_room(room_input("TER")).unwrap();
    let save_table = |room_id: &str, code: &str, seats: u8| {
        let actor = shop.owner.actor();
        let input = TableInput {
            room_id: room_id.into(),
            code: code.into(),
            seats,
            shape: TableShape::Rect,
            grid_col: 0,
            grid_row: 0,
        };
        shop.db
            .write(move |tx| floor::save_table(tx, &actor, None, input))
    };
    let err = save_table(&terrace.id, "TERRASSE1", 4).unwrap_err();
    assert_eq!(
        (err.code(), err.payload().field.as_deref()),
        ("too_long".to_owned(), Some("code"))
    );
    let err = save_table(&terrace.id, "E1", 0).unwrap_err();
    assert_eq!(
        (err.code(), err.payload().field.as_deref()),
        ("value_invalid".to_owned(), Some("seats"))
    );
    let (actor, id) = (shop.owner.actor(), terrace.id.clone());
    shop.db
        .write(move |tx| floor::archive_room(tx, &actor, &id))
        .unwrap();
    assert_eq!(
        save_table(&terrace.id, "E1", 4).unwrap_err().code(),
        "dining_room_not_found",
        "an archived room takes no new table"
    );
    let ticket = dine_in(&shop, &t1, 2);
    let (actor, id) = (shop.owner.actor(), t1.clone());
    let busy = shop
        .db
        .write(move |tx| floor::archive_table(tx, &actor, &id))
        .unwrap_err();
    assert_eq!(busy.code(), "table_busy");
    let soup = shop.product("Chorba", Money(25_000), &shop.tva9);
    let ctx = shop.till();
    let (tid, pid) = (ticket.id.clone(), soup.id.clone());
    let zero = shop.db.write(move |tx| {
        tickets::add_line(
            tx,
            &ctx,
            &tid,
            AddLineInput {
                cart_line: CartLine::product(&pid, Qty(0)),
                seat: None,
                course: None,
                modifiers: vec![],
            },
        )
    });
    assert_eq!(zero.unwrap_err().code(), "qty_invalid");
    let ctx = shop.till();
    let tid = ticket.id.clone();
    let unknown = shop.db.write(move |tx| {
        tickets::add_line(
            tx,
            &ctx,
            &tid,
            AddLineInput {
                cart_line: CartLine::product("00000000-0000-7000-8000-00000000dead", Qty(1_000)),
                seat: None,
                course: None,
                modifiers: vec![],
            },
        )
    });
    assert_eq!(unknown.unwrap_err().code(), "product_not_found");
    let other = dine_in(&shop, &t2, 1);
    let ctx = shop.till();
    let (from, into) = (ticket.id.clone(), other.id.clone());
    shop.db
        .write(move |tx| tickets::merge(tx, &ctx, &from, &into))
        .unwrap();
    let ctx = shop.till();
    let (tid, pid) = (ticket.id.clone(), soup.id.clone());
    let late = shop.db.write(move |tx| {
        tickets::add_line(
            tx,
            &ctx,
            &tid,
            AddLineInput {
                cart_line: CartLine::product(&pid, Qty(1_000)),
                seat: None,
                course: None,
                modifiers: vec![],
            },
        )
    });
    assert_eq!(late.unwrap_err().code(), "ticket_not_open");
}

/// RST-003: the counter's predefined tickets are at most 30 labels of 1 to 40 characters.
#[test]
fn predefined_tickets_are_short_labels() {
    let shop = shop();
    assert_eq!(
        shop.db
            .read(|c| crate::settings::get_value(c, "restaurant.predefined_tickets"))
            .unwrap(),
        serde_json::json!(["Comptoir", "Terrasse"])
    );
    let set = |value: serde_json::Value| {
        let actor = shop.owner.actor();
        shop.db.write(move |tx| {
            crate::settings::set_value(tx, &actor, "restaurant.predefined_tickets", value)
        })
    };
    assert!(set(serde_json::json!(["Comptoir", "Étage", "الطابق"])).is_ok());
    for bad in [
        serde_json::json!(["x".repeat(41)]),
        serde_json::json!(["  "]),
        serde_json::json!([1]),
        serde_json::json!(vec!["T"; 31]),
        serde_json::json!("Comptoir"),
    ] {
        assert_eq!(
            set(bad.clone()).unwrap_err().code(),
            "value_invalid",
            "{bad}"
        );
    }
}

/// A forced group needs as many options as its minimum, and a product lists a group once.
#[test]
fn a_modifier_group_can_always_be_satisfied_and_is_attached_once() {
    let (shop, _, _) = restaurant();
    let burger = shop.product("Burger", Money(45_000), &shop.tva9);
    let actor = shop.owner.actor();
    let impossible = shop.db.write(move |tx| {
        modifiers::save_group(
            tx,
            &actor,
            None,
            ModifierGroupInput {
                name_ar: String::new(),
                name_fr: "Pain".into(),
                name_en: String::new(),
                min_select: 2,
                max_select: 0,
                sort_order: 1,
                options: vec![ModifierOptionInput {
                    id: None,
                    name_ar: String::new(),
                    name_fr: "Complet".into(),
                    name_en: String::new(),
                    product_id: None,
                    sort_order: 1,
                }],
            },
        )
    });
    let err = impossible.unwrap_err();
    assert_eq!(
        (err.code(), err.payload().field.as_deref()),
        ("value_invalid".to_owned(), Some("minSelect"))
    );
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| modifiers::seed(tx, &actor))
        .unwrap();
    let group = shop
        .db
        .read(|c| modifiers::groups(c, false))
        .unwrap()
        .remove(0);
    let (actor, bid, gid) = (shop.owner.actor(), burger.id.clone(), group.id.clone());
    let twice = shop
        .db
        .write(move |tx| modifiers::set_product_groups(tx, &actor, &bid, &[gid.clone(), gid]));
    assert_eq!(twice.unwrap_err().code(), "value_invalid");
}

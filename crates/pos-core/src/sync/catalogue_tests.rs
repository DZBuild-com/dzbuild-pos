//! Who owns a row whose two parents were made on different PCs, and the `users` columns a till
//! writes and keeps (M5 Task 2 review). The catalogue's own tests are in `catalogue.rs`.

use super::catalogue::*;
use crate::audit::Actor;
use crate::auth::session;
use crate::db::meta_set;
use crate::error::HOST_REQUIRED_RAISE;
use crate::lang::Lang;
use crate::ledger::{self, EntryKind, NewEntry, Side};
use crate::money::Money;
use crate::terminal::META_TERMINAL_ID;
use crate::testkit::{NOON, Shop, shop};
use serde_json::{Map, Value};

const C2: &str = "00000000-0000-7000-8000-0000000000c2";
const C3: &str = "00000000-0000-7000-8000-0000000000c3";

/// The shop's own terminal becomes the main PC; C2 and C3 are its tills.
fn lan_shop() -> (Shop, String) {
    let shop = shop();
    let main = shop.terminal.id.clone();
    let id = main.clone();
    shop.db
        .write(move |tx| {
            tx.execute("UPDATE terminals SET role = 'host' WHERE id = ?1", [&id])?;
            tx.execute_batch(
                "INSERT INTO terminals (id, code, name, role, created_at) VALUES
                   ('00000000-0000-7000-8000-0000000000c2', 'C2', 'Caisse 2', 'client', 0),
                   ('00000000-0000-7000-8000-0000000000c3', 'C3', 'Caisse 3', 'client', 0);",
            )?;
            Ok(())
        })
        .unwrap();
    (shop, main)
}

/// A store credit (an avoir refunded to the account) recorded on `credit_on`, then a kridi sale
/// on `debit_on` that the ledger settles against it: the image of that allocation.
fn allocation(shop: &Shop, name: &str, credit_on: &str, debit_on: &str) -> Map<String, Value> {
    let customer = shop.customer(name).id;
    let user = shop.owner.user_id.clone();
    let (credit_on, debit_on) = (credit_on.to_owned(), debit_on.to_owned());
    let image = shop
        .db
        .write(move |tx| {
            let on = |terminal: &str| Actor {
                user_id: Some(user.clone()),
                terminal_id: Some(terminal.to_owned()),
            };
            let credit = NewEntry::new(&customer, EntryKind::Refund, Money(-5_000));
            let credit = ledger::append(tx, Side::Customer, &on(&credit_on), &credit, "2026-09-28", NOON)?;
            let debit = NewEntry::new(&customer, EntryKind::SaleCredit, Money(3_000));
            let debit = ledger::append(tx, Side::Customer, &on(&debit_on), &debit, "2026-09-28", NOON + 1)?;
            let image: String = tx.query_row(
                "SELECT json_object('id', id, 'customer_id', customer_id, 'debit_id', debit_id,
                                    'credit_id', credit_id, 'amount', amount, 'created_at', created_at)
                 FROM customer_allocations WHERE debit_id = ?1 AND credit_id = ?2",
                [&debit.id, &credit.id],
                |r| r.get(0),
            )?;
            Ok(image)
        })
        .unwrap();
    match serde_json::from_str(&image).unwrap() {
        Value::Object(map) => map,
        other => panic!("not an object: {other}"),
    }
}

/// On the PC whose terminal is `local`: the `_t` its capture trigger writes for the allocation
/// (`Owner::sql`), `owner_of_image`, and whether its till guard refuses the row (`guard_sql`).
fn seen_from(
    shop: &Shop,
    local: &str,
    image: &Map<String, Value>,
) -> (Option<String>, Option<String>, bool) {
    let spec = spec("customer_allocations").unwrap();
    let captured = spec.owner.sql("a").unwrap();
    let refused = spec.owner.guard_sql("a", local).unwrap();
    let (local, image) = (local.to_owned(), image.clone());
    shop.db
        .write(move |tx| {
            meta_set(tx, META_TERMINAL_ID, &local)?;
            let id = image["id"].as_str().unwrap_or_default().to_owned();
            let (captured, refused): (Option<String>, bool) = tx.query_row(
                &format!(
                    "SELECT {captured}, {refused} FROM customer_allocations AS a WHERE a.id = ?1"
                ),
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            let owner = owner_of_image(tx, spec, &image)?;
            Ok((captured, owner, refused))
        })
        .unwrap()
}

#[test]
fn the_till_that_spends_a_main_pc_credit_owns_the_allocation() {
    let (shop, main) = lan_shop();
    // Store credit from an avoir made on the main PC, spent on kridi at till C2.
    let image = allocation(&shop, "Karim", &main, C2);
    let c2 = Some(C2.to_owned());
    assert_eq!(
        seen_from(&shop, C2, &image),
        (c2.clone(), c2, false),
        "C2 made it: its capture names C2, so its outbox sends it"
    );
    let main_owner = Some(main.clone());
    assert_eq!(
        seen_from(&shop, &main, &image),
        (main_owner.clone(), main_owner, false),
        "on the main PC it names the main PC, so every till receives it"
    );
    assert_eq!(
        seen_from(&shop, C3, &image),
        (None, None, true),
        "another till neither owns nor writes it"
    );
}

#[test]
fn an_allocation_between_two_tills_reaches_both() {
    let (shop, main) = lan_shop();
    // A versement taken at C3, then a kridi sale at C2 settled against it.
    let image = allocation(&shop, "Nadia", C3, C2);
    assert_eq!(seen_from(&shop, C2, &image).0.as_deref(), Some(C2));
    assert_eq!(seen_from(&shop, C3, &image).0.as_deref(), Some(C3));
    assert_eq!(
        seen_from(&shop, &main, &image),
        (None, None, true),
        "the main PC cannot tell which till made it: no till skips it in the feed"
    );
    let image = allocation(&shop, "Samir", C2, C2);
    assert_eq!(
        seen_from(&shop, &main, &image),
        (Some(C2.to_owned()), Some(C2.to_owned()), true),
        "both entries from C2: C2's"
    );
}

#[test]
fn the_main_pc_accepts_an_allocation_from_the_till_on_either_side() {
    let (shop, main) = lan_shop();
    let owners = |image: Map<String, Value>| {
        shop.db
            .read(move |c| owners_of_image(c, spec("customer_allocations").unwrap(), &image))
            .unwrap()
    };
    let image = allocation(&shop, "Karim", &main, C2);
    let found = owners(image);
    assert_eq!(
        found,
        [main.clone(), C2.to_owned()],
        "credit side, then debit side"
    );
    assert!(
        found.iter().any(|o| o == C2),
        "the replay from C2 is accepted, as C2's guard let it write the row"
    );
    assert!(!found.iter().any(|o| o == C3), "a replay from C3 is not");
    assert_eq!(owners(allocation(&shop, "Nadia", C3, C2)), [C3, C2]);
    assert_eq!(
        owners(allocation(&shop, "Samir", C2, C2)),
        [C2],
        "one till, named once"
    );

    let single = |table: &'static str, image: Value| {
        shop.db
            .read(move |c| {
                let Value::Object(image) = image else {
                    unreachable!()
                };
                owners_of_image(c, spec(table).unwrap(), &image)
            })
            .unwrap()
    };
    assert_eq!(
        single("customer_ledger", serde_json::json!({ "terminal_id": C2 })),
        [C2]
    );
    assert_eq!(
        single("counters", serde_json::json!({ "series": "C3" })),
        [C3]
    );
    assert!(
        single("products", serde_json::json!({ "id": "p" })).is_empty(),
        "host data"
    );
}

#[test]
fn a_till_writes_and_keeps_its_users_lockout_and_language() {
    let users = spec("users").unwrap();
    assert_eq!(
        users.client_writable,
        [
            "failed_attempts",
            "locked_until",
            "last_login_at",
            "lang",
            "updated_at"
        ]
    );
    assert_eq!(
        users.local_columns,
        ["failed_attempts", "locked_until", "last_login_at", "lang"],
        "a till's language choice survives the next image of the user from the main PC"
    );
}

#[test]
fn a_cashier_switches_language_under_the_till_guard() {
    let shop = shop();
    let users = spec("users").unwrap();
    // The till's guard on `users` as Task 4 generates it: an update of a column outside
    // `client_writable` needs the main PC.
    shop.db
        .write(move |tx| {
            let mut stmt = tx.prepare("SELECT name FROM pragma_table_info('users')")?;
            let columns = stmt
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            drop(stmt);
            let guarded = columns
                .iter()
                .filter(|c| !users.client_writable.contains(&c.as_str()))
                .map(|c| quote(c))
                .collect::<Vec<_>>()
                .join(", ");
            tx.execute_batch(&format!(
                "CREATE TRIGGER till_guard BEFORE UPDATE OF {guarded} ON users
                 BEGIN SELECT RAISE(ABORT, '{HOST_REQUIRED_RAISE}'); END"
            ))?;
            Ok(())
        })
        .unwrap();
    let session = session::change_language(&shop.db, &shop.cashier, Lang::Ar).unwrap();
    assert_eq!(
        session.lang,
        Lang::Ar,
        "the AR/FR/EN toggle works on a till"
    );
    let refused = shop
        .db
        .write(|tx| Ok(tx.execute_batch("UPDATE users SET display_name = 'X'")?))
        .unwrap_err();
    assert_eq!(refused.code(), "host_required", "the guard is live");
}

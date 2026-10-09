//! Joining a main PC on a PC already set up replaces its books with the main PC's copy at the
//! next start, the same swap as M0's restore: the owner confirms it with their PIN (M5 design,
//! plan line 56) before anything leaves this PC, so a wrong PIN never spends the pairing code.
//! A new PC (setup not completed) joins without a session or a PIN. The address is empty in
//! every case: `pos_server` refuses it before any network call, so these tests never touch the
//! LAN, and reaching that refusal proves the guard let the join through.

use super::lan::{self, JoinInput};
use super::testshop::{OWNER_PIN, shop};
use crate::state::AppState;
use std::sync::Arc;

fn input(owner_pin: Option<&str>) -> JoinInput {
    JoinInput {
        address: String::new(),
        code: "042917".into(),
        terminal_code: "C2".into(),
        terminal_name: "Caisse C2".into(),
        owner_pin: owner_pin.map(str::to_owned),
    }
}

fn join_code(st: &AppState, owner_pin: Option<&str>) -> String {
    lan::join(st, input(owner_pin), Arc::new(|_: u64, _: Option<u64>| {}))
        .unwrap_err()
        .code()
}

#[test]
fn a_set_up_pc_joins_only_with_the_owners_pin_checked_before_the_network() {
    let shop = shop();
    let missing = lan::join(&shop.st, input(None), Arc::new(|_: u64, _: Option<u64>| {}))
        .unwrap_err()
        .payload();
    assert_eq!(missing.code, "required");
    assert_eq!(missing.field.as_deref(), Some("ownerPin"));
    assert_eq!(
        join_code(&shop.st, Some("0000")),
        "invalid_pin",
        "a wrong PIN stops the join before the main PC sees the code"
    );
    assert_eq!(
        join_code(&shop.st, Some(OWNER_PIN)),
        "lan_address_invalid",
        "the owner's PIN lets the join go on to the main PC"
    );
}

#[test]
fn a_wrong_pin_counts_toward_the_owners_lockout() {
    let shop = shop();
    assert_eq!(join_code(&shop.st, Some("0000")), "invalid_pin");
    let owner = shop.owner.user_id.clone();
    let failed: i64 = shop
        .st
        .db
        .read(move |c| {
            Ok(c.query_row(
                "SELECT failed_attempts FROM users WHERE id = ?1",
                [owner],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(failed, 1);
}

#[test]
fn a_new_pc_joins_without_a_session_or_a_pin() {
    let st = AppState::for_tests();
    assert_eq!(join_code(&st, None), "lan_address_invalid");
}

#[test]
fn only_the_owner_joins_a_set_up_pc_whatever_pin_is_typed() {
    let shop = shop();
    shop.cashier();
    assert_eq!(
        join_code(&shop.st, Some(OWNER_PIN)),
        "insufficient_permission"
    );
    crate::commands::auth::logout(&shop.st).unwrap();
    assert_eq!(join_code(&shop.st, Some(OWNER_PIN)), "session_required");
}

/// A main PC holding paired tills' books never joins another main PC: the copy installed at the
/// next start would archive their tickets, Z and journals and orphan them. Refused at the
/// wizard's first step (discovery, the address check) and before the PIN, like
/// `become_standalone`.
#[test]
fn a_main_pc_with_paired_tills_does_not_join_another_one() {
    let shop = shop();
    lan::become_host(&shop.st, &lan::quiet()).unwrap();
    shop.st
        .db
        .write(|tx| {
            tx.execute_batch(
                "INSERT INTO terminals (id, code, name, role, created_at)
                   VALUES ('00000000-0000-7000-8000-0000000000c2', 'C2', 'Caisse 2', 'client', 0);
                 INSERT INTO lan_devices (terminal_id, name, token_hash, paired_at)
                   VALUES ('00000000-0000-7000-8000-0000000000c2', 'Caisse 2',
                           'dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd', 0);",
            )?;
            Ok(())
        })
        .unwrap();
    let refused = lan::may_join(&shop.st).unwrap_err().payload();
    assert_eq!(refused.code, "lan_tills_paired");
    assert_eq!(refused.details.unwrap()["tills"], 1);
    assert_eq!(
        join_code(&shop.st, Some("0000")),
        "lan_tills_paired",
        "refused before the PIN is even checked"
    );
    shop.st.lan.stop();
}

/// A PC linked to DZBuild unlinks first (Settings › DZBuild): joining would drop the link
/// without revoking its token or removing its device, and archive its unsent uploads.
#[test]
fn a_pc_linked_to_dzbuild_unlinks_before_it_joins() {
    let shop = shop();
    shop.st
        .db
        .write(|tx| {
            pos_core::cloud::store::insert_link(
                tx,
                &pos_core::cloud::store::NewLink {
                    id: pos_core::ids::new_id(),
                    account_sub: "sub-1".into(),
                    account_name: None,
                    store_id: "store-1".into(),
                    store_name: "El Amel en ligne".into(),
                    store_language: pos_core::lang::Lang::Fr,
                    store_plan: None,
                    public_url: None,
                    location_id: None,
                    device_id: None,
                    scopes: Vec::new(),
                    stock_deduction: None,
                    active_products_cap: None,
                },
                0,
            )
            .map(drop)
        })
        .unwrap();
    let refused = lan::may_join(&shop.st).unwrap_err().payload();
    assert_eq!(refused.code, "cloud_unlink_first");
    let details = refused.details.unwrap();
    assert_eq!(details["store"], "El Amel en ligne");
    assert_eq!(details["pending"], 0);
    assert_eq!(
        join_code(&shop.st, Some(OWNER_PIN)),
        "cloud_unlink_first",
        "refused before the code is spent"
    );
}

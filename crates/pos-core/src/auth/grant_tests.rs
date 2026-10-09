//! A manager's approval a till's user brings to a host service (Task 20 review, finding 2): the
//! main PC takes it only from an active user of its own, other than the requester, whose role
//! holds the permission there, and rebuilds it from its own database.

use super::session::confirm_grant;
use super::users::{self, NewUser};
use super::{CASHIER_ROLE_ID, MANAGER_ROLE_ID, Permission};
use crate::audit::Actor;
use crate::lang::Lang;
use crate::testkit::{FAKE_PIN_HASH, NOON, Shop, shop};

fn user(shop: &Shop, name: &str, role: &str, limit: i32) -> String {
    let input = NewUser {
        username: None,
        display_name: name.into(),
        pin: String::new(),
        role_id: role.into(),
        lang: Lang::Fr,
        discount_limit_bp: limit,
    };
    shop.db
        .write(move |tx| users::create(tx, &Actor::system(None), &input, FAKE_PIN_HASH))
        .unwrap()
        .id
}

fn confirm(shop: &Shop, approver: &str, permission: Permission) -> Option<super::OverrideGrant> {
    let (requester, approver) = (shop.cashier.user_id.clone(), approver.to_owned());
    shop.db
        .read(move |c| confirm_grant(c, &requester, &approver, permission, NOON - 30_000))
        .unwrap()
}

#[test]
fn a_tills_approval_counts_on_the_main_pc_only_from_its_own_manager() {
    let shop = shop();
    let karim = user(&shop, "Karim", MANAGER_ROLE_ID, 2_000);
    let grant = confirm(&shop, &karim, Permission::SalesReturn).unwrap();
    assert_eq!(
        (
            grant.approver_id.as_str(),
            grant.approver_name.as_str(),
            grant.permission,
            grant.granted_at,
            grant.discount_limit_bp
        ),
        (
            karim.as_str(),
            "Karim",
            Permission::SalesReturn,
            NOON - 30_000,
            2_000
        )
    );
    let owner = shop.owner.user_id.clone();
    assert_eq!(
        confirm(&shop, &owner, Permission::SalesDiscount).map(|g| g.discount_limit_bp),
        Some(10_000),
        "an owner approves any discount"
    );
    let cashier = shop.cashier.user_id.clone();
    assert_eq!(
        confirm(&shop, &cashier, Permission::SalesReturn),
        None,
        "nobody approves their own request"
    );
    let other = user(&shop, "Sofiane", CASHIER_ROLE_ID, 0);
    assert_eq!(
        confirm(&shop, &other, Permission::SalesReturn),
        None,
        "a cashier cannot approve a return"
    );
    assert_eq!(
        confirm(
            &shop,
            "00000000-0000-7000-8000-00000000beef",
            Permission::SalesReturn
        ),
        None,
        "unknown here"
    );
    let gone = karim.clone();
    shop.db
        .write(move |tx| users::deactivate(tx, &Actor::system(None), &gone))
        .unwrap();
    assert_eq!(
        confirm(&shop, &karim, Permission::SalesReturn),
        None,
        "a manager deactivated on the main PC approves nothing"
    );
}

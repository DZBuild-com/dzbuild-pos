//! Till plumbing: approvals kept in Rust, the operator context, the catalogue at start.

use super::auth::{self, OverrideInput};
use super::testshop::{OWNER_PIN, shop};
use crate::approvals::APPROVAL_TTL_MS;
use crate::state::{AppPaths, AppState};
use pos_core::auth::{OverrideGrant, Permission};
use pos_core::money::{Money, Qty, Rate};
use pos_core::time::now_ms;

#[test]
fn approvals_are_server_side_single_use_and_expire() {
    let shop = shop();
    let cashier = shop.cashier();
    assert_eq!(
        shop.st.till(Permission::SalesDiscount).unwrap_err().code(),
        "insufficient_permission"
    );
    let grant = auth::manager_override(
        &shop.st,
        &OverrideInput {
            approver_id: shop.owner.user_id.clone(),
            pin: OWNER_PIN.into(),
            permission: Permission::SalesDiscount,
        },
    )
    .unwrap();
    assert_eq!(
        grant.approver_name, "Amine Benali",
        "returned for display only"
    );
    let ctx = shop.st.till(Permission::SalesDiscount).unwrap();
    assert!(ctx.allows(Permission::SalesDiscount));
    assert_eq!(ctx.approvals.len(), 1);
    assert_eq!(ctx.session.user_id, cashier.user_id);
    let again = shop.st.till(Permission::SalesCreate).unwrap();
    assert_eq!(again.approvals.len(), 1, "a read does not spend it");
    shop.st.spend_approvals(&ctx).unwrap();
    assert_eq!(
        shop.st.till(Permission::SalesDiscount).unwrap_err().code(),
        "insufficient_permission",
        "one approval, one operation"
    );
    let old = OverrideGrant {
        approver_id: shop.owner.user_id.clone(),
        approver_name: "Amine Benali".into(),
        permission: Permission::SalesDiscount,
        granted_at: now_ms() - APPROVAL_TTL_MS - 1,
        discount_limit_bp: 10_000,
    };
    shop.st.approvals.add(&cashier, old).unwrap();
    assert!(shop.st.till(Permission::SalesDiscount).is_err(), "expired");
}

#[test]
fn a_logout_drops_pending_approvals() {
    let shop = shop();
    shop.cashier();
    auth::manager_override(
        &shop.st,
        &OverrideInput {
            approver_id: shop.owner.user_id.clone(),
            pin: OWNER_PIN.into(),
            permission: Permission::SalesVoidLine,
        },
    )
    .unwrap();
    auth::logout(&shop.st).unwrap();
    let cashier = shop.cashier();
    assert!(
        shop.st
            .approvals
            .pending(&cashier, now_ms())
            .unwrap()
            .is_empty()
    );
    assert!(shop.st.till(Permission::SalesVoidLine).is_err());
}

#[test]
fn another_sign_in_cannot_use_the_grant() {
    let shop = shop();
    let first = shop.cashier();
    auth::manager_override(
        &shop.st,
        &OverrideInput {
            approver_id: shop.owner.user_id.clone(),
            pin: OWNER_PIN.into(),
            permission: Permission::SalesReturn,
        },
    )
    .unwrap();
    // the same PC, a new sign-in (the login clears every pending grant anyway)
    let manager = shop.manager();
    assert_ne!(manager.user_id, first.user_id);
    assert!(
        shop.st
            .approvals
            .pending(&first, now_ms())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn owners_need_no_approval_and_require_any_accepts_any_listed_permission() {
    let shop = shop();
    let ctx = shop.st.till(Permission::SalesPriceOverride).unwrap();
    assert!(ctx.approvals.is_empty());
    shop.cashier();
    shop.st
        .require_any(&[Permission::SettingsData, Permission::SalesCreate])
        .unwrap();
    assert_eq!(
        shop.st
            .require_any(&[Permission::SettingsData, Permission::AuditView])
            .unwrap_err()
            .code(),
        "insufficient_permission"
    );
}

#[test]
fn the_catalogue_defaults_are_seeded_at_every_start() {
    let shop = shop();
    let rates = shop.st.db.read(pos_core::catalog::tax_rates::list).unwrap();
    assert!(
        rates.iter().any(|r| r.rate_bp.0 == 1_900),
        "VAT rates come from the data pack"
    );
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path());
    drop(AppState::open(&paths, crate::printing::silent()).unwrap());
    let state = AppState::open(&paths, crate::printing::silent()).unwrap();
    let again = state.db.read(pos_core::catalog::tax_rates::list).unwrap();
    assert_eq!(again.len(), rates.len(), "idempotent");
}

/// The shared test shop's helpers go through the M1a services (later till command tests
/// build on them): a 9 % piece product with its barcode, opening stock, the owner's sign-in.
#[test]
fn the_test_shop_stocks_products_through_the_services() {
    let shop = shop();
    shop.cashier();
    assert!(shop.owner().is_owner, "the owner signs back in");
    let milk = shop.product("Lait 1 L", 12_000, 900, Some("6130000000017"));
    assert_eq!(
        (milk.sell_price, milk.cost_price, milk.tax_rate_bp),
        (Money(12_000), Money(7_200), Rate(900))
    );
    assert_eq!(milk.barcodes[0].barcode, "6130000000017");
    shop.stock_in(&milk.id, 5);
    let id = milk.id.clone();
    let on_hand = shop
        .st
        .db
        .read(move |c| pos_core::stock::on_hand(c, &id))
        .unwrap();
    assert_eq!(on_hand, Qty(5_000));
}

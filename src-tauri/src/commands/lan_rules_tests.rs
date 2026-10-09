//! The till's rules in the commands: kridi needs the main PC, the Z waits for the outbox or is
//! closed offline with an approval, the clock is checked at session open, restores are blocked,
//! routed edits go to the main PC. A till here is an `AppState` whose database was made a till
//! by hand (no network): `Lan::test_link` sets its link.

use super::testshop::{OWNER_PIN, TestShop, shop};
use super::{auth, cash, sales as sales_cmd};
use pos_core::cash::CloseInput;
use pos_core::money::Money;
use pos_core::payments::{PaymentInput, PaymentMethod};
use pos_core::sales::{Cart, CartLine, SaleInput};

/// Makes the test shop's database a till of a (fictive) main PC, with its triggers and hook.
pub(super) fn as_till(shop: &TestShop) {
    let st = &shop.st;
    st.db
        .write(|tx| {
            tx.execute(
                "UPDATE lan_local SET role = 'client', host_id = ?1, host_address = '127.0.0.1:9', host_name = 'Principal', paired_at = 0",
                [pos_core::ids::new_id()],
            )?;
            pos_core::sync::changelog::register(tx, pos_core::sync::changelog::LAN_OUTBOX_CONSUMER, 0)?;
            Ok(())
        })
        .unwrap();
    let id = st.terminal.id.clone();
    st.db
        .write(move |tx| {
            pos_core::sync::capture::install(
                tx,
                &pos_core::sync::capture::CaptureSet::Client { terminal_id: id },
            )
            .map(drop)
        })
        .unwrap();
    let role = st.db.read(pos_core::sync::role::current).unwrap();
    st.db.set_write_hook(st.lan.hook(&role, &st.terminal.id));
    st.lan.set_role(pos_core::terminal::TerminalRole::Client);
}

pub(super) fn link(shop: &TestShop, online: bool, skew: Option<i64>) {
    *shop.st.lan.test_link.lock().unwrap() = Some((online, skew));
}

fn credit_sale(
    shop: &TestShop,
    product: &str,
    customer: &str,
) -> Result<sales_cmd::SaleDone, pos_core::error::AppError> {
    let mut cart = Cart::new(
        pos_core::lang::Lang::Fr,
        vec![CartLine::product(product, pos_core::money::Qty(1_000))],
    );
    cart.customer_id = Some(customer.into());
    sales_cmd::create(
        &shop.st,
        sales_cmd::SaleRequest {
            sale: SaleInput {
                cart,
                payments: vec![PaymentInput {
                    method: PaymentMethod::Credit,
                    ..PaymentInput::cash(Money(10_000))
                }],
            },
            print_receipt: false,
            extras: Default::default(),
        },
    )
}

#[test]
fn kridi_on_an_offline_till_is_refused_and_cash_sales_are_not() {
    let shop = shop();
    let milk = shop.product("Lait", 10_000, 900, None).id;
    shop.stock_in(&milk, 10);
    let karim = super::customers::create(
        &shop.st,
        pos_core::customers::CustomerInput {
            credit_limit: Some(Money(1_000_000)),
            ..pos_core::customers::CustomerInput::named("Karim")
        },
    )
    .unwrap()
    .id;
    cash::open_session(&shop.st, Money(0)).unwrap();
    as_till(&shop);
    link(&shop, false, None);
    assert_eq!(
        credit_sale(&shop, &milk, &karim).unwrap_err().code(),
        "kridi_needs_host"
    );
    shop.sell(&milk, 1, 10_500); // cash: fine offline
    link(&shop, true, Some(0));
    credit_sale(&shop, &milk, &karim).unwrap();
}

#[test]
fn the_z_waits_for_the_outbox_or_is_closed_offline_with_an_approval() {
    let shop = shop();
    let milk = shop.product("Lait", 10_000, 900, None).id;
    shop.stock_in(&milk, 10);
    let cashier = shop.user(
        "Yacine",
        pos_core::auth::CASHIER_ROLE_ID,
        super::testshop::CASHIER_PIN,
        1_000,
    );
    as_till(&shop);
    link(&shop, false, None);
    shop.sign_in(&cashier, super::testshop::CASHIER_PIN); // an owner may close offline alone
    cash::open_session(&shop.st, Money(0)).unwrap();
    shop.sell(&milk, 1, 10_500);
    let counts = || CloseInput {
        counts: vec![pos_core::cash::closing::CountLine {
            denomination: Money(10_500),
            count: 1,
        }],
        notes: None,
    };
    let refused = cash::close_session(&shop.st, counts()).unwrap_err();
    assert_eq!(refused.code(), "outbox_pending_at_close");
    assert!(
        refused.payload().details.unwrap()["pending"]
            .as_i64()
            .unwrap()
            >= 2
    );
    assert_eq!(
        cash::close_offline(&shop.st, counts()).unwrap_err().code(),
        "offline_z_needs_approval"
    );
    auth::manager_override(
        &shop.st,
        &auth::OverrideInput {
            approver_id: shop.owner.user_id.clone(),
            pin: OWNER_PIN.into(),
            permission: pos_core::auth::Permission::CashViewExpected,
        },
    )
    .unwrap();
    let closed = cash::close_offline(&shop.st, counts()).unwrap();
    assert!(closed.z.offline_pending.unwrap() >= 2);
}

/// CSH-005 on an offline Z: an approval given for the offline Z before the count never accepts
/// a variance above the threshold. The count is journaled as refused and the recount is asked
/// for; a manager approving after that refused count closes it.
#[test]
fn an_offline_z_with_a_variance_needs_its_own_approval_after_the_recount() {
    let shop = shop();
    let milk = shop.product("Lait", 10_000, 900, None).id;
    shop.stock_in(&milk, 10);
    let cashier = shop.user(
        "Yacine",
        pos_core::auth::CASHIER_ROLE_ID,
        super::testshop::CASHIER_PIN,
        1_000,
    );
    as_till(&shop);
    link(&shop, false, None);
    shop.sign_in(&cashier, super::testshop::CASHIER_PIN);
    cash::open_session(&shop.st, Money(0)).unwrap();
    shop.sell(&milk, 6, 70_000);
    let approve = || {
        auth::manager_override(
            &shop.st,
            &auth::OverrideInput {
                approver_id: shop.owner.user_id.clone(),
                pin: OWNER_PIN.into(),
                permission: pos_core::auth::Permission::CashViewExpected,
            },
        )
        .unwrap();
    };
    let refused_counts = || {
        shop.st
            .db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT COUNT(*) FROM audit_log WHERE action = 'cash.close_refused'",
                    [],
                    |r| r.get::<_, i64>(0),
                )?)
            })
            .unwrap()
    };
    // The drawer is empty: the day's takings short, far above the 500 DA threshold.
    approve(); // for the offline Z only
    assert_eq!(
        cash::close_offline(&shop.st, CloseInput::default())
            .unwrap_err()
            .code(),
        "variance_needs_approval"
    );
    assert_eq!(refused_counts(), 1, "the refused count is on record");
    assert_eq!(
        cash::close_offline(&shop.st, CloseInput::default())
            .unwrap_err()
            .code(),
        "variance_needs_approval",
        "the approval given before the count still does not cover it"
    );
    assert_eq!(refused_counts(), 2, "the recount too");
    approve(); // after the recount, with the variance shown
    let closed = cash::close_offline(&shop.st, CloseInput::default()).unwrap();
    assert!(closed.z.offline_pending.unwrap() >= 1);
    assert!(closed.z.variance.0 < -50_000, "{:?}", closed.z.variance);
}

#[test]
fn a_session_does_not_open_online_with_a_clock_off_by_more_than_15_minutes() {
    let shop = shop();
    as_till(&shop);
    link(&shop, true, Some(20 * 60_000));
    assert_eq!(
        cash::open_session(&shop.st, Money(0)).unwrap_err().code(),
        "lan_clock_skew"
    );
    link(&shop, false, Some(20 * 60_000));
    cash::open_session(&shop.st, Money(0)).unwrap();
}

#[test]
fn restores_are_blocked_on_a_till() {
    let shop = shop();
    shop.st.db.backup(pos_core::db::BackupKind::Manual).ok(); // memory: no file, the check comes first
    as_till(&shop);
    let err = super::backup::restore(&shop.st, "any.db", OWNER_PIN).unwrap_err();
    assert_eq!(err.code(), "lan_restore_blocked");
}

#[test]
fn routed_edits_need_the_main_pc_on_a_till() {
    let shop = shop();
    let milk = shop.product("Lait", 10_000, 900, None).id;
    as_till(&shop);
    link(&shop, false, None);
    let err = super::products::set_price(
        &shop.st,
        milk.clone(),
        pos_core::catalog::RETAIL_LEVEL_ID.into(),
        Money(12_000),
    )
    .unwrap_err();
    assert_eq!(err.code(), "host_required");
}

/// What a routed service answers on the main PC comes back to the till as JSON: every routed
/// result type reads back what it wrote.
#[test]
fn routed_results_read_back_what_the_main_pc_wrote() {
    let shop = shop();
    let milk = shop.product("Lait", 10_000, 900, Some("6130000000017"));
    shop.stock_in(&milk.id, 5);
    let back: pos_core::catalog::products::Product =
        serde_json::from_value(serde_json::to_value(&milk).unwrap()).unwrap();
    assert_eq!(back, milk);
    let query = pos_core::catalog::search::SearchQuery {
        text: "lait".into(),
        ..Default::default()
    };
    let hits = super::products::search(&shop.st, query.clone()).unwrap();
    let hits_back: Vec<pos_core::catalog::search::ProductHit> =
        serde_json::from_value(serde_json::to_value(&hits).unwrap()).unwrap();
    assert_eq!(hits_back, hits);
    let query_back: pos_core::catalog::search::SearchQuery =
        serde_json::from_value(serde_json::to_value(&query).unwrap()).unwrap();
    assert_eq!(query_back, query);
    let karim =
        super::customers::create(&shop.st, pos_core::customers::CustomerInput::named("Karim"))
            .unwrap();
    let karim_back: pos_core::customers::Customer =
        serde_json::from_value(serde_json::to_value(&karim).unwrap()).unwrap();
    assert_eq!(karim_back, karim);
    shop.open_till(0);
    let sold = shop.sell(&milk.id, 1, 10_500);
    let doc = sold.document.clone();
    let doc_back: pos_core::documents::Document =
        serde_json::from_value(serde_json::to_value(&doc).unwrap()).unwrap();
    assert_eq!(doc_back, doc);
}

/// A versement and a refund to the customer's account move a balance that lives on the main
/// PC: an offline till refuses them like a kridi tender.
#[test]
fn versements_and_refunds_to_the_account_need_the_main_pc_on_a_till() {
    let shop = shop();
    let milk = shop.product("Lait", 10_000, 900, None).id;
    shop.stock_in(&milk, 10);
    let karim =
        super::customers::create(&shop.st, pos_core::customers::CustomerInput::named("Karim"))
            .unwrap()
            .id;
    cash::open_session(&shop.st, Money(0)).unwrap();
    let sold = shop.sell(&milk, 1, 10_500).document;
    as_till(&shop);
    link(&shop, false, None);
    let versement = super::credit::collect(
        &shop.st,
        super::credit::CollectRequest {
            input: pos_core::customers::credit::CollectInput {
                customer_id: karim.clone(),
                amount: Money(5_000),
                method: PaymentMethod::Cash,
                reference: None,
                cheque_bank: None,
                cheque_date: None,
                allocate_to: Vec::new(),
                note: None,
            },
            print_receipt: false,
        },
    );
    assert_eq!(versement.unwrap_err().code(), "kridi_needs_host");
    let to_account = super::returns::create(
        &shop.st,
        super::returns::ReturnRequest {
            input: pos_core::returns::ReturnInput {
                original_id: Some(sold.id.clone()),
                lines: Vec::new(),
                refund_method: PaymentMethod::Voucher,
                refund_reference: None,
                lang: pos_core::lang::Lang::Fr,
                note: None,
            },
            print_receipt: false,
            extras: Default::default(),
        },
    );
    assert_eq!(to_account.unwrap_err().code(), "kridi_needs_host");
}

/// What a label print writes (the reprint queue, the barcodes it assigns) is the main
/// PC's, and an IMEI sold on a till is recorded there: offline, nothing prints and nothing is
/// linked, rather than a label or a phone the main PC never hears of.
#[test]
fn label_prints_and_imeis_on_an_offline_till_need_the_main_pc() {
    let shop = shop();
    let milk = shop.product("Lait", 10_000, 900, None).id;
    as_till(&shop);
    link(&shop, false, None);
    let print = super::labels::print(
        &shop.st,
        super::labels::LabelPrintInput {
            request: pos_core::labels::LabelRequest {
                source: pos_core::labels::LabelSource::Products {
                    items: vec![pos_core::labels::LabelItem {
                        product_id: milk,
                        copies: 1,
                        lot_id: None,
                    }],
                },
                template_id: None,
                lang: None,
            },
            printer_id: None,
            assign_missing_barcodes: false,
            arabic_override: true,
        },
    );
    assert_eq!(print.unwrap_err().code(), "host_required");
    let link_sale = super::stock_m2::link_sale(
        &shop.st,
        pos_core::ids::new_id(),
        vec![pos_core::stock::serials::SerialLink {
            line_id: pos_core::ids::new_id(),
            serial_no: "356789101112131".into(),
        }],
    );
    assert_eq!(link_sale.unwrap_err().code(), "host_required");
}

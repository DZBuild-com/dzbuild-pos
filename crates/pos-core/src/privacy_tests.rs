//! Personal-data tools (Task 17).

use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::payments::{PaymentInput, PaymentMethod};
use crate::privacy::{self, AccessPurpose, ConsentChannel, ConsentSource};
use crate::reports::{ReportKind, ReportRequest, run};
use crate::sales::{Cart, CartLine, SaleInput, create_sale};
use crate::testkit::{Shop, shop};

fn with_customer() -> (Shop, String) {
    let shop = shop();
    shop.open_till(100_000);
    let id = shop.customer("Karima Saadi").id;
    let c = id.clone();
    shop.db
        .write(move |tx| {
            Ok(tx.execute(
                "UPDATE customers SET phone = '0550123456', email = 'k@exemple.dz', address = 'Oran' WHERE id = ?1",
                [&c],
            )?)
        })
        .unwrap();
    (shop, id)
}

#[test]
fn consent_is_logged_and_the_latest_answer_counts() {
    let (shop, c) = with_customer();
    for granted in [true, false] {
        let (ctx, id) = (shop.owner_till(), c.clone());
        shop.db
            .write(move |tx| {
                privacy::set_consent(
                    tx,
                    &ctx,
                    &id,
                    ConsentChannel::Whatsapp,
                    granted,
                    ConsentSource::Counter,
                    None,
                )
            })
            .unwrap();
    }
    let id = c.clone();
    let now = shop
        .db
        .read(move |conn| privacy::consents(conn, &id))
        .unwrap();
    assert_eq!((now.len(), now[0].granted), (1, false));
    assert_eq!(now[0].user_name, "Amine");
    let id = c.clone();
    assert_eq!(
        shop.db
            .read(move |conn| privacy::consent_history(conn, &id))
            .unwrap()
            .len(),
        2
    );
    let id = c.clone();
    let customer = shop
        .db
        .read(move |conn| crate::customers::get(conn, &id))
        .unwrap();
    assert!(
        !customer.marketing_consent,
        "M2's flag follows the latest answer"
    );
}

#[test]
fn anonymising_keeps_fiscal_records_and_refuses_open_debts() {
    let (shop, c) = with_customer();
    let tea = shop.product("Thé vert", Money(50_000), &shop.tva19).id;
    let sell = |payments: Vec<PaymentInput>| {
        let ctx = shop.till();
        let mut cart = Cart::new(Lang::Fr, vec![CartLine::product(&tea, Qty(1_000))]);
        cart.customer_id = Some(c.clone());
        shop.db
            .write(move |tx| create_sale(tx, &ctx, SaleInput { cart, payments }))
    };
    sell(vec![PaymentInput {
        method: PaymentMethod::Credit,
        amount: Money(50_000),
        reference: None,
        cheque_bank: None,
        cheque_date: None,
    }])
    .unwrap();
    let (ctx, id) = (shop.owner_till(), c.clone());
    let err = shop
        .db
        .write(move |tx| privacy::anonymise(tx, &ctx, &id, Some("Demande du client".into())))
        .unwrap_err();
    assert_eq!(err.code(), "customer_has_open_items");
    assert_eq!(
        err.payload().details,
        Some(serde_json::json!({ "open": ["balance"] }))
    );
    let (ctx, id) = (shop.owner_till(), c.clone());
    shop.db
        .write(move |tx| {
            crate::customers::credit::collect(
                tx,
                &ctx,
                crate::customers::credit::CollectInput {
                    customer_id: id,
                    amount: Money(50_000),
                    method: PaymentMethod::Cash,
                    reference: None,
                    cheque_bank: None,
                    cheque_date: None,
                    allocate_to: Vec::new(),
                    note: None,
                },
            )
        })
        .unwrap();
    let (ctx, id) = (shop.owner_till(), c.clone());
    let report = shop
        .db
        .write(move |tx| privacy::anonymise(tx, &ctx, &id, Some("Demande du client".into())))
        .unwrap();
    assert_eq!(
        report.kept_documents, 1,
        "the ticket keeps its buyer for the fiscal retention"
    );
    let id = c.clone();
    let after = shop
        .db
        .read(move |conn| crate::customers::get(conn, &id))
        .unwrap();
    assert!(after.name.starts_with("Client anonymisé"));
    assert_eq!(
        (after.phone, after.email, after.address, after.archived),
        (None, None, None, true)
    );
    let id = c.clone();
    let consents = shop
        .db
        .read(move |conn| privacy::consents(conn, &id))
        .unwrap();
    assert_eq!(consents.len(), 3);
    assert!(consents.iter().all(|c| !c.granted), "every channel refused");
    let journal: String = shop
        .db
        .read(|conn| {
            Ok(conn.query_row(
                "SELECT group_concat(payload, '|') FROM audit_log WHERE action LIKE 'privacy.%'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert!(!journal.contains("Karima") && !journal.contains("0550123456"));
    let anonymised: i64 = shop
        .db
        .read(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM audit_log WHERE action IN ('privacy.anonymised', 'customer.archived')",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(anonymised, 1, "one journal row for the anonymisation");
}

/// Final review (Task 17 minor, loi 18-07): an anonymised sole trader keeps nothing that names
/// them — not their NIF, NIS, RC, AI, tags, wilaya, VAT exemption reference, legal form or
/// activity, besides the contact and identity M8 erased.
#[test]
fn anonymising_erases_every_personal_column_of_the_customer() {
    let (shop, c) = with_customer();
    let id = c.clone();
    shop.db
        .write(move |tx| {
            Ok(tx.execute(
                "UPDATE customers SET phone2 = '0661000000', wilaya = '31', nin = '109876543210987654',
                        nif = '000016001234567', nis = '000016123456789', rc = '16/00-1234567B21',
                        ai = '16123456789', tags = 'vip,karima', vat_exempt_ref = 'ANDI 2026/12',
                        legal_form = 'EI', activity = 'Couturière', notes = 'cliente fidèle'
                 WHERE id = ?1",
                [&id],
            )?)
        })
        .unwrap();
    let (ctx, id) = (shop.owner_till(), c.clone());
    shop.db
        .write(move |tx| privacy::anonymise(tx, &ctx, &id, None))
        .unwrap();
    let id = c.clone();
    let left: Vec<Option<String>> = shop
        .db
        .read(move |conn| {
            Ok(conn.query_row(
                "SELECT phone, phone2, email, address, wilaya, nin, nif, nis, rc, ai, tags, vat_exempt_ref,
                        legal_form, activity, notes, ids_verified_at
                 FROM customers WHERE id = ?1",
                [&id],
                |r| (0..16).map(|i| r.get::<_, Option<String>>(i)).collect(),
            )?)
        })
        .unwrap();
    assert!(left.iter().all(Option::is_none), "{left:?}");
}

/// Final review: the append-only rows the app cannot erase (a contract's documents with their ID
/// numbers and phones, a gift voucher's buyer name) keep the customer from being anonymised; the
/// owner is told which.
#[test]
fn a_customer_whose_append_only_rows_hold_personal_data_is_not_anonymised() {
    let (shop, c) = with_customer();
    let ctx = shop.owner_till();
    let customer = c.clone();
    shop.db
        .write(move |tx| {
            crate::packs::set_modules(tx, &ctx.actor(), &[crate::packs::Module::GiftVouchers])?;
            crate::gift_vouchers::sell(
                tx,
                &ctx,
                crate::gift_vouchers::SellInput {
                    amount: Money(100_000),
                    method: PaymentMethod::CardCib,
                    reference: Some("A7".into()),
                    cheque_bank: None,
                    cheque_date: None,
                    customer_id: Some(customer),
                    buyer_name: Some("Karima Saadi".into()),
                },
            )
            .map(drop)
        })
        .unwrap();
    let (ctx, id) = (shop.owner_till(), c.clone());
    let err = shop
        .db
        .write(move |tx| privacy::anonymise(tx, &ctx, &id, None))
        .unwrap_err();
    assert_eq!(err.code(), "customer_has_open_items");
    let open = err.payload().details.unwrap()["open"].clone();
    assert!(
        open.as_array()
            .unwrap()
            .contains(&serde_json::json!("voucherBuyer")),
        "{open}"
    );
}

#[test]
fn a_customer_export_holds_everything_the_shop_keeps_about_them() {
    let (shop, c) = with_customer();
    let (ctx, id) = (shop.owner_till(), c.clone());
    shop.db
        .write(move |tx| {
            privacy::set_consent(
                tx,
                &ctx,
                &id,
                ConsentChannel::Sms,
                true,
                ConsentSource::Form,
                None,
            )
        })
        .unwrap();
    let id = c.clone();
    let export = shop
        .db
        .read(move |conn| privacy::export_customer(conn, &id))
        .unwrap();
    assert_eq!(
        (export.customer.name.as_str(), export.consents.len()),
        ("Karima Saadi", 1)
    );
    assert!(export.customer.marketing_consent);
    let json = serde_json::to_string(&export).unwrap();
    for key in [
        "ledger",
        "documents",
        "loyalty",
        "vouchers",
        "orders",
        "appointments",
        "serviceTickets",
        "instalments",
    ] {
        assert!(json.contains(&format!("\"{key}\"")), "{key}");
    }
}

#[test]
fn personal_data_access_is_journaled_and_reported() {
    let (shop, c) = with_customer();
    let (actor, id) = (shop.owner.actor(), c.clone());
    shop.db
        .write(move |tx| privacy::log_access(tx, &actor, &id, AccessPurpose::View))
        .unwrap();
    // The journal dates its rows with the real clock.
    let today = crate::time::business_date(crate::time::now_ms())
        .format("%Y-%m-%d")
        .to_string();
    let req = ReportRequest::new(ReportKind::PersonalDataAccess, &today, &today);
    let report = shop.db.read(move |conn| run(conn, &req)).unwrap();
    assert_eq!(report.table.rows.len(), 1);
    let row = &report.table.rows[0];
    let code = shop.db.read({
        let id = c.clone();
        move |conn| Ok(crate::customers::get(conn, &id)?.code)
    });
    assert_eq!(row[2], crate::table::Cell::text(code.unwrap()));
    assert_eq!(row[3], crate::table::Cell::text("privacy.accessed"));
    assert_eq!(row[4], crate::table::Cell::text("view"));
    assert_eq!(
        ReportKind::PersonalDataAccess.permission(),
        crate::auth::Permission::AuditView
    );
    assert_eq!(ReportKind::PersonalDataAccess.module(), None);
    assert_eq!(
        ReportKind::PersonalDataAccess.group(),
        crate::reports::ReportGroup::Customers
    );
    let req = ReportRequest::new(ReportKind::PersonalDataAccess, "2026-09-01", "2026-09-02");
    let later = shop.db.read(move |conn| run(conn, &req)).unwrap();
    assert!(later.table.rows.is_empty(), "the period is respected");
}

/// The processing register (PDP-005) lists the customer file always and each module in use that
/// keeps personal data; the DPO comes from `privacy.dpo`.
#[test]
fn the_processing_activities_follow_the_modules_and_the_dpo_is_a_setting() {
    let shop = shop();
    let keys = |shop: &Shop| {
        shop.db
            .read(privacy::activities)
            .unwrap()
            .into_iter()
            .map(|a| a.key)
            .collect::<Vec<_>>()
    };
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| crate::packs::set_modules(tx, &actor, &[]))
        .unwrap();
    assert_eq!(keys(&shop), ["customers"]);
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| {
            crate::packs::set_modules(
                tx,
                &actor,
                &[
                    crate::packs::Module::Pharmacy,
                    crate::packs::Module::Loyalty,
                ],
            )
        })
        .unwrap();
    assert_eq!(keys(&shop), ["customers", "loyalty", "pharmacy_register"]);
    let activity = shop.db.read(privacy::activities).unwrap().remove(2);
    assert_eq!(
        activity.retention,
        "a4.privacy.activity.pharmacy_register.retention"
    );
    assert_eq!(shop.db.read(privacy::dpo).unwrap(), None);
    shop.set(
        "privacy.dpo",
        serde_json::json!({ "name": "Nadia Ferhat", "email": "dpo@exemple.dz" }),
    );
    assert_eq!(
        shop.db.read(privacy::dpo).unwrap(),
        Some(privacy::Dpo {
            name: "Nadia Ferhat".into(),
            email: Some("dpo@exemple.dz".into()),
            phone: None,
        })
    );
    for refused in [
        serde_json::json!({ "name": "" }),
        serde_json::json!({ "name": "Nadia", "email": "pas-un-courriel" }),
        serde_json::json!({ "name": "Nadia", "phone": "0".repeat(21) }),
        serde_json::json!({ "name": "Nadia", "fax": "021" }),
    ] {
        let actor = shop.owner.actor();
        let err = shop
            .db
            .write(move |tx| crate::settings::set_value(tx, &actor, "privacy.dpo", refused))
            .unwrap_err();
        assert_eq!(err.code(), "value_invalid");
    }
}

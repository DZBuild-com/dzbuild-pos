//! Migration 0004 (documents and fiscal): the M3 columns and tables, the triggers that freeze
//! them with their document, the B2B customer columns, the warranty months, and the upgrade of
//! an M2 database whose document and audit chains must still verify.

use super::*;
use crate::audit;
use crate::company::{PriceMode, TaxRegime};
use crate::documents::{self, DocType, DraftDocument, DraftLine};
use crate::ids::new_id;
use crate::lang::Lang;
use crate::money::{Money, Qty, Rate};
use crate::payments::PaymentInput;
use crate::sales::{Cart, CartLine, SaleInput, create_sale};
use crate::tax::timbre::StampMode;
use crate::tax::tva::{Discount, LineInput, compute_line};
use crate::testkit::{NOON, Shop, shop_in};
use chrono::NaiveDate;
use std::path::PathBuf;

fn m2_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/m2-back-office.db")
}

fn count(db: &Db, sql: &'static str) -> i64 {
    db.read(move |c| Ok(c.query_row(sql, [], |r| r.get(0))?))
        .unwrap()
}

fn run(db: &Db, sql: &'static str, id: &str) -> Result<usize, AppError> {
    let id = id.to_owned();
    db.write(move |tx| Ok(tx.execute(sql, [id])?))
}

fn run2(db: &Db, sql: &'static str, a: &str, b: &str) -> Result<usize, AppError> {
    let (a, b) = (a.to_owned(), b.to_owned());
    db.write(move |tx| Ok(tx.execute(sql, [a, b])?))
}

fn run3(db: &Db, sql: &'static str, a: &str, b: &str, c: &str) -> Result<usize, AppError> {
    let (a, b, c) = (a.to_owned(), b.to_owned(), c.to_owned());
    db.write(move |tx| Ok(tx.execute(sql, [a, b, c])?))
}

fn has_column(db: &Db, table: &'static str, column: &'static str) -> bool {
    db.read(move |c| {
        Ok(c.query_row(
            "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2",
            [table, column],
            |r| r.get::<_, i64>(0),
        )?)
    })
    .unwrap()
        == 1
}

fn table_exists(db: &Db, name: &'static str) -> bool {
    db.read(move |c| {
        Ok(c.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [name],
            |r| r.get::<_, i64>(0),
        )?)
    })
    .unwrap()
        == 1
}

/// A one-line draft of `doc_type` (119.00 TTC at 19 %), written through the M1a engine.
fn draft_of(doc_type: DocType) -> DraftDocument {
    let amounts = compute_line(
        PriceMode::Ttc,
        &LineInput {
            unit_price: Money(11_900),
            qty: Qty(1_000),
            discount: Discount::None,
            rate: Rate(1_900),
        },
    )
    .unwrap();
    DraftDocument {
        doc_type,
        terminal_code: "C1".into(),
        business_date: NaiveDate::from_ymd_opt(2026, 9, 28).unwrap(),
        customer_id: None,
        supplier_id: None,
        user_id: "u1".into(),
        terminal_id: "t1".into(),
        cash_session_id: None,
        price_level_id: None,
        lang: Lang::Fr,
        tax_regime: TaxRegime::Reel,
        price_mode: PriceMode::Ttc,
        stamp_mode: StampMode::ChargeCustomer,
        stamp_duty: Money::ZERO,
        stamp_charged: Money::ZERO,
        rounding: Money::ZERO,
        paid_total: Money::ZERO,
        change_given: Money::ZERO,
        credit_amount: Money::ZERO,
        note: None,
        parent_id: None,
        source_ref: None,
        lines: vec![DraftLine {
            id: new_id(),
            product_id: None,
            description: "Article".into(),
            qty: Qty(1_000),
            unit_code: "pc".into(),
            unit_price: Money(11_900),
            price_overridden: false,
            tax_rate_bp: Rate(1_900),
            amounts,
            cost_price: Money::ZERO,
            parent_line_id: None,
            origin_line_id: None,
            restock_location_id: None,
            reason_code: None,
            note: None,
        }],
        payments: Vec::new(),
    }
}

fn draft(db: &Db, doc_type: DocType) -> String {
    db.write(move |tx| documents::insert_draft(tx, &draft_of(doc_type), NOON))
        .unwrap()
}

fn validated(db: &Db, doc_type: DocType) -> String {
    let id = draft(db, doc_type);
    let to_validate = id.clone();
    db.write(move |tx| documents::validate(tx, &to_validate, NOON).map(drop))
        .unwrap();
    id
}

#[test]
fn the_m3_tables_and_columns_exist() {
    let db = Db::open_memory().unwrap();
    assert_eq!(db.report().schema_version, 10);
    for (table, column) in [
        ("documents", "valid_until"),
        ("documents", "due_date"),
        ("documents", "second_lang"),
        ("documents", "credit_kind"),
        ("documents", "origin"),
        ("document_lines", "description_alt"),
        ("customers", "legal_form"),
        ("customers", "activity"),
        ("customers", "vat_exempt"),
        ("customers", "vat_exempt_ref"),
        ("product_extras", "warranty_months"),
    ] {
        assert!(has_column(&db, table, column), "{table}.{column}");
    }
    for table in [
        "document_parties",
        "document_links",
        "document_drafts",
        "counter_skips",
    ] {
        assert!(table_exists(&db, table), "{table}");
    }
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name IN
               ('idx_documents_type_day', 'idx_documents_customer_day', 'idx_documents_total_due',
                'idx_documents_recent', 'idx_document_links_invoiced_once')"
        ),
        5
    );
}

#[test]
fn m3_fields_fit_their_kind() {
    let db = Db::open_memory().unwrap();
    let ticket = draft(&db, DocType::Ticket);
    let quote = draft(&db, DocType::Quote);
    let invoice = draft(&db, DocType::Invoice);
    let credit = draft(&db, DocType::CreditNote);
    assert!(
        run(
            &db,
            "UPDATE documents SET valid_until = '2026-10-28' WHERE id = ?1",
            &ticket
        )
        .is_err()
    );
    run(
        &db,
        "UPDATE documents SET valid_until = '2026-10-28' WHERE id = ?1",
        &quote,
    )
    .unwrap();
    assert!(
        run(
            &db,
            "UPDATE documents SET valid_until = '28/10/2026' WHERE id = ?1",
            &quote
        )
        .is_err()
    );
    assert!(
        run(
            &db,
            "UPDATE documents SET due_date = '2026-10-28' WHERE id = ?1",
            &quote
        )
        .is_err()
    );
    run(
        &db,
        "UPDATE documents SET due_date = '2026-10-28' WHERE id = ?1",
        &invoice,
    )
    .unwrap();
    assert!(
        run(
            &db,
            "UPDATE documents SET credit_kind = 'goods' WHERE id = ?1",
            &invoice
        )
        .is_err()
    );
    run(
        &db,
        "UPDATE documents SET credit_kind = 'financial' WHERE id = ?1",
        &credit,
    )
    .unwrap();
    assert!(
        run(
            &db,
            "UPDATE documents SET credit_kind = 'gift' WHERE id = ?1",
            &credit
        )
        .is_err()
    );
    assert!(
        run(
            &db,
            "UPDATE documents SET second_lang = 'de' WHERE id = ?1",
            &invoice
        )
        .is_err()
    );
    run(
        &db,
        "UPDATE documents SET second_lang = 'ar', origin = 'ticket' WHERE id = ?1",
        &invoice,
    )
    .unwrap();
    assert!(
        run(
            &db,
            "UPDATE documents SET origin = 'gift' WHERE id = ?1",
            &invoice
        )
        .is_err()
    );
}

#[test]
fn parties_links_and_carts_freeze_with_their_document() {
    let db = Db::open_memory().unwrap();
    let invoice = draft(&db, DocType::Invoice);
    run(
        &db,
        "INSERT INTO document_parties (document_id, role, name, nif) VALUES (?1, 'buyer', 'SARL Amine', '000016001234567')",
        &invoice,
    )
    .unwrap();
    run(
        &db,
        "UPDATE document_parties SET nif = '000016009999999' WHERE document_id = ?1",
        &invoice,
    )
    .unwrap();
    run(
        &db,
        "INSERT INTO document_drafts (document_id, cart, updated_at) VALUES (?1, '{\"lines\":[]}', 1)",
        &invoice,
    )
    .unwrap();
    run(
        &db,
        "UPDATE document_drafts SET cart = '{}' WHERE document_id = ?1",
        &invoice,
    )
    .unwrap();
    let to_validate = invoice.clone();
    db.write(move |tx| documents::validate(tx, &to_validate, NOON).map(drop))
        .unwrap();
    assert!(
        run(
            &db,
            "UPDATE document_parties SET nif = 'x' WHERE document_id = ?1",
            &invoice
        )
        .is_err()
    );
    assert!(
        run(
            &db,
            "DELETE FROM document_parties WHERE document_id = ?1",
            &invoice
        )
        .is_err()
    );
    assert!(
        run(
            &db,
            "INSERT INTO document_parties (document_id, role, name) VALUES (?1, 'seller', 'X')",
            &invoice
        )
        .is_err(),
        "no party is added after validation"
    );
    assert!(
        run(
            &db,
            "UPDATE document_drafts SET cart = '[]' WHERE document_id = ?1",
            &invoice
        )
        .is_err()
    );
    assert!(
        run(
            &db,
            "DELETE FROM document_drafts WHERE document_id = ?1",
            &invoice
        )
        .is_err()
    );

    // Beyond the brief: a row of a draft cannot be moved onto the validated invoice, and no
    // cart is stored for it after its validation.
    let other = draft(&db, DocType::Invoice);
    run(
        &db,
        "INSERT INTO document_parties (document_id, role, name) VALUES (?1, 'buyer', 'Karim')",
        &other,
    )
    .unwrap();
    run(
        &db,
        "INSERT INTO document_drafts (document_id, cart, updated_at) VALUES (?1, '{}', 1)",
        &other,
    )
    .unwrap();
    assert!(
        run2(
            &db,
            "UPDATE document_parties SET document_id = ?1, role = 'seller' WHERE document_id = ?2",
            &invoice,
            &other,
        )
        .is_err(),
        "a party is not moved onto a validated document"
    );
    assert!(
        run2(
            &db,
            "UPDATE document_drafts SET document_id = ?1 WHERE document_id = ?2",
            &invoice,
            &other,
        )
        .is_err(),
        "a cart is not moved onto a validated document"
    );
    let delivery_draft = draft(&db, DocType::DeliveryNote);
    let to_validate = delivery_draft.clone();
    db.write(move |tx| documents::validate(tx, &to_validate, NOON).map(drop))
        .unwrap();
    assert!(
        run(
            &db,
            "INSERT INTO document_drafts (document_id, cart, updated_at) VALUES (?1, '{}', 1)",
            &delivery_draft
        )
        .is_err(),
        "no cart is stored for a validated document"
    );

    let delivery = validated(&db, DocType::DeliveryNote);
    let (first, second) = (draft(&db, DocType::Invoice), draft(&db, DocType::Invoice));
    run3(
        &db,
        "INSERT INTO document_links (id, from_id, to_id, kind, created_at) VALUES (?1, ?2, ?3, 'delivered', 1)",
        &new_id(),
        &delivery,
        &first,
    )
    .unwrap();
    assert!(
        run3(
            &db,
            "INSERT INTO document_links (id, from_id, to_id, kind, created_at) VALUES (?1, ?2, ?3, 'delivered', 1)",
            &new_id(),
            &delivery,
            &second,
        )
        .is_err(),
        "a delivery note is invoiced once"
    );
    run3(
        &db,
        "INSERT INTO document_links (id, from_id, to_id, kind, created_at) VALUES (?1, ?2, ?3, 'converted', 1)",
        &new_id(),
        &delivery,
        &second,
    )
    .unwrap();
    assert!(
        run(
            &db,
            "UPDATE document_links SET kind = 'converted' WHERE from_id = ?1",
            &delivery
        )
        .is_err()
    );
    assert!(
        run(
            &db,
            "DELETE FROM document_links WHERE from_id = ?1",
            &delivery
        )
        .is_err()
    );
}

fn count_for(db: &Db, sql: &'static str, id: &str) -> i64 {
    let id = id.to_owned();
    db.read(move |c| Ok(c.query_row(sql, [id], |r| r.get(0))?))
        .unwrap()
}

/// Review fix: 0004 gives a draft rows that point at it (its stored cart, its parties, the link
/// from the document it was made from). Rewriting the draft (M2 `replace_draft`, which the
/// document editor's save and an invoice's finish call) keeps the document row, so they all
/// survive; only its lines, taxes and payments are replaced.
#[test]
fn a_replaced_draft_keeps_its_cart_parties_and_links() {
    let db = Db::open_memory().unwrap();
    let quote = validated(&db, DocType::Quote);
    let mut first = draft_of(DocType::Invoice);
    first.payments = vec![documents::DraftPayment {
        method: crate::payments::PaymentMethod::Cash,
        amount: Money(5_000),
        reference: None,
        cheque_bank: None,
        cheque_date: None,
    }];
    let invoice = db
        .write(move |tx| documents::insert_draft(tx, &first, NOON))
        .unwrap();
    run(
        &db,
        "INSERT INTO document_drafts (document_id, cart, updated_at) VALUES (?1, '{\"lines\":[1]}', 1)",
        &invoice,
    )
    .unwrap();
    run(
        &db,
        "INSERT INTO document_parties (document_id, role, name, nif) VALUES (?1, 'buyer', 'SARL Amine', '000016001234567')",
        &invoice,
    )
    .unwrap();
    run3(
        &db,
        "INSERT INTO document_links (id, from_id, to_id, kind, created_at) VALUES (?1, ?2, ?3, 'converted', 1)",
        &new_id(),
        &quote,
        &invoice,
    )
    .unwrap();
    run(
        &db,
        "UPDATE documents SET origin = 'quote', due_date = '2026-10-28' WHERE id = ?1",
        &invoice,
    )
    .unwrap();

    let mut rewritten = draft_of(DocType::Invoice);
    let mut second_line = rewritten.lines[0].clone();
    second_line.id = new_id();
    second_line.description = "Article B".into();
    rewritten.lines.push(second_line);
    rewritten.note = Some("livraison vendredi".into());
    rewritten.payments = vec![documents::DraftPayment {
        method: crate::payments::PaymentMethod::Cash,
        amount: Money(23_800),
        reference: None,
        cheque_bank: None,
        cheque_date: None,
    }];
    let (id, d) = (invoice.clone(), rewritten.clone());
    db.write(move |tx| documents::replace_draft(tx, &id, &d, NOON + 1))
        .unwrap();

    let doc = db
        .read({
            let id = invoice.clone();
            move |c| documents::get(c, &id)
        })
        .unwrap();
    assert_eq!(
        doc.lines
            .iter()
            .map(|l| l.description.as_str())
            .collect::<Vec<_>>(),
        ["Article", "Article B"]
    );
    assert_eq!(
        (doc.total_ttc, doc.total_due, doc.note.as_deref()),
        (Money(23_800), Money(23_800), Some("livraison vendredi"))
    );
    assert_eq!(
        doc.taxes
            .iter()
            .map(|t| (t.rate_bp, t.base_ht, t.tax_amount))
            .collect::<Vec<_>>(),
        [(Rate(1_900), Money(20_000), Money(3_800))]
    );
    assert_eq!(
        doc.payments.iter().map(|p| p.amount).collect::<Vec<_>>(),
        [Money(23_800)],
        "the old payments are gone"
    );
    assert_eq!(doc.created_at, NOON, "the draft keeps its creation time");
    assert_eq!(
        count_for(
            &db,
            "SELECT COUNT(*) FROM document_lines WHERE document_id = ?1",
            &invoice
        ),
        2
    );
    assert_eq!(
        count_for(
            &db,
            "SELECT COUNT(*) FROM document_drafts WHERE document_id = ?1 AND cart = '{\"lines\":[1]}'",
            &invoice
        ),
        1,
        "the stored cart survives"
    );
    assert_eq!(
        count_for(
            &db,
            "SELECT COUNT(*) FROM document_parties WHERE document_id = ?1 AND role = 'buyer'
               AND name = 'SARL Amine' AND nif = '000016001234567'",
            &invoice
        ),
        1,
        "the buyer party survives"
    );
    assert_eq!(
        count_for(
            &db,
            "SELECT COUNT(*) FROM document_links WHERE to_id = ?1 AND kind = 'converted'",
            &invoice
        ),
        1,
        "the incoming link survives"
    );
    assert_eq!(
        count_for(
            &db,
            "SELECT COUNT(*) FROM documents WHERE id = ?1 AND origin = 'quote' AND due_date = '2026-10-28'",
            &invoice
        ),
        1,
        "the M3 fields of the draft survive"
    );

    // The rewritten draft validates and every chain still verifies.
    let to_validate = invoice.clone();
    db.write(move |tx| documents::validate(tx, &to_validate, NOON + 2).map(drop))
        .unwrap();
    assert!(
        db.read(documents::verify_all)
            .unwrap()
            .iter()
            .all(|chain| chain.first_broken_seq.is_none())
    );
    let (id, d) = (invoice.clone(), rewritten);
    assert_eq!(
        db.write(move |tx| documents::replace_draft(tx, &id, &d, NOON + 3))
            .unwrap_err()
            .code(),
        "document_already_validated"
    );
}

#[test]
fn customers_carry_b2b_mentions_and_products_a_warranty() {
    let db = Db::open_memory().unwrap();
    run(
        &db,
        "INSERT INTO customers (id, code, name, legal_form, activity, vat_exempt, vat_exempt_ref, created_at, updated_at)
         VALUES (?1, 'C00001', 'SARL Amine', 'SARL', 'Quincaillerie', 1, 'ATT-2026-114', 0, 0)",
        "00000000-0000-7000-8000-00000000d001",
    )
    .unwrap();
    assert_eq!(count(&db, "SELECT vat_exempt FROM customers"), 1);
    assert!(
        run(
            &db,
            "UPDATE customers SET vat_exempt = 2 WHERE id = ?1",
            "00000000-0000-7000-8000-00000000d001"
        )
        .is_err()
    );
    // A memory database has no rate until the data pack seeds one (`catalog::ensure_defaults`).
    run(
        &db,
        "INSERT INTO tax_rates (id, code, rate_bp, created_at, updated_at) VALUES (?1, 'TVA19', 1900, 0, 0)",
        "00000000-0000-7000-8000-00000000e401",
    )
    .unwrap();
    run(
        &db,
        "INSERT INTO products (id, name_fr, unit_id, tax_rate_id, sell_price, created_at, updated_at)
         SELECT ?1, 'Téléphone', '00000000-0000-7000-8000-000000000101', id, 3500000, 0, 0 FROM tax_rates LIMIT 1",
        "00000000-0000-7000-8000-00000000e001",
    )
    .unwrap();
    run(
        &db,
        "INSERT INTO product_extras (product_id, warranty_months, updated_at) VALUES (?1, 12, 0)",
        "00000000-0000-7000-8000-00000000e001",
    )
    .unwrap();
    assert!(
        run(
            &db,
            "UPDATE product_extras SET warranty_months = 0 WHERE product_id = ?1",
            "00000000-0000-7000-8000-00000000e001"
        )
        .is_err()
    );
}

#[test]
fn the_m2_fixture_upgrades_in_place_after_a_backup() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("pos.db");
    std::fs::copy(m2_fixture_path(), &file).unwrap();
    let db = Db::open(
        &file,
        DbOptions {
            readers: 1,
            backup_dir: Some(dir.path().join("backups")),
        },
    )
    .unwrap();
    let report = db.report().clone();
    assert_eq!((report.schema_version, report.migrations_applied), (10, 7));
    assert!(
        report.pre_migration_backup.is_some(),
        "backed up before 0004"
    );
    let chains = db.read(documents::verify_all).unwrap();
    assert!(!chains.is_empty());
    assert!(
        chains.iter().all(|c| c.first_broken_seq.is_none()),
        "every M2 document still verifies: {chains:?}"
    );
    assert_eq!(db.read(audit::verify_chain).unwrap().first_broken_seq, None);
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM documents WHERE valid_until IS NOT NULL OR due_date IS NOT NULL
               OR second_lang IS NOT NULL OR credit_kind IS NOT NULL OR origin IS NOT NULL"
        ),
        0
    );
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM customers WHERE vat_exempt = 0"),
        1
    );
    db.close().unwrap();
}

/// One bottle sold for cash at the till; its ticket number.
fn sell_one(shop: &Shop, product_id: &str) -> String {
    let ctx = shop.till();
    let input = SaleInput {
        cart: Cart::new(Lang::Fr, vec![CartLine::product(product_id, Qty(1_000))]),
        payments: vec![PaymentInput::cash(Money(12_000))],
    };
    let sale = shop
        .db
        .write(move |tx| create_sale(tx, &ctx, input))
        .unwrap();
    sale.document.number.unwrap()
}

/// The owner closes the till (every permission: no variance approval); its Z number.
fn close_till(shop: &Shop) -> String {
    let ctx = shop.owner_till();
    shop.db
        .write(move |tx| crate::cash::close_session(tx, &ctx, crate::cash::CloseInput::default()))
        .unwrap()
        .z_number
}

fn chains_verify(shop: &Shop) -> (bool, Option<String>, Option<i64>) {
    let terminal = shop.terminal.id.clone();
    shop.db
        .read(move |c| {
            let documents = documents::verify_all(c)?
                .iter()
                .all(|chain| chain.first_broken_seq.is_none());
            let closures = crate::cash::closing::verify_closures(c, &terminal)?.first_broken_z;
            let journal = audit::verify_chain(c)?.first_broken_seq;
            Ok((documents, closures, journal))
        })
        .unwrap()
}

/// M1 review: a restored database is behind the one it replaces. At its first start its
/// counters and its Z counter move past the numbers that database had printed; the jumps are
/// journaled with the restore, the next ticket and the next Z take the following numbers, and
/// every chain (documents, Z, journal) still verifies. A jump the journal does not list stays a
/// gap.
#[test]
fn a_restore_fast_forwards_the_counters_and_every_chain_still_verifies() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pos.db");
    let options = || DbOptions {
        readers: 1,
        backup_dir: Some(dir.path().join("backups")),
    };
    let shop = shop_in(
        Db::open(&path, options()).unwrap(),
        TaxRegime::Reel,
        PriceMode::Ttc,
    );
    let milk = shop.product("Lait", Money(12_000), &shop.tva9);
    shop.stock_in(&milk.id, 50_000);
    shop.open_till(1_000_000);
    assert_eq!(sell_one(&shop, &milk.id), "TK2026-C1-000001");
    let chosen = shop.db.backup(BackupKind::Manual).unwrap();
    // Printed after the chosen backup: what the restore replaces.
    assert_eq!(sell_one(&shop, &milk.id), "TK2026-C1-000002");
    assert_eq!(sell_one(&shop, &milk.id), "TK2026-C1-000003");
    assert_eq!(close_till(&shop), "ZC1-000001");
    let requested_by = shop.owner.user_id.clone();
    shop.db
        .schedule_restore(&chosen.file_name, Some(&requested_by))
        .unwrap();
    let Shop {
        db,
        pack,
        terminal,
        owner,
        cashier,
        tva19,
        tva9,
        exempt,
    } = shop;
    db.close().unwrap();

    assert!(apply_pending_restore(&path).unwrap().is_some());
    let db = Db::open(&path, options()).unwrap();
    let terminal_id = terminal.id.clone();
    let reissued = db
        .write(move |tx| journal_pending_restore(tx, Some(&terminal_id)))
        .unwrap();
    assert_eq!(reissued, Some(Vec::new()), "no number is handed out twice");
    let shop = Shop {
        db,
        pack,
        terminal,
        owner,
        cashier,
        tva19,
        tva9,
        exempt,
    };
    assert_eq!(
        count(
            &shop.db,
            "SELECT last_number FROM counters WHERE doc_type = 'ticket' AND series = 'C1'"
        ),
        3
    );
    assert_eq!(
        count(
            &shop.db,
            "SELECT last_number FROM counters WHERE doc_type = 'z_report' AND series = 'C1'"
        ),
        1
    );
    let payload: String = shop
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT payload FROM audit_log WHERE action = 'backup.restored'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
    let skipped: Vec<(&str, &str, i64, i64)> = payload["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["first"].as_str().unwrap(),
                s["last"].as_str().unwrap(),
                s["from"].as_i64().unwrap(),
                s["to"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        skipped,
        [
            ("TK2026-C1-000002", "TK2026-C1-000003", 2, 3),
            ("ZC1-000001", "ZC1-000001", 1, 1)
        ]
    );
    assert_eq!(payload["reissued"], serde_json::json!([]));
    assert_eq!(count(&shop.db, "SELECT COUNT(*) FROM counter_skips"), 2);

    assert_eq!(
        payload["replacedMarks"]["closures"][0]["grandTotal"],
        serde_json::json!(36_000),
        "the replaced database's grand total is journaled"
    );

    // The restored till goes on: the next numbers follow the replaced database's, and so does
    // the perpetual grand total (AUD-005: never reset by a restore).
    assert_eq!(sell_one(&shop, &milk.id), "TK2026-C1-000004");
    let ctx = shop.owner_till();
    let z2 = shop
        .db
        .write(move |tx| crate::cash::close_session(tx, &ctx, crate::cash::CloseInput::default()))
        .unwrap();
    assert_eq!(z2.z_number, "ZC1-000002");
    assert_eq!(
        (z2.grand_total_carried, z2.grand_total),
        (Some(Money(36_000)), Money(36_000 + 24_000)),
        "on from ZC1-000001's 360,00, never back to the restored copy's 0"
    );
    assert_eq!(chains_verify(&shop), (true, None, None));

    // A jump that no restore journaled is a gap, for the documents and for the Z.
    shop.db
        .write(|tx| {
            tx.execute_batch(
                "INSERT INTO counter_skips (id, doc_type, series, fiscal_year, from_number, to_number,
                                            restore_id, created_at)
                   VALUES ('00000000-0000-7000-8000-0000000051a1', 'ticket', 'C1', 2026, 5, 6,
                           '00000000-0000-7000-8000-0000000051a0', 0),
                          ('00000000-0000-7000-8000-0000000051a2', 'z_report', 'C1', 0, 3, 4,
                           '00000000-0000-7000-8000-0000000051a0', 0);
                 UPDATE counters SET last_number = 6 WHERE doc_type = 'ticket' AND series = 'C1';
                 UPDATE counters SET last_number = 4 WHERE doc_type = 'z_report' AND series = 'C1';",
            )?;
            Ok(())
        })
        .unwrap();
    assert_eq!(
        chains_verify(&shop),
        (false, Some("ZC1-000003".to_owned()), None)
    );
}

#[test]
#[ignore = "writes tests/fixtures/m2-back-office.db; run once before 0004 is listed"]
fn write_m2_fixture() {
    let path = m2_fixture_path();
    let _ = std::fs::remove_file(&path);
    let db = Db::open(
        &path,
        DbOptions {
            readers: 1,
            backup_dir: Some(std::env::temp_dir().join("dzpos-fixture-backups")),
        },
    )
    .unwrap();
    assert_eq!(
        db.report().schema_version,
        3,
        "run this before listing 0004"
    );
    let shop = shop_in(db, TaxRegime::Reel, PriceMode::Ttc);
    let karim = shop.customer("Karim Bensalah");
    let milk = shop.product("Lait", Money(12_000), &shop.tva9);
    shop.stock_in(&milk.id, 20_000);
    shop.open_till(1_000_000);
    let ctx = shop.till();
    let mut cart = Cart::new(Lang::Fr, vec![CartLine::product(&milk.id, Qty(2_000))]);
    cart.customer_id = Some(karim.id.clone());
    let input = SaleInput {
        cart,
        payments: vec![PaymentInput::cash(Money(30_000))],
    };
    shop.db
        .write(move |tx| create_sale(tx, &ctx, input))
        .unwrap();
    shop.db.close().unwrap();
}

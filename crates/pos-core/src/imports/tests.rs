//! Imports never overwrite silently or half-apply (Review Focus 3).

use super::engine::*;
use super::spec::{ImportEntity, ImportPolicy, Problem};
use crate::catalog::UNIT_PIECE_ID;
use crate::catalog::products::{self, ProductInput};
use crate::error::AppError;
use crate::money::{Money, Qty};
use crate::testkit::{Shop, shop};
use std::collections::BTreeMap;

fn options(entity: ImportEntity, policy: ImportPolicy, mapping: &[&str]) -> ImportOptions {
    ImportOptions {
        entity,
        policy,
        file_name: "articles.xlsx".into(),
        header_row: 0,
        mapping: mapping
            .iter()
            .map(|k| (!k.is_empty()).then(|| (*k).to_owned()))
            .collect(),
        location_id: None,
        count_id: None,
    }
}

fn sheet(rows: &[&[&str]]) -> Vec<Vec<String>> {
    rows.iter()
        .map(|r| r.iter().map(|c| (*c).to_owned()).collect())
        .collect()
}

fn choices() -> BTreeMap<String, String> {
    [
        ("customer_type:reseller", "reseller"),
        ("customer_type:revendeur", "reseller"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect()
}

fn existing(shop: &Shop, reference: &str, name: &str, price: i64) -> products::Product {
    let mut input = ProductInput::simple(name, UNIT_PIECE_ID, &shop.tva19, Money(price));
    input.reference = Some(reference.into());
    shop.product_with(input)
}

fn by_reference(shop: &Shop, reference: &'static str) -> products::Product {
    shop.db
        .read(move |c| {
            let id: String = c.query_row(
                "SELECT id FROM products WHERE reference = ?1",
                [reference],
                |r| r.get(0),
            )?;
            products::get(c, &id)
        })
        .unwrap()
}

fn run(shop: &Shop, o: &ImportOptions, rows: &[ImportRow]) -> ImportJob {
    let dry = dry_run(&shop.db, &shop.owner_till(), o, rows, &choices()).unwrap();
    apply(
        &shop.db,
        &shop.owner_till(),
        o,
        rows,
        &choices(),
        &dry.fingerprint,
        &mut |_| {},
    )
    .unwrap()
}

const PRODUCT_COLUMNS: &[&str] = &["reference", "nameFr", "sellPrice", "unit"];

#[test]
fn a_dry_run_writes_nothing() {
    let shop = shop();
    let o = options(
        ImportEntity::Products,
        ImportPolicy::CreateOnly,
        PRODUCT_COLUMNS,
    );
    let rows = rows_of(
        &sheet(&[
            &["Réf", "Désignation", "Prix", "Unité"],
            &["A1", "Huile 5 L", "1190", ""],
            &[],
            &["A2", "Sucre 1 kg", "130,00", ""],
            &["A3", "Café", "1,234", ""],
        ]),
        &o,
    );
    assert_eq!(
        rows.iter().map(|r| r.number).collect::<Vec<_>>(),
        vec![2, 4, 5],
        "spreadsheet row numbers, empty row dropped"
    );
    let before = (
        shop.count("SELECT COUNT(*) FROM products"),
        shop.count("SELECT COUNT(*) FROM audit_log"),
    );
    let dry = dry_run(&shop.db, &shop.owner_till(), &o, &rows, &choices()).unwrap();
    assert_eq!(
        (dry.total, dry.create, dry.update, dry.skip, dry.fail),
        (3, 2, 0, 0, 1)
    );
    assert_eq!(
        dry.problems,
        vec![Problem {
            row: 5,
            field: Some("sellPrice".into()),
            value: Some("1,234".into()),
            code: "ambiguous_separator".into()
        }]
    );
    assert_eq!(
        (
            shop.count("SELECT COUNT(*) FROM products"),
            shop.count("SELECT COUNT(*) FROM audit_log")
        ),
        before
    );
    assert_eq!(shop.count("SELECT COUNT(*) FROM import_jobs"), 0);
    assert_eq!(dry.fingerprint, fingerprint(&o, &rows));
}

#[test]
fn create_only_never_touches_existing_products() {
    let shop = shop();
    existing(&shop, "A1", "Huile", 100_000);
    let o = options(
        ImportEntity::Products,
        ImportPolicy::CreateOnly,
        PRODUCT_COLUMNS,
    );
    let rows = rows_of(
        &sheet(&[
            &["r", "n", "p", "u"],
            &["A1", "Huile Elio 5 L", "1190", ""],
            &["A9", "Thé", "250", ""],
        ]),
        &o,
    );
    let job = run(&shop, &o, &rows);
    assert_eq!(
        (
            job.created,
            job.updated,
            job.skipped,
            job.failed,
            job.status
        ),
        (1, 0, 1, 0, JobStatus::Done)
    );
    assert_eq!(
        job.problems,
        vec![Problem {
            row: 2,
            field: None,
            value: None,
            code: "already_exists".into()
        }]
    );
    let a1 = by_reference(&shop, "A1");
    assert_eq!(
        (a1.name_fr.as_str(), a1.sell_price),
        ("Huile", Money(100_000))
    );
    assert_eq!(by_reference(&shop, "A9").sell_price, Money(25_000));
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'import.applied'"),
        1
    );
}

#[test]
fn a_failing_row_is_reported_and_the_batch_goes_on() {
    let shop = shop();
    let o = options(
        ImportEntity::Products,
        ImportPolicy::Upsert,
        PRODUCT_COLUMNS,
    );
    let rows = rows_of(
        &sheet(&[
            &["r", "n", "p", "u"],
            &["A1", "Huile", "1190", ""],
            // Not a seeded unit (migration 0002 seeds `carton`, matched without case).
            &["A2", "Eau en palette", "900", "PALETTE"],
            &["A3", "Sucre", "130", ""],
        ]),
        &o,
    );
    let job = run(&shop, &o, &rows);
    assert_eq!((job.created, job.failed), (2, 1));
    assert_eq!(job.problems[0].row, 3);
    assert_eq!(job.problems[0].code, "unit_not_found");
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM products WHERE reference IN ('A1', 'A3')"),
        2
    );
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM products WHERE reference = 'A2'"),
        0,
        "the failed row left nothing"
    );
}

#[test]
fn undo_restores_updated_products_and_archives_created_ones() {
    let shop = shop();
    existing(&shop, "A1", "Huile", 100_000);
    let o = options(
        ImportEntity::Products,
        ImportPolicy::Upsert,
        PRODUCT_COLUMNS,
    );
    let rows = rows_of(
        &sheet(&[
            &["r", "n", "p", "u"],
            &["A1", "Huile Elio 5 L", "1190", ""],
            &["A2", "Sucre", "130", ""],
        ]),
        &o,
    );
    let job = run(&shop, &o, &rows);
    assert_eq!((job.created, job.updated, job.undoable), (1, 1, true));
    assert_eq!(by_reference(&shop, "A1").sell_price, Money(119_000));
    let ctx = shop.owner_till();
    let id = job.id.clone();
    let undone = shop.db.write(move |tx| undo(tx, &ctx, &id)).unwrap();
    assert_eq!(undone.status, JobStatus::Undone);
    let a1 = by_reference(&shop, "A1");
    assert_eq!(
        (a1.name_fr.as_str(), a1.sell_price, a1.archived),
        ("Huile", Money(100_000), false)
    );
    assert!(
        by_reference(&shop, "A2").archived,
        "created by the import: archived, never deleted"
    );
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'import.undone'"),
        1
    );
    let ctx = shop.owner_till();
    let id = job.id.clone();
    assert_eq!(
        shop.db.write(move |tx| undo(tx, &ctx, &id)).unwrap_err(),
        AppError::conflict("import_undo_not_allowed")
    );
}

#[test]
fn postings_are_undone_by_counter_entries() {
    let shop = shop();
    let karim = shop.customer("Karim");
    let oil = existing(&shop, "A1", "Huile", 100_000);
    let balances = options(
        ImportEntity::CustomerBalances,
        ImportPolicy::CreateOnly,
        &["code", "amount"],
    );
    let rows = rows_of(
        &sheet(&[&["Code", "Solde"], &[karim.code.as_str(), "1 500,00"]]),
        &balances,
    );
    let credit_job = run(&shop, &balances, &rows);
    assert_eq!(
        (credit_job.created, credit_job.failed),
        (1, 0),
        "a posting counts as created"
    );
    let stock = options(
        ImportEntity::OpeningStock,
        ImportPolicy::CreateOnly,
        &["reference", "qty", "unitCost"],
    );
    let rows = rows_of(
        &sheet(&[&["Réf", "Qté", "PA"], &["A1", "12", "800"]]),
        &stock,
    );
    let stock_job = run(&shop, &stock, &rows);
    let level = |shop: &Shop| {
        let id = oil.id.clone();
        shop.db
            .read(move |c| crate::stock::on_hand(c, &id))
            .unwrap()
    };
    let balance = |shop: &Shop| {
        let id = karim.id.clone();
        shop.db
            .read(move |c| crate::customers::get(c, &id))
            .unwrap()
            .balance
    };
    assert_eq!(
        (level(&shop), balance(&shop)),
        (Qty(12_000), Money(150_000))
    );
    for job in [stock_job, credit_job] {
        let ctx = shop.owner_till();
        shop.db.write(move |tx| undo(tx, &ctx, &job.id)).unwrap();
    }
    assert_eq!((level(&shop), balance(&shop)), (Qty::ZERO, Money::ZERO));
    let id = karim.id.clone();
    assert!(
        !shop
            .db
            .read(move |c| crate::ledger::has_opening(c, crate::ledger::Side::Customer, &id))
            .unwrap(),
        "the opening can be typed again"
    );
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM customer_ledger"),
        2,
        "nothing deleted: an opening and its counter-entry"
    );
}

#[test]
fn apply_needs_the_fingerprint_of_its_dry_run() {
    let shop = shop();
    let o = options(
        ImportEntity::Products,
        ImportPolicy::CreateOnly,
        PRODUCT_COLUMNS,
    );
    let rows = rows_of(
        &sheet(&[&["r", "n", "p", "u"], &["A1", "Huile", "1190", ""]]),
        &o,
    );
    let dry = dry_run(&shop.db, &shop.owner_till(), &o, &rows, &choices()).unwrap();
    let remapped = options(
        ImportEntity::Products,
        ImportPolicy::Upsert,
        PRODUCT_COLUMNS,
    );
    let err = apply(
        &shop.db,
        &shop.owner_till(),
        &remapped,
        &rows,
        &choices(),
        &dry.fingerprint,
        &mut |_| {},
    )
    .unwrap_err();
    assert_eq!(err, AppError::conflict("import_mapping_invalid"));
    assert_eq!(shop.count("SELECT COUNT(*) FROM products"), 0);
}

#[test]
fn duplicates_in_the_file_are_reported_once_and_the_first_row_wins() {
    let shop = shop();
    let o = options(
        ImportEntity::Products,
        ImportPolicy::CreateOnly,
        PRODUCT_COLUMNS,
    );
    let rows = rows_of(
        &sheet(&[
            &["r", "n", "p", "u"],
            &["A1", "Huile", "1190", ""],
            &["a1", "Huile bis", "1200", ""],
        ]),
        &o,
    );
    let dry = dry_run(&shop.db, &shop.owner_till(), &o, &rows, &choices()).unwrap();
    assert_eq!((dry.create, dry.fail), (1, 1));
    assert_eq!(
        dry.problems[0],
        Problem {
            row: 3,
            field: Some("reference".into()),
            value: Some("a1".into()),
            code: "duplicate_in_file".into()
        }
    );
}

#[test]
fn sync_archives_the_products_missing_from_the_file_and_undo_brings_them_back() {
    let shop = shop();
    existing(&shop, "A1", "Huile", 100_000);
    existing(&shop, "A2", "Sucre", 13_000);
    let o = options(ImportEntity::Products, ImportPolicy::Sync, PRODUCT_COLUMNS);
    let rows = rows_of(
        &sheet(&[&["r", "n", "p", "u"], &["A1", "Huile", "1190", ""]]),
        &o,
    );
    let job = run(&shop, &o, &rows);
    assert!(by_reference(&shop, "A2").archived);
    let ctx = shop.owner_till();
    shop.db.write(move |tx| undo(tx, &ctx, &job.id)).unwrap();
    assert!(!by_reference(&shop, "A2").archived);
}

#[test]
fn the_mapping_is_checked_before_anything_runs() {
    let fields = super::spec::base_fields(ImportEntity::Customers);
    let bad = |mapping: &[&str]| {
        check_mapping(
            &fields,
            &options(ImportEntity::Customers, ImportPolicy::Upsert, mapping),
        )
        .unwrap_err()
    };
    assert_eq!(
        bad(&["code", "prix"]),
        AppError::validation("mapping", "import_mapping_invalid")
    );
    assert_eq!(
        bad(&["name", "name"]),
        AppError::validation("mapping", "import_mapping_invalid")
    );
    assert_eq!(
        bad(&["code", "phone"]),
        AppError::validation("mapping", "import_mapping_invalid"),
        "name is required"
    );
    let balances = super::spec::base_fields(ImportEntity::CustomerBalances);
    let o = options(
        ImportEntity::CustomerBalances,
        ImportPolicy::CreateOnly,
        &["name", "amount"],
    );
    assert_eq!(
        check_mapping(&balances, &o).unwrap_err(),
        AppError::validation("mapping", "import_mapping_invalid")
    );
}

#[test]
fn customers_are_read_with_types_in_any_language_and_algerian_phones() {
    let shop = shop();
    let o = options(
        ImportEntity::Customers,
        ImportPolicy::CreateOnly,
        &[
            "name",
            "customerType",
            "phone",
            "creditLimit",
            "nif",
            "rc",
            "ai",
            "address",
        ],
    );
    // A reseller carries its NIF, RC, AI and address (CRM-006, CIDTA 183 ter).
    let rows = rows_of(
        &sheet(&[
            &[
                "Nom", "Type", "Tél", "Plafond", "NIF", "RC", "AI", "Adresse",
            ],
            &[
                "Quincaillerie Amine",
                "Revendeur",
                "0550 12 34 56",
                "50 000",
                "000116001234567",
                "16/00-1234567B21",
                "16012345678",
                "12 rue Didouche Mourad, Alger",
            ],
        ]),
        &o,
    );
    let job = run(&shop, &o, &rows);
    assert_eq!(job.created, 1, "{:?}", job.problems);
    let found = shop
        .db
        .read(|c| crate::customers::find_by_phone(c, "0550123456"))
        .unwrap()
        .unwrap();
    assert_eq!(
        (found.customer_type.as_str(), found.credit_limit),
        ("reseller", Some(Money(5_000_000)))
    );
}

/// The product export as the csv writer fills its cells (Task 15), under a header row of its
/// column keys, and the mapping that reads it back (the stock on hand is not an import field).
fn export_grid(shop: &Shop) -> (Vec<Vec<String>>, Vec<Option<String>>) {
    let (table, _) = shop
        .db
        .read(|c| products::export_table(c, crate::lang::Lang::Fr))
        .unwrap();
    let keys: Vec<&str> = table.columns.iter().map(|c| c.key.as_str()).collect();
    let text = |kind: crate::table::ColumnKind, cell: &crate::table::Cell| -> String {
        use crate::table::{Cell, ColumnKind as K};
        match (kind, cell) {
            (K::Money, Cell::Int(v)) => format!("{},{:02}", v / 100, v % 100),
            (K::Qty, Cell::Int(v)) => {
                let s = format!("{},{:03}", v / 1000, v % 1000);
                s.trim_end_matches('0').trim_end_matches(',').to_owned()
            }
            (K::Percent, Cell::Int(v)) => format!("{},{:02}", v / 100, v % 100),
            (_, Cell::Int(v)) => v.to_string(),
            (_, Cell::Text(s)) => s.clone(),
            (_, Cell::Null(())) => String::new(),
        }
    };
    let mut grid = vec![keys.iter().map(|k| (*k).to_owned()).collect::<Vec<_>>()];
    for row in &table.rows {
        grid.push(
            table
                .columns
                .iter()
                .zip(row)
                .map(|(c, cell)| text(c.kind, cell))
                .collect(),
        );
    }
    let mapping = keys
        .iter()
        .map(|k| (*k != "qty").then(|| (*k).to_owned()))
        .collect();
    (grid, mapping)
}

#[test]
fn an_export_reimports_as_updates_without_problems() {
    let shop = shop();
    existing(&shop, "A1", "Huile", 100_000);
    existing(&shop, "A2", "Sucre", 13_000);
    let (grid, mapping) = export_grid(&shop);
    let mut o = options(ImportEntity::Products, ImportPolicy::Upsert, &[]);
    o.mapping = mapping;
    let rows = rows_of(&grid, &o);
    let dry = dry_run(&shop.db, &shop.owner_till(), &o, &rows, &choices()).unwrap();
    assert_eq!(
        (dry.update, dry.create, dry.fail),
        (2, 0, 0),
        "{:?}",
        dry.problems
    );
}

#[test]
fn a_sync_keeps_the_products_named_by_its_refused_rows() {
    let shop = shop();
    existing(&shop, "A1", "Huile", 100_000);
    existing(&shop, "A2", "Sucre", 13_000);
    let o = options(ImportEntity::Products, ImportPolicy::Sync, PRODUCT_COLUMNS);
    let rows = rows_of(
        &sheet(&[
            &["r", "n", "p", "u"],
            &["A1", "Huile", "1190", ""],
            &["A2", "Sucre", "1,234", ""],
        ]),
        &o,
    );
    let dry = dry_run(&shop.db, &shop.owner_till(), &o, &rows, &choices()).unwrap();
    assert_eq!(
        (dry.update, dry.fail, dry.archive),
        (1, 1, 0),
        "A2 is in the file, even though its row is refused: {:?}",
        dry.problems
    );
    let job = run(&shop, &o, &rows);
    assert_eq!(
        (job.updated, job.failed, job.status),
        (1, 1, JobStatus::Done)
    );
    assert!(!by_reference(&shop, "A2").archived);
    assert_eq!(by_reference(&shop, "A2").sell_price, Money(13_000));
}

#[test]
fn a_sync_that_creates_or_updates_nothing_is_refused_and_archives_nothing() {
    let shop = shop();
    existing(&shop, "A1", "Huile", 100_000);
    existing(&shop, "A2", "Sucre", 13_000);
    let o = options(ImportEntity::Products, ImportPolicy::Sync, PRODUCT_COLUMNS);
    let refused = AppError::validation("mapping", "import_mapping_invalid");
    // Every row refused (a wrong column mapped to the price), none naming a product.
    let rows = rows_of(
        &sheet(&[
            &["r", "n", "p", "u"],
            &["X1", "Huile", "1,234", ""],
            &["X2", "Sucre", "9,999", ""],
        ]),
        &o,
    );
    let dry = dry_run(&shop.db, &shop.owner_till(), &o, &rows, &choices()).unwrap();
    assert_eq!(
        (dry.update, dry.create, dry.fail, dry.archive),
        (0, 0, 2, 0)
    );
    let err = apply(
        &shop.db,
        &shop.owner_till(),
        &o,
        &rows,
        &choices(),
        &dry.fingerprint,
        &mut |_| {},
    )
    .unwrap_err();
    assert_eq!(err, refused);
    let jobs = shop.db.read(|c| jobs(c, "2000-01-01", 10)).unwrap();
    assert_eq!(
        (
            jobs.len(),
            jobs[0].status,
            jobs[0].failed,
            jobs[0].problems.len()
        ),
        (1, JobStatus::Failed, 2, 2),
        "the job keeps why its rows were refused"
    );
    // An empty file: refused before any job is written.
    let empty = rows_of(&sheet(&[&["r", "n", "p", "u"]]), &o);
    let dry = dry_run(&shop.db, &shop.owner_till(), &o, &empty, &choices()).unwrap();
    assert_eq!((dry.total, dry.archive), (0, 0));
    let err = apply(
        &shop.db,
        &shop.owner_till(),
        &o,
        &empty,
        &choices(),
        &dry.fingerprint,
        &mut |_| {},
    )
    .unwrap_err();
    assert_eq!(err, refused);
    assert_eq!(shop.count("SELECT COUNT(*) FROM import_jobs"), 1);
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM products WHERE archived_at IS NOT NULL"),
        0
    );
}

#[test]
fn a_sync_of_the_export_leaves_variant_models_active() {
    use crate::catalog::products::ProductKind;
    use crate::catalog::variants::{self, VariantAxis, VariantsInput};
    let shop = shop();
    existing(&shop, "A1", "Huile", 100_000);
    let mut model = ProductInput::simple(
        "T-shirt col rond",
        UNIT_PIECE_ID,
        &shop.tva19,
        Money(150_000),
    );
    model.kind = ProductKind::VariantParent;
    model.track_stock = false;
    model.reference = Some("TS01".into());
    let parent = shop.product_with(model);
    let (actor, id) = (shop.owner.actor(), parent.id.clone());
    let sizes = VariantsInput {
        axes: vec![VariantAxis {
            name: "Taille".into(),
            values: vec!["S".into(), "M".into()],
        }],
        sell_price: None,
        cost_price: None,
        internal_barcodes: true,
    };
    let made = shop
        .db
        .write(move |tx| variants::generate(tx, &actor, &id, sizes))
        .unwrap();
    assert_eq!(made.len(), 2);
    let (grid, mapping) = export_grid(&shop);
    assert_eq!(grid.len(), 4, "A1 and the two variants, never the model");
    let mut o = options(ImportEntity::Products, ImportPolicy::Sync, &[]);
    o.mapping = mapping;
    let rows = rows_of(&grid, &o);
    let dry = dry_run(&shop.db, &shop.owner_till(), &o, &rows, &choices()).unwrap();
    assert_eq!(
        (dry.update, dry.fail, dry.archive),
        (3, 0, 0),
        "{:?}",
        dry.problems
    );
    let job = run(&shop, &o, &rows);
    assert_eq!((job.updated, job.status), (3, JobStatus::Done));
    assert!(!by_reference(&shop, "TS01").archived, "the model stays");
    assert!(!by_reference(&shop, "TS01-S").archived);
    assert_eq!(
        shop.count("SELECT COUNT(*) FROM products WHERE archived_at IS NOT NULL"),
        0
    );
}

#[test]
fn an_excel_error_in_a_text_or_code_cell_is_refused_never_stored() {
    let shop = shop();
    let o = options(
        ImportEntity::Products,
        ImportPolicy::CreateOnly,
        PRODUCT_COLUMNS,
    );
    let rows = rows_of(
        &sheet(&[
            &["r", "n", "p", "u"],
            &["A1", "#N/A", "1190", ""],
            &["#REF!", "Sucre", "130", ""],
            &["A3", "Café", "#VALUE!", ""],
        ]),
        &o,
    );
    let dry = dry_run(&shop.db, &shop.owner_till(), &o, &rows, &choices()).unwrap();
    assert_eq!((dry.create, dry.fail), (0, 3));
    assert_eq!(
        dry.problems,
        vec![
            Problem {
                row: 2,
                field: Some("nameFr".into()),
                value: Some("#N/A".into()),
                code: "value_invalid".into()
            },
            Problem {
                row: 3,
                field: Some("reference".into()),
                value: Some("#REF!".into()),
                code: "value_invalid".into()
            },
            Problem {
                row: 4,
                field: Some("sellPrice".into()),
                value: Some("#VALUE!".into()),
                code: "number_invalid".into()
            },
        ]
    );
}

#[test]
fn a_large_file_is_applied_in_batches_of_500_each_journaled_with_the_job_id() {
    let shop = shop();
    let o = options(
        ImportEntity::Products,
        ImportPolicy::CreateOnly,
        PRODUCT_COLUMNS,
    );
    let mut grid = vec![vec!["r".to_owned(), "n".into(), "p".into(), "u".into()]];
    for i in 0..=BATCH_ROWS {
        grid.push(vec![
            format!("R{i:04}"),
            format!("Article {i}"),
            "100".into(),
            String::new(),
        ]);
    }
    let rows = rows_of(&grid, &o);
    let dry = dry_run(&shop.db, &shop.owner_till(), &o, &rows, &choices()).unwrap();
    assert_eq!((dry.total, dry.create, dry.fail), (501, 501, 0));
    let mut seen = Vec::new();
    let job = apply(
        &shop.db,
        &shop.owner_till(),
        &o,
        &rows,
        &choices(),
        &dry.fingerprint,
        &mut |p| {
            seen.push((p.done, p.total, p.created));
        },
    )
    .unwrap();
    assert_eq!(
        seen,
        vec![(500, 501, 500), (501, 501, 501)],
        "progress after each batch"
    );
    assert_eq!((job.created, job.status), (501, JobStatus::Done));
    let journaled: Vec<String> = shop
        .db
        .read(|c| {
            let mut stmt = c.prepare(
                "SELECT json_extract(payload, '$.jobId') FROM audit_log
                  WHERE action = 'import.applied' ORDER BY seq",
            )?;
            let ids = stmt
                .query_map([], |r| r.get(0))?
                .collect::<Result<Vec<String>, _>>()?;
            Ok(ids)
        })
        .unwrap();
    assert_eq!(
        journaled,
        vec![job.id.clone(), job.id.clone()],
        "one journal row per batch transaction"
    );
    assert_eq!(
        shop.count("SELECT next_row FROM import_jobs"),
        502,
        "the last row applied"
    );
    assert_eq!(shop.count("SELECT COUNT(*) FROM import_job_rows"), 501);
}

/// Goods in at `cost` (the CMUP averages it in, with its `cost_history` row), like a receipt.
fn receive(shop: &Shop, product_id: &str, qty: i64, cost: i64) {
    let input = crate::stock::AdjustInput {
        product_id: product_id.to_owned(),
        location_id: None,
        qty_delta: Qty(qty),
        reason: crate::stock::AdjustReason::Opening,
        unit_cost: Some(Money(cost)),
        note: None,
    };
    let actor = shop.owner.actor();
    shop.db
        .write(move |tx| crate::stock::adjust(tx, &actor, input, crate::testkit::NOON))
        .unwrap();
}

fn cost_of(shop: &Shop, reference: &'static str) -> Money {
    by_reference(shop, reference).cost_price
}

/// A sync never archives a product that holds stock (its stock would leave every report): it
/// keeps it and the dry run names it.
#[test]
fn a_sync_keeps_the_products_that_hold_stock_and_names_them() {
    let shop = shop();
    let oil = existing(&shop, "A1", "Huile", 100_000);
    existing(&shop, "A2", "Sucre", 13_000);
    existing(&shop, "A3", "Thé", 25_000);
    shop.stock_in(&oil.id, 4_000);
    let o = options(ImportEntity::Products, ImportPolicy::Sync, PRODUCT_COLUMNS);
    let rows = rows_of(
        &sheet(&[&["r", "n", "p", "u"], &["A3", "Thé", "260", ""]]),
        &o,
    );
    let dry = dry_run(&shop.db, &shop.owner_till(), &o, &rows, &choices()).unwrap();
    assert_eq!(
        (dry.update, dry.archive, dry.stock_kept),
        (1, 1, 1),
        "A2 archived, A1 kept for its stock"
    );
    assert_eq!(dry.stock_kept_names, vec!["A1 Huile".to_owned()]);
    run(&shop, &o, &rows);
    assert!(!by_reference(&shop, "A1").archived);
    assert!(by_reference(&shop, "A2").archived);
}

/// Undoing a product upsert puts back only what the file set, and the CMUP only while nothing
/// moved it since: a receipt after the import refuses the undo of an imported cost, and leaves
/// the cost alone when the file did not carry one.
#[test]
fn undoing_an_upsert_never_erases_a_cost_a_receipt_moved_since() {
    let shop = shop();
    let mut input = ProductInput::simple("Huile", UNIT_PIECE_ID, &shop.tva19, Money(100_000));
    input.reference = Some("A1".into());
    input.cost_price = Money(50_000);
    shop.product_with(input);
    let with_cost = options(
        ImportEntity::Products,
        ImportPolicy::Upsert,
        &["reference", "sellPrice", "costPrice"],
    );
    let rows = rows_of(
        &sheet(&[&["r", "p", "c"], &["A1", "1200", "800"]]),
        &with_cost,
    );
    let job = run(&shop, &with_cost, &rows);
    assert_eq!(cost_of(&shop, "A1"), Money(80_000));
    // 10 pieces at 1 000 DA: the CMUP moves to 1 000 DA (nothing was held).
    let id = by_reference(&shop, "A1").id;
    receive(&shop, &id, 10_000, 100_000);
    assert_eq!(cost_of(&shop, "A1"), Money(100_000));
    let ctx = shop.owner_till();
    let job_id = job.id.clone();
    assert_eq!(
        shop.db
            .write(move |tx| undo(tx, &ctx, &job_id))
            .unwrap_err(),
        AppError::conflict("import_undo_not_allowed"),
        "the cost the file set has moved since"
    );
    assert_eq!(cost_of(&shop, "A1"), Money(100_000));
    assert_eq!(by_reference(&shop, "A1").sell_price, Money(120_000));

    // A file without a cost: the undo puts the price back and leaves the received cost.
    let prices = options(
        ImportEntity::Products,
        ImportPolicy::Upsert,
        &["reference", "sellPrice"],
    );
    let rows = rows_of(&sheet(&[&["r", "p"], &["A1", "1300"]]), &prices);
    let job = run(&shop, &prices, &rows);
    receive(&shop, &id, 10_000, 120_000);
    assert_eq!(cost_of(&shop, "A1"), Money(110_000));
    let ctx = shop.owner_till();
    shop.db.write(move |tx| undo(tx, &ctx, &job.id)).unwrap();
    let a1 = by_reference(&shop, "A1");
    assert_eq!(
        (a1.sell_price, a1.cost_price),
        (Money(120_000), Money(110_000))
    );
}

/// Undoing an opening-stock import takes the stock back and the CMUP it blended in, with a
/// `cost_history` row; once a receipt moved that CMUP, the undo is refused.
#[test]
fn undoing_opening_stock_restores_the_cost_it_blended_in() {
    let shop = shop();
    let oil = existing(&shop, "A1", "Huile", 100_000);
    let stock = options(
        ImportEntity::OpeningStock,
        ImportPolicy::CreateOnly,
        &["reference", "qty", "unitCost"],
    );
    let rows = rows_of(
        &sheet(&[&["Réf", "Qté", "PA"], &["A1", "12", "800"]]),
        &stock,
    );
    let job = run(&shop, &stock, &rows);
    assert_eq!(cost_of(&shop, "A1"), Money(80_000));
    let ctx = shop.owner_till();
    shop.db.write(move |tx| undo(tx, &ctx, &job.id)).unwrap();
    assert_eq!(cost_of(&shop, "A1"), Money::ZERO);
    let id = oil.id.clone();
    let history = shop
        .db
        .read(move |c| crate::purchases::cost_history(c, &id, 10))
        .unwrap();
    assert_eq!(
        (
            history[0].source_type.as_str(),
            history[0].cost_before,
            history[0].cost_after
        ),
        ("import_undo", Money(80_000), Money::ZERO)
    );

    let job = run(&shop, &stock, &rows);
    receive(&shop, &oil.id, 12_000, 100_000);
    assert_eq!(cost_of(&shop, "A1"), Money(90_000));
    let ctx = shop.owner_till();
    assert_eq!(
        shop.db
            .write(move |tx| undo(tx, &ctx, &job.id))
            .unwrap_err(),
        AppError::conflict("import_undo_not_allowed")
    );
    assert_eq!(cost_of(&shop, "A1"), Money(90_000));
}

/// The field guards of the typed commands hold for imports: without `products.price` an upsert
/// may not name a price, without `reports.profit` the cost, without `credit.view` a limit
/// (`insufficient_permission` in the dry run already); a create-only import may.
#[test]
fn an_upsert_names_only_the_fields_the_user_may_change() {
    use crate::auth::Permission;
    let shop = shop();
    existing(&shop, "A1", "Huile", 100_000);
    // A storekeeper who also keeps the customer file: imports, but no prices, costs or kridi.
    let mut clerk = shop.cashier.clone();
    clerk.permissions = vec![
        Permission::ProductsView,
        Permission::ProductsEdit,
        Permission::ProductsImport,
        Permission::CustomersView,
        Permission::CustomersManage,
    ];
    let clerk = crate::till::TillContext::new(
        clerk,
        std::sync::Arc::clone(&shop.pack),
        crate::testkit::NOON,
    );
    for (entity, mapping, permission) in [
        (
            ImportEntity::Products,
            &["reference", "sellPrice"][..],
            "products.price",
        ),
        (
            ImportEntity::Products,
            &["reference", "price.GROS"][..],
            "products.price",
        ),
        (
            ImportEntity::Products,
            &["reference", "costPrice"][..],
            "reports.profit",
        ),
        (
            ImportEntity::Customers,
            &["name", "creditLimit"][..],
            "credit.view",
        ),
    ] {
        for policy in [ImportPolicy::Upsert, ImportPolicy::Sync] {
            let o = options(entity, policy, mapping);
            assert_eq!(
                check_field_permissions(&clerk, &o).unwrap_err(),
                AppError::Permission {
                    permission: permission.to_owned()
                }
            );
            check_field_permissions(&shop.owner_till(), &o).unwrap();
        }
        let create_only = options(entity, ImportPolicy::CreateOnly, mapping);
        check_field_permissions(&clerk, &create_only).unwrap();
    }
}

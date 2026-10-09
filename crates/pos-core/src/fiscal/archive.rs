//! The fiscal archive (AUD-010, RPT-061, DAT-022, AUD-011 technical part; LF 2026 art. 74 and 82:
//! computerised books kept in electronic form, data and documentation until the end of the
//! sixth year, vouchers ten years): the documents of a period with their lines, taxes,
//! payments, parties and links, the audit journal of the period with its hashes, the Z
//! closures, the versement receipts, the counters with their journaled skips, the tills'
//! journals kept on the main PC and the company — as text rows the zip writer (pos-docs)
//! streams. Every part, the schema and the chain checks of one archive are read in one read
//! transaction ([`in_snapshot`]): one state of the books, whatever commits meanwhile.

use crate::audit::{self, Actor, ChainReport};
use crate::cash::closing::{ClosureChainReport, verify_closures};
use crate::customers::receipts::ReceiptChainReport;
use crate::documents::{DocChainReport, verify_all};
use crate::error::AppError;
use crate::party::clean_day;
use chrono::NaiveDate;
use rusqlite::types::ValueRef;
use rusqlite::{Connection, ToSql, Transaction};
use serde::Serialize;
use serde_json::json;
use specta::Type;

/// The longest period of one archive, in days (about five years).
pub const MAX_DAYS: i64 = 1_830;

/// One file of the archive, one table (or its rows of the period).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ArchivePart {
    Company,
    Documents,
    DocumentLines,
    DocumentTaxes,
    Payments,
    DocumentParties,
    DocumentLinks,
    AuditLog,
    CashClosures,
    Counters,
    /// The versement receipts `RV…` of the period (customer ledger rows with a number).
    Receipts,
    /// Every journaled skip of the counters (a restore's numbers never issued).
    CounterSkips,
    /// The tills' journals the main PC keeps (M5), rows of the period; empty on a single PC.
    LanJournal,
}

impl ArchivePart {
    pub const ALL: [ArchivePart; 13] = [
        ArchivePart::Company,
        ArchivePart::Documents,
        ArchivePart::DocumentLines,
        ArchivePart::DocumentTaxes,
        ArchivePart::Payments,
        ArchivePart::DocumentParties,
        ArchivePart::DocumentLinks,
        ArchivePart::AuditLog,
        ArchivePart::CashClosures,
        ArchivePart::Counters,
        ArchivePart::Receipts,
        ArchivePart::CounterSkips,
        ArchivePart::LanJournal,
    ];

    pub const fn file_name(self) -> &'static str {
        match self {
            ArchivePart::Company => "company.csv",
            ArchivePart::Documents => "documents.csv",
            ArchivePart::DocumentLines => "document_lines.csv",
            ArchivePart::DocumentTaxes => "document_taxes.csv",
            ArchivePart::Payments => "payments.csv",
            ArchivePart::DocumentParties => "document_parties.csv",
            ArchivePart::DocumentLinks => "document_links.csv",
            ArchivePart::AuditLog => "audit_log.csv",
            ArchivePart::CashClosures => "cash_closures.csv",
            ArchivePart::Counters => "counters.csv",
            ArchivePart::Receipts => "receipts.csv",
            ArchivePart::CounterSkips => "counter_skips.csv",
            ArchivePart::LanJournal => "lan_journal.csv",
        }
    }

    fn sql(self) -> &'static str {
        match self {
            ArchivePart::Company => {
                "SELECT name_ar, name_fr, name_en, legal_form, capital, address, wilaya, phone, email, rc, nif, nis, ai,
                        rib, tax_regime, price_mode, default_doc_lang, updated_at
                 FROM company"
            }
            ArchivePart::Documents => {
                "SELECT * FROM documents WHERE status = 'validated' AND business_date BETWEEN :day_from AND :day_to
                 ORDER BY doc_type, series, chain_seq"
            }
            ArchivePart::DocumentLines => {
                "SELECT * FROM document_lines WHERE document_id IN
                   (SELECT id FROM documents WHERE status = 'validated' AND business_date BETWEEN :day_from AND :day_to)
                 ORDER BY document_id, line_no"
            }
            ArchivePart::DocumentTaxes => {
                "SELECT * FROM document_taxes WHERE document_id IN
                   (SELECT id FROM documents WHERE status = 'validated' AND business_date BETWEEN :day_from AND :day_to)
                 ORDER BY document_id, rate_bp"
            }
            ArchivePart::Payments => {
                "SELECT * FROM payments WHERE document_id IN
                   (SELECT id FROM documents WHERE status = 'validated' AND business_date BETWEEN :day_from AND :day_to)
                 ORDER BY document_id, created_at, id"
            }
            ArchivePart::DocumentParties => {
                "SELECT * FROM document_parties WHERE document_id IN
                   (SELECT id FROM documents WHERE status = 'validated' AND business_date BETWEEN :day_from AND :day_to)
                 ORDER BY document_id, role"
            }
            ArchivePart::DocumentLinks => {
                "SELECT k.* FROM document_links k WHERE k.to_id IN
                   (SELECT id FROM documents WHERE status = 'validated' AND business_date BETWEEN :day_from AND :day_to)
                 ORDER BY k.created_at, k.id"
            }
            ArchivePart::AuditLog => {
                "SELECT * FROM audit_log WHERE at >= :at_from AND at < :at_to ORDER BY seq"
            }
            ArchivePart::CashClosures => {
                "SELECT * FROM cash_closures WHERE business_date BETWEEN :day_from AND :day_to
                 ORDER BY terminal_id, z_seq"
            }
            ArchivePart::Counters => "SELECT * FROM counters ORDER BY doc_type, series, fiscal_year",
            // What `customers::receipts::verify` compares with the journal's credit.collected row.
            ArchivePart::Receipts => {
                "SELECT receipt_number, id, customer_id, amount, method, stamp_duty, stamp_charged,
                        balance_after, terminal_id, business_date, created_at
                 FROM customer_ledger
                 WHERE receipt_number IS NOT NULL AND business_date BETWEEN :day_from AND :day_to
                 ORDER BY receipt_number"
            }
            ArchivePart::CounterSkips => {
                "SELECT * FROM counter_skips ORDER BY doc_type, series, fiscal_year, from_number"
            }
            ArchivePart::LanJournal => {
                "SELECT * FROM lan_journal WHERE at >= :at_from AND at < :at_to ORDER BY merge_seq"
            }
        }
    }
}

/// The period of an archive, both days included (`YYYY-MM-DD`, Algiers business days).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveScope {
    pub from: String,
    pub to: String,
}

/// `required` for an empty day, `date_invalid` (on its own field) for a day that is not
/// `YYYY-MM-DD`; `archive_range_invalid` when `to` is before `from` or the period reaches
/// [`MAX_DAYS`]. The scope holds the canonical days (`2026-9-1` becomes `2026-09-01`): the
/// parts compare them to `business_date` as text, and the manifest and journal record them.
pub fn check_scope(from: &str, to: &str) -> Result<ArchiveScope, AppError> {
    let (from, first) = scope_day(from, "from")?;
    let (to, last) = scope_day(to, "to")?;
    if !(0..MAX_DAYS).contains(&(last - first).num_days()) {
        return Err(AppError::validation("to", "archive_range_invalid"));
    }
    Ok(ArchiveScope { from, to })
}

/// One end of the period: the canonical `YYYY-MM-DD` text and its date.
fn scope_day(value: &str, field: &str) -> Result<(String, NaiveDate), AppError> {
    let day =
        clean_day(Some(value), field)?.ok_or_else(|| AppError::validation(field, "required"))?;
    let date = NaiveDate::parse_from_str(&day, "%Y-%m-%d")
        .map_err(|_| AppError::validation(field, "date_invalid"))?;
    Ok((day, date))
}

/// What [`dump`] wrote: the column names and the number of rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PartDump {
    pub header: Vec<String>,
    pub rows: i64,
}

/// The instant `day` starts in Algiers, in Unix milliseconds.
fn algiers_midnight(day: &str) -> Result<i64, AppError> {
    let d = NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .map_err(|_| AppError::validation("from", "date_invalid"))?;
    let utc = d
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| AppError::validation("from", "date_invalid"))?
        .and_utc()
        .timestamp_millis();
    Ok(utc - crate::time::ALGIERS_OFFSET_MS)
}

/// A cell as text: `NULL` is empty, a blob never leaves (the columns archived hold none).
fn text(v: ValueRef<'_>) -> String {
    match v {
        ValueRef::Null | ValueRef::Blob(_) => String::new(),
        ValueRef::Integer(i) => i.to_string(),
        ValueRef::Real(f) => f.to_string(),
        ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned(),
    }
}

/// The column names of `part`: the CSV header, written before the rows are streamed.
pub fn header(conn: &Connection, part: ArchivePart) -> Result<Vec<String>, AppError> {
    let stmt = conn.prepare(part.sql())?;
    Ok(stmt
        .column_names()
        .iter()
        .map(|c| (*c).to_owned())
        .collect())
}

/// Hands every row of `part` in `scope` to `visit`, one at a time (nothing is collected: a
/// five-year journal streams from SQLite to the zip with flat memory).
pub fn dump(
    conn: &Connection,
    part: ArchivePart,
    scope: &ArchiveScope,
    visit: &mut dyn FnMut(&[String]) -> Result<(), AppError>,
) -> Result<PartDump, AppError> {
    let at_from = algiers_midnight(&scope.from)?;
    let at_to = algiers_midnight(&scope.to)? + 86_400_000;
    let sql = part.sql();
    let all: [(&str, &dyn ToSql); 4] = [
        (":day_from", &scope.from),
        (":day_to", &scope.to),
        (":at_from", &at_from),
        (":at_to", &at_to),
    ];
    // SQLite refuses a named parameter the statement does not have.
    let params: Vec<(&str, &dyn ToSql)> =
        all.into_iter().filter(|(n, _)| sql.contains(n)).collect();
    let mut stmt = conn.prepare(sql)?;
    let header: Vec<String> = stmt
        .column_names()
        .iter()
        .map(|c| (*c).to_owned())
        .collect();
    let width = header.len();
    let mut rows = stmt.query(params.as_slice())?;
    let mut count = 0_i64;
    while let Some(row) = rows.next()? {
        let cells = (0..width)
            .map(|i| row.get_ref(i).map(text))
            .collect::<Result<Vec<_>, _>>()?;
        visit(&cells)?;
        count += 1;
    }
    Ok(PartDump {
        header,
        rows: count,
    })
}

/// Runs `read` in one read transaction: every query sees the same state of the database (a WAL
/// snapshot taken at its first read), whatever the writer commits meanwhile. Already inside a
/// transaction, `read` runs in it.
pub fn in_snapshot<T>(
    conn: &Connection,
    read: impl FnOnce(&Connection) -> Result<T, AppError>,
) -> Result<T, AppError> {
    if !conn.is_autocommit() {
        return read(conn);
    }
    let tx = conn.unchecked_transaction()?;
    // A deferred transaction takes its snapshot at its first read: take it now.
    tx.query_row("SELECT COUNT(*) FROM sqlite_master", [], |r| {
        r.get::<_, i64>(0)
    })?;
    let out = read(&tx)?;
    tx.commit()?;
    Ok(out)
}

/// The schema of the database (the data dictionary part of AUD-011).
pub fn schema_sql(conn: &Connection) -> Result<String, AppError> {
    let mut stmt = conn.prepare(
        "SELECT sql FROM sqlite_master WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%'
         ORDER BY type DESC, name",
    )?;
    let parts = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(parts.join(";\n\n") + ";\n")
}

/// The Z closure chain of one terminal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TerminalClosures {
    pub terminal_id: String,
    pub terminal_code: String,
    pub report: ClosureChainReport,
}

/// Every chain checked on the day of the export (`chains.json`, the manifest, the summary).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChainSummary {
    pub documents: Vec<DocChainReport>,
    pub audit: ChainReport,
    pub closures: Vec<TerminalClosures>,
    /// The versement receipts `RV…` (outside `documents`, anchored in the journal).
    pub receipts: ReceiptChainReport,
    /// The journals of the tills this main PC keeps (M5), revoked ones included: evidence.
    pub till_journals: Vec<TillJournalChain>,
    /// The first `merge_seq` of the main PC's chain over every till's journal that does not
    /// recompute (`sync::journal::verify_merge`), `None` when it is intact or empty.
    pub till_merge_first_broken: Option<i64>,
}

/// The tills' journals this main PC keeps, each checked, and its merge chain over them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TillJournals {
    pub tills: Vec<TillJournalChain>,
    pub merge_first_broken: Option<i64>,
}

/// Every till journal the main PC keeps (`sync::journal::verify`, revoked tills included) and
/// the merge chain (`sync::journal::verify_merge`). Empty off a main PC.
pub fn till_journals(conn: &Connection) -> Result<TillJournals, AppError> {
    let mut stmt = conn.prepare(
        "SELECT d.terminal_id, t.code FROM lan_devices d JOIN terminals t ON t.id = d.terminal_id
         ORDER BY t.code",
    )?;
    let devices = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut tills = Vec::new();
    for (id, code) in devices {
        tills.push(TillJournalChain {
            report: crate::sync::journal::verify(conn, &id)?,
            terminal_id: id,
            terminal_code: code,
        });
    }
    Ok(TillJournals {
        tills,
        merge_first_broken: crate::sync::journal::verify_merge(conn)?,
    })
}

/// One till's journal as the main PC keeps it (`sync::journal::verify`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TillJournalChain {
    pub terminal_id: String,
    pub terminal_code: String,
    pub report: crate::sync::journal::JournalReport,
}

/// Verifies every document chain, the audit journal, the Z chain of every terminal and the
/// versement receipts, all in one snapshot ([`in_snapshot`]).
pub fn chains(conn: &Connection) -> Result<ChainSummary, AppError> {
    in_snapshot(conn, chains_now)
}

fn chains_now(conn: &Connection) -> Result<ChainSummary, AppError> {
    let mut stmt = conn.prepare("SELECT id, code FROM terminals ORDER BY code")?;
    let terminals = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut closures = Vec::new();
    for (id, code) in terminals {
        closures.push(TerminalClosures {
            report: verify_closures(conn, &id)?,
            terminal_id: id,
            terminal_code: code,
        });
    }
    let journals = till_journals(conn)?;
    Ok(ChainSummary {
        documents: verify_all(conn)?,
        audit: audit::verify_chain(conn)?,
        closures,
        receipts: crate::customers::receipts::verify(conn)?,
        till_journals: journals.tills,
        till_merge_first_broken: journals.merge_first_broken,
    })
}

/// Anchors an exported archive in the hash-chained journal: its manifest's SHA-256 and the
/// hash of every file, so the archive can later be proven to be the one exported that day.
pub fn record_export(
    tx: &Transaction<'_>,
    actor: &Actor,
    scope: &ArchiveScope,
    manifest_sha256: &str,
    files: &[(String, String)],
) -> Result<(), AppError> {
    let files: Vec<_> = files
        .iter()
        .map(|(name, sha)| json!({ "name": name, "sha256": sha }))
        .collect();
    audit::append(
        tx,
        &actor.event(
            "fiscal.archive_exported",
            "fiscal_archive",
            None,
            json!({ "from": scope.from, "to": scope.to, "manifestSha256": manifest_sha256, "files": files }),
        ),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::{Actor, ChainFields, GENESIS_HASH};
    use crate::documents::DocType;
    use crate::documents::tests_support::validated_of;
    use crate::testkit::shop;

    fn collect(
        shop: &crate::testkit::Shop,
        part: ArchivePart,
        from: &str,
        to: &str,
    ) -> (PartDump, Vec<Vec<String>>) {
        let (from, to) = (from.to_owned(), to.to_owned());
        shop.db
            .read(move |c| {
                let mut rows = Vec::new();
                let dump = dump(c, part, &check_scope(&from, &to)?, &mut |r| {
                    rows.push(r.to_vec());
                    Ok(())
                })?;
                Ok((dump, rows))
            })
            .unwrap()
    }

    #[test]
    fn the_archive_holds_every_part_of_the_period_and_nothing_else() {
        let shop = shop();
        validated_of(&shop.db, DocType::Invoice);
        validated_of(&shop.db, DocType::Quote);
        crate::documents::tests_support::invoice_draft(&shop.db);
        let (docs, rows) = collect(&shop, ArchivePart::Documents, "2026-09-01", "2026-09-30");
        assert_eq!(docs.rows, 2, "validated documents only");
        let hash = docs.header.iter().position(|h| h == "hash").unwrap();
        assert!(rows.iter().all(|r| r[hash].len() == 64));
        let (_, none) = collect(&shop, ArchivePart::Documents, "2026-10-01", "2026-10-31");
        assert!(none.is_empty());
        assert_eq!(
            shop.db.read(|c| header(c, ArchivePart::Documents)).unwrap(),
            docs.header,
            "the header comes before the rows"
        );
        let (company, rows) = collect(&shop, ArchivePart::Company, "2026-09-01", "2026-09-30");
        assert!(
            !company.header.contains(&"logo".to_owned()),
            "the logo never leaves"
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(
            check_scope("2026-09-30", "2026-09-01").unwrap_err().code(),
            "archive_range_invalid"
        );
        let sql = shop.db.read(schema_sql).unwrap();
        assert!(
            sql.contains("CREATE TABLE documents")
                && sql.contains("CREATE TRIGGER document_parties_draft_update")
        );
    }

    #[test]
    fn a_loose_day_is_stored_canonical_so_the_period_finds_its_documents() {
        let september = ArchiveScope {
            from: "2026-09-01".into(),
            to: "2026-09-30".into(),
        };
        assert_eq!(
            check_scope("2026-9-1", "2026-9-30").unwrap(),
            september,
            "the manifest and the journal record the canonical days"
        );
        assert_eq!(
            check_scope(" 2026-09-01", "2026-09-30 ").unwrap(),
            september
        );
        let shop = shop();
        validated_of(&shop.db, DocType::Invoice);
        validated_of(&shop.db, DocType::Quote);
        let (docs, _) = collect(&shop, ArchivePart::Documents, "2026-9-1", "2026-9-30");
        assert_eq!(docs.rows, 2, "business_date is compared to canonical days");
        let (lines, _) = collect(&shop, ArchivePart::DocumentLines, "2026-9-1", "2026-9-30");
        assert!(lines.rows > 0);
        assert_eq!(
            check_scope("2026-09-01", "30/09/2026").unwrap_err(),
            AppError::validation("to", "date_invalid"),
            "a malformed end is reported on its own field"
        );
        assert_eq!(
            check_scope("", "2026-09-30").unwrap_err(),
            AppError::validation("from", "required")
        );
        assert_eq!(
            check_scope("2026-09-01", " ").unwrap_err(),
            AppError::validation("to", "required")
        );
    }

    #[test]
    fn the_audit_part_carries_the_hashes_that_verify_the_chain() {
        let shop = shop();
        validated_of(&shop.db, DocType::Invoice);
        // The journal is written at the wall clock: a period of about five years around today.
        let today = crate::time::algiers_datetime(crate::time::now_ms()).date();
        let from = today - chrono::Days::new(900);
        let to = today + chrono::Days::new(900);
        let (dump, rows) = collect(
            &shop,
            ArchivePart::AuditLog,
            &from.to_string(),
            &to.to_string(),
        );
        assert!(!rows.is_empty(), "the shop's setup is journaled");
        let col = |name: &str| dump.header.iter().position(|h| h == name).unwrap();
        let mut prev = GENESIS_HASH.to_owned();
        for r in &rows {
            let opt = |i: usize| (!r[i].is_empty()).then_some(r[i].as_str());
            assert_eq!(r[col("prev_hash")], prev);
            let hash = ChainFields {
                prev_hash: &r[col("prev_hash")],
                seq: r[col("seq")].parse().unwrap(),
                at: r[col("at")].parse().unwrap(),
                user_id: opt(col("user_id")),
                terminal_id: opt(col("terminal_id")),
                action: &r[col("action")],
                entity_type: &r[col("entity_type")],
                entity_id: opt(col("entity_id")),
                payload: &r[col("payload")],
            }
            .hash();
            assert_eq!(hash, r[col("hash")], "row {}", r[col("seq")]);
            prev = hash;
        }
    }

    #[test]
    fn an_export_is_anchored_in_the_journal() {
        let shop = shop();
        let actor = Actor::system(None);
        shop.db
            .write(move |tx| {
                record_export(
                    tx,
                    &actor,
                    &check_scope("2026-01-01", "2026-12-31")?,
                    &"f".repeat(64),
                    &[("documents.csv".into(), "a".repeat(64))],
                )
            })
            .unwrap();
        assert_eq!(
            shop.count(
                "SELECT COUNT(*) FROM audit_log WHERE action = 'fiscal.archive_exported' AND payload LIKE '%ffffffff%'"
            ),
            1
        );
        let summary = shop.db.read(chains).unwrap();
        assert_eq!(summary.audit.first_broken_seq, None);
    }

    #[test]
    fn the_archive_carries_the_receipts_the_skips_and_the_tills_journals() {
        use crate::customers::credit::{self, CollectInput};
        let shop = shop();
        let karim = shop.customer("Karim");
        let (ctx, id) = (shop.owner_till(), karim.id.clone());
        shop.db
            .write(move |tx| credit::set_opening(tx, &ctx, &id, crate::money::Money(500_000), None))
            .unwrap();
        shop.open_till(0);
        let ctx = shop.till();
        let versement = CollectInput {
            customer_id: karim.id,
            amount: crate::money::Money(200_000),
            method: crate::payments::PaymentMethod::Cash,
            reference: None,
            cheque_bank: None,
            cheque_date: None,
            allocate_to: Vec::new(),
            note: None,
        };
        shop.db
            .write(move |tx| credit::collect(tx, &ctx, versement))
            .unwrap();
        let (receipts, rows) = collect(&shop, ArchivePart::Receipts, "2026-09-01", "2026-09-30");
        assert_eq!(receipts.rows, 1);
        let col = |name: &str| receipts.header.iter().position(|h| h == name).unwrap();
        assert_eq!(rows[0][col("receipt_number")], "RV2026-000001");
        assert_eq!(rows[0][col("amount")], "-200000");
        assert!(receipts.header.iter().any(|h| h == "stamp_duty"));
        let (none, _) = collect(&shop, ArchivePart::Receipts, "2026-10-01", "2026-10-31");
        assert_eq!(none.rows, 0, "the receipts of the period only");
        let (skips, _) = collect(&shop, ArchivePart::CounterSkips, "2026-09-01", "2026-09-30");
        assert!(skips.header.iter().any(|h| h == "from_number"));
        assert_eq!(skips.rows, 0, "no restore skipped a number");
        let (journal, _) = collect(&shop, ArchivePart::LanJournal, "2026-09-01", "2026-09-30");
        assert!(journal.header.iter().any(|h| h == "merge_hash"));
        assert_eq!(journal.rows, 0, "empty on a single PC");
        assert_eq!(
            ArchivePart::ALL
                .iter()
                .map(|p| p.file_name())
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            ArchivePart::ALL.len()
        );
    }

    #[test]
    fn every_part_of_an_archive_is_read_in_one_snapshot() {
        use crate::db::{Db, DbOptions};
        use crate::money::{Money, Qty};
        use crate::payments::PaymentInput;
        use crate::sales::{Cart, CartLine, SaleInput, create_sale};
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(
            &dir.path().join("pos.db"),
            DbOptions {
                readers: 2,
                backup_dir: Some(dir.path().join("backups")),
            },
        )
        .unwrap();
        let shop = crate::testkit::shop_in(
            db,
            crate::company::TaxRegime::Reel,
            crate::company::PriceMode::Ttc,
        );
        let milk = shop.product("Lait", Money(4_500), &shop.tva19);
        shop.open_till(0);
        let sell = {
            let (db, ctx, id) = (shop.db.clone(), shop.till(), milk.id.clone());
            move || {
                let input = SaleInput {
                    cart: Cart::new(
                        crate::lang::Lang::Fr,
                        vec![CartLine::product(&id, Qty(1_000))],
                    ),
                    payments: vec![PaymentInput::cash(Money(5_000))],
                };
                let ctx = ctx.clone();
                db.write(move |tx| create_sale(tx, &ctx, input).map(drop))
            }
        };
        sell().unwrap();
        let scope = check_scope("2026-09-01", "2026-09-30").unwrap();
        let (documents, lines) = shop
            .db
            .read(move |c| {
                in_snapshot(c, |c| {
                    let mut ids = Vec::new();
                    let docs = dump(c, ArchivePart::Documents, &scope, &mut |r| {
                        ids.push(r[0].clone());
                        Ok(())
                    })?;
                    // A sale commits between two parts of the archive.
                    sell()?;
                    let mut owners = Vec::new();
                    dump(c, ArchivePart::DocumentLines, &scope, &mut |r| {
                        owners.push(r.to_vec());
                        Ok(())
                    })?;
                    let at = header(c, ArchivePart::DocumentLines)?
                        .iter()
                        .position(|h| h == "document_id")
                        .unwrap_or_default();
                    let owners: Vec<String> = owners.into_iter().map(|r| r[at].clone()).collect();
                    assert_eq!(docs.header[0], "id");
                    Ok((ids, owners))
                })
            })
            .unwrap();
        assert_eq!(
            documents.len(),
            1,
            "the sale made meanwhile is not in this archive"
        );
        assert!(
            lines.iter().all(|id| documents.contains(id)),
            "every line's document is in documents.csv"
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM documents WHERE doc_type = 'ticket'"),
            2,
            "the sale was made"
        );
    }

    #[test]
    fn the_chains_cover_the_versement_receipts() {
        use crate::customers::credit::{self, CollectInput};
        let shop = shop();
        let karim = shop.customer("Karim");
        let (ctx, id) = (shop.owner_till(), karim.id.clone());
        shop.db
            .write(move |tx| credit::set_opening(tx, &ctx, &id, crate::money::Money(500_000), None))
            .unwrap();
        shop.open_till(0);
        let ctx = shop.till();
        let versement = CollectInput {
            customer_id: karim.id,
            amount: crate::money::Money(200_000),
            method: crate::payments::PaymentMethod::Cash,
            reference: None,
            cheque_bank: None,
            cheque_date: None,
            allocate_to: Vec::new(),
            note: None,
        };
        shop.db
            .write(move |tx| credit::collect(tx, &ctx, versement))
            .unwrap();
        let summary = shop.db.read(chains).unwrap();
        assert_eq!(
            (summary.receipts.rows, summary.receipts.first_broken_number),
            (1, None)
        );
    }
}

//! Gap-free document numbering (spec §5.2, CFG-005, DOC-001): one counter per
//! (document type, series, fiscal year), incremented inside the validating transaction, so a
//! failed validation rolls the number back with everything else. Ids never drive numbering.
//! The one gap there can be is a journaled skip: after a restore, the counters jump past the
//! numbers the replaced database had printed ([`fast_forward`], [`journaled_skips`]).

use crate::error::AppError;
use crate::ids::new_id;
use crate::sqlenum::sql_text_enum;
use chrono::{Datelike, NaiveDate};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum DocType {
    Ticket,
    Invoice,
    Proforma,
    Quote,
    DeliveryNote,
    CreditNote,
    ReturnNote,
    CustomerOrder,
    PurchaseOrder,
    GoodsReceipt,
    SupplierInvoice,
    Transfer,
}

impl DocType {
    pub const ALL: [DocType; 12] = [
        DocType::Ticket,
        DocType::Invoice,
        DocType::Proforma,
        DocType::Quote,
        DocType::DeliveryNote,
        DocType::CreditNote,
        DocType::ReturnNote,
        DocType::CustomerOrder,
        DocType::PurchaseOrder,
        DocType::GoodsReceipt,
        DocType::SupplierInvoice,
        DocType::Transfer,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            DocType::Ticket => "ticket",
            DocType::Invoice => "invoice",
            DocType::Proforma => "proforma",
            DocType::Quote => "quote",
            DocType::DeliveryNote => "delivery_note",
            DocType::CreditNote => "credit_note",
            DocType::ReturnNote => "return_note",
            DocType::CustomerOrder => "customer_order",
            DocType::PurchaseOrder => "purchase_order",
            DocType::GoodsReceipt => "goods_receipt",
            DocType::SupplierInvoice => "supplier_invoice",
            DocType::Transfer => "transfer",
        }
    }

    pub fn parse(s: &str) -> Option<DocType> {
        DocType::ALL.into_iter().find(|t| t.as_str() == s)
    }

    /// Number prefix of spec §5.2 (`CC` for customer orders and `BT` for transfers, which the
    /// spec table does not list).
    pub const fn prefix(self) -> &'static str {
        match self {
            DocType::Ticket => "TK",
            DocType::Invoice => "FA",
            DocType::Proforma => "PF",
            DocType::Quote => "DV",
            DocType::DeliveryNote => "BL",
            DocType::CreditNote => "AV",
            DocType::ReturnNote => "BR",
            DocType::CustomerOrder => "CC",
            DocType::PurchaseOrder => "BC",
            DocType::GoodsReceipt => "RC",
            DocType::SupplierInvoice => "FF",
            DocType::Transfer => "BT",
        }
    }

    /// Tickets and return notes are numbered per terminal, so offline tills never collide
    /// (spec §4.6); every other document per company.
    pub const fn per_terminal(self) -> bool {
        matches!(self, DocType::Ticket | DocType::ReturnNote)
    }

    /// The series of a document of this type made on `terminal_code`.
    pub fn series(self, terminal_code: &str) -> String {
        if self.per_terminal() {
            terminal_code.to_owned()
        } else {
            String::new()
        }
    }
}

sql_text_enum!(DocType);

/// Counter key of the Z reports: one per terminal, never reset (AUD-005).
pub const Z_COUNTER: &str = "z_report";

/// The counters of [`assign_custom`] and the prefix their numbers are printed with: versement
/// receipts (M2), then the M8 packs' numbered documents and the pharmacy register's own counter
/// (so a restore warning prints every pack number as the pack prints it).
pub const CUSTOM_COUNTERS: [(&str, &str); 9] = [
    ("credit_receipt", "RV"),
    ("yield_run", "DC"),
    ("service_ticket", "RP"),
    ("deposit", "AC"),
    ("instalment_plan", "CT"),
    ("gift_voucher", "BA"),
    ("delivery_round", "TR"),
    ("zakat_statement", "ZK"),
    // The controlled medicines register (`pharmacy::register`): `2026-000012`, no prefix.
    ("controlled_register", ""),
];

/// The never-resetting codes of [`next_code`]: no printed document carries them.
pub const CODE_COUNTERS: [&str; 3] = ["customer_code", "supplier_code", "product_reference"];

/// The prefix a custom counter's numbers are printed with.
pub fn custom_prefix(counter: &str) -> Option<&'static str> {
    CUSTOM_COUNTERS
        .iter()
        .find(|(key, _)| *key == counter)
        .map(|(_, prefix)| *prefix)
}

/// `RV2026-000001`: a number of [`assign_custom`].
pub fn format_custom(prefix: &str, year: i32, n: i64) -> String {
    format!("{prefix}{year}-{n:06}")
}

/// The next number of a counter, inside the caller's transaction. The first call of a key
/// creates it at 1; the triggers of migration 0002 forbid any other change.
pub fn next_number(
    tx: &Transaction<'_>,
    counter: &str,
    series: &str,
    fiscal_year: i32,
) -> Result<i64, AppError> {
    Ok(tx
        .prepare_cached(
            "INSERT INTO counters (doc_type, series, fiscal_year, last_number) VALUES (?1, ?2, ?3, 1)
             ON CONFLICT (doc_type, series, fiscal_year) DO UPDATE SET last_number = last_number + 1
             RETURNING last_number",
        )?
        .query_row(params![counter, series, fiscal_year], |r| r.get(0))?)
}

/// The last number handed out for a key (0 when none).
pub fn last_number(
    conn: &Connection,
    counter: &str,
    series: &str,
    fiscal_year: i32,
) -> Result<i64, AppError> {
    Ok(conn.query_row(
        "SELECT COALESCE((SELECT last_number FROM counters
                          WHERE doc_type = ?1 AND series = ?2 AND fiscal_year = ?3), 0)",
        params![counter, series, fiscal_year],
        |r| r.get(0),
    )?)
}

/// `TK2026-C1-000123` (per terminal) or `FA2026-000045` (company).
pub fn format_number(doc_type: DocType, series: &str, year: i32, n: i64) -> String {
    if series.is_empty() {
        format!("{}{year:04}-{n:06}", doc_type.prefix())
    } else {
        format!("{}{year:04}-{series}-{n:06}", doc_type.prefix())
    }
}

/// `ZC1-000031`.
pub fn format_z_number(terminal_code: &str, n: i64) -> String {
    format!("Z{terminal_code}-{n:06}")
}

/// A number handed out to a document being validated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Assigned {
    pub number: String,
    pub seq: i64,
    pub fiscal_year: i32,
}

/// Numbers a document of `doc_type` in `series` dated `business_date`: the fiscal year is the
/// calendar year of the business date; the counter restarts each year unless the setting
/// `numbering.restart_yearly` is off (then one counter runs across years, key year 0). The
/// setting is frozen once a document is numbered (`settings::set_value`), so the key never
/// switches under existing numbers.
pub fn assign(
    tx: &Transaction<'_>,
    doc_type: DocType,
    series: &str,
    business_date: NaiveDate,
) -> Result<Assigned, AppError> {
    let year = business_date.year();
    let restart: bool = crate::settings::get(tx, "numbering.restart_yearly")?;
    let key_year = if restart { year } else { 0 };
    let seq = next_number(tx, doc_type.as_str(), series, key_year)?;
    Ok(Assigned {
        number: format_number(doc_type, series, year, seq),
        seq,
        fiscal_year: year,
    })
}

/// A company-wide number outside the document types (versement receipts `RV2026-000001`):
/// gap-free inside the caller's transaction, restarting each fiscal year unless
/// `numbering.restart_yearly` is off.
pub fn assign_custom(
    tx: &Transaction<'_>,
    counter: &str,
    prefix: &str,
    business_date: NaiveDate,
) -> Result<Assigned, AppError> {
    let year = business_date.year();
    let restart: bool = crate::settings::get(tx, "numbering.restart_yearly")?;
    let seq = next_number(tx, counter, "", if restart { year } else { 0 })?;
    Ok(Assigned {
        number: format_custom(prefix, year, seq),
        seq,
        fiscal_year: year,
    })
}

/// [`assign_custom`] in a series: a till's own receipts (`RV2026-C2-000001`, M5) never collide
/// with another till's offline ones; an empty series is the company's (`RV2026-000001`).
pub fn assign_custom_in(
    tx: &Transaction<'_>,
    counter: &str,
    prefix: &str,
    series: &str,
    business_date: NaiveDate,
) -> Result<Assigned, AppError> {
    if series.is_empty() {
        return assign_custom(tx, counter, prefix, business_date);
    }
    let year = business_date.year();
    let restart: bool = crate::settings::get(tx, "numbering.restart_yearly")?;
    let seq = next_number(tx, counter, series, if restart { year } else { 0 })?;
    Ok(Assigned {
        number: format!("{prefix}{year}-{series}-{seq:06}"),
        seq,
        fiscal_year: year,
    })
}

/// A never-resetting code (`C00001` customers, `F00001` suppliers, `P000001` products).
pub fn next_code(
    tx: &Transaction<'_>,
    counter: &str,
    prefix: &str,
    width: usize,
) -> Result<String, AppError> {
    let seq = next_number(tx, counter, "", 0)?;
    Ok(format!("{prefix}{seq:0width$}"))
}

/// The next Z number of a terminal.
pub fn assign_z(tx: &Transaction<'_>, terminal_code: &str) -> Result<(i64, String), AppError> {
    let seq = next_number(tx, Z_COUNTER, terminal_code, 0)?;
    Ok((seq, format_z_number(terminal_code, seq)))
}

/// Numbers a counter jumped over (`counter_skips`, migration 0004). A restored database moves
/// its counters past the numbers the database it replaced had already printed, so that none is
/// handed out twice (`db::journal_pending_restore`), and the `backup.restored` row of that
/// restore lists each skip (`skipped`). A row that no such journal row lists is no skip: its
/// gap stays a gap for the verifications.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skip {
    pub id: String,
    /// The counter key: a document type, [`Z_COUNTER`], or a code counter (`customer_code`, …).
    pub doc_type: String,
    pub series: String,
    pub fiscal_year: i64,
    /// First and last number skipped.
    pub from: i64,
    pub to: i64,
}

/// The audit action whose rows journal the skips of their restore (entity: the restore id).
const SKIP_JOURNAL_ACTION: &str = "backup.restored";

/// The audit action whose row journals the skips of a till's promotion to main PC (LAN-010,
/// entity: the new main PC's id): the numbers the old main PC may have printed after this PC
/// last heard from it (`sync::role::promote`).
pub const PROMOTION_JOURNAL_ACTION: &str = "lan.promoted";

/// Moves the counter of (`counter`, `series`, `fiscal_year`) up to `to` when it is behind,
/// inside the caller's transaction, recording the numbers in between as one skip of the restore
/// `restore_id`: the triggers of 0004 let a counter jump over a recorded skip only. `None` when
/// the counter already reached `to`. The caller lists the skip in the restore's journal row.
pub fn fast_forward(
    tx: &Transaction<'_>,
    counter: &str,
    series: &str,
    fiscal_year: i64,
    to: i64,
    restore_id: &str,
    now: i64,
) -> Result<Option<Skip>, AppError> {
    let live: Option<i64> = tx
        .query_row(
            "SELECT last_number FROM counters
             WHERE doc_type = ?1 AND series = ?2 AND fiscal_year = ?3",
            params![counter, series, fiscal_year],
            |r| r.get(0),
        )
        .optional()?;
    let from = live.unwrap_or(0).saturating_add(1);
    if to < from {
        return Ok(None);
    }
    let skip = Skip {
        id: new_id(),
        doc_type: counter.to_owned(),
        series: series.to_owned(),
        fiscal_year,
        from,
        to,
    };
    tx.execute(
        "INSERT INTO counter_skips (id, doc_type, series, fiscal_year, from_number, to_number,
                                    restore_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            skip.id,
            counter,
            series,
            fiscal_year,
            from,
            to,
            restore_id,
            now
        ],
    )?;
    let jump = if live.is_some() {
        "UPDATE counters SET last_number = ?4
         WHERE doc_type = ?1 AND series = ?2 AND fiscal_year = ?3"
    } else {
        "INSERT INTO counters (doc_type, series, fiscal_year, last_number) VALUES (?1, ?2, ?3, ?4)"
    };
    tx.execute(jump, params![counter, series, fiscal_year, to])?;
    Ok(Some(skip))
}

/// The skips of a counter, every fiscal year, in number order, that the `backup.restored` row
/// of their restore journals: it lists each with the same id, key and range. Any other
/// `counter_skips` row (written by hand, or naming a range the journal does not) is left out.
/// On the main PC a till's journal is kept in `lan_journal` (M5): a till's restore journals the
/// skips of that till's own series there, and only those.
pub fn journaled_skips(
    conn: &Connection,
    counter: &str,
    series: &str,
) -> Result<Vec<Skip>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT s.id, s.fiscal_year, s.from_number, s.to_number, a.payload, 0, a.seq
         FROM counter_skips s
         JOIN audit_log a ON a.entity_id = s.restore_id
                         AND ((a.entity_type = 'backup' AND a.action = ?3)
                              OR (a.entity_type = 'lan' AND a.action = ?4))
         WHERE s.doc_type = ?1 AND s.series = ?2
         UNION ALL
         SELECT s.id, s.fiscal_year, s.from_number, s.to_number, j.payload, 1, j.seq
         FROM counter_skips s
         JOIN lan_journal j ON j.entity_type = 'backup' AND j.entity_id = s.restore_id
                           AND j.action = ?3
                           AND j.terminal_id = (SELECT t.id FROM terminals t WHERE t.code = s.series)
         WHERE s.doc_type = ?1 AND s.series = ?2
         ORDER BY 2, 3, 6, 7",
    )?;
    let rows = stmt
        .query_map(
            params![
                counter,
                series,
                SKIP_JOURNAL_ACTION,
                PROMOTION_JOURNAL_ACTION
            ],
            |r| {
                let skip = Skip {
                    id: r.get(0)?,
                    doc_type: counter.to_owned(),
                    series: series.to_owned(),
                    fiscal_year: r.get(1)?,
                    from: r.get(2)?,
                    to: r.get(3)?,
                };
                Ok((skip, r.get::<_, String>(4)?))
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let mut skips: Vec<Skip> = Vec::new();
    for (skip, payload) in rows {
        // Two journal rows naming one restore would join a skip twice: it counts once.
        if !skips.contains(&skip) && journal_lists(&payload, &skip) {
            skips.push(skip);
        }
    }
    Ok(skips)
}

/// The journal row's payload lists `skip` among its `skipped` ranges.
fn journal_lists(payload: &str, skip: &Skip) -> bool {
    let Ok(payload) = serde_json::from_str::<Value>(payload) else {
        return false;
    };
    payload
        .get("skipped")
        .and_then(Value::as_array)
        .is_some_and(|listed| {
            listed.iter().any(|entry| {
                serde_json::from_value::<Skip>(entry.clone()).is_ok_and(|listed| listed == *skip)
            })
        })
}

/// Numbers a counter handed out over every fiscal year: the sum of its counters less the
/// numbers of its journaled skips. A chain of documents or of Z accounts for each of them.
pub fn issued(conn: &Connection, counter: &str, series: &str) -> Result<i64, AppError> {
    let numbered: i64 = conn.query_row(
        "SELECT COALESCE(SUM(last_number), 0) FROM counters WHERE doc_type = ?1 AND series = ?2",
        params![counter, series],
        |r| r.get(0),
    )?;
    let skipped = journaled_skips(conn, counter, series)?
        .iter()
        .fold(0_i64, |sum, s| {
            sum.saturating_add(s.to.saturating_sub(s.from).saturating_add(1))
        });
    Ok(numbered.saturating_sub(skipped))
}

/// The number that follows `n` in a counter whose journaled skips are `skips` (one key): `n + 1`,
/// or the number after the skips that start there.
pub fn next_after(skips: &[Skip], n: i64) -> i64 {
    let mut next = n.saturating_add(1);
    while let Some(skip) = skips.iter().find(|s| s.from == next && s.to >= next) {
        match skip.to.checked_add(1) {
            Some(after) => next = after,
            None => break,
        }
    }
    next
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::Actor;
    use crate::db::Db;
    use serde_json::json;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn assign_on(db: &Db, doc_type: DocType, series: &'static str, day: NaiveDate) -> String {
        db.write(move |tx| assign(tx, doc_type, series, day))
            .unwrap()
            .number
    }

    #[test]
    fn formats_follow_the_spec_table() {
        assert_eq!(
            format_number(DocType::Ticket, "C1", 2026, 123),
            "TK2026-C1-000123"
        );
        assert_eq!(
            format_number(DocType::Invoice, "", 2026, 45),
            "FA2026-000045"
        );
        assert_eq!(
            format_number(DocType::ReturnNote, "C2", 2027, 1),
            "BR2027-C2-000001"
        );
        assert_eq!(format_z_number("C1", 31), "ZC1-000031");
        let prefixes: Vec<&str> = DocType::ALL.iter().map(|t| t.prefix()).collect();
        assert_eq!(
            prefixes,
            [
                "TK", "FA", "PF", "DV", "BL", "AV", "BR", "CC", "BC", "RC", "FF", "BT"
            ]
        );
        assert_eq!(DocType::Ticket.series("C1"), "C1");
        assert_eq!(DocType::Invoice.series("C1"), "");
        for t in DocType::ALL {
            assert_eq!(DocType::parse(t.as_str()), Some(t));
        }
    }

    #[test]
    fn numbers_are_consecutive_per_series_and_restart_each_year() {
        let db = Db::open_memory().unwrap();
        let d = date(2026, 12, 31);
        assert_eq!(assign_on(&db, DocType::Ticket, "C1", d), "TK2026-C1-000001");
        assert_eq!(assign_on(&db, DocType::Ticket, "C1", d), "TK2026-C1-000002");
        assert_eq!(assign_on(&db, DocType::Ticket, "C2", d), "TK2026-C2-000001");
        assert_eq!(
            assign_on(&db, DocType::ReturnNote, "C1", d),
            "BR2026-C1-000001"
        );
        assert_eq!(
            assign_on(&db, DocType::Ticket, "C1", date(2027, 1, 1)),
            "TK2027-C1-000001"
        );
    }

    #[test]
    fn one_counter_runs_across_years_when_the_restart_is_off() {
        let db = Db::open_memory().unwrap();
        db.write(|tx| {
            crate::settings::set_value(
                tx,
                &Actor::default(),
                "numbering.restart_yearly",
                json!(false),
            )
        })
        .unwrap();
        assert_eq!(
            assign_on(&db, DocType::Ticket, "C1", date(2026, 12, 31)),
            "TK2026-C1-000001"
        );
        assert_eq!(
            assign_on(&db, DocType::Ticket, "C1", date(2027, 1, 1)),
            "TK2027-C1-000002"
        );
    }

    fn set_restart(db: &Db, on: bool) -> Result<serde_json::Value, AppError> {
        db.write(move |tx| {
            crate::settings::set_value(tx, &Actor::default(), "numbering.restart_yearly", json!(on))
        })
    }

    #[test]
    fn the_restart_setting_is_frozen_once_a_document_is_numbered() {
        let db = Db::open_memory().unwrap();
        let d = date(2026, 9, 28);
        assert_eq!(assign_on(&db, DocType::Ticket, "C1", d), "TK2026-C1-000001");
        let refused = set_restart(&db, false).unwrap_err();
        assert_eq!(
            (refused.code().as_str(), refused.payload().field.as_deref()),
            ("value_invalid", Some("value"))
        );
        // Restating the value in effect is not a change.
        set_restart(&db, true).unwrap();
        assert!(
            db.read(|c| crate::settings::get::<bool>(c, "numbering.restart_yearly"))
                .unwrap()
        );
        assert_eq!(assign_on(&db, DocType::Ticket, "C1", d), "TK2026-C1-000002");
    }

    #[test]
    fn the_restart_setting_changes_freely_before_any_document_is_numbered() {
        let db = Db::open_memory().unwrap();
        // Z counters never restart, so a Z report does not freeze the setting.
        db.write(|tx| assign_z(tx, "C1")).unwrap();
        set_restart(&db, false).unwrap();
        set_restart(&db, true).unwrap();
        set_restart(&db, false).unwrap();
        assert!(
            !db.read(|c| crate::settings::get::<bool>(c, "numbering.restart_yearly"))
                .unwrap()
        );
        assert_eq!(
            assign_on(&db, DocType::Ticket, "C1", date(2026, 9, 28)),
            "TK2026-C1-000001"
        );
    }

    #[test]
    fn a_rolled_back_validation_leaves_no_gap() {
        let db = Db::open_memory().unwrap();
        let d = date(2026, 9, 28);
        assign_on(&db, DocType::Ticket, "C1", d);
        let failed = db.write(move |tx| {
            assign(tx, DocType::Ticket, "C1", d)?;
            Err::<(), _>(AppError::validation("probe", "forced"))
        });
        assert_eq!(failed.unwrap_err().code(), "forced");
        assert_eq!(assign_on(&db, DocType::Ticket, "C1", d), "TK2026-C1-000002");
        let last = db
            .read(move |c| last_number(c, "ticket", "C1", 2026))
            .unwrap();
        assert_eq!(last, 2);
    }

    #[test]
    fn custom_numbers_follow_the_documents_rules_and_codes_never_reset() {
        let db = crate::db::Db::open_memory().unwrap();
        let day = chrono::NaiveDate::from_ymd_opt(2026, 9, 28).unwrap();
        let next_year = chrono::NaiveDate::from_ymd_opt(2027, 1, 2).unwrap();
        let a = db
            .write(move |tx| assign_custom(tx, "credit_receipt", "RV", day))
            .unwrap();
        let b = db
            .write(move |tx| assign_custom(tx, "credit_receipt", "RV", day))
            .unwrap();
        let c = db
            .write(move |tx| assign_custom(tx, "credit_receipt", "RV", next_year))
            .unwrap();
        assert_eq!(
            (a.number.as_str(), b.number.as_str(), c.number.as_str()),
            ("RV2026-000001", "RV2026-000002", "RV2027-000001")
        );
        let first = db
            .write(|tx| next_code(tx, "customer_code", "C", 5))
            .unwrap();
        let second = db
            .write(|tx| next_code(tx, "customer_code", "C", 5))
            .unwrap();
        assert_eq!((first.as_str(), second.as_str()), ("C00001", "C00002"));
    }

    /// Every custom counter the code numbers with is known here, so a restore over a newer
    /// backup names the numbers it will issue again as printed (`backup::printed`): the M8 packs'
    /// yield runs, service tickets, deposit receipts, instalment contracts, vouchers, delivery
    /// rounds and zakat statements included.
    #[test]
    fn every_custom_counter_in_the_code_has_its_prefix() {
        let sources = [
            include_str!("butcher/yields.rs"),
            include_str!("service_tickets.rs"),
            include_str!("deposits.rs"),
            include_str!("instalments.rs"),
            include_str!("gift_vouchers.rs"),
            include_str!("zakat.rs"),
            include_str!("wholesale/rounds.rs"),
            include_str!("customers/credit.rs"),
        ];
        let mut found = Vec::new();
        for src in sources {
            for (i, _) in src.match_indices("assign_custom(tx, \"") {
                let mut parts = src[i..].split('"');
                parts.next();
                let counter = parts.next().unwrap();
                parts.next();
                let prefix = parts.next().unwrap();
                found.push((counter, prefix));
            }
        }
        assert_eq!(found.len(), 7, "{found:?}");
        for (counter, prefix) in found {
            assert_eq!(custom_prefix(counter), Some(prefix), "{counter}");
        }
        assert_eq!(custom_prefix("yield_run"), Some("DC"));
        // The controlled medicines register numbers its entries `2026-000012`, without a prefix
        // (pharmacy::register's own counter).
        assert!(
            include_str!("pharmacy/register.rs")
                .contains("const COUNTER: &str = \"controlled_register\";")
        );
        assert_eq!(
            custom_prefix("controlled_register").map(|p| format_custom(p, 2026, 12)),
            Some("2026-000012".to_owned())
        );
    }

    #[test]
    fn a_custom_series_carries_its_code() {
        let db = Db::open_memory().unwrap();
        let day = date(2026, 9, 28);
        let till = db
            .write(move |tx| assign_custom_in(tx, "credit_receipt", "RV", "C2", day))
            .unwrap();
        let company = db
            .write(move |tx| assign_custom_in(tx, "credit_receipt", "RV", "", day))
            .unwrap();
        assert_eq!(
            (till.number.as_str(), company.number.as_str()),
            ("RV2026-C2-000001", "RV2026-000001"),
            "each series counts on its own"
        );
    }

    #[test]
    fn z_numbers_never_restart() {
        let db = Db::open_memory().unwrap();
        let first = db.write(|tx| assign_z(tx, "C1")).unwrap();
        let second = db.write(|tx| assign_z(tx, "C1")).unwrap();
        assert_eq!(
            (first, second),
            ((1, "ZC1-000001".into()), (2, "ZC1-000002".into()))
        );
    }

    fn exec(db: &Db, sql: &'static str) -> Result<(), AppError> {
        db.write(move |tx| Ok(tx.execute_batch(sql)?))
    }

    /// Migration 0004: a counter jumps only over a skip recorded for its key, from its next
    /// number to its new last one; a new key starts past a skip that begins at 1. Skips are
    /// append-only.
    #[test]
    fn a_counter_jumps_only_over_a_recorded_skip() {
        let db = Db::open_memory().unwrap();
        db.write(|tx| next_number(tx, "ticket", "C1", 2026).map(drop))
            .unwrap();
        let jump = "UPDATE counters SET last_number = 5 WHERE doc_type = 'ticket'";
        assert!(exec(&db, jump).is_err());
        exec(
            &db,
            "INSERT INTO counter_skips (id, doc_type, series, fiscal_year, from_number, to_number,
                                        restore_id, created_at)
               VALUES ('00000000-0000-7000-8000-0000000051b1', 'ticket', 'C1', 2026, 3, 5, 'r1', 0)",
        )
        .unwrap();
        assert!(
            exec(&db, jump).is_err(),
            "the skip must start at the counter's next number"
        );
        exec(
            &db,
            "INSERT INTO counter_skips (id, doc_type, series, fiscal_year, from_number, to_number,
                                        restore_id, created_at)
               VALUES ('00000000-0000-7000-8000-0000000051b2', 'ticket', 'C1', 2025, 2, 5, 'r1', 0)",
        )
        .unwrap();
        assert!(exec(&db, jump).is_err(), "nor belong to another key");
        exec(
            &db,
            "INSERT INTO counter_skips (id, doc_type, series, fiscal_year, from_number, to_number,
                                        restore_id, created_at)
               VALUES ('00000000-0000-7000-8000-0000000051b3', 'ticket', 'C1', 2026, 2, 5, 'r1', 0)",
        )
        .unwrap();
        exec(&db, jump).unwrap();
        let start = "INSERT INTO counters (doc_type, series, fiscal_year, last_number)
                       VALUES ('invoice', '', 2026, 3)";
        assert!(exec(&db, start).is_err());
        exec(
            &db,
            "INSERT INTO counter_skips (id, doc_type, series, fiscal_year, from_number, to_number,
                                        restore_id, created_at)
               VALUES ('00000000-0000-7000-8000-0000000051b4', 'invoice', '', 2026, 1, 3, 'r1', 0)",
        )
        .unwrap();
        exec(&db, start).unwrap();
        assert!(exec(&db, "UPDATE counter_skips SET to_number = 9").is_err());
        assert!(exec(&db, "DELETE FROM counter_skips").is_err());
        assert!(
            exec(
                &db,
                "INSERT INTO counter_skips (id, doc_type, series, fiscal_year, from_number, to_number,
                                            restore_id, created_at)
                   VALUES ('00000000-0000-7000-8000-0000000051b5', 'ticket', 'C1', 2026, 7, 6, 'r1', 0)",
            )
            .is_err(),
            "a skip holds at least one number"
        );
    }

    /// `fast_forward` records one skip and moves the counter. Only the skips that the
    /// `backup.restored` row of their restore lists (same key, same range) are subtracted from
    /// the numbers issued, once each.
    #[test]
    fn only_the_skips_a_restore_journals_count() {
        let db = Db::open_memory().unwrap();
        let restore = crate::ids::new_id();
        let r = restore.clone();
        let skip = db
            .write(move |tx| {
                next_number(tx, "ticket", "C1", 2026)?;
                fast_forward(tx, "ticket", "C1", 2026, 4, &r, 0)
            })
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                skip.doc_type.as_str(),
                skip.series.as_str(),
                skip.fiscal_year
            ),
            ("ticket", "C1", 2026)
        );
        assert_eq!((skip.from, skip.to), (2, 4));
        let r = restore.clone();
        assert_eq!(
            db.write(move |tx| fast_forward(tx, "ticket", "C1", 2026, 4, &r, 0))
                .unwrap(),
            None,
            "a counter that is not behind does not move"
        );
        let issued_now = || db.read(|c| issued(c, "ticket", "C1")).unwrap();
        assert_eq!(issued_now(), 4, "a jump nobody journaled stays a gap");
        let journal = |skipped: serde_json::Value| {
            let restore = restore.clone();
            db.write(move |tx| {
                let event = Actor::default().event(
                    "backup.restored",
                    "backup",
                    Some(&restore),
                    json!({ "skipped": skipped }),
                );
                crate::audit::append(tx, &event).map(drop)
            })
            .unwrap();
        };
        let mut other_range = serde_json::to_value(&skip).unwrap();
        other_range["to"] = json!(5);
        journal(json!([other_range]));
        assert_eq!(issued_now(), 4, "the journal names another range");
        journal(json!([skip]));
        journal(json!([skip]));
        assert_eq!(issued_now(), 1, "counted once");
        assert_eq!(
            db.read(|c| journaled_skips(c, "ticket", "C1")).unwrap(),
            [skip]
        );
        assert_eq!(
            db.write(|tx| next_number(tx, "ticket", "C1", 2026))
                .unwrap(),
            5
        );
    }

    /// On the main PC a till's journal lives in `lan_journal` (M5 replay): the `backup.restored`
    /// row of a till's restore justifies the skips of that till's own series, no other.
    #[test]
    fn a_tills_restore_journaled_on_the_main_pc_counts_for_its_series() {
        const C2: &str = "00000000-0000-7000-8000-0000000000c2";
        const C3: &str = "00000000-0000-7000-8000-0000000000c3";
        let db = Db::open_memory().unwrap();
        let restore = crate::ids::new_id();
        let r = restore.clone();
        let skip = db
            .write(move |tx| {
                tx.execute_batch(&format!(
                    "INSERT INTO terminals (id, code, name, role, created_at)
                       VALUES ('{C2}', 'C2', 'Caisse 2', 'client', 0), ('{C3}', 'C3', 'Caisse 3', 'client', 0);"
                ))?;
                fast_forward(tx, "ticket", "C2", 2026, 3, &r, 0)
            })
            .unwrap()
            .unwrap();
        let issued_now = || db.read(|c| issued(c, "ticket", "C2")).unwrap();
        assert_eq!(issued_now(), 3, "nobody journaled the jump yet");
        let journal = |terminal: &'static str| {
            let row = crate::sync::outbox::JournalImage {
                seq: 1,
                id: crate::ids::new_id(),
                at: 0,
                user_id: None,
                terminal_id: Some(terminal.into()),
                action: "backup.restored".into(),
                entity_type: "backup".into(),
                entity_id: Some(restore.clone()),
                payload: json!({ "skipped": [skip.clone()] }).to_string(),
                prev_hash: crate::audit::GENESIS_HASH.into(),
                hash: "b".repeat(64),
            };
            db.write(move |tx| crate::sync::journal::append(tx, terminal, &row, 1).map(drop))
                .unwrap();
        };
        journal(C3);
        assert_eq!(
            issued_now(),
            3,
            "another till's journal says nothing of C2's numbers"
        );
        journal(C2);
        assert_eq!(issued_now(), 0, "C2's own journal lists the skip");
        assert_eq!(
            db.read(|c| journaled_skips(c, "ticket", "C2")).unwrap(),
            [skip]
        );
    }

    #[test]
    fn the_number_after_a_skip_is_past_it() {
        let skip = |from, to| Skip {
            id: String::new(),
            doc_type: Z_COUNTER.into(),
            series: "C1".into(),
            fiscal_year: 0,
            from,
            to,
        };
        let skips = [skip(2, 3), skip(4, 4), skip(9, 9)];
        assert_eq!(next_after(&skips, 1), 5);
        assert_eq!(next_after(&skips, 5), 6);
        assert_eq!(next_after(&skips, 8), 10);
        assert_eq!(next_after(&[], 7), 8);
        assert_eq!(
            next_after(&[skip(i64::MAX, i64::MAX)], i64::MAX - 1),
            i64::MAX,
            "no overflow"
        );
    }

    #[test]
    fn concurrent_validations_get_every_number_once() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("pos.db"), crate::db::DbOptions::default()).unwrap();
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let db = db.clone();
                std::thread::spawn(move || {
                    (0..25)
                        .map(|_| {
                            db.write(|tx| assign(tx, DocType::Ticket, "C1", date(2026, 9, 28)))
                                .unwrap()
                                .seq
                        })
                        .collect::<Vec<i64>>()
                })
            })
            .collect();
        let mut all: Vec<i64> = handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect();
        all.sort_unstable();
        assert_eq!(all, (1..=200).collect::<Vec<i64>>());
    }
}

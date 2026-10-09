//! The versement receipts `RV…` in the integrity checks (AUD-004, the fiscal archive's chains).
//! They are numbered by their own counter (`numbering::assign_custom_in`) and written to the
//! customer ledger, not to `documents`, so they carry no hash of their own: each one is anchored
//! instead by the `credit.collected` row of the hash-chained journal that `credit::collect`
//! writes in the same transaction.
//!
//! One chain per series and counter key: the company's (`RV2026-000001`) and each till's own
//! (`RV2026-C2-000001`, M5). The main PC checks its own receipts against its journal and a
//! till's against that till's journal it keeps (`lan_journal`); a till checks its own series
//! against its journal (the others' journals are not on it, they are checked on the main PC).

use crate::error::AppError;
use crate::numbering::{self, Skip};
use chrono::Datelike;
use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, HashMap};

/// The counter of the versement receipts and the prefix of their numbers (`credit::collect`).
const COUNTER: &str = "credit_receipt";
const PREFIX: &str = "RV";

/// What [`verify`] found: every number the counter handed out (less the numbers a restore
/// skipped) is on one versement, each versement is the one the journal recorded, and no
/// versement carries a number the counter never handed out.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptChainReport {
    /// Versement receipts checked.
    pub rows: i64,
    /// The first number at fault, by year then number; `None` when every receipt holds.
    pub first_broken_number: Option<String>,
}

/// What the journal's `credit.collected` rows recorded of each versement (`credit::collect`):
/// the receipt number, then the customer, the entry, the amount received, the method, the duty,
/// the duty charged and the balance after.
const JOURNAL: &str = "SELECT json_extract(payload, '$.receiptNumber'), entity_id,
        json_extract(payload, '$.entryId'), json_extract(payload, '$.amount'),
        json_extract(payload, '$.method'), json_extract(payload, '$.stampDuty'),
        json_extract(payload, '$.stampCharged'), json_extract(payload, '$.balanceAfter')
     FROM audit_log WHERE action = 'credit.collected' AND entity_type = 'customer'";

/// The same rows of a till's journal kept on the main PC.
const TILL_JOURNAL: &str = "SELECT json_extract(payload, '$.receiptNumber'), entity_id,
        json_extract(payload, '$.entryId'), json_extract(payload, '$.amount'),
        json_extract(payload, '$.method'), json_extract(payload, '$.stampDuty'),
        json_extract(payload, '$.stampCharged'), json_extract(payload, '$.balanceAfter')
     FROM lan_journal WHERE terminal_id = ?1 AND action = 'credit.collected'
       AND entity_type = 'customer'";

/// The same of each receipt in the customer ledger.
const LEDGER: &str = "SELECT receipt_number, customer_id, id, -amount, method, stamp_duty,
        stamp_charged, balance_after
     FROM customer_ledger WHERE receipt_number IS NOT NULL";

/// A versement's fields after its receipt number, compared as SQLite values: a payload edited
/// into another type never matches, and never fails the check.
type Fields = [Value; 7];

fn fields(r: &Row<'_>) -> rusqlite::Result<Fields> {
    Ok([
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
        r.get(6)?,
        r.get(7)?,
    ])
}

/// Where the receipts of a series are anchored, as seen from this PC.
enum Anchors {
    /// This PC's own journal: the company's series off a till, a till's own series on it.
    Here,
    /// A till's journal kept on the main PC.
    Till(String),
}

/// The journal of `series` on this PC; `None` for a series checked elsewhere (on a till, the
/// main PC's and the other tills' receipts that reached it).
fn anchors_of(
    conn: &Connection,
    series: &str,
    client: bool,
    local_code: Option<&str>,
) -> Result<Option<Anchors>, AppError> {
    if client {
        return Ok((!series.is_empty() && Some(series) == local_code).then_some(Anchors::Here));
    }
    if series.is_empty() || Some(series) == local_code {
        return Ok(Some(Anchors::Here));
    }
    let terminal: Option<String> = conn
        .query_row("SELECT id FROM terminals WHERE code = ?1", [series], |r| {
            r.get(0)
        })
        .optional()?;
    // A series no terminal of the shop prints: nothing anchors it, its receipts break.
    Ok(Some(Anchors::Till(terminal.unwrap_or_default())))
}

/// The versements a journal recorded, by receipt number, in one pass over the rows of the action.
fn journal(
    conn: &Connection,
    anchors: &Anchors,
    into: &mut HashMap<String, Vec<Fields>>,
) -> Result<(), AppError> {
    let mut stmt;
    let mut rows = match anchors {
        Anchors::Here => {
            stmt = conn.prepare(JOURNAL)?;
            stmt.query([])?
        }
        Anchors::Till(id) => {
            stmt = conn.prepare(TILL_JOURNAL)?;
            stmt.query([id])?
        }
    };
    while let Some(r) = rows.next()? {
        if let Value::Text(number) = r.get(0)? {
            into.entry(number).or_default().push(fields(r)?);
        }
    }
    Ok(())
}

/// `RV2026-000001` → ("", 2026, 1) and `RV2026-C2-000001` → ("C2", 2026, 1); `None` for anything
/// `numbering::assign_custom_in` did not print.
fn parse(number: &str) -> Option<(String, i32, i64)> {
    let rest = number.strip_prefix(PREFIX)?;
    let (year, rest) = rest.split_once('-')?;
    let (series, seq) = rest.rsplit_once('-').unwrap_or(("", rest));
    let (year, seq): (i32, i64) = (year.parse().ok()?, seq.parse().ok()?);
    (print(series, year, seq) == number).then(|| (series.to_owned(), year, seq))
}

/// The number `numbering::assign_custom_in` prints.
fn print(series: &str, year: i32, seq: i64) -> String {
    if series.is_empty() {
        numbering::format_custom(PREFIX, year, seq)
    } else {
        format!("{PREFIX}{year}-{series}-{seq:06}")
    }
}

/// A place in the receipts: series, counter key, number.
type Place = (String, i32, i64);

/// Where a receipt number nothing parses is reported: after every chain.
fn nowhere() -> Place {
    (char::MAX.to_string(), i32::MAX, i64::MAX)
}

/// The receipts as one chain per series and counter key (the fiscal year, or 0 when the numbers
/// run on across years: `numbering.restart_yearly` is frozen once a counter exists, so the two
/// never mix). Read only and linear: one entry per receipt and per versement of the journal.
pub fn verify(conn: &Connection) -> Result<ReceiptChainReport, AppError> {
    let client = crate::sync::role::is_client(conn)?;
    let local_code = crate::terminal::local(conn)?.map(|t| t.code);
    let local_code = local_code.as_deref();
    let counters = conn
        .prepare(
            "SELECT series, fiscal_year, last_number FROM counters
             WHERE doc_type = ?1 ORDER BY series, fiscal_year",
        )?
        .query_map([COUNTER], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i32>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut ledger: Vec<(String, Fields)> = Vec::new();
    {
        let mut stmt = conn.prepare(LEDGER)?;
        let mut found = stmt.query([])?;
        while let Some(r) = found.next()? {
            ledger.push((r.get(0)?, fields(r)?));
        }
    }
    // Every series a counter or a receipt names, with where it is anchored from this PC.
    let mut series: BTreeMap<String, Option<Anchors>> = BTreeMap::new();
    let named = counters
        .iter()
        .map(|(s, _, _)| s.clone())
        .chain(ledger.iter().filter_map(|(n, _)| parse(n).map(|p| p.0)));
    for s in named {
        if let std::collections::btree_map::Entry::Vacant(slot) = series.entry(s) {
            let anchors = anchors_of(conn, slot.key(), client, local_code)?;
            slot.insert(anchors);
        }
    }
    let mut journals: HashMap<String, Vec<Fields>> = HashMap::new();
    if series.values().any(|a| matches!(a, Some(Anchors::Here))) {
        journal(conn, &Anchors::Here, &mut journals)?;
    }
    for anchors in series.values().flatten() {
        if let Anchors::Till(_) = anchors {
            journal(conn, anchors, &mut journals)?;
        }
    }
    let checked = |s: &str| series.get(s).is_some_and(Option::is_some);
    let runs_on = |s: &str| counters.iter().any(|(c, key, _)| c == s && *key == 0);
    // (series, counter key, number) → the printed numbers there and whether the journal holds
    // each.
    let mut held: BTreeMap<Place, Vec<(String, bool)>> = BTreeMap::new();
    // The year printed on each number of a series whose counter runs on (a missing one's year).
    let mut years: BTreeMap<(String, i64), i32> = BTreeMap::new();
    let mut breaks = Breaks::default();
    let mut rows = 0_i64;
    for (number, here) in ledger {
        let parsed = parse(&number);
        if parsed.as_ref().is_some_and(|(s, _, _)| !checked(s)) {
            continue; // checked on the PC that keeps its journal
        }
        rows += 1;
        let anchored = journals.get(&number).is_some_and(|j| j.contains(&here));
        match parsed {
            Some((s, year, seq)) => {
                let on = runs_on(&s);
                if on {
                    years.insert((s.clone(), seq), year);
                }
                let key = if on { 0 } else { year };
                held.entry((s, key, seq))
                    .or_default()
                    .push((number, anchored));
            }
            None => breaks.flag(nowhere(), Some(number)),
        }
    }
    for (s, key, last) in counters.iter().filter(|(s, _, _)| checked(s)) {
        let skips = numbering::journaled_skips(conn, COUNTER, s)?;
        for seq in 1..=*last {
            let skipped = skips.iter().any(|skip| skipped(skip, *key, seq));
            let place = (s.clone(), *key, seq);
            match held.remove(&place) {
                None if skipped => {}
                None => {
                    let number = (*key != 0).then(|| print(s, *key, seq));
                    breaks.flag(place, number);
                }
                Some(here) => {
                    let whole = !skipped && here.len() == 1 && here.iter().all(|(_, a)| *a);
                    if !whole {
                        breaks.flag(place, here.into_iter().next().map(|(n, _)| n));
                    }
                }
            }
        }
    }
    // What is left carries a number no counter handed out.
    for (place, here) in held {
        breaks.flag(place, here.into_iter().next().map(|(n, _)| n));
    }
    let first_broken_number = match breaks.first {
        None => None,
        Some((_, Some(number))) => Some(number),
        Some(((s, _, seq), None)) => Some(missing_number(conn, &s, seq, &years)?),
    };
    Ok(ReceiptChainReport {
        rows,
        first_broken_number,
    })
}

fn skipped(skip: &Skip, key: i32, seq: i64) -> bool {
    skip.fiscal_year == i64::from(key) && (skip.from..=skip.to).contains(&seq)
}

/// The first break seen: its place (series, counter key, number) and its printed number when
/// known.
#[derive(Default)]
struct Breaks {
    first: Option<(Place, Option<String>)>,
}

impl Breaks {
    fn flag(&mut self, place: Place, number: Option<String>) {
        if self.first.as_ref().is_none_or(|(p, _)| place < *p) {
            self.first = Some((place, number));
        }
    }
}

/// A number the counter of `series` handed out across years that no versement carries: the
/// journal knows its year; failing that, the year of the receipt before it (or after it, or
/// today's).
fn missing_number(
    conn: &Connection,
    series: &str,
    seq: i64,
    years: &BTreeMap<(String, i64), i32>,
) -> Result<String, AppError> {
    let mut stmt = conn.prepare(
        "SELECT json_extract(payload, '$.receiptNumber') FROM audit_log
         WHERE action = 'credit.collected' AND json_extract(payload, '$.receiptNumber') LIKE ?1
         UNION ALL
         SELECT json_extract(payload, '$.receiptNumber') FROM lan_journal
         WHERE action = 'credit.collected' AND json_extract(payload, '$.receiptNumber') LIKE ?1",
    )?;
    let journaled = stmt
        .query_map([format!("{PREFIX}%-{seq:06}")], |r| {
            r.get::<_, Option<String>>(0)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(number) = journaled
        .into_iter()
        .flatten()
        .find(|n| parse(n).is_some_and(|(s, _, n)| s == series && n == seq))
    {
        return Ok(number);
    }
    let key = |n: i64| (series.to_owned(), n);
    let year = years
        .range(key(i64::MIN)..key(seq))
        .next_back()
        .or_else(|| years.range(key(seq)..=key(i64::MAX)).next())
        .map(|(_, year)| *year)
        .unwrap_or_else(|| crate::time::business_date(crate::time::now_ms()).year());
    Ok(print(series, year, seq))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::customers::credit::{CollectInput, collect, set_opening};
    use crate::money::Money;
    use crate::payments::PaymentMethod;
    use crate::testkit::{NOON, Shop, shop};
    use crate::till::TillContext;
    use serde_json::json;
    use std::sync::Arc;

    /// 2027-09-28 at noon: a year after `NOON`.
    const NEXT_YEAR: i64 = NOON + 365 * 24 * 3_600_000;

    /// Karim owes 100 000.00 DA from the paper notebook and the cashier's till is open.
    fn karim(shop: &Shop) -> String {
        let karim = shop.customer("Karim");
        let (ctx, id) = (shop.owner_till(), karim.id.clone());
        shop.db
            .write(move |tx| set_opening(tx, &ctx, &id, Money(10_000_000), None).map(drop))
            .unwrap();
        shop.open_till(0);
        karim.id
    }

    /// A cash versement of `amount` centimes taken by the cashier at `now`: its receipt number.
    fn versement(shop: &Shop, customer: &str, amount: i64, now: i64) -> String {
        let ctx = TillContext::new(shop.cashier.clone(), Arc::clone(&shop.pack), now);
        let input = CollectInput {
            customer_id: customer.into(),
            amount: Money(amount),
            method: PaymentMethod::Cash,
            reference: None,
            cheque_bank: None,
            cheque_date: None,
            allocate_to: Vec::new(),
            note: None,
        };
        shop.db
            .write(move |tx| collect(tx, &ctx, input))
            .unwrap()
            .entry
            .receipt_number
            .unwrap()
    }

    /// Karim's three versements of 2026: `RV2026-000001` to `RV2026-000003`.
    fn three_versements() -> Shop {
        let shop = shop();
        let karim = karim(&shop);
        for amount in [100_000, 250_000, 50_000] {
            versement(&shop, &karim, amount, NOON);
        }
        shop
    }

    /// Raw SQL behind the application's back (the triggers dropped first where needed).
    fn tamper(shop: &Shop, sql: &'static str) {
        shop.db.write(move |tx| Ok(tx.execute_batch(sql)?)).unwrap();
    }

    fn report(shop: &Shop) -> (i64, Option<String>) {
        let report = shop.db.read(verify).unwrap();
        (report.rows, report.first_broken_number)
    }

    /// A versement receipt written straight into the ledger, without its journal row.
    const FORGED: &str =
        "INSERT INTO customer_ledger (id, customer_id, kind, amount, balance_after,
             method, receipt_number, business_date, created_at)
         SELECT '01900000-0000-7000-8000-000000000001', id, 'payment', -100000, balance - 100000,
             'cash', 'RV2026-000004', '2026-09-28', 1790593200000
         FROM customers WHERE name = 'Karim';";

    #[test]
    fn a_shop_without_versements_has_no_receipt_to_check() {
        assert_eq!(report(&shop()), (0, None));
    }

    #[test]
    fn every_number_handed_out_is_on_its_versement_as_the_journal_recorded_it() {
        assert_eq!(report(&three_versements()), (3, None));
    }

    #[test]
    fn an_edited_versement_breaks_the_receipts_at_its_number() {
        let shop = three_versements();
        tamper(
            &shop,
            "DROP TRIGGER customer_ledger_no_update;
             UPDATE customer_ledger SET amount = -240000 WHERE receipt_number = 'RV2026-000002';",
        );
        assert_eq!(report(&shop), (3, Some("RV2026-000002".into())));
    }

    #[test]
    fn a_deleted_versement_leaves_its_number_missing_even_the_last_one() {
        let shop = three_versements();
        tamper(
            &shop,
            "DROP TRIGGER customer_allocations_no_delete;
             DROP TRIGGER customer_ledger_no_delete;
             DELETE FROM customer_allocations WHERE credit_id IN
               (SELECT id FROM customer_ledger WHERE receipt_number = 'RV2026-000003');
             DELETE FROM customer_ledger WHERE receipt_number = 'RV2026-000003';",
        );
        assert_eq!(report(&shop), (2, Some("RV2026-000003".into())));
    }

    #[test]
    fn a_receipt_slipped_into_the_ledger_breaks_the_receipts_counted_or_not() {
        let shop = three_versements();
        tamper(&shop, FORGED);
        assert_eq!(
            report(&shop),
            (4, Some("RV2026-000004".into())),
            "a number the counter never handed out"
        );
        let shop = three_versements();
        tamper(
            &shop,
            "UPDATE counters SET last_number = last_number + 1 WHERE doc_type = 'credit_receipt';",
        );
        tamper(&shop, FORGED);
        assert_eq!(
            report(&shop),
            (4, Some("RV2026-000004".into())),
            "counted, but the journal never recorded it"
        );
    }

    #[test]
    fn a_tills_versements_verify_on_the_till_and_on_the_main_pc() {
        use crate::sync::testkit::lan_pair;
        let lan = lan_pair();
        let karim = karim(&lan.host);
        assert_eq!(versement(&lan.host, &karim, 5_000, NOON), "RV2026-000001");
        lan.pull();
        lan.open_till_session(0);
        for expected in ["RV2026-C2-000001", "RV2026-C2-000002"] {
            let ctx = TillContext::new(lan.till_owner.clone(), Arc::clone(&lan.host.pack), NOON);
            let input = CollectInput {
                customer_id: karim.clone(),
                amount: Money(10_000),
                method: PaymentMethod::Cash,
                reference: None,
                cheque_bank: None,
                cheque_date: None,
                allocate_to: Vec::new(),
                note: None,
            };
            let number = lan
                .till
                .write(move |tx| collect(tx, &ctx, input))
                .unwrap()
                .entry
                .receipt_number;
            assert_eq!(number.as_deref(), Some(expected));
        }
        let till = |lan: &crate::sync::testkit::LanPair| {
            let r = lan.till.read(verify).unwrap();
            (r.rows, r.first_broken_number)
        };
        assert_eq!(
            till(&lan),
            (2, None),
            "the till checks its own series; the main PC's receipt is checked there"
        );
        lan.push();
        assert_eq!(
            report(&lan.host),
            (3, None),
            "the main PC checks the till's series against the till's journal it keeps"
        );
        tamper(
            &lan.host,
            "DROP TRIGGER customer_ledger_no_update;
             UPDATE customer_ledger SET amount = -9000 WHERE receipt_number = 'RV2026-C2-000002';",
        );
        assert_eq!(report(&lan.host), (3, Some("RV2026-C2-000002".into())));
        assert_eq!(till(&lan), (2, None));
    }

    #[test]
    fn receipt_numbers_of_every_series_parse_back() {
        assert_eq!(parse("RV2026-000001"), Some((String::new(), 2026, 1)));
        assert_eq!(parse("RV2026-C2-000042"), Some(("C2".into(), 2026, 42)));
        assert_eq!(parse("RV2026-C2-42"), None);
        assert_eq!(parse("RV2026-1"), None);
        assert_eq!(parse("TK2026-C2-000001"), None);
    }

    #[test]
    fn receipts_restart_each_year_unless_the_counter_runs_on() {
        let yearly = shop();
        let customer = karim(&yearly);
        assert_eq!(
            versement(&yearly, &customer, 100_000, NOON),
            "RV2026-000001"
        );
        assert_eq!(
            versement(&yearly, &customer, 100_000, NEXT_YEAR),
            "RV2027-000001"
        );
        assert_eq!(report(&yearly), (2, None));

        let running = shop();
        running.set("numbering.restart_yearly", json!(false));
        let customer = karim(&running);
        assert_eq!(
            versement(&running, &customer, 100_000, NOON),
            "RV2026-000001"
        );
        assert_eq!(
            versement(&running, &customer, 100_000, NEXT_YEAR),
            "RV2027-000002"
        );
        assert_eq!(report(&running), (2, None));
        tamper(
            &running,
            "DROP TRIGGER customer_allocations_no_delete;
             DROP TRIGGER customer_ledger_no_delete;
             DELETE FROM customer_allocations WHERE credit_id IN
               (SELECT id FROM customer_ledger WHERE receipt_number = 'RV2027-000002');
             DELETE FROM customer_ledger WHERE receipt_number = 'RV2027-000002';",
        );
        assert_eq!(
            report(&running),
            (1, Some("RV2027-000002".into())),
            "the journal still knows the year of the missing number"
        );
    }
}

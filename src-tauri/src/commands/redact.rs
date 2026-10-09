//! Costs and margins are for people holding `reports.profit`; everyone else gets zeros (a
//! cashier has no business knowing the purchase price). Kridi balances and limits are for people
//! holding `credit.view` (M2), in the answers of M3's invoices and credit notes too.

use pos_core::audit::AuditPage;
use pos_core::auth::{Permission, Session};
use pos_core::billing::archive::ArchiveRow;
use pos_core::billing::credit_notes::{CreditableLine, ValidatedCreditNote};
use pos_core::billing::validate::ValidatedDocument;
use pos_core::catalog::products::Product;
use pos_core::customers::{Customer, CustomerHit};
use pos_core::documents::{DocType, Document, DocumentSummary};
use pos_core::money::Money;
use pos_core::purchases::CostChange;
use pos_core::purchases::receipts::CostMove;
use pos_core::sales::PricedCart;
use pos_core::stock::StockMovement;
use pos_core::stock::counts::CountResult;
use pos_core::stock::transfers::TransferSummary;
use pos_core::stock::views::{LevelRow, MovementRow};

pub(crate) fn sees_costs(session: &Session) -> bool {
    session.has(Permission::ReportsProfit)
}

pub(crate) fn product(p: &mut Product, session: &Session) {
    if !sees_costs(session) {
        p.cost_price = Money::ZERO;
    }
}

/// A transfer note is valued at the CMUP whichever command returns it (`valued_at_cost`).
pub(crate) fn document(d: &mut Document, session: &Session) {
    if d.doc_type == DocType::Transfer {
        valued_at_cost(d, session);
    } else if !sees_costs(session) {
        for line in &mut d.lines {
            line.cost_price = Money::ZERO;
        }
    }
}

/// A transfer note's total is its value at the CMUP.
pub(crate) fn summary(s: &mut DocumentSummary, session: &Session) {
    if s.doc_type == DocType::Transfer && !sees_costs(session) {
        s.total_due = Money::ZERO;
    }
}

/// The archive's row of a transfer note: its totals are its value at the CMUP.
pub(crate) fn archive_row(r: &mut ArchiveRow, session: &Session) {
    if r.doc_type == DocType::Transfer && !sees_costs(session) {
        r.total_ttc = Money::ZERO;
        r.total_due = Money::ZERO;
    }
}

/// The invoiced lines a credit note may take back carry their cost.
pub(crate) fn creditable(lines: &mut [CreditableLine], session: &Session) {
    if !sees_costs(session) {
        for l in lines {
            l.line.cost_price = Money::ZERO;
        }
    }
}

/// The customer's balance after a write is kridi data (`credit.view`), like his file's.
fn balance(b: &mut Option<Money>, session: &Session) {
    if !session.has(Permission::CreditView) {
        *b = None;
    }
}

/// A validated M3 document: its costs, and the customer's balance an on-account part left.
pub(crate) fn validated(v: &mut ValidatedDocument, session: &Session) {
    document(&mut v.document, session);
    balance(&mut v.customer_balance, session);
}

/// A credit note: its costs, and the customer's balance a refund to his account left.
pub(crate) fn credit_note(v: &mut ValidatedCreditNote, session: &Session) {
    document(&mut v.document, session);
    balance(&mut v.customer_balance, session);
}

pub(crate) fn priced(cart: &mut PricedCart, session: &Session) {
    if !sees_costs(session) {
        for line in &mut cart.lines {
            line.cost_price = Money::ZERO;
        }
    }
}

pub(crate) fn movement(m: &mut StockMovement, session: &Session) {
    if !sees_costs(session) {
        m.unit_cost = None;
    }
}

/// A document valued at the CMUP (a transfer note): its prices and totals are costs.
pub(crate) fn valued_at_cost(d: &mut Document, session: &Session) {
    if !sees_costs(session) {
        for line in &mut d.lines {
            line.unit_price = Money::ZERO;
            line.total_ht = Money::ZERO;
            line.tax_amount = Money::ZERO;
            line.total_ttc = Money::ZERO;
            line.cost_price = Money::ZERO;
        }
        d.taxes.clear();
        d.subtotal_ht = Money::ZERO;
        d.tax_total = Money::ZERO;
        d.total_ttc = Money::ZERO;
        d.total_due = Money::ZERO;
    }
}

pub(crate) fn transfer_summary(t: &mut TransferSummary, session: &Session) {
    if !sees_costs(session) {
        t.value = Money::ZERO;
    }
}

/// The stock value at cost goes; the retail value (sale prices) stays.
pub(crate) fn level_row(r: &mut LevelRow, session: &Session) {
    if !sees_costs(session) {
        r.cost_price = Money::ZERO;
        r.cost_value = Money::ZERO;
    }
}

pub(crate) fn movement_row(m: &mut MovementRow, session: &Session) {
    if !sees_costs(session) {
        m.unit_cost = None;
    }
}

/// A count's variance is valued at the CMUP.
pub(crate) fn count_result(r: &mut CountResult, session: &Session) {
    if !sees_costs(session) {
        r.variance_value = Money::ZERO;
    }
}

/// The average cost a purchase moved (the storekeeper types the purchase price; the CMUP it
/// gives is a cost like the product's).
pub(crate) fn cost_change(c: &mut CostChange, session: &Session) {
    if !sees_costs(session) {
        c.cost_before = Money::ZERO;
        c.cost_after = Money::ZERO;
    }
}

pub(crate) fn cost_move(c: &mut CostMove, session: &Session) {
    if !sees_costs(session) {
        c.cost_before = Money::ZERO;
        c.cost_after = Money::ZERO;
    }
}

/// Balances and limits are kridi data (`credit.view`); the store credit and the credit still
/// available derive from them, so they go too.
pub(crate) fn customer(c: &mut Customer, session: &Session) {
    if !session.has(Permission::CreditView) {
        c.balance = Money::ZERO;
        c.credit_limit = None;
        c.available_credit = None;
        c.store_credit = Money::ZERO;
    }
}

pub(crate) fn customer_hit(h: &mut CustomerHit, session: &Session) {
    if !session.has(Permission::CreditView) {
        h.balance = Money::ZERO;
        h.credit_limit = None;
        h.available_credit = None;
        h.store_credit = Money::ZERO;
    }
}

/// Journal fields that carry a cost: a product's CMUP in its snapshots, a stock adjustment's
/// cost before and after, a movement's unit cost.
const COST_KEYS: [&str; 4] = ["costPrice", "costBefore", "costAfter", "unitCost"];

fn strip_costs(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for key in COST_KEYS {
                map.remove(key);
            }
            map.values_mut().for_each(strip_costs);
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(strip_costs),
        _ => {}
    }
}

/// The journal (AUD-002 record history included) never shows a cost to someone without
/// `reports.profit`: the field is left out, as if it had not changed.
pub(crate) fn audit(page: &mut AuditPage, session: &Session) {
    if !sees_costs(session) {
        for row in &mut page.rows {
            strip_costs(&mut row.payload);
        }
    }
}

/// Request fields whose values never reach a log line (PLT-018, PDP): a jewel buyer's identity
/// (`SaleExtras.buyer`, its ID number and address), the prescriptions of controlled medicines
/// and the names and references they hold. The logging layer masks phones, e-mails and secrets
/// on top (`pos_core::redact::mask_personal`).
const LOG_REDACTED_KEYS: [&str; 7] = [
    "buyer",
    "idNumber",
    "address",
    "prescriptions",
    "patientName",
    "patientRef",
    "prescriberName",
];

fn redact_for_log(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, field) in map.iter_mut() {
                if LOG_REDACTED_KEYS.contains(&key.as_str()) {
                    *field = serde_json::Value::String("[redacted]".into());
                } else {
                    redact_for_log(field);
                }
            }
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(redact_for_log),
        _ => {}
    }
}

/// `value` as JSON for a log line, its personal fields replaced by `"[redacted]"`.
pub(crate) fn log_line<T: serde::Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(mut json) => {
            redact_for_log(&mut json);
            json.to_string()
        }
        Err(_) => "[unloggable]".into(),
    }
}

#[cfg(test)]
mod log_tests {
    use super::*;
    use pos_core::jewellery::{BuyerIdKind, BuyerIdentity};
    use pos_core::pharmacy::register::PrescriptionInput;
    use pos_core::sales::SaleExtras;

    /// PLT-018, PDP: a sale's extras in a log line keep neither the jewel buyer's identity nor the
    /// prescription; the serials stay (they are what support looks for).
    #[test]
    fn a_log_line_holds_no_buyer_nor_prescription() {
        let extras = SaleExtras {
            prescriptions: vec![PrescriptionInput {
                line: 0,
                prescriber_name: "Dr Benali".into(),
                prescriber_ref: Some("CNOM 1234".into()),
                prescription_no: Some("0457".into()),
                prescription_date: "2026-09-27".into(),
                patient_name: "M. Haddad".into(),
                patient_ref: Some("Chifa 99887766".into()),
            }],
            buyer: Some(BuyerIdentity {
                name: "Nadia Ferhat".into(),
                id_kind: BuyerIdKind::Cni,
                id_number: "109876543210987654".into(),
                address: "3 rue des Frères Bouadou, Bir Mourad Raïs".into(),
            }),
            serials: vec![pos_core::sales::extensions::SaleSerial {
                line: 1,
                serial_no: "356789101112131".into(),
            }],
            ..SaleExtras::default()
        };
        let line = log_line(&serde_json::json!({ "sale": { "lines": 2 }, "extras": extras }));
        for secret in [
            "Dr Benali",
            "CNOM 1234",
            "M. Haddad",
            "Chifa 99887766",
            "Nadia Ferhat",
            "109876543210987654",
            "Bouadou",
        ] {
            assert!(!line.contains(secret), "{secret} in {line}");
        }
        assert!(line.contains("[redacted]"), "{line}");
        assert!(line.contains("356789101112131"), "the serial stays: {line}");
    }
}

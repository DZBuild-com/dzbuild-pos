//! Where a document stands (the archive's status column, the editor's banner): derived from the
//! document, its links, its credit notes and the customer's ledger (M2) — never stored, so it
//! can never disagree with them.

use crate::documents::links::{self, LinkKind, LinkedDocument};
use crate::documents::{self, DocOrigin, DocStatus, DocType, Document};
use crate::error::AppError;
use crate::money::Money;
use rusqlite::Connection;
use serde::Serialize;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(
    tag = "state",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum DocState {
    Draft,
    Cancelled,
    /// A quote or proforma that still holds.
    Open {
        valid_until: Option<String>,
    },
    Expired {
        valid_until: String,
    },
    /// Made into another document (their numbers).
    Converted {
        into: Vec<String>,
    },
    /// A delivery note waiting for its invoice.
    ToInvoice,
    /// A delivery note or a ticket with its invoice.
    Invoiced {
        invoice: String,
    },
    /// An invoice with a part left on the customer's account.
    Unpaid {
        remaining: Money,
        due_date: Option<String>,
        overdue: bool,
    },
    Paid,
    PartlyCredited {
        credited: Money,
        remaining: Money,
    },
    /// Fully corrected by credit notes: printed "FACTURE ANNULÉE" (DOC-007).
    Credited {
        by: Vec<String>,
    },
    /// Tickets, return notes, credit notes and the other kinds.
    Final,
}

impl DocState {
    /// Every code of [`DocState::tag`], the closed list of the archive's exported state column.
    pub const TAGS: [&'static str; 13] = [
        "draft",
        "cancelled",
        "open",
        "expired",
        "converted",
        "to_invoice",
        "invoiced",
        "unpaid",
        "overdue",
        "paid",
        "partly_credited",
        "credited",
        "final",
    ];

    /// The state as a code of `TagDomain::DocState` (an unpaid invoice past its due date is
    /// `overdue`); the amounts and numbers it carries are not part of it.
    pub const fn tag(&self) -> &'static str {
        match self {
            DocState::Draft => "draft",
            DocState::Cancelled => "cancelled",
            DocState::Open { .. } => "open",
            DocState::Expired { .. } => "expired",
            DocState::Converted { .. } => "converted",
            DocState::ToInvoice => "to_invoice",
            DocState::Invoiced { .. } => "invoiced",
            DocState::Unpaid { overdue: true, .. } => "overdue",
            DocState::Unpaid { .. } => "unpaid",
            DocState::Paid => "paid",
            DocState::PartlyCredited { .. } => "partly_credited",
            DocState::Credited { .. } => "credited",
            DocState::Final => "final",
        }
    }
}

fn numbers(links: &[&LinkedDocument]) -> Vec<String> {
    links.iter().filter_map(|l| l.number.clone()).collect()
}

pub fn state(conn: &Connection, id: &str, today: &str) -> Result<DocState, AppError> {
    let doc = documents::get(conn, id)?;
    state_of(conn, &doc, today)
}

pub fn state_of(conn: &Connection, doc: &Document, today: &str) -> Result<DocState, AppError> {
    match doc.status {
        DocStatus::Draft => return Ok(DocState::Draft),
        DocStatus::Cancelled => return Ok(DocState::Cancelled),
        DocStatus::Validated => {}
    }
    let targets = links::targets(conn, &doc.id)?;
    let validated = |kinds: &[LinkKind]| -> Vec<&LinkedDocument> {
        targets
            .iter()
            .filter(|t| kinds.contains(&t.link.kind) && t.status == DocStatus::Validated)
            .collect()
    };
    Ok(match doc.doc_type {
        DocType::Quote | DocType::Proforma => {
            let into = validated(&[LinkKind::Converted, LinkKind::SoldFromQuote]);
            match (&doc.valid_until, into.is_empty()) {
                (_, false) => DocState::Converted {
                    into: numbers(&into),
                },
                (Some(v), true) if v.as_str() < today => DocState::Expired {
                    valid_until: v.clone(),
                },
                _ => DocState::Open {
                    valid_until: doc.valid_until.clone(),
                },
            }
        }
        DocType::DeliveryNote | DocType::Ticket => {
            match validated(&[LinkKind::Delivered, LinkKind::InvoicedTicket]).first() {
                Some(invoice) => DocState::Invoiced {
                    invoice: invoice.number.clone().unwrap_or_default(),
                },
                None if doc.doc_type == DocType::DeliveryNote => DocState::ToInvoice,
                None => DocState::Final,
            }
        }
        DocType::Invoice => {
            let credits = validated(&[LinkKind::Credited]);
            let credited = Money::checked_sum(
                credits
                    .iter()
                    .map(|c| c.total_due.checked_neg().unwrap_or(Money::ZERO)),
            )
            .unwrap_or(Money::ZERO);
            if credited.is_positive() && credited >= doc.total_ttc {
                DocState::Credited {
                    by: numbers(&credits),
                }
            } else if credited.is_positive() {
                DocState::PartlyCredited {
                    credited,
                    remaining: doc.total_ttc.checked_sub(credited).unwrap_or(Money::ZERO),
                }
            } else {
                let debt_of = if doc.origin == Some(DocOrigin::Ticket) {
                    links::sources(conn, &doc.id)?
                        .into_iter()
                        .find(|s| s.link.kind == LinkKind::InvoicedTicket)
                        .map_or_else(|| doc.id.clone(), |s| s.link.from_id)
                } else {
                    doc.id.clone()
                };
                let remaining: Money = conn.query_row(
                    "SELECT COALESCE(SUM(e.amount - COALESCE((SELECT SUM(a.amount) FROM customer_allocations a
                                                              WHERE a.debit_id = e.id), 0)), 0)
                     FROM customer_ledger e
                     WHERE e.ref_type = 'document' AND e.ref_id = ?1 AND e.kind = 'sale_credit'",
                    [&debt_of],
                    |r| r.get(0),
                )?;
                if remaining.is_positive() {
                    DocState::Unpaid {
                        remaining,
                        due_date: doc.due_date.clone(),
                        overdue: doc.due_date.as_deref().is_some_and(|d| d < today),
                    }
                } else {
                    DocState::Paid
                }
            }
        }
        _ => DocState::Final,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_state_has_the_code_of_its_export_tag() {
        let unpaid = |overdue| DocState::Unpaid {
            remaining: Money(1_800_000),
            due_date: Some("2026-10-28".into()),
            overdue,
        };
        let cases = [
            (DocState::Draft, "draft"),
            (DocState::Cancelled, "cancelled"),
            (DocState::Open { valid_until: None }, "open"),
            (
                DocState::Expired {
                    valid_until: "2026-09-01".into(),
                },
                "expired",
            ),
            (DocState::Converted { into: Vec::new() }, "converted"),
            (DocState::ToInvoice, "to_invoice"),
            (
                DocState::Invoiced {
                    invoice: "FA2026-000001".into(),
                },
                "invoiced",
            ),
            (unpaid(false), "unpaid"),
            (unpaid(true), "overdue"),
            (DocState::Paid, "paid"),
            (
                DocState::PartlyCredited {
                    credited: Money(100),
                    remaining: Money(900),
                },
                "partly_credited",
            ),
            (DocState::Credited { by: Vec::new() }, "credited"),
            (DocState::Final, "final"),
        ];
        let codes: Vec<&str> = cases.iter().map(|(_, code)| *code).collect();
        assert_eq!(codes, DocState::TAGS, "every code listed once, in order");
        for (state, code) in cases {
            assert_eq!(state.tag(), code, "{state:?}");
        }
    }
}

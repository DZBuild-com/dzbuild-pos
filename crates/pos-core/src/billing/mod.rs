//! Commercial documents of M3 (spec §9.2, mvp-backlog DOC): drafts typed in the editor or made
//! from another document, the checks of décret 05-468, the validation of quotes, proformas,
//! delivery notes and invoices, credit notes, states, the archive search and the print context.
//! Every write runs inside the caller's `Db::write` and appends its one audit row.

pub mod archive;
pub mod convert;
pub mod credit_notes;
pub mod drafts;
pub mod guard;
pub mod invoices;
pub mod mentions;
pub mod pricing;
pub mod print;
pub mod settings;
pub mod state;
pub mod validate;
pub mod warranty;

use crate::auth::Permission;
use crate::documents::DocType;

/// The kinds the editor and the archive handle. A customer order's state is its
/// `customer_orders.status` (the orders screen), not M3's generic document state.
pub const BILLING_KINDS: [DocType; 6] = [
    DocType::Invoice,
    DocType::Quote,
    DocType::Proforma,
    DocType::DeliveryNote,
    DocType::CreditNote,
    DocType::CustomerOrder,
];

/// The permission a kind needs (spec §4.5 "Documents").
pub const fn kind_permission(doc_type: DocType) -> Option<Permission> {
    match doc_type {
        DocType::Invoice => Some(Permission::DocumentsInvoice),
        DocType::Quote | DocType::Proforma | DocType::CustomerOrder => {
            Some(Permission::DocumentsQuote)
        }
        DocType::DeliveryNote => Some(Permission::DocumentsDelivery),
        DocType::CreditNote => Some(Permission::DocumentsCreditNote),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_billing_kind_has_its_permission_and_the_others_none() {
        assert_eq!(
            BILLING_KINDS.map(kind_permission),
            [
                Some(Permission::DocumentsInvoice),
                Some(Permission::DocumentsQuote),
                Some(Permission::DocumentsQuote),
                Some(Permission::DocumentsDelivery),
                Some(Permission::DocumentsCreditNote),
                Some(Permission::DocumentsQuote),
            ]
        );
        for other in DocType::ALL
            .into_iter()
            .filter(|t| !BILLING_KINDS.contains(t))
        {
            assert_eq!(kind_permission(other), None, "{other:?}");
        }
    }
}

//! The PDF of a ticket or a return note (SAL-052 "receipt as PDF"): the receipt model and its
//! words, rendered at 80 mm by the print host.

use crate::receipt::{Receipt, ReceiptLabels, labels, method_label};
use pos_core::lang::Lang;
use serde::Serialize;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptPage {
    pub receipt: Receipt,
    pub labels: ReceiptLabels,
    /// The words in a second language, only when it differs from the receipt's.
    pub second: Option<ReceiptLabels>,
    /// The tender name of each of `receipt.payments`, in order.
    pub methods: Vec<String>,
    /// The same in the second language (with `second`).
    pub second_methods: Option<Vec<String>>,
}

pub fn receipt_page(receipt: Receipt, second: Option<Lang>) -> ReceiptPage {
    let doc_type = receipt.doc_type;
    let second = second.filter(|l| *l != receipt.lang);
    let labels_in = |lang: Lang| labels(lang, doc_type);
    let methods_in = |lang: Lang| {
        receipt
            .payments
            .iter()
            .map(|p| method_label(lang, p.method))
            .collect::<Vec<_>>()
    };
    ReceiptPage {
        labels: labels_in(receipt.lang),
        second: second.map(labels_in),
        methods: methods_in(receipt.lang),
        second_methods: second.map(methods_in),
        receipt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::receipt::tests::sample_receipt;

    #[test]
    fn a_receipt_page_carries_its_words_and_a_second_language_only_when_it_differs() {
        let page = receipt_page(sample_receipt(Lang::Fr), Some(Lang::Ar));
        assert_eq!(page.labels.title, "Ticket de caisse");
        assert_eq!(
            page.second.as_ref().map(|s| s.title.as_str()),
            Some("تذكرة الصندوق")
        );
        assert_eq!(
            receipt_page(sample_receipt(Lang::Fr), Some(Lang::Fr)).second,
            None
        );
    }
}

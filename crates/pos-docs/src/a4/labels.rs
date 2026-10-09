//! The words of A4 documents in one language (block `a4` and the `doc.*` titles of the
//! document catalogues).

use crate::locales;
use pos_core::documents::DocType;
use pos_core::lang::Lang;
use serde::Serialize;
use specta::Type;

/// `key` in `lang` with each `{{name}}` replaced; the key itself if a catalogue lacks it (the
/// parity tests keep them complete).
pub fn fill(lang: Lang, key: &str, vars: &[(&str, &str)]) -> String {
    let template = locales::text(lang.as_str(), key).unwrap_or_else(|| key.to_owned());
    vars.iter().fold(template, |text, (name, value)| {
        text.replace(&format!("{{{{{name}}}}}"), value)
    })
}

pub fn word(lang: Lang, key: &str) -> String {
    fill(lang, key, &[])
}

pub fn title(lang: Lang, doc_type: DocType) -> String {
    word(
        lang,
        match doc_type {
            DocType::Ticket => "doc.ticket",
            DocType::Invoice => "doc.invoice",
            DocType::Proforma => "doc.proforma",
            DocType::Quote => "doc.quote",
            DocType::DeliveryNote => "doc.deliveryNote",
            DocType::CreditNote => "doc.creditNote",
            DocType::ReturnNote => "doc.returnNote",
            DocType::CustomerOrder => "doc.customerOrder",
            DocType::PurchaseOrder => "doc.purchaseOrder",
            DocType::GoodsReceipt => "doc.goodsReceipt",
            DocType::SupplierInvoice => "doc.supplierInvoice",
            DocType::Transfer => "doc.transfer",
        },
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DocumentLabels {
    pub number: String,
    pub date: String,
    pub valid_until: String,
    pub due_date: String,
    pub seller: String,
    pub customer: String,
    pub supplier: String,
    pub legal_form: String,
    pub activity: String,
    pub capital: String,
    pub address: String,
    pub phone: String,
    pub email: String,
    pub rc: String,
    pub ae_card: String,
    pub nif: String,
    pub nis: String,
    pub ai: String,
    pub nin: String,
    pub rib: String,
    pub designation: String,
    pub qty: String,
    pub unit: String,
    pub unit_price_ht: String,
    pub unit_price_ttc: String,
    pub discount: String,
    pub vat_rate: String,
    pub total_ht: String,
    pub total_ttc: String,
    pub subtotal_ht: String,
    pub discount_total: String,
    pub total_ttc_label: String,
    pub stamp_duty: String,
    pub stamp_absorbed: String,
    pub rounding: String,
    pub net_to_pay: String,
    pub payment_mode: String,
    pub paid: String,
    pub remaining: String,
    pub references: String,
    pub stamp_box: String,
    pub page: String,
    pub printed_at: String,
    pub from: String,
    pub to: String,
    pub driver: String,
    pub vehicle: String,
    pub note: String,
    pub terms: String,
    pub reason: String,
    pub received_by: String,
}

pub fn document_labels(lang: Lang) -> DocumentLabels {
    let l = |key: &str| word(lang, &format!("a4.{key}"));
    DocumentLabels {
        number: l("number"),
        date: l("date"),
        valid_until: l("validUntil"),
        due_date: l("dueDate"),
        seller: l("seller"),
        customer: l("customer"),
        supplier: l("supplier"),
        legal_form: l("legalForm"),
        activity: l("activity"),
        capital: l("capital"),
        address: l("address"),
        phone: l("phone"),
        email: l("email"),
        rc: l("rc"),
        ae_card: l("aeCard"),
        nif: l("nif"),
        nis: l("nis"),
        ai: l("ai"),
        nin: l("nin"),
        rib: l("rib"),
        designation: l("designation"),
        qty: l("qty"),
        unit: l("unit"),
        unit_price_ht: l("unitPriceHt"),
        unit_price_ttc: l("unitPriceTtc"),
        discount: l("discount"),
        vat_rate: l("vatRate"),
        total_ht: l("totalHt"),
        total_ttc: l("totalTtc"),
        subtotal_ht: l("subtotalHt"),
        discount_total: l("discountTotal"),
        total_ttc_label: l("totalTtcLabel"),
        stamp_duty: l("stampDuty"),
        stamp_absorbed: l("stampAbsorbed"),
        rounding: l("rounding"),
        net_to_pay: l("netToPay"),
        payment_mode: l("paymentMode"),
        paid: l("paid"),
        remaining: l("remaining"),
        references: l("references"),
        stamp_box: l("stampBox"),
        page: l("page"),
        printed_at: l("printedAt"),
        from: l("from"),
        to: l("to"),
        driver: l("driver"),
        vehicle: l("vehicle"),
        note: l("note"),
        terms: l("terms"),
        reason: l("reason"),
        received_by: l("receivedBy"),
    }
}

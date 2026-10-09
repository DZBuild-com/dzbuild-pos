//! The warranty certificate of décret 13-327 art. 6: guarantor with its RC, buyer, the invoice or
//! ticket number and date, type, brand and serial number (IMEI) of the goods, price, duration,
//! and the representative's signature.

use super::document::{PartyBlock, day, party_block};
use super::labels::{DocumentLabels, document_labels, title, word};
use super::qr::{QrMatrix, qr};
use pos_core::billing::settings::PrintSettings;
use pos_core::billing::warranty::WarrantySource;
use pos_core::datapack::Pack;
use pos_core::lang::Lang;
use pos_core::money::{Money, Qty};
use serde::Serialize;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WarrantyLabels {
    pub guarantor: String,
    pub buyer: String,
    pub document: String,
    pub product: String,
    pub brand: String,
    pub serial: String,
    pub price: String,
    pub duration: String,
    pub months: String,
    pub ends_on: String,
    pub conditions: String,
    pub representative: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WarrantyPrint {
    pub lang: Lang,
    pub rtl: bool,
    pub title: String,
    /// The guarantor, with its RC.
    pub seller: PartyBlock,
    pub buyer: Option<PartyBlock>,
    /// "Ticket de caisse", "Facture".
    pub document_title: String,
    pub document_number: String,
    /// `dd/mm/yyyy`.
    pub document_date: String,
    pub product: String,
    pub brand: Option<String>,
    pub serials: Vec<String>,
    pub price_ttc: Money,
    pub qty: Qty,
    pub months: i64,
    /// `dd/mm/yyyy`: the purchase day plus the months.
    pub ends_on: String,
    /// The shop's warranty terms (`documents.texts.warranty`).
    pub conditions: Option<String>,
    pub labels: WarrantyLabels,
    /// The words of the guarantor's and the buyer's cards (RC, NIF, address…).
    pub party_labels: DocumentLabels,
    /// `dd/mm/yyyy HH:MM`, Algiers time.
    pub printed_at: String,
    /// The number of the purchase document.
    pub qr: Option<QrMatrix>,
}

pub fn build_warranty(
    src: &WarrantySource,
    settings: &PrintSettings,
    pack: &Pack,
    lang: Lang,
    now: i64,
) -> WarrantyPrint {
    let l = |k: &str| word(lang, &format!("warranty.{k}"));
    let ends_on = chrono::NaiveDate::parse_from_str(&src.business_date, "%Y-%m-%d")
        .ok()
        .and_then(|d| {
            d.checked_add_months(chrono::Months::new(u32::try_from(src.months).unwrap_or(0)))
        })
        .map(|d| d.format("%d/%m/%Y").to_string())
        .unwrap_or_default();
    WarrantyPrint {
        lang,
        rtl: lang.is_rtl(),
        title: word(lang, "doc.warranty"),
        seller: party_block(&src.seller, pack, lang),
        buyer: src.buyer.as_ref().map(|b| party_block(b, pack, lang)),
        document_title: title(lang, src.doc_type),
        document_number: src.number.clone(),
        document_date: day(&src.business_date),
        product: src.product.clone(),
        brand: src.brand.clone(),
        serials: src.serials.clone(),
        price_ttc: src.unit_price_ttc,
        qty: src.qty,
        months: src.months,
        ends_on,
        conditions: settings.texts.warranty.in_lang(lang).map(str::to_owned),
        labels: WarrantyLabels {
            guarantor: l("guarantor"),
            buyer: l("buyer"),
            document: l("document"),
            product: l("product"),
            brand: l("brand"),
            serial: l("serial"),
            price: l("price"),
            duration: l("duration"),
            months: l("months"),
            ends_on: l("endsOn"),
            conditions: l("conditions"),
            representative: l("representative"),
            signature: l("signature"),
        },
        party_labels: document_labels(lang),
        printed_at: pos_core::time::algiers_datetime(now)
            .format("%d/%m/%Y %H:%M")
            .to_string(),
        qr: qr(&src.number),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::a4::testdata;
    use pos_core::billing::warranty::WarrantySource;
    use pos_core::documents::DocType;

    pub(crate) fn source() -> WarrantySource {
        WarrantySource {
            document_id: "tk-1".into(),
            doc_type: DocType::Ticket,
            number: "TK2026-C1-000123".into(),
            business_date: "2026-09-28".into(),
            seller: testdata::seller(),
            buyer: None,
            line_id: "l1".into(),
            product: "Redmi 13 128 Go".into(),
            brand: Some("Xiaomi".into()),
            serials: vec!["356789104512345".into()],
            unit_price_ttc: Money(3_500_000),
            qty: Qty(1_000),
            months: 12,
        }
    }

    #[test]
    fn a_certificate_carries_the_decree_mentions_and_its_end_date() {
        let pack = pos_core::datapack::bundled().unwrap();
        let w = build_warranty(
            &source(),
            &testdata::settings(),
            &pack,
            Lang::Fr,
            testdata::PRINTED_AT,
        );
        assert_eq!(w.title, "Certificat de garantie");
        assert_eq!(w.document_title, "Ticket de caisse");
        assert_eq!((w.months, w.ends_on.as_str()), (12, "28/09/2027"));
        assert_eq!(w.serials, ["356789104512345"]);
        assert!(
            w.seller.ids.iter().any(|i| i.value == "16/00-1234567B26"),
            "guarantor with its RC"
        );
        assert_eq!(w.price_ttc, Money(3_500_000));
        assert_eq!(
            w.conditions.as_deref(),
            Some("Garantie contre les défauts de fabrication, hors casse et oxydation.")
        );
    }
}

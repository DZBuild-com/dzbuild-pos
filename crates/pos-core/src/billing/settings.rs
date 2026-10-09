//! Document settings (spec §9.2: header/footer texts, logo, colours and the bilingual default are
//! editable; the mandatory mentions are not) and the fiscal identity the company row lacks:
//! the nature of the activity (décret 05-468 art. 3) and the auto-entrepreneur card (loi 22-23).

use crate::error::AppError;
use crate::lang::Lang;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;

/// One text in the three languages.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, deny_unknown_fields)]
pub struct Trilingual {
    pub ar: String,
    pub fr: String,
    pub en: String,
}

impl Trilingual {
    /// The text in `lang`, else in French, Arabic, English (spec §5 fallback); `None` when empty.
    pub fn in_lang(&self, lang: Lang) -> Option<&str> {
        let preferred = match lang {
            Lang::Ar => &self.ar,
            Lang::Fr => &self.fr,
            Lang::En => &self.en,
        };
        [preferred, &self.fr, &self.ar, &self.en]
            .into_iter()
            .map(|s| s.trim())
            .find(|s| !s.is_empty())
    }

    /// The text in any language (French first), `None` when all three are empty.
    pub fn any(&self) -> Option<&str> {
        self.in_lang(Lang::Fr)
    }

    fn fits(&self, max: usize) -> bool {
        [&self.ar, &self.fr, &self.en]
            .iter()
            .all(|s| s.chars().count() <= max && !s.chars().any(|c| c.is_control() && c != '\n'))
    }
}

/// The accent of the printed documents: a token of spec §10.1, never a free colour.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Accent {
    #[default]
    Brand,
    Ink,
    Info,
    Ok,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Paper {
    #[default]
    A4,
    A5,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentLayout {
    pub accent: Accent,
    pub show_logo: bool,
    pub paper: Paper,
    /// Second language of new documents (bilingual AR + FR by default when set).
    pub second_lang: Option<Lang>,
    /// Prices and totals on delivery notes.
    pub show_delivery_prices: bool,
    /// The stamp-and-signature box on documents other than invoices and credit notes (always
    /// printed on those, décret 05-468 art. 4).
    pub show_stamp_box: bool,
}

impl Default for DocumentLayout {
    fn default() -> Self {
        DocumentLayout {
            accent: Accent::Brand,
            show_logo: true,
            paper: Paper::A4,
            second_lang: None,
            show_delivery_prices: true,
            show_stamp_box: true,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct DocumentTexts {
    /// Under the shop's identity.
    pub header: Trilingual,
    /// At the bottom of every page.
    pub footer: Trilingual,
    /// Terms of sale and return policy (DOC-019).
    pub terms: Trilingual,
    /// Conditions printed on warranty certificates (DOC-012).
    pub warranty: Trilingual,
}

/// Everything the print model needs besides the document and the company row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PrintSettings {
    pub layout: DocumentLayout,
    pub texts: DocumentTexts,
    pub activity: Trilingual,
    pub ae_card: Option<String>,
    /// Social handles and website (DOC-014), printed in the footer.
    pub social: Vec<String>,
}

pub const HEADER_MAX: usize = 200;
pub const TERMS_MAX: usize = 600;
pub const ACTIVITY_MAX: usize = 120;

pub fn trilingual_ok(v: &Value, max: usize) -> bool {
    serde_json::from_value::<Trilingual>(v.clone()).is_ok_and(|t| t.fits(max))
}

pub fn layout_ok(v: &Value) -> bool {
    serde_json::from_value::<DocumentLayout>(v.clone()).is_ok()
}

pub fn texts_ok(v: &Value) -> bool {
    serde_json::from_value::<DocumentTexts>(v.clone()).is_ok_and(|t| {
        t.header.fits(HEADER_MAX)
            && t.footer.fits(HEADER_MAX)
            && t.terms.fits(TERMS_MAX)
            && t.warranty.fits(TERMS_MAX)
    })
}

pub fn print_settings(conn: &Connection) -> Result<PrintSettings, AppError> {
    let ae_card: String = crate::settings::get(conn, "company.ae_card")?;
    let social: Value = crate::settings::get_value(conn, "company.social")?;
    let social = ["facebook", "instagram", "tiktok", "website"]
        .into_iter()
        .filter_map(|k| social.get(k).and_then(Value::as_str))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    Ok(PrintSettings {
        layout: crate::settings::get(conn, "documents.layout")?,
        texts: crate::settings::get(conn, "documents.texts")?,
        activity: crate::settings::get(conn, "company.activity")?,
        ae_card: Some(ae_card.trim().to_owned()).filter(|s| !s.is_empty()),
        social,
    })
}

pub fn quote_validity_days(conn: &Connection) -> Result<i64, AppError> {
    crate::settings::get(conn, "documents.quote_validity_days")
}

pub fn invoice_due_days(conn: &Connection) -> Result<i64, AppError> {
    crate::settings::get(conn, "documents.invoice_due_days")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::shop;
    use serde_json::json;

    #[test]
    fn document_settings_have_safe_defaults() {
        let shop = shop();
        let s = shop.db.read(print_settings).unwrap();
        assert_eq!(
            s.layout,
            DocumentLayout {
                accent: Accent::Brand,
                show_logo: true,
                paper: Paper::A4,
                second_lang: None,
                show_delivery_prices: true,
                show_stamp_box: true,
            }
        );
        assert_eq!(s.texts, DocumentTexts::default());
        assert_eq!(s.activity, Trilingual::default());
        assert_eq!(s.ae_card, None);
        assert_eq!(shop.db.read(quote_validity_days).unwrap(), 30);
        assert_eq!(shop.db.read(invoice_due_days).unwrap(), 30);
    }

    #[test]
    fn settings_are_validated_and_read_back() {
        let shop = shop();
        shop.set(
            "company.activity",
            json!({ "ar": "بيع مواد البناء", "fr": "Vente de matériaux de construction", "en": "" }),
        );
        shop.set("company.ae_card", json!("AE-16-000123"));
        shop.set(
            "company.social",
            json!({ "facebook": "elamel.dz", "website": "elamel.dz" }),
        );
        shop.set(
            "documents.layout",
            json!({ "accent": "ink", "showLogo": false, "paper": "a5", "secondLang": "ar",
                    "showDeliveryPrices": false, "showStampBox": true }),
        );
        let s = shop.db.read(print_settings).unwrap();
        assert_eq!(
            s.activity.in_lang(Lang::En),
            Some("Vente de matériaux de construction"),
            "fr fallback"
        );
        assert_eq!(s.activity.in_lang(Lang::Ar), Some("بيع مواد البناء"));
        assert_eq!(s.ae_card.as_deref(), Some("AE-16-000123"));
        assert_eq!(s.social, ["elamel.dz", "elamel.dz"]);
        assert_eq!(
            (s.layout.paper, s.layout.second_lang),
            (Paper::A5, Some(Lang::Ar))
        );
        for (key, bad) in [
            ("documents.layout", json!({ "accent": "pink" })),
            (
                "documents.texts",
                json!({ "header": { "ar": "", "fr": "x".repeat(201), "en": "" } }),
            ),
            ("company.activity", json!({ "fr": "x".repeat(121) })),
            ("documents.quote_validity_days", json!(0)),
            ("documents.invoice_due_days", json!(366)),
            ("documents.arabic_guard", json!("always")),
        ] {
            let actor = shop.owner.actor();
            let err = shop
                .db
                .write(move |tx| crate::settings::set_value(tx, &actor, key, bad))
                .unwrap_err();
            assert_eq!(err.code(), "value_invalid", "{key}");
        }
    }
}

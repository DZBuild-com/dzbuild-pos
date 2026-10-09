//! The mandatory mentions of décret 05-468 art. 3 (and loi 04-02 art. 34 for the NIF): which
//! ones are missing on the seller's side (company row and fiscal settings) and on the buyer's
//! (the customer file), which ones block an invoice or a credit note (DOC-003), which ones are
//! warnings. See the table of the M3 plan's Global Constraints.

use super::settings::print_settings;
use crate::company;
use crate::customers::{self, Customer, CustomerType};
use crate::documents::DocType;
use crate::error::AppError;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Mention {
    Name,
    LegalForm,
    Activity,
    Capital,
    Address,
    Phone,
    Rc,
    Nif,
    Nis,
    Ai,
}

impl Mention {
    pub const fn as_str(self) -> &'static str {
        match self {
            Mention::Name => "name",
            Mention::LegalForm => "legal_form",
            Mention::Activity => "activity",
            Mention::Capital => "capital",
            Mention::Address => "address",
            Mention::Phone => "phone",
            Mention::Rc => "rc",
            Mention::Nif => "nif",
            Mention::Nis => "nis",
            Mention::Ai => "ai",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum BuyerKind {
    /// A company, a reseller or anyone with a NIF: identity and tax numbers.
    Business,
    /// A private person: name and address (décret 05-468 art. 3-2°).
    Consumer,
    /// No customer on the document.
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MentionCheck {
    pub seller_missing: Vec<Mention>,
    pub seller_warnings: Vec<Mention>,
    pub buyer_kind: BuyerKind,
    pub buyer_missing: Vec<Mention>,
    pub buyer_warnings: Vec<Mention>,
}

impl MentionCheck {
    /// No blocking mention is missing on either side (warnings do not count).
    pub fn complete(&self) -> bool {
        self.seller_missing.is_empty() && self.buyer_missing.is_empty()
    }
}

/// Legal forms that have a share capital (compared without dots, spaces or case).
pub const COMPANY_FORMS: [&str; 9] = [
    "SARL", "EURL", "SPA", "SNC", "SCS", "SCA", "SAS", "SPAS", "SPASU",
];

fn blank(v: &Option<String>) -> bool {
    v.as_deref().is_none_or(|s| s.trim().is_empty())
}

/// `SARL`, `S.A.R.L.`, `sarl`, `EPE SPA`: a word of the legal form, dots dropped, is a company
/// form.
fn company_form(legal_form: &str) -> bool {
    legal_form
        .split(|c: char| c.is_whitespace() || c == '-' || c == '/')
        .map(|word| {
            word.chars()
                .filter(char::is_ascii_alphabetic)
                .collect::<String>()
                .to_ascii_uppercase()
        })
        .any(|word| COMPANY_FORMS.contains(&word.as_str()))
}

/// A company, a reseller or any customer with a NIF is a business buyer; the rest are consumers.
pub fn buyer_kind(c: &Customer) -> BuyerKind {
    if c.customer_type != CustomerType::Individual || !blank(&c.nif) {
        BuyerKind::Business
    } else {
        BuyerKind::Consumer
    }
}

/// What is missing for a document of this seller to `customer_id` (or to nobody).
pub fn check(conn: &Connection, customer_id: Option<&str>) -> Result<MentionCheck, AppError> {
    let company = company::get(conn)?.ok_or_else(|| AppError::conflict("setup_required"))?;
    let settings = print_settings(conn)?;
    let mut seller_missing = Vec::new();
    let mut seller_warnings = Vec::new();
    let has_name = [&company.name_ar, &company.name_fr, &company.name_en]
        .iter()
        .any(|n| !n.trim().is_empty());
    if !has_name {
        seller_missing.push(Mention::Name);
    }
    if blank(&company.legal_form) {
        seller_missing.push(Mention::LegalForm);
    }
    if settings.activity.any().is_none() {
        seller_missing.push(Mention::Activity);
    }
    if blank(&company.address) {
        seller_missing.push(Mention::Address);
    }
    if blank(&company.rc) && settings.ae_card.is_none() {
        seller_missing.push(Mention::Rc);
    }
    if blank(&company.nif) {
        seller_missing.push(Mention::Nif);
    }
    if blank(&company.nis) {
        seller_missing.push(Mention::Nis);
    }
    if company.capital.is_none() && company.legal_form.as_deref().is_some_and(company_form) {
        seller_warnings.push(Mention::Capital);
    }
    if blank(&company.phone) {
        seller_warnings.push(Mention::Phone);
    }
    if blank(&company.ai) {
        seller_warnings.push(Mention::Ai);
    }
    let (buyer_kind, buyer_missing, buyer_warnings) = match customer_id {
        None => (
            BuyerKind::Missing,
            vec![Mention::Name, Mention::Address],
            Vec::new(),
        ),
        Some(id) => {
            let c = customers::get(conn, id)?;
            let kind = buyer_kind(&c);
            let mut missing = Vec::new();
            let mut warnings = Vec::new();
            if c.name.trim().is_empty() {
                missing.push(Mention::Name);
            }
            if blank(&c.address) {
                missing.push(Mention::Address);
            }
            if kind == BuyerKind::Business {
                if blank(&c.nif) {
                    missing.push(Mention::Nif);
                }
                if blank(&c.rc) {
                    missing.push(Mention::Rc);
                }
                for (mention, value) in [
                    (Mention::LegalForm, &c.legal_form),
                    (Mention::Activity, &c.activity),
                    (Mention::Nis, &c.nis),
                    (Mention::Ai, &c.ai),
                ] {
                    if blank(value) {
                        warnings.push(mention);
                    }
                }
            }
            (kind, missing, warnings)
        }
    };
    Ok(MentionCheck {
        seller_missing,
        seller_warnings,
        buyer_kind,
        buyer_missing,
        buyer_warnings,
    })
}

/// The check, refusing an invoice or a credit note with a blocking mention missing (DOC-003).
pub fn require(
    conn: &Connection,
    doc_type: DocType,
    customer_id: Option<&str>,
) -> Result<MentionCheck, AppError> {
    let check = check(conn, customer_id)?;
    if matches!(doc_type, DocType::Invoice | DocType::CreditNote) && !check.complete() {
        let names = |m: &[Mention]| m.iter().map(|x| x.as_str()).collect::<Vec<_>>();
        return Err(AppError::rejected(
            json!({ "seller": names(&check.seller_missing), "buyer": names(&check.buyer_missing) }),
            "mentions_missing",
        ));
    }
    Ok(check)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::customers::CustomerInput;
    use crate::testkit::shop;
    use serde_json::json;

    #[test]
    fn an_invoice_needs_every_blocking_mention_of_both_parties() {
        let shop = shop(); // name, address and NIF only
        let amine = shop.b2b_customer("SARL Amine");
        let check = {
            let id = amine.id.clone();
            shop.db.read(move |c| check(c, Some(&id))).unwrap()
        };
        assert_eq!(
            check.seller_missing,
            [
                Mention::LegalForm,
                Mention::Activity,
                Mention::Rc,
                Mention::Nis
            ]
        );
        assert_eq!(check.buyer_kind, BuyerKind::Business);
        assert!(check.buyer_missing.is_empty());
        let id = amine.id.clone();
        let err = shop
            .db
            .read(move |c| require(c, DocType::Invoice, Some(&id)))
            .unwrap_err();
        assert_eq!(err.code(), "mentions_missing");
        assert_eq!(
            err.payload().details,
            Some(json!({ "seller": ["legal_form", "activity", "rc", "nis"], "buyer": [] }))
        );
        let id = amine.id.clone();
        let quote = shop
            .db
            .read(move |c| require(c, DocType::Quote, Some(&id)))
            .unwrap();
        assert!(!quote.complete(), "a quote validates with warnings");

        shop.complete_company();
        let id = amine.id.clone();
        let ok = shop
            .db
            .read(move |c| require(c, DocType::Invoice, Some(&id)))
            .unwrap();
        assert!(ok.complete());
        assert_eq!(ok.seller_warnings, Vec::<Mention>::new());
    }

    #[test]
    fn business_and_consumer_buyers_have_their_own_rules() {
        let shop = shop();
        shop.complete_company();
        let bare = shop.customer("Quincaillerie Amine"); // known by its name only
        let actor = shop.owner.actor();
        let pack = shop.pack.clone();
        let id = bare.id.clone();
        shop.db
            .write(move |tx| {
                let mut input = CustomerInput::named("Quincaillerie Amine");
                input.customer_type = crate::customers::CustomerType::Company;
                input.address = Some("5 rue Didouche Mourad, Alger".into());
                crate::customers::update(tx, &actor, &pack, &id, input).map(drop)
            })
            .unwrap();
        let id = bare.id.clone();
        let check = shop.db.read(move |c| check(c, Some(&id))).unwrap();
        assert_eq!(check.buyer_kind, BuyerKind::Business);
        assert_eq!(check.buyer_missing, [Mention::Nif, Mention::Rc]);
        assert_eq!(
            check.buyer_warnings,
            [
                Mention::LegalForm,
                Mention::Activity,
                Mention::Nis,
                Mention::Ai
            ]
        );
        let karim = shop.consumer("Karim Bensalah");
        let id = karim.id.clone();
        let check = shop.db.read(move |c| super::check(c, Some(&id))).unwrap();
        assert_eq!(
            (check.buyer_kind, check.buyer_missing.len()),
            (BuyerKind::Consumer, 0)
        );
        let nobody = shop.db.read(|c| super::check(c, None)).unwrap();
        assert_eq!(
            (nobody.buyer_kind, nobody.buyer_missing),
            (BuyerKind::Missing, vec![Mention::Name, Mention::Address])
        );
    }

    #[test]
    fn a_company_form_without_capital_is_a_warning() {
        let shop = shop();
        shop.complete_company();
        let actor = shop.owner.actor();
        let pack = shop.pack.clone();
        shop.db
            .write(move |tx| {
                let mut input = crate::company::CompanyInput::from(
                    crate::company::get(tx)?.ok_or_else(|| AppError::conflict("setup_required"))?,
                );
                input.capital = None;
                input.legal_form = Some("S.A.R.L.".into());
                crate::company::save(tx, &actor, &pack, input).map(drop)
            })
            .unwrap();
        let check = shop.db.read(|c| check(c, None)).unwrap();
        assert!(check.seller_missing.is_empty());
        assert_eq!(check.seller_warnings, [Mention::Capital]);
    }
}

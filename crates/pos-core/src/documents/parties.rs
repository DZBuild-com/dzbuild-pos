//! Seller and buyer as they were when a document was validated (décret 05-468 art. 3): what the
//! invoice shows today and on every reprint in ten years, whatever the company or the customer
//! file says by then. Written while the document is a draft, frozen by the triggers of 0004,
//! and part of the document hash (`ext.parties`).

use crate::error::AppError;
use crate::money::Money;
use crate::sqlenum::sql_text_enum;
use rusqlite::{Connection, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use specta::Type;

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum PartyRole {
    #[default]
    Seller,
    Buyer,
}

impl PartyRole {
    pub const ALL: [PartyRole; 2] = [PartyRole::Seller, PartyRole::Buyer];

    pub const fn as_str(self) -> &'static str {
        match self {
            PartyRole::Seller => "seller",
            PartyRole::Buyer => "buyer",
        }
    }

    pub fn parse(s: &str) -> Option<PartyRole> {
        PartyRole::ALL.into_iter().find(|r| r.as_str() == s)
    }
}

sql_text_enum!(PartyRole);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DocumentParty {
    pub role: PartyRole,
    /// The customer id of a buyer; `None` for the seller.
    pub party_id: Option<String>,
    /// In the document language (the seller's name has three; a customer has one).
    pub name: String,
    /// In the second language of a bilingual document.
    pub name_alt: Option<String>,
    pub legal_form: Option<String>,
    pub activity: Option<String>,
    pub activity_alt: Option<String>,
    pub capital: Option<Money>,
    pub address: Option<String>,
    /// Wilaya code (`"16"`); its name is printed in the document language.
    pub wilaya: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub rc: Option<String>,
    /// Carte d'auto-entrepreneur, printed instead of the RC (loi 22-23).
    pub ae_card: Option<String>,
    pub nif: Option<String>,
    pub nis: Option<String>,
    pub ai: Option<String>,
    pub nin: Option<String>,
    pub rib: Option<String>,
    /// A VAT-exempt buyer's attestation number (TAX-004).
    pub vat_exempt_ref: Option<String>,
}

impl DocumentParty {
    pub fn named(role: PartyRole, name: &str) -> DocumentParty {
        DocumentParty {
            role,
            name: name.to_owned(),
            ..DocumentParty::default()
        }
    }
}

/// Records (or, on a draft, replaces) the party of `role`.
pub fn record(tx: &Transaction<'_>, document_id: &str, p: &DocumentParty) -> Result<(), AppError> {
    if p.name.trim().is_empty() {
        return Err(AppError::validation("name", "required"));
    }
    tx.prepare_cached(
        "INSERT INTO document_parties (document_id, role, party_id, name, name_alt, legal_form, activity,
             activity_alt, capital, address, wilaya, phone, email, rc, ae_card, nif, nis, ai, nin, rib,
             vat_exempt_ref)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)
         ON CONFLICT (document_id, role) DO UPDATE SET party_id = excluded.party_id, name = excluded.name,
             name_alt = excluded.name_alt, legal_form = excluded.legal_form, activity = excluded.activity,
             activity_alt = excluded.activity_alt, capital = excluded.capital, address = excluded.address,
             wilaya = excluded.wilaya, phone = excluded.phone, email = excluded.email, rc = excluded.rc,
             ae_card = excluded.ae_card, nif = excluded.nif, nis = excluded.nis, ai = excluded.ai,
             nin = excluded.nin, rib = excluded.rib, vat_exempt_ref = excluded.vat_exempt_ref",
    )?
    .execute(params![
        document_id,
        p.role,
        p.party_id,
        p.name,
        p.name_alt,
        p.legal_form,
        p.activity,
        p.activity_alt,
        p.capital,
        p.address,
        p.wilaya,
        p.phone,
        p.email,
        p.rc,
        p.ae_card,
        p.nif,
        p.nis,
        p.ai,
        p.nin,
        p.rib,
        p.vat_exempt_ref
    ])?;
    Ok(())
}

/// The parties of a document, seller first.
pub fn of(conn: &Connection, document_id: &str) -> Result<Vec<DocumentParty>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT role, party_id, name, name_alt, legal_form, activity, activity_alt, capital, address, wilaya,
                phone, email, rc, ae_card, nif, nis, ai, nin, rib, vat_exempt_ref
         FROM document_parties WHERE document_id = ?1 ORDER BY role = 'buyer'",
    )?;
    let rows = stmt
        .query_map([document_id], |r| {
            Ok(DocumentParty {
                role: r.get(0)?,
                party_id: r.get(1)?,
                name: r.get(2)?,
                name_alt: r.get(3)?,
                legal_form: r.get(4)?,
                activity: r.get(5)?,
                activity_alt: r.get(6)?,
                capital: r.get(7)?,
                address: r.get(8)?,
                wilaya: r.get(9)?,
                phone: r.get(10)?,
                email: r.get(11)?,
                rc: r.get(12)?,
                ae_card: r.get(13)?,
                nif: r.get(14)?,
                nis: r.get(15)?,
                ai: r.get(16)?,
                nin: r.get(17)?,
                rib: r.get(18)?,
                vat_exempt_ref: r.get(19)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Every field of a party, for the hash.
pub fn party_json(p: &DocumentParty) -> Value {
    json!({
        "role": p.role,
        "partyId": p.party_id,
        "name": p.name,
        "nameAlt": p.name_alt,
        "legalForm": p.legal_form,
        "activity": p.activity,
        "activityAlt": p.activity_alt,
        "capital": p.capital,
        "address": p.address,
        "wilaya": p.wilaya,
        "phone": p.phone,
        "email": p.email,
        "rc": p.rc,
        "aeCard": p.ae_card,
        "nif": p.nif,
        "nis": p.nis,
        "ai": p.ai,
        "nin": p.nin,
        "rib": p.rib,
        "vatExemptRef": p.vat_exempt_ref,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::documents::tests_support::invoice_draft;
    use crate::testkit::shop;

    #[test]
    fn parties_are_recorded_on_drafts_and_read_seller_first() {
        let shop = shop();
        let id = invoice_draft(&shop.db);
        let for_tx = id.clone();
        shop.db
            .write(move |tx| {
                let mut buyer = DocumentParty::named(PartyRole::Buyer, "SARL Amine");
                buyer.rc = Some("16/00-1234567B26".into());
                record(tx, &for_tx, &buyer)?;
                buyer.rc = Some("16/00-7654321B26".into());
                record(tx, &for_tx, &buyer)?;
                record(
                    tx,
                    &for_tx,
                    &DocumentParty::named(PartyRole::Seller, "Supérette El Amel"),
                )
            })
            .unwrap();
        let parties = shop.db.read(move |c| of(c, &id)).unwrap();
        assert_eq!(
            parties
                .iter()
                .map(|p| (p.role, p.name.as_str()))
                .collect::<Vec<_>>(),
            [
                (PartyRole::Seller, "Supérette El Amel"),
                (PartyRole::Buyer, "SARL Amine")
            ]
        );
        assert_eq!(
            parties[1].rc.as_deref(),
            Some("16/00-7654321B26"),
            "a draft's party is replaced"
        );
    }
}

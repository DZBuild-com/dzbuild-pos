//! Everything a printed page needs besides the words (pos-docs builds the model from it): the
//! document, the print languages, seller and counterpart (the snapshot of the day of the
//! document when there is one), the documents it cites, how it was settled, its state (for the
//! "annulée" and "expirée" marks), the line names in the second language and the unit names.

use super::settings::{PrintSettings, print_settings};
use super::state::{DocState, state_of};
use super::validate::{buyer_party, in_lang, seller_party};
use crate::company::Company;
use crate::documents::links::{self, LinkKind};
use crate::documents::{self, DocStatus, DocType, Document, DocumentParty, PartyRole};
use crate::error::AppError;
use crate::lang::Lang;
use crate::payments::PaymentMethod;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PrintOptions {
    /// The document's language when absent (UX-004: another one at reprint).
    pub lang: Option<Lang>,
    /// The document's second language when absent.
    pub second_lang: Option<Lang>,
    /// One language only, even for a bilingual document.
    pub monolingual: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Reference {
    pub doc_type: DocType,
    pub number: String,
    pub business_date: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PaidBy {
    pub reference: Reference,
    pub methods: Vec<PaymentMethod>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TransferInfo {
    pub from: String,
    pub to: String,
    pub driver_name: Option<String>,
    pub vehicle: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PrintContext {
    pub document: Document,
    pub lang: Lang,
    pub second_lang: Option<Lang>,
    pub seller: DocumentParty,
    pub counterpart: Option<DocumentParty>,
    pub counterpart_is_supplier: bool,
    pub settings: PrintSettings,
    pub references: Vec<Reference>,
    pub paid_by: Option<PaidBy>,
    pub state: DocState,
    /// Line id → name in the second language.
    pub alt_names: BTreeMap<String, String>,
    /// Unit code → name in the print language.
    pub unit_names: BTreeMap<String, String>,
    pub transfer: Option<TransferInfo>,
    pub has_logo: bool,
    pub today: String,
    pub printed_at: i64,
}

/// The snapshot's name (or activity) in `lang` when the document was stored in that language
/// (an empty one stays empty; a missing second value means "the same as the first", see
/// `validate::seller_party`), the shop's current one only for a language it never had.
fn pick(
    lang: Lang,
    doc: &Document,
    stored: &str,
    stored_alt: Option<&str>,
    current: &str,
) -> String {
    if lang == doc.lang {
        stored.to_owned()
    } else if Some(lang) == doc.second_lang {
        stored_alt.unwrap_or(stored).to_owned()
    } else {
        current.to_owned()
    }
}

fn reference(doc: &Document) -> Option<Reference> {
    Some(Reference {
        doc_type: doc.doc_type,
        number: doc.number.clone()?,
        business_date: doc.business_date.clone(),
    })
}

/// The seller snapshot with its names and activity in the print languages: the snapshot's
/// when the document was stored in that language, the shop's current ones otherwise.
fn seller_in(
    snapshot: &DocumentParty,
    doc: &Document,
    company: &Company,
    settings: &PrintSettings,
    lang: Lang,
    second_lang: Option<Lang>,
) -> DocumentParty {
    let current_name = |l: Lang| in_lang(l, &company.name_ar, &company.name_fr, &company.name_en);
    let current_activity = |l: Lang| settings.activity.in_lang(l).unwrap_or("").to_owned();
    let stored_activity = snapshot.activity.as_deref().unwrap_or("");
    let name = pick(
        lang,
        doc,
        &snapshot.name,
        snapshot.name_alt.as_deref(),
        &current_name(lang),
    );
    let activity = pick(
        lang,
        doc,
        stored_activity,
        snapshot.activity_alt.as_deref(),
        &current_activity(lang),
    );
    let name_alt = second_lang
        .map(|l| {
            pick(
                l,
                doc,
                &snapshot.name,
                snapshot.name_alt.as_deref(),
                &current_name(l),
            )
        })
        .filter(|n| !n.is_empty() && *n != name);
    let activity_alt = second_lang
        .map(|l| {
            pick(
                l,
                doc,
                stored_activity,
                snapshot.activity_alt.as_deref(),
                &current_activity(l),
            )
        })
        .filter(|a| !a.is_empty() && *a != activity);
    DocumentParty {
        name,
        name_alt,
        activity: Some(activity).filter(|a| !a.is_empty()),
        activity_alt,
        ..snapshot.clone()
    }
}

/// The counterpart of a document without a buyer snapshot: its customer (or supplier) today.
fn current_counterpart(
    conn: &Connection,
    doc: &Document,
) -> Result<(Option<DocumentParty>, bool), AppError> {
    if let Some(customer) = &doc.customer_id {
        return Ok((
            Some(buyer_party(&crate::customers::get(conn, customer)?)),
            false,
        ));
    }
    let Some(supplier) = &doc.supplier_id else {
        return Ok((None, false));
    };
    let s = crate::suppliers::get(conn, supplier)?;
    Ok((
        Some(DocumentParty {
            role: PartyRole::Buyer,
            party_id: Some(s.id),
            name: s.name,
            address: s.address,
            wilaya: s.wilaya,
            phone: s.phone,
            email: s.email,
            rc: s.rc,
            nif: s.nif,
            nis: s.nis,
            ai: s.ai,
            nin: s.nin,
            rib: s.rib,
            ..DocumentParty::default()
        }),
        true,
    ))
}

/// The ticket an invoice was made from, with how it was paid (change left out).
fn paid_by_ticket(conn: &Connection, ticket_id: &str) -> Result<Option<PaidBy>, AppError> {
    let ticket = documents::get(conn, ticket_id)?;
    let mut methods: Vec<PaymentMethod> = Vec::new();
    for m in ticket.payments.iter().map(|p| p.method) {
        if m != PaymentMethod::Change && !methods.contains(&m) {
            methods.push(m);
        }
    }
    Ok(reference(&ticket).map(|reference| PaidBy { reference, methods }))
}

fn transfer_info(
    conn: &Connection,
    document_id: &str,
    lang: Lang,
) -> Result<Option<TransferInfo>, AppError> {
    Ok(conn
        .query_row(
            "SELECT f.name_ar, f.name_fr, f.name_en, t.name_ar, t.name_fr, t.name_en, x.driver_name, x.vehicle
             FROM stock_transfers x
             JOIN stock_locations f ON f.id = x.from_location_id
             JOIN stock_locations t ON t.id = x.to_location_id
             WHERE x.document_id = ?1",
            [document_id],
            |r| {
                Ok(TransferInfo {
                    from: in_lang(
                        lang,
                        &r.get::<_, String>(0)?,
                        &r.get::<_, String>(1)?,
                        &r.get::<_, String>(2)?,
                    ),
                    to: in_lang(
                        lang,
                        &r.get::<_, String>(3)?,
                        &r.get::<_, String>(4)?,
                        &r.get::<_, String>(5)?,
                    ),
                    driver_name: r.get(6)?,
                    vehicle: r.get(7)?,
                })
            },
        )
        .optional()?)
}

/// What printing `id` in the languages of `options` needs, as of `today` (its state) and `now`.
pub fn print_context(
    conn: &Connection,
    id: &str,
    options: &PrintOptions,
    today: &str,
    now: i64,
) -> Result<PrintContext, AppError> {
    let doc = documents::get(conn, id)?;
    let lang = options.lang.unwrap_or(doc.lang);
    let second_lang = if options.monolingual {
        None
    } else {
        options
            .second_lang
            .or(doc.second_lang)
            .filter(|l| *l != lang)
    };
    let company = crate::company::get(conn)?.ok_or_else(|| AppError::conflict("setup_required"))?;
    let settings = print_settings(conn)?;
    let seller = match doc.parties.iter().find(|p| p.role == PartyRole::Seller) {
        Some(snapshot) => seller_in(snapshot, &doc, &company, &settings, lang, second_lang),
        None => seller_party(conn, lang, second_lang)?,
    };
    let (counterpart, counterpart_is_supplier) =
        match doc.parties.iter().find(|p| p.role == PartyRole::Buyer) {
            Some(buyer) => (Some(buyer.clone()), false),
            None => current_counterpart(conn, &doc)?,
        };
    let sources = links::sources(conn, &doc.id)?;
    let mut references: Vec<Reference> = sources
        .iter()
        .filter(|s| s.status == DocStatus::Validated && s.link.kind != LinkKind::Credited)
        .filter_map(|s| {
            Some(Reference {
                doc_type: s.doc_type,
                number: s.number.clone()?,
                business_date: s.business_date.clone(),
            })
        })
        .collect();
    if let Some(parent_id) = &doc.parent_id {
        references.extend(reference(&documents::get(conn, parent_id)?));
    }
    let paid_by = match sources
        .iter()
        .find(|s| s.link.kind == LinkKind::InvoicedTicket)
    {
        Some(ticket) => paid_by_ticket(conn, &ticket.link.from_id)?,
        None => None,
    };
    let mut alt_names = BTreeMap::new();
    if let Some(second) = second_lang {
        for line in &doc.lines {
            let name = if Some(second) == doc.second_lang {
                line.description_alt.clone()
            } else {
                match &line.product_id {
                    Some(pid) => Some(
                        crate::catalog::products::get(conn, pid)?
                            .name_in(second)
                            .to_owned(),
                    ),
                    None => None,
                }
            };
            if let Some(n) = name.filter(|n| !n.trim().is_empty() && *n != line.description) {
                alt_names.insert(line.id.clone(), n);
            }
        }
    }
    let mut unit_names = BTreeMap::new();
    let mut stmt = conn.prepare_cached("SELECT code, name_ar, name_fr, name_en FROM units")?;
    let units = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    })?;
    for unit in units {
        let (code, ar, fr, en) = unit?;
        unit_names.insert(code, in_lang(lang, &ar, &fr, &en));
    }
    let transfer = if doc.doc_type == DocType::Transfer {
        transfer_info(conn, &doc.id, lang)?
    } else {
        None
    };
    Ok(PrintContext {
        state: state_of(conn, &doc, today)?,
        document: doc,
        lang,
        second_lang,
        seller,
        counterpart,
        counterpart_is_supplier,
        settings,
        references,
        paid_by,
        alt_names,
        unit_names,
        transfer,
        has_logo: company.has_logo,
        today: today.to_owned(),
        printed_at: now,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing::convert::{DeliveriesInput, invoice_deliveries};
    use crate::billing::drafts::tests::{cart, fixture, input, save};
    use crate::billing::validate::{ValidateInput, validate};
    use crate::company::CompanyInput;
    use crate::money::{Money, Qty};
    use crate::payments::{PaymentInput, PaymentMethod};
    use crate::sales::cart::CartLine;
    use crate::testkit::{NOON, shop};

    #[test]
    fn the_context_keeps_the_snapshot_and_names_what_the_invoice_cites() {
        let shop = shop();
        let (amine, cement, _) = fixture(&shop);
        shop.stock_in(&cement.id, 100_000);
        let draft = save(
            &shop,
            None,
            input(
                DocType::DeliveryNote,
                cart(
                    Some(&amine),
                    vec![CartLine::product(&cement.id, Qty(2_000))],
                ),
            ),
        )
        .unwrap();
        let ctx = shop.owner_till();
        let bl = shop
            .db
            .write(move |tx| {
                validate(
                    tx,
                    &ctx,
                    ValidateInput {
                        id: draft.id,
                        payments: Vec::new(),
                    },
                )
            })
            .unwrap()
            .document;
        let (ctx, ids) = (shop.owner_till(), vec![bl.id.clone()]);
        let invoice = shop
            .db
            .write(move |tx| {
                invoice_deliveries(
                    tx,
                    &ctx,
                    DeliveriesInput {
                        delivery_ids: ids,
                        payments: vec![PaymentInput::electronic(
                            PaymentMethod::Transfer,
                            Money(238_000),
                            Some("VIR-1"),
                        )],
                        due_date: None,
                        second_lang: None,
                        note: None,
                    },
                )
            })
            .unwrap()
            .document;
        // The shop moves after the invoice.
        let (actor, pack) = (shop.owner.actor(), shop.pack.clone());
        shop.db
            .write(move |tx| {
                let mut c = CompanyInput::from(
                    crate::company::get(tx)?.ok_or_else(|| AppError::conflict("setup_required"))?,
                );
                c.address = Some("Nouvelle adresse, Oran".into());
                crate::company::save(tx, &actor, &pack, c).map(drop)
            })
            .unwrap();
        let id = invoice.id.clone();
        let pc = shop
            .db
            .read(move |c| print_context(c, &id, &PrintOptions::default(), "2026-09-28", NOON))
            .unwrap();
        assert_eq!(
            pc.seller.address.as_deref(),
            Some("12 rue Larbi Ben M'hidi, Alger"),
            "as on the day of the invoice"
        );
        assert_eq!(
            pc.counterpart.as_ref().and_then(|b| b.nif.as_deref()),
            Some("000016001234567")
        );
        assert_eq!(
            pc.references
                .iter()
                .map(|r| (r.doc_type, r.number.as_str()))
                .collect::<Vec<_>>(),
            [(DocType::DeliveryNote, "BL2026-000001")]
        );
        assert_eq!(pc.unit_names.get("pc").map(String::as_str), Some("Pièce"));
        let id = invoice.id.clone();
        let en = shop
            .db
            .read(move |c| {
                print_context(
                    c,
                    &id,
                    &PrintOptions {
                        lang: Some(Lang::Ar),
                        second_lang: Some(Lang::Fr),
                        monolingual: false,
                    },
                    "2026-09-28",
                    NOON,
                )
            })
            .unwrap();
        assert_eq!((en.lang, en.second_lang), (Lang::Ar, Some(Lang::Fr)));
        assert_eq!(en.seller.name, "مغازة الأمل");
        assert_eq!(en.seller.name_alt.as_deref(), Some("Supérette El Amel"));
        let id = invoice.id.clone();
        let one = shop
            .db
            .read(move |c| {
                print_context(
                    c,
                    &id,
                    &PrintOptions {
                        monolingual: true,
                        ..PrintOptions::default()
                    },
                    "2026-09-28",
                    NOON,
                )
            })
            .unwrap();
        assert_eq!(one.second_lang, None);
    }

    fn rename(shop: &crate::testkit::Shop, name_fr: &'static str, name_ar: &'static str) {
        let (actor, pack) = (shop.owner.actor(), shop.pack.clone());
        shop.db
            .write(move |tx| {
                let mut c = CompanyInput::from(
                    crate::company::get(tx)?.ok_or_else(|| AppError::conflict("setup_required"))?,
                );
                c.name_fr = name_fr.into();
                c.name_ar = name_ar.into();
                crate::company::save(tx, &actor, &pack, c).map(drop)
            })
            .unwrap();
    }

    #[test]
    fn a_reprint_never_names_the_seller_with_what_the_snapshot_did_not_hold() {
        let shop = shop();
        let (amine, cement, _) = fixture(&shop);
        shop.stock_in(&cement.id, 100_000);
        // A French-only shop: the Arabic name and activity fall back to the French ones.
        rename(&shop, "Supérette El Amel", "");
        shop.set(
            "company.activity",
            serde_json::json!({ "ar": "", "fr": "Commerce de détail", "en": "" }),
        );
        let mut bilingual = input(
            DocType::Invoice,
            cart(
                Some(&amine),
                vec![CartLine::product(&cement.id, Qty(1_000))],
            ),
        );
        bilingual.second_lang = Some(Lang::Ar);
        let draft = save(&shop, None, bilingual).unwrap();
        let ctx = shop.owner_till();
        let invoice = shop
            .db
            .write(move |tx| {
                validate(
                    tx,
                    &ctx,
                    ValidateInput {
                        id: draft.id,
                        payments: vec![PaymentInput::electronic(
                            PaymentMethod::Transfer,
                            Money(119_000),
                            Some("VIR-2"),
                        )],
                    },
                )
            })
            .unwrap()
            .document;
        let seller = invoice
            .parties
            .iter()
            .find(|p| p.role == PartyRole::Seller)
            .unwrap();
        assert_eq!(
            (seller.name_alt.as_deref(), seller.activity_alt.as_deref()),
            (None, None),
            "the same in both languages: stored once"
        );
        // A quote made while the shop had no activity at all.
        shop.set(
            "company.activity",
            serde_json::json!({ "ar": "", "fr": "", "en": "" }),
        );
        let draft = save(
            &shop,
            None,
            input(
                DocType::Quote,
                cart(
                    Some(&amine),
                    vec![CartLine::product(&cement.id, Qty(1_000))],
                ),
            ),
        )
        .unwrap();
        let ctx = shop.owner_till();
        let quote = shop
            .db
            .write(move |tx| {
                validate(
                    tx,
                    &ctx,
                    ValidateInput {
                        id: draft.id,
                        payments: Vec::new(),
                    },
                )
            })
            .unwrap()
            .document;
        // Later the shop is renamed, gets an Arabic name and an activity in every language.
        rename(&shop, "Supérette Nouvelle", "مغازة جديدة");
        shop.set(
            "company.activity",
            serde_json::json!({ "ar": "تجارة التجزئة", "fr": "Commerce de gros", "en": "Wholesale" }),
        );
        let context = |id: &str, options: PrintOptions| {
            let id = id.to_owned();
            shop.db
                .read(move |c| print_context(c, &id, &options, "2026-09-28", NOON))
                .unwrap()
        };

        let reprint = context(&invoice.id, PrintOptions::default());
        assert_eq!(
            (reprint.lang, reprint.second_lang),
            (Lang::Fr, Some(Lang::Ar))
        );
        assert_eq!(reprint.seller.name, "Supérette El Amel");
        assert_eq!(
            reprint.seller.name_alt, None,
            "no Arabic name that never was on the invoice"
        );
        assert_eq!(
            reprint.seller.activity.as_deref(),
            Some("Commerce de détail")
        );
        assert_eq!(reprint.seller.activity_alt, None);

        let arabic_first = context(
            &invoice.id,
            PrintOptions {
                lang: Some(Lang::Ar),
                second_lang: Some(Lang::Fr),
                monolingual: false,
            },
        );
        assert_eq!(
            (
                arabic_first.seller.name.as_str(),
                arabic_first.seller.name_alt.as_deref()
            ),
            ("Supérette El Amel", None),
            "its second language was stored as the same name"
        );

        let english = context(
            &invoice.id,
            PrintOptions {
                lang: Some(Lang::En),
                monolingual: true,
                ..PrintOptions::default()
            },
        );
        assert_eq!(
            (
                english.seller.name.as_str(),
                english.seller.activity.as_deref()
            ),
            ("Supérette Nouvelle", Some("Wholesale")),
            "a language the invoice never had: the shop's names today"
        );

        let quote = context(&quote.id, PrintOptions::default());
        assert_eq!(quote.seller.name, "Supérette El Amel");
        assert_eq!(
            quote.seller.activity, None,
            "no activity on the day, none on the reprint"
        );
    }
}

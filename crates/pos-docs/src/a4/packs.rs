//! A4 models of the pack documents printed through M3's print host: the instalment contract
//! (Task 10), the route sheet (Task 11), the zakat statement (Task 15). Every word comes from the
//! document catalogues (`a4.contract.*`, `a4.route.*`, `a4.zakat.*`).

use super::document::{DocumentPrintModel, PartyBlock, day};
use super::labels::{DocumentLabels, document_labels, word};
use pos_core::documents::{DocType, Document};
use pos_core::instalments::{GuaranteeKind, Plan};
use pos_core::lang::Lang;
use pos_core::money::{Money, Qty};
use pos_core::privacy::{Activity, Dpo};
use pos_core::wholesale::BlInvoiceAuthorisation;
use pos_core::wholesale::rounds::Round;
use pos_core::zakat::{Valuation, ZakatStatement, hijri_label};
use serde::Serialize;
use specta::Type;

/// The clauses of the contract, in print order (TMP-001). `clauseNoCheque` states that no
/// guarantee cheque is asked for or accepted (TMP-009).
pub const CONTRACT_CLAUSES: [&str; 7] = [
    "clauseGoods",
    "clausePrice",
    "clauseSchedule",
    "clausePlace",
    "clauseEarly",
    "clauseNoCheque",
    "clauseOwnership",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ContractItem {
    pub name: String,
    pub qty: Qty,
    pub total: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ContractLabels {
    pub items: String,
    pub total: String,
    pub down: String,
    pub financed: String,
    pub schedule: String,
    pub due: String,
    pub amount: String,
    pub guarantees: String,
}

/// The plain-language instalment sale contract (vente à tempérament, TMP-001).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ContractPrint {
    pub lang: Lang,
    pub rtl: bool,
    pub title: String,
    /// `CT2026-000001`.
    pub number: String,
    /// `dd/mm/yyyy`: the signature day.
    pub date: String,
    pub seller: PartyBlock,
    pub buyer: PartyBlock,
    /// The goods of the contract's ticket.
    pub items: Vec<ContractItem>,
    pub total: Money,
    pub down: Money,
    pub financed: Money,
    pub count: u8,
    /// (due date `dd/mm/yyyy`, amount) in schedule order.
    pub dues: Vec<(String, Money)>,
    /// The supporting documents given: "Fiche de paie — Farid Bouzid — Fiche de paie août 2026".
    pub guarantees: Vec<String>,
    pub clauses: Vec<String>,
    pub labels: ContractLabels,
    /// The words of the seller's and the buyer's cards (RC, NIF, address…).
    pub party_labels: DocumentLabels,
    /// (seller, buyer) signature captions.
    pub signatures: (String, String),
}

fn guarantee_word(lang: Lang, kind: GuaranteeKind) -> String {
    word(lang, &format!("a4.contract.guarantee.{}", kind.as_str()))
}

/// The contract of `plan`, sold on `doc`, between `seller` and `buyer` (built by the command
/// layer from M3's print context), in `lang`.
pub fn build_contract(
    plan: &Plan,
    doc: &Document,
    seller: PartyBlock,
    buyer: PartyBlock,
    lang: Lang,
) -> ContractPrint {
    let w = |k: &str| word(lang, &format!("a4.contract.{k}"));
    ContractPrint {
        lang,
        rtl: lang.is_rtl(),
        title: w("title"),
        number: plan.number.clone(),
        date: day(&plan.business_date),
        seller,
        buyer,
        items: doc
            .lines
            .iter()
            .filter(|l| l.parent_line_id.is_none())
            .map(|l| ContractItem {
                name: l.description.clone(),
                qty: l.qty,
                total: l.total_ttc,
            })
            .collect(),
        total: plan.total,
        down: plan.down_payment,
        financed: plan.financed,
        count: plan.count,
        dues: plan
            .dues
            .iter()
            .map(|d| (day(&d.due_on), d.amount))
            .collect(),
        guarantees: plan
            .guarantees
            .iter()
            .map(|g| {
                std::iter::once(guarantee_word(lang, g.kind))
                    .chain(g.holder_name.clone())
                    .chain(g.reference.clone())
                    .collect::<Vec<_>>()
                    .join(" — ")
            })
            .collect(),
        clauses: CONTRACT_CLAUSES.iter().map(|k| w(k)).collect(),
        labels: ContractLabels {
            items: w("items"),
            total: w("total"),
            down: w("down"),
            financed: w("financed"),
            schedule: w("schedule"),
            due: w("due"),
            amount: w("amount"),
            guarantees: w("guarantees"),
        },
        party_labels: document_labels(lang),
        signatures: (w("signSeller"), w("signBuyer")),
    }
}

/// One stop of the route sheet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RouteStopPrint {
    pub seq: u32,
    pub customer: String,
    pub address: Option<String>,
    pub phone: Option<String>,
    /// The number of the delivery note or invoice.
    pub document: String,
    pub amount_due: Money,
    pub collected: Money,
    /// The stop's state in the sheet's language.
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RouteLabels {
    pub driver: String,
    pub plate: String,
    pub van: String,
    pub customer: String,
    pub address: String,
    pub phone: String,
    pub document: String,
    pub amount_due: String,
    pub collected: String,
    pub status: String,
    pub signature: String,
}

/// The bon de route of a delivery round (VRT-100, DOC-030): the driver, the vehicle and every
/// stop with what to collect, and a signature box per stop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RouteSheetPrint {
    pub lang: Lang,
    pub rtl: bool,
    pub title: String,
    /// `TR2026-000001`.
    pub number: String,
    /// `dd/mm/yyyy`: the round's business day.
    pub date: String,
    pub seller: PartyBlock,
    pub driver: String,
    pub plate: Option<String>,
    /// The van's stock location, when the round has one.
    pub van: Option<String>,
    pub stops: Vec<RouteStopPrint>,
    pub total_due: Money,
    pub total_collected: Money,
    pub labels: RouteLabels,
    /// The seller's card, the number, the date and the page numbers.
    pub party_labels: DocumentLabels,
    /// (driver, shop) signature captions.
    pub signatures: (String, String),
}

/// The route sheet of `round` for `seller` (built by the command layer from M3's print
/// context), in `lang`.
pub fn build_route_sheet(round: &Round, seller: PartyBlock, lang: Lang) -> RouteSheetPrint {
    let w = |k: &str| word(lang, &format!("a4.route.{k}"));
    RouteSheetPrint {
        lang,
        rtl: lang.is_rtl(),
        title: w("title"),
        number: round.number.clone(),
        date: day(&round.business_date),
        seller,
        driver: round.driver_name.clone(),
        plate: round.vehicle_plate.clone(),
        van: round.van_location_name.clone(),
        stops: round
            .stops
            .iter()
            .map(|s| RouteStopPrint {
                seq: s.seq,
                customer: s.customer_name.clone().unwrap_or_default(),
                address: s.address.clone(),
                phone: s.phone.clone(),
                document: s.document_number.clone().unwrap_or_default(),
                amount_due: s.amount_due,
                collected: s.collected,
                status: w(&format!("stop.{}", s.status.as_str())),
            })
            .collect(),
        total_due: round.total_due,
        total_collected: round.total_collected,
        labels: RouteLabels {
            driver: w("driver"),
            plate: w("plate"),
            van: w("van"),
            customer: w("customer"),
            address: w("address"),
            phone: w("phone"),
            document: w("document"),
            amount_due: w("amountDue"),
            collected: w("collected"),
            status: w("status"),
            signature: w("signature"),
        },
        party_labels: document_labels(lang),
        signatures: (w("signDriver"), w("signShop")),
    }
}

/// The mention a delivery note carries while the shop's "BL valant facture" authorisation is set
/// (CFG-003); the command layer adds it to M3's `DocumentPrintModel.legal`.
pub fn bl_invoice_line(auth: &BlInvoiceAuthorisation, lang: Lang) -> String {
    word(lang, "a4.route.blAsInvoice")
        .replace("{number}", &auth.number)
        .replace("{date}", &auth.date)
}

/// A delivery note while the shop's "BL valant facture" authorisation is set (CFG-003): the
/// authorisation's line takes the place of "Ce bon de livraison n'est pas une facture" (a note that
/// stands as an invoice never says it is not one), in both languages of a bilingual note and at
/// the same position, so the two languages stay paired line by line.
pub fn stand_as_invoice(model: &mut DocumentPrintModel, auth: &BlInvoiceAuthorisation) {
    if model.doc_type != DocType::DeliveryNote {
        return;
    }
    let swap = |legal: &mut Vec<String>, lang: Lang| {
        let not_invoice = word(lang, "a4.legal.deliveryNotInvoice");
        match legal.iter_mut().find(|l| **l == not_invoice) {
            Some(line) => *line = bl_invoice_line(auth, lang),
            None => legal.push(bl_invoice_line(auth, lang)),
        }
    };
    swap(&mut model.legal, model.lang);
    if let Some(second) = &mut model.second {
        swap(&mut second.legal, second.lang);
    }
}

/// The words of the zakat statement's result rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ZakatLabels {
    pub base: String,
    pub nisab: String,
    pub rate: String,
    pub due: String,
}

/// The indicative zakat statement (FIN-007): the seven figures of the base, the base against
/// the nisab, the rate and the zakat due, with the "not a fatwa" disclaimer and the nisab's
/// source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ZakatStatementPrint {
    pub lang: Lang,
    pub rtl: bool,
    pub title: String,
    /// `ZK2026-000001`.
    pub number: String,
    /// `dd/mm/yyyy`: the statement's day.
    pub date: String,
    /// The Umm al-Qura date in the statement's language.
    pub hijri: String,
    pub seller: PartyBlock,
    /// (label, amount): stock with its valuation, cash in the drawers, bank, other assets,
    /// receivables, supplier debts, other debts.
    pub lines: Vec<(String, Money)>,
    pub base: Money,
    pub nisab: Money,
    /// "2,5 %" in the statement's language.
    pub rate: String,
    pub due: Money,
    /// The "no zakat is due" sentence when the base is below the nisab.
    pub below_nisab: Option<String>,
    pub disclaimer: String,
    /// "Nissab : <the data pack's source>".
    pub source: String,
    pub labels: ZakatLabels,
    /// The words of the seller's card (RC, NIF, address…).
    pub party_labels: DocumentLabels,
}

/// Basis points as a short percentage: `250` → "2,5 %" ("2.5 %" in English), `200` → "2 %".
fn short_percent(bp: i32, lang: Lang) -> String {
    let whole = bp / 100;
    let fraction = format!("{:02}", (bp % 100).abs());
    let fraction = fraction.trim_end_matches('0');
    if fraction.is_empty() {
        return format!("{whole} %");
    }
    let separator = if lang == Lang::En { '.' } else { ',' };
    format!("{whole}{separator}{fraction} %")
}

/// The print of a saved zakat `statement` for `seller` (built by the command layer from M3's
/// print context), with the data pack's `nisab_source`, in `lang`.
pub fn build_zakat(
    statement: &ZakatStatement,
    seller: PartyBlock,
    nisab_source: &str,
    lang: Lang,
) -> ZakatStatementPrint {
    let w = |k: &str| word(lang, &format!("a4.zakat.{k}"));
    let p = &statement.preview;
    let stock = match p.valuation {
        Valuation::Cost => w("stockCost"),
        Valuation::SalePrice => w("stockSale"),
    };
    ZakatStatementPrint {
        lang,
        rtl: lang.is_rtl(),
        title: w("title"),
        number: statement.number.clone(),
        date: day(&p.day),
        hijri: hijri_label(p.hijri_date, lang),
        seller,
        lines: vec![
            (stock, p.stock_value),
            (w("cash"), p.cash),
            (w("bank"), p.bank),
            (w("otherAssets"), p.other_assets),
            (w("receivables"), p.receivables),
            (w("payables"), p.payables),
            (w("otherDebts"), p.other_debts),
        ],
        base: p.base,
        nisab: p.nisab,
        rate: short_percent(p.rate_bp, lang),
        due: p.zakat_due,
        below_nisab: p.below_nisab.then(|| w("belowNisab")),
        disclaimer: w("disclaimer"),
        source: w("source").replace("{source}", nisab_source),
        labels: ZakatLabels {
            base: w("base"),
            nisab: w("nisab"),
            rate: w("rate"),
            due: w("due"),
        },
        party_labels: document_labels(lang),
    }
}

/// The words of the processing register's table and blocks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProcessingRegisterLabels {
    pub activity: String,
    pub basis: String,
    pub data: String,
    pub retention: String,
    pub security: String,
    pub dpo: String,
    /// What the DPO line says when none is named.
    pub no_dpo: String,
}

/// The record of processing activities (PDP-005, loi 18-07): the shop, its DPO, one row per
/// activity of the modules in use and the security measures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProcessingRegisterPrint {
    pub lang: Lang,
    pub rtl: bool,
    pub title: String,
    pub seller: PartyBlock,
    /// "Nadia Ferhat — dpo@exemple.dz" (the phone without an e-mail); `None` when none is named.
    pub dpo: Option<String>,
    /// (name, legal basis, data, retention) per activity.
    pub activities: Vec<(String, String, String, String)>,
    pub security: String,
    /// `dd/mm/yyyy`: the day it was printed.
    pub date: String,
    pub labels: ProcessingRegisterLabels,
    /// The seller card's words and the page numbers (Task 21b: the page's `PartyCard`).
    pub party_labels: DocumentLabels,
}

/// The processing register of `activities` (`privacy::activities`) for `seller` with the
/// shop's `dpo`, in `lang`, dated `today` (`YYYY-MM-DD`).
pub fn build_processing_register(
    activities: &[Activity],
    seller: PartyBlock,
    dpo: Option<Dpo>,
    lang: Lang,
    today: &str,
) -> ProcessingRegisterPrint {
    let w = |k: &str| word(lang, &format!("a4.privacy.{k}"));
    ProcessingRegisterPrint {
        lang,
        rtl: lang.is_rtl(),
        title: w("title"),
        seller,
        dpo: dpo.map(|d| match d.email.or(d.phone) {
            Some(contact) => format!("{} — {contact}", d.name),
            None => d.name,
        }),
        activities: activities
            .iter()
            .map(|a| {
                (
                    w(&format!("activity.{}.name", a.key)),
                    word(lang, &a.legal_basis),
                    word(lang, &a.data),
                    word(lang, &a.retention),
                )
            })
            .collect(),
        security: w("security"),
        date: day(today),
        labels: ProcessingRegisterLabels {
            activity: w("labels.activity"),
            basis: w("labels.basis"),
            data: w("labels.data"),
            retention: w("labels.retention"),
            security: w("labels.security"),
            dpo: w("dpo"),
            no_dpo: w("noDpo"),
        },
        party_labels: document_labels(lang),
    }
}

#[cfg(test)]
mod fixtures {
    use super::super::document::{PartyBlock, party_block};
    use super::super::testdata;
    use chrono::NaiveDate;
    use pos_core::documents::{DocType, Document};
    use pos_core::instalments::{DueState, DueStatus, Guarantee, GuaranteeKind, Plan, schedule};
    use pos_core::lang::Lang;
    use pos_core::money::{Money, Qty};
    use pos_core::payments::PaymentMethod;
    use pos_core::wholesale::rounds::{Round, RoundStatus, Stop, StopStatus};
    use pos_core::zakat::{HijriDate, Valuation, ZakatPreview, ZakatStatement};

    /// The 120 000 DA fridge of Farid Bouzid: 30 000 DA down, 90 000 DA in six dues of 15 000 DA.
    pub(super) fn plan() -> Plan {
        let first = NaiveDate::from_ymd_opt(2026, 10, 31).unwrap();
        let dues = schedule(Money(9_000_000), 6, first)
            .unwrap()
            .into_iter()
            .map(|d| DueState {
                seq: d.seq,
                due_on: d.due_on,
                amount: d.amount,
                paid: Money::ZERO,
                status: DueStatus::Upcoming,
                days_late: 0,
            })
            .collect();
        Plan {
            id: "00000000-0000-7000-8000-000000000010".into(),
            number: "CT2026-000001".into(),
            customer_id: "c-farid".into(),
            customer_name: "Farid Bouzid".into(),
            document_id: "tk-1".into(),
            document_number: "TK2026-C1-000001".into(),
            credit_entry_id: "00000000-0000-7000-8000-000000000011".into(),
            total: Money(12_000_000),
            down_payment: Money(3_000_000),
            financed: Money(9_000_000),
            count: 6,
            first_due_on: "2026-10-31".into(),
            dues,
            paid: Money::ZERO,
            remaining: Money(9_000_000),
            guarantees: vec![Guarantee {
                id: "00000000-0000-7000-8000-000000000012".into(),
                kind: GuaranteeKind::PaySlip,
                holder_name: Some("Farid Bouzid".into()),
                reference: Some("Fiche de paie août 2026".into()),
                phone: None,
                note: None,
                created_at: testdata::PRINTED_AT,
            }],
            business_date: "2026-09-28".into(),
            created_at: testdata::PRINTED_AT,
        }
    }

    /// The contract's ticket: one fridge, 120 000 DA TTC.
    pub(super) fn document() -> Document {
        let mut d = testdata::invoice_document();
        d.doc_type = DocType::Ticket;
        d.number = Some("TK2026-C1-000001".into());
        let mut fridge = d.lines[0].clone();
        fridge.description = "Réfrigérateur Condor 400 L".into();
        fridge.qty = Qty(1_000);
        fridge.unit_price = Money(12_000_000);
        fridge.discount_amount = Money::ZERO;
        fridge.total_ht = Money(10_084_034);
        fridge.tax_amount = Money(1_915_966);
        fridge.total_ttc = Money(12_000_000);
        d.lines = vec![fridge];
        d
    }

    /// Rachid's round: an invoice of 5 000 DA collected in cash, a delivery note of 10 000 DA
    /// still to deliver.
    pub(super) fn round() -> Round {
        let stop = |seq: u32, number: &str, customer: &str, due: i64| Stop {
            seq,
            document_id: format!("doc-{seq}"),
            document_number: Some(number.into()),
            doc_type: DocType::DeliveryNote,
            customer_id: Some(format!("c-{seq}")),
            customer_name: Some(customer.into()),
            address: Some("Cité 200 logements, Bab Ezzouar".into()),
            phone: Some("0555123456".into()),
            amount_due: Money(due),
            status: StopStatus::Pending,
            collected: Money::ZERO,
            method: None,
            reference: None,
            cheque_bank: None,
            cheque_date: None,
            receipt_number: None,
            note: None,
        };
        let mut first = stop(1, "FA2026-000004", "Épicerie Amine", 500_000);
        first.doc_type = DocType::Invoice;
        first.status = StopStatus::Delivered;
        first.collected = Money(500_000);
        first.method = Some(PaymentMethod::Cash);
        Round {
            id: "00000000-0000-7000-8000-000000000020".into(),
            number: "TR2026-000001".into(),
            business_date: "2026-09-28".into(),
            driver_name: "Rachid".into(),
            driver_user_id: None,
            vehicle_plate: Some("01234-116-16".into()),
            van_location_id: None,
            van_location_name: None,
            status: RoundStatus::Out,
            note: None,
            started_at: Some(testdata::PRINTED_AT),
            closed_at: None,
            stops: vec![first, stop(2, "BL2026-000007", "Supérette Nour", 1_000_000)],
            total_due: Money(1_500_000),
            total_collected: Money(500_000),
            cash_collected: Money(500_000),
        }
    }

    /// The statement of 2026-09-28 (17 Rabi' al-Thani 1448): a base of 2 451 000 DA above the
    /// nisab, 61 275 DA due.
    pub(super) fn zakat() -> ZakatStatement {
        ZakatStatement {
            id: "00000000-0000-7000-8000-000000000030".into(),
            number: "ZK2026-000001".into(),
            preview: ZakatPreview {
                day: "2026-09-28".into(),
                hijri_date: HijriDate {
                    year: 1448,
                    month: 4,
                    day: 17,
                },
                hijri_label: "17 Rabi' al-Thani 1448".into(),
                valuation: Valuation::SalePrice,
                stock_value: Money(149_700_000),
                cash: Money(100_000),
                bank: Money(100_000_000),
                other_assets: Money::ZERO,
                receivables: Money(300_000),
                payables: Money::ZERO,
                other_debts: Money(5_000_000),
                base: Money(245_100_000),
                nisab: Money(229_500_000),
                rate_bp: 250,
                zakat_due: Money(6_127_500),
                below_nisab: false,
            },
            note: None,
            created_at: testdata::PRINTED_AT,
            user_name: "Amine".into(),
        }
    }

    /// What `privacy::activities` returns for a pharmacy with loyalty.
    pub(super) fn activities() -> Vec<pos_core::privacy::Activity> {
        ["customers", "loyalty", "pharmacy_register"]
            .into_iter()
            .map(pos_core::privacy::Activity::of)
            .collect()
    }

    pub(super) fn dpo() -> pos_core::privacy::Dpo {
        pos_core::privacy::Dpo {
            name: "Nadia Ferhat".into(),
            email: Some("dpo@exemple.dz".into()),
            phone: Some("0550 12 34 56".into()),
        }
    }

    pub(super) fn seller() -> PartyBlock {
        party_block(
            &testdata::seller(),
            &pos_core::datapack::bundled().unwrap(),
            Lang::Fr,
        )
    }

    pub(super) fn buyer() -> PartyBlock {
        let mut farid = testdata::buyer();
        farid.name = "Farid Bouzid".into();
        farid.legal_form = None;
        party_block(&farid, &pos_core::datapack::bundled().unwrap(), Lang::Fr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../src/documents/__fixtures__")
    }

    /// The contract and the route sheet the React pages render in their tests (`fixtures-m8.ts`),
    /// in the three languages.
    fn pack_fixtures() -> Vec<(String, String)> {
        let mut out = Vec::new();
        for lang in [Lang::Ar, Lang::Fr, Lang::En] {
            let contract = build_contract(
                &fixtures::plan(),
                &fixtures::document(),
                fixtures::seller(),
                fixtures::buyer(),
                lang,
            );
            let sheet = build_route_sheet(&fixtures::round(), fixtures::seller(), lang);
            out.push((format!("contract.{}", lang.as_str()), pretty(&contract)));
            out.push((format!("route-sheet.{}", lang.as_str()), pretty(&sheet)));
            // Task 21b: the processing register page (PDP-005) renders the Rust model too
            let register = build_processing_register(
                &fixtures::activities(),
                fixtures::seller(),
                Some(fixtures::dpo()),
                lang,
                "2026-09-28",
            );
            out.push((
                format!("processing-register.{}", lang.as_str()),
                pretty(&register),
            ));
        }
        out
    }

    fn pretty<T: Serialize>(model: &T) -> String {
        let mut text = serde_json::to_string_pretty(model).unwrap();
        text.push('\n');
        text
    }

    #[test]
    #[ignore = "writes src/documents/__fixtures__/{contract,route-sheet}.*.json"]
    fn write_pack_fixtures() {
        std::fs::create_dir_all(fixture_dir()).unwrap();
        for (name, text) in pack_fixtures() {
            std::fs::write(fixture_dir().join(format!("{name}.json")), text).unwrap();
        }
    }

    #[test]
    fn pack_fixtures_are_up_to_date() {
        for (name, text) in pack_fixtures() {
            let stored = std::fs::read_to_string(fixture_dir().join(format!("{name}.json")))
                .unwrap_or_default()
                .replace("\r\n", "\n");
            assert_eq!(
                stored, text,
                "{name}.json is stale: cargo test -p pos-docs write_pack_fixtures -- --ignored"
            );
        }
    }

    #[test]
    fn a_delivery_note_standing_as_an_invoice_says_so_instead_of_not_being_one() {
        let auth = pos_core::wholesale::BlInvoiceAuthorisation {
            number: "117/2026".into(),
            date: "2026-01-15".into(),
        };
        let pack = pos_core::datapack::bundled().unwrap();
        let mut bl = super::super::document::build_document(
            &super::super::testdata::delivery(),
            &pack,
            None,
        )
        .unwrap();
        let not_invoice = word(Lang::Fr, "a4.legal.deliveryNotInvoice");
        let at = bl.legal.iter().position(|l| *l == not_invoice).unwrap();
        stand_as_invoice(&mut bl, &auth);
        assert_eq!(bl.legal[at], bl_invoice_line(&auth, Lang::Fr));
        assert!(!bl.legal.contains(&not_invoice));
        let mut invoice =
            super::super::document::build_document(&super::super::testdata::invoice(), &pack, None)
                .unwrap();
        let before = invoice.legal.clone();
        stand_as_invoice(&mut invoice, &auth);
        assert_eq!(invoice.legal, before, "an invoice is left as it is");
    }

    #[test]
    fn the_contract_states_the_schedule_and_refuses_guarantee_cheques_in_every_language() {
        for lang in [Lang::Ar, Lang::Fr, Lang::En] {
            let c = build_contract(
                &fixtures::plan(),
                &fixtures::document(),
                fixtures::seller(),
                fixtures::buyer(),
                lang,
            );
            assert_eq!(c.dues.len(), 6);
            let no_cheque =
                crate::locales::text(lang.as_str(), "a4.contract.clauseNoCheque").unwrap();
            assert!(c.clauses.contains(&no_cheque));
        }
        let fr = build_contract(
            &fixtures::plan(),
            &fixtures::document(),
            fixtures::seller(),
            fixtures::buyer(),
            Lang::Fr,
        );
        assert_eq!(fr.title, "Contrat de vente à tempérament");
    }

    #[test]
    fn the_route_sheet_lists_every_stop_with_its_amount_and_the_bl_line_names_the_decision() {
        let sheet = build_route_sheet(&fixtures::round(), fixtures::seller(), Lang::Fr);
        assert_eq!(
            (sheet.title.as_str(), sheet.stops.len(), sheet.total_due),
            ("Bon de route", 2, Money(1_500_000))
        );
        assert_eq!(sheet.driver, "Rachid");
        let auth = pos_core::wholesale::BlInvoiceAuthorisation {
            number: "117/2026".into(),
            date: "2026-01-15".into(),
        };
        assert_eq!(
            bl_invoice_line(&auth, Lang::Fr),
            "BL valant facture — autorisation n° 117/2026 du 2026-01-15"
        );
        assert!(bl_invoice_line(&auth, Lang::Ar).contains("117/2026"));
    }

    #[test]
    fn the_processing_register_lists_the_shops_activities_with_the_dpo() {
        let print = build_processing_register(
            &fixtures::activities(),
            fixtures::seller(),
            Some(fixtures::dpo()),
            Lang::Fr,
            "2026-09-28",
        );
        assert_eq!(print.title, "Registre des activités de traitement");
        assert!(
            print
                .activities
                .iter()
                .any(|a| a.0 == "Registre des médicaments contrôlés")
        );
        assert_eq!(print.dpo.as_deref(), Some("Nadia Ferhat — dpo@exemple.dz"));
        assert_eq!(print.date, "28/09/2026");
        for lang in [Lang::Ar, Lang::Fr, Lang::En] {
            let print = build_processing_register(
                &fixtures::activities(),
                fixtures::seller(),
                None,
                lang,
                "2026-09-28",
            );
            assert_eq!(print.activities.len(), 3);
            for (name, basis, data, retention) in &print.activities {
                for word in [name, basis, data, retention] {
                    assert!(!word.is_empty() && !word.starts_with("a4."), "{word}");
                }
            }
            assert_eq!(print.dpo, None);
            assert_eq!(
                print.security,
                crate::locales::text(lang.as_str(), "a4.privacy.security").unwrap()
            );
        }
    }

    #[test]
    fn the_zakat_statement_says_it_is_not_a_fatwa_in_every_language() {
        for lang in [Lang::Ar, Lang::Fr, Lang::En] {
            let print = build_zakat(
                &fixtures::zakat(),
                fixtures::seller(),
                "Ministère des Affaires religieuses et des Wakfs",
                lang,
            );
            assert_eq!(print.lines.len(), 7);
            assert_eq!(
                print.disclaimer,
                crate::locales::text(lang.as_str(), "a4.zakat.disclaimer").unwrap()
            );
        }
        let fr = build_zakat(&fixtures::zakat(), fixtures::seller(), "MARW", Lang::Fr);
        assert_eq!(
            (fr.title.as_str(), fr.hijri.as_str()),
            ("Calcul de la zakat", "17 Rabi' al-Thani 1448")
        );
        assert_eq!((fr.rate.as_str(), fr.below_nisab), ("2,5 %", None));
        assert_eq!(fr.source, "Nissab : MARW");
        assert_eq!(
            fr.party_labels,
            super::document_labels(Lang::Fr),
            "the seller's card is printed with the document words (RC, NIF…)"
        );
    }
}

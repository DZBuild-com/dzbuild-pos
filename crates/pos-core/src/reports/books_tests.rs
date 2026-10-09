//! The books agree on the droit de timbre and the kridi (M2 review, carried to M3): the X and Z
//! of a session and the sales summary count the duty of the invoices and of the versement
//! receipts (RV), not the tickets' only, and never twice for an invoice made from a ticket; the
//! P&L counts the kridi adjustments of the period (a debt written off is a loss).

use super::{Report, ReportKind, ReportRequest, run};
use crate::billing::convert::{TicketInvoiceInput, invoice_ticket};
use crate::billing::drafts::tests::{cart, fixture, input, save};
use crate::billing::validate::{ValidateInput, validate};
use crate::cash::closing::x_report;
use crate::customers::credit::{CollectInput, adjust, collect, set_opening};
use crate::imports::engine::{ImportOptions, apply, dry_run, rows_of, undo};
use crate::imports::spec::{ImportEntity, ImportPolicy};
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::numbering::DocType;
use crate::payments::{PaymentInput, PaymentMethod};
use crate::sales::cart::{Cart, CartLine};
use crate::sales::sale::{SaleInput, create_sale};
use crate::testkit::{Shop, shop};
use serde_json::json;
use std::collections::BTreeMap;

const DAY: &str = "2026-09-28";

fn report(shop: &Shop, req: ReportRequest) -> Report {
    shop.db.read(move |c| run(c, &req)).unwrap()
}

fn total(report: &Report, key: &str) -> i64 {
    let i = report.table.column(key).unwrap();
    report.table.totals.as_ref().unwrap()[i].as_i64().unwrap()
}

/// The amount of the P&L line `code`.
fn line_amount(pnl: &Report, code: &str) -> i64 {
    pnl.table
        .rows
        .iter()
        .find(|r| r[0].as_str() == Some(code))
        .and_then(|r| r[2].as_i64())
        .unwrap_or_else(|| panic!("no {code} line"))
}

fn versement(customer_id: &str, amount: i64) -> CollectInput {
    CollectInput {
        customer_id: customer_id.to_owned(),
        amount: Money(amount),
        method: PaymentMethod::Cash,
        reference: None,
        cheque_bank: None,
        cheque_date: None,
        allocate_to: Vec::new(),
        note: None,
    }
}

/// One session, every kind of duty: a cash ticket of 2 380 DA (24 DA), then its invoice (the
/// ticket's duty again on paper, not in the books); an invoice of 11 900 DA paid in cash
/// (119 DA); a 1 000 DA cash versement whose duty the customer pays (10 DA) and another the
/// shop absorbs (10 DA). Returns the session id and the customer's id.
fn a_day_of_duties(shop: &Shop) -> (String, String) {
    let (amine, cement, _) = fixture(shop);
    shop.stock_in(&cement.id, 100_000);
    let session = shop.open_till(1_000_000);
    let (ctx, product) = (shop.till(), cement.id.clone());
    let ticket = shop
        .db
        .write(move |tx| {
            create_sale(
                tx,
                &ctx,
                SaleInput {
                    cart: Cart::new(Lang::Fr, vec![CartLine::product(&product, Qty(2_000))]),
                    payments: vec![PaymentInput::cash(Money(250_000))],
                },
            )
        })
        .unwrap()
        .document;
    assert_eq!(ticket.stamp_duty, Money(2_400));
    let (ctx, ticket_id, customer_id) = (shop.owner_till(), ticket.id, amine.id.clone());
    let invoiced = shop
        .db
        .write(move |tx| {
            invoice_ticket(
                tx,
                &ctx,
                TicketInvoiceInput {
                    ticket_id,
                    customer_id,
                    second_lang: None,
                    note: None,
                },
            )
        })
        .unwrap()
        .document;
    assert_eq!(
        invoiced.stamp_duty,
        Money(2_400),
        "the invoice of a ticket prints its duty"
    );
    let draft = save(
        shop,
        None,
        input(
            DocType::Invoice,
            cart(
                Some(&amine),
                vec![CartLine::product(&cement.id, Qty(10_000))],
            ),
        ),
    )
    .unwrap();
    let ctx = shop.owner_till();
    let invoice = shop
        .db
        .write(move |tx| {
            validate(
                tx,
                &ctx,
                ValidateInput {
                    id: draft.id,
                    payments: vec![PaymentInput::cash(Money(1_210_000))],
                },
            )
        })
        .unwrap()
        .document;
    assert_eq!(invoice.stamp_duty, Money(11_900));
    let (ctx, id) = (shop.owner_till(), amine.id.clone());
    shop.db
        .write(move |tx| set_opening(tx, &ctx, &id, Money(500_000), None))
        .unwrap();
    let (ctx, input) = (shop.till(), versement(&amine.id, 100_000));
    let charged = shop.db.write(move |tx| collect(tx, &ctx, input)).unwrap();
    assert_eq!(
        (charged.entry.stamp_duty, charged.entry.stamp_charged),
        (Money(1_000), Money(1_000))
    );
    shop.set("stamp_duty.mode", json!("absorb"));
    let (ctx, input) = (shop.till(), versement(&amine.id, 100_000));
    let absorbed = shop.db.write(move |tx| collect(tx, &ctx, input)).unwrap();
    assert_eq!(
        (absorbed.entry.stamp_duty, absorbed.entry.stamp_charged),
        (Money(1_000), Money::ZERO)
    );
    (session.id, amine.id)
}

#[test]
fn the_x_and_z_count_the_duty_of_invoices_and_versements() {
    let shop = shop();
    let (session, _) = a_day_of_duties(&shop);
    let x = shop.db.read(move |c| x_report(c, &session, true)).unwrap();
    assert_eq!(
        (x.stamp_charged, x.stamp_absorbed),
        (Money(2_400 + 11_900 + 1_000), Money(1_000)),
        "ticket + invoice + charged versement; the absorbed versement; the ticket's invoice adds nothing"
    );
}

#[test]
fn the_sales_summary_counts_the_duty_of_invoices_and_versements() {
    let shop = shop();
    let (session, amine) = a_day_of_duties(&shop);
    let x = shop.db.read(move |c| x_report(c, &session, true)).unwrap();
    let summary = report(
        &shop,
        ReportRequest::new(ReportKind::SalesSummary, DAY, DAY),
    );
    assert_eq!(
        total(&summary, "stampDuty"),
        x.stamp_charged.0,
        "the summary's duty is the duty the Z charged"
    );
    let mut req = ReportRequest::new(ReportKind::SalesSummary, DAY, DAY);
    req.customer_id = Some(amine);
    let theirs = report(&shop, req);
    assert_eq!(
        total(&theirs, "stampDuty"),
        11_900 + 1_000,
        "the customer's invoice and charged versement; the ticket had no customer"
    );
}

#[test]
fn the_pnl_counts_the_kridi_written_off() {
    let shop = shop();
    let karim = shop.customer("Karim");
    let (ctx, id) = (shop.owner_till(), karim.id.clone());
    shop.db
        .write(move |tx| set_opening(tx, &ctx, &id, Money(500_000), None))
        .unwrap();
    let (ctx, id) = (shop.owner_till(), karim.id.clone());
    shop.db
        .write(move |tx| adjust(tx, &ctx, &id, Money(-200_000), "Dette abandonnée"))
        .unwrap();
    let (ctx, id) = (shop.owner_till(), karim.id);
    shop.db
        .write(move |tx| adjust(tx, &ctx, &id, Money(50_000), "Erreur de report"))
        .unwrap();
    let pnl = report(
        &shop,
        ReportRequest::new(ReportKind::ProfitAndLoss, "2026-09-01", "2026-09-30"),
    );
    let amount = |code: &str| line_amount(&pnl, code);
    assert_eq!(
        amount("credit_adjustments"),
        -150_000,
        "2 000 DA written off, 500 DA added back; the opening balance is no income"
    );
    assert_eq!(amount("net_profit"), -150_000);
}

/// Undoing an import of customer balances writes off no debt: its counter-entry (an
/// `adjustment` referencing `opening_undo`) cancels an opening the P&L never counted.
#[test]
fn the_pnl_leaves_out_the_undo_of_an_imported_opening() {
    let shop = shop();
    let karim = shop.customer("Karim");
    let balances = ImportOptions {
        entity: ImportEntity::CustomerBalances,
        policy: ImportPolicy::CreateOnly,
        file_name: "soldes.xlsx".into(),
        header_row: 0,
        mapping: vec![Some("code".into()), Some("amount".into())],
        location_id: None,
        count_id: None,
    };
    let sheet = [
        vec!["Code".to_owned(), "Solde".to_owned()],
        vec![karim.code.clone(), "1 500,00".to_owned()],
    ];
    let rows = rows_of(&sheet, &balances);
    let choices = BTreeMap::new();
    let dry = dry_run(&shop.db, &shop.owner_till(), &balances, &rows, &choices).unwrap();
    let job = apply(
        &shop.db,
        &shop.owner_till(),
        &balances,
        &rows,
        &choices,
        &dry.fingerprint,
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(job.created, 1, "the opening is posted");
    let ctx = shop.owner_till();
    shop.db.write(move |tx| undo(tx, &ctx, &job.id)).unwrap();
    assert_eq!(
        shop.count(
            "SELECT COUNT(*) FROM customer_ledger
             WHERE kind = 'adjustment' AND ref_type = 'opening_undo' AND amount = -150000"
        ),
        1,
        "the undo is an adjustment counter-entry"
    );
    let pnl = report(
        &shop,
        ReportRequest::new(ReportKind::ProfitAndLoss, "2026-09-01", "2026-09-30"),
    );
    assert_eq!(
        (
            line_amount(&pnl, "credit_adjustments"),
            line_amount(&pnl, "net_profit")
        ),
        (0, 0),
        "an opening and its undo: no debt written off, no loss"
    );
}

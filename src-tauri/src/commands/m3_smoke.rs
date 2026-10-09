//! One month of a réel shop through the M3 commands: quote → invoice with cash and credit,
//! two delivery notes → one invoice on account, a ticket → its invoice, a goods credit note, a
//! full financial credit note, a versement, a second ticket. Everything must balance: the sales
//! summary counts each sale once, the G50 equals the VAT and stamp statements, the customer's
//! balance equals its credit parts less its credit notes and versement, every chain verifies,
//! the fiscal archive verifies, the first print is the original.

use super::{billing, fiscal, print};
use crate::commands::testshop::shop;
use crate::print_host::{
    HostChannel, HostJob, PageSetup, PrintHost, PrintStatus, PrintSurface, Rendered,
};
use pos_core::auth::Permission;
use pos_core::billing::convert::{ConvertInput, DeliveriesInput, TicketInvoiceInput};
use pos_core::billing::credit_notes::{CreditLineInput, CreditNoteInput, CreditReason};
use pos_core::billing::drafts::DraftInput;
use pos_core::billing::print::PrintOptions;
use pos_core::billing::state::DocState;
use pos_core::billing::validate::ValidateInput;
use pos_core::documents::{CreditKind, DocType, Document};
use pos_core::error::AppError;
use pos_core::lang::Lang;
use pos_core::money::{Money, Qty};
use pos_core::payments::{PaymentInput, PaymentMethod};
use pos_core::reports::{ReportKind, ReportRequest};
use pos_core::sales::{Cart, CartLine};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

struct Sink;

impl PrintSurface for Sink {
    fn render(&self, _: HostJob) -> Result<Rendered, AppError> {
        Ok(Rendered { height_px: 1_123 })
    }
    fn print(&self, _: &PageSetup, _: Option<&str>, _: u32) -> Result<PrintStatus, AppError> {
        Ok(PrintStatus::Printed)
    }
    fn pdf(&self, _: &PageSetup, path: &Path) -> Result<(), AppError> {
        std::fs::write(path, b"%PDF-1.7 smoke").map_err(|e| AppError::Io(e.to_string()))
    }
    fn close_if_idle(&self, _: Duration) {}
}

fn credit(amount: Money) -> PaymentInput {
    PaymentInput {
        method: PaymentMethod::Credit,
        ..PaymentInput::cash(amount)
    }
}

fn draft(kind: DocType, customer: &str, lines: Vec<CartLine>) -> DraftInput {
    let mut cart = Cart::new(Lang::Fr, lines);
    cart.customer_id = Some(customer.to_owned());
    DraftInput {
        doc_type: kind,
        cart,
        valid_until: None,
        due_date: None,
        second_lang: None,
        draft_id: None,
    }
}

#[test]
fn a_month_of_invoices_balances_with_the_reports() {
    let t = shop();
    t.reel_company();
    let amine = t.b2b_customer("SARL Amine");
    let cement = t.product("Ciment 25 kg", 119_000, 1_900, None);
    let sand = t.product("Sable 1 m3", 109_000, 900, None);
    t.stock_in(&cement.id, 100);
    t.stock_in(&sand.id, 100);
    t.owner();
    t.open_till(1_000_000);
    let host = PrintHost::new(Arc::new(Sink), Arc::new(HostChannel::new()));
    let mut sales: Vec<Document> = Vec::new();

    // 1. A ticket, then its invoice: the turnover is the ticket's, once.
    let ticket = t.sell(&cement.id, 2, 300_000).document;
    let ticket_invoice = billing::invoice_ticket(
        &t.st,
        TicketInvoiceInput {
            ticket_id: ticket.id.clone(),
            customer_id: amine.id.clone(),
            second_lang: None,
            note: None,
        },
    )
    .unwrap()
    .document;
    sales.push(ticket.clone());

    // 2. A quote converted to an invoice, paid 5 000.00 in cash and the rest on account.
    let quote = billing::save_draft(
        &t.st,
        None,
        draft(
            DocType::Quote,
            &amine.id,
            vec![
                CartLine::product(&cement.id, Qty(10_000)),
                CartLine::product(&sand.id, Qty(2_000)),
            ],
        ),
    )
    .unwrap();
    billing::validate(
        &t.st,
        ValidateInput {
            id: quote.id.clone(),
            payments: Vec::new(),
        },
    )
    .unwrap();
    let invoice_draft = billing::convert(
        &t.st,
        ConvertInput {
            source_id: quote.id.clone(),
            target: DocType::Invoice,
            reprice: false,
        },
    )
    .unwrap();
    let cash_part = PaymentInput::cash(Money(500_000));
    let preview =
        billing::payment_preview(&t.st, invoice_draft.id.clone(), vec![cash_part.clone()]).unwrap();
    let invoice = billing::validate(
        &t.st,
        ValidateInput {
            id: invoice_draft.id.clone(),
            payments: vec![cash_part, credit(preview.missing)],
        },
    )
    .unwrap()
    .document;
    assert!(
        invoice.stamp_duty.is_positive(),
        "the cash part pays the duty"
    );
    sales.push(invoice.clone());

    // 3. Two delivery notes, one invoice on account, no second stock movement.
    let mut deliveries = Vec::new();
    for _ in 0..2 {
        let bl = billing::save_draft(
            &t.st,
            None,
            draft(
                DocType::DeliveryNote,
                &amine.id,
                vec![CartLine::product(&sand.id, Qty(1_000))],
            ),
        )
        .unwrap();
        billing::validate(
            &t.st,
            ValidateInput {
                id: bl.id.clone(),
                payments: Vec::new(),
            },
        )
        .unwrap();
        deliveries.push(bl.id);
    }
    let sand_before = t.count("SELECT COUNT(*) FROM stock_movements");
    let bl_invoice = billing::invoice_deliveries(
        &t.st,
        DeliveriesInput {
            delivery_ids: deliveries.clone(),
            payments: vec![credit(Money(218_000))],
            due_date: None,
            second_lang: None,
            note: None,
        },
    )
    .unwrap()
    .document;
    assert_eq!(
        t.count("SELECT COUNT(*) FROM stock_movements"),
        sand_before,
        "invoicing BL moves no stock"
    );
    assert_eq!(
        bl_invoice.stamp_duty,
        Money::ZERO,
        "no duty on the credit part"
    );
    sales.push(bl_invoice.clone());

    // 4. A goods credit note of 3 bags back to the account.
    let av1 = billing::create_credit_note(
        &t.st,
        CreditNoteInput {
            invoice_id: invoice.id.clone(),
            kind: CreditKind::Goods,
            lines: vec![CreditLineInput {
                origin_line_id: invoice.lines[0].id.clone(),
                qty: Qty(3_000),
                restock: true,
            }],
            amount: None,
            reason: CreditReason::Defective,
            refund_method: PaymentMethod::Credit,
            refund_reference: None,
            note: None,
            second_lang: None,
        },
    )
    .unwrap();
    sales.push(av1.document.clone());

    // 5. The BL invoice fully credited: it stays in its series and is shown cancelled.
    let av2 = billing::create_credit_note(
        &t.st,
        CreditNoteInput {
            invoice_id: bl_invoice.id.clone(),
            kind: CreditKind::Financial,
            lines: Vec::new(),
            amount: Some(bl_invoice.total_ttc),
            reason: CreditReason::Cancellation,
            refund_method: PaymentMethod::Credit,
            refund_reference: None,
            note: None,
            second_lang: None,
        },
    )
    .unwrap();
    assert!(av2.invoice_fully_credited);
    sales.push(av2.document.clone());

    // 6. A versement of 1 000.00 in cash.
    let ctx = t.st.till(Permission::CreditCollect).unwrap();
    let versement = pos_core::customers::credit::CollectInput {
        customer_id: amine.id.clone(),
        amount: Money(100_000),
        method: PaymentMethod::Cash,
        reference: None,
        cheque_bank: None,
        cheque_date: None,
        allocate_to: Vec::new(),
        note: None,
    };
    t.st.db
        .write(move |tx| pos_core::customers::credit::collect(tx, &ctx, versement))
        .unwrap();

    // 7. Another ticket: 1 090.00 + 11.00 of duty, paid with 1 200.00.
    sales.push(t.sell(&sand.id, 1, 120_000).document);

    // The sales summary counts each sale once (the ticket's invoice adds nothing).
    let today = pos_core::time::business_date(pos_core::time::now_ms());
    let (from, to) = (
        today.format("%Y-%m-01").to_string(),
        today.format("%Y-%m-%d").to_string(),
    );
    let report = |kind: ReportKind| {
        crate::commands::reports::run(&t.st, ReportRequest::new(kind, &from, &to))
            .unwrap()
            .report
    };
    let total = |r: &pos_core::reports::Report, key: &str| {
        let i = r.table.column(key).unwrap();
        r.table.totals.as_ref().unwrap()[i].as_i64().unwrap()
    };
    let expected: i64 = sales.iter().map(|d| d.total_ttc.0).sum();
    assert_eq!(total(&report(ReportKind::SalesSummary), "netTtc"), expected);

    // The G50 equals the VAT and stamp statements.
    let g50 = fiscal::g50_of(&t.st, today.format("%Y-%m").to_string()).unwrap();
    let line = |key: &str| g50.lines.iter().find(|l| l.key == key).unwrap().amount.0;
    assert_eq!(
        line("tvaTotal"),
        total(&report(ReportKind::VatByRate), "tax")
    );
    assert_eq!(
        line("stampDuty"),
        total(&report(ReportKind::StampDutyStatement), "dutyTotal")
    );
    assert!(
        line("stampDuty") >= ticket.stamp_duty.0 + invoice.stamp_duty.0,
        "documents and the versement"
    );

    // The customer's balance: credit parts less credit notes and the versement.
    let balance = {
        let id = amine.id.clone();
        t.st.db
            .read(move |c| pos_core::customers::get(c, &id))
            .unwrap()
            .balance
    };
    let on_account = invoice.credit_amount.0 + bl_invoice.credit_amount.0;
    assert_eq!(
        balance.0,
        on_account - av1.refund.0 - av2.refund.0 - 100_000
    );

    // States.
    let state = |id: &str| billing::state(&t.st, id.to_owned()).unwrap();
    assert!(matches!(state(&quote.id), DocState::Converted { .. }));
    assert!(matches!(state(&ticket.id), DocState::Invoiced { .. }));
    assert!(matches!(state(&deliveries[0]), DocState::Invoiced { .. }));
    assert!(matches!(
        state(&invoice.id),
        DocState::PartlyCredited { .. }
    ));
    assert!(matches!(state(&bl_invoice.id), DocState::Credited { .. }));
    assert!(matches!(
        state(&ticket_invoice.id),
        DocState::Paid | DocState::Final
    ));

    // First print original, then DUPLICATA; the chains and the archive verify.
    let first = print::print_document(
        &t.st,
        &host,
        print::PrintDocumentInput {
            id: invoice.id.clone(),
            options: PrintOptions::default(),
            printer: None,
            copies: None,
        },
    )
    .unwrap();
    let again = print::print_document(
        &t.st,
        &host,
        print::PrintDocumentInput {
            id: invoice.id.clone(),
            options: PrintOptions::default(),
            printer: None,
            copies: None,
        },
    )
    .unwrap();
    assert!(!first.duplicate && again.duplicate);
    assert!(
        crate::commands::documents::verify_chain(&t.st)
            .unwrap()
            .iter()
            .all(|c| c.first_broken_seq.is_none())
    );
    assert_eq!(
        crate::commands::audit::verify(&t.st)
            .unwrap()
            .first_broken_seq,
        None
    );
    let receipts = crate::commands::credit::verify_receipts(&t.st).unwrap();
    assert_eq!(
        (receipts.rows, receipts.first_broken_number),
        (1, None),
        "the versement's RV receipt is checked too"
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("archive.zip");
    let done = fiscal::export_archive_to(
        &t.st,
        &host,
        fiscal::ArchiveInput {
            from: from.clone(),
            to: to.clone(),
            lang: Lang::Ar,
        },
        &path,
        |_| {},
    )
    .unwrap();
    assert!(
        pos_docs::fiscal_archive::verify(&path)
            .unwrap()
            .bad_files
            .is_empty()
    );
    assert_eq!(done.chains.receipts.first_broken_number, None);
    let validated = t.count("SELECT COUNT(*) FROM documents WHERE status = 'validated'");
    assert_eq!(
        done.files
            .iter()
            .find(|f| f.name == "documents.csv")
            .and_then(|f| f.rows),
        Some(validated)
    );
}

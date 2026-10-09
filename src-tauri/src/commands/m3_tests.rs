//! M3 commands against a real in-memory shop: permissions, kinds per role, approvals, costs.

use super::billing;
use super::documents;
use super::exports;
use crate::commands::testshop::shop;
use pos_core::auth::Permission;
use pos_core::billing::archive::ArchiveQuery;
use pos_core::billing::convert::TicketInvoiceInput;
use pos_core::billing::credit_notes::{CreditLineInput, CreditNoteInput, CreditReason};
use pos_core::billing::drafts::DraftInput;
use pos_core::billing::state::DocState;
use pos_core::billing::validate::ValidateInput;
use pos_core::documents::{CreditKind, DocOrigin, DocType};
use pos_core::lang::Lang;
use pos_core::money::{Money, Qty};
use pos_core::payments::{PaymentInput, PaymentMethod};
use pos_core::sales::{Cart, CartLine};
use pos_core::stock::LocationInput;
use pos_core::stock::transfers::{TransferInput, TransferLine};
use pos_docs::sheet::SheetFormat;

fn draft(customer: &str, product: &str, kind: DocType) -> DraftInput {
    let mut cart = Cart::new(Lang::Fr, vec![CartLine::product(product, Qty(1_000))]);
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

fn on_account(amount: i64) -> Vec<PaymentInput> {
    vec![PaymentInput {
        method: PaymentMethod::Credit,
        ..PaymentInput::cash(Money(amount))
    }]
}

#[test]
fn a_cashier_makes_no_invoice_and_sees_no_commercial_document() {
    let t = shop();
    t.complete_company();
    let customer = t.b2b_customer("SARL Amine");
    let cement = t.product("Ciment 25 kg", 119_000, 1_900, None);
    t.stock_in(&cement.id, 10);
    t.owner();
    let doc = billing::save_draft(
        &t.st,
        None,
        draft(&customer.id, &cement.id, DocType::Invoice),
    )
    .unwrap();
    let done = billing::validate(
        &t.st,
        ValidateInput {
            id: doc.id.clone(),
            payments: on_account(119_000),
        },
    )
    .unwrap();
    let year = pos_core::time::business_date(pos_core::time::now_ms())
        .format("%Y")
        .to_string();
    assert_eq!(
        done.document.number,
        Some(format!("FA{year}-000001")),
        "the first invoice of the year"
    );
    let page = billing::search(&t.st, ArchiveQuery::default()).unwrap();
    assert_eq!(
        page.rows.len(),
        1,
        "an empty kind list means every kind the user may see"
    );

    t.cashier();
    let err = billing::save_draft(
        &t.st,
        None,
        draft(&customer.id, &cement.id, DocType::Invoice),
    )
    .unwrap_err();
    assert_eq!(err.code(), "insufficient_permission");
    let page = billing::search(
        &t.st,
        ArchiveQuery {
            kinds: vec![DocType::Invoice, DocType::Ticket],
            ..ArchiveQuery::default()
        },
    )
    .unwrap();
    assert!(
        page.rows.is_empty(),
        "a cashier's own tickets stay in M1b's /sales history"
    );
    assert_eq!(
        billing::state(&t.st, doc.id).unwrap_err().code(),
        "document_not_found"
    );
}

#[test]
fn a_user_without_reports_profit_gets_documents_without_costs() {
    let t = shop();
    t.complete_company();
    let customer = t.b2b_customer("SARL Amine");
    let cement = t.product("Ciment 25 kg", 119_000, 1_900, None);
    t.with_permissions(&[Permission::DocumentsQuote]);
    let doc =
        billing::save_draft(&t.st, None, draft(&customer.id, &cement.id, DocType::Quote)).unwrap();
    assert!(
        doc.lines.iter().all(|l| l.cost_price == Money::ZERO),
        "the product costs 714.00"
    );
    t.owner();
    let doc =
        billing::save_draft(&t.st, None, draft(&customer.id, &cement.id, DocType::Quote)).unwrap();
    assert_eq!(doc.lines[0].cost_price, Money(71_400));
}

#[test]
fn a_price_override_on_an_invoice_needs_a_fresh_manager_approval() {
    let t = shop();
    t.complete_company();
    let customer = t.b2b_customer("SARL Amine");
    let cement = t.product("Ciment 25 kg", 119_000, 1_900, None);
    t.with_permissions(&[Permission::DocumentsInvoice]);
    let mut input = draft(&customer.id, &cement.id, DocType::Invoice);
    input.cart.lines[0].unit_price = Some(Money(100_000));
    let err = billing::save_draft(&t.st, None, input.clone()).unwrap_err();
    assert_eq!(err.code(), "price_override_not_allowed");
    t.approve(Permission::SalesPriceOverride);
    billing::save_draft(&t.st, None, input.clone()).unwrap();
    assert_eq!(
        billing::save_draft(&t.st, None, input).unwrap_err().code(),
        "price_override_not_allowed",
        "spent"
    );
}

/// M1b's lookups show an invoice to whoever holds `documents.invoice`, and no longer to a
/// cashier of the terminal it was made on.
#[test]
fn the_sales_lookups_show_a_commercial_document_to_its_kind_only() {
    let t = shop();
    let invoice = t.validated_invoice();
    t.with_permissions(&[Permission::DocumentsInvoice]);
    let seen = documents::get(&t.st, invoice.clone()).unwrap();
    assert_eq!(seen.lines[0].cost_price, Money::ZERO, "no reports.profit");
    let number = seen.number.clone().unwrap();
    assert!(documents::find(&t.st, number.clone()).unwrap().is_some());

    t.cashier();
    assert_eq!(
        documents::get(&t.st, invoice).unwrap_err().code(),
        "document_not_found"
    );
    assert_eq!(
        documents::find(&t.st, number).unwrap_err().code(),
        "document_not_found"
    );
}

#[test]
fn the_sales_history_lists_a_commercial_document_to_its_kind_only() {
    use pos_core::documents::DocumentQuery;
    let t = shop();
    let invoice = t.validated_invoice();
    let ticket = t.cash_ticket();
    let listed = |t: &crate::commands::testshop::TestShop| {
        documents::list(&t.st, DocumentQuery::default())
            .unwrap()
            .into_iter()
            .map(|r| r.id)
            .collect::<Vec<_>>()
    };
    let all = listed(&t);
    assert!(
        all.contains(&invoice) && all.contains(&ticket),
        "the owner sees both"
    );
    t.cashier();
    let seen = listed(&t);
    assert!(seen.contains(&ticket), "the till's own ticket");
    assert!(
        !seen.contains(&invoice),
        "an invoice made on this terminal is not in a cashier's history"
    );
    t.with_permissions(&[Permission::SalesViewAll]);
    let seen = listed(&t);
    assert!(seen.contains(&ticket));
    assert!(
        !seen.contains(&invoice),
        "sales.view_all opens every terminal, not the invoices"
    );
}

#[test]
fn a_sales_reader_never_sees_a_purchase_document_or_a_return_to_a_supplier() {
    use pos_core::purchases::invoices::InvoiceInput;
    use pos_core::purchases::lines::PurchaseLineInput;
    use pos_core::purchases::returns::{SupplierReturnInput, SupplierReturnLine};
    use pos_core::suppliers::SupplierInput;
    let t = shop();
    t.owner();
    let cement = t.product("Ciment 25 kg", 119_000, 1_900, None);
    t.stock_in(&cement.id, 10);
    let supplier = super::suppliers::create(&t.st, SupplierInput::named("Cimenterie Chlef"))
        .unwrap()
        .id;
    let bought = super::purchases::record_invoice(
        &t.st,
        InvoiceInput {
            supplier_id: supplier.clone(),
            supplier_number: "F-2026-0931".into(),
            receipt_ids: Vec::new(),
            lines: vec![PurchaseLineInput {
                product_id: cement.id.clone(),
                qty: Qty(5_000),
                unit_cost: Money(90_000),
                pack_qty: None,
                tax_rate_id: None,
                discount: Default::default(),
                origin_line_id: None,
                lot_number: None,
                expiry_date: None,
                serials: Vec::new(),
                note: None,
            }],
            note: None,
            payment: None,
        },
    )
    .unwrap()
    .invoice;
    let back = super::purchases::return_to_supplier(
        &t.st,
        SupplierReturnInput {
            supplier_id: supplier,
            location_id: None,
            lines: vec![SupplierReturnLine {
                product_id: cement.id.clone(),
                qty: Qty(1_000),
                unit_cost: Some(Money(90_000)),
                lot_number: None,
                serials: Vec::new(),
                note: None,
            }],
            credit_supplier: true,
            note: None,
        },
    )
    .unwrap()
    .document;
    assert_eq!(back.doc_type, DocType::ReturnNote);
    let numbers = [bought.number.clone().unwrap(), back.number.clone().unwrap()];
    // The owner (purchases.view) finds both, in the lookups and in the archive.
    assert!(documents::get(&t.st, back.id.clone()).is_ok());
    let page = billing::search(
        &t.st,
        ArchiveQuery {
            kinds: vec![DocType::ReturnNote],
            ..ArchiveQuery::default()
        },
    )
    .unwrap();
    assert_eq!(page.rows.len(), 1, "the return to the supplier");

    t.cashier();
    for id in [bought.id.clone(), back.id.clone()] {
        assert_eq!(
            documents::get(&t.st, id).unwrap_err().code(),
            "document_not_found"
        );
    }
    for number in numbers.clone() {
        assert_eq!(
            documents::find(&t.st, number).unwrap_err().code(),
            "document_not_found",
            "purchase costs stay with purchases.view"
        );
    }
    t.with_permissions(&[Permission::SalesViewAll, Permission::SalesReprint]);
    assert_eq!(
        documents::get(&t.st, back.id.clone()).unwrap_err().code(),
        "document_not_found",
        "a return note to a supplier is not a sales return note"
    );
    let page = billing::search(
        &t.st,
        ArchiveQuery {
            kinds: vec![DocType::ReturnNote],
            ..ArchiveQuery::default()
        },
    )
    .unwrap();
    assert!(page.rows.is_empty(), "nor in the archive's return notes");
    assert_eq!(
        super::print::require_output(&t.st, &back.id)
            .unwrap_err()
            .code(),
        "insufficient_permission",
        "nor printed"
    );
}

/// A clerk holding only `documents.credit_note` corrects an invoice: he sees neither its costs,
/// nor the customer's balance (kridi data), nor the invoice itself in the states and links.
#[test]
fn a_credit_note_clerk_corrects_the_invoice_without_costs_balances_or_invoices() {
    let t = shop();
    let invoice = t.validated_invoice();
    let lines = billing::creditable(&t.st, invoice.clone()).unwrap();
    assert_eq!(
        lines[0].line.cost_price,
        Money(71_400),
        "the owner sees costs"
    );

    t.with_permissions(&[Permission::DocumentsCreditNote]);
    let lines = billing::creditable(&t.st, invoice.clone()).unwrap();
    assert_eq!(lines[0].line.cost_price, Money::ZERO);
    let input = CreditNoteInput {
        invoice_id: invoice.clone(),
        kind: CreditKind::Goods,
        lines: vec![CreditLineInput {
            origin_line_id: lines[0].line.id.clone(),
            qty: Qty(1_000),
            restock: true,
        }],
        amount: None,
        reason: CreditReason::Defective,
        refund_method: PaymentMethod::Credit,
        refund_reference: None,
        note: None,
        second_lang: None,
    };
    let quote = billing::credit_quote(&t.st, input.clone()).unwrap();
    assert_eq!(
        (quote.refund, quote.invoice_remaining),
        (Money(119_000), Money::ZERO)
    );
    let done = billing::create_credit_note(&t.st, input).unwrap();
    assert!(done.invoice_fully_credited);
    assert_eq!(done.customer_balance, None, "no credit.view");
    assert!(
        done.document
            .lines
            .iter()
            .all(|l| l.cost_price == Money::ZERO)
    );
    assert_eq!(
        t.count("SELECT COUNT(*) FROM audit_log WHERE action = 'credit_note.created'"),
        1
    );
    assert_eq!(
        billing::state(&t.st, invoice.clone()).unwrap_err().code(),
        "document_not_found",
        "an invoice is for documents.invoice"
    );
    assert_eq!(
        billing::state(&t.st, done.document.id.clone()).unwrap(),
        DocState::Final
    );
    let from = billing::links(&t.st, done.document.id.clone()).unwrap();
    assert_eq!(from.sources.len(), 1, "the credit note cites its invoice");

    t.owner();
    assert!(matches!(
        billing::state(&t.st, invoice.clone()).unwrap(),
        DocState::Credited { .. }
    ));
    let links = billing::links(&t.st, invoice).unwrap();
    assert!(links.sources.is_empty());
    assert_eq!(
        links
            .targets
            .iter()
            .map(|l| l.number.clone())
            .collect::<Vec<_>>(),
        vec![done.document.number]
    );
}

/// A transfer note's totals in the archive are its value at the CMUP: `reports.profit` only.
#[test]
fn the_archive_values_a_transfer_note_for_reports_profit_only() {
    let t = shop();
    t.owner();
    let oil = t.product("Huile 5 L", 119_000, 1_900, None);
    t.stock_in(&oil.id, 10);
    let shelf = super::stock::locations(&t.st)
        .unwrap()
        .into_iter()
        .find(|l| l.is_default)
        .unwrap()
        .id;
    let depot = LocationInput {
        code: "DEPOT".into(),
        name_fr: "Dépôt".into(),
        ..LocationInput::default()
    };
    let depot = super::stock::create_location(&t.st, depot).unwrap().id;
    let transfer = TransferInput {
        from_location_id: shelf,
        to_location_id: depot,
        lines: vec![TransferLine {
            product_id: oil.id,
            qty: Qty(2_000),
            lot_number: None,
            serials: Vec::new(),
        }],
        driver_name: None,
        vehicle: None,
        note: None,
    };
    super::stock_m2::transfer(&t.st, transfer).unwrap();
    let transfers = ArchiveQuery {
        kinds: vec![DocType::Transfer],
        ..ArchiveQuery::default()
    };
    let row = &billing::search(&t.st, transfers.clone()).unwrap().rows[0];
    assert_eq!(row.total_due, Money(2 * 71_400), "the owner sees the value");

    t.with_permissions(&[Permission::StockTransfer]);
    let row = &billing::search(&t.st, transfers).unwrap().rows[0];
    assert_eq!((row.total_ttc, row.total_due), (Money::ZERO, Money::ZERO));
}

#[test]
fn a_till_ticket_is_invoiced_and_its_warranty_lines_follow_the_sales_lookups() {
    let t = shop();
    t.complete_company();
    let customer = t.b2b_customer("SARL Amine");
    let ticket = t.cash_ticket();
    let done = billing::invoice_ticket(
        &t.st,
        TicketInvoiceInput {
            ticket_id: ticket.clone(),
            customer_id: customer.id,
            second_lang: None,
            note: None,
        },
    )
    .unwrap();
    assert_eq!(done.document.origin, Some(DocOrigin::Ticket));
    assert_eq!(
        billing::state(&t.st, ticket.clone()).unwrap(),
        DocState::Invoiced {
            invoice: done.document.number.clone().unwrap()
        }
    );
    assert_eq!(
        billing::warranty_lines(&t.st, ticket.clone())
            .unwrap_err()
            .code(),
        "warranty_not_set",
        "milk carries no warranty"
    );

    t.cashier();
    assert_eq!(
        billing::warranty_lines(&t.st, ticket).unwrap_err().code(),
        "warranty_not_set",
        "the ticket of this till"
    );
    assert_eq!(
        billing::warranty_lines(&t.st, done.document.id)
            .unwrap_err()
            .code(),
        "document_not_found",
        "an invoice is for documents.invoice"
    );
}

/// The archive's export (xlsx, csv and the PDF of the table) has the archive's own permission:
/// every page of the rows of the kinds the user may see, with their kind and state in the file's
/// language; a quote clerk's file holds no invoice, and a role without the archive gets nothing.
#[test]
fn the_archive_exports_every_page_of_the_kinds_the_user_may_see() {
    let t = shop();
    let invoice = t.validated_invoice();
    let bina = t.b2b_customer("EURL Bina");
    let sand = t.product("Sable 1 m3", 250_000, 1_900, None);
    let quote =
        billing::save_draft(&t.st, None, draft(&bina.id, &sand.id, DocType::Quote)).unwrap();
    let quote = billing::validate(
        &t.st,
        ValidateInput {
            id: quote.id,
            payments: Vec::new(),
        },
    )
    .unwrap()
    .document;
    let invoice = documents::get(&t.st, invoice).unwrap();
    let (fa, dv) = (invoice.number.unwrap(), quote.number.unwrap());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("documents.csv");
    let request = |limit| exports::ExportRequest {
        target: exports::ExportTarget::Documents {
            query: ArchiveQuery {
                limit,
                ..ArchiveQuery::default()
            },
        },
        format: SheetFormat::Csv,
        lang: Lang::Fr,
    };
    let read = || std::fs::read_to_string(&path).unwrap();

    let done = exports::export_to(&t.st, request(Some(1)), &path).unwrap();
    assert_eq!(done.rows, 2, "one row a page, every page");
    let csv = read();
    for text in [
        fa.as_str(),
        dv.as_str(),
        "Client / fournisseur",
        "Net à payer",
        "Facture",
        "Devis",
        "SARL Amine",
        "EURL Bina",
        "Reste dû",
        "En cours",
    ] {
        assert!(csv.contains(text), "{text} in {csv}");
    }

    t.with_permissions(&[Permission::DocumentsQuote]);
    let done = exports::export_to(&t.st, request(None), &path).unwrap();
    assert_eq!(done.rows, 1);
    let csv = read();
    assert!(csv.contains(&dv) && !csv.contains(&fa), "{csv}");

    let actor = t.owner.actor();
    let clerk =
        t.st.db
            .write(move |tx| {
                pos_core::auth::roles::create(
                    tx,
                    &actor,
                    pos_core::auth::roles::RoleInput {
                        name_ar: "مكلف بالمنتجات".into(),
                        name_fr: "Produits".into(),
                        name_en: "Products".into(),
                        permissions: vec![Permission::ProductsView],
                    },
                )
            })
            .unwrap();
    let clerk = t.user("Nadia", &clerk.id, "9753", 0);
    t.sign_in(&clerk, "9753");
    assert_eq!(
        exports::export_to(&t.st, request(None), &path)
            .unwrap_err()
            .code(),
        "insufficient_permission"
    );
}

#[test]
fn the_till_loads_a_quote_and_links_its_sale() {
    let t = shop();
    t.complete_company();
    let customer = t.b2b_customer("SARL Amine");
    let cement = t.product("Ciment 25 kg", 119_000, 1_900, None);
    t.stock_in(&cement.id, 10);
    t.owner();
    let quote =
        billing::save_draft(&t.st, None, draft(&customer.id, &cement.id, DocType::Quote)).unwrap();
    billing::validate(
        &t.st,
        ValidateInput {
            id: quote.id.clone(),
            payments: Vec::new(),
        },
    )
    .unwrap();
    t.cashier();
    t.open_till(1_000_000);
    let cart = billing::offer_cart(&t.st, quote.id.clone()).unwrap();
    assert_eq!(cart.customer_id.as_deref(), Some(customer.id.as_str()));
    assert_eq!(cart.lines[0].qty, Qty(1_000));
    // 1 190.00 DA and the stamp duty on the cash part: 1 300 DA covers both.
    let ticket = t.sell(&cement.id, 1, 130_000).document;
    billing::link_sale(&t.st, quote.id.clone(), ticket.id).unwrap();
    t.owner();
    let state = billing::state(&t.st, quote.id).unwrap();
    assert!(matches!(
        state,
        pos_core::billing::state::DocState::Converted { .. }
    ));
}

/// SAL-046: the till finds a validated quote by its printed number (typed in any case) although
/// the cashier holds no documents permission; a draft quote stays hidden from him.
#[test]
fn the_till_finds_a_validated_offer_by_its_number() {
    let t = shop();
    t.complete_company();
    let customer = t.b2b_customer("SARL Amine");
    let cement = t.product("Ciment 25 kg", 119_000, 1_900, None);
    t.owner();
    let quote =
        billing::save_draft(&t.st, None, draft(&customer.id, &cement.id, DocType::Quote)).unwrap();
    let pending =
        billing::save_draft(&t.st, None, draft(&customer.id, &cement.id, DocType::Quote)).unwrap();
    let number = billing::validate(
        &t.st,
        ValidateInput {
            id: quote.id.clone(),
            payments: Vec::new(),
        },
    )
    .unwrap()
    .document
    .number
    .unwrap();

    t.cashier();
    let found = documents::find(&t.st, number.to_lowercase())
        .unwrap()
        .unwrap();
    assert_eq!(found.id, quote.id);
    assert_eq!(found.lines[0].cost_price, Money::ZERO, "no reports.profit");
    assert_eq!(
        documents::get(&t.st, pending.id).unwrap_err().code(),
        "document_not_found",
        "a draft is no offer to sell"
    );
}

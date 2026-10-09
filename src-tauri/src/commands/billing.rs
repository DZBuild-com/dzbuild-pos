//! Commercial documents (M3): drafts, validation, conversions, credit notes, archive, states,
//! mentions, the Arabic guard and warranty lines. Every command is `<module>_<verb>` over one
//! `pos_core::billing` call; costs are redacted like M1b's documents. Each kind needs its own
//! permission (`kind_permission`): a user sees and handles only the kinds he may.

use super::{blocking, documents as lookups, redact};
use crate::state::{AppState, SharedState};
use pos_core::auth::{Permission, Session};
use pos_core::billing::archive::{self, ArchivePage, ArchiveQuery, ArchiveRow};
use pos_core::billing::convert::{self, ConvertInput, DeliveriesInput, TicketInvoiceInput};
use pos_core::billing::credit_notes::{
    self, CreditNoteInput, CreditQuote, CreditableLine, ValidatedCreditNote,
};
use pos_core::billing::drafts::{self, DraftCart, DraftInput, DraftPreview};
use pos_core::billing::guard::{self, ArabicWarning, GuardInput};
use pos_core::billing::invoices;
use pos_core::billing::mentions::{self, MentionCheck};
use pos_core::billing::state::{self as doc_state, DocState};
use pos_core::billing::validate::{self, ValidateInput, ValidatedDocument};
use pos_core::billing::warranty::{self, WarrantySource};
use pos_core::billing::{BILLING_KINDS, kind_permission};
use pos_core::documents::links::{self, LinkedDocument};
use pos_core::documents::{self, DocType, Document};
use pos_core::error::AppError;
use pos_core::payments::{PaymentInput, Settlement};
use pos_core::sales::Cart;
use pos_core::time::business_date;
use serde::Serialize;
use specta::Type;
use tauri::State;

/// The documents a document was made from and those made from it, oldest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DocumentLinks {
    pub sources: Vec<LinkedDocument>,
    pub targets: Vec<LinkedDocument>,
}

/// Any of these opens the commercial documents.
const DOCUMENT_PERMISSIONS: [Permission; 4] = [
    Permission::DocumentsInvoice,
    Permission::DocumentsQuote,
    Permission::DocumentsDelivery,
    Permission::DocumentsCreditNote,
];

/// Any of these opens the archive (and its export); what it lists is `visible_kinds`.
/// `sales.reprint` lets the cashier in, to find no kind there: his own tickets stay in M1b's
/// sales history.
pub(crate) const ARCHIVE_PERMISSIONS: [Permission; 8] = [
    Permission::DocumentsInvoice,
    Permission::DocumentsQuote,
    Permission::DocumentsDelivery,
    Permission::DocumentsCreditNote,
    Permission::SalesViewAll,
    Permission::SalesReprint,
    Permission::PurchasesView,
    Permission::StockTransfer,
];

/// The kinds a session may see in the archive. `return_note` stands for both sides: which ones
/// it sees is [`returns_of`].
pub(crate) fn visible_kinds(session: &Session) -> Vec<DocType> {
    let mut kinds: Vec<DocType> = BILLING_KINDS
        .into_iter()
        .filter(|k| kind_permission(*k).is_some_and(|p| session.has(p)))
        .collect();
    if session.has(Permission::SalesViewAll) {
        kinds.extend([DocType::Ticket, DocType::ReturnNote]);
    }
    if session.has(Permission::PurchasesView) {
        kinds.extend([
            DocType::PurchaseOrder,
            DocType::GoodsReceipt,
            DocType::SupplierInvoice,
        ]);
        if !kinds.contains(&DocType::ReturnNote) {
            kinds.push(DocType::ReturnNote);
        }
    }
    if session.has(Permission::StockTransfer) {
        kinds.push(DocType::Transfer);
    }
    kinds
}

/// The return notes a session sees: to customers with `sales.view_all`, to suppliers (purchase
/// costs) with `purchases.view`.
pub(crate) fn returns_of(session: &Session) -> archive::Returns {
    archive::Returns {
        sales: session.has(Permission::SalesViewAll),
        supplier: session.has(Permission::PurchasesView),
    }
}

fn kind_of(doc_type: DocType) -> Result<Permission, AppError> {
    kind_permission(doc_type)
        .ok_or_else(|| AppError::validation("docType", "document_kind_invalid"))
}

/// The kind of a stored draft; the session is checked before the lookup, so a call without one
/// fails on it and a user without any documents permission learns nothing about the id.
fn draft_kind(st: &AppState, id: &str) -> Result<DocType, AppError> {
    st.require_any(&DOCUMENT_PERMISSIONS)?;
    let id = id.to_owned();
    Ok(st.db.read(move |c| documents::get(c, &id))?.doc_type)
}

/// `doc` when the session sees it: its kind is in `kinds` and, for a return note, its side in
/// `returns`; any other is "not found".
fn visible(
    doc: Document,
    kinds: &[DocType],
    returns: archive::Returns,
) -> Result<Document, AppError> {
    let side = doc.doc_type != DocType::ReturnNote || returns.shows(doc.supplier_id.is_some());
    if kinds.contains(&doc.doc_type) && side {
        Ok(doc)
    } else {
        Err(AppError::not_found("document"))
    }
}

/// Today's business day on the commands' clock (`AppState::now`, the one `till` dates with).
fn today(st: &AppState) -> String {
    business_date(st.now()).format("%Y-%m-%d").to_string()
}

#[tauri::command]
#[specta::specta]
pub async fn documents_preview_draft(
    state: State<'_, SharedState>,
    input: DraftInput,
) -> Result<DraftPreview, AppError> {
    let st = state.inner().clone();
    blocking(move || preview(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn documents_save_draft(
    state: State<'_, SharedState>,
    id: Option<String>,
    input: DraftInput,
) -> Result<Document, AppError> {
    let st = state.inner().clone();
    blocking(move || save_draft(&st, id, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn documents_draft_cart(
    state: State<'_, SharedState>,
    id: String,
) -> Result<DraftCart, AppError> {
    let st = state.inner().clone();
    blocking(move || draft_cart(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn documents_cancel_draft(
    state: State<'_, SharedState>,
    id: String,
) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || cancel_draft(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn documents_validate(
    state: State<'_, SharedState>,
    input: ValidateInput,
) -> Result<ValidatedDocument, AppError> {
    let st = state.inner().clone();
    blocking(move || validate(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn documents_payment_preview(
    state: State<'_, SharedState>,
    id: String,
    tenders: Vec<PaymentInput>,
) -> Result<Settlement, AppError> {
    let st = state.inner().clone();
    blocking(move || payment_preview(&st, id, tenders)).await
}

#[tauri::command]
#[specta::specta]
pub async fn documents_convert(
    state: State<'_, SharedState>,
    input: ConvertInput,
) -> Result<Document, AppError> {
    let st = state.inner().clone();
    blocking(move || convert(&st, input)).await
}

/// DOC-031: the figures of the invoice of the ticked delivery notes with the typed tenders (its
/// TTC, the duty on the cash part, the net to pay, what is left, the change); nothing written.
#[tauri::command]
#[specta::specta]
pub async fn documents_deliveries_preview(
    state: State<'_, SharedState>,
    delivery_ids: Vec<String>,
    tenders: Vec<PaymentInput>,
) -> Result<Settlement, AppError> {
    let st = state.inner().clone();
    blocking(move || deliveries_preview(&st, delivery_ids, tenders)).await
}

#[tauri::command]
#[specta::specta]
pub async fn documents_invoice_deliveries(
    state: State<'_, SharedState>,
    input: DeliveriesInput,
) -> Result<ValidatedDocument, AppError> {
    let st = state.inner().clone();
    blocking(move || invoice_deliveries(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn documents_invoice_ticket(
    state: State<'_, SharedState>,
    input: TicketInvoiceInput,
) -> Result<ValidatedDocument, AppError> {
    let st = state.inner().clone();
    blocking(move || invoice_ticket(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn documents_link_sale(
    state: State<'_, SharedState>,
    quote_id: String,
    ticket_id: String,
) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || link_sale(&st, quote_id, ticket_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn documents_offer_cart(
    state: State<'_, SharedState>,
    id: String,
) -> Result<Cart, AppError> {
    let st = state.inner().clone();
    blocking(move || offer_cart(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn documents_creditable(
    state: State<'_, SharedState>,
    invoice_id: String,
) -> Result<Vec<CreditableLine>, AppError> {
    let st = state.inner().clone();
    blocking(move || creditable(&st, invoice_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn documents_credit_quote(
    state: State<'_, SharedState>,
    input: CreditNoteInput,
) -> Result<CreditQuote, AppError> {
    let st = state.inner().clone();
    blocking(move || credit_quote(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn documents_create_credit_note(
    state: State<'_, SharedState>,
    input: CreditNoteInput,
) -> Result<ValidatedCreditNote, AppError> {
    let st = state.inner().clone();
    blocking(move || create_credit_note(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn documents_search(
    state: State<'_, SharedState>,
    query: ArchiveQuery,
) -> Result<ArchivePage, AppError> {
    let st = state.inner().clone();
    blocking(move || search(&st, query)).await
}

#[tauri::command]
#[specta::specta]
pub async fn documents_state(
    state: State<'_, SharedState>,
    id: String,
) -> Result<DocState, AppError> {
    let st = state.inner().clone();
    blocking(move || self::state(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn documents_links(
    state: State<'_, SharedState>,
    id: String,
) -> Result<DocumentLinks, AppError> {
    let st = state.inner().clone();
    blocking(move || links(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn documents_mentions(
    state: State<'_, SharedState>,
    customer_id: Option<String>,
) -> Result<MentionCheck, AppError> {
    let st = state.inner().clone();
    blocking(move || mentions(&st, customer_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn documents_arabic_guard(
    state: State<'_, SharedState>,
    input: GuardInput,
) -> Result<Option<ArabicWarning>, AppError> {
    let st = state.inner().clone();
    blocking(move || arabic_guard(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn documents_warranty_lines(
    state: State<'_, SharedState>,
    document_id: String,
) -> Result<Vec<WarrantySource>, AppError> {
    let st = state.inner().clone();
    blocking(move || warranty_lines(&st, document_id)).await
}

/// The editor's live figures and missing mentions; pending approvals apply, none is spent.
pub(crate) fn preview(st: &AppState, input: DraftInput) -> Result<DraftPreview, AppError> {
    let ctx = st.till(kind_of(input.doc_type)?)?;
    let session = ctx.session.clone();
    let mut out = st.db.read(move |c| drafts::preview(c, &ctx, &input))?;
    redact::priced(&mut out.priced.cart, &session);
    Ok(out)
}

pub(crate) fn save_draft(
    st: &AppState,
    id: Option<String>,
    input: DraftInput,
) -> Result<Document, AppError> {
    let ctx = st.till(kind_of(input.doc_type)?)?;
    let session = ctx.session.clone();
    let for_tx = ctx.clone();
    let mut doc = st
        .db
        .write(move |tx| drafts::save_draft(tx, &for_tx, id.as_deref(), input))?;
    st.spend_approvals(&ctx)?;
    redact::document(&mut doc, &session);
    Ok(doc)
}

pub(crate) fn draft_cart(st: &AppState, id: String) -> Result<DraftCart, AppError> {
    st.require(kind_of(draft_kind(st, &id)?)?)?;
    st.db.read(move |c| drafts::draft_cart(c, &id))
}

pub(crate) fn cancel_draft(st: &AppState, id: String) -> Result<(), AppError> {
    let ctx = st.till(kind_of(draft_kind(st, &id)?)?)?;
    let for_tx = ctx.clone();
    st.db
        .write(move |tx| drafts::cancel_draft(tx, &for_tx, &id))?;
    st.spend_approvals(&ctx)
}

pub(crate) fn validate(st: &AppState, input: ValidateInput) -> Result<ValidatedDocument, AppError> {
    let ctx = st.till(kind_of(draft_kind(st, &input.id)?)?)?;
    let session = ctx.session.clone();
    let for_tx = ctx.clone();
    let mut done = st
        .db
        .write(move |tx| validate::validate(tx, &for_tx, input))?;
    st.spend_approvals(&ctx)?;
    redact::validated(&mut done, &session);
    Ok(done)
}

/// The validation dialog's settlement (duty on the cash part, change, what is left).
pub(crate) fn payment_preview(
    st: &AppState,
    id: String,
    tenders: Vec<PaymentInput>,
) -> Result<Settlement, AppError> {
    let ctx = st.till(Permission::DocumentsInvoice)?;
    st.db
        .read(move |c| invoices::payment_preview(c, &ctx, &id, &tenders))
}

/// A quote or proforma into a draft of the target kind: the target's permission, and the
/// source's kind must be one the session may see.
pub(crate) fn convert(st: &AppState, input: ConvertInput) -> Result<Document, AppError> {
    let ctx = st.till(kind_of(input.target)?)?;
    let session = ctx.session.clone();
    let (kinds, returns, source_id) = (
        visible_kinds(&session),
        returns_of(&session),
        input.source_id.clone(),
    );
    st.db
        .read(move |c| visible(documents::get(c, &source_id)?, &kinds, returns).map(drop))?;
    let for_tx = ctx.clone();
    let mut doc = st
        .db
        .write(move |tx| convert::convert(tx, &for_tx, input))?;
    st.spend_approvals(&ctx)?;
    redact::document(&mut doc, &session);
    Ok(doc)
}

pub(crate) fn deliveries_preview(
    st: &AppState,
    delivery_ids: Vec<String>,
    tenders: Vec<PaymentInput>,
) -> Result<Settlement, AppError> {
    let ctx = st.till(Permission::DocumentsInvoice)?;
    st.db
        .read(move |c| convert::deliveries_preview(c, &ctx, &delivery_ids, &tenders))
}

pub(crate) fn invoice_deliveries(
    st: &AppState,
    input: DeliveriesInput,
) -> Result<ValidatedDocument, AppError> {
    let ctx = st.till(Permission::DocumentsInvoice)?;
    let session = ctx.session.clone();
    let for_tx = ctx.clone();
    let mut done = st
        .db
        .write(move |tx| convert::invoice_deliveries(tx, &for_tx, input))?;
    st.spend_approvals(&ctx)?;
    redact::validated(&mut done, &session);
    Ok(done)
}

/// An invoice is a company-series document: on a till it is made on the main PC, which by then
/// holds the ticket (the till's outbox goes first, `lan::route::forward`).
pub(crate) fn invoice_ticket(
    st: &AppState,
    input: TicketInvoiceInput,
) -> Result<ValidatedDocument, AppError> {
    if let Some(done) = crate::lan::route::forward(
        st,
        "documents.invoice_ticket",
        serde_json::json!({ "input": input }),
    )? {
        return Ok(done);
    }
    let ctx = st.till(Permission::DocumentsInvoice)?;
    let session = ctx.session.clone();
    let for_tx = ctx.clone();
    let mut done = st
        .db
        .write(move |tx| convert::invoice_ticket(tx, &for_tx, input))?;
    st.spend_approvals(&ctx)?;
    redact::validated(&mut done, &session);
    Ok(done)
}

/// SAL-046: a validated, unexpired quote or proforma as a till cart; the till prices it at
/// today's prices like any sale (`sales_preview`), the offered prices riding along to flag the
/// lines whose price changed.
pub(crate) fn offer_cart(st: &AppState, id: String) -> Result<Cart, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    let today = ctx.business_day();
    st.db.read(move |c| convert::offer_cart(c, &id, &today))
}

/// A quote sold at the till: the ticket is recorded as made from it (SAL-046).
pub(crate) fn link_sale(
    st: &AppState,
    quote_id: String,
    ticket_id: String,
) -> Result<(), AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    let for_tx = ctx.clone();
    st.db
        .write(move |tx| convert::link_sale(tx, &for_tx, &quote_id, &ticket_id))?;
    st.spend_approvals(&ctx)
}

pub(crate) fn creditable(
    st: &AppState,
    invoice_id: String,
) -> Result<Vec<CreditableLine>, AppError> {
    let session = st.require(Permission::DocumentsCreditNote)?;
    let mut lines = st
        .db
        .read(move |c| credit_notes::creditable(c, &invoice_id))?;
    redact::creditable(&mut lines, &session);
    Ok(lines)
}

/// The figures of the credit note being typed (nothing written, no approval spent).
pub(crate) fn credit_quote(st: &AppState, input: CreditNoteInput) -> Result<CreditQuote, AppError> {
    let ctx = st.till(Permission::DocumentsCreditNote)?;
    st.db.read(move |c| credit_notes::quote(c, &ctx, &input))
}

pub(crate) fn create_credit_note(
    st: &AppState,
    input: CreditNoteInput,
) -> Result<ValidatedCreditNote, AppError> {
    let ctx = st.till(Permission::DocumentsCreditNote)?;
    let session = ctx.session.clone();
    let for_tx = ctx.clone();
    let mut done = st
        .db
        .write(move |tx| credit_notes::create_credit_note(tx, &for_tx, input))?;
    st.spend_approvals(&ctx)?;
    redact::credit_note(&mut done, &session);
    Ok(done)
}

pub(crate) fn search(st: &AppState, mut query: ArchiveQuery) -> Result<ArchivePage, AppError> {
    let session = st.require_any(&ARCHIVE_PERMISSIONS)?;
    // A kind the session may not see is dropped; none left gives an empty page (`archive::search`).
    let allowed = visible_kinds(&session);
    query.kinds = if query.kinds.is_empty() {
        allowed
    } else {
        query
            .kinds
            .into_iter()
            .filter(|k| allowed.contains(k))
            .collect()
    };
    let today = today(st);
    let returns = returns_of(&session);
    let mut page = st
        .db
        .read(move |c| archive::search(c, &query, &today, returns))?;
    for row in &mut page.rows {
        redact::archive_row(row, &session);
    }
    Ok(page)
}

/// Every row [`search`] finds for `query`, page after page from the newest (`query.limit` rows a
/// page, 200 by default): what the archive's export writes.
pub(crate) fn search_all(
    st: &AppState,
    mut query: ArchiveQuery,
) -> Result<Vec<ArchiveRow>, AppError> {
    query.before = None;
    query.limit = Some(query.limit.unwrap_or(archive::MAX_LIMIT));
    let mut rows = Vec::new();
    loop {
        let page = search(st, query.clone())?;
        rows.extend(page.rows);
        match page.next {
            Some(next) => query.before = Some(next),
            None => return Ok(rows),
        }
    }
}

pub(crate) fn state(st: &AppState, id: String) -> Result<DocState, AppError> {
    let session = st.require_any(&ARCHIVE_PERMISSIONS)?;
    let (kinds, returns) = (visible_kinds(&session), returns_of(&session));
    let today = today(st);
    st.db.read(move |c| {
        let doc = visible(documents::get(c, &id)?, &kinds, returns)?;
        doc_state::state_of(c, &doc, &today)
    })
}

pub(crate) fn links(st: &AppState, id: String) -> Result<DocumentLinks, AppError> {
    let session = st.require_any(&ARCHIVE_PERMISSIONS)?;
    let (kinds, returns) = (visible_kinds(&session), returns_of(&session));
    st.db.read(move |c| {
        let doc = visible(documents::get(c, &id)?, &kinds, returns)?;
        Ok(DocumentLinks {
            sources: links::sources(c, &doc.id)?,
            targets: links::targets(c, &doc.id)?,
        })
    })
}

/// What is missing for a document to `customer_id` (or to nobody), DOC-003.
pub(crate) fn mentions(
    st: &AppState,
    customer_id: Option<String>,
) -> Result<MentionCheck, AppError> {
    st.require_any(&DOCUMENT_PERMISSIONS)?;
    st.db
        .read(move |c| mentions::check(c, customer_id.as_deref()))
}

/// DOC-034: the warning before printing a consumer document without Arabic.
pub(crate) fn arabic_guard(
    st: &AppState,
    input: GuardInput,
) -> Result<Option<ArabicWarning>, AppError> {
    st.session()?;
    st.db.read(move |c| guard::arabic_guard(c, &input))
}

/// DOC-012: the warranted lines of a ticket or an invoice the session may look up (M1b's
/// rule: a ticket of this till, an invoice by `documents.invoice`).
pub(crate) fn warranty_lines(
    st: &AppState,
    document_id: String,
) -> Result<Vec<WarrantySource>, AppError> {
    let session = st.require_any(&[Permission::SalesReprint, Permission::DocumentsInvoice])?;
    st.db.read(move |c| {
        if !lookups::shown(&documents::get(c, &document_id)?, &session) {
            return Err(AppError::not_found("document"));
        }
        warranty::sources(c, &document_id, None)
    })
}

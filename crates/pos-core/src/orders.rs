//! Customer orders and layaway (master spec §25 ORD-001…003, CRD-007/008): an order is a
//! validated `customer_order` document (`CC2026-…`, drafted in M3's editor or at the till,
//! converted from a quote) with its kind, due date and status in `customer_orders`; its deposits
//! are Task 9's (`AC…` receipts, droit de timbre on their cash); its stock can be reserved.
//! Fulfilled at the till, it becomes a ticket at the ordered prices paid by a `voucher` tender
//! naming `CC…` (the deposit) and anything else — a layaway only when nothing is left on credit;
//! delivered as a BL or invoice, the deposit goes on the customer's account against it. Kept
//! arrhes are sold as the "Arrhes conservées" service line, never written off silently.
//!
//! LAN: the deposits are host-only writes (`pay_in` is `deposits::take`); every other
//! write here is one function the app registers like any document service.

use crate::audit::{self, Actor};
use crate::billing::drafts::{DraftInput, save_draft};
use crate::billing::validate::{self, ValidateInput, ValidatedDocument};
use crate::catalog::clean_opt;
use crate::catalog::products;
use crate::customers;
use crate::deposits::{self, Deposit, DepositInput, OwnerType};
use crate::documents::{self, DocStatus, DocType, Document};
use crate::error::AppError;
use crate::ids::new_id;
use crate::ledger::{self, EntryKind, NewEntry, Side};
use crate::money::{Money, Qty, Rate};
use crate::packs::{self, Module};
use crate::payments::{PaymentInput, PaymentMethod};
use crate::sales::{Cart, CartLine, SaleExtras, SaleInput, ValidatedSale};
use crate::sqlenum::sql_text_enum;
use crate::stock;
use crate::till::TillContext;
use crate::vouchers::{self, VoucherTarget};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OrderKind {
    Order,
    Layaway,
}

impl OrderKind {
    pub const ALL: [OrderKind; 2] = [OrderKind::Order, OrderKind::Layaway];

    pub const fn as_str(self) -> &'static str {
        match self {
            OrderKind::Order => "order",
            OrderKind::Layaway => "layaway",
        }
    }

    pub fn parse(s: &str) -> Option<OrderKind> {
        OrderKind::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

sql_text_enum!(OrderKind);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CustomerOrderStatus {
    Open,
    Ready,
    Fulfilled,
    Closed,
}

impl CustomerOrderStatus {
    pub const ALL: [CustomerOrderStatus; 4] = [
        CustomerOrderStatus::Open,
        CustomerOrderStatus::Ready,
        CustomerOrderStatus::Fulfilled,
        CustomerOrderStatus::Closed,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            CustomerOrderStatus::Open => "open",
            CustomerOrderStatus::Ready => "ready",
            CustomerOrderStatus::Fulfilled => "fulfilled",
            CustomerOrderStatus::Closed => "closed",
        }
    }

    pub fn parse(s: &str) -> Option<CustomerOrderStatus> {
        CustomerOrderStatus::ALL
            .into_iter()
            .find(|k| k.as_str() == s)
    }

    /// Open or ready: the order still takes deposits and can be served or closed.
    const fn live(self) -> bool {
        matches!(self, CustomerOrderStatus::Open | CustomerOrderStatus::Ready)
    }
}

sql_text_enum!(CustomerOrderStatus);

/// A deposit paid on an order (Task 9's tender rules: no kridi nor voucher, cheque details).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OrderDeposit {
    pub amount: Money,
    pub method: PaymentMethod,
    pub reference: Option<String>,
    pub cheque_bank: Option<String>,
    pub cheque_date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CustomerOrderInput {
    /// The editor's draft (`customer_order`, typed or converted from a quote); `None` at the till.
    pub draft_id: Option<String>,
    pub cart: Cart,
    pub kind: OrderKind,
    /// `YYYY-MM-DD`, not before today.
    pub due_on: Option<String>,
    /// Reserve the stock of the lines (always for a layaway).
    pub reserve: bool,
    pub deposit: Option<OrderDeposit>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Reserved {
    pub product_id: String,
    /// In French (the screen names it in its own language from the product list).
    pub name: String,
    pub qty: Qty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OrderLine {
    pub product_id: Option<String>,
    pub description: String,
    pub qty: Qty,
    pub unit_price: Money,
    pub total_ttc: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OrderEvent {
    pub status: CustomerOrderStatus,
    pub note: Option<String>,
    pub created_at: i64,
    pub user_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Order {
    /// The order document's id.
    pub id: String,
    pub number: String,
    pub kind: OrderKind,
    pub status: CustomerOrderStatus,
    pub customer_id: String,
    pub customer_name: String,
    pub due_on: Option<String>,
    pub total: Money,
    /// What its deposits hold (the sum of their ledger).
    pub held: Money,
    /// The active reservations.
    pub reserved: Vec<Reserved>,
    /// The ticket, delivery note or invoice that served it.
    pub fulfilled_document_id: Option<String>,
    pub fulfilled_number: Option<String>,
    pub lines: Vec<OrderLine>,
    pub events: Vec<OrderEvent>,
    pub business_date: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct CustomerOrderFilter {
    pub status: Option<CustomerOrderStatus>,
    pub kind: Option<OrderKind>,
    pub customer_id: Option<String>,
    /// Matches the number or the customer's name.
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OrderSummary {
    pub id: String,
    pub number: String,
    pub kind: OrderKind,
    pub status: CustomerOrderStatus,
    pub customer_name: String,
    pub due_on: Option<String>,
    pub total: Money,
    pub held: Money,
    pub days_open: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FulfilInput {
    pub id: String,
    /// The ticket's tenders: a `voucher` naming the order's `CC…` number spends its deposit.
    pub payments: Vec<PaymentInput>,
    #[serde(default)]
    pub extras: SaleExtras,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Fulfilled {
    pub order: Order,
    pub sale: ValidatedSale,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeliverInput {
    pub id: String,
    /// `delivery_note` or `invoice`.
    pub target: DocType,
    /// Invoices only (M3): the tenders, a `credit` one for the part left on the account.
    #[serde(default)]
    pub payments: Vec<PaymentInput>,
}

fn amount_err() -> AppError {
    AppError::validation("amount", "amount_invalid")
}

/// The quantity of `product_id` reserved at `location_id` by orders not yet served or closed.
pub(crate) fn reserved(
    conn: &Connection,
    product_id: &str,
    location_id: &str,
) -> Result<Qty, AppError> {
    Ok(conn
        .prepare_cached(
            "SELECT COALESCE(SUM(qty), 0) FROM stock_reservations
             WHERE product_id = ?1 AND location_id = ?2 AND released_at IS NULL",
        )?
        .query_row(params![product_id, location_id], |r| r.get(0))?)
}

fn release(tx: &Transaction<'_>, ctx: &TillContext, order_id: &str) -> Result<(), AppError> {
    tx.execute(
        "UPDATE stock_reservations SET released_at = ?2
         WHERE owner_type = 'customer_order' AND owner_id = ?1 AND released_at IS NULL",
        params![order_id, ctx.now],
    )?;
    Ok(())
}

fn event(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    order_id: &str,
    status: CustomerOrderStatus,
    note: Option<&str>,
) -> Result<(), AppError> {
    tx.execute(
        "INSERT INTO order_events (id, document_id, status, note, created_at, user_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            new_id(),
            order_id,
            status,
            clean_opt(note, "note", 200)?,
            ctx.now,
            ctx.session.user_id
        ],
    )?;
    Ok(())
}

fn status_of(conn: &Connection, id: &str) -> Result<(OrderKind, CustomerOrderStatus), AppError> {
    conn.query_row(
        "SELECT kind, status FROM customer_orders WHERE document_id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("order"))
}

/// The order still open or ready (`order_status_invalid` otherwise), with its kind.
fn live(conn: &Connection, id: &str) -> Result<OrderKind, AppError> {
    let (kind, status) = status_of(conn, id)?;
    if !status.live() {
        return Err(AppError::conflict("order_status_invalid"));
    }
    Ok(kind)
}

fn tender(d: &OrderDeposit, id: &str) -> DepositInput {
    DepositInput {
        owner_type: OwnerType::CustomerOrder,
        owner_id: id.to_owned(),
        amount: d.amount,
        method: d.method,
        reference: d.reference.clone(),
        cheque_bank: d.cheque_bank.clone(),
        cheque_date: d.cheque_date.clone(),
        note: None,
    }
}

/// Records an order: its draft (the editor's or a new one) priced like the till, the layaway's
/// minimum deposit, the document frozen and numbered `CC…`, its `customer_orders` row and first
/// event, the stock reserved, the deposit taken (Task 9). One `orders.created` row.
pub fn create(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: CustomerOrderInput,
) -> Result<Order, AppError> {
    packs::require(tx, Module::Orders)?;
    let customer_id = input
        .cart
        .customer_id
        .clone()
        .ok_or_else(|| AppError::validation("customerId", "customer_required"))?;
    let customer = customers::selectable(tx, &customer_id)?;
    let due_on = crate::party::clean_day(input.due_on.as_deref(), "dueOn")?;
    if due_on
        .as_deref()
        .is_some_and(|d| d < ctx.business_day().as_str())
    {
        return Err(AppError::validation("dueOn", "date_invalid"));
    }
    let draft = save_draft(
        tx,
        ctx,
        input.draft_id.as_deref(),
        DraftInput {
            doc_type: DocType::CustomerOrder,
            cart: input.cart.clone(),
            valid_until: None,
            due_date: None,
            second_lang: None,
            draft_id: None,
        },
    )?;
    if input.kind == OrderKind::Layaway {
        let min_bp: i64 = crate::settings::get(tx, "orders.layaway_min_deposit_bp")?;
        let min = draft
            .total_ttc
            .apply_rate(Rate(i32::try_from(min_bp).map_err(|_| amount_err())?))
            .ok_or_else(amount_err)?;
        if input.deposit.as_ref().map_or(Money::ZERO, |d| d.amount) < min {
            return Err(AppError::rejected(
                json!({ "minimum": min }),
                "layaway_deposit_too_low",
            ));
        }
    }
    validate::freeze(tx, ctx, &draft, Some(&customer), true)?;
    let document = documents::validate(tx, &draft.id, ctx.now)?;
    tx.execute(
        "INSERT INTO customer_orders (document_id, kind, due_on, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?4)",
        params![document.id, input.kind, due_on, ctx.now],
    )?;
    event(
        tx,
        ctx,
        &document.id,
        CustomerOrderStatus::Open,
        input.note.as_deref(),
    )?;
    if input.reserve || input.kind == OrderKind::Layaway {
        reserve(tx, ctx, &document)?;
    }
    if let Some(d) = &input.deposit {
        deposits::take(tx, ctx, tender(d, &document.id))?;
    }
    audit::append(
        tx,
        &ctx.actor().event(
            "orders.created",
            "document",
            Some(&document.id),
            json!({
                "number": document.number,
                "kind": input.kind,
                "total": document.total_ttc,
                "customerId": customer_id,
                "deposit": input.deposit.as_ref().map(|d| d.amount),
            }),
        ),
    )?;
    get(tx, &document.id)
}

/// Reserves the stock the order's lines take (the components of a kit) at the default location,
/// refused under the `block` policy when it is not there.
fn reserve(tx: &Transaction<'_>, ctx: &TillContext, doc: &Document) -> Result<(), AppError> {
    let location = stock::default_location(tx)?;
    let mut needs = Vec::new();
    for line in doc.lines.iter().filter(|l| l.parent_line_id.is_none()) {
        let Some(product_id) = &line.product_id else {
            continue;
        };
        let product = products::get(tx, product_id)?;
        for (component, qty) in stock::expand(tx, &product, line.qty)? {
            if component.track_stock && qty.is_positive() {
                needs.push(stock::StockNeed {
                    product_id: component.id,
                    location_id: location.id.clone(),
                    qty,
                });
            }
        }
    }
    stock::check_available(tx, &needs)?;
    for n in needs {
        tx.execute(
            "INSERT INTO stock_reservations (id, product_id, location_id, qty, owner_type, owner_id,
                                             created_at, user_id)
             VALUES (?1, ?2, ?3, ?4, 'customer_order', ?5, ?6, ?7)",
            params![
                new_id(),
                n.product_id,
                n.location_id,
                n.qty,
                doc.id,
                ctx.now,
                ctx.session.user_id
            ],
        )?;
    }
    Ok(())
}

/// A further deposit on an open or ready order (layaway payments over time). Journaled by
/// Task 9's `deposit.taken`; a host-only write.
pub fn pay_in(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    id: &str,
    d: OrderDeposit,
) -> Result<Deposit, AppError> {
    packs::require(tx, Module::Orders)?;
    live(tx, id)?;
    deposits::take(tx, ctx, tender(&d, id))
}

/// The goods are ready for the customer: `open` → `ready`. One `orders.ready` row.
pub fn set_ready(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    id: &str,
    note: Option<String>,
) -> Result<Order, AppError> {
    packs::require(tx, Module::Orders)?;
    let (_, status) = status_of(tx, id)?;
    if status != CustomerOrderStatus::Open {
        return Err(AppError::conflict("order_status_invalid"));
    }
    tx.execute(
        "UPDATE customer_orders SET status = 'ready', updated_at = ?2 WHERE document_id = ?1",
        params![id, ctx.now],
    )?;
    event(tx, ctx, id, CustomerOrderStatus::Ready, note.as_deref())?;
    audit::append(
        tx,
        &ctx.actor().event(
            "orders.ready",
            "document",
            Some(id),
            json!({ "note": note }),
        ),
    )?;
    get(tx, id)
}

/// The order's lines as a till cart at the ordered prices (a changed catalogue price needs
/// `sales.price_override`, as for any price set by hand).
pub fn fulfil_cart(conn: &Connection, id: &str) -> Result<Cart, AppError> {
    let doc = order_document(conn, id)?;
    let mut cart: Cart = match documents::stored_cart(conn, id)? {
        Some(v) => serde_json::from_value(v)?,
        None => crate::billing::drafts::cart_from_lines(conn, &doc)?,
    };
    let prices: Vec<Money> = doc
        .lines
        .iter()
        .filter(|l| l.parent_line_id.is_none())
        .map(|l| l.unit_price)
        .collect();
    if prices.len() != cart.lines.len() {
        return Err(AppError::Internal(format!(
            "order {id}: its cart and its lines differ"
        )));
    }
    for (line, price) in cart.lines.iter_mut().zip(prices) {
        line.unit_price = Some(price);
        line.expected_unit_price = Some(price);
    }
    cart.customer_id = doc.customer_id.clone();
    Ok(cart)
}

/// The order's number (`CC…`).
fn number_of(doc: &Document) -> Result<String, AppError> {
    doc.number
        .clone()
        .ok_or_else(|| AppError::Internal(format!("order {} has no number", doc.id)))
}

/// What the voucher tenders naming `number` spend.
fn deposit_tendered(payments: &[PaymentInput], number: &str) -> Result<Money, AppError> {
    Money::checked_sum(
        payments
            .iter()
            .filter(|p| p.method == PaymentMethod::Voucher)
            .filter(|p| {
                matches!(
                    vouchers::classify(p.reference.as_deref()),
                    VoucherTarget::Deposit { owner_type: OwnerType::CustomerOrder, number: n } if n == number
                )
            })
            .map(|p| p.amount),
    )
    .ok_or_else(|| AppError::validation("payments", "amount_invalid"))
}

/// Serves the order at the till: its reservations released, a ticket of its lines at the ordered
/// prices to its customer, the deposit spent by a `voucher` tender naming `CC…` — a layaway only
/// with nothing on credit (CRD-008) — and the order fulfilled. A served order never keeps money
/// held (`deposit_held`: refund the excess first). One `orders.fulfilled` row (the sale writes
/// its own).
pub fn fulfil(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: FulfilInput,
) -> Result<Fulfilled, AppError> {
    packs::require(tx, Module::Orders)?;
    let kind = live(tx, &input.id)?;
    let held = deposits::held(tx, OwnerType::CustomerOrder, &input.id)?;
    if kind == OrderKind::Layaway
        && input
            .payments
            .iter()
            .any(|p| p.method == PaymentMethod::Credit)
    {
        return Err(AppError::rejected(
            json!({ "held": held }),
            "layaway_not_paid",
        ));
    }
    let number = number_of(&order_document(tx, &input.id)?)?;
    if deposit_tendered(&input.payments, &number)? > held {
        return Err(AppError::rejected(
            json!({ "reference": number, "held": held }),
            "deposit_insufficient",
        ));
    }
    release(tx, ctx, &input.id)?;
    let cart = fulfil_cart(tx, &input.id)?;
    let sale = crate::sales::create_sale_with(
        tx,
        ctx,
        SaleInput {
            cart,
            payments: input.payments,
        },
        &input.extras,
    )?;
    let left = deposits::held(tx, OwnerType::CustomerOrder, &input.id)?;
    if left.is_positive() {
        return Err(AppError::rejected(json!({ "held": left }), "deposit_held"));
    }
    tx.execute(
        "UPDATE customer_orders SET status = 'fulfilled', fulfilled_document_id = ?2, updated_at = ?3
         WHERE document_id = ?1",
        params![input.id, sale.document.id, ctx.now],
    )?;
    event(tx, ctx, &input.id, CustomerOrderStatus::Fulfilled, None)?;
    audit::append(
        tx,
        &ctx.actor().event(
            "orders.fulfilled",
            "document",
            Some(&input.id),
            json!({
                "ticketId": sale.document.id,
                "ticketNumber": sale.document.number,
                "deposit": held,
            }),
        ),
    )?;
    Ok(Fulfilled {
        order: get(tx, &input.id)?,
        sale,
    })
}

/// Serves the order as a delivery note or an invoice of its lines (M3's validation: a BL takes no
/// tender, an invoice its tenders with a `credit` part on the account). The deposit held leaves
/// the order and goes on the customer's account as a payment against the new document's debt —
/// an invoice's; a BL carries no debt in M3, so there it waits as an advance its invoice takes
/// (M2 allocation). No cash moves: the deposit's cash and duty were counted when it was taken.
/// No tender may be a voucher naming a deposit (`payments.{i}.reference` `value_invalid`): the
/// deposit is spent only here, never by the validation. A layaway is delivered only paid in full
/// (`layaway_not_paid`): the `credit` part of its invoice no more than its deposits held (which
/// settle it), deposits covering its BL. One `orders.delivered` row (the validation writes its own).
pub fn deliver(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: DeliverInput,
) -> Result<ValidatedDocument, AppError> {
    packs::require(tx, Module::Orders)?;
    let kind = live(tx, &input.id)?;
    if !matches!(input.target, DocType::DeliveryNote | DocType::Invoice) {
        return Err(AppError::validation("target", "document_kind_invalid"));
    }
    // M3's invoice validation books no voucher: a tender naming a deposit (`CC…`, `RP…`) would
    // settle the invoice while spending nothing, and the deposit would be applied again below.
    // The deposit leaves the order only through `deposits::apply` and its ledger payment.
    if let Some(i) = input.payments.iter().position(|p| {
        p.method == PaymentMethod::Voucher
            && matches!(
                vouchers::classify(p.reference.as_deref()),
                VoucherTarget::Deposit { .. }
            )
    }) {
        return Err(AppError::validation(
            &format!("payments.{i}.reference"),
            "value_invalid",
        ));
    }
    let layaway_unpaid =
        |held: Money| AppError::rejected(json!({ "held": held }), "layaway_not_paid");
    // CRD-008 on an invoice: a layaway's credit part is only what its deposits settle.
    if kind == OrderKind::Layaway {
        let credit = Money::checked_sum(
            input
                .payments
                .iter()
                .filter(|p| p.method == PaymentMethod::Credit)
                .map(|p| p.amount),
        )
        .ok_or_else(|| AppError::validation("payments", "amount_invalid"))?;
        let held = deposits::held(tx, OwnerType::CustomerOrder, &input.id)?;
        if credit > held {
            return Err(layaway_unpaid(held));
        }
    }
    release(tx, ctx, &input.id)?;
    let order_doc = order_document(tx, &input.id)?;
    let customer_id = order_doc
        .customer_id
        .clone()
        .ok_or_else(|| AppError::validation("customerId", "customer_required"))?;
    let cart = fulfil_cart(tx, &input.id)?;
    let draft = save_draft(
        tx,
        ctx,
        None,
        DraftInput {
            doc_type: input.target,
            cart,
            valid_until: None,
            due_date: None,
            second_lang: order_doc.second_lang,
            draft_id: None,
        },
    )?;
    let done = validate::validate(
        tx,
        ctx,
        ValidateInput {
            id: draft.id,
            payments: input.payments,
        },
    )?;
    let held = deposits::held(tx, OwnerType::CustomerOrder, &input.id)?;
    // CRD-008: a layaway leaves the shop only paid in full — on a BL (no tender in M3) its
    // deposits must cover the note.
    if kind == OrderKind::Layaway
        && input.target == DocType::DeliveryNote
        && held < done.document.total_due
    {
        return Err(layaway_unpaid(held));
    }
    if held.is_positive() {
        deposits::apply(
            tx,
            ctx,
            OwnerType::CustomerOrder,
            &input.id,
            held,
            &done.document.id,
        )?;
        let debt = ledger::open_debits(tx, Side::Customer, &customer_id)?
            .into_iter()
            .find(|e| {
                e.ref_type.as_deref() == Some("document")
                    && e.ref_id.as_deref() == Some(done.document.id.as_str())
            });
        let mut entry = NewEntry::new(
            &customer_id,
            EntryKind::Payment,
            held.checked_neg().ok_or_else(amount_err)?,
        )
        .referencing("document", &input.id);
        entry.method = Some(PaymentMethod::Voucher);
        entry.reference = order_doc.number.clone();
        entry.allocate_to = debt.into_iter().map(|e| e.id).collect();
        ledger::append(
            tx,
            Side::Customer,
            &ctx.actor(),
            &entry,
            &ctx.business_day(),
            ctx.now,
        )?;
    }
    tx.execute(
        "UPDATE customer_orders SET status = 'fulfilled', fulfilled_document_id = ?2, updated_at = ?3
         WHERE document_id = ?1",
        params![input.id, done.document.id, ctx.now],
    )?;
    event(tx, ctx, &input.id, CustomerOrderStatus::Fulfilled, None)?;
    audit::append(
        tx,
        &ctx.actor().event(
            "orders.delivered",
            "document",
            Some(&input.id),
            json!({
                "documentId": done.document.id,
                "number": done.document.number,
                "deposit": held,
            }),
        ),
    )?;
    Ok(done)
}

/// Closes an order not served (called off): refused with `deposit_held` while a deposit is held
/// (refund it with `deposits::refund` or keep it with [`keep_deposit`] first); its reservations
/// are released. One `orders.closed` row.
pub fn close(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    id: &str,
    note: Option<String>,
) -> Result<Order, AppError> {
    packs::require(tx, Module::Orders)?;
    live(tx, id)?;
    let held = deposits::held(tx, OwnerType::CustomerOrder, id)?;
    if held.is_positive() {
        return Err(AppError::rejected(json!({ "held": held }), "deposit_held"));
    }
    release(tx, ctx, id)?;
    tx.execute(
        "UPDATE customer_orders SET status = 'closed', updated_at = ?2 WHERE document_id = ?1",
        params![id, ctx.now],
    )?;
    event(tx, ctx, id, CustomerOrderStatus::Closed, note.as_deref())?;
    audit::append(
        tx,
        &ctx.actor().event(
            "orders.closed",
            "document",
            Some(id),
            json!({ "note": note }),
        ),
    )?;
    get(tx, id)
}

/// The customer gave up and the shop keeps the arrhes: the deposit held is sold to the order's
/// customer as one "Arrhes conservées" line paid by the voucher `CC…` (kept arrhes always leave a
/// ticket, with its TVA), then the order is closed and its reservations released. One
/// `orders.closed` row (the sale writes its own).
pub fn keep_deposit(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    id: &str,
    note: Option<String>,
) -> Result<Fulfilled, AppError> {
    packs::require(tx, Module::Orders)?;
    live(tx, id)?;
    let held = deposits::held(tx, OwnerType::CustomerOrder, id)?;
    if !held.is_positive() {
        return Err(AppError::rejected(
            json!({ "held": held }),
            "deposit_insufficient",
        ));
    }
    release(tx, ctx, id)?;
    let doc = order_document(tx, id)?;
    let number = number_of(&doc)?;
    let product_id = kept_deposit_product(tx, &ctx.actor())?;
    let mut line = CartLine::product(&product_id, Qty(1_000));
    line.unit_price = Some(held);
    let mut cart = Cart::new(doc.lang, vec![line]);
    cart.customer_id = doc.customer_id.clone();
    let payments = vec![PaymentInput {
        method: PaymentMethod::Voucher,
        amount: held,
        reference: Some(number.clone()),
        cheque_bank: None,
        cheque_date: None,
    }];
    let sale = crate::sales::create_sale_with(
        tx,
        ctx,
        SaleInput { cart, payments },
        &SaleExtras::default(),
    )?;
    let note = clean_opt(note.as_deref(), "note", 200)?;
    tx.execute(
        "UPDATE customer_orders SET status = 'closed', updated_at = ?2 WHERE document_id = ?1",
        params![id, ctx.now],
    )?;
    event(tx, ctx, id, CustomerOrderStatus::Closed, note.as_deref())?;
    audit::append(
        tx,
        &ctx.actor().event(
            "orders.closed",
            "document",
            Some(id),
            json!({
                "note": note,
                "keptDeposit": held,
                "ticketId": sale.document.id,
                "ticketNumber": sale.document.number,
            }),
        ),
    )?;
    Ok(Fulfilled {
        order: get(tx, id)?,
        sale,
    })
}

/// The open-price service "Arrhes conservées" (TVA 19 % by default; the owner's accountant may
/// change its rate on the product): the one in `orders.kept_deposit_product_id` while it is still
/// sellable, else created (or found by its reference) and remembered there.
pub(crate) fn kept_deposit_product(
    tx: &Transaction<'_>,
    actor: &Actor,
) -> Result<String, AppError> {
    const CODE: &str = "KEPT_DEPOSIT";
    let current: Option<String> = crate::settings::get(tx, "orders.kept_deposit_product_id")?;
    if let Some(id) = current {
        match products::get(tx, &id) {
            Ok(p) if !p.archived => return Ok(p.id),
            Ok(_) | Err(AppError::NotFound { .. }) => {}
            Err(e) => return Err(e),
        }
    }
    let id = match crate::packs::seeds::service_product_named(
        tx,
        actor,
        CODE,
        ("عربون محتفظ به", "Arrhes conservées", "Kept deposit"),
        crate::catalog::UNIT_PIECE_ID,
        "TVA19",
        Money::ZERO,
        true,
    )? {
        Some(id) => id,
        None => tx.query_row(
            "SELECT id FROM products WHERE reference = ?1",
            [CODE],
            |r| r.get(0),
        )?,
    };
    crate::settings::set_quiet(tx, "orders.kept_deposit_product_id", json!(id))?;
    Ok(id)
}

/// The validated `customer_order` document behind an order (`order_not_found` otherwise).
fn order_document(conn: &Connection, id: &str) -> Result<Document, AppError> {
    let doc = documents::get(conn, id).map_err(|e| match e {
        AppError::NotFound { .. } => AppError::not_found("order"),
        other => other,
    })?;
    if doc.doc_type != DocType::CustomerOrder || doc.status != DocStatus::Validated {
        return Err(AppError::not_found("order"));
    }
    Ok(doc)
}

/// An order with its lines, deposits held, active reservations, serving document and history.
pub fn get(conn: &Connection, id: &str) -> Result<Order, AppError> {
    let doc = order_document(conn, id)?;
    let (kind, status, due_on, fulfilled_document_id, created_at, fulfilled_number): (
        OrderKind,
        CustomerOrderStatus,
        Option<String>,
        Option<String>,
        i64,
        Option<String>,
    ) = conn
        .query_row(
            "SELECT o.kind, o.status, o.due_on, o.fulfilled_document_id, o.created_at, f.number
             FROM customer_orders o LEFT JOIN documents f ON f.id = o.fulfilled_document_id
             WHERE o.document_id = ?1",
            [id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("order"))?;
    let customer_id = doc
        .customer_id
        .clone()
        .ok_or_else(|| AppError::Internal(format!("order {id} has no customer")))?;
    let customer_name = customers::get(conn, &customer_id)?.name;
    let mut stmt = conn.prepare_cached(
        "SELECT r.product_id, p.name_fr, SUM(r.qty)
         FROM stock_reservations r JOIN products p ON p.id = r.product_id
         WHERE r.owner_type = 'customer_order' AND r.owner_id = ?1 AND r.released_at IS NULL
         GROUP BY r.product_id ORDER BY MIN(r.rowid)",
    )?;
    let reserved = stmt
        .query_map([id], |r| {
            Ok(Reserved {
                product_id: r.get(0)?,
                name: r.get(1)?,
                qty: r.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut stmt = conn.prepare_cached(
        "SELECT e.status, e.note, e.created_at, COALESCE(u.display_name, '')
         FROM order_events e LEFT JOIN users u ON u.id = e.user_id
         WHERE e.document_id = ?1 ORDER BY e.created_at, e.rowid",
    )?;
    let events = stmt
        .query_map([id], |r| {
            Ok(OrderEvent {
                status: r.get(0)?,
                note: r.get(1)?,
                created_at: r.get(2)?,
                user_name: r.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Order {
        number: number_of(&doc)?,
        held: deposits::held(conn, OwnerType::CustomerOrder, id)?,
        lines: doc
            .lines
            .iter()
            .filter(|l| l.parent_line_id.is_none())
            .map(|l| OrderLine {
                product_id: l.product_id.clone(),
                description: l.description.clone(),
                qty: l.qty,
                unit_price: l.unit_price,
                total_ttc: l.total_ttc,
            })
            .collect(),
        id: doc.id,
        kind,
        status,
        customer_id,
        customer_name,
        due_on,
        total: doc.total_ttc,
        reserved,
        fulfilled_document_id,
        fulfilled_number,
        events,
        business_date: doc.business_date,
        created_at,
    })
}

/// The orders screen: by status, kind, customer and a text (number or customer name), newest
/// first, at most 500; the days open are counted to today.
pub fn list(
    conn: &Connection,
    filter: &CustomerOrderFilter,
) -> Result<Vec<OrderSummary>, AppError> {
    let today = crate::time::business_date(crate::time::now_ms());
    let text = filter.text.trim();
    let mut stmt = conn.prepare_cached(
        "SELECT d.id, d.number, o.kind, o.status, COALESCE(c.name, ''), o.due_on, d.total_ttc,
                (SELECT COALESCE(SUM(x.amount), 0) FROM deposits x
                 WHERE x.owner_type = 'customer_order' AND x.owner_id = d.id),
                o.created_at
         FROM customer_orders o
         JOIN documents d ON d.id = o.document_id
         LEFT JOIN customers c ON c.id = d.customer_id
         WHERE (?1 IS NULL OR o.status = ?1) AND (?2 IS NULL OR o.kind = ?2)
           AND (?3 IS NULL OR d.customer_id = ?3)
           AND (?4 = '' OR d.number LIKE '%' || ?4 || '%' OR c.name LIKE '%' || ?4 || '%')
         ORDER BY o.created_at DESC, d.number DESC
         LIMIT 500",
    )?;
    let rows = stmt
        .query_map(
            params![filter.status, filter.kind, filter.customer_id, text],
            |r| {
                let created_at: i64 = r.get(8)?;
                Ok(OrderSummary {
                    id: r.get(0)?,
                    number: r.get(1)?,
                    kind: r.get(2)?,
                    status: r.get(3)?,
                    customer_name: r.get(4)?,
                    due_on: r.get(5)?,
                    total: r.get(6)?,
                    held: r.get(7)?,
                    days_open: (today - crate::time::business_date(created_at)).num_days(),
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

//! The one place where the vertical packs join the sale transaction (spec §7.1 stays the M1a
//! flow): `create_sale_with` runs it after the ticket is validated and before the sale's audit
//! row, inside the same transaction — a pack that refuses rolls the whole sale back, number
//! included. Each pack adds its field to `SaleExtras` (all optional, default empty) and its call
//! here. Returns join the same way through `ReturnExtras` and `after_return`.
//!
//! The controlled-medicine and jeweller registers are the main PC's (`HostOnly`). A
//! till's sale checks and writes down its entries ([`HostRegisters`], kept in this PC's
//! `app_meta` in the sale's transaction) and the main PC numbers and keeps them once the till's
//! sale has reached it ([`record_host_registers`], an M5 host service); an offline till refuses
//! such a sale up front ([`needs_host_registers`]). The same goes for the main PC's ledgers a
//! sale moves (Task 31): a till checks the serials it sells, its top-ups
//! against the float and its voucher tenders against their balances, counting what its sales
//! still waiting for the main PC claim, and the main PC links the serials, draws the float and
//! spends the deposits and vouchers against the till's ticket.

use super::SaleInput;
use crate::documents::Document;
use crate::error::AppError;
use crate::packs::{self, Module};
use crate::till::TillContext;
use rusqlite::{Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct SaleExtras {
    /// Pharmacy (Task 6): one register entry per controlled line.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[specta(optional)]
    pub prescriptions: Vec<crate::pharmacy::register::PrescriptionInput>,
    /// Phones (Task 8): the serials or IMEIs sold, per cart line.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[specta(optional)]
    pub serials: Vec<SaleSerial>,
    /// The stock location the goods leave (a van on its round, VRT-101); `None` = the default.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub location_id: Option<String>,
    /// Jewellery (Task 14): the buyer written in the jeweller register.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub buyer: Option<crate::jewellery::BuyerIdentity>,
    /// Custom payment types (Task 16): the tenders taken as a type riding their base method.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[specta(optional)]
    pub payment_types: Vec<crate::payment_types::TenderType>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SaleSerial {
    /// The cart line (0-based).
    pub line: u32,
    pub serial_no: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReturnSerial {
    /// The return input line (0-based).
    pub line: u32,
    pub serial_no: String,
}

/// What the packs add to a return (serials coming back, Task 8; the exchange voucher, Task 13).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct ReturnExtras {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[specta(optional)]
    pub serials: Vec<ReturnSerial>,
    /// A `voucher` refund without a customer becomes an exchange voucher (RET-007) instead of
    /// M2's store credit, which needs one.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[specta(optional)]
    pub exchange_voucher: bool,
}

/// What the packs' part of a return produced.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReturnOutcome {
    /// The exchange voucher of the refund (`ReturnExtras::exchange_voucher`).
    pub exchange_voucher: Option<crate::gift_vouchers::Voucher>,
}

/// The legal registers' entries of a sale made on a till, for the main PC to number
/// and keep: the till's sale id, its lines, its business day, user and terminal.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostRegisters {
    pub document_id: String,
    #[serde(default)]
    pub controlled: Vec<crate::pharmacy::register::ControlledRow>,
    #[serde(default)]
    pub jewels: Vec<crate::jewellery::JewelRow>,
    /// The serials the sale names, linked to its lines by the main PC.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub serials: Vec<crate::stock::serials::SerialLink>,
    /// The sale draws top-ups on a float or spends deposits or vouchers: the main PC books them
    /// on its ledgers from the ticket.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ledgers: bool,
    /// On the till: the code of the main PC's last refusal of these entries (a voucher or a float
    /// spent meanwhile, a serial sold elsewhere…). They stay here for the owner and go again with
    /// each sending; they hold back none of the till's later entries (Task 31 review).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refused: Option<String>,
}

impl HostRegisters {
    pub fn is_empty(&self) -> bool {
        self.controlled.is_empty()
            && self.jewels.is_empty()
            && self.serials.is_empty()
            && !self.ledgers
    }
}

/// `app_meta` key prefix of a till's register entries not yet kept by the main PC.
const PENDING_PREFIX: &str = "host_registers:";

/// Every pack's part of the sale. `main_line_ids[i]` is the document line of cart line `i`.
pub(crate) fn after_sale(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: &SaleInput,
    extras: &SaleExtras,
    document: &Document,
    main_line_ids: &[String],
) -> Result<(), AppError> {
    // On a till the registers' entries are written down for the main PC.
    let on_till = crate::sync::role::is_client(tx)?;
    let mut registers = HostRegisters {
        document_id: document.id.clone(),
        ..HostRegisters::default()
    };
    let waiting = if on_till {
        waiting_claims(tx)?
    } else {
        WaitingClaims::default()
    };
    if on_till {
        registers.ledgers |=
            crate::vouchers::check(tx, ctx, document, &input.payments, &waiting.tickets)?;
    } else {
        crate::vouchers::book(tx, ctx, document, &input.payments)?;
    }
    crate::loyalty::earn(tx, ctx, document)?;
    registers.controlled =
        crate::pharmacy::register::plan(tx, ctx, document, main_line_ids, &extras.prescriptions)?;
    if !on_till {
        crate::pharmacy::register::write(tx, ctx, &document.id, &registers.controlled)?;
    }
    registers.serials = super::serials::capture(
        tx,
        ctx,
        input,
        &extras.serials,
        document,
        main_line_ids,
        on_till.then_some(waiting.serials.as_slice()),
    )?;
    registers.jewels = crate::jewellery::plan_register(
        tx,
        ctx,
        input,
        extras.buyer.as_ref(),
        document,
        main_line_ids,
    )?;
    if !on_till {
        crate::jewellery::write_register(tx, &registers.jewels)?;
    }
    if on_till {
        registers.ledgers |=
            crate::flexy::check_sale(tx, document, main_line_ids, &waiting.tickets)?;
    } else {
        crate::flexy::record_sale(tx, ctx, document, main_line_ids)?;
    }
    crate::payment_types::record_uses(tx, document, &input.payments, &extras.payment_types)?;
    if on_till && !registers.is_empty() {
        crate::db::meta_set(
            tx,
            &format!("{PENDING_PREFIX}{}", document.id),
            &serde_json::to_string(&registers)?,
        )?;
    }
    Ok(())
}

/// What a till's tickets still waiting for the main PC claim on its ledgers (Task 31 review):
/// the till's copy shows none of it until the main PC's booking reaches it, so a sale checked
/// against the copy counts it, whether the call failed or the main PC refused it. A waiting
/// ticket whose booking already reached the copy (its answer was lost) is in the copy's balances
/// and is not counted twice.
#[derive(Default)]
struct WaitingClaims {
    /// The tickets whose top-ups, deposits and vouchers wait ([`HostRegisters::ledgers`]).
    tickets: Vec<Document>,
    /// The serials they sold.
    serials: Vec<crate::stock::serials::SerialLink>,
}

fn waiting_claims(conn: &Connection) -> Result<WaitingClaims, AppError> {
    let mut claims = WaitingClaims::default();
    for registers in pending_host_registers(conn)? {
        // Each part is counted until its own booking reached the copy (final review: a part
        // booked never hides another one still waiting).
        if registers.ledgers && !booked(conn, Part::Ledgers, &registers.document_id)? {
            claims
                .tickets
                .push(crate::documents::get(conn, &registers.document_id)?);
        }
        if !booked(conn, Part::Serials, &registers.document_id)? {
            claims.serials.extend(registers.serials);
        }
    }
    Ok(claims)
}

/// One part of a till's ticket the main PC books on its own (final review): the two legal
/// registers, the serials linked to its lines, and its ledgers (float, deposits, vouchers).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Controlled,
    Jewels,
    Serials,
    Ledgers,
}

/// Whether this PC holds the `part` of the ticket `document_id`: on the main PC it was booked; on
/// a till the main PC's booking reached the copy. Each part is checked on its own, so a part
/// booked earlier never makes the call skip (or drop) another.
fn booked(conn: &Connection, part: Part, document_id: &str) -> Result<bool, AppError> {
    let sql = match part {
        Part::Controlled => "SELECT EXISTS (SELECT 1 FROM controlled_register WHERE document_id = ?1)",
        Part::Jewels => "SELECT EXISTS (SELECT 1 FROM jeweller_register WHERE document_id = ?1)",
        Part::Serials => {
            "SELECT EXISTS (SELECT 1 FROM serials s JOIN document_lines l ON l.id = s.sale_line_id
                            WHERE l.document_id = ?1)"
        }
        Part::Ledgers => {
            "SELECT EXISTS (SELECT 1 FROM flexy_float WHERE document_id = ?1 AND kind = 'sale')
                 OR EXISTS (SELECT 1 FROM gift_voucher_ledger WHERE document_id = ?1 AND kind = 'redeem')
                 OR EXISTS (SELECT 1 FROM deposits WHERE document_id = ?1 AND kind = 'applied')"
        }
    };
    Ok(conn.query_row(sql, [document_id], |r| r.get(0))?)
}

/// Whether selling `product_ids` writes a legal register entry (a controlled medicine
/// with the pharmacy module on, a precious-metal piece with the jewellery module on) or moves a
/// ledger of the main PC (Task 31: a serial-tracked product with the serials module on, a top-up
/// with the flexy module on). An offline till refuses such a sale up front (`host_required`): the
/// registers and those ledgers are the main PC's. (An offline till refuses a voucher tender as
/// it refuses kridi, `kridi_needs_host`.)
pub fn needs_host_registers<'a>(
    conn: &Connection,
    product_ids: impl IntoIterator<Item = &'a str>,
) -> Result<bool, AppError> {
    let pharmacy = packs::enabled(conn, Module::Pharmacy)?;
    let jewellery = packs::enabled(conn, Module::Jewellery)?;
    let serials = packs::enabled(conn, Module::Serials)?;
    let flexy = packs::enabled(conn, Module::Flexy)?;
    if !pharmacy && !jewellery && !serials && !flexy {
        return Ok(false);
    }
    for id in product_ids {
        if (pharmacy && crate::pharmacy::register::controlled(conn, id)?)
            || (jewellery && crate::jewellery::is_piece(conn, id)?)
            || (serials && crate::catalog::extras::get(conn, id)?.track_serials)
            || (flexy && crate::flexy::is_top_up(conn, id)?)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The register entries this till's sales wrote down and the main PC has not kept yet, oldest
/// sale first.
pub fn pending_host_registers(conn: &Connection) -> Result<Vec<HostRegisters>, AppError> {
    let mut stmt = conn
        .prepare("SELECT value FROM app_meta WHERE substr(key, 1, length(?1)) = ?1 ORDER BY key")?;
    let rows = stmt
        .query_map([PENDING_PREFIX], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    rows.iter()
        .map(|json| Ok(serde_json::from_str(json)?))
        .collect()
}

/// What a till's ticket still waits to have booked on the main PC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PendingKind {
    /// Controlled-medicine register entries.
    Controlled,
    /// Jeweller register entries.
    Jewels,
    /// IMEIs or serial numbers to link.
    Serials,
    /// Top-ups, deposits or vouchers to book.
    Ledgers,
}

/// One of this till's tickets whose host entries wait for the main PC: its number,
/// day and what waits, never the entries themselves (no patient, prescriber or buyer data).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PendingHostEntry {
    pub document_id: String,
    pub number: Option<String>,
    pub business_date: String,
    pub kinds: Vec<PendingKind>,
    /// The code of the main PC's last refusal (`None`: not answered yet).
    pub refused: Option<String>,
}

impl PendingHostEntry {
    /// The entry holds legal-register rows (a controlled medicine, a jewel).
    pub fn holds_registers(&self) -> bool {
        self.kinds
            .iter()
            .any(|k| matches!(k, PendingKind::Controlled | PendingKind::Jewels))
    }
}

/// [`pending_host_registers`] as the owner sees them (Settings › Réseau).
pub fn pending_host_entries(conn: &Connection) -> Result<Vec<PendingHostEntry>, AppError> {
    pending_host_registers(conn)?
        .into_iter()
        .map(|r| {
            let (number, business_date) = conn
                .query_row(
                    "SELECT number, business_date FROM documents WHERE id = ?1",
                    [&r.document_id],
                    |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?
                .unwrap_or_default();
            let kinds = [
                (!r.controlled.is_empty(), PendingKind::Controlled),
                (!r.jewels.is_empty(), PendingKind::Jewels),
                (!r.serials.is_empty(), PendingKind::Serials),
                (r.ledgers, PendingKind::Ledgers),
            ]
            .into_iter()
            .filter_map(|(on, kind)| on.then_some(kind))
            .collect();
            Ok(PendingHostEntry {
                document_id: r.document_id,
                number,
                business_date,
                kinds,
                refused: r.refused,
            })
        })
        .collect()
}

/// The main PC kept the entries of the till's sale `document_id`: they leave this PC.
pub fn host_registers_kept(tx: &Transaction<'_>, document_id: &str) -> Result<(), AppError> {
    tx.execute(
        "DELETE FROM app_meta WHERE key = ?1",
        [format!("{PENDING_PREFIX}{document_id}")],
    )?;
    Ok(())
}

/// The main PC's answer for the entries of the till's sale `document_id` (final review): every
/// part kept, or the legal registers kept and the serials and ledgers refused with
/// [`HostBooking::ledgers_refused`]. The registers' entries (their health and identity data
/// included) leave this PC either way; a refused part stays, marked with the refusal.
pub fn host_registers_answered(
    tx: &Transaction<'_>,
    document_id: &str,
    booking: &HostBooking,
) -> Result<(), AppError> {
    let Some(code) = booking.ledgers_refused.as_deref() else {
        return host_registers_kept(tx, document_id);
    };
    let key = format!("{PENDING_PREFIX}{document_id}");
    let Some(json) = crate::db::meta_get(tx, &key)? else {
        return Ok(());
    };
    let mut registers: HostRegisters = serde_json::from_str(&json)?;
    registers.controlled.clear();
    registers.jewels.clear();
    registers.refused = Some(code.to_owned());
    if registers.is_empty() {
        return host_registers_kept(tx, document_id);
    }
    crate::db::meta_set(tx, &key, &serde_json::to_string(&registers)?)?;
    Ok(())
}

/// A refused entry is cleared of its mark to go again at once (Settings › Réseau's
/// « Réessayer »). Returns how many marks were cleared.
pub fn host_registers_retry(tx: &Transaction<'_>) -> Result<u32, AppError> {
    let mut cleared = 0;
    for mut registers in pending_host_registers(tx)? {
        if registers.refused.take().is_some() {
            crate::db::meta_set(
                tx,
                &format!("{PENDING_PREFIX}{}", registers.document_id),
                &serde_json::to_string(&registers)?,
            )?;
            cleared += 1;
        }
    }
    Ok(cleared)
}

/// The main PC refused the entries of the till's sale `document_id` with `code`: they stay here,
/// marked with the refusal ([`HostRegisters::refused`]), and go again with the next sending.
pub fn host_registers_refused(
    tx: &Transaction<'_>,
    document_id: &str,
    code: &str,
) -> Result<(), AppError> {
    let key = format!("{PENDING_PREFIX}{document_id}");
    let Some(json) = crate::db::meta_get(tx, &key)? else {
        return Ok(());
    };
    let mut registers: HostRegisters = serde_json::from_str(&json)?;
    registers.refused = Some(code.to_owned());
    crate::db::meta_set(tx, &key, &serde_json::to_string(&registers)?)?;
    Ok(())
}

/// What the main PC's booking of a till's ticket did (final review): the legal registers'
/// entries are written whatever the ledgers say (they depend on no balance); the serials and the
/// ledgers go together in a savepoint and, refused, roll back alone.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HostBooking {
    /// The controlled-medicine and jeweller register entries written by this call.
    pub registers_written: u32,
    /// The serials linked to the ticket's lines by this call.
    pub serials_written: u32,
    /// The ticket's top-ups, deposits and vouchers were booked by this call.
    pub ledgers_booked: bool,
    /// The code of the main PC's refusal of the serials and ledgers (a voucher expired or spent
    /// meanwhile, a float short, a serial sold elsewhere…): nothing of them was written, the
    /// registers' entries were.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ledgers_refused: Option<String>,
}

impl HostBooking {
    /// Entries and serials written, plus one for the ledgers booked.
    pub fn written(&self) -> u32 {
        self.registers_written
            .saturating_add(self.serials_written)
            .saturating_add(u32::from(self.ledgers_booked))
    }
}

/// On the main PC: numbers and writes the register entries of a till's sale, links its
/// serials and books its top-ups, deposits and vouchers (Task 31), sent by the user who made it.
/// The sale is in this PC's copy (the till's outbox goes before the call); every entry names one
/// of its lines and product. Sent again after a lost answer, it writes nothing twice: each part
/// (each register, the serials, the ledgers) is checked on its own (final review).
///
/// The registers' entries are written first and unconditionally; the serials and the ledgers run
/// in a savepoint and a refusal of theirs rolls back that savepoint only
/// ([`HostBooking::ledgers_refused`]), so a voucher spent elsewhere never keeps a controlled
/// medicine or a jewel out of its register. The ledgers are booked as of the ticket (its business
/// day, terminal and validation time, [`TillContext::at_ticket`]); one `registers.host_booked`
/// audit row journals what was written, as this PC's user and terminal.
pub fn record_host_registers(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    registers: &HostRegisters,
) -> Result<HostBooking, AppError> {
    let document = crate::documents::get(tx, &registers.document_id)?;
    let line_of = |line_id: &str, product_id: &str| {
        document
            .lines
            .iter()
            .any(|l| l.id == line_id && l.product_id.as_deref() == Some(product_id))
    };
    // The till checked the entries (and the seller's `pharmacy.register`) at the sale; here each
    // entry names a line of that sale, sold by the user who sends it.
    let seller = ctx.session.user_id.as_str();
    let names_its_sale = registers.controlled.iter().all(|r| {
        r.document_id == document.id && r.user_id == seller && line_of(&r.line_id, &r.product_id)
    }) && registers.jewels.iter().all(|r| {
        r.document_id == document.id && r.user_id == seller && line_of(&r.line_id, &r.product_id)
    });
    // The serials and ledgers carry no seller of their own: the ticket's is the one who sends.
    let sold_by_sender =
        (registers.serials.is_empty() && !registers.ledgers) || document.user_id == seller;
    if !names_its_sale || !sold_by_sender {
        return Err(AppError::validation("registers", "value_invalid"));
    }
    let mut booking = HostBooking::default();
    let (mut controlled, mut jewels) = (0, 0);
    if !registers.controlled.is_empty() && !booked(tx, Part::Controlled, &document.id)? {
        packs::require(tx, Module::Pharmacy)?;
        controlled = crate::pharmacy::register::write_rows(tx, &registers.controlled)?;
    }
    if !registers.jewels.is_empty() && !booked(tx, Part::Jewels, &document.id)? {
        packs::require(tx, Module::Jewellery)?;
        jewels = crate::jewellery::write_register(tx, &registers.jewels)?;
    }
    booking.registers_written = controlled.saturating_add(jewels);
    let serials = !registers.serials.is_empty() && !booked(tx, Part::Serials, &document.id)?;
    let ledgers = registers.ledgers && !booked(tx, Part::Ledgers, &document.id)?;
    if serials || ledgers {
        tx.execute_batch("SAVEPOINT host_ledgers")?;
        match book_ledgers(tx, ctx, &document, registers, serials, ledgers) {
            Ok(linked) => {
                tx.execute_batch("RELEASE host_ledgers")?;
                booking.serials_written = linked;
                booking.ledgers_booked = ledgers;
            }
            Err(error) if refuses(&error) => {
                tx.execute_batch("ROLLBACK TO host_ledgers; RELEASE host_ledgers")?;
                booking.ledgers_refused = Some(error.code());
            }
            Err(error) => return Err(error),
        }
    }
    if booking.written() > 0 {
        crate::audit::append(
            tx,
            &ctx.actor().event(
                "registers.host_booked",
                "document",
                Some(&document.id),
                serde_json::json!({
                    "controlled": controlled,
                    "jewels": jewels,
                    "serials": booking.serials_written,
                    "ledgers": booking.ledgers_booked,
                    "ledgersRefused": booking.ledgers_refused,
                    "ticketTerminalId": document.terminal_id,
                    "approvals": ctx.approvers(),
                }),
            ),
        )?;
    }
    Ok(booking)
}

/// A refusal of the main PC's books (a balance, a status, a module, a value): the savepoint's
/// part waits on the till. A database or link failure is no refusal: the whole call fails and the
/// till sends it again.
fn refuses(error: &AppError) -> bool {
    matches!(
        error,
        AppError::Validation { .. }
            | AppError::Rejected { .. }
            | AppError::Conflict { .. }
            | AppError::NotFound { .. }
            | AppError::Permission { .. }
    )
}

/// The serials and the ledgers of a till's ticket, as of the ticket (final review): its business
/// day, terminal and time on the float, deposit and voucher rows, and a voucher's expiry checked
/// against the ticket's day, not the booking's. Returns the serials linked.
fn book_ledgers(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    document: &Document,
    registers: &HostRegisters,
    serials: bool,
    ledgers: bool,
) -> Result<u32, AppError> {
    let mut linked = 0;
    if serials {
        let done = crate::stock::serials::link_sale(
            tx,
            &ctx.actor(),
            &document.id,
            registers.serials.clone(),
        )?;
        linked = u32::try_from(done.len()).unwrap_or(u32::MAX);
    }
    if ledgers {
        let at_ticket = ctx.at_ticket(document)?;
        let tenders: Vec<crate::payments::PaymentInput> = document
            .payments
            .iter()
            .filter(|p| p.method == crate::payments::PaymentMethod::Voucher)
            .map(|p| crate::payments::PaymentInput {
                method: p.method,
                amount: p.amount,
                reference: p.reference.clone(),
                cheque_bank: None,
                cheque_date: None,
            })
            .collect();
        crate::vouchers::book(tx, &at_ticket, document, &tenders)?;
        let lines: Vec<String> = document.lines.iter().map(|l| l.id.clone()).collect();
        crate::flexy::record_sale(tx, &at_ticket, document, &lines)?;
    }
    Ok(linked)
}

/// Every pack's part of a return, inside its transaction (before the return's audit row).
pub(crate) fn after_return(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: &crate::returns::ReturnInput,
    extras: &ReturnExtras,
    returned: &crate::returns::ValidatedReturn,
) -> Result<ReturnOutcome, AppError> {
    crate::flexy::refuse_return(tx, input)?;
    super::serials::bring_back(tx, ctx, input, &extras.serials)?;
    if let Some(original) = &input.original_id {
        crate::loyalty::reverse(tx, ctx, original, returned.refund, &returned.document.id)?;
    }
    let exchange_voucher = if wants_exchange_voucher(input, extras, returned) {
        Some(crate::gift_vouchers::issue(
            tx,
            ctx,
            crate::gift_vouchers::VoucherKind::Exchange,
            returned.refund,
            None,
            Some(&returned.document.id),
            "return",
        )?)
    } else {
        None
    };
    Ok(ReturnOutcome { exchange_voucher })
}

/// A `voucher` refund of a return without customer, asked as an exchange voucher.
fn wants_exchange_voucher(
    input: &crate::returns::ReturnInput,
    extras: &ReturnExtras,
    returned: &crate::returns::ValidatedReturn,
) -> bool {
    extras.exchange_voucher
        && input.refund_method == crate::payments::PaymentMethod::Voucher
        && returned.document.customer_id.is_none()
        && returned.refund.is_positive()
}

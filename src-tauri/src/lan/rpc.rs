//! Host services a till may call when connected (spec §4.6: catalogue edits and documents need
//! the main PC). Each one is an existing command body run on the main PC as the till's user:
//! the session is rebuilt from the main PC's users and roles, with the main PC's terminal (what
//! it creates belongs to the main PC), so every permission, limit and redaction applies as if
//! the user were sitting there. M5 adds the writes of a till's label print (the barcodes
//! it assigns, the reprint queue) and the IMEIs a till sells: the till prints and sells, the
//! main PC records. M8 adds every write of the M8 packs that touches a balance
//! (flexy float, deposits, instalment plans) or a shop-wide series (`AC…`, `RP…`, `CT…`, `CC…`,
//! `TR…`) and the shared catalogue and books (customer prices, packs, appointments, delivery
//! rounds, whose rows never reach a till): a till's call runs here as its user; the till prints
//! the slips and receipts itself. Task 20 review: the money of a till's call is in the till's
//! drawer, so it is booked on the till's open session and recorded by the till
//! (`TillContext::drawer_till`); the manager approvals given at the till come with the call and
//! count once this PC has confirmed each approver (`pos_core::auth::confirm_grant`).

use crate::commands;
use crate::state::{AppState, Caller, with_till_caller};
use pos_core::error::AppError;
use pos_core::sync::answers;
use pos_server::host::{RpcCaller, RpcHandler};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::sync::Weak;

/// M8 also adds the pack writes whose balances or shop-wide numbers are the main PC's
/// (loyalty points, vouchers `BA…`, the gold board, salary advances, zakat statements `ZK…`) and
/// the reads of what a till never holds (the jeweller register, the zakat statements); Task 21b
/// the customers' consents and anonymisation (the customer books are the main PC's).
pub const SERVICES: [&str; 93] = [
    "products.create",
    "products.update",
    "products.set_price",
    "products.search",
    "customers.quick_create",
    "customers.update",
    "documents.invoice_ticket",
    "labels.assign_barcodes",
    "labels.record_printed",
    "serials.link_sale",
    "flexy.purchase",
    "deposits.take",
    "deposits.refund",
    "service.intake",
    "service.set_status",
    "service.assign",
    "service.set_items",
    "service.pickup",
    "instalments.sign",
    "instalments.note_reminder",
    "instalments.add_guarantee",
    "wholesale.price_set",
    "wholesale.min_order_set",
    "wholesale.pack_create",
    "orders.create",
    "orders.pay_in",
    "orders.set_ready",
    "orders.fulfil",
    "orders.deliver",
    "orders.close",
    "orders.keep_deposit",
    "appointments.book",
    "appointments.update",
    "appointments.set_status",
    "rounds.list",
    "rounds.get",
    "rounds.create",
    "rounds.set_stops",
    "rounds.start",
    "rounds.report_stop",
    "rounds.close_preview",
    "rounds.close",
    "rounds.create_van",
    "rounds.load",
    "rounds.unload",
    "loyalty.enrol",
    "loyalty.block",
    "loyalty.redeem",
    "loyalty.adjust",
    "vouchers.sell",
    "jewellery.piece_set",
    "jewellery.metal_price_set",
    "jewellery.reprice",
    "jewellery.register_entries",
    "staff.profile_set",
    "staff.advance",
    "staff.repay",
    "staff.deduct",
    "zakat.save",
    "zakat.list",
    "zakat.get",
    "privacy.set_consent",
    "privacy.anonymise",
    // M8: the packs' shared records, the module switches, the register kept on the
    // main PC only, and the yield runs numbered shop-wide (`DC…`).
    "packs.apply_template",
    "packs.set_modules",
    "packs.reset_modules",
    "packs.set_till_mode",
    "regulated.set",
    "regulated.seed",
    "departments.save",
    "departments.archive",
    "dining.room_save",
    "dining.room_archive",
    "dining.table_save",
    "dining.table_archive",
    "dining.menu_set",
    "modifiers.group_save",
    "modifiers.group_archive",
    "modifiers.set_product",
    "kitchen.station_save",
    "kitchen.station_archive",
    "pharmacy.product_set",
    "pharmacy.chifa_apply",
    "pharmacy.register_entries",
    "butcher.template_save",
    "butcher.template_archive",
    "butcher.yield_run",
    // The register entries of a till's sale (controlled medicines, jewels), kept here
    "registers.record",
    // The reports of what a till never holds (the registers, the
    // delivery rounds)
    "reports.run",
    // A till's return of a serial, of points or as an exchange voucher
    "returns.quote",
    "returns.create",
    "returns.exchange",
    // final review: a till follows its main PC's regulatory data pack
    "datapack.active",
];

pub struct AppRpc {
    state: Weak<AppState>,
    /// The keyed calls running now (final review): the same key sent again meanwhile waits for
    /// its turn on the till.
    in_flight: std::sync::Mutex<std::collections::HashSet<String>>,
}

impl AppRpc {
    pub fn new(state: Weak<AppState>) -> AppRpc {
        AppRpc {
            state,
            in_flight: std::sync::Mutex::new(std::collections::HashSet::new()),
        }
    }
}

/// The claim of a keyed call while it runs: dropped, the key is free again.
struct InFlight<'a> {
    set: &'a std::sync::Mutex<std::collections::HashSet<String>>,
    key: String,
}

impl<'a> InFlight<'a> {
    /// `lan_offline` (the till keeps the call and sends it again) while the key runs.
    fn claim(
        set: &'a std::sync::Mutex<std::collections::HashSet<String>>,
        key: &str,
    ) -> Result<InFlight<'a>, AppError> {
        let mut running = set
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !running.insert(key.to_owned()) {
            return Err(AppError::sync("lan_offline"));
        }
        Ok(InFlight {
            set,
            key: key.to_owned(),
        })
    }
}

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        self.set
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.key);
    }
}

/// Keeps `output` for the till's `key` right after the call committed (`pos_core::sync::answers`).
/// A failure is logged: the answer still goes to the till.
fn keep_answer(st: &AppState, till: String, key: String, service: &str, output: &Value) {
    let (now, name, kept) = (st.now(), service.to_owned(), output.clone());
    let done = st
        .db
        .write(move |tx| answers::keep(tx, &till, &key, &name, &kept, now));
    if let Err(error) = done {
        tracing::warn!(service, %error, "the answer of a till's keyed call is not kept");
    }
}

fn args<T: DeserializeOwned>(input: Value) -> Result<T, AppError> {
    serde_json::from_value(input).map_err(|_| AppError::validation("input", "value_invalid"))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdInput<T> {
    id: String,
    input: T,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Only<T> {
    input: T,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetPrice {
    id: String,
    price_level_id: String,
    price: pos_core::money::Money,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuickCustomer {
    name: String,
    phone: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Search {
    query: pos_core::catalog::search::SearchQuery,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssignBarcodes {
    product_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecordPrinted {
    batch: pos_core::labels::LabelBatch,
    arabic_override: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LinkSale {
    document_id: String,
    links: Vec<pos_core::stock::serials::SerialLink>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdNote {
    id: String,
    note: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DepositRefund {
    owner_type: pos_core::deposits::OwnerType,
    owner_id: String,
    amount: pos_core::money::Money,
    method: pos_core::payments::PaymentMethod,
    reference: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServiceStatusSet {
    id: String,
    status: pos_core::service_tickets::ServiceStatus,
    note: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServiceAssign {
    id: String,
    technician_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServiceItems {
    id: String,
    items: Vec<pos_core::service_tickets::ServiceItemInput>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReminderNote {
    plan_id: String,
    seq: u8,
    note: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GuaranteeAdd {
    plan_id: String,
    guarantee: pos_core::instalments::GuaranteeInput,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CustomerPriceSet {
    customer_id: String,
    product_id: String,
    price: Option<pos_core::money::Money>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MinOrderSet {
    product_id: String,
    min_order_qty: Option<pos_core::money::Qty>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrderPayIn {
    id: String,
    deposit: pos_core::orders::OrderDeposit,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppointmentStatusSet {
    id: String,
    status: pos_core::appointments::AppointmentStatus,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdOnly {
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RoundList {
    filter: pos_core::wholesale::rounds::RoundFilter,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RoundStops {
    id: String,
    document_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RoundStop {
    id: String,
    report: pos_core::wholesale::rounds::StopReport,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VanCreate {
    code: String,
    names: commands::wholesale::VanNames,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RoundLoad {
    id: String,
    lines: Vec<pos_core::stock::transfers::TransferLine>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Nothing {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Card {
    code: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Enrol {
    customer_id: String,
    code: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Points {
    customer_id: String,
    points: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PointsAdjust {
    customer_id: String,
    points: i64,
    note: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PieceSet {
    product_id: String,
    input: pos_core::jewellery::PieceInput,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MetalPriceSet {
    metal: pos_core::jewellery::Metal,
    titre: u16,
    price_per_gram: pos_core::money::Money,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegisterPage {
    year: i32,
    from_seq: u32,
    limit: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileSet {
    user_id: String,
    input: pos_core::staff::ProfileInput,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Repay {
    user_id: String,
    amount: pos_core::money::Money,
    method: pos_core::payments::PaymentMethod,
    note: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConsentSet {
    customer_id: String,
    channel: pos_core::privacy::ConsentChannel,
    granted: bool,
    source: pos_core::privacy::ConsentSource,
    note: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Anonymise {
    customer_id: String,
    note: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Deduct {
    user_id: String,
    amount: pos_core::money::Money,
    month: String,
    note: Option<String>,
}

/// A record created (`id: null`) or changed.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Save<T> {
    id: Option<String>,
    input: T,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApplyTemplate {
    activity: pos_core::company::ShopActivity,
    seed: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetModules {
    modules: Vec<pos_core::packs::Module>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetTillMode {
    mode: pos_core::packs::TillMode,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetRegulated {
    product_id: String,
    code: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MenuSet {
    day: String,
    product_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProductGroups {
    product_id: String,
    group_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProductInput<T> {
    product_id: String,
    input: T,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChifaRows {
    file_name: String,
    rows: Vec<Vec<String>>,
    mapping: pos_core::pharmacy::chifa::ChifaMapping,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Period {
    from: String,
    to: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Registers {
    registers: pos_core::sales::extensions::HostRegisters,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReturnQuoteRun {
    input: pos_core::returns::ReturnInput,
    #[serde(default)]
    extras: pos_core::sales::extensions::ReturnExtras,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Request<T> {
    request: T,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReportRun {
    request: pos_core::reports::ReportRequest,
    export: bool,
}

fn out<T: serde::Serialize>(value: T) -> Result<Value, AppError> {
    Ok(serde_json::to_value(value)?)
}

/// Runs `service` on this PC as whoever is the current session of this thread.
pub fn dispatch(st: &AppState, service: &str, input: Value) -> Result<Value, AppError> {
    match service {
        "products.create" => {
            let a: Only<pos_core::catalog::products::ProductInput> = args(input)?;
            out(commands::products::create(st, a.input)?)
        }
        "products.update" => {
            let a: IdInput<pos_core::catalog::products::ProductInput> = args(input)?;
            out(commands::products::update(st, a.id, a.input)?)
        }
        "products.set_price" => {
            let a: SetPrice = args(input)?;
            out(commands::products::set_price(
                st,
                a.id,
                a.price_level_id,
                a.price,
            )?)
        }
        "products.search" => {
            let a: Search = args(input)?;
            out(commands::products::search(st, a.query)?)
        }
        "customers.quick_create" => {
            let a: QuickCustomer = args(input)?;
            out(commands::customers::quick_create(st, a.name, a.phone)?)
        }
        "customers.update" => {
            let a: IdInput<pos_core::customers::CustomerInput> = args(input)?;
            out(commands::customers::update(st, a.id, a.input)?)
        }
        "documents.invoice_ticket" => {
            let a: Only<pos_core::billing::convert::TicketInvoiceInput> = args(input)?;
            out(commands::billing::invoice_ticket(st, a.input)?)
        }
        "labels.assign_barcodes" => {
            let a: AssignBarcodes = args(input)?;
            out(commands::labels::assign_barcodes(st, a.product_ids)?)
        }
        "labels.record_printed" => {
            let a: RecordPrinted = args(input)?;
            out(commands::labels::record_printed(
                st,
                a.batch,
                a.arabic_override,
            )?)
        }
        "serials.link_sale" => {
            let a: LinkSale = args(input)?;
            out(commands::stock_m2::link_sale(st, a.document_id, a.links)?)
        }
        _ => dispatch_packs(st, service, input),
    }
}

/// The M8 pack services: each runs the command's write without printing (the
/// till prints its own slips and receipts from the answer).
fn dispatch_packs(st: &AppState, service: &str, input: Value) -> Result<Value, AppError> {
    use crate::commands::{instalments, orders, phones, services, wholesale};
    match service {
        "flexy.purchase" => {
            let a: Only<pos_core::flexy::FloatPurchase> = args(input)?;
            out(phones::purchase(st, a.input)?)
        }
        "deposits.take" => {
            let a: Only<pos_core::deposits::DepositInput> = args(input)?;
            out(services::take_here(st, a.input)?)
        }
        "deposits.refund" => {
            let a: DepositRefund = args(input)?;
            out(services::refund_here(
                st,
                a.owner_type,
                a.owner_id,
                a.amount,
                a.method,
                a.reference,
            )?)
        }
        "service.intake" => {
            let a: Only<pos_core::service_tickets::IntakeInput> = args(input)?;
            out(services::intake_here(st, a.input)?)
        }
        "service.set_status" => {
            let a: ServiceStatusSet = args(input)?;
            out(services::set_status(st, a.id, a.status, a.note)?)
        }
        "service.assign" => {
            let a: ServiceAssign = args(input)?;
            out(services::assign(st, a.id, a.technician_id)?)
        }
        "service.set_items" => {
            let a: ServiceItems = args(input)?;
            out(services::set_items(st, a.id, a.items)?)
        }
        "service.pickup" => {
            let a: Only<pos_core::service_tickets::PickupInput> = args(input)?;
            out(services::pickup_here(st, a.input)?)
        }
        "instalments.sign" => {
            let a: Only<pos_core::instalments::SignInput> = args(input)?;
            out(instalments::sign_here(st, a.input)?)
        }
        "instalments.note_reminder" => {
            let a: ReminderNote = args(input)?;
            out(instalments::note_reminder(st, a.plan_id, a.seq, a.note)?)
        }
        "instalments.add_guarantee" => {
            let a: GuaranteeAdd = args(input)?;
            out(instalments::add_guarantee(st, a.plan_id, a.guarantee)?)
        }
        "wholesale.price_set" => {
            let a: CustomerPriceSet = args(input)?;
            out(wholesale::price_set(
                st,
                a.customer_id,
                a.product_id,
                a.price,
            )?)
        }
        "wholesale.min_order_set" => {
            let a: MinOrderSet = args(input)?;
            out(wholesale::min_order_set(st, a.product_id, a.min_order_qty)?)
        }
        "wholesale.pack_create" => {
            let a: Only<pos_core::wholesale::PackInput> = args(input)?;
            out(wholesale::pack_create(st, a.input)?)
        }
        "orders.create" => {
            let a: Only<pos_core::orders::CustomerOrderInput> = args(input)?;
            out(orders::create_here(st, a.input)?)
        }
        "orders.pay_in" => {
            let a: OrderPayIn = args(input)?;
            out(orders::pay_in_here(st, a.id, a.deposit)?)
        }
        "orders.set_ready" => {
            let a: IdNote = args(input)?;
            out(orders::set_ready(st, a.id, a.note)?)
        }
        "orders.fulfil" => {
            let a: Only<pos_core::orders::FulfilInput> = args(input)?;
            out(orders::fulfil_here(st, a.input)?)
        }
        "orders.deliver" => {
            let a: Only<pos_core::orders::DeliverInput> = args(input)?;
            out(orders::deliver(st, a.input)?)
        }
        "orders.close" => {
            let a: IdNote = args(input)?;
            out(orders::close(st, a.id, a.note)?)
        }
        "orders.keep_deposit" => {
            let a: IdNote = args(input)?;
            out(orders::keep_here(st, a.id, a.note)?)
        }
        "appointments.book" => {
            let a: Only<pos_core::appointments::AppointmentInput> = args(input)?;
            out(orders::appointment_book(st, a.input)?)
        }
        "appointments.update" => {
            let a: IdInput<pos_core::appointments::AppointmentInput> = args(input)?;
            out(orders::appointment_update(st, a.id, a.input)?)
        }
        "appointments.set_status" => {
            let a: AppointmentStatusSet = args(input)?;
            out(orders::appointment_set_status(st, a.id, a.status)?)
        }
        _ => dispatch_rounds(st, service, input),
    }
}

/// The delivery rounds (`TR…`; Task 20 review): their rows live on this PC only, so a
/// till reads them here too; closing a round books its collections on the till's drawer.
fn dispatch_rounds(st: &AppState, service: &str, input: Value) -> Result<Value, AppError> {
    use crate::commands::wholesale;
    match service {
        "rounds.list" => {
            let a: RoundList = args(input)?;
            out(wholesale::rounds(st, a.filter)?)
        }
        "rounds.get" => {
            let a: IdOnly = args(input)?;
            out(wholesale::round_get(st, a.id)?)
        }
        "rounds.create" => {
            let a: Only<pos_core::wholesale::rounds::RoundInput> = args(input)?;
            out(wholesale::round_create(st, a.input)?)
        }
        "rounds.set_stops" => {
            let a: RoundStops = args(input)?;
            out(wholesale::round_set_stops(st, a.id, a.document_ids)?)
        }
        "rounds.start" => {
            let a: IdOnly = args(input)?;
            out(wholesale::round_start(st, a.id)?)
        }
        "rounds.report_stop" => {
            let a: RoundStop = args(input)?;
            out(wholesale::round_report_stop(st, a.id, a.report)?)
        }
        "rounds.close_preview" => {
            let a: IdOnly = args(input)?;
            out(wholesale::round_close_preview(st, a.id)?)
        }
        "rounds.close" => {
            let a: IdOnly = args(input)?;
            out(wholesale::round_close_here(st, a.id)?)
        }
        "rounds.create_van" => {
            let a: VanCreate = args(input)?;
            out(wholesale::van_create(st, a.code, a.names)?)
        }
        "rounds.load" => {
            let a: RoundLoad = args(input)?;
            out(wholesale::round_load(st, a.id, a.lines)?)
        }
        "rounds.unload" => {
            let a: IdOnly = args(input)?;
            out(wholesale::round_unload(st, a.id)?)
        }
        _ => dispatch_loyalty_staff(st, service, input),
    }
}

/// Loyalty and vouchers, jewellery, staff and zakat (Task 21): the write runs here; the slip of a
/// voucher prints on the till that asked (`redeem_here`, `sell_here` print nothing).
fn dispatch_loyalty_staff(st: &AppState, service: &str, input: Value) -> Result<Value, AppError> {
    use crate::commands::{jewellery, loyalty, staff};
    match service {
        "loyalty.enrol" => {
            let a: Enrol = args(input)?;
            out(loyalty::enrol(st, a.customer_id, a.code)?)
        }
        "loyalty.block" => {
            let a: Card = args(input)?;
            out(loyalty::block(st, a.code)?)
        }
        "loyalty.redeem" => {
            let a: Points = args(input)?;
            out(loyalty::redeem_here(st, a.customer_id, a.points)?)
        }
        "loyalty.adjust" => {
            let a: PointsAdjust = args(input)?;
            out(loyalty::adjust(st, a.customer_id, a.points, a.note)?)
        }
        "vouchers.sell" => {
            let a: Only<pos_core::gift_vouchers::SellInput> = args(input)?;
            out(loyalty::sell_here(st, a.input)?)
        }
        "jewellery.piece_set" => {
            let a: PieceSet = args(input)?;
            out(jewellery::piece_set(st, a.product_id, a.input)?)
        }
        "jewellery.metal_price_set" => {
            let a: MetalPriceSet = args(input)?;
            out(jewellery::set_metal_price(
                st,
                a.metal,
                a.titre,
                a.price_per_gram,
            )?)
        }
        "jewellery.reprice" => {
            let _: Nothing = args(input)?;
            out(jewellery::reprice(st)?)
        }
        "jewellery.register_entries" => {
            let a: RegisterPage = args(input)?;
            out(jewellery::register_entries(
                st, a.year, a.from_seq, a.limit,
            )?)
        }
        "staff.profile_set" => {
            let a: ProfileSet = args(input)?;
            out(staff::profile_set(st, a.user_id, a.input)?)
        }
        "staff.advance" => {
            let a: Only<pos_core::staff::AdvanceInput> = args(input)?;
            out(staff::advance_here(st, a.input)?)
        }
        "staff.repay" => {
            let a: Repay = args(input)?;
            out(staff::repay_here(
                st, a.user_id, a.amount, a.method, a.note,
            )?)
        }
        "staff.deduct" => {
            let a: Deduct = args(input)?;
            out(staff::deduct(st, a.user_id, a.amount, a.month, a.note)?)
        }
        "zakat.save" => {
            let a: Only<pos_core::zakat::ZakatInput> = args(input)?;
            out(staff::save_zakat(st, a.input)?)
        }
        "zakat.list" => {
            let _: Nothing = args(input)?;
            out(staff::list_zakat(st)?)
        }
        "zakat.get" => {
            let a: IdOnly = args(input)?;
            out(staff::get_zakat(st, a.id)?)
        }
        "privacy.set_consent" => {
            let a: ConsentSet = args(input)?;
            out(crate::commands::privacy::set_consent(
                st,
                a.customer_id,
                a.channel,
                a.granted,
                a.source,
                a.note,
            )?)
        }
        "privacy.anonymise" => {
            let a: Anonymise = args(input)?;
            out(crate::commands::privacy::anonymise(
                st,
                a.customer_id,
                a.note,
            )?)
        }
        _ => dispatch_activity(st, service, input),
    }
}

/// M8 services: the activity and modules, the superette, the restaurant's
/// records, the pharmacy and the butcher; and the register entries of a till's sale.
fn dispatch_activity(st: &AppState, service: &str, input: Value) -> Result<Value, AppError> {
    use commands::{butcher, packs, pharmacy, restaurant};
    match service {
        "packs.apply_template" => {
            let a: ApplyTemplate = args(input)?;
            out(packs::apply_template(st, a.activity, a.seed)?)
        }
        "packs.set_modules" => {
            let a: SetModules = args(input)?;
            out(packs::set_modules(st, a.modules)?)
        }
        "packs.reset_modules" => {
            let _: Nothing = args(input)?;
            out(packs::reset_modules(st)?)
        }
        "packs.set_till_mode" => {
            let a: SetTillMode = args(input)?;
            out(packs::set_till_mode(st, a.mode)?)
        }
        "regulated.set" => {
            let a: SetRegulated = args(input)?;
            out(packs::set_regulation(st, a.product_id, a.code)?)
        }
        "regulated.seed" => {
            let _: Nothing = args(input)?;
            out(packs::seed_regulated(st)?)
        }
        "departments.save" => {
            let a: Save<pos_core::catalog::departments::DepartmentKeyInput> = args(input)?;
            out(packs::save_department(st, a.id, a.input)?)
        }
        "departments.archive" => {
            let a: IdOnly = args(input)?;
            out(packs::archive_department(st, a.id)?)
        }
        "dining.room_save" => {
            let a: Save<pos_core::restaurant::floor::RoomInput> = args(input)?;
            out(restaurant::room_save(st, a.id, a.input)?)
        }
        "dining.room_archive" => {
            let a: IdOnly = args(input)?;
            out(restaurant::room_archive(st, a.id)?)
        }
        "dining.table_save" => {
            let a: Save<pos_core::restaurant::floor::TableInput> = args(input)?;
            out(restaurant::table_save(st, a.id, a.input)?)
        }
        "dining.table_archive" => {
            let a: IdOnly = args(input)?;
            out(restaurant::table_archive(st, a.id)?)
        }
        "dining.menu_set" => {
            let a: MenuSet = args(input)?;
            out(restaurant::menu_set(st, a.day, a.product_ids)?)
        }
        "modifiers.group_save" => {
            let a: Save<pos_core::restaurant::modifiers::ModifierGroupInput> = args(input)?;
            out(restaurant::group_save(st, a.id, a.input)?)
        }
        "modifiers.group_archive" => {
            let a: IdOnly = args(input)?;
            out(restaurant::group_archive(st, a.id)?)
        }
        "modifiers.set_product" => {
            let a: ProductGroups = args(input)?;
            out(restaurant::set_product_groups(
                st,
                a.product_id,
                a.group_ids,
            )?)
        }
        "kitchen.station_save" => {
            let a: Save<pos_core::restaurant::kitchen::StationInput> = args(input)?;
            out(restaurant::station_save(st, a.id, a.input)?)
        }
        "kitchen.station_archive" => {
            let a: IdOnly = args(input)?;
            out(restaurant::station_archive(st, a.id)?)
        }
        "pharmacy.product_set" => {
            let a: ProductInput<pos_core::pharmacy::PharmacyProductInput> = args(input)?;
            out(pharmacy::product_set(st, a.product_id, a.input)?)
        }
        "pharmacy.chifa_apply" => {
            let a: ChifaRows = args(input)?;
            out(pharmacy::apply_chifa_rows(
                st,
                a.file_name,
                a.rows,
                a.mapping,
            )?)
        }
        "pharmacy.register_entries" => {
            let a: Period = args(input)?;
            out(pharmacy::register_entries(st, a.from, a.to)?)
        }
        "butcher.template_save" => {
            let a: Save<pos_core::butcher::yields::YieldTemplateInput> = args(input)?;
            out(butcher::template_save(st, a.id, a.input)?)
        }
        "butcher.template_archive" => {
            let a: IdOnly = args(input)?;
            out(butcher::template_archive(st, a.id)?)
        }
        "butcher.yield_run" => {
            let a: Only<pos_core::butcher::yields::RunInput> = args(input)?;
            out(butcher::run(st, a.input)?)
        }
        "registers.record" => {
            let a: Registers = args(input)?;
            out(crate::lan::registers::record(st, a.registers)?)
        }
        "reports.run" => {
            let a: ReportRun = args(input)?;
            out(commands::reports::host_report(st, &a.request, a.export)?)
        }
        "datapack.active" => {
            let _: Nothing = args(input)?;
            out(crate::lan::datapack::active(st))
        }
        "returns.quote" => {
            let a: ReturnQuoteRun = args(input)?;
            out(commands::returns::quote_with(st, a.input, a.extras)?)
        }
        "returns.create" => {
            let a: Request<commands::returns::ReturnRequest> = args(input)?;
            out(commands::returns::create_here(st, a.request)?)
        }
        "returns.exchange" => {
            let a: Request<commands::returns::ExchangeRequest> = args(input)?;
            out(commands::returns::exchange_here(st, a.request)?)
        }
        _ => Err(AppError::not_found("rpc_service")),
    }
}

impl RpcHandler for AppRpc {
    fn call(&self, caller: RpcCaller, service: &str, input: Value) -> Result<Value, AppError> {
        let st = self
            .state
            .upgrade()
            .ok_or_else(|| AppError::Internal("the main PC is closing".into()))?;
        let (user, terminal, now) = (caller.user_id.clone(), st.terminal.clone(), st.now());
        let grants = caller.approvals.clone();
        let (session, approvals) = st.db.read(move |c| {
            let session = pos_core::auth::session::build_session(c, &user, &terminal, now)
                .map_err(|error| match error {
                    AppError::NotFound { .. } => AppError::auth("session_invalid"),
                    other => other,
                })?;
            // A user deactivated on the main PC is signed out everywhere.
            let session = pos_core::auth::session::refresh(c, &session)?;
            // The approvals given at the till count only from this PC's own managers.
            let approvals = grants
                .iter()
                .map(|g| {
                    pos_core::auth::confirm_grant(
                        c,
                        &session.user_id,
                        &g.approver_id,
                        g.permission,
                        g.granted_at,
                    )
                })
                .filter_map(Result::transpose)
                .collect::<Result<Vec<_>, _>>()?;
            Ok((session, approvals))
        })?;
        tracing::info!(
            service,
            till = %caller.terminal_code,
            approvals = approvals.len(),
            "host service for a till"
        );
        tracing::debug!(service, input = %crate::commands::redact::log_line(&input), "host service input");
        let key = caller.idempotency_key.clone();
        let till = caller.terminal_id.clone();
        let caller = Caller {
            session,
            drawer_till: Some(caller.terminal_id),
            approvals,
        };
        // Final review: a keyed call (a till's write that moves money) answered before is
        // answered again from what it answered then; nothing is written twice.
        let Some(key) = key else {
            return with_till_caller(caller, || dispatch(&st, service, input));
        };
        if !answers::key_is_valid(&key) {
            return Err(AppError::validation("idempotencyKey", "value_invalid"));
        }
        let _running = InFlight::claim(&self.in_flight, &format!("{till}:{key}"))?;
        let (read_till, read_key) = (till.clone(), key.clone());
        if let Some(output) = st
            .db
            .read(move |c| answers::kept(c, &read_till, &read_key))?
        {
            tracing::info!(service, "a till's call answered again from the kept answer");
            return Ok(output);
        }
        let output = with_till_caller(caller, || dispatch(&st, service, input))?;
        keep_answer(&st, till, key, service, &output);
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::testshop::{CASHIER_PIN, shop};
    use pos_server::host::{RpcCaller, RpcHandler};
    use serde_json::json;

    #[test]
    fn a_host_service_runs_as_the_till_user_with_the_main_pcs_permissions() {
        let shop = shop();
        let milk = shop.product("Lait", 10_000, 900, None).id;
        let cashier = shop.user(
            "Yacine",
            pos_core::auth::CASHIER_ROLE_ID,
            CASHIER_PIN,
            1_000,
        );
        let rpc = AppRpc::new(std::sync::Arc::downgrade(&shop.st));
        let caller = |user: &str| RpcCaller {
            terminal_id: "00000000-0000-7000-8000-0000000000c2".into(),
            terminal_code: "C2".into(),
            user_id: user.into(),
            approvals: Vec::new(),
            idempotency_key: None,
        };
        let input = json!({ "id": milk, "priceLevelId": pos_core::catalog::RETAIL_LEVEL_ID, "price": 12_000 });
        let refused = rpc
            .call(caller(&cashier), "products.set_price", input.clone())
            .unwrap_err();
        assert_eq!(
            refused.code(),
            "insufficient_permission",
            "the main PC's roles decide"
        );
        let done = rpc
            .call(caller(&shop.owner.user_id), "products.set_price", input)
            .unwrap();
        assert_eq!(done["sellPrice"], 12_000);
        assert_eq!(
            shop.st
                .current_session()
                .unwrap()
                .as_ref()
                .map(|s| s.user_id.clone()),
            Some(shop.owner.user_id.clone()),
            "the main PC's own sign-in is untouched"
        );
        let unknown = rpc
            .call(caller(&shop.owner.user_id), "users.delete", json!({}))
            .unwrap_err();
        assert_eq!(unknown.code(), "rpc_service_not_found");
        let id = cashier.clone();
        shop.st
            .db
            .write(move |tx| Ok(tx.execute("UPDATE users SET active = 0 WHERE id = ?1", [&id])?))
            .unwrap();
        let gone = rpc
            .call(
                caller(&cashier),
                "products.search",
                json!({ "query": { "text": "lait", "limit": 5, "categoryId": null, "includeArchived": false } }),
            )
            .unwrap_err();
        assert_eq!(gone.code(), "session_invalid");
        let audit: String = shop
            .st
            .db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT terminal_id FROM audit_log WHERE action = 'product.price_changed' ORDER BY seq DESC LIMIT 1",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(
            audit, shop.st.terminal.id,
            "made on the main PC, by the till's user"
        );
    }

    /// Every service the main PC announces is one it serves: a call with a wrong input is
    /// refused as an invalid value, never as an unknown service.
    #[test]
    fn every_announced_service_is_served() {
        let shop = shop();
        let rpc = AppRpc::new(std::sync::Arc::downgrade(&shop.st));
        for service in SERVICES {
            let caller = RpcCaller {
                terminal_id: "00000000-0000-7000-8000-0000000000c2".into(),
                terminal_code: "C2".into(),
                user_id: shop.owner.user_id.clone(),
                approvals: Vec::new(),
                idempotency_key: None,
            };
            let refused = rpc.call(caller, service, json!(null)).unwrap_err();
            assert_eq!(refused.code(), "value_invalid", "{service}");
        }
    }

    /// A till's label print records its reprint queue and the barcodes it assigns on the
    /// main PC, and an IMEI sold on a till is linked there, as the till's user.
    #[test]
    fn label_prints_and_imeis_of_a_till_are_recorded_on_the_main_pc_as_its_user() {
        let shop = shop();
        let rpc = AppRpc::new(std::sync::Arc::downgrade(&shop.st));
        let owner = || RpcCaller {
            terminal_id: "00000000-0000-7000-8000-0000000000c2".into(),
            terminal_code: "C2".into(),
            user_id: shop.owner.user_id.clone(),
            approvals: Vec::new(),
            idempotency_key: None,
        };
        let milk = shop.product("Lait", 10_000, 900, None).id;
        let assigned: Vec<pos_core::labels::AssignedBarcode> = serde_json::from_value(
            rpc.call(
                owner(),
                "labels.assign_barcodes",
                json!({ "productIds": [milk] }),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(assigned.len(), 1, "an internal EAN-13 for the milk");
        let request = pos_core::labels::LabelRequest {
            source: pos_core::labels::LabelSource::Products {
                items: vec![pos_core::labels::LabelItem {
                    product_id: milk.clone(),
                    copies: 2,
                    lot_id: None,
                }],
            },
            template_id: None,
            lang: None,
        };
        let day = pos_core::time::business_date(shop.st.now())
            .format("%Y-%m-%d")
            .to_string();
        let batch = shop
            .st
            .db
            .read(move |c| pos_core::labels::build(c, &request, &day))
            .unwrap();
        let recorded = rpc
            .call(
                owner(),
                "labels.record_printed",
                json!({ "batch": batch, "arabicOverride": true }),
            )
            .unwrap();
        assert_eq!(recorded, json!(null));
        let id = milk.clone();
        let (barcode, user, copies): (Option<String>, String, i64) = shop
            .st
            .db
            .read(move |c| {
                Ok(c.query_row(
                    "SELECT barcode, user_id, copies FROM label_prints WHERE product_id = ?1",
                    [&id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )?)
            })
            .unwrap();
        assert_eq!(
            (barcode, user, copies),
            (
                Some(assigned[0].barcode.clone()),
                shop.owner.user_id.clone(),
                2
            )
        );

        let phone = shop.product("Redmi Note 13", 4_500_000, 1_900, None).id;
        crate::commands::product_extras::set_extras(
            &shop.st,
            phone.clone(),
            pos_core::catalog::extras::ProductExtrasInput {
                track_serials: true,
                ..Default::default()
            },
        )
        .unwrap();
        shop.stock_in(&phone, 1);
        crate::commands::stock_m2::register_serials(
            &shop.st,
            phone.clone(),
            vec!["356789101112131".into()],
            None,
        )
        .unwrap();
        shop.open_till(0);
        let sold = shop.sell(&phone, 1, 5_000_000).document;
        let linked: Vec<pos_core::stock::serials::Serial> = serde_json::from_value(
            rpc.call(
                owner(),
                "serials.link_sale",
                json!({
                    "documentId": sold.id,
                    "links": [{ "lineId": sold.lines[0].id, "serialNo": "356789101112131" }],
                }),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            linked[0].status,
            pos_core::stock::serials::SerialStatus::Sold
        );
        let (terminal, user): (String, String) = shop
            .st
            .db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT terminal_id, user_id FROM audit_log WHERE action = 'serial.sold'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?)
            })
            .unwrap();
        assert_eq!(
            (terminal, user),
            (shop.st.terminal.id.clone(), shop.owner.user_id.clone())
        );
    }

    /// The till's user is this thread's session for the call only, even when the call fails or
    /// panics; other threads keep the main PC's own sign-in throughout.
    #[test]
    fn the_callers_session_lasts_for_the_call_on_its_thread_only() {
        let shop = shop();
        let cashier_id = shop.user(
            "Yacine",
            pos_core::auth::CASHIER_ROLE_ID,
            CASHIER_PIN,
            1_000,
        );
        let (id, terminal) = (cashier_id.clone(), shop.st.terminal.clone());
        let cashier = shop
            .st
            .db
            .read(move |c| {
                pos_core::auth::session::build_session(c, &id, &terminal, pos_core::time::now_ms())
            })
            .unwrap();
        fn signed_in(st: &crate::state::AppState) -> String {
            st.current_session()
                .unwrap()
                .map(|s| s.user_id)
                .unwrap_or_default()
        }
        let other = shop.st.clone();
        let seen = crate::state::with_caller(cashier.clone(), || {
            let elsewhere = std::thread::spawn(move || signed_in(&other))
                .join()
                .unwrap();
            (signed_in(&shop.st), elsewhere)
        });
        assert_eq!(seen, (cashier_id.clone(), shop.owner.user_id.clone()));
        let failed: Result<(), pos_core::error::AppError> =
            crate::state::with_caller(cashier.clone(), || {
                Err(pos_core::error::AppError::conflict("host_required"))
            });
        assert!(failed.is_err());
        assert_eq!(signed_in(&shop.st), shop.owner.user_id);
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::state::with_caller(cashier, || panic!("a command body panicked"))
        }));
        assert!(panicked.is_err());
        assert_eq!(
            signed_in(&shop.st),
            shop.owner.user_id,
            "reset after a panic"
        );
    }
}

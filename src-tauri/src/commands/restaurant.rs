//! Restaurant and café (master spec §29, RST-001…009): rooms and tables, the floor, modifiers,
//! stations and the daily menu; table, takeaway and delivery tickets, the kitchen, voids and
//! comps, the addition, splits and the payment. A table ticket is the till's own (it is paid as
//! one M1a ticket, its receipt queued like a sale's); kitchen tickets and the addition go to the
//! print queue after their write. The floor's master data is the main PC's (forwarded
//! from a till, `host_required` offline). Everything refuses with `module_disabled` while the
//! restaurant module is off.

use super::{blocking, redact};
use crate::lan::route::{self, forward};
use crate::printing::{PackSlip, PrintRequest};
use crate::state::{AppState, SharedState};
use pos_core::auth::Permission;
use pos_core::error::AppError;
use pos_core::money::Qty;
use pos_core::packs::{self, Module};
use pos_core::payments::PaymentInput;
use pos_core::restaurant::bill::{
    self, PaidTicket, PayInput, PreBill, SplitBy, SplitDone, VoidOutcome,
};
use pos_core::restaurant::floor::{
    self, DiningTable, Room, RoomInput, TableInput, TableStatus, TicketSummary,
};
use pos_core::restaurant::kitchen::{self, KitchenTicket, Station, StationInput};
use pos_core::restaurant::menu;
use pos_core::restaurant::modifiers::{self, ModifierGroup, ModifierGroupInput};
use pos_core::restaurant::tickets::{
    self, AddLineInput, CompReason, LineEdit, OpenInput, Ticket, TicketDetails, VoidReason,
};
use pos_core::sales::{PaymentPreview, SaleExtras};
use serde::Serialize;
use serde_json::json;
use specta::Type;
use tauri::State;

/// A paid table ticket and whether its receipt (or drawer) job was queued.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PaidTicketDone {
    pub paid: PaidTicket,
    /// A print or drawer job was queued; failures arrive later as `print-failed`.
    pub print_queued: bool,
}

#[tauri::command]
#[specta::specta]
pub async fn dining_rooms(state: State<'_, SharedState>) -> Result<Vec<Room>, AppError> {
    let st = state.inner().clone();
    blocking(move || rooms(&st)).await
}

/// Creates a room (`id: null`) or changes one.
#[tauri::command]
#[specta::specta]
pub async fn dining_room_save(
    state: State<'_, SharedState>,
    id: Option<String>,
    input: RoomInput,
) -> Result<Room, AppError> {
    let st = state.inner().clone();
    blocking(move || room_save(&st, id, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn dining_room_archive(
    state: State<'_, SharedState>,
    id: String,
) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || room_archive(&st, id)).await
}

/// The live tables of `room_id`, or of every room.
#[tauri::command]
#[specta::specta]
pub async fn dining_tables(
    state: State<'_, SharedState>,
    room_id: Option<String>,
) -> Result<Vec<DiningTable>, AppError> {
    let st = state.inner().clone();
    blocking(move || tables(&st, room_id)).await
}

/// Creates a table (`id: null`) or changes one: also how a table moves to another cell.
#[tauri::command]
#[specta::specta]
pub async fn dining_table_save(
    state: State<'_, SharedState>,
    id: Option<String>,
    input: TableInput,
) -> Result<DiningTable, AppError> {
    let st = state.inner().clone();
    blocking(move || table_save(&st, id, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn dining_table_archive(
    state: State<'_, SharedState>,
    id: String,
) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || table_archive(&st, id)).await
}

/// The floor: each table free, occupied or billed, with its tickets and their minutes.
#[tauri::command]
#[specta::specta]
pub async fn dining_board(
    state: State<'_, SharedState>,
    room_id: Option<String>,
) -> Result<Vec<TableStatus>, AppError> {
    let st = state.inner().clone();
    blocking(move || board(&st, room_id)).await
}

/// The live tickets without a table: takeaway, delivery, the counter.
#[tauri::command]
#[specta::specta]
pub async fn dining_counter(state: State<'_, SharedState>) -> Result<Vec<TicketSummary>, AppError> {
    let st = state.inner().clone();
    blocking(move || counter(&st)).await
}

/// The products of the daily menu of `day` (`YYYY-MM-DD`), in their order.
#[tauri::command]
#[specta::specta]
pub async fn daily_menu_get(
    state: State<'_, SharedState>,
    day: String,
) -> Result<Vec<String>, AppError> {
    let st = state.inner().clone();
    blocking(move || menu_get(&st, day)).await
}

#[tauri::command]
#[specta::specta]
pub async fn daily_menu_set(
    state: State<'_, SharedState>,
    day: String,
    product_ids: Vec<String>,
) -> Result<Vec<String>, AppError> {
    let st = state.inner().clone();
    blocking(move || menu_set(&st, day, product_ids)).await
}

#[tauri::command]
#[specta::specta]
pub async fn modifier_groups(
    state: State<'_, SharedState>,
) -> Result<Vec<ModifierGroup>, AppError> {
    let st = state.inner().clone();
    blocking(move || groups(&st)).await
}

/// Creates a modifier group (`id: null`) or changes one with its options.
#[tauri::command]
#[specta::specta]
pub async fn modifier_group_save(
    state: State<'_, SharedState>,
    id: Option<String>,
    input: ModifierGroupInput,
) -> Result<ModifierGroup, AppError> {
    let st = state.inner().clone();
    blocking(move || group_save(&st, id, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn modifier_group_archive(
    state: State<'_, SharedState>,
    id: String,
) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || group_archive(&st, id)).await
}

/// The live modifier groups of a product, in their order on it.
#[tauri::command]
#[specta::specta]
pub async fn modifiers_for_product(
    state: State<'_, SharedState>,
    product_id: String,
) -> Result<Vec<ModifierGroup>, AppError> {
    let st = state.inner().clone();
    blocking(move || groups_of_product(&st, product_id)).await
}

/// The groups a product asks at the till, in this order.
#[tauri::command]
#[specta::specta]
pub async fn modifiers_set_product(
    state: State<'_, SharedState>,
    product_id: String,
    group_ids: Vec<String>,
) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || set_product_groups(&st, product_id, group_ids)).await
}

#[tauri::command]
#[specta::specta]
pub async fn kitchen_stations(state: State<'_, SharedState>) -> Result<Vec<Station>, AppError> {
    let st = state.inner().clone();
    blocking(move || stations(&st)).await
}

/// Creates a station (`id: null`) or changes one with the categories it prepares.
#[tauri::command]
#[specta::specta]
pub async fn kitchen_station_save(
    state: State<'_, SharedState>,
    id: Option<String>,
    input: StationInput,
) -> Result<Station, AppError> {
    let st = state.inner().clone();
    blocking(move || station_save(&st, id, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn kitchen_station_archive(
    state: State<'_, SharedState>,
    id: String,
) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || station_archive(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn dining_ticket_open(
    state: State<'_, SharedState>,
    input: OpenInput,
) -> Result<Ticket, AppError> {
    let st = state.inner().clone();
    blocking(move || ticket_open(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn dining_ticket_get(
    state: State<'_, SharedState>,
    id: String,
) -> Result<Ticket, AppError> {
    let st = state.inner().clone();
    blocking(move || ticket_get(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn dining_ticket_add_line(
    state: State<'_, SharedState>,
    ticket_id: String,
    input: AddLineInput,
) -> Result<Ticket, AppError> {
    let st = state.inner().clone();
    blocking(move || ticket_add_line(&st, ticket_id, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn dining_ticket_set_qty(
    state: State<'_, SharedState>,
    line_id: String,
    qty: Qty,
) -> Result<Ticket, AppError> {
    let st = state.inner().clone();
    blocking(move || ticket_set_qty(&st, line_id, qty)).await
}

/// Removes a line the kitchen has not received (a sent one is voided with a reason).
#[tauri::command]
#[specta::specta]
pub async fn dining_ticket_remove_unsent(
    state: State<'_, SharedState>,
    line_id: String,
) -> Result<Ticket, AppError> {
    let st = state.inner().clone();
    blocking(move || ticket_remove_unsent(&st, line_id)).await
}

/// Moves a line the kitchen has not received to another seat or course and sets its note.
#[tauri::command]
#[specta::specta]
pub async fn dining_ticket_edit_line(
    state: State<'_, SharedState>,
    line_id: String,
    edit: LineEdit,
) -> Result<Ticket, AppError> {
    let st = state.inner().clone();
    blocking(move || ticket_edit_line(&st, line_id, edit)).await
}

#[tauri::command]
#[specta::specta]
pub async fn dining_ticket_set_details(
    state: State<'_, SharedState>,
    ticket_id: String,
    details: TicketDetails,
) -> Result<Ticket, AppError> {
    let st = state.inner().clone();
    blocking(move || ticket_set_details(&st, ticket_id, details)).await
}

/// Moves a dine-in ticket to another table.
#[tauri::command]
#[specta::specta]
pub async fn dining_ticket_transfer(
    state: State<'_, SharedState>,
    ticket_id: String,
    table_id: String,
) -> Result<Ticket, AppError> {
    let st = state.inner().clone();
    blocking(move || ticket_transfer(&st, ticket_id, table_id)).await
}

/// Moves every line of `from_id` to `into_id` (lines move, never copy).
#[tauri::command]
#[specta::specta]
pub async fn dining_ticket_merge(
    state: State<'_, SharedState>,
    from_id: String,
    into_id: String,
) -> Result<Ticket, AppError> {
    let st = state.inner().clone();
    blocking(move || ticket_merge(&st, from_id, into_id)).await
}

/// Sends what the kitchen has not received, one ticket per station, and queues their print.
#[tauri::command]
#[specta::specta]
pub async fn dining_ticket_send(
    state: State<'_, SharedState>,
    ticket_id: String,
) -> Result<Vec<KitchenTicket>, AppError> {
    let st = state.inner().clone();
    blocking(move || ticket_send(&st, ticket_id)).await
}

/// Voids `qty` of a line (all of it with `null`) with a reason; the kitchen gets a cancellation
/// for what it had received. `sales.void_line` or a manager's approval of it.
#[tauri::command]
#[specta::specta]
pub async fn dining_ticket_void_line(
    state: State<'_, SharedState>,
    line_id: String,
    qty: Option<Qty>,
    reason: VoidReason,
) -> Result<VoidOutcome, AppError> {
    let st = state.inner().clone();
    blocking(move || void_line(&st, line_id, qty, reason)).await
}

#[tauri::command]
#[specta::specta]
pub async fn dining_ticket_void(
    state: State<'_, SharedState>,
    ticket_id: String,
    reason: VoidReason,
) -> Result<VoidOutcome, AppError> {
    let st = state.inner().clone();
    blocking(move || void_ticket(&st, ticket_id, reason)).await
}

/// Offers a line (100 % discount with its reason): `sales.discount` or a manager's approval.
#[tauri::command]
#[specta::specta]
pub async fn dining_ticket_comp_line(
    state: State<'_, SharedState>,
    line_id: String,
    reason: CompReason,
) -> Result<Ticket, AppError> {
    let st = state.inner().clone();
    blocking(move || comp_line(&st, line_id, reason)).await
}

/// The addition: marks the ticket billed and prints it ("ce document n'est pas un ticket de
/// caisse"); `parts` asks for even shares.
#[tauri::command]
#[specta::specta]
pub async fn dining_ticket_prebill(
    state: State<'_, SharedState>,
    ticket_id: String,
    parts: Option<u8>,
) -> Result<PreBill, AppError> {
    let st = state.inner().clone();
    blocking(move || ticket_prebill(&st, ticket_id, parts)).await
}

/// The payment dialog's figures for a table ticket: the bill the payment will take and the
/// settlement of the tenders being typed (reads only).
#[tauri::command]
#[specta::specta]
pub async fn dining_ticket_preview(
    state: State<'_, SharedState>,
    ticket_id: String,
    tenders: Vec<PaymentInput>,
) -> Result<PaymentPreview, AppError> {
    let st = state.inner().clone();
    blocking(move || ticket_preview(&st, ticket_id, tenders)).await
}

#[tauri::command]
#[specta::specta]
pub async fn dining_ticket_split(
    state: State<'_, SharedState>,
    ticket_id: String,
    by: SplitBy,
) -> Result<SplitDone, AppError> {
    let st = state.inner().clone();
    blocking(move || ticket_split(&st, ticket_id, by)).await
}

/// Pays the ticket: one validated sale with the till's pack extras, its receipt queued, the
/// kitchen given what it had not received.
#[tauri::command]
#[specta::specta]
pub async fn dining_ticket_pay(
    state: State<'_, SharedState>,
    input: PayInput,
    print_receipt: bool,
    extras: Option<SaleExtras>,
) -> Result<PaidTicketDone, AppError> {
    let st = state.inner().clone();
    blocking(move || ticket_pay_with(&st, input, extras.unwrap_or_default(), print_receipt)).await
}

/// Prints a kitchen ticket again (one whose print failed: the floor shows it unprinted).
#[tauri::command]
#[specta::specta]
pub async fn kitchen_reprint(state: State<'_, SharedState>, id: String) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || reprint_kitchen(&st, id)).await
}

/// The floor's catalogue reads are shared by the till and the back office.
const FLOOR_READ: &[Permission] = &[Permission::SalesCreate, Permission::ProductsView];

pub(crate) fn rooms(st: &AppState) -> Result<Vec<Room>, AppError> {
    st.require_any(FLOOR_READ)?;
    st.db.read(|c| {
        packs::require(c, Module::Restaurant)?;
        floor::rooms(c, false)
    })
}

pub(crate) fn room_save(
    st: &AppState,
    id: Option<String>,
    input: RoomInput,
) -> Result<Room, AppError> {
    if let Some(done) = forward(st, "dining.room_save", json!({ "id": id, "input": input }))? {
        return Ok(done);
    }
    let actor = st.require(Permission::SettingsCompany)?.actor();
    st.db.write(move |tx| {
        packs::require(tx, Module::Restaurant)?;
        floor::save_room(tx, &actor, id.as_deref(), input)
    })
}

pub(crate) fn room_archive(st: &AppState, id: String) -> Result<(), AppError> {
    if let Some(done) = forward(st, "dining.room_archive", json!({ "id": id }))? {
        return Ok(done);
    }
    let actor = st.require(Permission::SettingsCompany)?.actor();
    st.db.write(move |tx| {
        packs::require(tx, Module::Restaurant)?;
        floor::archive_room(tx, &actor, &id)
    })
}

pub(crate) fn tables(st: &AppState, room_id: Option<String>) -> Result<Vec<DiningTable>, AppError> {
    st.require_any(FLOOR_READ)?;
    st.db.read(move |c| {
        packs::require(c, Module::Restaurant)?;
        floor::tables(c, room_id.as_deref())
    })
}

pub(crate) fn table_save(
    st: &AppState,
    id: Option<String>,
    input: TableInput,
) -> Result<DiningTable, AppError> {
    if let Some(done) = forward(st, "dining.table_save", json!({ "id": id, "input": input }))? {
        return Ok(done);
    }
    let actor = st.require(Permission::SettingsCompany)?.actor();
    st.db.write(move |tx| {
        packs::require(tx, Module::Restaurant)?;
        floor::save_table(tx, &actor, id.as_deref(), input)
    })
}

pub(crate) fn table_archive(st: &AppState, id: String) -> Result<(), AppError> {
    if let Some(done) = forward(st, "dining.table_archive", json!({ "id": id }))? {
        return Ok(done);
    }
    let actor = st.require(Permission::SettingsCompany)?.actor();
    st.db.write(move |tx| {
        packs::require(tx, Module::Restaurant)?;
        floor::archive_table(tx, &actor, &id)
    })
}

pub(crate) fn board(st: &AppState, room_id: Option<String>) -> Result<Vec<TableStatus>, AppError> {
    st.require(Permission::SalesCreate)?;
    let now = st.now();
    st.db.read(move |c| {
        packs::require(c, Module::Restaurant)?;
        floor::board(c, room_id.as_deref(), now)
    })
}

pub(crate) fn counter(st: &AppState) -> Result<Vec<TicketSummary>, AppError> {
    st.require(Permission::SalesCreate)?;
    let now = st.now();
    st.db.read(move |c| {
        packs::require(c, Module::Restaurant)?;
        floor::counter(c, now)
    })
}

pub(crate) fn menu_get(st: &AppState, day: String) -> Result<Vec<String>, AppError> {
    st.require_any(FLOOR_READ)?;
    st.db.read(move |c| {
        packs::require(c, Module::Restaurant)?;
        menu::of_day(c, &day)
    })
}

pub(crate) fn menu_set(
    st: &AppState,
    day: String,
    product_ids: Vec<String>,
) -> Result<Vec<String>, AppError> {
    if let Some(done) = forward(
        st,
        "dining.menu_set",
        json!({ "day": day, "productIds": product_ids }),
    )? {
        return Ok(done);
    }
    let actor = st.require(Permission::ProductsEdit)?.actor();
    st.db.write(move |tx| {
        packs::require(tx, Module::Restaurant)?;
        menu::set_day(tx, &actor, &day, &product_ids)
    })
}

pub(crate) fn groups(st: &AppState) -> Result<Vec<ModifierGroup>, AppError> {
    st.require_any(FLOOR_READ)?;
    st.db.read(|c| {
        packs::require(c, Module::Restaurant)?;
        modifiers::groups(c, false)
    })
}

pub(crate) fn group_save(
    st: &AppState,
    id: Option<String>,
    input: ModifierGroupInput,
) -> Result<ModifierGroup, AppError> {
    if let Some(done) = forward(
        st,
        "modifiers.group_save",
        json!({ "id": id, "input": input }),
    )? {
        return Ok(done);
    }
    let actor = st.require(Permission::ProductsEdit)?.actor();
    st.db.write(move |tx| {
        packs::require(tx, Module::Restaurant)?;
        modifiers::save_group(tx, &actor, id.as_deref(), input)
    })
}

pub(crate) fn group_archive(st: &AppState, id: String) -> Result<(), AppError> {
    if let Some(done) = forward(st, "modifiers.group_archive", json!({ "id": id }))? {
        return Ok(done);
    }
    let actor = st.require(Permission::ProductsEdit)?.actor();
    st.db.write(move |tx| {
        packs::require(tx, Module::Restaurant)?;
        modifiers::archive_group(tx, &actor, &id)
    })
}

pub(crate) fn groups_of_product(
    st: &AppState,
    product_id: String,
) -> Result<Vec<ModifierGroup>, AppError> {
    st.require_any(FLOOR_READ)?;
    st.db.read(move |c| {
        packs::require(c, Module::Restaurant)?;
        modifiers::for_product(c, &product_id)
    })
}

pub(crate) fn set_product_groups(
    st: &AppState,
    product_id: String,
    group_ids: Vec<String>,
) -> Result<(), AppError> {
    if let Some(done) = forward(
        st,
        "modifiers.set_product",
        json!({ "productId": product_id, "groupIds": group_ids }),
    )? {
        return Ok(done);
    }
    let actor = st.require(Permission::ProductsEdit)?.actor();
    st.db.write(move |tx| {
        packs::require(tx, Module::Restaurant)?;
        modifiers::set_product_groups(tx, &actor, &product_id, &group_ids)
    })
}

pub(crate) fn stations(st: &AppState) -> Result<Vec<Station>, AppError> {
    st.require_any(FLOOR_READ)?;
    st.db.read(|c| {
        packs::require(c, Module::Restaurant)?;
        kitchen::stations(c, false)
    })
}

pub(crate) fn station_save(
    st: &AppState,
    id: Option<String>,
    input: StationInput,
) -> Result<Station, AppError> {
    if let Some(done) = forward(
        st,
        "kitchen.station_save",
        json!({ "id": id, "input": input }),
    )? {
        return Ok(done);
    }
    let actor = st.require(Permission::SettingsCompany)?.actor();
    st.db.write(move |tx| {
        packs::require(tx, Module::Restaurant)?;
        kitchen::save_station(tx, &actor, id.as_deref(), input)
    })
}

pub(crate) fn station_archive(st: &AppState, id: String) -> Result<(), AppError> {
    if let Some(done) = forward(st, "kitchen.station_archive", json!({ "id": id }))? {
        return Ok(done);
    }
    let actor = st.require(Permission::SettingsCompany)?.actor();
    st.db.write(move |tx| {
        packs::require(tx, Module::Restaurant)?;
        kitchen::archive_station(tx, &actor, &id)
    })
}

/// The ticket writes run as the operator (`sales.create`, or the command's permission with a
/// pending manager's approval of it), module on.
pub(crate) fn ticket_open(st: &AppState, input: OpenInput) -> Result<Ticket, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    st.db.write(move |tx| {
        packs::require(tx, Module::Restaurant)?;
        tickets::open(tx, &ctx, input)
    })
}

pub(crate) fn ticket_get(st: &AppState, id: String) -> Result<Ticket, AppError> {
    st.require(Permission::SalesCreate)?;
    st.db.read(move |c| {
        packs::require(c, Module::Restaurant)?;
        tickets::get(c, &id)
    })
}

pub(crate) fn ticket_add_line(
    st: &AppState,
    ticket_id: String,
    input: AddLineInput,
) -> Result<Ticket, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    st.db.write(move |tx| {
        packs::require(tx, Module::Restaurant)?;
        tickets::add_line(tx, &ctx, &ticket_id, input)
    })
}

pub(crate) fn ticket_set_qty(st: &AppState, line_id: String, qty: Qty) -> Result<Ticket, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    st.db.write(move |tx| {
        packs::require(tx, Module::Restaurant)?;
        tickets::set_qty(tx, &ctx, &line_id, qty)
    })
}

pub(crate) fn ticket_remove_unsent(st: &AppState, line_id: String) -> Result<Ticket, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    st.db.write(move |tx| {
        packs::require(tx, Module::Restaurant)?;
        tickets::remove_unsent(tx, &ctx, &line_id)
    })
}

pub(crate) fn ticket_edit_line(
    st: &AppState,
    line_id: String,
    edit: LineEdit,
) -> Result<Ticket, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    st.db.write(move |tx| {
        packs::require(tx, Module::Restaurant)?;
        tickets::edit_unsent(tx, &ctx, &line_id, edit)
    })
}

pub(crate) fn ticket_set_details(
    st: &AppState,
    ticket_id: String,
    details: TicketDetails,
) -> Result<Ticket, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    st.db.write(move |tx| {
        packs::require(tx, Module::Restaurant)?;
        tickets::set_details(tx, &ctx, &ticket_id, details)
    })
}

pub(crate) fn ticket_transfer(
    st: &AppState,
    ticket_id: String,
    table_id: String,
) -> Result<Ticket, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    st.db.write(move |tx| {
        packs::require(tx, Module::Restaurant)?;
        tickets::transfer(tx, &ctx, &ticket_id, &table_id)
    })
}

pub(crate) fn ticket_merge(
    st: &AppState,
    from_id: String,
    into_id: String,
) -> Result<Ticket, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    st.db.write(move |tx| {
        packs::require(tx, Module::Restaurant)?;
        tickets::merge(tx, &ctx, &from_id, &into_id)
    })
}

/// Queues the print of each kitchen ticket; the print thread marks it printed.
fn print_kitchen(st: &AppState, sent: &[KitchenTicket]) {
    for k in sent {
        st.printer
            .enqueue(PrintRequest::slip(PackSlip::Kitchen, &k.id));
    }
}

pub(crate) fn ticket_send(
    st: &AppState,
    ticket_id: String,
) -> Result<Vec<KitchenTicket>, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    let sent = st.db.write(move |tx| {
        packs::require(tx, Module::Restaurant)?;
        kitchen::send(tx, &ctx, &ticket_id)
    })?;
    print_kitchen(st, &sent);
    Ok(sent)
}

/// Voids and comps accept a manager's approval of their permission, spent once written.
pub(crate) fn void_line(
    st: &AppState,
    line_id: String,
    qty: Option<Qty>,
    reason: VoidReason,
) -> Result<VoidOutcome, AppError> {
    let ctx = st.till(Permission::SalesVoidLine)?;
    let write = ctx.clone();
    let done = st.db.write(move |tx| {
        packs::require(tx, Module::Restaurant)?;
        bill::void_line(tx, &write, &line_id, qty, reason)
    })?;
    st.spend_approvals(&ctx)?;
    print_kitchen(st, &done.cancels);
    Ok(done)
}

pub(crate) fn void_ticket(
    st: &AppState,
    ticket_id: String,
    reason: VoidReason,
) -> Result<VoidOutcome, AppError> {
    let ctx = st.till(Permission::SalesVoidLine)?;
    let write = ctx.clone();
    let done = st.db.write(move |tx| {
        packs::require(tx, Module::Restaurant)?;
        bill::void_ticket(tx, &write, &ticket_id, reason)
    })?;
    st.spend_approvals(&ctx)?;
    print_kitchen(st, &done.cancels);
    Ok(done)
}

pub(crate) fn comp_line(
    st: &AppState,
    line_id: String,
    reason: CompReason,
) -> Result<Ticket, AppError> {
    let ctx = st.till(Permission::SalesDiscount)?;
    let write = ctx.clone();
    let done = st.db.write(move |tx| {
        packs::require(tx, Module::Restaurant)?;
        bill::comp_line(tx, &write, &line_id, reason)
    })?;
    st.spend_approvals(&ctx)?;
    Ok(done)
}

/// The addition in the ticket's language on the receipt printer, after its write; a failed print
/// is named as the addition of `ticket_id` and printed again by this command.
pub(crate) fn ticket_prebill(
    st: &AppState,
    ticket_id: String,
    parts: Option<u8>,
) -> Result<PreBill, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    let session = ctx.session.clone();
    let id = ticket_id.clone();
    let mut bill = st.db.write(move |tx| {
        packs::require(tx, Module::Restaurant)?;
        bill::prebill(tx, &ctx, &id, parts)
    })?;
    let lang = bill.ticket.lang;
    // RST-007 through pos-hw's layout, with the demo shop's mention
    let (shop, demo) = crate::printing::slip_shop(&st.db, lang)?;
    let layout = pos_hw::slips::with_demo(
        pos_hw::slips::prebill_layout(&pos_docs::slips::prebill_slip(&bill, &shop.name, lang)),
        lang,
        demo,
    );
    st.printer
        .enqueue(PrintRequest::layout(PackSlip::PreBill, &ticket_id, layout));
    redact::priced(&mut bill.priced, &session);
    Ok(bill)
}

/// The bill priced as the payment will price it; costs are redacted like a till preview's.
pub(crate) fn ticket_preview(
    st: &AppState,
    ticket_id: String,
    tenders: Vec<PaymentInput>,
) -> Result<PaymentPreview, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    let session = ctx.session.clone();
    let mut out = st.db.read(move |c| {
        packs::require(c, Module::Restaurant)?;
        bill::preview(c, &ctx, &ticket_id, &tenders)
    })?;
    redact::priced(&mut out.cart, &session);
    Ok(out)
}

pub(crate) fn ticket_split(
    st: &AppState,
    ticket_id: String,
    by: SplitBy,
) -> Result<SplitDone, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    st.db.write(move |tx| {
        packs::require(tx, Module::Restaurant)?;
        bill::split(tx, &ctx, &ticket_id, by)
    })
}

/// [`ticket_pay_with`] without pack extras (the tests' table payments).
#[cfg(test)]
pub(crate) fn ticket_pay(
    st: &AppState,
    input: PayInput,
    print: bool,
) -> Result<PaidTicketDone, AppError> {
    ticket_pay_with(st, input, SaleExtras::default(), print)
}

/// On a till, a kridi tender needs the main PC to answer (`kridi_needs_host`), like any sale.
pub(crate) fn ticket_pay_with(
    st: &AppState,
    input: PayInput,
    extras: SaleExtras,
    print: bool,
) -> Result<PaidTicketDone, AppError> {
    let ctx = st.till(Permission::SalesCreate)?;
    route::require_live_for_kridi(st, route::uses_account(&input.payments))?;
    // A table's controlled medicine or jewel (rare) is registered on the main PC
    let ticket_id = input.ticket_id.clone();
    let products = st.db.read(move |c| {
        Ok(tickets::get(c, &ticket_id)?
            .lines
            .into_iter()
            .filter_map(|line| line.product_id)
            .collect::<Vec<_>>())
    })?;
    crate::lan::registers::require_host(st, products)?;
    let write = ctx.clone();
    let mut paid = st.db.write(move |tx| {
        packs::require(tx, Module::Restaurant)?;
        bill::pay_with(tx, &write, input, &extras)
    })?;
    st.spend_approvals(&ctx)?;
    crate::lan::registers::send_pending(st);
    print_kitchen(st, &paid.kitchen);
    let cash_moved = paid.sale.settlement.cash_tendered.is_positive();
    let print_queued = (print || cash_moved)
        && st.printer.enqueue(PrintRequest {
            receipt: print,
            cash_moved,
            ..PrintRequest::receipt(&paid.sale.document.id)
        });
    redact::document(&mut paid.sale.document, &ctx.session);
    Ok(PaidTicketDone { paid, print_queued })
}

pub(crate) fn reprint_kitchen(st: &AppState, id: String) -> Result<(), AppError> {
    st.require(Permission::SalesCreate)?;
    let kitchen_id = id.clone();
    st.db.read(move |c| {
        packs::require(c, Module::Restaurant)?;
        kitchen::get(c, &kitchen_id).map(drop)
    })?;
    if st
        .printer
        .enqueue(PrintRequest::slip(PackSlip::Kitchen, &id))
    {
        Ok(())
    } else {
        Err(AppError::Internal("print queue unavailable".into()))
    }
}

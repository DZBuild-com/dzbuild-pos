//! Customers (CRM-001…012): directory, quick creation at the till, blocking, archive. Balances
//! and limits are hidden without `credit.view` (`redact::customer`).

use super::{blocking, redact};
use crate::lan::route::forward;
use crate::state::{AppState, SharedState};
use pos_core::auth::{Permission, Session};
use pos_core::customers::{self, Customer, CustomerHit, CustomerInput, CustomerQuery};
use pos_core::documents::DocumentSummary;
use pos_core::error::AppError;
use serde_json::json;
use tauri::State;

/// The directory is shared by the back office and the till (F3 at the sale).
const READ: &[Permission] = &[Permission::CustomersView, Permission::SalesCreate];

#[tauri::command]
#[specta::specta]
pub async fn customers_search(
    state: State<'_, SharedState>,
    query: CustomerQuery,
) -> Result<Vec<CustomerHit>, AppError> {
    let st = state.inner().clone();
    blocking(move || search(&st, query)).await
}

#[tauri::command]
#[specta::specta]
pub async fn customers_get(
    state: State<'_, SharedState>,
    id: String,
) -> Result<Customer, AppError> {
    let st = state.inner().clone();
    blocking(move || get(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn customers_find_by_phone(
    state: State<'_, SharedState>,
    phone: String,
) -> Result<Option<Customer>, AppError> {
    let st = state.inner().clone();
    blocking(move || find_by_phone(&st, phone)).await
}

#[tauri::command]
#[specta::specta]
pub async fn customers_create(
    state: State<'_, SharedState>,
    input: CustomerInput,
) -> Result<Customer, AppError> {
    let st = state.inner().clone();
    blocking(move || create(&st, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn customers_quick_create(
    state: State<'_, SharedState>,
    name: String,
    phone: Option<String>,
) -> Result<Customer, AppError> {
    let st = state.inner().clone();
    blocking(move || quick_create(&st, name, phone)).await
}

#[tauri::command]
#[specta::specta]
pub async fn customers_update(
    state: State<'_, SharedState>,
    id: String,
    input: CustomerInput,
) -> Result<Customer, AppError> {
    let st = state.inner().clone();
    blocking(move || update(&st, id, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn customers_set_blocked(
    state: State<'_, SharedState>,
    id: String,
    blocked: bool,
) -> Result<Customer, AppError> {
    let st = state.inner().clone();
    blocking(move || set_blocked(&st, id, blocked)).await
}

#[tauri::command]
#[specta::specta]
pub async fn customers_archive(state: State<'_, SharedState>, id: String) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || archive(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn customers_restore(state: State<'_, SharedState>, id: String) -> Result<(), AppError> {
    let st = state.inner().clone();
    blocking(move || restore(&st, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn customers_documents(
    state: State<'_, SharedState>,
    id: String,
    limit: u32,
) -> Result<Vec<DocumentSummary>, AppError> {
    let st = state.inner().clone();
    blocking(move || documents_of(&st, id, limit)).await
}

fn shown(mut customer: Customer, session: &Session) -> Customer {
    redact::customer(&mut customer, session);
    customer
}

/// Who owes the shop money is kridi data: without `credit.view` the "with debt" filter is
/// ignored, like the balances are redacted (`redact::customer_hit`).
pub(crate) fn search(
    st: &AppState,
    mut query: CustomerQuery,
) -> Result<Vec<CustomerHit>, AppError> {
    let session = st.require_any(READ)?;
    if !session.has(Permission::CreditView) {
        query.with_debt = false;
    }
    let mut hits = st.db.read(move |c| customers::search(c, &query))?;
    for hit in &mut hits {
        redact::customer_hit(hit, &session);
    }
    Ok(hits)
}

pub(crate) fn get(st: &AppState, id: String) -> Result<Customer, AppError> {
    let session = st.require_any(READ)?;
    let found = st.db.read(move |c| customers::get(c, &id))?;
    // PDP-003 (loi 18-07): opening a file that holds personal data is journaled, never its fields.
    if session.has(Permission::CustomersView) && holds_personal_data(&found) {
        let (actor, customer_id) = (session.actor(), found.id.clone());
        st.db.write(move |tx| {
            pos_core::privacy::log_access(
                tx,
                &actor,
                &customer_id,
                pos_core::privacy::AccessPurpose::View,
            )
        })?;
    }
    Ok(shown(found, &session))
}

/// A phone, an e-mail, an address or a national identity number: what PDP-003 journals access to.
fn holds_personal_data(c: &Customer) -> bool {
    [&c.phone, &c.phone2, &c.email, &c.address, &c.nin]
        .into_iter()
        .any(|field| field.as_deref().is_some_and(|v| !v.trim().is_empty()))
}

pub(crate) fn find_by_phone(st: &AppState, phone: String) -> Result<Option<Customer>, AppError> {
    let session = st.require_any(READ)?;
    let found = st.db.read(move |c| customers::find_by_phone(c, &phone))?;
    Ok(found.map(|c| shown(c, &session)))
}

pub(crate) fn create(st: &AppState, input: CustomerInput) -> Result<Customer, AppError> {
    let session = st.require(Permission::CustomersManage)?;
    let actor = session.actor();
    let pack = st.datapack.clone();
    let created = st
        .db
        .write(move |tx| customers::create(tx, &actor, &pack, input))?;
    Ok(shown(created, &session))
}

/// CRM-003: the cashier types a name and maybe a phone; nothing else can be set from the till.
/// On a till the customer is made on the main PC (`lan::route::forward`).
pub(crate) fn quick_create(
    st: &AppState,
    name: String,
    phone: Option<String>,
) -> Result<Customer, AppError> {
    if let Some(done) = forward(
        st,
        "customers.quick_create",
        json!({ "name": name, "phone": phone }),
    )? {
        return Ok(done);
    }
    let session = st.require_any(&[Permission::SalesCreate, Permission::CustomersManage])?;
    let actor = session.actor();
    let pack = st.datapack.clone();
    let input = CustomerInput {
        phone,
        ..CustomerInput::named(&name)
    };
    let created = st
        .db
        .write(move |tx| customers::create(tx, &actor, &pack, input))?;
    Ok(shown(created, &session))
}

/// Without `credit.view` the form showed no limit (`redact::customer`): the stored one is kept,
/// never wiped by the empty field sent back.
pub(crate) fn update(
    st: &AppState,
    id: String,
    input: CustomerInput,
) -> Result<Customer, AppError> {
    if let Some(done) = forward(st, "customers.update", json!({ "id": id, "input": input }))? {
        return Ok(done);
    }
    let session = st.require(Permission::CustomersManage)?;
    let actor = session.actor();
    let sees_credit = session.has(Permission::CreditView);
    let pack = st.datapack.clone();
    let updated = st.db.write(move |tx| {
        let mut input = input;
        if !sees_credit {
            input.credit_limit = customers::get(tx, &id)?.credit_limit;
        }
        customers::update(tx, &actor, &pack, &id, input)
    })?;
    Ok(shown(updated, &session))
}

pub(crate) fn set_blocked(st: &AppState, id: String, blocked: bool) -> Result<Customer, AppError> {
    let session = st.require(Permission::CustomersManage)?;
    let actor = session.actor();
    let changed = st
        .db
        .write(move |tx| customers::set_blocked(tx, &actor, &id, blocked))?;
    Ok(shown(changed, &session))
}

pub(crate) fn archive(st: &AppState, id: String) -> Result<(), AppError> {
    let actor = st.require(Permission::CustomersManage)?.actor();
    st.db.write(move |tx| customers::archive(tx, &actor, &id))
}

pub(crate) fn restore(st: &AppState, id: String) -> Result<(), AppError> {
    let actor = st.require(Permission::CustomersManage)?.actor();
    st.db.write(move |tx| customers::restore(tx, &actor, &id))
}

pub(crate) fn documents_of(
    st: &AppState,
    id: String,
    limit: u32,
) -> Result<Vec<DocumentSummary>, AppError> {
    st.require(Permission::CustomersView)?;
    st.db
        .read(move |c| customers::documents(c, &id, limit.clamp(1, 500)))
}

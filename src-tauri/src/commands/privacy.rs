//! The rows M6 handed over to M8 (Task 17's core, wired here as "Task 21b"): personal data
//! (consents, a customer's data exported on request, anonymisation, the processing register,
//! PDP-002…005), the signed regulatory data packs installed for the next start (PLT-024) and the
//! till's key map (UX-017). Consents and anonymisation write the shared customer books: on a till
//! they are the main PC's host services (`host_required` offline). A data pack is this PC's
//! file; the key map is a shop setting like any other.

use super::print::{A4Printed, print_page};
use super::{blocking, pick_dialog};
use crate::lan::route::{forward, forward_spent};
use crate::print_host::{HostPage, PrintHost};
use crate::state::{AppState, SharedState};
use pos_core::auth::Permission;
use pos_core::billing::archive::{self, ArchiveQuery};
use pos_core::datapack_update::{self, DATAPACK_PUBLIC_KEY, InstalledPack};
use pos_core::error::AppError;
use pos_core::lang::Lang;
use pos_core::privacy::{
    self, AccessPurpose, AnonymiseReport, Consent, ConsentChannel, ConsentSource, Dpo,
};
use pos_docs::sheet::{self, SheetFormat, SheetMeta};
use serde::Serialize;
use serde_json::json;
use specta::Type;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, State, Window};

/// Consents are read at the till (the ready message of a repair) and in the customer's file.
const READ: &[Permission] = &[Permission::CustomersView, Permission::SalesCreate];

/// The two files of a customer's data (PDP-004), written where the user chose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportedCustomer {
    pub json_path: String,
    pub xlsx_path: String,
}

#[tauri::command]
#[specta::specta]
pub async fn privacy_consents(
    state: State<'_, SharedState>,
    customer_id: String,
) -> Result<Vec<Consent>, AppError> {
    let st = state.inner().clone();
    blocking(move || consents(&st, customer_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn privacy_set_consent(
    state: State<'_, SharedState>,
    customer_id: String,
    channel: ConsentChannel,
    granted: bool,
    source: ConsentSource,
    note: Option<String>,
) -> Result<Consent, AppError> {
    let st = state.inner().clone();
    blocking(move || set_consent(&st, customer_id, channel, granted, source, note)).await
}

/// The folder comes from a Rust dialog (`None` when the user cancels); the webview never names it.
#[tauri::command]
#[specta::specta]
pub async fn privacy_export_customer(
    app: AppHandle,
    window: Window,
    state: State<'_, SharedState>,
    customer_id: String,
) -> Result<Option<ExportedCustomer>, AppError> {
    let st = state.inner().clone();
    st.require(Permission::CustomersManage)?;
    blocking(move || {
        use tauri_plugin_dialog::DialogExt;
        let Some(picked) = app
            .dialog()
            .file()
            .set_parent(&window)
            .blocking_pick_folder()
        else {
            return Ok(None);
        };
        let folder = picked
            .into_path()
            .map_err(|_| AppError::Io("the chosen folder cannot be used".into()))?;
        export_customer(&st, customer_id, &folder).map(Some)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn privacy_anonymise(
    state: State<'_, SharedState>,
    customer_id: String,
    note: Option<String>,
) -> Result<AnonymiseReport, AppError> {
    let st = state.inner().clone();
    blocking(move || anonymise(&st, customer_id, note)).await
}

#[tauri::command]
#[specta::specta]
pub async fn privacy_dpo(state: State<'_, SharedState>) -> Result<Option<Dpo>, AppError> {
    let st = state.inner().clone();
    blocking(move || dpo(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn privacy_register(
    state: State<'_, SharedState>,
    host: State<'_, Arc<PrintHost>>,
    lang: Lang,
) -> Result<A4Printed, AppError> {
    let (st, host) = (state.inner().clone(), host.inner().clone());
    blocking(move || print_register(&st, &host, lang)).await
}

#[tauri::command]
#[specta::specta]
pub async fn till_key_map(
    state: State<'_, SharedState>,
) -> Result<Vec<(String, String)>, AppError> {
    let st = state.inner().clone();
    blocking(move || key_map(&st)).await
}

#[tauri::command]
#[specta::specta]
pub async fn till_set_key_map(
    state: State<'_, SharedState>,
    map: BTreeMap<String, String>,
) -> Result<Vec<(String, String)>, AppError> {
    let st = state.inner().clone();
    blocking(move || set_key_map(&st, map)).await
}

#[tauri::command]
#[specta::specta]
pub async fn datapack_installed(
    state: State<'_, SharedState>,
) -> Result<Vec<InstalledPack>, AppError> {
    let st = state.inner().clone();
    blocking(move || installed(&st)).await
}

/// The pack's `.json` then its `.sig` come from Rust dialogs (`None` when one is cancelled);
/// the webview never names a file.
#[tauri::command]
#[specta::specta]
pub async fn datapack_install(
    app: AppHandle,
    window: Window,
    state: State<'_, SharedState>,
) -> Result<Option<InstalledPack>, AppError> {
    let st = state.inner().clone();
    st.require(Permission::SettingsCompany)?;
    blocking(move || {
        let Some(json) = pick_dialog(&app, &window, "JSON", &["json"])? else {
            return Ok(None);
        };
        let Some(sig) = pick_dialog(&app, &window, "SIG", &["sig"])? else {
            return Ok(None);
        };
        install_files(&st, &datapack_dir(&st), &json, &sig, DATAPACK_PUBLIC_KEY).map(Some)
    })
    .await
}

/// Every answer of the customer, newest first (the latest per channel is the one in force).
pub(crate) fn consents(st: &AppState, customer_id: String) -> Result<Vec<Consent>, AppError> {
    st.require_any(READ)?;
    st.db
        .read(move |c| privacy::consent_history(c, &customer_id))
}

/// PDP-002: the customer's answer about a channel, recorded where the customer books live.
pub(crate) fn set_consent(
    st: &AppState,
    customer_id: String,
    channel: ConsentChannel,
    granted: bool,
    source: ConsentSource,
    note: Option<String>,
) -> Result<Consent, AppError> {
    if let Some(done) = forward(
        st,
        "privacy.set_consent",
        json!({
            "customerId": customer_id,
            "channel": channel,
            "granted": granted,
            "source": source,
            "note": note,
        }),
    )? {
        return Ok(done);
    }
    let ctx = st.till(Permission::CustomersView)?;
    st.db.write(move |tx| {
        privacy::set_consent(tx, &ctx, &customer_id, channel, granted, source, note)
    })
}

/// PDP-004: everything the shop keeps about the customer as `<code>-donnees.json`, beside an xlsx
/// of the documents naming him (`<code>-documents.xlsx`), in `folder`; one `privacy.exported`
/// journal row (purpose `export`).
pub(crate) fn export_customer(
    st: &AppState,
    customer_id: String,
    folder: &Path,
) -> Result<ExportedCustomer, AppError> {
    let session = st.require(Permission::CustomersManage)?;
    let id = customer_id.clone();
    let data = st.db.read(move |c| privacy::export_customer(c, &id))?;
    let rows = super::billing::search_all(
        st,
        ArchiveQuery {
            customer_id: Some(customer_id.clone()),
            ..ArchiveQuery::default()
        },
    )?;
    let table = archive::table(&rows)?;
    let lang = session.lang;
    let meta = SheetMeta {
        title: pos_docs::locales::text(lang.as_str(), "exports.documents").unwrap_or_default(),
        subtitle: Some(data.customer.code.clone()),
        lang,
        extra_labels: BTreeMap::new(),
    };
    let stem = file_stem(&data.customer.code);
    let json_path = folder.join(format!("{stem}-donnees.json"));
    let xlsx_path = folder.join(format!("{stem}-documents.xlsx"));
    std::fs::write(&json_path, serde_json::to_vec_pretty(&data)?)?;
    std::fs::write(&xlsx_path, sheet::render(&table, &meta, SheetFormat::Xlsx)?)?;
    let actor = session.actor();
    let documents = rows.len();
    st.db.write(move |tx| {
        pos_core::audit::append(
            tx,
            &actor.event(
                "privacy.exported",
                "customer",
                Some(&customer_id),
                json!({ "purpose": AccessPurpose::Export, "documents": documents }),
            ),
        )
        .map(drop)
    })?;
    Ok(ExportedCustomer {
        json_path: json_path.to_string_lossy().into_owned(),
        xlsx_path: xlsx_path.to_string_lossy().into_owned(),
    })
}

/// A customer's code as a file name: letters, digits and dashes only.
fn file_stem(code: &str) -> String {
    let stem: String = code
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    if stem.is_empty() {
        "client".into()
    } else {
        stem
    }
}

/// PDP-004: anonymised on request where the customer books live, a manager's act (or the
/// owner's, or with a manager's approval at the till).
pub(crate) fn anonymise(
    st: &AppState,
    customer_id: String,
    note: Option<String>,
) -> Result<AnonymiseReport, AppError> {
    if let Some(done) = forward_spent(
        st,
        "privacy.anonymise",
        json!({ "customerId": customer_id, "note": note }),
    )? {
        return Ok(done);
    }
    let ctx = st.till(Permission::CustomersManage)?;
    let write = ctx.clone();
    let report = st
        .db
        .write(move |tx| privacy::anonymise(tx, &write, &customer_id, note))?;
    st.spend_approvals(&ctx)?;
    Ok(report)
}

pub(crate) fn dpo(st: &AppState) -> Result<Option<Dpo>, AppError> {
    st.session()?;
    st.db.read(privacy::dpo)
}

/// PDP-005: the record of processing activities of the modules in use, with the DPO.
pub(crate) fn register(st: &AppState, lang: Lang) -> Result<HostPage, AppError> {
    st.require(Permission::SettingsCompany)?;
    let pack = st.datapack.clone();
    let today = pos_core::time::business_date(st.now())
        .format("%Y-%m-%d")
        .to_string();
    let print = st.db.read(move |c| {
        let seller = pos_core::billing::validate::seller_party(c, lang, None)?;
        let block = pos_docs::a4::document::party_block(&seller, &pack, lang);
        Ok(pos_docs::a4::packs::build_processing_register(
            &privacy::activities(c)?,
            block,
            privacy::dpo(c)?,
            lang,
            &today,
        ))
    })?;
    Ok(HostPage::ProcessingRegister(Box::new(print)))
}

pub(crate) fn print_register(
    st: &AppState,
    host: &PrintHost,
    lang: Lang,
) -> Result<A4Printed, AppError> {
    let page = register(st, lang)?;
    print_page(st, host, page)
}

/// UX-017: the till's keys, the defaults overridden by the owner's choices.
pub(crate) fn key_map(st: &AppState) -> Result<Vec<(String, String)>, AppError> {
    st.session()?;
    st.db.read(pos_core::till::key_map)
}

pub(crate) fn set_key_map(
    st: &AppState,
    map: BTreeMap<String, String>,
) -> Result<Vec<(String, String)>, AppError> {
    let actor = st.require(Permission::SettingsCompany)?.actor();
    st.db
        .write(move |tx| pos_core::till::set_key_map(tx, &actor, map))
}

/// This PC's folder of signed data packs (`<data>\datapacks`), read at every start.
pub(crate) fn datapack_dir(st: &AppState) -> PathBuf {
    Path::new(&st.paths.data_dir).join("datapacks")
}

pub(crate) fn installed(st: &AppState) -> Result<Vec<InstalledPack>, AppError> {
    installed_in(st, &datapack_dir(st), DATAPACK_PUBLIC_KEY)
}

/// The packs of `dir` signed with `public_key`, newest version first.
pub(crate) fn installed_in(
    st: &AppState,
    dir: &Path,
    public_key: &str,
) -> Result<Vec<InstalledPack>, AppError> {
    st.require(Permission::SettingsCompany)?;
    Ok(datapack_update::installed(dir, public_key))
}

/// PLT-024: verifies the pack in `json` against its signature `sig` with `public_key` (the
/// owner's, `DATAPACK_PUBLIC_KEY`, outside the tests) and installs it into `dir` (this PC's
/// [`datapack_dir`]) for the next start; journaled `datapack.installed`. Refused unless newer
/// than the pack in force.
pub(crate) fn install_files(
    st: &AppState,
    dir: &Path,
    json: &Path,
    sig: &Path,
    public_key: &str,
) -> Result<InstalledPack, AppError> {
    let actor = st.require(Permission::SettingsCompany)?.actor();
    let read = |path: &Path| {
        std::fs::read_to_string(path)
            .map_err(|_| AppError::validation("file", "datapack_file_invalid"))
    };
    let (json, sig) = (read(json)?, read(sig)?);
    let done = datapack_update::install(dir, &json, &sig, &st.datapack, public_key)?;
    let journaled = done.clone();
    st.db
        .write(move |tx| datapack_update::record_installed(tx, &actor, &journaled))?;
    Ok(done)
}

//! Thin IPC wrappers (spec §4.2): validate → `state.require(...)` → pos-core → mapped errors.

pub mod app;
pub mod audit;
pub mod auth;
pub mod backup;
pub mod billing;
pub mod butcher;
pub mod cash;
pub mod catalog;
pub mod cloud;
pub mod cloud_orders;
pub mod company;
pub mod completion;
pub mod credit;
pub mod customers;
pub mod data_files;
pub mod demo;
pub mod devices;
pub mod diagnostics;
pub mod display;
pub mod documents;
pub mod expenses;
pub mod exports;
pub mod fiscal;
pub mod hardware;
pub mod imports;
pub mod instalments;
pub mod jewellery;
pub mod label_sheets;
pub mod labels;
pub mod lan;
pub mod license;
pub mod loyalty;
pub mod orders;
pub mod packs;
pub mod pharmacy;
pub mod phones;
pub mod print;
pub mod print_host;
pub mod privacy;
pub mod product_extras;
pub mod products;
pub mod promotions;
pub mod purchases;
pub mod recovery;
pub(crate) mod redact;
pub mod reports;
pub mod restaurant;
pub mod returns;
pub mod sales;
pub mod services;
pub mod settings;
pub mod setup;
pub mod staff;
pub mod stock;
pub mod stock_m2;
pub mod suppliers;
#[cfg(test)]
pub(crate) mod tests;
pub mod updates;
pub mod users;
pub mod wholesale;

#[cfg(test)]
mod backup_tests;
#[cfg(test)]
mod catalog_tests;
#[cfg(test)]
pub(crate) mod cloud_mock;
#[cfg(test)]
mod cloud_orders_tests;
#[cfg(test)]
mod cloud_smoke_tests;
#[cfg(test)]
mod cloud_tests;
#[cfg(test)]
mod count_lots_tests;
#[cfg(test)]
mod demo_tests;
#[cfg(test)]
mod devices_tests;
#[cfg(test)]
mod diagnostics_tests;
#[cfg(test)]
mod dining_preview_tests;
#[cfg(test)]
mod display_tests;
#[cfg(test)]
mod label_sheets_tests;
#[cfg(test)]
mod labels_tests;
#[cfg(test)]
mod lan_join_tests;
#[cfg(test)]
mod lan_rules_tests;
#[cfg(test)]
mod license_tests;
#[cfg(test)]
mod lost_database_tests;
#[cfg(test)]
mod m2_month;
#[cfg(test)]
mod m2_tests;
#[cfg(test)]
mod m3_smoke;
#[cfg(test)]
mod m3_tests;
#[cfg(test)]
mod m4_polish_tests;
#[cfg(test)]
mod m4_smoke;
#[cfg(test)]
mod m8_flows_tests;
#[cfg(test)]
mod m8_more_tests;
#[cfg(test)]
mod m8_privacy_tests;
#[cfg(test)]
mod m8_screens_tests;
#[cfg(test)]
mod m8_smoke_tests;
#[cfg(test)]
mod m8_tests;
#[cfg(test)]
mod points_preview_tests;
#[cfg(test)]
mod print_tests;
#[cfg(test)]
mod printing_tests;
#[cfg(test)]
mod recovery_rejoin_tests;
#[cfg(test)]
mod recovery_tests;
#[cfg(test)]
mod sales_tests;
#[cfg(test)]
mod shift_tests;
#[cfg(test)]
pub(crate) mod testshop;
#[cfg(test)]
mod till_tests;
#[cfg(test)]
mod updates_launch_tests;
#[cfg(test)]
mod updates_tests;

use pos_core::error::AppError;
use pos_core::time::{business_date, now_ms};
use std::path::PathBuf;
use tauri::{AppHandle, Window};
use tauri_plugin_dialog::DialogExt;

/// Runs blocking work (SQLite, Argon2) off the async runtime threads.
pub async fn blocking<T, F>(f: F) -> Result<T, AppError>
where
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| AppError::Internal(format!("blocking task failed: {e}")))?
}

/// Today's business day, `YYYY-MM-DD` (file names).
pub(crate) fn today() -> String {
    business_date(now_ms()).format("%Y-%m-%d").to_string()
}

/// A Rust save dialog proposing `file_name`, filtered on `extensions` (the filter's name is its
/// extensions, the same in every language); `None` when the user cancels it. Blocking: call it
/// inside `blocking`.
pub(crate) fn save_dialog(
    app: &AppHandle,
    window: &Window,
    file_name: &str,
    extensions: &[&str],
) -> Result<Option<PathBuf>, AppError> {
    let picked = app
        .dialog()
        .file()
        .set_file_name(file_name)
        .add_filter(extensions.join(", "), extensions)
        .set_parent(window)
        .blocking_save_file();
    let Some(file) = picked else {
        return Ok(None);
    };
    file.into_path()
        .map(Some)
        .map_err(|_| AppError::Io("the chosen file cannot be written".into()))
}

/// A Rust open dialog filtered like [`save_dialog`]; `None` when the user cancels it.
pub(crate) fn pick_dialog(
    app: &AppHandle,
    window: &Window,
    filter: &str,
    extensions: &[&str],
) -> Result<Option<PathBuf>, AppError> {
    let picked = app
        .dialog()
        .file()
        .add_filter(filter, extensions)
        .set_parent(window)
        .blocking_pick_file();
    let Some(file) = picked else {
        return Ok(None);
    };
    file.into_path()
        .map(Some)
        .map_err(|_| AppError::Io("the chosen file cannot be read".into()))
}

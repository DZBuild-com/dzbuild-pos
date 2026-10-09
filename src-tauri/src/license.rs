//! This PC's licence in the running app: checked right after the database
//! opens (`AppState::open_with`), every 10 minutes (`spawn_refresher`), and in memory at every
//! command that is not in [`OPEN`] (`handle`, around tauri-specta's invoke handler). The
//! verification is `pos_license`'s; this file holds this PC's stores, the high-water clock, the
//! seller's contact and the list of commands read-only mode keeps.

use crate::state::{AppState, SharedState};
use pos_core::audit::Actor;
use pos_core::cloud::secrets::SecretStore;
use pos_core::db::Db;
use pos_core::error::AppError;
use pos_license::codes;
use pos_license::device::DeviceId;
use pos_license::engine::{self, Context};
use pos_license::keys::Keyring;
use pos_license::payload::LicenseKind;
use pos_license::state::{Licence, LicenseState};
use pos_license::store::FileStore;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tauri::Manager;

pub const REFRESH_EVERY: Duration = Duration::from_secs(600);

/// A store Windows will not open this run: left out of the check (never judged, never repaired).
#[derive(Debug)]
struct Unavailable;

impl SecretStore for Unavailable {
    fn get(&self, _: &str) -> Result<Option<String>, AppError> {
        Err(AppError::sync("cloud_secret_store_failed"))
    }
    fn set(&self, _: &str, _: &str) -> Result<(), AppError> {
        Err(AppError::sync("cloud_secret_store_failed"))
    }
    fn delete(&self, _: &str) -> Result<(), AppError> {
        Ok(())
    }
}

/// Where this PC keeps its licence and which keys it trusts.
pub struct LicenseEnv {
    pub device: Result<DeviceId, AppError>,
    pub vault: Arc<dyn SecretStore>,
    pub file: Arc<dyn SecretStore>,
    pub keyring: Keyring,
    /// The free edition (0.9.0, `license.toml` `free = true`): every PC writes with no key, no
    /// store is read and nothing about licences is journaled.
    pub free: bool,
}

impl LicenseEnv {
    /// This PC: its identity from Windows, its Credential Manager, `%LOCALAPPDATA%\dev.dzbuild.pos`,
    /// the keys of this build.
    pub fn system() -> LicenseEnv {
        let device = DeviceId::this_pc();
        if let Err(error) = &device {
            tracing::error!(target: "license", code = %error.code(), "this PC's identity could not be read");
        }
        let file: Arc<dyn SecretStore> = match FileStore::this_pc() {
            Some(store) => Arc::new(store),
            None => Arc::new(Unavailable),
        };
        LicenseEnv {
            device,
            vault: credential_manager(),
            file,
            keyring: Keyring::embedded(),
            free: free_edition(),
        }
    }

    /// [`for_tests`](Self::for_tests) as the free edition.
    #[cfg(test)]
    pub fn for_tests_free() -> LicenseEnv {
        LicenseEnv {
            free: true,
            ..LicenseEnv::for_tests()
        }
    }

    /// The test device and key, stores in memory.
    #[cfg(test)]
    pub fn for_tests() -> LicenseEnv {
        LicenseEnv {
            device: Ok(pos_license::testkit::device()),
            vault: Arc::new(pos_core::cloud::secrets::MemorySecrets::default()),
            file: Arc::new(pos_core::cloud::secrets::MemorySecrets::default()),
            keyring: pos_license::testkit::keyring(),
            free: false,
        }
    }

    /// [`for_tests`](Self::for_tests) with the same in-memory stores for every start of the shop in
    /// `data_dir` (a test that reopens a file database keeps "this PC's" stores; nothing is written
    /// beside the test's files).
    #[cfg(test)]
    pub fn for_tests_in(data_dir: &std::path::Path) -> LicenseEnv {
        use pos_core::cloud::secrets::MemorySecrets;
        use std::collections::HashMap;
        use std::path::PathBuf;
        use std::sync::{LazyLock, Mutex, PoisonError};
        type Stores = (Arc<MemorySecrets>, Arc<MemorySecrets>);
        static STORES: LazyLock<Mutex<HashMap<PathBuf, Stores>>> =
            LazyLock::new(|| Mutex::new(HashMap::new()));
        let (vault, file) = STORES
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(data_dir.to_path_buf())
            .or_default()
            .clone();
        LicenseEnv {
            device: Ok(pos_license::testkit::device()),
            vault,
            file,
            keyring: pos_license::testkit::keyring(),
            free: false,
        }
    }
}

#[cfg(windows)]
fn credential_manager() -> Arc<dyn SecretStore> {
    match pos_core::cloud::secrets::CredentialManager::open() {
        Ok(store) => Arc::new(store),
        Err(_) => {
            tracing::warn!(target: "license", "the Credential Manager is unavailable this run");
            Arc::new(Unavailable)
        }
    }
}

#[cfg(not(windows))]
fn credential_manager() -> Arc<dyn SecretStore> {
    Arc::new(Unavailable)
}

/// The licence of this PC while the app runs. Never fails the start: an error leaves this PC
/// unlicensed (read-only mode, the books open), logged.
pub struct LicenseRuntime {
    env: LicenseEnv,
    demo: bool,
    state: RwLock<LicenseState>,
    high_water: AtomicI64,
    /// `license.tampered` is journaled once per run.
    tamper_journaled: AtomicBool,
}

impl LicenseRuntime {
    pub fn open(db: &Db, env: LicenseEnv, demo: bool, actor: &Actor, now: i64) -> LicenseRuntime {
        let runtime = LicenseRuntime {
            env,
            demo,
            state: RwLock::new(LicenseState::Unlicensed),
            high_water: AtomicI64::new(now),
            tamper_journaled: AtomicBool::new(false),
        };
        runtime.refresh(db, actor, now);
        runtime
    }

    pub fn device_code(&self) -> Option<&str> {
        self.env.device.as_ref().ok().map(DeviceId::code)
    }

    /// The newest instant this PC showed, `now` included.
    pub fn high_water(&self, now: i64) -> i64 {
        self.high_water.fetch_max(now, Ordering::SeqCst).max(now)
    }

    /// The state at `now`: an active trial past its end reads as expired at once (the next
    /// 10-minute check journals it), and a trial whose clock is now set back behind the newest
    /// time this PC showed refuses writes until the date is set (`license_clock_behind`, final
    /// review): `now` is compared with the high-water as it stood before it.
    pub fn state(&self, now: i64) -> LicenseState {
        if self.demo {
            return LicenseState::Demo;
        }
        if self.env.free {
            return LicenseState::Active(free_licence());
        }
        let high_water = self.high_water(now);
        self.current().at_clock(now, high_water)
    }

    /// The gate's question: may a write run now? No store is read here (the sale budget).
    pub fn check(&self, now: i64) -> Result<(), AppError> {
        self.state(now).refusal().map_or(Ok(()), Err)
    }

    fn current(&self) -> LicenseState {
        match self.state.read() {
            Ok(state) => state.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    fn set(&self, state: LicenseState) {
        match self.state.write() {
            Ok(mut slot) => *slot = state,
            Err(poisoned) => *poisoned.into_inner() = state,
        }
    }

    fn context<'a>(
        &'a self,
        db: &'a Db,
        device: &'a DeviceId,
        actor: &'a Actor,
        now: i64,
    ) -> Context<'a> {
        Context {
            db,
            vault: self.env.vault.as_ref(),
            file: self.env.file.as_ref(),
            device,
            keyring: &self.env.keyring,
            actor,
            now,
        }
    }

    /// The full check (the start, every 10 minutes): the stores reconciled, repairs and the trial's
    /// end journaled. A failure keeps the last state.
    pub fn refresh(&self, db: &Db, actor: &Actor, now: i64) {
        if self.demo {
            self.set(LicenseState::Demo);
            return;
        }
        if self.env.free {
            // The free edition: no store read, nothing journaled, nothing repaired.
            self.set(LicenseState::Active(free_licence()));
            return;
        }
        let Ok(device) = &self.env.device else {
            self.set(LicenseState::DeviceUnreadable);
            return;
        };
        let journal_tamper = !self.tamper_journaled.load(Ordering::SeqCst);
        match engine::refresh(&self.context(db, device, actor, now), journal_tamper) {
            Ok(outcome) => {
                if outcome.state == LicenseState::Tampered {
                    self.tamper_journaled.store(true, Ordering::SeqCst);
                }
                self.high_water
                    .fetch_max(outcome.high_water, Ordering::SeqCst);
                self.set(outcome.state);
            }
            Err(error) => {
                tracing::error!(target: "license", code = %error.code(), "the licence check failed; the last state stays");
            }
        }
    }

    pub fn activate(
        &self,
        db: &Db,
        actor: &Actor,
        now: i64,
        text: &str,
    ) -> Result<LicenseState, AppError> {
        if self.demo {
            return Err(pos_core::demo::not_allowed());
        }
        if self.env.free {
            return Ok(self.state(now));
        }
        let device = self.env.device.as_ref().map_err(Clone::clone)?;
        let outcome = engine::activate(&self.context(db, device, actor, now), text)?;
        self.tamper_journaled.store(false, Ordering::SeqCst);
        self.high_water
            .fetch_max(outcome.high_water, Ordering::SeqCst);
        self.set(outcome.state.clone());
        Ok(outcome.state)
    }

    pub fn remove(&self, db: &Db, actor: &Actor, now: i64) -> Result<LicenseState, AppError> {
        if self.demo {
            return Err(pos_core::demo::not_allowed());
        }
        if self.env.free {
            return Ok(self.state(now));
        }
        let device = self.env.device.as_ref().map_err(Clone::clone)?;
        let outcome = engine::remove(&self.context(db, device, actor, now))?;
        self.high_water
            .fetch_max(outcome.high_water, Ordering::SeqCst);
        self.set(outcome.state.clone());
        Ok(outcome.state)
    }
}

/// The 10-minute check, on a blocking thread (the database, the Credential Manager).
pub fn spawn_refresher(state: SharedState) {
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(REFRESH_EVERY);
        tick.tick().await; // the first tick is immediate: the start has just checked
        loop {
            tick.tick().await;
            let st = state.clone();
            let ran = tauri::async_runtime::spawn_blocking(move || {
                let actor = Actor::system(Some(st.terminal.id.clone()));
                st.license.refresh(&st.db, &actor, st.now());
            })
            .await;
            if let Err(error) = ran {
                tracing::warn!(target: "license", %error, "the 10-minute licence check did not run");
            }
        }
    });
}

/// Commands that work whatever the licence says: the start and the fatal screen, sign-in
/// and who may sign in, reads and previews that write nothing, backups and restores, exports and
/// reprints of existing documents, the fiscal archive, the Z of a cash session already open, the
/// network's and DZBuild's sync of what is already done, this PC's printers, scale and displays
/// (the books must still print), updates, diagnostics, the demo shop. Every other command needs a
/// licence that allows writes (or the demo shop).
///
/// **Default deny**: the gate lets through only what is listed here. Every command of
/// `crate::COMMANDS` is classified on purpose in exactly one of [`OPEN`] and [`NEVER_OPEN`]
/// (`license_tests::every_command_is_classified_in_exactly_one_list`); whoever adds a command lists
/// it here when it writes nothing to the
/// shop's books or settings, else in [`NEVER_OPEN`].
pub const OPEN: &[&str] = &[
    // the app, the start and the fatal screen
    "app_ping",
    "app_info",
    "app_data_dir",
    "app_startup_error",
    "app_startup_notices",
    "app_open_folder",
    "app_ready",
    "app_open_link",
    "datapack_get",
    "setup_status",
    "setup_import_backup",
    "recovery_list",
    "recovery_inspect",
    "recovery_restore",
    "recovery_rejoin",
    "recovery_start_fresh",
    // sign-in, and who may sign in: a manager's approval (the Z of a session with a variance
    // needs one), a locked-out user's new PIN, a departed employee switched off
    "auth_list_users",
    "auth_login",
    "auth_logout",
    "auth_session",
    "auth_set_language",
    "auth_manager_override",
    "users_set_pin",
    "users_deactivate",
    // the shop and the journal, read
    "company_get",
    "settings_get",
    "users_list",
    "roles_list",
    "roles_catalog",
    "audit_list",
    "audit_verify_chain",
    "audit_history",
    // backups, restores and the data's exports: the books stay safe
    "backup_now",
    "backup_list",
    "backup_status",
    "backup_restore",
    "backup_mirror_status",
    "backup_mirror_choose",
    "backup_mirror_clear",
    "backup_drives",
    "backup_to_drive",
    "backup_inspect",
    "backup_check_database",
    "backup_export_file",
    "backup_import_file",
    "data_export_all",
    // the catalogue and the stock, read
    "catalog_units_list",
    "catalog_categories_list",
    "catalog_tax_rates_list",
    "catalog_price_levels_list",
    "products_get",
    "products_search",
    "products_find_by_code",
    "products_grid",
    "products_catalog",
    "products_variants",
    "products_extras_get",
    "products_breaks_list",
    "products_price_help",
    "products_bulk_price_preview",
    "stock_locations",
    "stock_levels",
    "stock_movements",
    "stock_low",
    "stock_levels_view",
    "stock_movement_log",
    "stock_minimums_get",
    "stock_transfers",
    "lots_of_product",
    "lots_expiry",
    "serials_list",
    "serials_find",
    "counts_list",
    "counts_lines",
    "scale_items_list",
    // this PC's printers, scale and displays: their settings, tests and readings (the books must
    // still print); a receipt that failed to print, printed again
    "hardware_get",
    "hardware_save",
    "hardware_discover",
    "hardware_printer_status",
    "hardware_doctor",
    "hardware_test_print",
    "hardware_test_label",
    "hardware_retry_print",
    "devices_status",
    "display_current",
    "display_shop",
    "display_monitors",
    "display_show",
    "display_test_vfd",
    "display_open_preview",
    "display_close",
    "scale_latest",
    "scale_read_stable",
    "scale_probe",
    "scale_tare",
    "scale_clear_tare",
    "scale_zero",
    "scale_plu_export",
    // documents already made: read, verified, reprinted, printed, exported; previews that write
    // nothing
    "documents_list",
    "documents_get",
    "documents_find",
    "documents_reprint",
    "documents_verify_chain",
    "documents_search",
    "documents_state",
    "documents_links",
    "documents_mentions",
    "documents_warranty_lines",
    "documents_print",
    "documents_export_pdf",
    "documents_print_model",
    "documents_preview_draft",
    "documents_draft_cart",
    "documents_offer_cart",
    "documents_payment_preview",
    "documents_deliveries_preview",
    "documents_creditable",
    "documents_credit_quote",
    "documents_arabic_guard",
    "statements_print",
    "statements_export_pdf",
    "warranty_print",
    "warranty_export_pdf",
    "exports_pdf",
    "exports_save",
    "print_host_next",
    "print_host_rendered",
    // the till's previews, which write nothing
    "sales_preview",
    "returns_returnable",
    "returns_quote",
    "credit_quote",
    // the cash: read, and the Z of a session already open (a trial may end mid-shift)
    "cash_current",
    "cash_movements",
    "cash_x_report",
    "cash_z_report",
    "cash_sessions",
    "cash_verify_closures",
    "cash_print_z",
    "cash_print_x",
    "cash_close_session",
    "cash_close_offline",
    "sales_held",
    // customers, suppliers, purchases, promotions, expenses, imports: read
    "customers_search",
    "customers_get",
    "customers_find_by_phone",
    "customers_documents",
    "credit_entries",
    "credit_open_items",
    "credit_statement",
    "credit_print_statement",
    "credit_aging",
    "credit_receivables",
    "credit_verify_receipts",
    "credit_reprint_receipt",
    "suppliers_search",
    "suppliers_get",
    "suppliers_entries",
    "suppliers_open_items",
    "suppliers_statement",
    "suppliers_aging",
    "suppliers_payables",
    "purchases_list",
    "purchases_get",
    "purchases_remaining",
    "purchases_cost_history",
    "promotions_list",
    "promotions_get",
    "promotions_presets",
    "promotions_declaration",
    "expenses_categories",
    "expenses_list",
    "expenses_totals",
    "imports_jobs",
    // an import's file, preview, dry run and reports, which keep nothing (`imports_apply` does)
    "imports_open_file",
    "imports_preview",
    "imports_dry_run",
    "imports_discard",
    "imports_error_report",
    "imports_template",
    // reports and the fiscal archive
    "reports_catalog",
    "reports_run",
    "reports_dashboard",
    "fiscal_g50",
    "fiscal_export_archive",
    "fiscal_verify_archive",
    // labels, read and laid out (printing them is journaled: never open)
    "labels_templates",
    "labels_stale",
    "labels_build",
    "labels_preview",
    "labels_sheet",
    // the shop's network: its state, sending what is already done, a pairing called off and a
    // till revoked (a stolen or replaced PC)
    "lan_status",
    "lan_sync_now",
    "lan_tills",
    "lan_till_closures",
    "lan_verify_journals",
    "lan_notices",
    "lan_acknowledge_notice",
    "lan_retry_rejected",
    // Past tickets' entries reaching the main PC (no new sale, like the outbox)
    "lan_pending_entries",
    "lan_pending_retry",
    "lan_cancel_pairing",
    "lan_revoke_till",
    "lan_firewall",
    "lan_discover",
    "lan_search_host",
    "lan_confirm_close",
    // the DZBuild link: its state, sending what is already done, a link called off or ended
    // (the owner's data never waits for a licence)
    "cloud_status",
    "cloud_sync_now",
    "cloud_notices",
    "cloud_notice_dismiss",
    "cloud_outbox_failed",
    "cloud_outbox_retry",
    "cloud_stock_reconcile",
    "cloud_publish_list",
    "cloud_devices",
    "cloud_open_page",
    "cloud_link_cancel",
    "cloud_unlink",
    "cloud_orders_list",
    "cloud_orders_get",
    "cloud_orders_seen",
    "cloud_orders_quote",
    "cloud_customer_flag",
    // updates, diagnostics, support, this PC's display
    "updates_status",
    "updates_check",
    "updates_download",
    "updates_install",
    "diagnostics_info",
    "diagnostics_export",
    "diagnostics_set_log_level",
    "app_last_crash",
    "support_contact",
    "support_open",
    "display_get",
    "display_set",
    // the demo shop (it needs no licence)
    "demo_status",
    "demo_enter",
    "demo_leave",
    "demo_erase",
    // the licence itself
    "license_status",
    "license_activate",
    "license_import_file",
    "license_remove",
    // M8 packs: reads, lists, quotes and previews, the prints of what is
    // already written (contracts, route sheets, zakat statements, a voucher's slip, a receipt as a
    // page), a customer's data exported, and the restart that applies an installed data pack
    "app_restart",
    "serials_available",
    "flexy_operators",
    "flexy_line",
    "flexy_balances",
    "flexy_entries",
    "deposits_quote",
    "deposits_list",
    "vouchers_describe",
    "service_get",
    "service_board",
    "service_ready_message",
    "service_uncollected",
    "instalments_quote",
    "instalments_contract",
    "instalments_contract_print",
    "instalments_get",
    "instalments_list",
    "instalments_reminders",
    "instalments_reminder_text",
    "wholesale_prices",
    "wholesale_min_order",
    "rounds_list",
    "rounds_get",
    "rounds_close_preview",
    "rounds_route_sheet",
    "rounds_route_sheet_print",
    "orders_get",
    "orders_list",
    "orders_fulfil_cart",
    "appointments_day",
    "appointments_range",
    "loyalty_card_lookup",
    "loyalty_account",
    "loyalty_entries",
    "vouchers_quote_sale",
    "vouchers_get",
    "vouchers_list",
    "vouchers_reprint",
    "jewellery_piece_get",
    "metal_board",
    "metal_history",
    "jeweller_register_entries",
    "staff_list",
    "staff_entries",
    "staff_shifts",
    "zakat_preview",
    "zakat_list",
    "zakat_statement",
    "payment_types_list",
    "variant_grid",
    "payments_baridpay_qr",
    "documents_print_receipt_roll",
    // M8 Task 21b: personal data read, exported and registered; the key map and data packs read
    "privacy_consents",
    "privacy_export_customer",
    "privacy_dpo",
    "privacy_register",
    "till_key_map",
    "datapack_installed",
    // M8: the pack state and templates, the floor, its tickets and menus read,
    // the previews that write nothing (a bill, a cutting run, the points a ticket earns), the
    // pharmacy and butcher reads, the controlled register read, a kitchen ticket printed again
    "sales_points_preview",
    "packs_templates",
    "packs_state",
    "regulated_board",
    "departments_list",
    "dining_rooms",
    "dining_tables",
    "dining_board",
    "dining_counter",
    "daily_menu_get",
    "modifier_groups",
    "modifiers_for_product",
    "kitchen_stations",
    "dining_ticket_get",
    "dining_ticket_preview",
    "kitchen_reprint",
    "pharmacy_product_get",
    "pharmacy_equivalents",
    "pharmacy_expiry_board",
    "chifa_check",
    "chifa_list",
    "chifa_of_product",
    "controlled_register_entries",
    "yield_templates",
    "yield_preview",
    "yield_runs",
    "waste_day",
    // M8 (STK-004): a count's lot lines read
    "counts_lot_lines",
];

/// Never in [`OPEN`]: the writes that sell, move money or stock, issue or change documents, change
/// the shop (its catalogue, customers, suppliers, people, settings, labels), or bring this PC into
/// or out of a shop's network or DZBuild link. Listed so that every command is classified on
/// purpose (see [`OPEN`]); the gate itself only reads [`OPEN`].
pub const NEVER_OPEN: &[&str] = &[
    // the first set-up (behind the activation screen)
    "setup_complete_first_run",
    // selling, returns, documents
    "sales_create",
    "sales_hold",
    "sales_recall",
    "sales_discard_held",
    "sales_record_event",
    "returns_create",
    "returns_exchange",
    "documents_save_draft",
    "documents_cancel_draft",
    "documents_validate",
    "documents_convert",
    "documents_invoice_ticket",
    "documents_invoice_deliveries",
    "documents_link_sale",
    "documents_create_credit_note",
    // money: the cash, accounts, payments, expenses
    "cash_open_session",
    "cash_move",
    "cash_expense",
    "credit_collect",
    "credit_set_opening",
    "credit_adjust",
    "credit_rebuild_balances",
    "suppliers_pay",
    "suppliers_set_opening",
    "suppliers_adjust",
    "expenses_record",
    "expenses_reverse",
    "expenses_category_create",
    "expenses_category_update",
    "expenses_category_archive",
    "hardware_open_drawer",
    // purchases and the stock
    "purchases_save_order",
    "purchases_validate_order",
    "purchases_cancel_order",
    "purchases_receive",
    "purchases_record_invoice",
    "purchases_return",
    "stock_adjust",
    "stock_adjust_batch",
    "stock_transfer",
    "stock_create_location",
    "stock_minimums_set",
    "stock_rebuild_levels",
    "counts_start",
    "counts_set",
    "counts_set_lot",
    "counts_scan",
    "counts_close",
    "counts_cancel",
    "counts_import_collector",
    "serials_register",
    "serials_link_sale",
    // imports
    "imports_apply",
    "imports_undo",
    // the catalogue
    "catalog_units_create",
    "catalog_units_update",
    "catalog_categories_create",
    "catalog_categories_update",
    "catalog_categories_archive",
    "catalog_tax_rates_update",
    "catalog_price_levels_create",
    "catalog_price_levels_update",
    "catalog_price_levels_set_default",
    "catalog_price_levels_archive",
    "products_create",
    "products_update",
    "products_archive",
    "products_restore",
    "products_duplicate",
    "products_set_price",
    "products_bulk_price_apply",
    "products_bulk_edit",
    "products_extras_set",
    "products_breaks_replace",
    "products_variants_generate",
    "products_next_ean13",
    "products_next_reference",
    "products_pick_image",
    "products_clear_image",
    "products_import_images",
    "scale_items_set",
    "promotions_create",
    "promotions_update",
    "promotions_archive",
    // labels printed (journaled) and their settings
    "labels_print",
    "labels_print_weighed",
    "labels_sheet_print",
    "labels_sheet_export_pdf",
    "labels_dismiss",
    "labels_save_templates",
    // customers and suppliers
    "customers_create",
    "customers_quick_create",
    "customers_update",
    "customers_set_blocked",
    "customers_archive",
    "customers_restore",
    "suppliers_create",
    "suppliers_update",
    "suppliers_archive",
    "suppliers_restore",
    // the shop, its people and its settings
    "company_update",
    "company_set_logo",
    "settings_set",
    "users_create",
    "users_update",
    "roles_create",
    "roles_update",
    "roles_delete",
    // the shop's network: joining, pairing, changing roles, leaving
    "lan_join",
    "lan_probe",
    "lan_pairing_code",
    "lan_restart",
    "lan_become_host",
    "lan_become_standalone",
    "lan_promote",
    "lan_leave",
    // the DZBuild link: linking, publishing, importing, online orders
    "cloud_link_start",
    "cloud_device_flow_start",
    "cloud_link_poll",
    "cloud_link_choose",
    "cloud_publish_set",
    "cloud_import_online",
    "cloud_orders_claim",
    "cloud_orders_release",
    "cloud_orders_prepare",
    "cloud_orders_sell",
    "cloud_orders_hand_over",
    "cloud_orders_ship",
    "cloud_orders_refuse",
    // M8 packs: every money, stock, document and register write — the float, deposits
    // and repairs, instalments, wholesale prices and packs, delivery rounds, customer orders and
    // appointments, loyalty and vouchers, jewellery, staff advances, zakat statements, payment types,
    // role and user templates
    "flexy_purchase",
    "deposits_take",
    "deposits_refund",
    "service_intake",
    "service_set_status",
    "service_assign",
    "service_set_items",
    "service_pickup",
    "instalments_sign",
    "instalments_collect",
    "instalments_note_reminder",
    "instalments_add_guarantee",
    "wholesale_price_set",
    "wholesale_min_order_set",
    "wholesale_pack_create",
    "rounds_create",
    "rounds_set_stops",
    "rounds_start",
    "rounds_report_stop",
    "rounds_close",
    "rounds_create_van",
    "rounds_load",
    "rounds_unload",
    "orders_create",
    "orders_pay_in",
    "orders_set_ready",
    "orders_fulfil",
    "orders_deliver",
    "orders_close",
    "orders_keep_deposit",
    "appointments_book",
    "appointments_update",
    "appointments_set_status",
    "loyalty_enrol",
    "loyalty_block",
    "loyalty_redeem",
    "loyalty_adjust",
    "vouchers_sell",
    "jewellery_piece_set",
    "metal_price_set",
    "jewellery_reprice",
    "staff_profile_set",
    "staff_advance",
    "staff_repay",
    "staff_deduct",
    "zakat_save",
    "payment_type_save",
    "payment_type_archive",
    "roles_duplicate",
    "users_duplicate",
    // M8 Task 21b: a customer's consents and anonymisation, the till's keys, a data pack installed
    "privacy_set_consent",
    "privacy_anonymise",
    "till_set_key_map",
    "datapack_install",
    // M8: the activity and modules, the shop's records of the packs (regulated prices,
    // department keys, rooms, tables, menus, modifiers, stations, medicines, the Chifa list,
    // cutting templates), every table ticket write (the addition included) and its payment, a
    // cutting run and the waste booked
    "packs_apply_template",
    "packs_set_modules",
    "packs_reset_modules",
    "packs_set_till_mode",
    "regulated_set",
    "regulated_seed",
    "departments_save",
    "departments_archive",
    "dining_room_save",
    "dining_room_archive",
    "dining_table_save",
    "dining_table_archive",
    "daily_menu_set",
    "modifier_group_save",
    "modifier_group_archive",
    "modifiers_set_product",
    "kitchen_station_save",
    "kitchen_station_archive",
    "dining_ticket_open",
    "dining_ticket_add_line",
    "dining_ticket_set_qty",
    "dining_ticket_remove_unsent",
    "dining_ticket_edit_line",
    "dining_ticket_set_details",
    "dining_ticket_transfer",
    "dining_ticket_merge",
    "dining_ticket_send",
    "dining_ticket_void_line",
    "dining_ticket_void",
    "dining_ticket_comp_line",
    "dining_ticket_prebill",
    "dining_ticket_split",
    "dining_ticket_pay",
    "pharmacy_product_set",
    "chifa_apply",
    "yield_template_save",
    "yield_template_archive",
    "yield_run",
    "waste_record",
];

pub fn is_open(command: &str) -> bool {
    OPEN.contains(&command)
}

/// The gate's verdict for `command`: open commands always pass; the others pass when this PC's
/// licence allows writes. Before the database opened there is no licence to ask: refused.
pub fn verdict(command: &str, state: Option<&AppState>) -> Result<(), AppError> {
    if is_open(command) {
        return Ok(());
    }
    match state {
        Some(st) => st.license.check(st.now()),
        None => Err(AppError::conflict(codes::REQUIRED)),
    }
}

/// Wraps the commands' invoke handler: a refused command answers its licence error without
/// running (the webview gets `{ code: "license_expired" }` like any command error).
pub fn handle<R: tauri::Runtime>(
    invoke: tauri::ipc::Invoke<R>,
    inner: &dyn Fn(tauri::ipc::Invoke<R>) -> bool,
) -> bool {
    let refused = {
        let state = invoke
            .message
            .webview_ref()
            .app_handle()
            .try_state::<SharedState>();
        verdict(
            invoke.message.command(),
            state.as_ref().map(|s| s.inner().as_ref()),
        )
    };
    match refused {
        Ok(()) => inner(invoke),
        Err(error) => {
            invoke.resolver.reject(error);
            true
        }
    }
}

/// Who sells the licences (bundled `license.toml`). Unknown keys are allowed: the same file
/// carries the edition switch (`free`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LicenseSeller {
    pub seller: String,
    /// International form, digits only; empty when there is none.
    pub whatsapp: String,
    pub phone: String,
    pub url: String,
}

const SELLER: &str = include_str!("../license.toml");

/// The edition switch of `license.toml` (0.9.0): `free = true` licenses every PC with no key.
#[derive(Deserialize)]
struct Edition {
    #[serde(default)]
    free: bool,
}

/// Whether this build is the free edition (`license.toml` `free = true`): no key, no activation
/// screen, no read-only mode. A file that does not parse is not free (the licence is checked).
pub fn free_edition() -> bool {
    toml::from_str::<Edition>(SELLER).is_ok_and(|e| e.free)
}

/// The lifetime licence every PC of the free edition holds: never stored, never signed, never
/// expiring; the screens show it as an active lifetime licence with no customer.
pub fn free_licence() -> Licence {
    Licence {
        id: "free".into(),
        kind: LicenseKind::Lifetime,
        // 2026-10-08, the day of 0.9.0.
        issued_at: 1_791_417_600_000,
        expires_at: None,
        customer: None,
        activated_at: None,
    }
}

pub fn seller() -> LicenseSeller {
    toml::from_str(SELLER).unwrap_or_else(|error: toml::de::Error| {
        tracing::error!(target: "license", span = ?error.span(), "license.toml does not parse");
        LicenseSeller {
            seller: "DZBuild".into(),
            whatsapp: String::new(),
            phone: String::new(),
            url: "https://dzbuild.com/pos".into(),
        }
    })
}

/// What the activation QR holds: a WhatsApp message to the seller with this PC's code, or the code.
pub fn qr_text(seller: &LicenseSeller, device_code: &str) -> String {
    if seller.whatsapp.is_empty() {
        device_code.to_owned()
    } else {
        format!(
            "https://wa.me/{}?text=DZBuild%20POS%20{device_code}",
            seller.whatsapp
        )
    }
}

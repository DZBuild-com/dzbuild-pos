//! The single error type crossing every boundary (spec §4.4).

use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Value, json};
use specta::Type;

/// What the webview receives: `{ code, message?, field?, details? }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct ErrorPayload {
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional, type = Option<crate::json::JsonValue>)]
    pub details: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error, Type)]
#[specta(type = ErrorPayload)]
pub enum AppError {
    #[error("{entity} not found")]
    NotFound { entity: String },
    #[error("invalid {field}: {code}")]
    Validation { field: String, code: String },
    #[error("missing permission {permission}")]
    Permission { permission: String },
    #[error("authentication failed: {code}")]
    Auth { code: String },
    #[error("conflict: {code}")]
    Conflict { code: String },
    /// A business rule refused the operation and the UI needs the specifics (which products
    /// lack stock, …) in `details`.
    #[error("rejected: {code}")]
    Rejected { code: String, details: Value },
    #[error("database error: {0}")]
    Db(String),
    #[error("i/o error: {0}")]
    Io(String),
    #[error("hardware error: {code}")]
    Hardware { code: String },
    #[error("sync error: {code}")]
    Sync { code: String },
    #[error("internal error: {0}")]
    Internal(String),
}

pub type AppResult<T> = Result<T, AppError>;

/// Every code the backend can emit. `src/locales/*/errors.json` must translate each one
/// (checked by `src-tauri` contract tests); `every_emitted_code_is_known` scans the sources.
pub const KNOWN_CODES: &[&str] = &[
    // infrastructure
    "internal_error",
    "database_error",
    "database_busy",
    "database_corrupt",
    "database_newer_than_app",
    "io_error",
    "constraint_violation",
    "datapack_invalid",
    "backup_dir_missing",
    "backup_invalid",
    "memory_database",
    // an interrupted restore left the database's WAL set aside: do not open normally
    "restore_needs_attention",
    // not found
    "record_not_found",
    "user_not_found",
    "role_not_found",
    "backup_not_found",
    // authentication and permissions
    "insufficient_permission",
    "session_required",
    // the session's user was deactivated or removed: sign in again
    "session_invalid",
    "invalid_credentials",
    "invalid_pin",
    "user_inactive",
    "too_many_attempts",
    "approver_lacks_permission",
    // nobody approves their own request (PRM-004: a capped discount limit stays capped)
    "override_self_not_allowed",
    "owner_only",
    // business rules
    "username_taken",
    "last_owner",
    "cannot_deactivate_self",
    "owner_role_locked",
    "system_role_locked",
    "role_in_use",
    "role_name_taken",
    "setup_already_done",
    // field validation
    "required",
    "too_long",
    "pin_format",
    "pin_too_simple",
    "nif_invalid",
    "nis_invalid",
    "email_invalid",
    "wilaya_invalid",
    "amount_invalid",
    "discount_limit_invalid",
    "username_invalid",
    "setting_unknown",
    "value_invalid",
    "logo_invalid",
    "logo_too_large",
    // the price mode (TTC/HT) is fixed once a product has a price or a document is numbered
    "price_mode_locked",
    // M1 catalogue
    "unit_not_found",
    "tax_rate_not_found",
    "price_level_not_found",
    "category_not_found",
    "code_taken",
    "unit_in_use",
    "price_level_is_default",
    "category_cycle",
    "category_has_children",
    // M1 products
    "product_not_found",
    "reference_taken",
    "barcode_taken",
    "barcode_invalid",
    "plu_taken",
    "qty_invalid",
    "unit_not_weighable",
    "variant_parent_required",
    "components_required",
    "component_invalid",
    "tax_rate_inactive",
    "product_not_sellable",
    "product_kind_locked",
    // M1 TVA
    "discount_invalid",
    // M1 tenders and timbre
    "payment_method_invalid",
    // no longer emitted since M2 sells on account; kept for the bindings and old journals
    "customers_not_available",
    "cheque_details_required",
    "transfer_reference_required",
    "non_cash_exceeds_total",
    "payment_insufficient",
    // M1 stock
    "stock_insufficient",
    "stock_location_not_found",
    "product_not_stocked",
    // a product holding stock is neither archived nor untracked; an archived one takes no stock
    "stock_not_zero",
    "product_archived",
    // M1 documents
    "document_not_found",
    "document_already_validated",
    "document_empty",
    "document_not_validated",
    // M1 cash sessions
    "cash_session_not_found",
    "cash_session_closed",
    "cash_session_already_open",
    // M1 sales
    "setup_required",
    "cart_empty",
    "price_override_not_allowed",
    "free_line_not_allowed",
    "discount_not_allowed",
    "discount_above_limit",
    // M1 held tickets
    "held_cart_not_found",
    // M1 returns
    "return_original_invalid",
    "return_line_invalid",
    "return_qty_exceeds_sold",
    "return_without_receipt_not_allowed",
    // M1 cash closing
    "variance_needs_approval",
    "count_invalid",
    // M1b printing: only a receipt whose first print failed can be printed again as the original
    "print_not_pending",
    // M2 parties and ledgers
    "phone_invalid",
    "nin_invalid",
    "date_invalid",
    "opening_already_set",
    "allocation_invalid",
    // `ledger::{get, balance}`: `not_found("ledger_entry" | "customer" | "supplier")`
    "ledger_entry_not_found",
    "customer_not_found",
    "supplier_not_found",
    // M2 customers (`customer_not_found` is registered above with the ledgers)
    "phone_taken",
    "balance_not_zero",
    // M2 sales on account
    "customer_required",
    "customer_blocked",
    "credit_limit_exceeded",
    "store_credit_insufficient",
    "change_too_large",
    // M2 product extras (`supplier_not_found` is registered above with the ledgers)
    "image_invalid",
    "image_too_large",
    "variant_axes_invalid",
    // M2 promotions and pricing
    "promotion_not_found",
    "promotion_dates_required",
    // a solde, liquidation or déballage that has sold keeps its prices: a new campaign instead
    "campaign_has_sales",
    "price_below_floor",
    // M2 lots and serials
    "lot_required",
    "lot_expired",
    "lot_not_found",
    "serials_count_mismatch",
    "serial_taken",
    "serial_not_found",
    "serial_status_invalid",
    // M2 purchasing
    "purchase_order_invalid",
    "purchase_line_invalid",
    "receipt_already_invoiced",
    "supplier_invoice_duplicate",
    "serial_not_at_location",
    // M2 counts and transfers
    "count_not_found",
    "count_open_exists",
    "count_closed",
    "transfer_same_location",
    // M2 expenses
    "expense_category_not_found",
    "expense_not_found",
    "expense_already_reversed",
    // a till expense whose session is closed: the cash comes back as a paid-in of today's session
    "expense_session_closed",
    // M2 reports
    "report_range_invalid",
    "report_filter_required",
    // M2 imports (files)
    "import_file_unreadable",
    "import_file_too_large",
    "import_file_empty",
    "import_sheet_not_found",
    // M2 imports and data files
    "import_not_found",
    "import_mapping_invalid",
    "import_undo_not_allowed",
    "backup_file_invalid",
    // M3 documents
    "delivery_already_invoiced",
    "ticket_already_invoiced",
    "mentions_missing",
    "vat_exempt_ref_required",
    "document_kind_invalid",
    "document_not_convertible",
    "quote_expired",
    "delivery_customer_mismatch",
    "ticket_returned",
    "ticket_invoiced",
    "ticket_vat_exempt_buyer",
    "ticket_customer_mismatch",
    "document_tax_context_changed",
    "invoice_not_creditable",
    "credit_line_invalid",
    "credit_exceeds_invoice",
    "warranty_not_set",
    "warranty_document_invalid",
    // M3 fiscal archive
    "archive_range_invalid",
    // M3 exports: a report longer than a file holds is refused, never written cut
    "export_too_large",
    // M3 printing
    "print_host_unavailable",
    "print_render_timeout",
    "print_render_failed",
    "printer_unavailable",
    "print_failed",
    "pdf_failed",
    // the print host never answered: the copy may still come out (journaled, next is DUPLICATA)
    "print_timeout",
    // M4 hardware: weight and price barcodes
    "prefix_taken",
    "scale_plu_unknown",
    "scale_product_not_weighed",
    "scale_price_missing",
    // M4 labels
    "labels_empty",
    "labels_too_many",
    "label_source_not_printable",
    "label_arabic_required",
    "label_arabic_name_missing",
    "label_template_not_found",
    // M4 scale items
    "scale_plu_required",
    "scale_hotkey_taken",
    // M7 DZBuild link
    "cloud_unavailable",
    "cloud_unauthorized",
    "cloud_not_ready",
    "cloud_plan_cap",
    "cloud_not_linked",
    "cloud_already_linked",
    "cloud_link_expired",
    "cloud_link_cancelled",
    "cloud_link_denied",
    "cloud_state_mismatch",
    "cloud_discovery_invalid",
    "cloud_config_invalid",
    "cloud_relink_required",
    "cloud_store_not_found",
    "cloud_request_invalid",
    "cloud_rate_limited",
    "cloud_secret_store_failed",
    "cloud_attempt_not_found",
    "cloud_response_invalid",
    "cloud_order_not_found",
    // an order the buyer paid online is handed over, never charged again at the till
    "cloud_order_prepaid",
    "cloud_order_claimed_elsewhere",
    "cloud_order_state_invalid",
    "cloud_order_unmapped_product",
    "cloud_outbox_item_not_found",
    // M5 LAN
    "host_required",
    "lan_version_mismatch",
    // M5 replay on the main PC
    "replay_out_of_order",
    "replay_forbidden_row",
    "replay_invalid",
    "replay_conflict",
    // a till's validated document whose content no longer hashes to the till's own hash
    "replay_document_mismatch",
    "lan_token_invalid",
    // M5 pairing
    "pairing_code_invalid",
    "pairing_code_expired",
    "terminal_code_invalid",
    "terminal_code_taken",
    // M5 snapshot for a new till
    "snapshot_invalid",
    // M5 roles
    "lan_role_invalid",
    "lan_tills_paired",
    "lan_outbox_not_empty",
    "lan_restore_blocked",
    "lan_not_paired",
    // M5 clocks
    "lan_clock_skew",
    // M5 LAN server on the main PC
    "lan_port_busy",
    "lan_rate_limited",
    // M5 discovery of the main PC
    "lan_discovery_failed",
    // M5 the till's engine
    "lan_offline",
    // final review: a till's money call whose answer was lost waits and goes again
    "lan_answer_pending",
    // A till's return of a ticket whose own entries still wait for the main PC
    "return_host_pending",
    "lan_address_invalid",
    "replay_rejected",
    // M5 the till's token in Windows Credential Manager
    "lan_vault_unavailable",
    // M5 the till's rules in the commands, host services
    "kridi_needs_host",
    "outbox_pending_at_close",
    "offline_z_needs_approval",
    "rpc_service_not_found",
    // M5 making a till the main PC while the old one still answers
    "lan_host_alive",
    // M5 the main PC answered a host service; this till's copy has not received it yet
    "lan_catching_up",
    // M5/M7 a PC linked to DZBuild unlinks before it joins a main PC as a till
    "cloud_unlink_first",
    // M6 backups: the second copy
    "mirror_unavailable",
    "mirror_not_configured",
    // M6 guided restore: a restore failed after the damaged database was moved aside
    "database_missing",
    // M6 guided restore: a second one before the restart that applies the first
    "restore_already_scheduled",
    // M6 guided restore: the database opened; use Settings › Backups
    "recovery_not_available",
    // M6 updates
    "update_endpoint_invalid",
    "update_corrupt",
    // M6 demo shop
    "demo_active",
    "demo_not_active",
    // M6 the updater
    "update_check_failed",
    "update_not_available",
    "update_not_ready",
    "update_busy",
    "update_blocked",
    "update_too_old",
    // M6 diagnostic: the support button without a contact in the setting
    "support_contact_missing",
    // The demo shop's fiscal lock and its walls around the real shop
    "demo_not_allowed",
    "backup_is_demo",
    // M6 review: a second copy inside the data folder, or in a cloud-synced folder
    "mirror_invalid",
    "mirror_cloud_folder",
    // M8 packs: a pack feature refuses its writes while its module is off
    "module_disabled",
    // M8 superette and kiosque: regulated staples and department keys
    "price_above_ceiling",
    "regulated_code_unknown",
    "department_key_not_found",
    // M8 restaurant: floor, open tickets and modifiers
    "dining_room_not_found",
    "dining_table_not_found",
    "dining_ticket_not_found",
    "dining_line_not_found",
    "modifier_group_not_found",
    "table_cell_invalid",
    "table_cell_taken",
    "table_code_taken",
    "table_busy",
    "ticket_not_open",
    "ticket_needs_place",
    "modifier_choice_invalid",
    "line_already_sent",
    // M8 restaurant: kitchen tickets, the addition, splits and the payment
    "kitchen_nothing_to_send",
    "kitchen_ticket_not_found",
    "split_invalid",
    "kitchen_station_not_found",
    // M8 pharmacy: the controlled-medicine register and the Chifa list
    "register_entry_required",
    "chifa_list_invalid",
    // M8 butcher: the carcass cutting yield
    "yield_template_not_found",
    "yield_shares_invalid",
    "yield_output_exceeds_input",
    "yield_source_short",
    // M8 phones and kiosks: serials at the sale and on returns, flexy top-ups
    "serials_required",
    "imei_invalid",
    "buyer_required",
    "serial_not_on_line",
    "flexy_operator_unknown",
    "flexy_float_insufficient",
    "flexy_not_returnable",
    // M8 deposits, the voucher router and service tickets
    "deposit_owner_invalid",
    "deposit_insufficient",
    "voucher_unknown",
    "service_ticket_not_found",
    "service_status_invalid",
    "service_not_ready",
    "deposit_held",
    "deposit_not_found",
    "flexy_float_not_found",
    // M8 instalment sales
    "instalment_nothing_financed",
    "instalment_count_invalid",
    "instalment_plan_not_found",
    "instalment_due_unknown",
    // M8 wholesale and delivery rounds
    "below_min_order",
    "customer_price_invalid",
    "pack_units_invalid",
    "round_not_found",
    "round_status_invalid",
    "round_stop_invalid",
    "collection_above_due",
    "van_location_invalid",
    // M8 customer orders, layaway and appointments
    "order_not_found",
    "order_status_invalid",
    "layaway_not_paid",
    "layaway_deposit_too_low",
    "appointment_not_found",
    "appointment_overlap",
    "appointment_time_invalid",
    // M8 loyalty points, gift, exchange and loyalty vouchers
    "loyalty_card_invalid",
    "loyalty_not_enrolled",
    "points_below_minimum",
    "points_insufficient",
    "voucher_expired",
    "voucher_balance_insufficient",
    // M8 jewellery
    "jewel_piece_invalid",
    "metal_price_missing",
    "buyer_identity_required",
    "cash_above_limit",
    // M8 employees lite and zakat
    "staff_amount_invalid",
    "advance_repay_too_much",
    "month_invalid",
    "zakat_statement_not_found",
    "zakat_nisab_missing",
    // M8 MVP completion: custom payment types, Code 39 and ITF labels
    "payment_type_not_found",
    "payment_type_code_taken",
    "payment_type_mismatch",
    "label_barcode_invalid",
    // M8 migration 0009: per-lot stock counts (STK-004)
    "count_lot_required",
    "product_not_lot_tracked",
    // M8 Task 17: signed regulatory packs, till keys, personal data
    "datapack_signature_invalid",
    "datapack_file_invalid",
    "datapack_older",
    "key_map_invalid",
    "customer_has_open_items",
    // M9 licensing (named through `pos_license::codes` in the pos-license crate)
    "license_required",
    "license_expired",
    "license_tampered",
    "license_invalid",
    "license_wrong_device",
    "license_trial_used",
    "license_clock_behind",
    "license_revoked",
    "license_downgrade",
    "license_device_unreadable",
];

/// The message of the LAN guard triggers (`sync::capture`): a till tried a host-only write.
pub const HOST_REQUIRED_RAISE: &str = "host_required";

impl AppError {
    /// The field of a validation error (`lines.0.qty`), `None` for every other kind.
    pub fn field(&self) -> Option<&str> {
        match self {
            AppError::Validation { field, .. } => Some(field),
            _ => None,
        }
    }

    /// The same error homed on `field` when it is a validation error (a tender's
    /// `payments.{i}.reference`); any other error passes through unchanged.
    #[must_use]
    pub fn at_field(self, field: &str) -> AppError {
        match self {
            AppError::Validation { code, .. } => AppError::validation(field, &code),
            other => other,
        }
    }

    pub fn not_found(entity: &str) -> Self {
        AppError::NotFound {
            entity: entity.to_owned(),
        }
    }

    pub fn validation(field: &str, code: &str) -> Self {
        AppError::Validation {
            field: field.to_owned(),
            code: code.to_owned(),
        }
    }

    pub fn auth(code: &str) -> Self {
        AppError::Auth {
            code: code.to_owned(),
        }
    }

    pub fn conflict(code: &str) -> Self {
        AppError::Conflict {
            code: code.to_owned(),
        }
    }

    /// A DZBuild link failure (`cloud_*`): the UI translates the code; details stay in the logs.
    pub fn sync(code: &str) -> Self {
        AppError::Sync {
            code: code.to_owned(),
        }
    }

    /// A refusal carrying `details` for the UI; the code comes last like every constructor.
    pub fn rejected(details: Value, code: &str) -> Self {
        AppError::Rejected {
            code: code.to_owned(),
            details,
        }
    }

    /// The stable snake_case code the UI translates.
    pub fn code(&self) -> String {
        self.payload().code
    }

    pub fn payload(&self) -> ErrorPayload {
        let (code, message, field, details) = match self {
            AppError::Rejected { code, details } => {
                (code.clone(), None, None, Some(details.clone()))
            }
            AppError::NotFound { entity } => (
                format!("{entity}_not_found"),
                None,
                None,
                Some(json!({ "entity": entity })),
            ),
            AppError::Validation { field, code } => (code.clone(), None, Some(field.clone()), None),
            AppError::Permission { permission } => (
                "insufficient_permission".to_owned(),
                None,
                None,
                Some(json!({ "permission": permission })),
            ),
            AppError::Auth { code }
            | AppError::Conflict { code }
            | AppError::Hardware { code }
            | AppError::Sync { code } => (code.clone(), None, None, None),
            AppError::Db(m) => ("database_error".to_owned(), Some(m.clone()), None, None),
            AppError::Io(m) => ("io_error".to_owned(), Some(m.clone()), None, None),
            AppError::Internal(m) => ("internal_error".to_owned(), Some(m.clone()), None, None),
        };
        ErrorPayload {
            code,
            message,
            field,
            details,
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.payload().serialize(serializer)
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        use rusqlite::ErrorCode;
        if let rusqlite::Error::SqliteFailure(_, Some(message)) = &e
            && message == HOST_REQUIRED_RAISE
        {
            return AppError::conflict(HOST_REQUIRED_RAISE);
        }
        if matches!(e, rusqlite::Error::QueryReturnedNoRows) {
            return AppError::not_found("record");
        }
        match e.sqlite_error_code() {
            Some(ErrorCode::ConstraintViolation) => {
                tracing::warn!(error = %e, "sqlite constraint violation");
                AppError::conflict("constraint_violation")
            }
            Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) => {
                AppError::conflict("database_busy")
            }
            Some(ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt) => {
                AppError::conflict("database_corrupt")
            }
            _ => {
                tracing::warn!(error = %e, "sqlite error");
                AppError::Db(sqlite_message(&e))
            }
        }
    }
}

impl From<rusqlite_migration::Error> for AppError {
    fn from(e: rusqlite_migration::Error) -> Self {
        tracing::warn!(error = %e, "migration failed");
        // The Display of `RusqliteError` embeds the whole migration query: keep only the cause.
        let cause = match &e {
            rusqlite_migration::Error::RusqliteError { err, .. } => sqlite_message(err),
            other => other.to_string(),
        };
        AppError::Db(format!("migration: {cause}"))
    }
}

/// SQLite's message without the statement. `SqlInputError` prints the full SQL in its
/// `Display`, and SQL text must never reach the webview; callers log the full error.
fn sqlite_message(e: &rusqlite::Error) -> String {
    match e {
        rusqlite::Error::SqlInputError { msg, .. } => msg.clone(),
        other => other.to_string(),
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Internal(format!("json: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::{Path, PathBuf};

    #[test]
    fn validation_error_shape() {
        let e = AppError::validation("nif", "nif_invalid");
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            json!({ "code": "nif_invalid", "field": "nif" })
        );
    }

    #[test]
    fn at_field_rehomes_a_validation_error_only() {
        let e =
            AppError::validation("reference", "voucher_unknown").at_field("payments.1.reference");
        assert_eq!(
            (e.field(), e.code().as_str()),
            (Some("payments.1.reference"), "voucher_unknown")
        );
        let refused = AppError::rejected(json!({ "balance": 5 }), "voucher_balance_insufficient")
            .at_field("payments.1.reference");
        assert_eq!(
            refused,
            AppError::rejected(json!({ "balance": 5 }), "voucher_balance_insufficient")
        );
    }

    #[test]
    fn permission_error_shape() {
        let e = AppError::Permission {
            permission: "sales.create".into(),
        };
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            json!({ "code": "insufficient_permission", "details": { "permission": "sales.create" } })
        );
    }

    #[test]
    fn not_found_and_developer_messages() {
        assert_eq!(
            serde_json::to_value(AppError::not_found("user")).unwrap(),
            json!({ "code": "user_not_found", "details": { "entity": "user" } })
        );
        assert_eq!(
            serde_json::to_value(AppError::Db("disk I/O error".into())).unwrap(),
            json!({ "code": "database_error", "message": "disk I/O error" })
        );
        assert_eq!(AppError::auth("invalid_pin").code(), "invalid_pin");
    }

    #[test]
    fn sqlite_errors_are_mapped() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE t (k TEXT PRIMARY KEY); INSERT INTO t VALUES ('a');")
            .unwrap();
        let dup: AppError = conn
            .execute("INSERT INTO t VALUES ('a')", [])
            .unwrap_err()
            .into();
        assert_eq!(dup.code(), "constraint_violation");
        let none: AppError = conn
            .query_row("SELECT k FROM t WHERE k = 'z'", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap_err()
            .into();
        assert_eq!(none.code(), "record_not_found");
    }

    #[test]
    fn a_guard_trigger_raising_host_required_is_a_conflict() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE t (x INTEGER);
             CREATE TRIGGER g BEFORE INSERT ON t BEGIN SELECT RAISE(ABORT, 'host_required'); END;",
        )
        .unwrap();
        let err: AppError = conn
            .execute("INSERT INTO t VALUES (1)", [])
            .unwrap_err()
            .into();
        assert_eq!(err, AppError::conflict(HOST_REQUIRED_RAISE));
        assert_eq!(err.code(), "host_required");
    }

    #[test]
    fn known_codes_are_unique_snake_case() {
        let mut seen = std::collections::BTreeSet::new();
        for code in KNOWN_CODES {
            assert!(seen.insert(*code), "duplicate code {code}");
            assert!(
                code.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{code}"
            );
        }
    }

    fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }

    /// Constructors whose last argument is the code, with the suffix `payload()` adds.
    const CTORS: [(&str, &str); 6] = [
        ("validation", ""),
        ("auth", ""),
        ("conflict", ""),
        ("rejected", ""),
        ("sync", ""),
        ("not_found", "_not_found"),
    ];

    /// Struct-literal variants, the field that carries the code, and its suffix.
    const VARIANTS: [(&str, &str, &str); 7] = [
        ("Validation", "code", ""),
        ("Auth", "code", ""),
        ("Conflict", "code", ""),
        ("Rejected", "code", ""),
        ("Hardware", "code", ""),
        ("Sync", "code", ""),
        ("NotFound", "entity", "_not_found"),
    ];

    /// Splits the comma-separated items of a list whose opening bracket was already consumed,
    /// up to its matching close. Returns the items and the byte index of that close, or `None`
    /// when it is unterminated. Commas inside strings and nested brackets are skipped;
    /// char literals holding a bracket or a quote are not understood (heuristic scan).
    fn split_list(s: &str) -> Option<(Vec<&str>, usize)> {
        let (mut items, mut start, mut depth) = (Vec::new(), 0, 0_usize);
        let (mut in_str, mut escaped) = (false, false);
        for (i, c) in s.char_indices() {
            if in_str {
                match c {
                    _ if escaped => escaped = false,
                    '\\' => escaped = true,
                    '"' => in_str = false,
                    _ => {}
                }
                continue;
            }
            match c {
                '"' => in_str = true,
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' if depth == 0 => {
                    items.push(&s[start..i]);
                    return Some((items, i));
                }
                ')' | ']' | '}' => depth -= 1,
                ',' if depth == 0 => {
                    items.push(&s[start..i]);
                    start = i + 1;
                }
                _ => {}
            }
        }
        None
    }

    /// The contents of a string literal that `s` starts with.
    fn leading_literal(s: &str) -> Option<&str> {
        s.strip_prefix('"')?
            .split_once('"')
            .map(|(literal, _)| literal)
    }

    /// Bodies of `impl … AppError { … }` blocks (inherent or trait), where `Self` is `AppError`.
    fn app_error_impls(text: &str) -> Vec<&str> {
        let mut bodies = Vec::new();
        for (at, _) in text.match_indices("impl") {
            let inside_word = text[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_');
            let after = &text[at + "impl".len()..];
            let Some(open) = after.find('{') else {
                continue;
            };
            let header = &after[..open];
            if inside_word || header.contains([';', '}']) || !header.ends_with("AppError") {
                continue;
            }
            let body = &after[open + 1..];
            if let Some((_, end)) = split_list(body) {
                bodies.push(&body[..end]);
            }
        }
        bodies
    }

    /// Every code `source` emits through `AppError::ctor(…, "code")`,
    /// `AppError::Variant { code: "…", .. }`, or the same shapes spelled `Self::` inside an
    /// `impl … AppError` block. Only string literals count: a code held in a variable is
    /// never reported, so it cannot turn into a garbage code.
    fn emitted_codes(source: &str) -> Vec<String> {
        let text: String = source.split_whitespace().collect();
        let mut scopes = vec![("AppError::", text.as_str())];
        scopes.extend(
            app_error_impls(&text)
                .into_iter()
                .map(|body| ("Self::", body)),
        );
        let mut codes = Vec::new();
        for (prefix, scope) in scopes {
            for (ctor, suffix) in CTORS {
                for rest in scope.split(&format!("{prefix}{ctor}(")).skip(1) {
                    let last_arg = split_list(rest).and_then(|(args, _)| args.last().copied());
                    if let Some(code) = last_arg.and_then(leading_literal) {
                        codes.push(format!("{code}{suffix}"));
                    }
                }
            }
            for (variant, field, suffix) in VARIANTS {
                for rest in scope.split(&format!("{prefix}{variant}{{")).skip(1) {
                    let value = split_list(rest).and_then(|(fields, _)| {
                        fields
                            .into_iter()
                            .find_map(|f| f.strip_prefix(field)?.strip_prefix(':'))
                    });
                    if let Some(code) = value.and_then(leading_literal) {
                        codes.push(format!("{code}{suffix}"));
                    }
                }
            }
        }
        codes
    }

    /// Every `AppError::x("literal")` code emitted by pos-core must be in KNOWN_CODES,
    /// because the UI needs a translation for it (see src-tauri contract tests).
    #[test]
    fn every_emitted_code_is_known() {
        let mut files = Vec::new();
        rust_files(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut files,
        );
        let mut unknown = Vec::new();
        let mut scanned = 0;
        for file in files {
            let emitted = emitted_codes(&std::fs::read_to_string(&file).unwrap());
            scanned += emitted.len();
            for code in emitted {
                // "forced" is the single fixture code tests use to force a rollback.
                if !KNOWN_CODES.contains(&code.as_str()) && code != "forced" {
                    unknown.push(format!("{}: {code}", file.display()));
                }
            }
        }
        assert!(scanned > 0, "the scan found no codes at all");
        assert!(
            unknown.is_empty(),
            "codes missing from KNOWN_CODES: {unknown:?}"
        );
    }

    #[test]
    fn scanner_reports_every_call_shape() {
        // `ERR` stands in for `AppError` so that this file's own text never matches the scan.
        let probe = r#"
            fn a(field: &str) -> ERR { ERR::validation(field, "bogus_a") }
            fn b() -> ERR { ERR::validation(&label("x", 2), "bogus_b") }
            fn c() -> ERR { ERR::conflict("bogus_c") }
            fn d() -> ERR { ERR::not_found("bogus_d") }
            fn e() -> ERR { ERR::Conflict { code: "bogus_e".into() } }
            fn f() -> ERR { ERR::NotFound { entity: "bogus_f".to_owned() } }
            impl ERR {
                fn g() -> Self {
                    Self::auth("bogus_g")
                }
            }
            impl From<Io> for ERR {
                fn from(_: Io) -> Self { Self::Hardware { code: "bogus_h".into() } }
            }
            // Not literal codes, and `Self` of another type: nothing to report.
            fn i(field: &str, code: &str) -> ERR { ERR::validation(field, code) }
            fn j(field: &str) -> ERR { ERR::validation("nif", field) }
            fn k(code: &str) -> ERR { ERR::Conflict { code: code.into() } }
            impl Other {
                fn l() -> Self { Self::auth("not_an_app_error") }
            }
        "#
        .replace("ERR", "AppError");
        let mut codes = emitted_codes(&probe);
        codes.sort();
        assert_eq!(
            codes,
            [
                "bogus_a",
                "bogus_b",
                "bogus_c",
                "bogus_d_not_found",
                "bogus_e",
                "bogus_f_not_found",
                "bogus_g",
                "bogus_h",
            ]
        );
    }

    #[test]
    fn rejected_errors_carry_their_details() {
        let e = AppError::rejected(
            json!({ "shortages": [{ "productId": "p1" }] }),
            "stock_insufficient",
        );
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            json!({ "code": "stock_insufficient", "details": { "shortages": [{ "productId": "p1" }] } })
        );
        assert_eq!(e.to_string(), "rejected: stock_insufficient");
        let probe = r#"fn f() -> ERR { ERR::rejected(json!({ "a": [1, 2] }), "bogus_r") }"#
            .replace("ERR", "AppError");
        assert_eq!(emitted_codes(&probe), ["bogus_r"]);
    }

    #[test]
    fn coded_variants_pass_their_code_through() {
        assert_eq!(
            serde_json::to_value(AppError::conflict("username_taken")).unwrap(),
            json!({ "code": "username_taken" })
        );
        assert_eq!(
            AppError::auth("invalid_pin").payload(),
            ErrorPayload {
                code: "invalid_pin".into(),
                message: None,
                field: None,
                details: None
            }
        );
        assert_eq!(
            AppError::Hardware {
                code: "forced".into()
            }
            .code(),
            "forced"
        );
        assert_eq!(
            AppError::Sync {
                code: "forced".into()
            }
            .code(),
            "forced"
        );
    }

    #[test]
    fn sync_errors_carry_their_code_only() {
        assert_eq!(
            serde_json::to_value(AppError::sync("cloud_unavailable")).unwrap(),
            json!({ "code": "cloud_unavailable" })
        );
        for code in [
            "cloud_unavailable",
            "cloud_not_ready",
            "cloud_relink_required",
            "cloud_order_not_found",
            "cloud_outbox_item_not_found",
        ] {
            assert!(KNOWN_CODES.contains(&code), "{code}");
        }
    }

    #[test]
    fn payload_roundtrips_through_json() {
        for e in [
            AppError::not_found("user"),
            AppError::validation("nif", "nif_invalid"),
            AppError::Internal("boom".into()),
        ] {
            let back: ErrorPayload =
                serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap();
            assert_eq!(back, e.payload());
        }
        // Absent optional fields deserialize as None.
        let minimal: ErrorPayload = serde_json::from_value(json!({ "code": "io_error" })).unwrap();
        assert_eq!(
            minimal,
            ErrorPayload {
                code: "io_error".into(),
                message: None,
                field: None,
                details: None
            }
        );
    }

    #[test]
    fn display_is_a_developer_sentence() {
        assert_eq!(AppError::not_found("user").to_string(), "user not found");
        assert_eq!(
            AppError::validation("nif", "nif_invalid").to_string(),
            "invalid nif: nif_invalid"
        );
        assert_eq!(
            AppError::Db("disk full".into()).to_string(),
            "database error: disk full"
        );
    }

    #[test]
    fn sqlite_busy_and_corruption_are_mapped() {
        use rusqlite::ffi;
        let from_code = |code| -> AppError {
            rusqlite::Error::SqliteFailure(ffi::Error::new(code), None).into()
        };
        assert_eq!(from_code(ffi::SQLITE_BUSY).code(), "database_busy");
        assert_eq!(from_code(ffi::SQLITE_LOCKED).code(), "database_busy");
        assert_eq!(from_code(ffi::SQLITE_CORRUPT).code(), "database_corrupt");
        assert_eq!(from_code(ffi::SQLITE_NOTADB).code(), "database_corrupt");
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let syntax: AppError = conn.execute("NOT SQL", []).unwrap_err().into();
        let payload = syntax.payload();
        assert_eq!(payload.code, "database_error");
        assert_eq!(
            payload.message.as_deref(),
            Some("near \"NOT\": syntax error")
        );
    }

    #[test]
    fn sql_text_never_reaches_the_payload() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let sql = "SELEKT secret_column FROM secret_table";
        let rusqlite_err = conn.execute(sql, []).unwrap_err();
        assert!(
            rusqlite_err.to_string().contains(sql),
            "precondition: rusqlite's Display embeds the statement"
        );
        let message = AppError::from(rusqlite_err).payload().message.unwrap();
        assert!(
            !message.contains(sql) && !message.contains("secret"),
            "{message}"
        );

        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        let query =
            "CREATE TABLE secret_table (secret_column); SELEKT secret_column FROM secret_table;";
        let broken = [rusqlite_migration::M::up(query)];
        let migration_err = rusqlite_migration::Migrations::from_slice(&broken)
            .to_latest(&mut conn)
            .unwrap_err();
        assert!(
            migration_err.to_string().contains(query),
            "precondition: rusqlite_migration's Display embeds the query"
        );
        let message = AppError::from(migration_err).payload().message.unwrap();
        assert!(
            !message.contains(query) && !message.contains("secret"),
            "{message}"
        );
        assert_eq!(message, "migration: near \"SELEKT\": syntax error");
    }

    #[test]
    fn io_json_and_migration_errors_keep_their_cause() {
        let io: AppError =
            std::io::Error::new(std::io::ErrorKind::NotFound, "no backup dir").into();
        assert_eq!(
            serde_json::to_value(&io).unwrap(),
            json!({ "code": "io_error", "message": "no backup dir" })
        );

        let json_err: AppError = serde_json::from_str::<i64>("x").unwrap_err().into();
        let payload = json_err.payload();
        assert_eq!(payload.code, "internal_error");
        assert!(payload.message.is_some_and(|m| m.starts_with("json: ")));

        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        let broken = [rusqlite_migration::M::up("NOT SQL")];
        let migrations = rusqlite_migration::Migrations::from_slice(&broken);
        let migration: AppError = migrations.to_latest(&mut conn).unwrap_err().into();
        let payload = migration.payload();
        assert_eq!(payload.code, "database_error");
        assert_eq!(
            payload.message.as_deref(),
            Some("migration: near \"NOT\": syntax error")
        );
    }
}

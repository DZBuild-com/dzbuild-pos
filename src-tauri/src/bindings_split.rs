//! The vertical packs' commands get their bindings in a module of their own,
//! `src/bindings-packs.ts`, so the till's first chunk (the /pos route, NFR-010's 300 kB budget)
//! carries only the commands it calls itself. `export_bindings` writes tauri-specta's file, then
//! moves the entries of [`PACK_BINDINGS`] out of its `commands` object into `packCommands`;
//! `src/lib/ipc/client.ts` answers a pack command by loading that module on its first call.

use std::collections::BTreeSet;

/// The commands of M8's packs (Task 21b and STK-004's lot counts included) that only the
/// pack screens, the back office and the till's lazy pack chunk call. The few the till's entry calls itself stay in
/// `src/bindings.ts`: the licence, the pack state, the key map, the payment types, and the
/// reprints of a kitchen ticket, an addition, a voucher and a receipt as a page.
pub(crate) const PACK_BINDINGS: &[&str] = &[
    "appointments_book",
    "appointments_day",
    "appointments_range",
    "appointments_set_status",
    "appointments_update",
    "chifa_apply",
    "chifa_check",
    "chifa_list",
    "chifa_of_product",
    "controlled_register_entries",
    "counts_lot_lines",
    "counts_set_lot",
    "daily_menu_get",
    "daily_menu_set",
    "datapack_install",
    "datapack_installed",
    "departments_archive",
    "departments_list",
    "departments_save",
    "deposits_list",
    "deposits_quote",
    "deposits_refund",
    "deposits_take",
    "dining_board",
    "dining_counter",
    "dining_room_archive",
    "dining_room_save",
    "dining_rooms",
    "dining_table_archive",
    "dining_table_save",
    "dining_tables",
    "dining_ticket_add_line",
    "dining_ticket_comp_line",
    "dining_ticket_edit_line",
    "dining_ticket_get",
    "dining_ticket_merge",
    "dining_ticket_open",
    "dining_ticket_pay",
    "dining_ticket_preview",
    "dining_ticket_remove_unsent",
    "dining_ticket_send",
    "dining_ticket_set_details",
    "dining_ticket_set_qty",
    "dining_ticket_split",
    "dining_ticket_transfer",
    "dining_ticket_void",
    "dining_ticket_void_line",
    "flexy_balances",
    "flexy_entries",
    "flexy_line",
    "flexy_operators",
    "flexy_purchase",
    "instalments_add_guarantee",
    "instalments_collect",
    "instalments_contract",
    "instalments_contract_print",
    "instalments_get",
    "instalments_list",
    "instalments_note_reminder",
    "instalments_quote",
    "instalments_reminder_text",
    "instalments_reminders",
    "instalments_sign",
    "jeweller_register_entries",
    "jewellery_piece_get",
    "jewellery_piece_set",
    "jewellery_reprice",
    "kitchen_station_archive",
    "kitchen_station_save",
    "kitchen_stations",
    "lan_pending_entries",
    "lan_pending_retry",
    "loyalty_account",
    "loyalty_adjust",
    "loyalty_block",
    "loyalty_card_lookup",
    "loyalty_enrol",
    "loyalty_entries",
    "loyalty_redeem",
    "metal_board",
    "metal_history",
    "metal_price_set",
    "modifier_group_archive",
    "modifier_group_save",
    "modifier_groups",
    "modifiers_for_product",
    "modifiers_set_product",
    "orders_close",
    "orders_create",
    "orders_deliver",
    "orders_fulfil",
    "orders_fulfil_cart",
    "orders_get",
    "orders_keep_deposit",
    "orders_list",
    "orders_pay_in",
    "orders_set_ready",
    "packs_apply_template",
    "packs_reset_modules",
    "packs_set_modules",
    "packs_set_till_mode",
    "packs_templates",
    "payment_type_archive",
    "payment_type_save",
    "payments_baridpay_qr",
    "pharmacy_equivalents",
    "pharmacy_expiry_board",
    "pharmacy_product_get",
    "pharmacy_product_set",
    "privacy_anonymise",
    "privacy_consents",
    "privacy_dpo",
    "privacy_export_customer",
    "privacy_register",
    "privacy_set_consent",
    "regulated_board",
    "regulated_seed",
    "regulated_set",
    "roles_duplicate",
    "rounds_close",
    "rounds_close_preview",
    "rounds_create",
    "rounds_create_van",
    "rounds_get",
    "rounds_list",
    "rounds_load",
    "rounds_report_stop",
    "rounds_route_sheet",
    "rounds_route_sheet_print",
    "rounds_set_stops",
    "rounds_start",
    "rounds_unload",
    "sales_points_preview",
    "serials_available",
    "service_assign",
    "service_board",
    "service_get",
    "service_intake",
    "service_pickup",
    "service_ready_message",
    "service_set_items",
    "service_set_status",
    "service_uncollected",
    "staff_advance",
    "staff_deduct",
    "staff_entries",
    "staff_list",
    "staff_profile_set",
    "staff_repay",
    "staff_shifts",
    "till_set_key_map",
    "users_duplicate",
    "variant_grid",
    "vouchers_describe",
    "vouchers_get",
    "vouchers_list",
    "vouchers_quote_sale",
    "vouchers_sell",
    "waste_day",
    "waste_record",
    "wholesale_min_order",
    "wholesale_min_order_set",
    "wholesale_pack_create",
    "wholesale_price_set",
    "wholesale_prices",
    "yield_preview",
    "yield_run",
    "yield_runs",
    "yield_template_archive",
    "yield_template_save",
    "yield_templates",
    "zakat_list",
    "zakat_preview",
    "zakat_save",
    "zakat_statement",
];

/// One entry of the `commands` object: its doc comment and code lines, and its command name.
struct Entry {
    name: String,
    text: String,
}

/// The command a line ends, when it is the last line of an entry: `…>("name"…),`.
fn ends_entry(line: &str) -> Option<String> {
    if !line.trim_end().ends_with("),") {
        return None;
    }
    let at = line.rfind(">(\"")? + 3;
    let rest = &line[at..];
    let name = &rest[..rest.find('"')?];
    name.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        .then(|| name.to_owned())
}

/// Every `export type` name the file declares.
fn declared_types(full: &str) -> BTreeSet<String> {
    full.lines()
        .filter_map(|line| line.strip_prefix("export type "))
        .filter_map(|rest| rest.split([' ', '<', '=']).next())
        .map(str::to_owned)
        .collect()
}

/// The type names `text` uses among `declared`.
fn used_types(text: &str, declared: &BTreeSet<String>) -> BTreeSet<String> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|word| declared.contains(*word))
        .map(str::to_owned)
        .collect()
}

/// Splits tauri-specta's file `full` (written with `header`) into `src/bindings.ts` without the
/// pack commands and `src/bindings-packs.ts` with them (`packCommands`, its types imported from
/// `./bindings`).
pub(crate) fn split(full: &str, header: &str, pack: &[&str]) -> Result<(String, String), String> {
    const OPEN: &str = "export const commands = {\n";
    let start = full.find(OPEN).ok_or("no commands object")? + OPEN.len();
    let end = start
        + full[start..]
            .find("\n};\n")
            .ok_or("no end of the commands object")?
        + 1;
    let mut entries = Vec::new();
    let mut text = String::new();
    for line in full[start..end].lines() {
        text.push_str(line);
        text.push('\n');
        if let Some(name) = ends_entry(line) {
            entries.push(Entry {
                name,
                text: std::mem::take(&mut text),
            });
        }
    }
    if !text.trim().is_empty() {
        return Err(format!("unparsed bindings text: {text}"));
    }
    let wanted: BTreeSet<&str> = pack.iter().copied().collect();
    let found: BTreeSet<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    if let Some(missing) = wanted.difference(&found).next() {
        return Err(format!("{missing} is not a command of the bindings"));
    }
    let (packs, own): (Vec<&Entry>, Vec<&Entry>) = entries
        .iter()
        .partition(|e| wanted.contains(e.name.as_str()));
    let core = format!(
        "{}{}{}",
        &full[..start],
        own.iter().map(|e| e.text.as_str()).collect::<String>(),
        &full[end..]
    );
    let pack_text: String = packs.iter().map(|e| e.text.as_str()).collect();
    let types = used_types(&pack_text, &declared_types(full));
    let imports = if types.is_empty() {
        String::new()
    } else {
        format!(
            "import type {{\n{}}} from \"./bindings\";\n",
            types
                .iter()
                .map(|t| format!("\t{t},\n"))
                .collect::<String>()
        )
    };
    let packs = format!(
        "{header}\n// The vertical packs' commands, loaded on their first call (`src/lib/ipc/client.ts`).\n\nimport {{ invoke as __TAURI_INVOKE }} from \"@tauri-apps/api/core\";\n{imports}\n/** Commands */\nexport const packCommands = {{\n{pack_text}}};\n"
    );
    Ok((core, packs))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = "// header\n\nimport { invoke as __TAURI_INVOKE } from \"@tauri-apps/api/core\";\n\n/** Commands */\nexport const commands = {\n\tappPing: () => __TAURI_INVOKE<string>(\"app_ping\"),\n\t/**  The floor. */\n\tdiningBoard: (roomId: string | null) => __TAURI_INVOKE<FloorBoard>(\"dining_board\", { roomId }),\n\tvouchersDescribe: (reference: string) => __TAURI_INVOKE<{\n\ttarget: VoucherTarget,\n} | null>(\"vouchers_describe\", { reference }),\n};\n\n/** Events */\nexport const events = {};\n\nexport type FloorBoard = { tables: string[] };\n\nexport type VoucherTarget = { kind: \"x\" };\n\nexport type Unused = { a: number };\n";

    #[test]
    fn the_pack_commands_move_to_their_own_module_with_their_types() {
        let (core, packs) =
            split(FULL, "// header", &["dining_board", "vouchers_describe"]).unwrap();
        assert!(core.contains("appPing"));
        assert!(!core.contains("dining_board") && !core.contains("vouchers_describe"));
        assert!(
            core.contains("export type FloorBoard"),
            "the types stay in bindings.ts"
        );
        assert!(
            packs.contains("export const packCommands = {\n\t/**  The floor. */\n\tdiningBoard")
        );
        assert!(packs.contains("} | null>(\"vouchers_describe\", { reference }),\n};\n"));
        assert!(
            packs
                .contains("import type {\n\tFloorBoard,\n\tVoucherTarget,\n} from \"./bindings\";")
        );
        assert!(!packs.contains("Unused"));
    }

    #[test]
    fn a_pack_command_the_bindings_lack_is_refused() {
        let refused = split(FULL, "// header", &["no_such_command"]).unwrap_err();
        assert!(refused.contains("no_such_command"), "{refused}");
    }

    #[test]
    fn every_pack_binding_is_a_registered_command() {
        let commands: BTreeSet<&str> = crate::COMMANDS.iter().copied().collect();
        for name in PACK_BINDINGS {
            assert!(commands.contains(name), "{name} is not in COMMANDS");
        }
    }
}

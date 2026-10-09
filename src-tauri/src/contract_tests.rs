use crate::{COMMANDS, export_bindings, specta_builder};
use pos_core::audit::KNOWN_ACTIONS;
use pos_core::auth::{Permission, PermissionGroup};
use pos_core::error::KNOWN_CODES;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub(crate) fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub(crate) fn repo_file(relative: &str) -> PathBuf {
    manifest_dir().join("..").join(relative)
}

/// `src/bindings.ts` and `src/bindings-packs.ts` as `write_bindings` writes them.
fn generated_files() -> (String, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bindings.ts");
    export_bindings(&specta_builder(), &path).unwrap();
    let read = |p: PathBuf| std::fs::read_to_string(p).unwrap().replace("\r\n", "\n");
    (
        read(path.clone()),
        read(path.with_file_name(crate::PACK_BINDINGS_FILE)),
    )
}

/// Both generated files, one after the other: every command once.
fn generated_bindings() -> String {
    let (core, packs) = generated_files();
    core + &packs
}

/// The command name of every `__TAURI_INVOKE<…>("name", …)` call. rc.25 breaks an inline
/// return type over several lines, so each call is read from its `INVOKE` up to its `("`.
fn invoked_commands(bindings: &str) -> BTreeSet<String> {
    bindings
        .split("INVOKE")
        .skip(1)
        .filter(|call| call.starts_with('<') || call.starts_with('('))
        .filter_map(|call| {
            let start = call.find("(\"")? + 2;
            let end = start + call[start..].find('"')?;
            Some(call[start..end].to_owned())
        })
        .collect()
}

#[test]
fn every_command_is_permitted_and_exported() {
    let unique: BTreeSet<&str> = COMMANDS.iter().copied().collect();
    assert_eq!(unique.len(), COMMANDS.len(), "duplicate name in COMMANDS");
    let main =
        std::fs::read_to_string(manifest_dir().join("permissions/pos-commands.toml")).unwrap();
    let host = std::fs::read_to_string(manifest_dir().join("permissions/print-host.toml")).unwrap();
    let bindings = generated_bindings();
    for name in COMMANDS {
        let quoted = format!("\"{name}\"");
        let (in_main, in_host) = (main.contains(&quoted), host.contains(&quoted));
        assert!(
            in_main ^ in_host,
            "{name} must be allowed by exactly one permission file"
        );
        assert_eq!(
            in_host,
            name.starts_with("print_host_"),
            "{name}: only the print host commands live in print-host.toml"
        );
        assert!(
            bindings.contains(&format!("\"{name}\"")),
            "{name} is missing from specta_builder()"
        );
    }
    let invoked = invoked_commands(&bindings);
    let listed: BTreeSet<String> = COMMANDS.iter().map(|c| (*c).to_owned()).collect();
    assert_eq!(invoked, listed, "collect_commands! and COMMANDS disagree");
}

/// Review Focus 5: the webview never names a file. The commands that write or read a file take
/// no path argument; the path comes from the Rust dialog.
#[test]
fn pdf_paths_come_from_the_rust_dialog_only() {
    let bindings = generated_bindings();
    for command in [
        "documentsExportPdf",
        "statementsExportPdf",
        "warrantyExportPdf",
        "exportsPdf",
        "labelsSheetExportPdf",
        "fiscalExportArchive",
        "fiscalVerifyArchive",
        // Task 21b: a data pack and its signature, a customer's data folder (PDP-004, PLT-024)
        "datapackInstall",
        "privacyExportCustomer",
    ] {
        // tauri-specta writes `\tname: (params) => __TAURI_INVOKE<…>("name", { … })`.
        let start = bindings
            .find(&format!("\t{command}: ("))
            .unwrap_or_else(|| panic!("{command} missing"));
        let rest = &bindings[start..];
        let params = &rest[..rest.find(") =>").unwrap()];
        assert!(
            !params.to_lowercase().contains("path"),
            "{command} takes a path: {params}"
        );
    }
    for input in [
        "PdfDocumentInput",
        "StatementPrintInput",
        "WarrantyInput",
        "PdfExportRequest",
        "ArchiveInput",
    ] {
        let start = bindings
            .find(&format!("export type {input} = "))
            .unwrap_or_else(|| panic!("{input} missing"));
        let rest = &bindings[start..];
        // The whole type: specta writes one field per line and a blank line after it.
        let fields = &rest[..rest.find("\n\n").unwrap_or(rest.len())];
        assert!(
            !fields.to_lowercase().contains("path"),
            "{input} carries a path: {fields}"
        );
    }
}

#[test]
fn bindings_are_up_to_date() {
    let committed = |file: &str| {
        std::fs::read_to_string(repo_file(file))
            .unwrap_or_default()
            .replace("\r\n", "\n")
    };
    let (core, packs) = generated_files();
    assert!(
        committed("src/bindings.ts") == core && committed("src/bindings-packs.ts") == packs,
        "src/bindings.ts or src/bindings-packs.ts is stale: run `cargo test -p dzbuild-pos write_bindings -- --ignored`"
    );
}

#[test]
#[ignore = "writes src/bindings.ts and src/bindings-packs.ts; run it after adding or changing a command"]
fn write_bindings() {
    export_bindings(&specta_builder(), repo_file("src/bindings.ts")).unwrap();
}

const LANGS: [&str; 3] = ["ar", "fr", "en"];

fn flatten(prefix: &str, value: &Value, out: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten(&path, child, out);
            }
        }
        _ => {
            out.insert(prefix.to_owned());
        }
    }
}

fn locale_keys(lang: &str, namespace: &str) -> BTreeSet<String> {
    let text = std::fs::read_to_string(repo_file(&format!("src/locales/{lang}/{namespace}.json")))
        .unwrap();
    let mut keys = BTreeSet::new();
    flatten("", &serde_json::from_str(&text).unwrap(), &mut keys);
    keys
}

#[test]
fn every_error_code_has_a_message_in_every_language() {
    for lang in LANGS {
        let keys = locale_keys(lang, "errors");
        assert!(
            keys.contains("unknown"),
            "{lang}/errors.json lacks the fallback"
        );
        for code in KNOWN_CODES {
            assert!(keys.contains(*code), "{lang}/errors.json lacks {code}");
        }
    }
}

#[test]
fn every_hardware_code_has_a_message_in_every_language() {
    for lang in LANGS {
        let keys = locale_keys(lang, "errors");
        for code in pos_hw::KNOWN_HW_CODES {
            assert!(keys.contains(*code), "{lang}/errors.json lacks {code}");
        }
    }
}

#[test]
fn every_permission_has_a_label_in_every_language() {
    for lang in LANGS {
        // the till names them too (a manager's approval): they are the till's `auth` texts
        let keys = locale_keys(lang, "auth");
        for permission in Permission::ALL {
            assert!(
                keys.contains(&format!("permissions.{}", permission.as_str())),
                "{lang}: {}",
                permission.as_str()
            );
        }
        for group in PermissionGroup::ALL {
            assert!(
                keys.contains(&format!("permissions.groups.{}", group.as_str())),
                "{lang}: group {}",
                group.as_str()
            );
        }
    }
}

#[test]
fn every_audit_action_has_a_label_in_every_language() {
    for lang in LANGS {
        let keys = locale_keys(lang, "settings");
        for action in KNOWN_ACTIONS {
            assert!(
                keys.contains(&format!("audit.actions.{action}")),
                "{lang}: {action}"
            );
        }
    }
}

/// Every Rust file under `dir`, whitespace removed and cut before its inline test module:
/// `audit::tests` appends a made-up action on purpose, to prove the debug assertion that
/// already stops any test using an unknown one.
fn rust_sources(dir: &Path, out: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let text: String = std::fs::read_to_string(&path)
                .unwrap()
                .split_whitespace()
                .collect();
            out.push(without_inline_tests(&text).to_owned());
        }
    }
}

/// `text` (whitespace removed) up to its first `#[cfg(test)] mod name { … }`; a declaration
/// such as `#[cfg(test)] mod tests;` cuts nothing.
fn without_inline_tests(text: &str) -> &str {
    const MARK: &str = "#[cfg(test)]mod";
    for (at, _) in text.match_indices(MARK) {
        let rest = &text[at + MARK.len()..];
        let name_end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        if rest[name_end..].starts_with('{') {
            return &text[..at];
        }
    }
    text
}

#[test]
fn the_scan_skips_inline_test_modules_only() {
    let file: String =
        "fn a() {}\n#[cfg(test)]\nmod tests;\nfn b() {}\n#[cfg(test)]\nmod tests {\n fn c() {}\n}"
            .split_whitespace()
            .collect();
    assert_eq!(
        without_inline_tests(&file),
        "fna(){}#[cfg(test)]modtests;fnb(){}"
    );
}

fn literals_after(text: &str, pattern: &str) -> Vec<String> {
    text.split(pattern)
        .skip(1)
        .filter_map(|part| part.find('"').map(|end| part[..end].to_owned()))
        .collect()
}

/// Error codes written in the shell and audit actions written anywhere must be known (and so
/// translated). pos-core's own codes are covered by `error::tests::every_emitted_code_is_known`
/// (Task 3), so they are not scanned twice.
#[test]
fn only_known_codes_and_actions_are_emitted() {
    let mut texts = Vec::new();
    rust_sources(&manifest_dir().join("src"), &mut texts);
    let shell_files = texts.len();
    rust_sources(&repo_file("crates/pos-core/src"), &mut texts);
    rust_sources(&repo_file("crates/pos-license/src"), &mut texts);
    for (index, text) in texts.iter().enumerate() {
        if index < shell_files {
            for code in literals_after(text, "AppError::auth(\"")
                .into_iter()
                .chain(literals_after(text, "AppError::conflict(\""))
            {
                assert!(
                    KNOWN_CODES.contains(&code.as_str()),
                    "unknown error code {code}"
                );
            }
        }
        for action in literals_after(text, ".event(\"") {
            assert!(
                KNOWN_ACTIONS.contains(&action.as_str()),
                "unknown audit action {action}"
            );
        }
    }
}

/// Review finding: when Tauri cannot start, Windows shows one message in the three languages
/// (`common:nativeFatal.body` of each catalogue) with the technical detail last.
#[test]
fn the_native_fatal_message_speaks_the_three_languages() {
    let (title, body) = crate::native_fatal_text("window creation failed");
    assert_eq!(title, "DZBuild POS");
    for lang in LANGS {
        let text =
            std::fs::read_to_string(repo_file(&format!("src/locales/{lang}/common.json"))).unwrap();
        let catalogue: Value = serde_json::from_str(&text).unwrap();
        let paragraph = catalogue["nativeFatal"]["body"].as_str().unwrap();
        assert!(body.contains(paragraph), "{lang} is missing");
    }
    assert!(body.ends_with("window creation failed"));
}

/// Review finding (Task 8): `core:event` is ACL-controlled, so the typed events that
/// `specta_builder()` exports (`print-failed`) reach the window only when its capability lets
/// it listen and unlisten, or `events.printFailed.listen` is refused and no toast ever shows
/// (spec §7.1). Emitting stays the backend's: the webview gets no emit permission.
#[test]
fn the_window_may_listen_to_the_exported_events_but_not_emit_them() {
    assert!(
        generated_bindings().contains("makeEvent<PrintFailed>(\"print-failed\")"),
        "specta_builder() no longer exports print-failed"
    );
    let text = std::fs::read_to_string(manifest_dir().join("capabilities/default.json")).unwrap();
    let capability: Value = serde_json::from_str(&text).unwrap();
    let permissions: BTreeSet<&str> = capability["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|p| p.as_str().or_else(|| p["identifier"].as_str()))
        .collect();
    for needed in ["core:event:allow-listen", "core:event:allow-unlisten"] {
        assert!(
            permissions.contains(needed),
            "capabilities/default.json lacks {needed}"
        );
    }
    for refused in [
        "core:default",
        "core:event:default",
        "core:event:allow-emit",
        "core:event:allow-emit-to",
    ] {
        assert!(
            !permissions.contains(refused),
            "capabilities/default.json must not grant {refused}"
        );
    }
}

#[test]
fn every_report_column_and_tag_has_a_label_in_every_language() {
    let shop = crate::commands::testshop::shop();
    shop.owner();
    let (promo, count) = crate::commands::testshop::promotion_and_count(&shop);
    // The pack kinds run while their module is on.
    let actor = shop.owner.actor();
    shop.st
        .db
        .write(move |tx| pos_core::packs::set_modules(tx, &actor, &pos_core::packs::Module::ALL))
        .unwrap();
    for kind in pos_core::reports::ReportKind::ALL {
        let mut request = pos_core::reports::ReportRequest::new(kind, "2026-09-01", "2026-09-30");
        request.promotion_id = Some(promo.clone());
        request.count_id = Some(count.clone());
        let view = crate::commands::reports::run(&shop.st, request).unwrap();
        for lang in pos_core::lang::Lang::ALL {
            for column in &view.report.table.columns {
                assert!(
                    pos_docs::sheet::column_label(lang, &column.key).is_some(),
                    "{} columns.{}",
                    lang.as_str(),
                    column.key
                );
            }
            assert!(
                pos_docs::sheet::report_title(lang, kind.as_str()).is_some(),
                "{} reports.{}",
                lang.as_str(),
                kind.as_str()
            );
            assert!(pos_docs::sheet::group_title(lang, kind.group().as_str()).is_some());
        }
    }
}

#[test]
fn the_customer_display_capability_allows_two_commands_and_listening_only() {
    let read = |p: &str| -> Value {
        serde_json::from_str(&std::fs::read_to_string(manifest_dir().join(p)).unwrap()).unwrap()
    };
    let display = read("capabilities/customer-display.json");
    assert_eq!(display["windows"], serde_json::json!(["customer-display"]));
    assert_eq!(
        display["permissions"],
        serde_json::json!([
            "customer-display-commands",
            "core:event:allow-listen",
            "core:event:allow-unlisten"
        ])
    );
    let main = read("capabilities/default.json");
    assert_eq!(
        main["windows"],
        serde_json::json!(["main"]),
        "the display window never gets pos-commands"
    );
    let toml =
        std::fs::read_to_string(manifest_dir().join("permissions/pos-commands.toml")).unwrap();
    let block = toml
        .split("[[permission]]")
        .find(|b| b.contains("\"customer-display-commands\""))
        .unwrap();
    let allowed: Vec<&str> = block
        .split('"')
        .filter(|s| s.contains('_') && !s.contains('-'))
        .collect();
    assert_eq!(allowed, ["display_current", "display_shop"]);
}

#[test]
fn the_bundled_config_carries_no_secret() {
    let text = std::fs::read_to_string(repo_file("src-tauri/dzbuild.toml")).unwrap();
    let lower = text.to_lowercase();
    for needle in [
        "dzas_", "dzpk_", "secret", "api_key", "apikey", "password", "token",
    ] {
        let hits = lower
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .filter(|l| l.contains(needle))
            .count();
        assert_eq!(hits, 0, "{needle} in dzbuild.toml");
    }
    let config = crate::cloud::bundled_config().unwrap();
    assert_eq!(config.client_id, pos_core::cloud::config::PUBLIC_CLIENT_ID);
    assert!(config.issuer.starts_with("https://") && config.api_base.starts_with("https://"));
    // nothing else in the shell's configuration or installer names a DZBuild credential
    for file in [
        "src-tauri/tauri.conf.json",
        "src-tauri/capabilities/default.json",
    ] {
        let text = std::fs::read_to_string(repo_file(file))
            .unwrap()
            .to_lowercase();
        assert!(!text.contains("dzas_") && !text.contains("dzpk_"), "{file}");
    }
}

/// The updater the app starts with (M6 signing rules 1–3): the plugin accepts the committed
/// config (a bad one stops the app at start), its key is a minisign public key, the endpoint is
/// the stable `https://` manifest, the signed version is required, no downgrade, the setup runs
/// passive, and a plain build makes no updater artifact.
#[test]
fn the_updater_takes_signed_https_updates_only() {
    use base64::Engine;
    let text = std::fs::read_to_string(manifest_dir().join("tauri.conf.json")).unwrap();
    let conf: Value = serde_json::from_str(&text).unwrap();
    let updater: tauri_plugin_updater::Config =
        serde_json::from_value(conf["plugins"]["updater"].clone()).unwrap();
    let public = base64::engine::general_purpose::STANDARD
        .decode(updater.pubkey.trim())
        .unwrap();
    let public = String::from_utf8(public).unwrap();
    assert!(
        public.contains("minisign public key") && !public.contains("secret key"),
        "plugins.updater.pubkey is the public half (scripts/release/set-pubkey.mjs)"
    );
    assert_eq!(
        updater
            .endpoints
            .iter()
            .map(|u| u.as_str())
            .collect::<Vec<_>>(),
        ["https://dzbuild.com/pos/updates/stable/latest.json"]
    );
    assert!(updater.require_signed_version);
    assert!(!updater.allow_downgrades);
    assert!(!updater.dangerous_insecure_transport_protocol);
    assert_eq!(
        updater.windows.map(|w| w.install_mode.nsis_args()),
        Some(["/P"].as_slice())
    );
    assert!(
        conf["bundle"]
            .get("createUpdaterArtifacts")
            .is_none_or(|v| v == false),
        "only scripts/release/build.mjs turns updater artifacts on"
    );
}

/// NFR-002: the app hands Tauri its commands through `tauri::generate_handler!` and never builds
/// tauri-specta's type descriptions, which only write the bindings (about 4 MB of release code,
/// and their work at every start). One list (`app_commands!`) feeds both, so the handler and the
/// bindings cannot drift apart; only a debug build's bindings export calls `specta_builder()`.
#[test]
fn the_app_runs_without_the_bindings_type_descriptions() {
    let lib = std::fs::read_to_string(manifest_dir().join("src").join("lib.rs")).unwrap();
    let compact = |text: &str| -> String { text.split_whitespace().collect() };
    let specta = compact(lib.split("pub fn specta_builder()").nth(1).unwrap());
    assert!(
        specta.contains(".commands(app_commands!(collect_commands))"),
        "the bindings take the one command list"
    );
    let run = compact(lib.split("pub fn run()").nth(1).unwrap());
    assert!(
        run.contains("letcommands=tauri_handler(app_commands!(tauri::generate_handler));"),
        "Tauri's handler takes the same list without the type descriptions"
    );
    let calls: Vec<usize> = run
        .match_indices("specta_builder()")
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        calls.len(),
        1,
        "run builds the bindings' builder once, for the export"
    );
    assert!(
        run[..calls[0]].ends_with("#[cfg(debug_assertions)]ifletErr(error)=export_bindings(&"),
        "only a debug build exports the bindings"
    );
    assert_eq!(
        lib.matches("app_commands!(").count(),
        2,
        "one list, two uses"
    );
}

/// The release build is stripped, fully optimised and aborts on panic.
#[test]
fn the_release_profile_is_hardened() {
    let manifest: toml::Value =
        toml::from_str(&std::fs::read_to_string(repo_file("Cargo.toml")).unwrap()).unwrap();
    let release = &manifest["profile"]["release"];
    assert_eq!(release["lto"].as_str(), Some("fat"));
    assert_eq!(release["codegen-units"].as_integer(), Some(1));
    assert_eq!(release["strip"].as_bool(), Some(true));
    assert_eq!(release["panic"].as_str(), Some("abort"));
    assert_eq!(release["opt-level"].as_integer(), Some(3));
}

/// One invoke handler, the gated one.
#[test]
fn every_command_goes_through_the_licence_gate() {
    let lib: String = std::fs::read_to_string(manifest_dir().join("src").join("lib.rs"))
        .unwrap()
        .split_whitespace()
        .collect();
    assert!(
        lib.contains(".invoke_handler(move|invoke|license::handle(invoke,&commands))"),
        "lib.rs wraps the commands in the licence gate"
    );
    assert_eq!(lib.matches(".invoke_handler(").count(), 1);
}

/// The seller's contact is public; nothing secret is compiled in with it.
#[test]
fn the_seller_contact_carries_no_secret() {
    let text = std::fs::read_to_string(repo_file("src-tauri/license.toml"))
        .unwrap()
        .to_lowercase();
    for needle in ["secret", "password", "token", "private", "seed", "dzl1."] {
        let hits = text
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .filter(|l| l.contains(needle))
            .count();
        assert_eq!(hits, 0, "{needle} in license.toml");
    }
}

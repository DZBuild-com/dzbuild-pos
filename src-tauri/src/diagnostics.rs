//! What the Diagnostic screen shows and zips (PLT-019): versions, memory, disk, WebView2, the
//! cold start, the last crash and the crash-dump count, never a customer, a sale or a secret.
//! Nothing leaves the PC: the zip goes where the owner saves it, and the owner sends it.

use crate::state::{AppPaths, AppState};
use pos_core::error::AppError;
use pos_core::redact::{DenyList, mask_personal};
use pos_docs::diagnostics::{BundleSummary, DiagnosticInput, write_bundle};
use serde::Serialize;
use serde_json::{Value, json};
use specta::Type;
use std::path::{Path, PathBuf};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

/// The newest daily log files the zip carries (a week; each is cut to its last 2 MiB).
const LOGS_IN_ZIP: usize = 7;

/// The longest crash message shown or sent: the panic and its place, not a page of text.
const CRASH_MESSAGE_CHARS: usize = 300;

/// The last panic `crash.log` recorded (`logging::write_crash_record`), masked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CrashRecord {
    /// `2026-09-28 12:00:00.000 Algiers`, as the record wrote it.
    pub at: String,
    pub message: String,
}

/// The memory (working set) of this process and of every process it started (the WebView2
/// browser and its renderers): what "idle RAM ≤ 250 MB" measures (NFR-004).
pub fn process_memory_bytes() -> u64 {
    let Ok(own) = sysinfo::get_current_pid() else {
        return 0;
    };
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_memory(),
    );
    // WebView2's renderers are the grandchildren of the app: a few generations are enough, and
    // the bound stops a parent loop (a reused pid) from running forever.
    let descends = |pid: Pid| {
        let mut current = Some(pid);
        for _ in 0..6 {
            match current {
                Some(p) if p == own => return true,
                Some(p) => current = system.process(p).and_then(|process| process.parent()),
                None => return false,
            }
        }
        false
    };
    system
        .processes()
        .iter()
        .filter(|(pid, _)| descends(**pid))
        .map(|(_, process)| process.memory())
        .sum()
}

/// The last record of `crash.log` (`[<time> Algiers] panic in thread '<name>': <panic>`, the
/// panic's own lines after it), masked and cut to [`CRASH_MESSAGE_CHARS`]; `None` without one.
pub fn last_crash(logs_dir: &Path) -> Option<CrashRecord> {
    let text = std::fs::read_to_string(logs_dir.join(crate::logging::CRASH_FILE)).ok()?;
    let lines: Vec<&str> = text.lines().collect();
    let start = lines
        .iter()
        .rposition(|l| l.starts_with('[') && l.contains("] panic in thread "))?;
    let first = lines.get(start)?;
    let end = first.find(']')?;
    let message = std::iter::once(&first[end + 1..])
        .chain(lines.iter().skip(start + 1).copied())
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    Some(CrashRecord {
        at: first[1..end].to_owned(),
        message: mask_personal(&message)
            .chars()
            .take(CRASH_MESSAGE_CHARS)
            .collect(),
    })
}

/// Minidumps Windows Error Reporting kept for `dzbuild-pos.exe` (the installer's LocalDumps
/// key points it at [`crash_dumps_dir`]); they stay on the PC and never go into the zip.
pub fn crash_dump_count(dir: &Path) -> u32 {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.file_name().to_string_lossy().ends_with(".dmp"))
                .count()
        })
        .map(|count| u32::try_from(count).unwrap_or(u32::MAX))
        .unwrap_or(0)
}

/// `%LOCALAPPDATA%\dev.dzbuild.pos\crashdumps`.
pub fn crash_dumps_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(crate::logging::IDENTIFIER)
        .join("crashdumps")
}

/// This PC as support needs to know it. `st` is `None` when the database did not open.
pub fn system_info(webview: Option<String>, st: Option<&AppState>, paths: &AppPaths) -> Value {
    let mut system = System::new();
    system.refresh_memory();
    let mb = |bytes: u64| bytes / 1_048_576;
    // The role on the shop's network as the database holds it now (`sync::role`); the
    // terminal row read at start when it cannot be read.
    let role = st.map(|s| {
        s.db.read(pos_core::sync::role::current)
            .map(|lan| lan.role)
            .unwrap_or(s.terminal.role)
            .as_str()
    });
    json!({
        "appVersion": crate::state::APP_VERSION,
        "schemaVersion": st.map(|s| s.db.report().schema_version),
        "os": System::long_os_version(),
        "kernel": System::kernel_version(),
        "cpuArch": System::cpu_arch(),
        "cpuCores": System::physical_core_count(),
        "ramTotalMb": mb(system.total_memory()),
        "ramAvailableMb": mb(system.available_memory()),
        "dataDiskFreeMb": crate::disks::free_space(Path::new(&paths.data_dir)).map(mb),
        "webview2": webview,
        "terminalCode": st.map(|s| s.terminal.code.clone()),
        "terminalRole": role,
        "demo": paths.demo,
        "coldStartMs": crate::commands::app::ready_ms_if_recorded(),
        "processMemoryMb": mb(process_memory_bytes()),
        "crashDumps": crash_dump_count(&crash_dumps_dir()),
        "lan": Value::Null,
    })
}

/// The newest [`LOGS_IN_ZIP`] daily log files (`dzbuild-pos.<date>.log`), newest first.
fn recent_logs(logs_dir: &Path) -> Vec<PathBuf> {
    let mut logs: Vec<PathBuf> = std::fs::read_dir(logs_dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file())
                .filter(|p| {
                    p.file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with("dzbuild-pos"))
                })
                .collect()
        })
        .unwrap_or_default();
    logs.sort();
    logs.reverse();
    logs.truncate(LOGS_IN_ZIP);
    logs
}

/// The identities to mask, and whether the PC's logs may go in the zip. The logs are this PC's,
/// shared by the real shop and the demo (finding 32): from the demo profile the real shop's
/// customers and suppliers are masked as well, read from its `pos.db` (closed while the demo is
/// open); when that database exists but cannot be read, the logs stay out of the zip.
fn deny_list(st: &AppState, paths: &AppPaths) -> Result<(DenyList, bool), AppError> {
    if !paths.demo {
        return Ok((st.db.read(DenyList::from_db)?, true));
    }
    let real_db = Path::new(&paths.data_dir).join("pos.db");
    if !real_db.exists() {
        return Ok((st.db.read(DenyList::from_db)?, true));
    }
    match st
        .db
        .read(move |demo| DenyList::from_db_and_file(demo, &real_db))
    {
        Ok(deny) => Ok((deny, true)),
        Err(error) => {
            tracing::warn!(code = %error.code(), "the real shop's names could not be read: no logs in the zip");
            Ok((st.db.read(DenyList::from_db)?, false))
        }
    }
}

/// Writes the zip (`pos_docs::diagnostics::write_bundle`) to `out` with the shop's customers and
/// suppliers masked, then journals `diagnostics.exported { entries, bytes }`. No database copy,
/// no crash dump, no secret: only the versions, the settings, the chains' check, this PC's
/// hardware and local choices and the technical logs.
pub fn export_to(
    st: &AppState,
    paths: &AppPaths,
    system: Value,
    out: &Path,
) -> Result<BundleSummary, AppError> {
    let (deny, logs_masked) = deny_list(st, paths)?;
    let snapshot = st.db.read(pos_core::health::snapshot)?;
    let logs_dir = PathBuf::from(&paths.logs_dir);
    let summary = write_bundle(
        &DiagnosticInput {
            system,
            snapshot,
            hardware_json: std::fs::read_to_string(&paths.hardware_config).ok(),
            local_json: std::fs::read_to_string(&paths.local_config).ok(),
            logs: if logs_masked {
                recent_logs(&logs_dir)
            } else {
                Vec::new()
            },
            crash_log: logs_masked.then(|| logs_dir.join(crate::logging::CRASH_FILE)),
            deny: &deny,
            user_dir: std::env::var("USERPROFILE").ok(),
        },
        out,
    )?;
    let actor = st.actor(st.current_session()?.as_ref());
    let (entries, bytes) = (summary.entries.len(), summary.bytes);
    st.db.write(move |tx| {
        pos_core::audit::append(
            tx,
            &actor.event(
                "diagnostics.exported",
                "diagnostics",
                None,
                json!({ "entries": entries, "bytes": bytes }),
            ),
        )
        .map(drop)
    })?;
    Ok(summary)
}

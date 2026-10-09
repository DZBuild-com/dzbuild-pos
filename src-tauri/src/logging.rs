//! Daily rolling log files in `<app data>/logs`, 14 days kept (PLT-018), at this PC's level
//! (`local.json`, changed live), and `crash.log`, which the panic hook writes synchronously. No
//! tokens, no PINs, no pairing codes, no personal data: every line is masked before the file.

use pos_core::error::AppError;
use pos_core::local::LogLevel;
use pos_core::time::{algiers_datetime, now_ms};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, RwLock};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{Registry, fmt, reload};

/// The bundle identifier (`tauri.conf.json`): Tauri keeps the app data in
/// `%APPDATA%\<identifier>` on Windows. A contract test compares the two.
pub const IDENTIFIER: &str = "dev.dzbuild.pos";

/// Written by the panic hook next to the daily log files.
pub const CRASH_FILE: &str = "crash.log";

/// Keeps the background log writer alive.
pub struct LogGuard {
    _guard: WorkerGuard,
}

/// The log writer's guard as managed state. `state::on_exit` drops it last, so the background
/// worker writes the final lines before the process ends.
#[derive(Default)]
pub struct LogSlot(Mutex<Option<LogGuard>>);

impl LogSlot {
    pub fn new(guard: Option<LogGuard>) -> LogSlot {
        LogSlot(Mutex::new(guard))
    }

    /// Drops the guard, which flushes every pending line to the file.
    pub fn flush(&self) {
        match self.0.lock() {
            Ok(mut guard) => drop(guard.take()),
            Err(poisoned) => drop(poisoned.into_inner().take()),
        }
    }
}

/// Set once the global subscriber is installed: `init` is idempotent within a process.
static INSTALLED: OnceLock<()> = OnceLock::new();

/// Where the panic hook writes `crash.log`: first a guess (`default_crash_dir`), then the real
/// logs folder once `setup` knows it.
static CRASH_DIR: RwLock<Option<PathBuf>> = RwLock::new(None);

/// PLT-018: two weeks of daily files.
pub const KEEP_LOG_FILES: usize = 14;

/// The running log's level, changed at once by [`set_level`].
static LEVEL: OnceLock<reload::Handle<LevelFilter, Registry>> = OnceLock::new();

/// Masks personal data and secrets (`pos_core::redact::mask_personal`: phones, e-mails, NIN, PIN
/// hashes, tokens, pairing and authorization codes, URL queries) in every line before it reaches
/// the file; runs on the log writer's own thread, never on a till's.
pub struct Redacting<W: Write>(pub W);

impl<W: Write> Write for Redacting<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let masked = pos_core::redact::mask_personal(&String::from_utf8_lossy(buf));
        self.0.write_all(masked.as_bytes())?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

pub fn to_filter(level: LogLevel) -> LevelFilter {
    match level {
        LogLevel::Warn => LevelFilter::WARN,
        LogLevel::Info => LevelFilter::INFO,
        LogLevel::Debug => LevelFilter::DEBUG,
    }
}

/// Changes the level of the running log at once (Diagnostic screen, `local.json` at start).
pub fn set_level(level: LogLevel) -> Result<(), AppError> {
    let handle = LEVEL
        .get()
        .ok_or_else(|| AppError::Internal("logging is not installed".into()))?;
    handle
        .modify(|filter| *filter = to_filter(level))
        .map_err(|e| AppError::Internal(format!("log level: {e}")))
}

pub fn init(logs_dir: &Path) -> Result<LogGuard, AppError> {
    std::fs::create_dir_all(logs_dir)?;
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("dzbuild-pos")
        .filename_suffix("log")
        .max_log_files(KEEP_LOG_FILES)
        .build(logs_dir)
        .map_err(|e| AppError::Io(e.to_string()))?;
    let (writer, guard) = tracing_appender::non_blocking(Redacting(appender));
    if INSTALLED.set(()).is_ok() {
        let (filter, handle) = reload::Layer::new(LevelFilter::INFO);
        let _ = LEVEL.set(handle);
        let subscriber = tracing_subscriber::registry().with(filter).with(
            fmt::layer()
                .with_writer(writer)
                .with_ansi(false)
                .with_target(true),
        );
        tracing::subscriber::set_global_default(subscriber)
            .map_err(|e| AppError::Internal(format!("logging: {e}")))?;
    }
    Ok(LogGuard { _guard: guard })
}

/// How logging started (`init_best_effort`).
pub struct Logging {
    pub guard: Option<LogGuard>,
    /// The folder the log files go to; `None` when no file logging works this run.
    pub dir: Option<PathBuf>,
    /// Why `logs_dir` could not be used; `None` when it is in use.
    pub failure: Option<AppError>,
}

/// Never fails the start: `logs_dir`, else `fallback_dir`, else no file logging. Each failure
/// is also written to stderr (the only channel left).
pub fn init_best_effort(logs_dir: &Path, fallback_dir: &Path) -> Logging {
    let failure = match init(logs_dir) {
        Ok(guard) => {
            set_crash_dir(logs_dir);
            return Logging {
                guard: Some(guard),
                dir: Some(logs_dir.to_path_buf()),
                failure: None,
            };
        }
        Err(error) => error,
    };
    eprintln!(
        "DZBuild POS: cannot write logs to {}: {failure}",
        logs_dir.display()
    );
    match init(fallback_dir) {
        Ok(guard) => {
            set_crash_dir(fallback_dir);
            tracing::warn!(error = %failure, logs_dir = %logs_dir.display(), "logs moved to the fallback folder");
            Logging {
                guard: Some(guard),
                dir: Some(fallback_dir.to_path_buf()),
                failure: Some(failure),
            }
        }
        Err(error) => {
            eprintln!(
                "DZBuild POS: cannot write logs to {} either: {error}; no log file this run",
                fallback_dir.display()
            );
            Logging {
                guard: None,
                dir: None,
                failure: Some(failure),
            }
        }
    }
}

/// `%TEMP%\dzbuild-pos\logs`: where logs go when the data folder refuses them.
pub fn fallback_logs_dir() -> PathBuf {
    std::env::temp_dir().join("dzbuild-pos").join("logs")
}

/// The crash file's folder before `setup` resolves the data folder: `%APPDATA%\<identifier>\logs`,
/// where Tauri keeps the app data on Windows, else the fallback folder.
pub fn default_crash_dir() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(|dir| PathBuf::from(dir).join(IDENTIFIER).join("logs"))
        .unwrap_or_else(fallback_logs_dir)
}

pub fn set_crash_dir(dir: &Path) {
    match CRASH_DIR.write() {
        Ok(mut slot) => *slot = Some(dir.to_path_buf()),
        Err(poisoned) => *poisoned.into_inner() = Some(dir.to_path_buf()),
    }
}

fn crash_dir() -> Option<PathBuf> {
    match CRASH_DIR.read() {
        Ok(slot) => slot.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

/// Installs the panic hook first thing in `run`, before Tauri builds anything, so even a panic
/// while the window or WebView2 starts leaves `crash.log`. The release build aborts on panic
/// (`panic = "abort"`): the log file's background writer would never get the line, so the hook
/// writes and syncs `crash.log` itself before it logs through tracing. Not installed under
/// `cargo test`, where failing tests need the default hook's output.
pub fn install_crash_hook(dir: &Path) {
    set_crash_dir(dir);
    if cfg!(test) {
        return;
    }
    std::panic::set_hook(Box::new(|info| {
        let text = info.to_string();
        match crash_dir() {
            Some(dir) => {
                if let Err(error) = write_crash_record(&dir, &text) {
                    eprintln!("DZBuild POS: could not write {CRASH_FILE}: {error}");
                }
            }
            None => eprintln!("DZBuild POS: {text}"),
        }
        tracing::error!(panic = %text, "panic");
    }));
}

/// Appends one panic (its message and location, as `PanicHookInfo` prints them) with the
/// Algiers time and the thread to `<dir>/crash.log`, and syncs the file before returning.
pub fn write_crash_record(dir: &Path, what: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(CRASH_FILE);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    let at = algiers_datetime(now_ms()).format("%Y-%m-%d %H:%M:%S%.3f");
    let thread = std::thread::current();
    let name = thread.name().unwrap_or("unnamed");
    writeln!(file, "[{at} Algiers] panic in thread '{name}': {what}")?;
    file.sync_all()?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logs_go_to_a_daily_file() {
        let dir = tempfile::tempdir().unwrap();
        let guard = init(dir.path()).unwrap();
        tracing::info!("probe line");
        drop(guard); // flushes the background writer
        let names: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            names
                .iter()
                .any(|n| n.starts_with("dzbuild-pos") && n.ends_with(".log")),
            "{names:?}"
        );
    }

    #[test]
    fn the_crash_record_is_on_disk_when_the_hook_returns() {
        let dir = tempfile::tempdir().unwrap();
        let logs = dir.path().join("logs");
        let first = "panicked at src/state.rs:42:7:\nfirst failure";
        let path = write_crash_record(&logs, first).unwrap();
        write_crash_record(&logs, "panicked at src/lib.rs:1:1:\nsecond").unwrap();
        assert_eq!(path, logs.join(CRASH_FILE));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("src/state.rs:42:7") && text.contains("first failure"));
        assert!(text.contains("second"), "appended, not replaced");
        assert!(text.contains("Algiers] panic in thread"), "{text}");
    }

    #[test]
    fn no_usable_folder_means_no_file_logging_and_a_reason() {
        let dir = tempfile::tempdir().unwrap();
        let (logs, fallback) = (dir.path().join("logs"), dir.path().join("fallback"));
        std::fs::write(&logs, b"a file where the folder should be").unwrap();
        std::fs::write(&fallback, b"another file").unwrap();
        let logging = init_best_effort(&logs, &fallback);
        assert!(logging.guard.is_none() && logging.dir.is_none());
        assert_eq!(
            logging.failure.map(|e| e.code()).as_deref(),
            Some("io_error")
        );
    }

    #[test]
    fn log_lines_are_masked_before_they_reach_the_file() {
        let mut sink = Redacting(Vec::new());
        sink.write_all(b"INFO customer phone 0550 12 34 56 hash $argon2id$v=19$x\n")
            .unwrap();
        let text = String::from_utf8(sink.0).unwrap();
        assert!(
            text.contains("<phone>") && !text.contains("0550") && !text.contains("argon2"),
            "{text}"
        );
        assert_eq!(KEEP_LOG_FILES, 14);
        assert_eq!(
            to_filter(pos_core::local::LogLevel::Debug),
            LevelFilter::DEBUG
        );
    }

    /// The log file's bytes, as the subscriber formats them.
    #[derive(Clone, Default)]
    struct Captured(std::sync::Arc<Mutex<Vec<u8>>>);

    impl Write for Captured {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            match self.0.lock() {
                Ok(mut bytes) => bytes.extend_from_slice(buf),
                Err(poisoned) => poisoned.into_inner().extend_from_slice(buf),
            }
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Security rule (binding): no token, PIN, pairing code or personal data reaches a log file,
    /// whatever field, debug print or escaped JSON carries it; error codes stay readable.
    #[test]
    fn no_token_pin_pairing_code_or_personal_data_reaches_the_log_file() {
        #[derive(Debug)]
        #[allow(dead_code)] // read through `Debug` only
        struct JoinRequest {
            address: &'static str,
            code: &'static str,
            terminal_code: &'static str,
        }
        let captured = Captured::default();
        let writer = captured.clone();
        // tracing caches each callsite's interest process-wide: a second live dispatcher makes
        // every callsite consult this scoped one too (as in `pos_core::cloud::http`'s log test).
        let _second = tracing::Dispatch::new(tracing_subscriber::registry());
        let subscriber = tracing_subscriber::registry().with(
            fmt::layer()
                .with_writer(move || Redacting(writer.clone()))
                .with_ansi(false)
                .without_time(), // a timestamp's digits could hold "1234" by chance
        );
        let lan_token = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
        tracing::subscriber::with_default(subscriber, || {
            let request = JoinRequest {
                address: "192.168.1.20:7345",
                code: "482913",
                terminal_code: "C2",
            };
            tracing::info!(?request, "joining the main PC");
            tracing::info!(
                pin = "1234",
                hash = "$argon2id$v=19$m=19456,t=2,p=1$abc$def",
                "sign-in"
            );
            tracing::warn!(token = lan_token, "till token");
            tracing::info!("Authorization: Bearer at_SECRET_ACCESS");
            tracing::info!(
                body = r#"{"refresh_token":"rt_SECRET_REFRESH","access_token":"at_SECRET_2"}"#,
                "oauth answer"
            );
            tracing::info!(
                url = "http://127.0.0.1:53682/callback?code=AC_SECRET_CODE&state=ST_SECRET",
                "redirect"
            );
            tracing::info!(user_code = "WDJB-MJHT", "device link");
            tracing::info!(
                "customer phone 0550 12 34 56 mail karim.b@example.dz NIN 109870123456789012"
            );
            tracing::warn!(code = "backup_invalid", "the daily backup failed");
        });
        let text = match captured.0.lock() {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(poisoned) => String::from_utf8_lossy(&poisoned.into_inner()).into_owned(),
        };
        assert!(text.contains("joining the main PC"), "{text}");
        for leaked in [
            "482913",
            "1234",
            "argon2",
            lan_token,
            "at_SECRET_ACCESS",
            "rt_SECRET_REFRESH",
            "at_SECRET_2",
            "AC_SECRET_CODE",
            "ST_SECRET",
            "WDJB-MJHT",
            "0550",
            "karim.b@example.dz",
            "109870123456789012",
        ] {
            assert!(!text.contains(leaked), "{leaked} leaked: {text}");
        }
        assert!(
            text.contains("code=\"backup_invalid\"") || text.contains("code=backup_invalid"),
            "{text}"
        );
        assert!(text.contains("terminal_code: \"C2\""), "{text}");
    }

    #[test]
    fn the_default_crash_folder_is_the_app_data_logs_folder() {
        let config: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(crate::contract_tests::manifest_dir().join("tauri.conf.json"))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(config["identifier"], IDENTIFIER);
        let dir = default_crash_dir();
        assert!(dir.ends_with(Path::new(IDENTIFIER).join("logs")) || dir == fallback_logs_dir());
    }
}

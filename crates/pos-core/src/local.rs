//! This PC's own choices, kept out of the shop database so that a restore, an import or a LAN
//! sync never changes them: the second backup folder, the log level, light mode, the text size,
//! the on-screen keyboard, and whether this PC is a till of the shop's network.
//! `%APPDATA%\dev.dzbuild.pos\local.json`, written atomically; a missing or damaged file gives
//! the defaults (a till never stops over it).

use crate::audit::{self, Actor};
use crate::error::AppError;
use rusqlite::Transaction;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use specta::Type;
use std::io::Write;
use std::path::Path;

pub const FILE_NAME: &str = "local.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MirrorFolder {
    pub path: String,
    /// The drive's name when Windows gives one ("KINGSTON"), shown beside the path.
    pub label: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Warn,
    #[default]
    Info,
    Debug,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TextSize {
    #[default]
    Normal,
    Large,
    Larger,
}

impl TextSize {
    /// The root font size in percent (UX-009 font scaling).
    pub const fn percent(self) -> u16 {
        match self {
            TextSize::Normal => 100,
            TextSize::Large => 115,
            TextSize::Larger => 130,
        }
    }
}

/// This PC's place on the shop's network as its last start found it: the guided
/// restore reads it when `pos.db` cannot be opened, and pairs a till again instead of rewinding
/// it behind its main PC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LocalLan {
    pub role: LocalLanRole,
    /// The main PC's shop name, for the message that sends the owner to it (a till); empty on
    /// the main PC itself.
    pub host: String,
    /// This PC's terminal (a till's id and code): after its damaged copy is set aside, the
    /// pairing sends the id again so the main PC lets it take over its own terminal and series.
    #[serde(default)]
    pub terminal_id: Option<String>,
    #[serde(default)]
    pub terminal_code: Option<String>,
    /// A main PC's paired tills: its guided restore would go back behind them.
    #[serde(default)]
    pub tills: i64,
}

impl LocalLan {
    /// A till of the main PC named `host`.
    pub fn till(host: &str) -> LocalLan {
        LocalLan {
            role: LocalLanRole::Till,
            host: host.to_owned(),
            terminal_id: None,
            terminal_code: None,
            tills: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum LocalLanRole {
    Till,
    /// The main PC of a shop network.
    Main,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct LocalConfig {
    pub backup_mirror: Option<MirrorFolder>,
    pub log_level: LogLevel,
    pub light_mode: bool,
    pub text_size: TextSize,
    pub onscreen_keyboard: bool,
    /// Kept by the start sequence ([`record_lan`]), never by the UI: not in `changes`.
    pub lan: Option<LocalLan>,
}

impl LocalConfig {
    /// The saved choices, or the defaults when the file is missing or unreadable (logged).
    pub fn load(path: &Path) -> LocalConfig {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|error| {
                tracing::warn!(%error, path = %path.display(), "local.json is unreadable; using the defaults");
                LocalConfig::default()
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => LocalConfig::default(),
            Err(error) => {
                tracing::warn!(%error, path = %path.display(), "local.json cannot be read; using the defaults");
                LocalConfig::default()
            }
        }
    }

    /// Writes `<file>.tmp`, syncs it, renames it over the file. The keys this version does not
    /// know (another writer's, or a newer version's) are kept: these fields are merged into the
    /// file as it is; a damaged file is replaced. `lan` is never written from here: its one
    /// writer is [`record_lan`], so a copy read before a pairing (or before leaving the network)
    /// cannot put a stale place back.
    pub fn save(&self, path: &Path) -> Result<(), AppError> {
        let mut mine = match serde_json::to_value(self)? {
            Value::Object(mine) => mine,
            _ => serde_json::Map::new(),
        };
        mine.remove(LAN_KEY);
        merge_into(path, mine)
    }

    /// `(key, before, after)` of every field that differs, in declaration order; keys are the
    /// JSON names the UI knows.
    pub fn changes(&self, next: &LocalConfig) -> Vec<(&'static str, Value, Value)> {
        let mut out = Vec::new();
        let mut field = |key: &'static str, before: Value, after: Value| {
            if before != after {
                out.push((key, before, after));
            }
        };
        field(
            "backupMirror",
            json!(self.backup_mirror),
            json!(next.backup_mirror),
        );
        field("logLevel", json!(self.log_level), json!(next.log_level));
        field("lightMode", json!(self.light_mode), json!(next.light_mode));
        field("textSize", json!(self.text_size), json!(next.text_size));
        field(
            "onscreenKeyboard",
            json!(self.onscreen_keyboard),
            json!(next.onscreen_keyboard),
        );
        out
    }
}

/// Journals each changed field as `local.updated { key, before, after }`; returns how many.
pub fn journal_changes(
    tx: &Transaction<'_>,
    actor: &Actor,
    before: &LocalConfig,
    after: &LocalConfig,
) -> Result<usize, AppError> {
    let changes = before.changes(after);
    for (key, old, new) in &changes {
        audit::append(
            tx,
            &actor.event(
                "local.updated",
                "local",
                Some(key),
                json!({ "key": key, "before": old, "after": new }),
            ),
        )?;
    }
    Ok(changes.len())
}

/// Writes this PC's place on the network into the `local.json` at `path` when it changed,
/// keeping the PC's other choices; `true` when it wrote. A PC that never was a till writes
/// nothing.
pub fn record_lan(path: &Path, lan: Option<LocalLan>) -> Result<bool, AppError> {
    if LocalConfig::load(path).lan == lan {
        return Ok(false);
    }
    let mut entry = serde_json::Map::new();
    entry.insert(LAN_KEY.to_owned(), serde_json::to_value(&lan)?);
    merge_into(path, entry)?;
    Ok(true)
}

/// The JSON key of [`LocalConfig::lan`], written by [`record_lan`] only.
const LAN_KEY: &str = "lan";

/// Merges `entries` into the JSON object in the file at `path` (a missing or damaged file
/// starts empty): writes `<file>.tmp`, syncs it, renames it over the file.
fn merge_into(path: &Path, entries: serde_json::Map<String, Value>) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut tmp_name = path.as_os_str().to_owned();
    tmp_name.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp_name);
    let mut merged = match std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
    {
        Some(Value::Object(existing)) => existing,
        _ => serde_json::Map::new(),
    };
    merged.extend(entries);
    let text = serde_json::to_string_pretty(&Value::Object(merged))?;
    let written = std::fs::File::create(&tmp).and_then(|mut file| {
        file.write_all(text.as_bytes())?;
        file.sync_all()
    });
    let result = written.and_then(|()| std::fs::rename(&tmp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    Ok(result?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    #[test]
    fn a_missing_or_damaged_file_gives_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        assert_eq!(LocalConfig::load(&path), LocalConfig::default());
        std::fs::write(&path, b"{ not json").unwrap();
        assert_eq!(LocalConfig::load(&path), LocalConfig::default());
        let defaults = LocalConfig::default();
        assert_eq!(
            (
                defaults.log_level,
                defaults.text_size,
                defaults.light_mode,
                defaults.onscreen_keyboard
            ),
            (LogLevel::Info, TextSize::Normal, false, false)
        );
    }

    #[test]
    fn saving_is_atomic_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        let config = LocalConfig {
            backup_mirror: Some(MirrorFolder {
                path: "E:\\Sauvegardes POS".into(),
                label: Some("USB".into()),
            }),
            log_level: LogLevel::Debug,
            light_mode: true,
            text_size: TextSize::Large,
            onscreen_keyboard: true,
            lan: None,
        };
        config.save(&path).unwrap();
        assert_eq!(LocalConfig::load(&path), config);
        assert!(!dir.path().join("local.json.tmp").exists());
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("\"backupMirror\"") && text.contains("\"textSize\": \"large\""),
            "{text}"
        );
        assert_eq!(TextSize::Larger.percent(), 130);
    }

    /// Another writer's keys (the LAN role a pairing writes, a newer version's choices) survive a
    /// change saved here: the file is merged, never replaced by this version's fields alone.
    #[test]
    fn saving_keeps_the_keys_this_version_does_not_know() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        std::fs::write(
            &path,
            br#"{ "lan": { "role": "till", "host": "CAISSE-PRINCIPALE" }, "lightMode": false }"#,
        )
        .unwrap();
        let mut config = LocalConfig::load(&path);
        config.light_mode = true;
        config.save(&path).unwrap();
        let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            saved["lan"],
            json!({ "role": "till", "host": "CAISSE-PRINCIPALE" })
        );
        assert_eq!(saved["lightMode"], json!(true));
        assert_eq!(LocalConfig::load(&path), config);
        std::fs::write(&path, b"{ not json").unwrap();
        LocalConfig::default().save(&path).unwrap();
        assert_eq!(
            LocalConfig::load(&path),
            LocalConfig::default(),
            "a damaged file is replaced"
        );
    }

    #[test]
    fn every_changed_field_is_journaled() {
        let before = LocalConfig::default();
        let mut after = before.clone();
        after.light_mode = true;
        after.log_level = LogLevel::Debug;
        let changes = before.changes(&after);
        let keys: Vec<&str> = changes.iter().map(|(k, ..)| *k).collect();
        assert_eq!(keys, ["logLevel", "lightMode"]);
        let db = Db::open_memory().unwrap();
        let written = db
            .write(move |tx| {
                journal_changes(tx, &crate::audit::Actor::system(None), &before, &after)
            })
            .unwrap();
        assert_eq!(written, 2);
        let payload: String = db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT payload FROM audit_log WHERE action = 'local.updated' ORDER BY seq LIMIT 1",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert!(
            payload.contains("\"key\":\"logLevel\"") && payload.contains("\"after\":\"debug\""),
            "{payload}"
        );
    }
}

#[cfg(test)]
mod lan_tests {
    use super::*;

    fn light() -> LocalConfig {
        LocalConfig {
            light_mode: true,
            ..LocalConfig::default()
        }
    }

    /// A till is written down in `local.json`, so the guided restore can refuse to
    /// rewind it when its database cannot be opened; the PC's other choices are kept.
    #[test]
    fn this_pcs_place_on_the_network_is_kept_with_its_other_choices() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        assert!(
            !record_lan(&path, None).unwrap(),
            "a single PC writes nothing"
        );
        assert!(!path.exists());
        light().save(&path).unwrap();
        let till = LocalLan::till("Supérette El Amel");
        assert!(record_lan(&path, Some(till.clone())).unwrap());
        assert!(
            !record_lan(&path, Some(till.clone())).unwrap(),
            "unchanged: not written again"
        );
        let saved = LocalConfig::load(&path);
        assert_eq!((saved.lan, saved.light_mode), (Some(till.clone()), true));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("\"lan\": {")
                && text.contains("\"role\": \"till\"")
                && text.contains("\"host\": \"Supérette El Amel\""),
            "{text}"
        );
        assert!(record_lan(&path, None).unwrap(), "the PC left the network");
        assert_eq!(LocalConfig::load(&path), light());
        std::fs::write(&path, br#"{ "lightMode": true }"#).unwrap();
        assert_eq!(
            LocalConfig::load(&path),
            light(),
            "a file from before the key"
        );
        let paired = LocalConfig {
            lan: Some(till),
            ..LocalConfig::default()
        };
        assert!(
            LocalConfig::default().changes(&paired).is_empty(),
            "not a choice of the UI: pairing and leaving journal themselves"
        );
    }

    /// One writer for `lan` (the M6 merge): a copy of the choices read before the PC paired, or
    /// before it left the network, saves its own fields only and never puts its stale `lan`
    /// back into the file.
    #[test]
    fn a_saved_choice_never_writes_the_network_place() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        let till = LocalLan::till("Supérette El Amel");
        let before_pairing = LocalConfig::load(&path);
        assert!(record_lan(&path, Some(till.clone())).unwrap());
        let mut cached = before_pairing;
        cached.light_mode = true;
        cached.save(&path).unwrap();
        let saved = LocalConfig::load(&path);
        assert_eq!(
            (saved.lan, saved.light_mode),
            (Some(till.clone()), true),
            "a copy from before the pairing keeps the till's place"
        );
        let mut while_till = LocalConfig::load(&path);
        assert!(record_lan(&path, None).unwrap(), "the PC left the network");
        while_till.text_size = TextSize::Large;
        while_till.save(&path).unwrap();
        let saved = LocalConfig::load(&path);
        assert_eq!(
            (saved.lan, saved.text_size),
            (None, TextSize::Large),
            "a copy from while it was a till does not bring the place back"
        );
    }
}

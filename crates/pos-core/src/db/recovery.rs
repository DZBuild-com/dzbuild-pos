//! What the fatal screen needs when the database cannot open (the M0 review leftover): a
//! preview of each backup read-only, the owner's PIN checked against the backup's own users
//! (the live file is unreadable), a lockout that survives restarts, and the guided restore:
//! the damaged files are moved aside — never deleted — and the existing restore marker makes
//! the next start swap the backup in and journal it (`backup.restored`, `guided: true`).

use super::backup::{
    BackupKind, HighWaterMarks, PendingRestore, RESTORE_FAILED_MARKER, RESTORE_MARKER,
    backup_file_name, high_water_marks, journal_tail, remove_quietly, validate_backup_file,
};
use super::migrations::LATEST_VERSION;
use super::schema_version;
use crate::auth::OWNER_ROLE_ID;
use crate::error::AppError;
use crate::ids::new_id;
use crate::terminal::TerminalRole;
use crate::time::algiers_datetime;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

pub const ATTEMPTS_FILE: &str = "recovery.attempts";
pub const MAX_PIN_FAILURES: u32 = 5;
pub const LOCKOUT_MS: i64 = 15 * 60 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PreviewStatus {
    Valid,
    Invalid,
    NewerThanApp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OwnerTile {
    pub id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LastDocument {
    pub number: String,
    pub at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupPreview {
    pub status: PreviewStatus,
    pub schema_version: i64,
    pub app_schema_version: i64,
    pub company_name: Option<String>,
    /// The last validated ticket: what the shop will find at the top of its history.
    pub last_ticket: Option<LastDocument>,
    pub documents: i64,
    pub products: i64,
    pub customers: i64,
    /// The newest journal row: the last thing anybody did in that copy.
    pub last_activity_at: Option<i64>,
    /// Active owners of that copy: who may restore it.
    pub owners: Vec<OwnerTile>,
    /// Whose copy it is (the demo shop's, a till's, a main PC's); `None` when it cannot be read.
    pub origin: Option<BackupOrigin>,
}

impl BackupPreview {
    fn invalid(status: PreviewStatus, schema_version: i64) -> BackupPreview {
        BackupPreview {
            status,
            schema_version,
            app_schema_version: LATEST_VERSION,
            company_name: None,
            last_ticket: None,
            documents: 0,
            products: 0,
            customers: 0,
            last_activity_at: None,
            owners: Vec::new(),
            origin: None,
        }
    }
}

fn open_read_only(path: &Path) -> Result<Connection, AppError> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| AppError::conflict("backup_invalid"))
}

fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [name],
        |r| r.get(0),
    )
    .unwrap_or(false)
}

fn count(conn: &Connection, table: &str, sql: &str) -> i64 {
    if !table_exists(conn, table) {
        return 0;
    }
    conn.query_row(sql, [], |r| r.get(0)).unwrap_or(0)
}

/// Reads a backup without changing it. `Invalid` when it fails `validate_backup_file` for any
/// reason but a newer schema, `NewerThanApp` for a copy made by a newer DZBuild POS.
pub fn inspect_backup(path: &Path) -> BackupPreview {
    match validate_backup_file(path) {
        Ok(()) => {}
        Err(error) if error.code() == "database_newer_than_app" => {
            let version = open_read_only(path)
                .ok()
                .and_then(|c| schema_version(&c).ok())
                .unwrap_or(0);
            return BackupPreview::invalid(PreviewStatus::NewerThanApp, version);
        }
        Err(_) => return BackupPreview::invalid(PreviewStatus::Invalid, 0),
    }
    let Ok(conn) = open_read_only(path) else {
        return BackupPreview::invalid(PreviewStatus::Invalid, 0);
    };
    let company_name = conn
        .query_row(
            "SELECT name_fr, name_ar, name_en FROM company WHERE id = 1",
            [],
            |r| {
                Ok([
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ])
            },
        )
        .optional()
        .ok()
        .flatten()
        .and_then(|names| names.into_iter().find(|n| !n.trim().is_empty()));
    let last_ticket = if table_exists(&conn, "documents") {
        conn.query_row(
            "SELECT number, validated_at FROM documents
             WHERE status = 'validated' AND doc_type = 'ticket'
             ORDER BY validated_at DESC, chain_seq DESC LIMIT 1",
            [],
            |r| {
                Ok(LastDocument {
                    number: r.get(0)?,
                    at: r.get(1)?,
                })
            },
        )
        .optional()
        .ok()
        .flatten()
    } else {
        None
    };
    let owners = conn
        .prepare(
            "SELECT id, display_name FROM users WHERE role_id = ?1 AND active = 1 ORDER BY display_name",
        )
        .and_then(|mut stmt| {
            stmt.query_map([OWNER_ROLE_ID], |r| {
                Ok(OwnerTile {
                    id: r.get(0)?,
                    display_name: r.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
        })
        .unwrap_or_default();
    BackupPreview {
        status: PreviewStatus::Valid,
        schema_version: schema_version(&conn).unwrap_or(0),
        app_schema_version: LATEST_VERSION,
        company_name,
        last_ticket,
        documents: count(
            &conn,
            "documents",
            "SELECT COUNT(*) FROM documents WHERE status = 'validated'",
        ),
        products: count(&conn, "products", "SELECT COUNT(*) FROM products"),
        customers: count(&conn, "customers", "SELECT COUNT(*) FROM customers"),
        last_activity_at: conn
            .query_row("SELECT MAX(at) FROM audit_log", [], |r| {
                r.get::<_, Option<i64>>(0)
            })
            .ok()
            .flatten(),
        owners,
        origin: origin_of(&conn).ok(),
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Attempts {
    failures: u32,
    locked_until: Option<i64>,
}

fn attempts_path(data_dir: &Path) -> PathBuf {
    data_dir.join(ATTEMPTS_FILE)
}

fn read_attempts(data_dir: &Path) -> Attempts {
    std::fs::read(attempts_path(data_dir))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// A lock holds while `now < until`, and only within one lockout of `now` (a lock dated
/// further ahead was written by a clock that ran fast).
fn locked(attempts: &Attempts, now: i64) -> bool {
    attempts
        .locked_until
        .is_some_and(|until| now < until && until - now <= LOCKOUT_MS)
}

/// The owner's PIN against the chosen backup's own users (the live database cannot be read).
/// Wrong PINs are counted in `recovery.attempts` in the data folder, so restarting the app
/// does not reset them; the fifth in a row locks for 15 minutes.
pub fn check_owner_pin(
    data_dir: &Path,
    backup: &Path,
    user_id: &str,
    pin: &str,
    now: i64,
) -> Result<(), AppError> {
    let mut attempts = read_attempts(data_dir);
    if locked(&attempts, now) {
        return Err(AppError::auth("too_many_attempts"));
    }
    let conn = open_read_only(backup)?;
    let user: Option<(String, String, bool)> = conn
        .query_row(
            "SELECT pin_hash, role_id, active FROM users WHERE id = ?1",
            [user_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((hash, role, active)) = user else {
        return Err(AppError::not_found("user"));
    };
    if role != OWNER_ROLE_ID {
        return Err(AppError::auth("owner_only"));
    }
    if !active {
        return Err(AppError::auth("user_inactive"));
    }
    if crate::auth::pin::verify_pin(pin, &hash)? {
        remove_quietly(&attempts_path(data_dir));
        return Ok(());
    }
    attempts.failures += 1;
    let now_locked = attempts.failures >= MAX_PIN_FAILURES;
    if now_locked {
        attempts = Attempts {
            failures: 0,
            locked_until: Some(now + LOCKOUT_MS),
        };
    } else {
        attempts.locked_until = None;
    }
    std::fs::write(attempts_path(data_dir), serde_json::to_vec(&attempts)?)?;
    tracing::warn!(
        user_id,
        locked = now_locked,
        "wrong owner PIN at the guided restore"
    );
    Err(AppError::auth(if now_locked {
        "too_many_attempts"
    } else {
        "invalid_pin"
    }))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GuidedRestore {
    /// `backups\corrupt-<YYYYMMDD-HHMMSS>`: the damaged files, kept for support. Empty when the
    /// data folder held no damaged file and no earlier guided restore left one.
    pub damaged_folder: String,
    /// The file names moved there, in order.
    pub moved: Vec<String>,
    /// The backup the next start restores (inside the backups folder).
    pub restored_from: String,
}

/// The files of a damaged database, in the order they are moved.
fn damaged_files(db_path: &Path) -> Vec<PathBuf> {
    [
        "",
        "-wal",
        "-shm",
        "-wal.replaced",
        "-shm.replaced",
        ".restoring",
    ]
    .iter()
    .map(|suffix| {
        let mut name = db_path.as_os_str().to_owned();
        name.push(suffix);
        PathBuf::from(name)
    })
    .collect()
}

/// Brings a backup that lives elsewhere (a USB stick) into `backups_dir` as a `manual` copy:
/// written under `<name>.tmp`, synced, checked, then renamed, so an interrupted or bad copy
/// leaves no file under a backup name.
fn bring_in(source: &Path, backups_dir: &Path, now: i64) -> Result<PathBuf, AppError> {
    let target = backups_dir.join(backup_file_name(now, BackupKind::Manual));
    let mut tmp = target.clone().into_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let outcome = std::fs::copy(source, &tmp)
        .and_then(|_| {
            std::fs::OpenOptions::new()
                .write(true)
                .open(&tmp)?
                .sync_all()
        })
        .map_err(AppError::from)
        .and_then(|()| validate_backup_file(&tmp))
        .and_then(|()| std::fs::rename(&tmp, &target).map_err(AppError::from));
    if let Err(error) = outcome {
        remove_quietly(&tmp);
        return Err(error);
    }
    Ok(target)
}

/// The journal anchor and high-water marks of the damaged database at `path`, when it can still
/// be read. A zero-byte file is not opened at all: SQLite deletes the `-wal` of an empty
/// database, even on a read-only connection, and that WAL is kept for support.
fn damaged_marks(path: &Path) -> DamagedMarks {
    if !std::fs::metadata(path).is_ok_and(|meta| meta.len() > 0) {
        return (None, None);
    }
    (journal_tail(path).ok(), high_water_marks(path).ok())
}

/// Moves the files of the damaged database into `folder`, all or none: a failed move puts back
/// what had moved. A file already in `folder` is never replaced (Windows' rename would).
/// Returns each file's path before and after, in order.
fn move_aside(db_path: &Path, folder: &Path) -> Result<Vec<(PathBuf, PathBuf)>, AppError> {
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    for file in damaged_files(db_path) {
        if !file.exists() {
            continue;
        }
        let Some(name) = file.file_name() else {
            continue;
        };
        let target = folder.join(name);
        let result = if target.exists() {
            Err(std::io::Error::from(std::io::ErrorKind::AlreadyExists))
        } else {
            std::fs::rename(&file, &target)
        };
        if let Err(error) = result {
            put_back(&moved, folder);
            return Err(error.into());
        }
        moved.push((file, target));
    }
    Ok(moved)
}

/// Undoes [`move_aside`], newest first, then removes `folder` if it is empty (a file that could
/// not go back stays in it).
fn put_back(moved: &[(PathBuf, PathBuf)], folder: &Path) {
    for (from, to) in moved.iter().rev() {
        if let Err(error) = std::fs::rename(to, from) {
            tracing::error!(%error, path = %to.display(), "could not put a damaged file back");
        }
    }
    let _ = std::fs::remove_dir(folder);
}

/// Creates (or truncates) `path` with `text`, synced to the disk before it returns.
fn write_synced(path: &Path, text: &str) -> std::io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()
}

/// Replaces `path` in one step: `<path>.tmp` is written and synced, then renamed over it, so a
/// crash leaves the old file or the new one, never part of one.
fn replace_synced(path: &Path, text: &str) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let outcome = write_synced(&tmp, text).and_then(|()| std::fs::rename(&tmp, path));
    if outcome.is_err() {
        remove_quietly(&tmp);
    }
    outcome
}

/// Written in the data folder before the first damaged file moves, and left there until a start
/// finds `pos.db` again (`refuse_empty_start` removes it then). It names the damaged folder:
/// while it is there without `pos.db`, a start is refused (`database_missing`) instead of making
/// an empty shop, and the next guided restore finishes the move into the same folder.
pub const MOVING_MARKER: &str = "restore.moving";

/// One guided restore at a time (the app runs as a single instance): a double click must not
/// move the damaged files into two folders.
static SCHEDULING: Mutex<()> = Mutex::new(());

/// The journal anchor (`seq`, `hash`) and the high-water marks of a damaged database.
type DamagedMarks = (Option<(i64, String)>, Option<HighWaterMarks>);

/// The damaged folder of a guided restore that did not finish (a power cut in the middle of the
/// move, or a restore that then failed at the start): `pos.db` is gone and `restore.moving`
/// names a `corrupt-*` folder of `backups_dir` that exists.
fn interrupted_move(db_path: &Path, backups_dir: &Path) -> Option<PathBuf> {
    if db_path.exists() {
        return None;
    }
    let text = std::fs::read_to_string(db_path.with_file_name(MOVING_MARKER)).ok()?;
    let folder = PathBuf::from(text.trim());
    let named = folder
        .file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with("corrupt-"));
    (named && folder.parent() == Some(backups_dir) && folder.is_dir()).then_some(folder)
}

/// Writes `restore.moving` (naming `folder`), then moves the damaged files into `folder`, all or
/// none. On a failure `restore.moving` goes too once `pos.db` is back in place; while it is not,
/// the marker stays and keeps a start from making an empty shop.
fn move_into(
    db_path: &Path,
    folder: &Path,
    moving: &Path,
) -> Result<Vec<(PathBuf, PathBuf)>, AppError> {
    let outcome = replace_synced(moving, &folder.to_string_lossy())
        .and_then(|()| std::fs::create_dir_all(folder))
        .map_err(AppError::from)
        .and_then(|()| move_aside(db_path, folder));
    if outcome.is_err() && db_path.exists() {
        remove_quietly(moving);
    }
    outcome
}

/// Schedules the restore of `source` over a database that cannot open (see the module doc).
/// Order: refuse while a restore is already scheduled (`restore_already_scheduled`) → validate
/// (nothing moves when either fails) → bring a copy that lives elsewhere into the backups folder
/// (the stick may be gone at the restart) → write `restore.moving` → move the damaged files
/// aside, all or none → write `restore.pending` → read what can still be read of the damaged
/// journal and put its anchor and marks into the marker. From the first move on, a crash leaves
/// `restore.moving` (the start is refused, the next guided restore finishes the move) or the
/// marker (the next start restores). The journal is read last, from the moved files: the read
/// can take seconds on a large file, and a read-only open of a WAL database creates its `-shm`,
/// which belongs neither next to the next database nor among the files the damaged one left.
/// The files of an interrupted guided restore are finished into its folder; with nothing to
/// move and no such folder, no damaged folder is made.
pub fn schedule_guided_restore(
    db_path: &Path,
    backups_dir: &Path,
    source: &Path,
    requested_by: &str,
    now: i64,
) -> Result<GuidedRestore, AppError> {
    schedule_with(
        db_path,
        backups_dir,
        source,
        requested_by,
        now,
        damaged_marks,
    )
}

/// [`schedule_guided_restore`] with the read of the damaged journal passed in (the tests cut the
/// power there).
fn schedule_with(
    db_path: &Path,
    backups_dir: &Path,
    source: &Path,
    requested_by: &str,
    now: i64,
    read_marks: impl FnOnce(&Path) -> DamagedMarks,
) -> Result<GuidedRestore, AppError> {
    let _one_at_a_time = SCHEDULING.lock().unwrap_or_else(PoisonError::into_inner);
    let marker = db_path.with_file_name(RESTORE_MARKER);
    if marker.is_file() {
        return Err(AppError::conflict("restore_already_scheduled"));
    }
    validate_backup_file(source)?;
    std::fs::create_dir_all(backups_dir)?;
    let local = if source.parent() == Some(backups_dir) {
        source.to_path_buf()
    } else {
        bring_in(source, backups_dir, now)?
    };
    let moving = db_path.with_file_name(MOVING_MARKER);
    let anything_to_move = damaged_files(db_path).iter().any(|file| file.exists());
    let folder = match interrupted_move(db_path, backups_dir) {
        Some(folder) => Some(folder),
        None if anything_to_move => Some(backups_dir.join(format!(
            "corrupt-{}",
            algiers_datetime(now).format("%Y%m%d-%H%M%S")
        ))),
        None => None,
    };
    let moved = match &folder {
        Some(folder) if anything_to_move => move_into(db_path, folder, &moving)?,
        _ => Vec::new(),
    };
    let mut pending = PendingRestore {
        source: local.clone(),
        id: Some(new_id()),
        requested_by: Some(requested_by.to_owned()),
        pre_restore_file: None,
        replaced_last_seq: None,
        replaced_last_hash: None,
        replaced_marks: None,
        damaged_folder: folder.clone(),
    };
    // Written as soon as the files have moved, before the slow read of the damaged journal.
    // Without it the next start would find no database: the damaged files go back.
    if let Err(error) = write_synced(&marker, &pending.to_text()) {
        remove_quietly(&marker);
        if let Some(folder) = &folder {
            put_back(&moved, folder);
        }
        if db_path.exists() {
            remove_quietly(&moving);
        }
        return Err(error.into());
    }
    let (anchor, marks) = match (&folder, db_path.file_name()) {
        (Some(folder), Some(name)) => read_marks(&folder.join(name)),
        _ => (None, None),
    };
    if anchor.is_some() || marks.is_some() {
        pending.replaced_last_seq = anchor.as_ref().map(|a| a.0);
        pending.replaced_last_hash = anchor.map(|a| a.1);
        pending.replaced_marks = marks;
        // The marker on disk already makes the next start restore; only the anchor is lost.
        if let Err(error) = replace_synced(&marker, &pending.to_text()) {
            tracing::warn!(%error, "guided restore scheduled without the anchor of the journal it replaces");
        }
    }
    Ok(GuidedRestore {
        damaged_folder: folder
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default(),
        moved: moved
            .iter()
            .filter_map(|(from, _)| from.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect(),
        restored_from: local.to_string_lossy().into_owned(),
    })
}

/// Called at each start after `apply_pending_restore`, before `Db::open`. A start must never
/// create an empty shop where a guided restore moved the damaged files aside: without `pos.db`,
/// `restore.failed` (the restore failed) or `restore.moving` (the move was cut short, or its
/// restore failed) sends it back to the fatal screen with `database_missing`. Once `pos.db` is
/// there again, `restore.moving` has done its job and is removed.
pub fn refuse_empty_start(db_path: &Path) -> Result<(), AppError> {
    let moving = db_path.with_file_name(MOVING_MARKER);
    if db_path.exists() {
        remove_quietly(&moving);
        return Ok(());
    }
    if db_path.with_file_name(RESTORE_FAILED_MARKER).exists() || moving.exists() {
        return Err(AppError::conflict("database_missing"));
    }
    Ok(())
}

/// The main PC's name when this PC's `local.json` (at `local_config`) says it is one
/// of its tills: `{"lan": {"role": "till", "host": "<name>"}}`, written by the pairing because a
/// damaged `pos.db` cannot say so. `None` for a single PC, a main PC, or a missing or unreadable
/// file. Empty when the main PC had no name.
pub fn till_of(local_config: &Path) -> Option<String> {
    #[derive(Deserialize)]
    struct Lan {
        role: String,
        #[serde(default)]
        host: String,
    }
    #[derive(Deserialize)]
    struct Local {
        lan: Option<Lan>,
    }
    let bytes = std::fs::read(local_config).ok()?;
    let local: Local = serde_json::from_slice(&bytes).ok()?;
    local
        .lan
        .filter(|lan| lan.role == "till")
        .map(|lan| lan.host)
}

/// What [`set_aside_for_pairing`] moved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SetAside {
    /// `backups\corrupt-<YYYYMMDD-HHMMSS>`: the damaged files, kept for support. Empty when
    /// there was nothing left to move.
    pub damaged_folder: String,
    /// The file names moved there, in order.
    pub moved: Vec<String>,
}

/// A till whose database cannot open is paired again with its main PC, never restored
/// (a backup would take it back behind its main PC). Its damaged files are moved aside as a
/// guided restore moves them, all or none and never deleted, followed by the `restore.failed` an
/// earlier restore left (else a start without `pos.db` stops on it). Nothing is scheduled: the
/// next start makes a new, empty database, and the setup's choice "till of a shop" brings the
/// main PC's copy back. A cut in the middle leaves `restore.moving`: the start is refused
/// (`database_missing`) and this finishes the move into the same folder.
pub fn set_aside_for_pairing(
    db_path: &Path,
    backups_dir: &Path,
    now: i64,
) -> Result<SetAside, AppError> {
    let _one_at_a_time = SCHEDULING.lock().unwrap_or_else(PoisonError::into_inner);
    if db_path.with_file_name(RESTORE_MARKER).is_file() {
        return Err(AppError::conflict("restore_already_scheduled"));
    }
    let moving = db_path.with_file_name(MOVING_MARKER);
    let failed = db_path.with_file_name(RESTORE_FAILED_MARKER);
    let anything_to_move =
        damaged_files(db_path).iter().any(|file| file.exists()) || failed.is_file();
    let folder = match interrupted_move(db_path, backups_dir) {
        Some(folder) => Some(folder),
        None if anything_to_move => Some(backups_dir.join(format!(
            "corrupt-{}",
            algiers_datetime(now).format("%Y%m%d-%H%M%S")
        ))),
        None => None,
    };
    let mut moved = Vec::new();
    if let Some(folder) = folder.as_deref().filter(|_| anything_to_move) {
        moved = move_into(db_path, folder, &moving)?;
        if failed.is_file() {
            let target = folder.join(RESTORE_FAILED_MARKER);
            let result = if target.exists() {
                Err(std::io::Error::from(std::io::ErrorKind::AlreadyExists))
            } else {
                std::fs::rename(&failed, &target)
            };
            if let Err(error) = result {
                put_back(&moved, folder);
                if db_path.exists() {
                    remove_quietly(&moving);
                }
                return Err(error.into());
            }
            moved.push((failed, target));
        }
    }
    let set_aside = SetAside {
        damaged_folder: folder
            .as_ref()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default(),
        moved: moved
            .iter()
            .filter_map(|(from, _)| from.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect(),
    };
    // What the damaged copy still owed its main PC, read from the moved files (best effort):
    // journaled once this PC is a till again, and kept in the folder for support.
    let unsent = match (&folder, db_path.file_name()) {
        (Some(folder), Some(name)) => unsent_in(&folder.join(name)),
        _ => None,
    };
    let note = RejoinNote {
        damaged_folder: set_aside.damaged_folder.clone(),
        moved: set_aside.moved.clone(),
        unsent,
        at: now,
    };
    // Written before `restore.moving` goes: the next start may then make the new, empty
    // database (`refuse_lost_database`), and journals the move once this PC is set up again.
    replace_synced(
        &db_path.with_file_name(REJOIN_MARKER),
        &serde_json::to_string(&note)?,
    )?;
    // The move is complete: the next start may make the new database.
    if let Err(error) = std::fs::remove_file(&moving)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        return Err(error.into());
    }
    Ok(set_aside)
}

/// Written by [`set_aside_for_pairing`] in the data folder: the next start makes an empty
/// database on purpose, and the first start of this PC set up again journals the move
/// (`lan.till_rejoined` once it is a till again, `recovery.set_aside` otherwise).
pub const REJOIN_MARKER: &str = "rejoin.json";

/// Written by the fatal screen's "create a new shop anyway" (the owner's choice, PIN checked
/// against the newest backup): the next start makes an empty shop although backups exist, and
/// journals `recovery.fresh_start` in it.
pub const FRESH_MARKER: &str = "fresh.start";

/// What [`REJOIN_MARKER`] holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RejoinNote {
    pub damaged_folder: String,
    pub moved: Vec<String>,
    /// The operations the damaged copy had not sent to its main PC, when it could be read.
    pub unsent: Option<i64>,
    pub at: i64,
}

/// The pending and refused operations of a till's copy at `path`, when it can be read: they
/// never reached the main PC. A zero-byte or unreadable file gives `None`.
pub fn unsent_in(path: &Path) -> Option<i64> {
    if !std::fs::metadata(path).is_ok_and(|meta| meta.len() > 0) {
        return None;
    }
    let conn = open_read_only(path).ok()?;
    if !table_exists(&conn, "lan_outbox") {
        return Some(0);
    }
    crate::sync::outbox::depth(&conn)
        .ok()
        .map(|depth| depth.pending + depth.rejected)
}

/// The note [`set_aside_for_pairing`] left in `data_dir`, if any.
pub fn rejoin_note(data_dir: &Path) -> Option<RejoinNote> {
    let bytes = std::fs::read(data_dir.join(REJOIN_MARKER)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Removes the markers a start journaled ([`REJOIN_MARKER`], [`FRESH_MARKER`]).
pub fn forget_marker(data_dir: &Path, name: &str) {
    remove_quietly(&data_dir.join(name));
}

/// Finding 10: before a start makes a database where `pos.db` is missing, the shop's backups are
/// looked for (the backups folder, then the second copy): when one holds a valid backup of a POS,
/// the database was lost (an antivirus quarantine, a deletion), and the start stops with
/// `database_missing` so the fatal screen offers the guided restore, instead of making an empty
/// shop. A new install has no backup; a till set aside to pair again ([`REJOIN_MARKER`]) and the
/// owner's "new shop anyway" ([`FRESH_MARKER`]) make their empty start on purpose.
pub fn refuse_lost_database(
    db_path: &Path,
    backups_dir: &Path,
    mirror: Option<&Path>,
) -> Result<(), AppError> {
    if db_path.exists()
        || db_path.with_file_name(REJOIN_MARKER).is_file()
        || db_path.with_file_name(FRESH_MARKER).is_file()
    {
        return Ok(());
    }
    let found = std::iter::once(backups_dir)
        .chain(mirror)
        .flat_map(|dir| super::list_backups_in(dir).unwrap_or_default())
        .any(|info| validate_backup_file(Path::new(&info.path)).is_ok());
    if found {
        tracing::error!("pos.db is missing but the shop's backups are there: offering the restore");
        return Err(AppError::conflict("database_missing"));
    }
    Ok(())
}

/// Who a backup belongs to, read from the copy itself (never from this PC's files): the demo
/// shop's mark, and its place on a shop network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupOrigin {
    /// The demo shop's database (`demo::META_DEMO`).
    pub demo: bool,
    /// Its role on the shop's network (`lan_local`); `standalone` before M5.
    pub lan_role: TerminalRole,
    /// A till's main PC, by its shop name.
    pub host_name: Option<String>,
    /// The copy's own terminal code (a till's `C2`).
    pub terminal_code: Option<String>,
    /// A main PC's paired tills not revoked.
    pub active_tills: i64,
}

fn origin_of(conn: &Connection) -> Result<BackupOrigin, AppError> {
    let demo = crate::demo::is_marked(conn)?;
    let (lan_role, host_name) = if table_exists(conn, "lan_local") {
        let place = crate::sync::role::current(conn)?;
        (place.role, place.host_name)
    } else {
        (TerminalRole::Standalone, None)
    };
    let active_tills = if table_exists(conn, "lan_devices") {
        conn.query_row(
            "SELECT COUNT(*) FROM lan_devices WHERE revoked_at IS NULL",
            [],
            |r| r.get(0),
        )?
    } else {
        0
    };
    Ok(BackupOrigin {
        demo,
        lan_role,
        host_name,
        terminal_code: crate::terminal::local(conn)?.map(|t| t.code),
        active_tills,
    })
}

/// [`BackupOrigin`] of the backup at `path` (read-only).
pub fn backup_origin(path: &Path) -> Result<BackupOrigin, AppError> {
    origin_of(&open_read_only(path)?)
}

/// Whether the backup at `path` may replace the database of this profile (`demo_profile`: the
/// demo shop is open). Read from the backup itself, whatever this PC's own files say:
/// - a demo shop's backup never replaces the real shop (`backup_is_demo`), nor a real shop's
///   backup the demo (`demo_not_allowed`);
/// - a till's copy is never restored (it would go back behind its main PC; it pairs
///   again), nor a main PC's copy that has active tills (it would go back behind them; a till is
///   made the main PC instead): `lan_restore_blocked`.
pub fn restore_guard(path: &Path, demo_profile: bool) -> Result<(), AppError> {
    let origin = backup_origin(path)?;
    match (origin.demo, demo_profile) {
        (true, false) => return Err(AppError::conflict("backup_is_demo")),
        (false, true) => return Err(crate::demo::not_allowed()),
        _ => {}
    }
    match origin.lan_role {
        TerminalRole::Client => Err(AppError::conflict("lan_restore_blocked")),
        TerminalRole::Host if origin.active_tills > 0 => {
            Err(AppError::conflict("lan_restore_blocked"))
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::Actor;
    use crate::auth::users::{self, NewUser};
    use crate::auth::{CASHIER_ROLE_ID, OWNER_ROLE_ID};
    use crate::db::{BackupKind, Db, DbOptions, apply_pending_restore, meta_set};
    use crate::lang::Lang;

    const NOW: i64 = 1_790_593_200_000;

    /// A shop database with an owner (PIN 2580) and a cashier, cleanly closed; returns the
    /// owner's id and a manual backup of it.
    fn shop(dir: &Path) -> (String, PathBuf) {
        let db = Db::open(&dir.join("pos.db"), DbOptions::default()).unwrap();
        crate::setup::bootstrap(&db).unwrap();
        let hash = crate::auth::pin::hash_pin("2580").unwrap();
        let cashier_hash = crate::auth::pin::hash_pin("4826").unwrap();
        let owner = db
            .write(move |tx| {
                let person = |name: &str, role: &str| NewUser {
                    username: None,
                    display_name: name.into(),
                    pin: String::new(),
                    role_id: role.into(),
                    lang: Lang::Fr,
                    discount_limit_bp: 0,
                };
                let owner = users::create(
                    tx,
                    &Actor::system(None),
                    &person("Amine", OWNER_ROLE_ID),
                    &hash,
                )?;
                users::create(
                    tx,
                    &Actor::system(None),
                    &person("Yacine", CASHIER_ROLE_ID),
                    &cashier_hash,
                )?;
                meta_set(tx, "probe", "in the backup")?;
                Ok(owner.id)
            })
            .unwrap();
        let backup = db.backup(BackupKind::Manual).unwrap();
        db.close().unwrap();
        (owner, PathBuf::from(backup.path))
    }

    fn cashier_id(backup: &Path) -> String {
        let conn = Connection::open_with_flags(backup, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        conn.query_row(
            "SELECT id FROM users WHERE display_name = 'Yacine'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn the_preview_tells_what_a_backup_holds() {
        let dir = tempfile::tempdir().unwrap();
        let (owner, backup) = shop(dir.path());
        let preview = inspect_backup(&backup);
        assert_eq!(preview.status, PreviewStatus::Valid);
        assert_eq!(preview.schema_version, preview.app_schema_version);
        assert_eq!(
            preview.owners,
            [OwnerTile {
                id: owner,
                display_name: "Amine".into()
            }]
        );
        assert_eq!((preview.documents, preview.last_ticket.clone()), (0, None));
        assert!(preview.last_activity_at.is_some());
        let garbage = dir.path().join("garbage.db");
        std::fs::write(&garbage, vec![9_u8; 8_192]).unwrap();
        assert_eq!(inspect_backup(&garbage).status, PreviewStatus::Invalid);
        assert!(inspect_backup(&garbage).owners.is_empty());
    }

    #[test]
    fn a_backup_from_a_newer_version_is_refused_and_nothing_is_moved() {
        let dir = tempfile::tempdir().unwrap();
        let (owner, backup) = shop(dir.path());
        {
            let conn = Connection::open(&backup).unwrap();
            conn.pragma_update(
                None,
                "user_version",
                crate::db::migrations::LATEST_VERSION + 1,
            )
            .unwrap();
        }
        assert_eq!(inspect_backup(&backup).status, PreviewStatus::NewerThanApp);
        let db_path = dir.path().join("pos.db");
        let error =
            schedule_guided_restore(&db_path, &dir.path().join("backups"), &backup, &owner, NOW)
                .unwrap_err();
        assert_eq!(error.code(), "database_newer_than_app");
        assert!(db_path.exists(), "the damaged database is still in place");
        assert!(!dir.path().join(RESTORE_MARKER).exists());
        assert!(
            std::fs::read_dir(dir.path().join("backups"))
                .unwrap()
                .all(|e| !e
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("corrupt-")),
            "no damaged folder was made"
        );
    }

    #[test]
    fn a_wrong_pin_moves_nothing_and_five_lock_the_restore() {
        let dir = tempfile::tempdir().unwrap();
        let (owner, backup) = shop(dir.path());
        let data = dir.path();
        for attempt in 1..=4 {
            let error = check_owner_pin(data, &backup, &owner, "0000", NOW + attempt).unwrap_err();
            assert_eq!(error.code(), "invalid_pin", "attempt {attempt}");
        }
        assert_eq!(
            check_owner_pin(data, &backup, &owner, "0000", NOW + 5)
                .unwrap_err()
                .code(),
            "too_many_attempts"
        );
        assert_eq!(
            check_owner_pin(data, &backup, &owner, "2580", NOW + 6)
                .unwrap_err()
                .code(),
            "too_many_attempts",
            "even the right PIN waits for the lock to end"
        );
        check_owner_pin(data, &backup, &owner, "2580", NOW + LOCKOUT_MS + 6).unwrap();
        assert!(
            !data.join(ATTEMPTS_FILE).exists(),
            "a match clears the count"
        );
        let cashier = cashier_id(&backup);
        assert_eq!(
            check_owner_pin(data, &backup, &cashier, "4826", NOW)
                .unwrap_err()
                .code(),
            "owner_only"
        );
        assert_eq!(
            check_owner_pin(data, &backup, "nobody", "2580", NOW)
                .unwrap_err()
                .code(),
            "user_not_found"
        );
        assert!(dir.path().join("pos.db").exists());
    }

    #[test]
    fn the_guided_restore_moves_the_damaged_files_aside_and_the_next_start_swaps_in_the_backup() {
        let dir = tempfile::tempdir().unwrap();
        let (owner, backup) = shop(dir.path());
        let db_path = dir.path().join("pos.db");
        // The till's disk damaged the live file after the backup.
        std::fs::write(&db_path, vec![0x5A_u8; 16_384]).unwrap();
        std::fs::write(dir.path().join("pos.db-wal"), b"torn wal").unwrap();
        // The owner picked the copy on a USB stick.
        let stick = dir.path().join("stick");
        std::fs::create_dir_all(&stick).unwrap();
        let on_stick = stick.join(backup.file_name().unwrap());
        std::fs::copy(&backup, &on_stick).unwrap();
        let backups = dir.path().join("backups");
        let done = schedule_guided_restore(&db_path, &backups, &on_stick, &owner, NOW).unwrap();
        assert!(!db_path.exists() && !dir.path().join("pos.db-wal").exists());
        let damaged = PathBuf::from(&done.damaged_folder);
        assert!(
            damaged.starts_with(&backups)
                && damaged
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("corrupt-")
        );
        assert_eq!(
            std::fs::read(damaged.join("pos.db-wal")).unwrap(),
            b"torn wal",
            "kept, never deleted"
        );
        assert_eq!(done.moved, ["pos.db", "pos.db-wal"]);
        assert!(
            Path::new(&done.restored_from).starts_with(&backups),
            "the stick copy was brought in first"
        );
        std::fs::remove_dir_all(&stick).unwrap(); // the stick leaves before the restart
        let applied = apply_pending_restore(&db_path).unwrap();
        assert!(applied.is_some());
        refuse_empty_start(&db_path).unwrap();
        assert!(!dir.path().join(MOVING_MARKER).exists());
        let db = Db::open(&db_path, DbOptions::default()).unwrap();
        let probe = db.read(|c| crate::db::meta_get(c, "probe")).unwrap();
        assert_eq!(probe.as_deref(), Some("in the backup"));
        let payload: String = db
            .write(|tx| {
                crate::db::journal_pending_restore(tx, None)?;
                Ok(tx.query_row(
                    "SELECT payload FROM audit_log WHERE action = 'backup.restored' ORDER BY seq DESC LIMIT 1",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert!(
            payload.contains("\"guided\":true") && payload.contains("damagedFolder"),
            "{payload}"
        );
    }

    fn marker(dir: &Path) -> PendingRestore {
        PendingRestore::parse(&std::fs::read_to_string(dir.join(RESTORE_MARKER)).unwrap())
    }

    /// A till whose database still opens, as a power cut leaves it: `pos.db` and its WAL, copied
    /// while the database was open, into `<dir>/till`. Returns the owner, a backup (outside the
    /// till's backups folder), the till folder and the last journal `seq` of that database.
    fn damaged_till(dir: &Path) -> (String, PathBuf, PathBuf, i64) {
        let (owner, backup) = shop(dir);
        let till = dir.join("till");
        std::fs::create_dir_all(&till).unwrap();
        let db = Db::open(&dir.join("pos.db"), DbOptions::default()).unwrap();
        db.write(|tx| meta_set(tx, "probe", "after the backup"))
            .unwrap();
        let last_seq: i64 = db
            .read(|c| Ok(c.query_row("SELECT MAX(seq) FROM audit_log", [], |r| r.get(0))?))
            .unwrap();
        for name in ["pos.db", "pos.db-wal"] {
            std::fs::copy(dir.join(name), till.join(name)).unwrap();
        }
        drop(db);
        (owner, backup, till, last_seq)
    }

    fn corrupt_folders(backups: &Path) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(backups) else {
            return Vec::new();
        };
        entries
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.is_dir()
                    && p.file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with("corrupt-"))
            })
            .collect()
    }

    /// A power cut left `pos.db` and its WAL: the journal is read from the moved files, WAL
    /// included, and the `-shm` that read creates stays out of the data folder and the list.
    #[test]
    fn a_readable_damaged_database_anchors_the_journal_it_replaces() {
        let dir = tempfile::tempdir().unwrap();
        let (owner, backup, till, last_seq) = damaged_till(dir.path());
        let db_path = till.join("pos.db");
        let done =
            schedule_guided_restore(&db_path, &till.join("backups"), &backup, &owner, NOW).unwrap();
        assert_eq!(done.moved, ["pos.db", "pos.db-wal"]);
        assert!(!till.join("pos.db-shm").exists());
        assert_eq!(marker(&till).replaced_last_seq, Some(last_seq));
    }

    #[test]
    fn a_zero_byte_database_keeps_its_wal() {
        let dir = tempfile::tempdir().unwrap();
        let (owner, backup) = shop(dir.path());
        let db_path = dir.path().join("pos.db");
        std::fs::write(&db_path, b"").unwrap();
        std::fs::write(dir.path().join("pos.db-wal"), vec![7_u8; 5_000]).unwrap();
        let done =
            schedule_guided_restore(&db_path, &dir.path().join("backups"), &backup, &owner, NOW)
                .unwrap();
        assert_eq!(done.moved, ["pos.db", "pos.db-wal"]);
        let wal = Path::new(&done.damaged_folder).join("pos.db-wal");
        assert_eq!(std::fs::read(wal).unwrap(), vec![7_u8; 5_000]);
        assert_eq!(marker(dir.path()).replaced_last_seq, None);
    }

    /// All or none: a move that fails puts back what had moved, and a file already in the
    /// damaged folder is never replaced.
    #[test]
    fn a_failed_move_puts_every_file_back() {
        let dir = tempfile::tempdir().unwrap();
        let (owner, backup) = shop(dir.path());
        let db_path = dir.path().join("pos.db");
        let live = std::fs::read(&db_path).unwrap();
        std::fs::write(dir.path().join("pos.db-wal"), b"live wal").unwrap();
        let backups = dir.path().join("backups");
        let folder = backups.join(format!(
            "corrupt-{}",
            algiers_datetime(NOW).format("%Y%m%d-%H%M%S")
        ));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("pos.db-wal"), b"earlier").unwrap();
        let error = schedule_guided_restore(&db_path, &backups, &backup, &owner, NOW).unwrap_err();
        assert_eq!(error.code(), "io_error");
        assert_eq!(std::fs::read(&db_path).unwrap(), live);
        assert_eq!(
            std::fs::read(dir.path().join("pos.db-wal")).unwrap(),
            b"live wal"
        );
        assert_eq!(
            std::fs::read(folder.join("pos.db-wal")).unwrap(),
            b"earlier"
        );
        assert!(!folder.join("pos.db").exists());
        assert!(!dir.path().join(RESTORE_MARKER).exists());
        assert!(
            !dir.path().join(MOVING_MARKER).exists(),
            "the database is back: nothing is left to refuse a start"
        );
    }

    #[test]
    fn a_marker_that_cannot_be_written_puts_the_damaged_files_back() {
        let dir = tempfile::tempdir().unwrap();
        let (owner, backup) = shop(dir.path());
        let db_path = dir.path().join("pos.db");
        let live = std::fs::read(&db_path).unwrap();
        std::fs::create_dir(dir.path().join(RESTORE_MARKER)).unwrap(); // a write there fails
        let error =
            schedule_guided_restore(&db_path, &dir.path().join("backups"), &backup, &owner, NOW)
                .unwrap_err();
        assert_eq!(error.code(), "io_error");
        assert_eq!(std::fs::read(&db_path).unwrap(), live, "back in place");
        assert!(!dir.path().join(MOVING_MARKER).exists());
        assert!(corrupt_folders(&dir.path().join("backups")).is_empty());
    }

    /// A power cut while the damaged journal is read (a long scan on an HDD) finds the restore
    /// already scheduled: the marker is on disk, with the damaged folder, before the read starts,
    /// and the next start swaps the backup in.
    #[test]
    fn a_power_cut_while_the_damaged_journal_is_read_still_restores_at_the_next_start() {
        let dir = tempfile::tempdir().unwrap();
        let (owner, backup, till, _) = damaged_till(dir.path());
        let db_path = till.join("pos.db");
        let backups = till.join("backups");
        let cut = std::panic::catch_unwind(|| {
            schedule_with(&db_path, &backups, &backup, &owner, NOW, |_| {
                panic!("power cut")
            })
        });
        assert!(cut.is_err());
        assert!(!db_path.exists());
        let pending = marker(&till);
        let folder = pending
            .damaged_folder
            .clone()
            .expect("the marker names the damaged folder");
        assert!(folder.join("pos.db").exists() && folder.join("pos.db-wal").exists());
        assert!(pending.source.starts_with(&backups), "{:?}", pending.source);
        assert_eq!(pending.replaced_last_seq, None);
        // The next start.
        assert!(apply_pending_restore(&db_path).unwrap().is_some());
        refuse_empty_start(&db_path).unwrap();
        let db = Db::open(&db_path, DbOptions::default()).unwrap();
        let probe = db.read(|c| crate::db::meta_get(c, "probe")).unwrap();
        assert_eq!(probe.as_deref(), Some("in the backup"));
    }

    /// The anchor is added by replacing the marker in one step; when that fails the first
    /// marker stays, and it already makes the next start restore.
    #[test]
    fn a_marker_that_cannot_take_the_anchor_keeps_the_first_one() {
        let dir = tempfile::tempdir().unwrap();
        let (owner, backup, till, _) = damaged_till(dir.path());
        std::fs::create_dir(till.join(format!("{RESTORE_MARKER}.tmp"))).unwrap(); // the replace fails
        let db_path = till.join("pos.db");
        let done =
            schedule_guided_restore(&db_path, &till.join("backups"), &backup, &owner, NOW).unwrap();
        let pending = marker(&till);
        assert_eq!(
            pending.damaged_folder,
            Some(PathBuf::from(&done.damaged_folder))
        );
        assert_eq!(pending.replaced_last_seq, None, "the first marker stayed");
        assert!(apply_pending_restore(&db_path).unwrap().is_some());
    }

    /// A double click, or another backup chosen before the restart: refused, and the first
    /// marker (its anchor, its damaged folder) stays as it was.
    #[test]
    fn a_second_restore_before_the_restart_is_refused_and_the_first_marker_stays() {
        let dir = tempfile::tempdir().unwrap();
        let (owner, backup, till, last_seq) = damaged_till(dir.path());
        let db_path = till.join("pos.db");
        let backups = till.join("backups");
        let first = schedule_guided_restore(&db_path, &backups, &backup, &owner, NOW).unwrap();
        let text = std::fs::read_to_string(till.join(RESTORE_MARKER)).unwrap();
        let error =
            schedule_guided_restore(&db_path, &backups, &backup, &owner, NOW + 5_000).unwrap_err();
        assert_eq!(error.code(), "restore_already_scheduled");
        assert_eq!(
            std::fs::read_to_string(till.join(RESTORE_MARKER)).unwrap(),
            text
        );
        assert_eq!(marker(&till).replaced_last_seq, Some(last_seq));
        assert_eq!(
            corrupt_folders(&backups),
            [PathBuf::from(&first.damaged_folder)]
        );
        assert_eq!(
            crate::db::list_backups_in(&backups).unwrap().len(),
            1,
            "the backup was brought in once"
        );
    }

    #[test]
    fn two_restores_at_once_move_the_damaged_files_into_one_folder() {
        let dir = tempfile::tempdir().unwrap();
        let (owner, backup, till, _) = damaged_till(dir.path());
        let (db_path, backups) = (till.join("pos.db"), till.join("backups"));
        let start = std::sync::Barrier::new(2);
        let (db_path, backups, backup, owner, start) =
            (&db_path, &backups, &backup, &owner, &start);
        let outcomes: Vec<Result<GuidedRestore, AppError>> = std::thread::scope(|s| {
            let calls = [NOW, NOW + 1_500].map(|now| {
                s.spawn(move || {
                    start.wait();
                    schedule_guided_restore(db_path, backups, backup, owner, now)
                })
            });
            calls.map(|call| call.join().unwrap()).into_iter().collect()
        });
        let refused: Vec<String> = outcomes
            .iter()
            .filter_map(|o| o.as_ref().err().map(AppError::code))
            .collect();
        assert_eq!(refused, ["restore_already_scheduled"]);
        let folders = corrupt_folders(backups);
        assert_eq!(folders.len(), 1);
        assert!(folders[0].join("pos.db").exists() && folders[0].join("pos.db-wal").exists());
    }

    /// No damaged file left to move (the data folder has none): no empty damaged folder is made
    /// and the marker names none.
    #[test]
    fn a_restore_with_nothing_to_move_makes_no_damaged_folder() {
        let dir = tempfile::tempdir().unwrap();
        let (owner, backup) = shop(dir.path());
        let till = dir.path().join("till");
        std::fs::create_dir_all(&till).unwrap();
        let backups = till.join("backups");
        let done =
            schedule_guided_restore(&till.join("pos.db"), &backups, &backup, &owner, NOW).unwrap();
        assert!(done.moved.is_empty());
        assert_eq!(done.damaged_folder, "");
        assert!(corrupt_folders(&backups).is_empty());
        assert_eq!(marker(&till).damaged_folder, None);
        assert!(!till.join(MOVING_MARKER).exists());
    }

    /// A power cut in the middle of the move: `pos.db` is in the damaged folder, its WAL still
    /// in the data folder, no marker yet. The start is refused instead of making an empty shop,
    /// and the next guided restore finishes the move into the same folder, anchor included.
    #[test]
    fn a_start_after_a_move_cut_short_is_refused_and_the_next_restore_finishes_it() {
        let dir = tempfile::tempdir().unwrap();
        let (owner, backup, till, last_seq) = damaged_till(dir.path());
        let db_path = till.join("pos.db");
        let backups = till.join("backups");
        let folder = backups.join("corrupt-20260928-091500");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            till.join(MOVING_MARKER),
            folder.to_string_lossy().as_bytes(),
        )
        .unwrap();
        std::fs::rename(&db_path, folder.join("pos.db")).unwrap();
        // The start after the cut.
        assert_eq!(apply_pending_restore(&db_path).unwrap(), None);
        assert_eq!(
            refuse_empty_start(&db_path).unwrap_err().code(),
            "database_missing"
        );
        let done = schedule_guided_restore(&db_path, &backups, &backup, &owner, NOW).unwrap();
        assert_eq!(PathBuf::from(&done.damaged_folder), folder);
        assert_eq!(done.moved, ["pos.db-wal"]);
        assert_eq!(corrupt_folders(&backups), std::slice::from_ref(&folder));
        assert_eq!(marker(&till).replaced_last_seq, Some(last_seq));
        assert!(apply_pending_restore(&db_path).unwrap().is_some());
        refuse_empty_start(&db_path).unwrap();
        assert!(
            !till.join(MOVING_MARKER).exists(),
            "cleared once the database is back"
        );
    }

    /// The restore failed at the next start (the copy it named was lost): the start is refused,
    /// and the next guided restore keeps the damaged folder and the replaced journal's anchor.
    #[test]
    fn a_guided_restore_retried_after_a_failed_start_keeps_the_damaged_folder_and_anchor() {
        let dir = tempfile::tempdir().unwrap();
        let (owner, backup, till, last_seq) = damaged_till(dir.path());
        let db_path = till.join("pos.db");
        let backups = till.join("backups");
        let first = schedule_guided_restore(&db_path, &backups, &backup, &owner, NOW).unwrap();
        std::fs::remove_file(&first.restored_from).unwrap();
        assert!(apply_pending_restore(&db_path).is_err());
        assert_eq!(
            refuse_empty_start(&db_path).unwrap_err().code(),
            "database_missing"
        );
        let second =
            schedule_guided_restore(&db_path, &backups, &backup, &owner, NOW + 60_000).unwrap();
        assert_eq!(second.damaged_folder, first.damaged_folder);
        assert!(second.moved.is_empty());
        assert_eq!(corrupt_folders(&backups).len(), 1);
        assert_eq!(marker(&till).replaced_last_seq, Some(last_seq));
        assert!(apply_pending_restore(&db_path).unwrap().is_some());
    }

    #[test]
    fn the_moving_marker_refuses_a_start_without_a_database_until_the_database_is_back() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("pos.db");
        let moving = dir.path().join(MOVING_MARKER);
        std::fs::write(&moving, "C:\\data\\backups\\corrupt-20260928-091500").unwrap();
        assert_eq!(
            refuse_empty_start(&db_path).unwrap_err().code(),
            "database_missing"
        );
        std::fs::write(&db_path, b"").unwrap();
        refuse_empty_start(&db_path).unwrap();
        assert!(!moving.exists());
    }

    #[test]
    fn a_start_without_a_database_after_a_failed_restore_stops_on_the_fatal_screen() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("pos.db");
        refuse_empty_start(&db_path).unwrap(); // a first start: nothing to protect
        std::fs::write(
            dir.path().join(RESTORE_FAILED_MARKER),
            "E:\\x.db\nbackup_invalid\nid\n",
        )
        .unwrap();
        assert_eq!(
            refuse_empty_start(&db_path).unwrap_err().code(),
            "database_missing"
        );
        std::fs::write(&db_path, b"").unwrap();
        refuse_empty_start(&db_path).unwrap();
    }
}

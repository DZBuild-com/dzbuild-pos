//! Online backups, restore scheduling and rotation (spec §13).

use super::{
    Db, META_CLEAN_SHUTDOWN, Role, configure, integrity_ok, meta_get, meta_set,
    migrations::LATEST_VERSION, refuse_when_closed, schema_version,
};
use crate::audit::{self, Actor};
use crate::error::AppError;
use crate::ids::new_id;
use crate::money::Money;
use crate::time::{ALGIERS_OFFSET_MS, algiers_datetime, business_date, now_ms};
use chrono::{Datelike, NaiveDate, NaiveDateTime};
use rusqlite::backup::{Backup, StepResult};
use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::Ordering;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const RESTORE_MARKER: &str = "restore.pending";
pub const RESTORE_FAILED_MARKER: &str = "restore.failed";
pub const KEEP_DAILY: usize = 14;
pub const KEEP_WEEKLY: usize = 8;
/// `app_meta` key: when the last full `integrity_check` of the live database ran (ms).
pub const META_LAST_INTEGRITY_CHECK: &str = "last_integrity_check";
/// `app_meta` key: when the last `PRAGMA quick_check` of the live database ran (ms).
pub const META_LAST_QUICK_CHECK: &str = "last_quick_check";
/// DAT-005: a full integrity check of the live database at least once a week.
pub const INTEGRITY_EVERY_DAYS: i64 = 7;
/// The previous process may still hold `pos.db`/`-wal`/`-shm` for a moment after a restart:
/// the swap is retried after 100 ms, 200 ms, 400 ms … until about 10 s have passed.
pub const RESTORE_RETRY_FIRST: Duration = Duration::from_millis(100);
pub const RESTORE_RETRY_BUDGET: Duration = Duration::from_secs(10);

/// The backup copied next to the database before the swap: `pos.db.restoring`.
const STAGED_SUFFIX: &str = ".restoring";
/// A WAL sidecar moved out of the way during the swap: `pos.db-wal.replaced`.
const SET_ASIDE_SUFFIX: &str = ".replaced";
/// `app_meta` key of the restored copy: the id of the restore that put it in place.
const META_RESTORE_ID: &str = "restore_id";
/// `app_meta` key of a restored copy whose `backup.restored` row is not journaled yet: that
/// row's payload as JSON (`journal_pending_restore`).
pub const META_RESTORE_JOURNAL: &str = "restore_journal_pending";
/// An antivirus may hold `restore.pending` for a moment: its removal is retried briefly.
const MARKER_RETRY_FIRST: Duration = Duration::from_millis(50);
const MARKER_RETRY_BUDGET: Duration = Duration::from_millis(500);

/// When this process first touched backups or restores. Temporary files dated before it were
/// left by a crashed run; newer ones may belong to a copy this run is still writing.
static RUN_STARTED: OnceLock<SystemTime> = OnceLock::new();

fn run_started() -> SystemTime {
    *RUN_STARTED.get_or_init(SystemTime::now)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum BackupKind {
    Auto,
    Manual,
    PreMigration,
    PreRestore,
    /// The database a PC had before it became a till (M5): kept, listed, never logged in
    /// `backups_log` (the copy is taken before the database opens).
    PrePairing,
}

impl BackupKind {
    pub const ALL: [BackupKind; 5] = [
        BackupKind::Auto,
        BackupKind::Manual,
        BackupKind::PreMigration,
        BackupKind::PreRestore,
        BackupKind::PrePairing,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            BackupKind::Auto => "auto",
            BackupKind::Manual => "manual",
            BackupKind::PreMigration => "pre_migration",
            BackupKind::PreRestore => "pre_restore",
            BackupKind::PrePairing => "pre_pairing",
        }
    }

    pub fn parse(s: &str) -> Option<BackupKind> {
        BackupKind::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub file_name: String,
    pub path: String,
    pub at: i64,
    pub size: i64,
    pub kind: BackupKind,
}

/// `pos-20260928-143012-345-manual.db`, in Algiers local time.
fn file_name_for(at: i64, kind: BackupKind) -> String {
    let local = algiers_datetime(at);
    format!(
        "pos-{}-{:03}-{}.db",
        local.format("%Y%m%d-%H%M%S"),
        at.rem_euclid(1_000),
        kind.as_str()
    )
}

/// The name a backup of `kind` taken at `at` gets in the backups folder (an imported `.dzpos`
/// file is placed under a `manual` name so the restore screen lists it).
pub fn backup_file_name(at: i64, kind: BackupKind) -> String {
    file_name_for(at, kind)
}

pub(crate) fn kind_of(file_name: &str) -> Option<BackupKind> {
    let stem = file_name.strip_prefix("pos-")?.strip_suffix(".db")?;
    let mut parts = stem.splitn(4, '-');
    parts.next()?;
    parts.next()?;
    parts.next()?;
    BackupKind::parse(parts.next()?)
}

/// The time a backup's file name records, as unix milliseconds. Unlike the file's date, it
/// survives a copy to a USB stick and back: a backup is dated by its name wherever it was
/// copied (the fatal screen lists the stick's copies by it).
pub fn time_of(file_name: &str) -> Option<i64> {
    let stem = file_name.strip_prefix("pos-")?;
    let mut parts = stem.splitn(4, '-');
    let (date, time, millis) = (parts.next()?, parts.next()?, parts.next()?);
    if date.len() != 8 || time.len() != 6 || millis.len() != 3 {
        return None;
    }
    let millis: i64 = millis.parse().ok()?;
    let local = NaiveDateTime::parse_from_str(&format!("{date}{time}"), "%Y%m%d%H%M%S").ok()?;
    Some(local.and_utc().timestamp_millis() - ALGIERS_OFFSET_MS + millis)
}

/// One copy attempt: the file renamed into place (`ok`), or the rejected temporary path.
struct BackupRecord {
    file_name: String,
    path: PathBuf,
    at: i64,
    size: i64,
    kind: BackupKind,
    ok: bool,
}

impl BackupRecord {
    fn log(&self, conn: &Connection) -> Result<(), AppError> {
        conn.execute(
            "INSERT INTO backups_log (id, at, path, size, kind, ok) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                new_id(),
                self.at,
                self.path.to_string_lossy(),
                self.size,
                self.kind.as_str(),
                self.ok
            ],
        )?;
        Ok(())
    }

    fn into_result(self) -> Result<BackupInfo, AppError> {
        if !self.ok {
            return Err(AppError::conflict("backup_invalid"));
        }
        Ok(BackupInfo {
            file_name: self.file_name,
            path: self.path.to_string_lossy().into_owned(),
            at: self.at,
            size: self.size,
            kind: self.kind,
        })
    }
}

/// Copies the database behind `source` into `dir` under a temporary name, checks the copy,
/// then renames it into place. Writes nothing to the live database: logging is the caller's
/// job. A failed attempt leaves no temporary file behind.
fn copy_database(
    source: &Connection,
    dir: &Path,
    kind: BackupKind,
    at: i64,
) -> Result<BackupRecord, AppError> {
    run_started(); // before the temporary file exists, so no sweep takes it for a leftover
    std::fs::create_dir_all(dir)?;
    let file_name = file_name_for(at, kind);
    let final_path = dir.join(&file_name);
    let tmp_path = dir.join(format!("{file_name}.tmp"));
    let ok = match snapshot_into(source, &tmp_path) {
        Ok(ok) => ok,
        Err(error) => {
            remove_quietly(&tmp_path);
            return Err(error);
        }
    };
    if !ok {
        remove_quietly(&tmp_path);
        return Ok(BackupRecord {
            file_name,
            path: tmp_path,
            at,
            size: 0,
            kind,
            ok: false,
        });
    }
    if let Err(error) = std::fs::rename(&tmp_path, &final_path) {
        remove_quietly(&tmp_path);
        return Err(error.into());
    }
    let size = i64::try_from(std::fs::metadata(&final_path)?.len()).unwrap_or(i64::MAX);
    Ok(BackupRecord {
        file_name,
        path: final_path,
        at,
        size,
        kind,
        ok: true,
    })
}

/// Online backup of `source` into `copy_path` in a single step, then the copy is converted to
/// a single file (`journal_mode=DELETE`) and checked on its own connection. One step reads one
/// snapshot: in WAL mode it does not block the writer, and unlike page-by-page steps it is
/// never restarted by a sale committed meanwhile.
fn snapshot_into(source: &Connection, copy_path: &Path) -> Result<bool, AppError> {
    {
        let mut copy = Connection::open(copy_path)?;
        let backup = Backup::new(source, &mut copy)?;
        if backup.step(-1)? != StepResult::Done {
            return Err(AppError::conflict("database_busy"));
        }
    }
    let copy = Connection::open(copy_path)?;
    copy.pragma_update_and_check(None, "journal_mode", "DELETE", |_| Ok(()))?;
    let ok = integrity_ok(&copy)?;
    // Closed before the caller renames the file: Windows refuses to rename an open file.
    copy.close().map_err(|(_, error)| AppError::from(error))?;
    Ok(ok)
}

/// Pre-migration copy, taken in `Db::open` on the opening connection before the writer thread
/// exists. Runtime backups use `Db::backup` instead.
pub(crate) fn backup_connection(
    conn: &Connection,
    dir: &Path,
    kind: BackupKind,
) -> Result<BackupInfo, AppError> {
    let record = copy_database(conn, dir, kind, now_ms())?;
    record.log(conn)?;
    record.into_result()
}

/// A dedicated read-only connection owned by the calling thread, never the writer: the source
/// of a runtime backup (WAL lets it read while sales keep committing), or the restore-id probe.
fn open_read_only(db_path: &Path) -> Result<Connection, AppError> {
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    configure(&conn, Role::Reader)?;
    Ok(conn)
}

impl Db {
    /// Online backup from a dedicated read-only connection: the copy and the integrity check of
    /// the copy never run on the writer thread, so writes are not blocked for seconds on an HDD.
    /// Only the one-row `backups_log` insert goes through the writer. Refused, like every write,
    /// once `mark_clean_shutdown` has run.
    pub fn backup(&self, kind: BackupKind) -> Result<BackupInfo, AppError> {
        self.backup_at(kind, now_ms())
    }

    /// [`Self::backup`] named and dated `at` (an hourly copy records the instant it was due).
    pub fn backup_at(&self, kind: BackupKind, at: i64) -> Result<BackupInfo, AppError> {
        refuse_when_closed(&self.inner.closed)?;
        let dir = self
            .backup_dir()
            .map(Path::to_path_buf)
            .ok_or_else(|| AppError::validation("backupDir", "backup_dir_missing"))?;
        let db_path = self
            .path()
            .map(Path::to_path_buf)
            .ok_or_else(|| AppError::validation("database", "memory_database"))?;
        let record = {
            let source = open_read_only(&db_path)?;
            copy_database(&source, &dir, kind, at)?
        };
        let closed = self.closed_flag();
        self.run_on_writer(move |conn| {
            refuse_when_closed(&closed)?;
            record.log(conn)?;
            record.into_result()
        })
    }

    /// A consistent copy of the whole database into `path` (the main PC's snapshot for a till,
    /// M5): one online-backup step from a dedicated read connection, converted to a single file
    /// and integrity-checked (`backup_invalid`). `memory_database` for an in-memory database.
    pub fn snapshot_to(&self, path: &Path) -> Result<(), AppError> {
        refuse_when_closed(&self.inner.closed)?;
        let db_path = self
            .path()
            .map(Path::to_path_buf)
            .ok_or_else(|| AppError::validation("database", "memory_database"))?;
        let source = open_read_only(&db_path)?;
        match snapshot_into(&source, path) {
            Ok(true) => Ok(()),
            Ok(false) => {
                remove_quietly(path);
                Err(AppError::conflict("backup_invalid"))
            }
            Err(error) => {
                remove_quietly(path);
                Err(error)
            }
        }
    }

    /// Backup files in the backups folder (the folder is the source of truth), newest first.
    pub fn list_backups(&self) -> Result<Vec<BackupInfo>, AppError> {
        match self.backup_dir() {
            Some(dir) => list_backups_in(dir),
            None => Ok(Vec::new()),
        }
    }

    pub fn integrity_check(&self) -> Result<bool, AppError> {
        self.read(integrity_ok)
    }

    /// `PRAGMA integrity_check` of the live database on a reader (sales keep committing), then
    /// the time is recorded and `db.integrity_checked { ok, scheduled }` journaled on the writer.
    pub fn check_integrity(&self, actor: Actor, scheduled: bool) -> Result<bool, AppError> {
        self.check_integrity_at(actor, scheduled, now_ms())
    }

    /// [`Db::check_integrity`] recorded as run at `at`: the scheduler's clock, so the next
    /// [`Db::integrity_check_if_due`] measures the week from the time it was given.
    fn check_integrity_at(&self, actor: Actor, scheduled: bool, at: i64) -> Result<bool, AppError> {
        let ok = self.integrity_check()?;
        self.write(move |tx| {
            meta_set(tx, META_LAST_INTEGRITY_CHECK, &at.to_string())?;
            audit::append(
                tx,
                &actor.event(
                    "db.integrity_checked",
                    "database",
                    None,
                    json!({ "ok": ok, "scheduled": scheduled }),
                ),
            )?;
            Ok(ok)
        })
    }

    /// The weekly check: runs when the last one is older than [`INTEGRITY_EVERY_DAYS`] (or dated
    /// in the future by a clock that ran fast); `None` when it is not due.
    pub fn integrity_check_if_due(&self, actor: Actor, now: i64) -> Result<Option<bool>, AppError> {
        let last: Option<i64> = self
            .read(|c| Ok(meta_get(c, META_LAST_INTEGRITY_CHECK)?.and_then(|v| v.parse().ok())))?;
        let fresh =
            last.is_some_and(|at| at <= now && now - at < INTEGRITY_EVERY_DAYS * 86_400_000);
        if fresh {
            return Ok(None);
        }
        self.check_integrity_at(actor, true, now).map(Some)
    }

    /// Writes the clean-shutdown mark and checkpoints the WAL into `pos.db`, truncating it.
    /// Call on exit and before a restart: without the mark, the next `Db::open` reports an
    /// unclean shutdown and runs the integrity check. From then on every handle refuses writes
    /// and backups (`internal_error`), so nothing lands behind the mark; reads keep working.
    pub fn mark_clean_shutdown(&self) -> Result<(), AppError> {
        let on_disk = self.path().is_some();
        let closed = self.closed_flag();
        self.run_on_writer(move |conn| {
            closed.store(true, Ordering::SeqCst);
            meta_set(conn, META_CLEAN_SHUTDOWN, "1")?;
            if on_disk {
                let busy: i64 =
                    conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))?;
                if busy != 0 {
                    tracing::warn!("a reader kept the WAL from being truncated at shutdown");
                }
            }
            Ok(())
        })
    }

    /// Clean exit: `mark_clean_shutdown`, then this handle is released. When it was the last
    /// one, the connections close and SQLite deletes `-wal` and `-shm`. A handle kept in state
    /// that is never dropped (Tauri managed state) is closed with `mark_clean_shutdown` alone:
    /// the files are consistent either way.
    pub fn close(self) -> Result<(), AppError> {
        self.mark_clean_shutdown()
    }

    /// First launch of the business day with the default retention (see `daily_backup_if_due_with`).
    pub fn daily_backup_if_due(&self) -> Result<Option<BackupInfo>, AppError> {
        self.daily_backup_if_due_with(Retention::default())
    }

    /// First launch of the business day: one automatic backup; then, at every call (even when
    /// today's copy already exists, e.g. an hourly copy made after midnight), rotation by
    /// `retention`. Backups are dated by their file name, and the copy just made is never
    /// pruned, even when the clock is behind the existing ones. Temporary copies a crashed run
    /// left are swept.
    pub fn daily_backup_if_due_with(
        &self,
        retention: Retention,
    ) -> Result<Option<BackupInfo>, AppError> {
        if let Some(dir) = self.backup_dir() {
            sweep_leftovers(dir, |name| {
                name.strip_suffix(".tmp").and_then(kind_of).is_some()
            });
        }
        let today = business_date(now_ms());
        let done_today = self
            .auto_backups()?
            .iter()
            .any(|(at, _)| business_date(*at) == today);
        let made = if done_today {
            None
        } else {
            Some(self.backup(BackupKind::Auto)?)
        };
        for path in select_to_prune(self.auto_backups()?, retention, today) {
            if made
                .as_ref()
                .is_none_or(|info| path != Path::new(&info.path))
            {
                remove_quietly(&path);
            }
        }
        Ok(made)
    }

    /// DAT-007 (hourly part): an automatic copy when the newest is an hour old or more; of
    /// today's automatic copies only the newest `keep_hourly` stay, and the past days are
    /// rotated at once by the shop's retention (`Retention::from_settings`: the newest copy of
    /// each day kept, then weeks and months), so a PC that runs past midnight or is never
    /// switched off keeps "hourly, keep 24" and not 24 copies a day forever. When the retention
    /// cannot be read, the past days are left alone this time (nothing is deleted on a guess).
    /// `keep_hourly == 0` makes none. The app calls it every ten minutes while a cash session
    /// is open on this PC; `now` names the copy.
    pub fn hourly_backup_if_due(
        &self,
        now: i64,
        keep_hourly: u32,
    ) -> Result<Option<BackupInfo>, AppError> {
        if keep_hourly == 0 {
            return Ok(None);
        }
        if self
            .auto_backups()?
            .iter()
            .any(|(at, _)| now - *at < 3_600_000)
        {
            return Ok(None);
        }
        let info = self.backup_at(BackupKind::Auto, now)?;
        let today = business_date(now);
        let autos = self.auto_backups()?;
        let mut prune = match self.read(Retention::from_settings) {
            Ok(retention) => select_to_prune(autos.clone(), retention, today),
            Err(error) => {
                tracing::warn!(%error, "backup retention unreadable; past days not rotated");
                Vec::new()
            }
        };
        let mut todays: Vec<(i64, PathBuf)> = autos
            .into_iter()
            .filter(|(at, _)| business_date(*at) == today)
            .collect();
        todays.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
        prune.extend(
            todays
                .into_iter()
                .skip(usize::try_from(keep_hourly).unwrap_or(usize::MAX))
                .map(|(_, path)| path),
        );
        let made = Path::new(&info.path);
        for path in prune {
            if path != made {
                remove_quietly(&path);
            }
        }
        Ok(Some(info))
    }

    /// DAT-005: `PRAGMA quick_check` on a reader (the app runs it in the background after
    /// start, never delaying it); when it ran is kept in `app_meta.last_quick_check`.
    pub fn quick_check(&self) -> Result<bool, AppError> {
        let ok = self.read(|c| {
            Ok(c.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))? == "ok")
        })?;
        let at = now_ms().to_string();
        self.write(move |tx| meta_set(tx, META_LAST_QUICK_CHECK, &at))?;
        Ok(ok)
    }

    /// Automatic backups with the time their file name records (the file date as a fallback).
    fn auto_backups(&self) -> Result<Vec<(i64, PathBuf)>, AppError> {
        Ok(self
            .list_backups()?
            .into_iter()
            .filter(|b| b.kind == BackupKind::Auto)
            .map(|b| (time_of(&b.file_name).unwrap_or(b.at), PathBuf::from(b.path)))
            .collect())
    }

    /// Validates the chosen backup, saves today's data as `pre_restore`, and leaves a marker
    /// that `apply_pending_restore` consumes on the next start: the backup's path, a new
    /// restore id that the restored copy will carry, the requesting user, the `pre_restore`
    /// file, and the anchor (last `seq` and `hash`) of the journal that copy preserves. The
    /// restored database journals all of it as `backup.restored` (`journal_pending_restore`),
    /// so the journal it replaces is never cut off without a trace.
    pub fn schedule_restore(
        &self,
        file_name: &str,
        requested_by: Option<&str>,
    ) -> Result<BackupInfo, AppError> {
        let db_path = self
            .path()
            .map(Path::to_path_buf)
            .ok_or_else(|| AppError::validation("database", "memory_database"))?;
        let chosen = self
            .list_backups()?
            .into_iter()
            .find(|b| b.file_name == file_name)
            .ok_or_else(|| AppError::not_found("backup"))?;
        validate_backup_file(Path::new(&chosen.path))?;
        let saved = self.backup(BackupKind::PreRestore)?;
        let (last_seq, last_hash) = journal_tail(Path::new(&saved.path))?;
        let marks = high_water_marks(Path::new(&saved.path))?;
        let pending = PendingRestore {
            source: PathBuf::from(&chosen.path),
            id: Some(new_id()),
            requested_by: requested_by.map(str::to_owned),
            pre_restore_file: Some(saved.file_name),
            replaced_last_seq: Some(last_seq),
            replaced_last_hash: Some(last_hash),
            replaced_marks: Some(marks),
            damaged_folder: None,
        };
        std::fs::write(db_path.with_file_name(RESTORE_MARKER), pending.to_text())?;
        Ok(chosen)
    }
}

/// `(seq, hash)` of the last audit row of the database at `path`, `(0, GENESIS_HASH)` when its
/// journal is empty: the anchor of the journal a restore replaces.
pub(super) fn journal_tail(path: &Path) -> Result<(i64, String), AppError> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let tail = conn
        .query_row(
            "SELECT seq, hash FROM audit_log ORDER BY seq DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(tail.unwrap_or_else(|| (0, crate::audit::GENESIS_HASH.to_owned())))
}

/// A counter of the database a restore replaces: the last number it had handed out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CounterMark {
    pub doc_type: String,
    pub series: String,
    pub fiscal_year: i64,
    pub last_number: i64,
}

/// The last link of a document chain of the replaced database.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainMark {
    pub doc_type: String,
    pub series: String,
    pub last_seq: i64,
    pub last_hash: String,
}

/// The last Z of a terminal of the replaced database.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClosureMark {
    pub terminal_id: String,
    pub z_seq: i64,
    pub hash: String,
    /// The perpetual grand total that Z printed (AUD-005): the restored copy's next Z goes on
    /// from it (`cash::closing`). 0 in a record written before it was kept.
    #[serde(default)]
    pub grand_total: Money,
}

/// What the database a restore replaces had already issued (numbers, chain links, Z): the
/// restored copy may be behind it. Journaled in `backup.restored`; the restored copy's counters
/// are moved past these numbers (`journal_pending_restore`), so none is printed twice.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HighWaterMarks {
    pub counters: Vec<CounterMark>,
    pub chains: Vec<ChainMark>,
    pub closures: Vec<ClosureMark>,
}

/// Numbers a restored database will hand out a second time: its counter is still behind the
/// replaced database's after `journal_pending_restore` (only a restore without an id, which
/// cannot be journaled as a skip). `first` and `last` are the numbers as printed (a counter
/// without a fiscal year is printed with the current year).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReissuedRange {
    pub doc_type: String,
    pub series: String,
    pub from: i64,
    pub to: i64,
    pub first: String,
    pub last: String,
}

/// A range a restored counter jumped over, as `backup.restored` lists it (`skipped`): the skip
/// (`numbering::journaled_skips` reads it back) and its first and last number as printed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedRange {
    #[serde(flatten)]
    pub skip: crate::numbering::Skip,
    pub first: String,
    pub last: String,
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool, AppError> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [name],
        |r| r.get(0),
    )?)
}

/// The [`HighWaterMarks`] of the database at `path`, read only (a table it does not have yet
/// counts as empty).
pub(super) fn high_water_marks(path: &Path) -> Result<HighWaterMarks, AppError> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut marks = HighWaterMarks::default();
    if table_exists(&conn, "counters")? {
        let mut stmt = conn.prepare(
            "SELECT doc_type, series, fiscal_year, last_number FROM counters ORDER BY 1, 2, 3",
        )?;
        marks.counters = stmt
            .query_map([], |r| {
                Ok(CounterMark {
                    doc_type: r.get(0)?,
                    series: r.get(1)?,
                    fiscal_year: r.get(2)?,
                    last_number: r.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
    }
    if table_exists(&conn, "documents")? {
        let mut stmt = conn.prepare(
            "SELECT d.doc_type, d.series, d.chain_seq, d.hash FROM documents d
             WHERE d.chain_seq = (SELECT MAX(e.chain_seq) FROM documents e
                                  WHERE e.doc_type = d.doc_type AND e.series = d.series)
             ORDER BY 1, 2",
        )?;
        marks.chains = stmt
            .query_map([], |r| {
                Ok(ChainMark {
                    doc_type: r.get(0)?,
                    series: r.get(1)?,
                    last_seq: r.get(2)?,
                    last_hash: r.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
    }
    if table_exists(&conn, "cash_closures")? {
        let mut stmt = conn.prepare(
            "SELECT c.terminal_id, c.z_seq, c.hash, c.grand_total FROM cash_closures c
             WHERE c.z_seq = (SELECT MAX(d.z_seq) FROM cash_closures d
                              WHERE d.terminal_id = c.terminal_id)
             ORDER BY 1",
        )?;
        marks.closures = stmt
            .query_map([], |r| {
                Ok(ClosureMark {
                    terminal_id: r.get(0)?,
                    z_seq: r.get(1)?,
                    hash: r.get(2)?,
                    grand_total: r.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
    }
    Ok(marks)
}

/// The number `n` of a counter as printed: documents (their series included, `BR…-F-` for
/// supplier returns), Z reports and custom series (versement receipts `RV…`).
fn printed(mark: &CounterMark, n: i64, year_now: i32) -> String {
    if mark.doc_type == crate::numbering::Z_COUNTER {
        return crate::numbering::format_z_number(&mark.series, n);
    }
    let year = i32::try_from(mark.fiscal_year)
        .ok()
        .filter(|y| *y != 0)
        .unwrap_or(year_now);
    match crate::numbering::DocType::parse(&mark.doc_type) {
        Some(doc_type) => crate::numbering::format_number(doc_type, &mark.series, year, n),
        None => match crate::numbering::custom_prefix(&mark.doc_type) {
            Some(prefix) => crate::numbering::format_custom(prefix, year, n),
            None => n.to_string(),
        },
    }
}

/// The counters of the live (restored) database that are still behind `marks`: the numbers in
/// between will be issued a second time. The never-resetting codes (customers, suppliers,
/// products) are left out: no printed document carries them, and the records that held them
/// are gone with the replaced database.
fn reissued_ranges(
    conn: &Connection,
    marks: &HighWaterMarks,
    year_now: i32,
) -> Result<Vec<ReissuedRange>, AppError> {
    let has_counters = table_exists(conn, "counters")?;
    let mut ranges = Vec::new();
    for mark in &marks.counters {
        if crate::numbering::CODE_COUNTERS.contains(&mark.doc_type.as_str()) {
            continue;
        }
        let live: i64 = if has_counters {
            conn.query_row(
                "SELECT COALESCE((SELECT last_number FROM counters
                                  WHERE doc_type = ?1 AND series = ?2 AND fiscal_year = ?3), 0)",
                params![mark.doc_type, mark.series, mark.fiscal_year],
                |r| r.get(0),
            )?
        } else {
            0
        };
        if mark.last_number > live {
            let from = live + 1;
            ranges.push(ReissuedRange {
                doc_type: mark.doc_type.clone(),
                series: mark.series.clone(),
                from,
                to: mark.last_number,
                first: printed(mark, from, year_now),
                last: printed(mark, mark.last_number, year_now),
            });
        }
    }
    Ok(ranges)
}

/// Moves every counter of the live (restored) database that is behind `marks` past the
/// replaced database's last number, one journaled skip each (`numbering::fast_forward`).
fn skip_replaced_numbers(
    tx: &Transaction<'_>,
    marks: &HighWaterMarks,
    restore_id: &str,
    now: i64,
    year_now: i32,
) -> Result<Vec<SkippedRange>, AppError> {
    let mut skipped = Vec::new();
    for mark in &marks.counters {
        let skip = crate::numbering::fast_forward(
            tx,
            &mark.doc_type,
            &mark.series,
            mark.fiscal_year,
            mark.last_number,
            restore_id,
            now,
        )?;
        if let Some(skip) = skip {
            skipped.push(SkippedRange {
                first: printed(mark, skip.from, year_now),
                last: printed(mark, skip.to, year_now),
                skip,
            });
        }
    }
    Ok(skipped)
}

/// Appends the `backup.restored` row that a restore left pending in the restored copy
/// (`META_RESTORE_JOURNAL`), and removes the key in the same transaction: the row is written
/// once, whether the start that swapped the files crashed before journaling it or not. The
/// row names the requester as its user and the restore id as its entity, anchors the journal
/// that the restore replaced and records what that database had issued (`replacedMarks`).
/// Every counter of the restored copy that is behind those marks (documents, Z, codes) first
/// jumps past them in the same transaction, so no number is printed twice; the row lists the
/// jumps (`skipped`, only when there are some), the one gap numbering verification accepts, and
/// the numbers still to be handed out again (`reissued`: none, unless the restore has no id).
/// Returns those `reissued` ranges when a row was written, `None` when no restore was pending.
pub fn journal_pending_restore(
    tx: &Transaction<'_>,
    terminal_id: Option<&str>,
) -> Result<Option<Vec<ReissuedRange>>, AppError> {
    let Some(text) = meta_get(tx, META_RESTORE_JOURNAL)? else {
        return Ok(None);
    };
    let mut payload: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|error| {
        tracing::warn!(%error, "unreadable pending restore record; journaling it as text");
        serde_json::json!({ "record": text })
    });
    let marks: Option<HighWaterMarks> = payload
        .get("replacedMarks")
        .and_then(|m| serde_json::from_value(m.clone()).ok());
    let restore_id = payload
        .get("restoreId")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let now = now_ms();
    let year_now = business_date(now).year();
    let skipped = match (&marks, restore_id.as_deref()) {
        (Some(marks), Some(restore_id)) => {
            skip_replaced_numbers(tx, marks, restore_id, now, year_now)?
        }
        _ => Vec::new(),
    };
    let reissued = match &marks {
        Some(marks) => reissued_ranges(tx, marks, year_now)?,
        None => Vec::new(),
    };
    if let Some(map) = payload.as_object_mut() {
        map.insert("reissued".into(), serde_json::to_value(&reissued)?);
        if !skipped.is_empty() {
            map.insert("skipped".into(), serde_json::to_value(&skipped)?);
        }
    }
    let requested_by = payload
        .get("requestedBy")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let actor = crate::audit::Actor {
        user_id: requested_by,
        terminal_id: terminal_id.map(str::to_owned),
    };
    let event = actor.event("backup.restored", "backup", restore_id.as_deref(), payload);
    crate::audit::append(tx, &event)?;
    tx.execute(
        "DELETE FROM app_meta WHERE key = ?1",
        [META_RESTORE_JOURNAL],
    )?;
    Ok(Some(reissued))
}

/// A backup is usable when it opens, passes `integrity_check`, holds a migrated POS schema
/// (SQLite opens an empty file as an empty database) and is not newer than this build.
pub fn validate_backup_file(path: &Path) -> Result<(), AppError> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| AppError::conflict("backup_invalid"))?;
    let ok = integrity_ok(&conn).map_err(|_| AppError::conflict("backup_invalid"))?;
    if !ok {
        return Err(AppError::conflict("backup_invalid"));
    }
    let version = schema_version(&conn)?;
    if version > LATEST_VERSION {
        return Err(AppError::conflict("database_newer_than_app"));
    }
    if version < 1 {
        return Err(AppError::conflict("backup_invalid"));
    }
    Ok(())
}

/// Called before `Db::open`: replaces the database with the scheduled backup.
///
/// First it repairs what a crash in the middle of a swap left (`recover_set_aside_sidecars`).
/// If a set-aside WAL cannot be resolved, it stops there with `restore_needs_attention`: the
/// database is missing its WAL and must not be opened normally, and nothing is staged over the
/// evidence. Then it sweeps an earlier run's staging files. A marker whose restore id the live
/// database already carries (the swap succeeded, the marker outlived it) or `restore.failed`
/// already records (the failure was reported, the marker outlived it) is removed without a
/// swap. Otherwise the backup is validated, staged next to the database with the restore id,
/// and swapped in; the swap is retried with backoff (100 ms × 2^n, about 10 s in all) while the
/// previous process still holds the files. On failure the current database is kept, the marker
/// becomes `restore.failed` (the backup's path, the reason code, the restore id) and the
/// restore's error is returned (the shell turns it into a `StartupNotice::RestoreFailed`).
pub fn apply_pending_restore(db_path: &Path) -> Result<Option<PathBuf>, AppError> {
    run_started();
    let staged = sidecar_path(db_path, STAGED_SUFFIX);
    recover_set_aside_sidecars(db_path, &staged);
    if set_aside_remains(db_path) {
        tracing::error!("a set-aside WAL file could not be resolved; the database needs attention");
        return Err(AppError::conflict("restore_needs_attention"));
    }
    if let Some(staged_name) = staged.file_name().map(|n| n.to_string_lossy().into_owned()) {
        sweep_leftovers(folder_of(db_path), |name| {
            name.starts_with(staged_name.as_str())
        });
    }
    let marker = db_path.with_file_name(RESTORE_MARKER);
    if !marker.exists() {
        return Ok(None);
    }
    let pending = match std::fs::read_to_string(&marker) {
        Ok(text) => PendingRestore::parse(&text),
        Err(error) => {
            let unreadable = PendingRestore::default();
            return Err(record_failure(&marker, db_path, &unreadable, error.into()));
        }
    };
    if pending.id.is_some() && pending.id == failed_restore_id(db_path) {
        tracing::warn!("this restore already failed and was reported; it is not retried");
        drop_marker(&marker);
        return Ok(None);
    }
    if pending.id.is_some() && pending.id == applied_restore_id(db_path) {
        drop_marker(&marker);
        return Ok(None);
    }
    match restore(&pending, &staged, db_path) {
        Ok(()) => {
            drop_marker(&marker);
            Ok(Some(pending.source))
        }
        Err(error) => {
            // A second chance for sidecars a failed put-back left aside. While any remain, the
            // staged copy stays too, so the next start still knows the swap never happened,
            // and the shell is told not to open the database without its WAL.
            recover_set_aside_sidecars(db_path, &staged);
            let error = if set_aside_remains(db_path) {
                tracing::error!(%error, "the restore failed and the database's WAL could not be put back");
                AppError::conflict("restore_needs_attention")
            } else {
                remove_quietly(&staged);
                error
            };
            Err(record_failure(&marker, db_path, &pending, error))
        }
    }
}

/// What `restore.pending` holds, one line each (`schedule_restore`): the backup's path, the
/// restore id, the requesting user id, the `pre_restore` file name, the last `seq` and `hash`
/// of the journal being replaced, its [`HighWaterMarks`] as one line of JSON, then the folder a
/// guided restore moved the damaged database into. Markers from before the last six lines parse
/// too.
#[derive(Debug, Default)]
pub(super) struct PendingRestore {
    pub(super) source: PathBuf,
    pub(super) id: Option<String>,
    pub(super) requested_by: Option<String>,
    pub(super) pre_restore_file: Option<String>,
    pub(super) replaced_last_seq: Option<i64>,
    pub(super) replaced_last_hash: Option<String>,
    pub(super) replaced_marks: Option<HighWaterMarks>,
    /// A guided restore (Task 3): where the damaged database files were moved.
    pub(super) damaged_folder: Option<PathBuf>,
}

impl PendingRestore {
    pub(super) fn parse(text: &str) -> PendingRestore {
        let mut lines = text.lines().map(str::trim);
        let source = PathBuf::from(lines.next().unwrap_or_default());
        let mut next = || lines.next().filter(|l| !l.is_empty()).map(str::to_owned);
        PendingRestore {
            source,
            id: next(),
            requested_by: next(),
            pre_restore_file: next(),
            replaced_last_seq: next().and_then(|seq| seq.parse().ok()),
            replaced_last_hash: next(),
            replaced_marks: next().and_then(|json| serde_json::from_str(&json).ok()),
            damaged_folder: next().map(PathBuf::from),
        }
    }

    pub(super) fn to_text(&self) -> String {
        let line = |value: Option<&str>| value.unwrap_or_default().to_owned();
        [
            self.source.to_string_lossy().into_owned(),
            line(self.id.as_deref()),
            line(self.requested_by.as_deref()),
            line(self.pre_restore_file.as_deref()),
            self.replaced_last_seq
                .map(|seq| seq.to_string())
                .unwrap_or_default(),
            line(self.replaced_last_hash.as_deref()),
            self.replaced_marks
                .as_ref()
                .and_then(|marks| serde_json::to_string(marks).ok())
                .unwrap_or_default(),
            self.damaged_folder
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
        ]
        .map(|l| l + "\n")
        .concat()
    }

    /// The payload of the `backup.restored` row the restored database journals.
    pub(super) fn journal_payload(&self) -> serde_json::Value {
        let mut payload = serde_json::json!({
            "restoreId": self.id,
            "requestedBy": self.requested_by,
            "source": self.source.to_string_lossy(),
            "preRestoreFile": self.pre_restore_file,
            "replacedLastSeq": self.replaced_last_seq,
            "replacedLastHash": self.replaced_last_hash,
            "replacedMarks": self.replaced_marks,
        });
        // Only guided restores carry these, so M0 restores journal exactly what they did.
        if let (Some(folder), Some(map)) = (&self.damaged_folder, payload.as_object_mut()) {
            map.insert(
                "damagedFolder".into(),
                serde_json::json!(folder.to_string_lossy()),
            );
            map.insert("guided".into(), serde_json::json!(true));
        }
        payload
    }
}

/// The restore id `restore.failed` records (its third line), if any.
fn failed_restore_id(db_path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(db_path.with_file_name(RESTORE_FAILED_MARKER)).ok()?;
    text.lines()
        .nth(2)
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
}

/// Validates the backup, stages a copy of it, then swaps it in.
fn restore(pending: &PendingRestore, staged: &Path, db_path: &Path) -> Result<(), AppError> {
    validate_backup_file(&pending.source)?;
    stage(pending, staged)?;
    retry_with_backoff(RESTORE_RETRY_FIRST, RESTORE_RETRY_BUDGET, || {
        swap_in(staged, db_path).inspect_err(|error| {
            tracing::warn!(%error, "restore swap refused; the database files may still be open");
        })
    })?;
    Ok(())
}

/// The restore id the live database carries; `None` when it has none or cannot be read.
fn applied_restore_id(db_path: &Path) -> Option<String> {
    if !db_path.exists() {
        return None;
    }
    match open_read_only(db_path).and_then(|conn| meta_get(&conn, META_RESTORE_ID)) {
        Ok(id) => id,
        Err(error) => {
            tracing::warn!(%error, "could not read the restore id of the live database");
            None
        }
    }
}

/// Removes `restore.pending`, retrying briefly while another program (an antivirus) holds it.
/// A marker that is already gone counts as removed.
fn remove_marker(marker: &Path) -> std::io::Result<()> {
    let removed = retry_with_backoff(MARKER_RETRY_FIRST, MARKER_RETRY_BUDGET, || {
        std::fs::remove_file(marker)
    });
    match removed {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// Removes a marker whose restore is settled (applied, or recorded as failed). One that stays
/// is harmless: the next start finds its id in the live database or in `restore.failed`.
fn drop_marker(marker: &Path) {
    if let Err(error) = remove_marker(marker) {
        tracing::warn!(%error, "the restore is settled but its marker could not be removed");
    }
}

/// Replaces the marker with `restore.failed` (the backup's path, the reason code, the restore
/// id) and hands back the restore's own error, which wins even when this bookkeeping fails.
/// Should the marker outlive it, the next start finds the id in `restore.failed` and does not
/// retry a restore already reported as failed.
fn record_failure(
    marker: &Path,
    db_path: &Path,
    pending: &PendingRestore,
    error: AppError,
) -> AppError {
    let failed = db_path.with_file_name(RESTORE_FAILED_MARKER);
    let text = format!(
        "{}\n{}\n{}\n",
        pending.source.to_string_lossy(),
        error.code(),
        pending.id.as_deref().unwrap_or_default()
    );
    if let Err(io) = std::fs::write(&failed, text).and_then(|()| remove_marker(marker)) {
        tracing::error!(error = %io, restore_error = %error, "could not record the failed restore");
    }
    error
}

/// Copies the backup next to the database, marks the copy as cleanly shut down (it was a
/// running database when it was taken, and this start is a restore, not a crash recovery),
/// stamps it with the restore id and leaves in it the `backup.restored` row to journal
/// (`META_RESTORE_JOURNAL`): the record travels with the restored data, so a crash between the
/// swap and the first write cannot lose it.
fn stage(pending: &PendingRestore, staged: &Path) -> Result<(), AppError> {
    std::fs::copy(&pending.source, staged)?;
    let conn = Connection::open(staged)?;
    meta_set(&conn, META_CLEAN_SHUTDOWN, "1")?;
    if let Some(id) = &pending.id {
        meta_set(&conn, META_RESTORE_ID, id)?;
    }
    meta_set(
        &conn,
        META_RESTORE_JOURNAL,
        &pending.journal_payload().to_string(),
    )?;
    // Closed before the swap renames the file: Windows refuses to rename an open file.
    conn.close().map_err(|(_, error)| AppError::from(error))
}

/// Moves `-wal` and `-shm` aside (a WAL left by a power cut must never be replayed onto the
/// restored copy), then renames the staged copy over `pos.db` in one step. While the previous
/// process still holds the files, Windows refuses the renames; any failure puts the sidecars
/// back, so the current database stays whole for the next attempt. A put-back that fails is
/// logged and left for `recover_set_aside_sidecars`.
fn swap_in(staged: &Path, db_path: &Path) -> std::io::Result<()> {
    let mut set_aside = Vec::new();
    let result =
        set_sidecars_aside(db_path, &mut set_aside).and_then(|()| std::fs::rename(staged, db_path));
    for (live, aside) in set_aside.iter().rev() {
        if result.is_ok() {
            remove_quietly(aside);
        } else if let Err(error) = std::fs::rename(aside, live) {
            tracing::error!(%error, path = %aside.display(), "could not put a WAL file back");
        }
    }
    result
}

/// `pos.db-wal` and `pos.db-shm`, each with the name it takes while set aside during a swap.
fn sidecars(db_path: &Path) -> [(PathBuf, PathBuf); 2] {
    ["-wal", "-shm"].map(|suffix| {
        let live = sidecar_path(db_path, suffix);
        let aside = sidecar_path(&live, SET_ASIDE_SUFFIX);
        (live, aside)
    })
}

/// Renames each existing sidecar to `<name>.replaced`, recording what it moved.
fn set_sidecars_aside(
    db_path: &Path,
    set_aside: &mut Vec<(PathBuf, PathBuf)>,
) -> std::io::Result<()> {
    for (live, aside) in sidecars(db_path) {
        if live.exists() {
            std::fs::rename(&live, &aside)?;
            set_aside.push((live, aside));
        }
    }
    Ok(())
}

fn set_aside_remains(db_path: &Path) -> bool {
    sidecars(db_path).iter().any(|(_, aside)| aside.exists())
}

/// Repairs a crash (or a failed put-back) between setting the sidecars aside and the final
/// rename. The staged copy disappears only when that rename gives it the database's name, so
/// while it exists the swap never happened: the set-aside sidecars belong to the current
/// database and go back where no live one has taken their place. Once it is gone, `pos.db` is
/// the restored copy and the set-aside files are the replaced database's: they are deleted,
/// never replayed onto it.
fn recover_set_aside_sidecars(db_path: &Path, staged: &Path) {
    let swapped = !staged.exists();
    for (live, aside) in sidecars(db_path) {
        if !aside.exists() {
            continue;
        }
        if swapped {
            remove_quietly(&aside);
        } else if live.exists() {
            tracing::error!(path = %aside.display(), "a set-aside WAL file was replaced by a live one; keeping both");
        } else if let Err(error) = std::fs::rename(&aside, &live) {
            tracing::error!(%error, path = %aside.display(), "could not put a WAL file back");
        } else {
            tracing::warn!(path = %live.display(), "put back a WAL file an interrupted restore had set aside");
        }
    }
}

/// Deletes the files of `dir` that `is_leftover` names and that are older than this run: a
/// crashed run's temporary copies. Newer ones may belong to a copy this run is still writing.
fn sweep_leftovers(dir: &Path, is_leftover: impl Fn(&str) -> bool) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            tracing::warn!(%error, dir = %dir.display(), "could not look for leftover files");
            return;
        }
    };
    let started = run_started();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let stale = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .is_ok_and(|at| at < started);
        if stale && is_leftover(&name) {
            remove_quietly(&entry.path());
        }
    }
}

/// The folder holding `path` (`.` for a bare file name).
fn folder_of(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}

/// An error worth waiting out in `retry_with_backoff`; any other error is returned at once.
trait Transient {
    fn is_transient(&self) -> bool;
}

impl Transient for std::io::Error {
    /// Windows answers for a file another process holds open with a sharing violation (32),
    /// a lock violation (33) or access denied (5).
    fn is_transient(&self) -> bool {
        matches!(self.raw_os_error(), Some(5 | 32 | 33))
    }
}

/// Calls `attempt` until it succeeds, sleeping `first`, then twice as long each time. Gives up
/// with the error at once when it is not transient, or once `budget` has been slept.
fn retry_with_backoff<T, E: Transient>(
    first: Duration,
    budget: Duration,
    mut attempt: impl FnMut() -> Result<T, E>,
) -> Result<T, E> {
    let mut delay = first;
    let mut slept = Duration::ZERO;
    loop {
        match attempt() {
            Ok(value) => return Ok(value),
            Err(error) if slept >= budget || !error.is_transient() => return Err(error),
            Err(_) => {
                let pause = delay.min(budget.saturating_sub(slept));
                std::thread::sleep(pause);
                slept += pause;
                delay = delay.saturating_mul(2);
            }
        }
    }
}

fn sidecar_path(db_path: &Path, suffix: &str) -> PathBuf {
    let mut name = db_path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Deletes a file this module made (a temporary copy, an old backup, a set-aside sidecar).
/// A file already gone is fine; any other failure is logged, never fatal.
pub(crate) fn remove_quietly(path: &Path) {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => tracing::warn!(%error, path = %path.display(), "could not remove a file"),
    }
}

/// Backup files in `dir`, newest first; an absent folder lists nothing. The folder is the source
/// of truth: the local backups folder (`Db::list_backups`) or a mirror folder.
pub fn list_backups_in(dir: &Path) -> Result<Vec<BackupInfo>, AppError> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let Some(kind) = kind_of(&file_name) else {
            continue;
        };
        let meta = entry.metadata()?;
        if !meta.is_file() {
            continue;
        }
        let at = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .and_then(|d| i64::try_from(d.as_millis()).ok())
            .unwrap_or(0);
        out.push(BackupInfo {
            path: entry.path().to_string_lossy().into_owned(),
            file_name,
            at,
            size: i64::try_from(meta.len()).unwrap_or(i64::MAX),
            kind,
        });
    }
    out.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| b.file_name.cmp(&a.file_name)));
    Ok(out)
}

/// How many automatic backups rotation keeps (DAT-007; settings `backup.keep_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Retention {
    /// The newest copy of each of this many most recent past business days (every copy of
    /// today is kept whatever this says).
    pub daily: u32,
    /// Then the newest copy of each of this many older ISO weeks holding no kept copy yet.
    pub weekly: u32,
    /// Then the newest copy of each of this many older months holding no kept copy yet.
    pub monthly: u32,
}

impl Default for Retention {
    fn default() -> Retention {
        Retention {
            daily: KEEP_DAILY as u32,
            weekly: KEEP_WEEKLY as u32,
            monthly: 12,
        }
    }
}

impl Retention {
    /// The shop's choice (`backup.keep_daily`, `backup.keep_weekly`, `backup.keep_monthly`).
    pub fn from_settings(conn: &Connection) -> Result<Retention, AppError> {
        Ok(Retention {
            daily: crate::settings::get(conn, "backup.keep_daily")?,
            weekly: crate::settings::get(conn, "backup.keep_weekly")?,
            monthly: crate::settings::get(conn, "backup.keep_monthly")?,
        })
    }
}

/// The automatic backups rotation deletes, given their time (from the file name) and path.
/// Every copy of `today` or later stays (a Z close and the morning copy of the same day; a
/// clock running behind). Of each older business day only the newest copy can stay, chosen in
/// three passes from the newest day: the `daily` most recent days, then the newest day of each
/// of `weekly` ISO weeks holding no kept copy yet, then the newest day of each of `monthly`
/// months holding no kept copy yet (a month the weekly pass already reached costs no slot).
pub fn select_to_prune(
    mut items: Vec<(i64, PathBuf)>,
    retention: Retention,
    today: NaiveDate,
) -> Vec<PathBuf> {
    items.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    let mut prune = Vec::new();
    // The newest copy of each past day, newest day first (the sort keeps a day's copies together).
    let mut days: Vec<(NaiveDate, PathBuf)> = Vec::new();
    for (at, path) in items {
        let day = business_date(at);
        if day >= today {
            continue;
        }
        if days.last().is_some_and(|(newer, _)| *newer == day) {
            prune.push(path);
        } else {
            days.push((day, path));
        }
    }
    let daily = usize::try_from(retention.daily).unwrap_or(usize::MAX);
    let mut kept: Vec<bool> = (0..days.len()).map(|index| index < daily).collect();
    keep_one_per_period(&days, &mut kept, retention.weekly, |day| {
        let iso = day.iso_week();
        (iso.year(), iso.week())
    });
    keep_one_per_period(&days, &mut kept, retention.monthly, |day| {
        (day.year(), day.month())
    });
    prune.extend(
        days.into_iter()
            .zip(kept)
            .filter(|(_, keep)| !keep)
            .map(|((_, path), _)| path),
    );
    prune
}

/// One rotation pass: walking `days` from the newest, keeps the first day of each of `limit`
/// periods (weeks, months) that no kept day falls in yet.
fn keep_one_per_period<P: PartialEq>(
    days: &[(NaiveDate, PathBuf)],
    kept: &mut [bool],
    limit: u32,
    period_of: impl Fn(NaiveDate) -> P,
) {
    let mut covered: Vec<P> = days
        .iter()
        .zip(kept.iter())
        .filter(|(_, keep)| **keep)
        .map(|((day, _), _)| period_of(*day))
        .collect();
    let mut used = 0_u32;
    for ((day, _), keep) in days.iter().zip(kept.iter_mut()) {
        if used >= limit {
            break;
        }
        let period = period_of(*day);
        if !*keep && !covered.contains(&period) {
            covered.push(period);
            *keep = true;
            used += 1;
        }
    }
}

/// [`select_to_prune`] with the default retention and today's business date.
pub fn select_auto_backups_to_prune(items: Vec<(i64, PathBuf)>) -> Vec<PathBuf> {
    select_to_prune(items, Retention::default(), business_date(now_ms()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{DbOptions, meta_get};
    use std::collections::HashSet;

    /// The retry test stands in for sharing violations with plain strings.
    impl Transient for &str {
        fn is_transient(&self) -> bool {
            true
        }
    }

    fn open(dir: &Path) -> Db {
        Db::open(&dir.join("pos.db"), DbOptions::default()).unwrap()
    }

    #[test]
    fn file_names_encode_time_and_kind() {
        assert_eq!(
            file_name_for(1_790_596_800_123, BackupKind::Manual),
            "pos-20260928-130000-123-manual.db"
        );
        assert_eq!(
            kind_of(&file_name_for(1_790_596_800_000, BackupKind::PreMigration)),
            Some(BackupKind::PreMigration)
        );
        assert_eq!(kind_of("notes.txt"), None);
    }

    #[test]
    fn a_pre_pairing_copy_is_listed_by_its_file_name() {
        assert_eq!(BackupKind::PrePairing.as_str(), "pre_pairing");
        assert_eq!(
            BackupKind::parse("pre_pairing"),
            Some(BackupKind::PrePairing)
        );
        let name = file_name_for(1_790_593_200_000, BackupKind::PrePairing);
        assert!(name.ends_with("-pre_pairing.db"), "{name}");
        assert_eq!(kind_of(&name), Some(BackupKind::PrePairing));
    }

    #[test]
    fn backup_creates_a_verified_single_file_copy() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(dir.path());
        db.write(|tx| meta_set(tx, "probe", "42")).unwrap();
        let info = db.backup(BackupKind::Manual).unwrap();
        assert!(info.file_name.starts_with("pos-") && info.file_name.ends_with("-manual.db"));
        let copy = Connection::open(&info.path).unwrap();
        let value: String = copy
            .query_row("SELECT value FROM app_meta WHERE key = 'probe'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(value, "42");
        let mode: String = copy
            .pragma_query_value(None, "journal_mode", |r| r.get(0))
            .unwrap();
        assert_eq!(mode, "delete");
        let logged: i64 = db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT COUNT(*) FROM backups_log WHERE kind = 'manual' AND ok = 1",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(logged, 1);
        assert_eq!(db.list_backups().unwrap()[0].file_name, info.file_name);
        assert!(db.integrity_check().unwrap());
    }

    #[test]
    fn an_hourly_backup_is_made_when_the_last_is_an_hour_old_and_today_keeps_the_newest() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("pos.db"), DbOptions::default()).unwrap();
        let noon = crate::testkit::NOON;
        let first = db.hourly_backup_if_due(noon, 2).unwrap();
        assert!(first.is_some());
        assert!(
            db.hourly_backup_if_due(noon + 59 * 60_000, 2)
                .unwrap()
                .is_none(),
            "less than an hour"
        );
        for h in 1..=3 {
            assert!(
                db.hourly_backup_if_due(noon + h * 3_600_000, 2)
                    .unwrap()
                    .is_some()
            );
        }
        let today: Vec<_> = db
            .list_backups()
            .unwrap()
            .into_iter()
            .filter(|b| b.kind == BackupKind::Auto)
            .collect();
        assert_eq!(today.len(), 2, "the two newest copies of the day stay");
        assert!(
            db.hourly_backup_if_due(noon + 5 * 3_600_000, 0)
                .unwrap()
                .is_none(),
            "0 turns the hourly copies off"
        );
        assert!(db.quick_check().unwrap());
        assert!(
            db.read(|c| meta_get(c, "last_quick_check"))
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn daily_backup_runs_once_per_business_day() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(dir.path());
        assert!(db.daily_backup_if_due().unwrap().is_some());
        assert!(db.daily_backup_if_due().unwrap().is_none());
    }

    #[test]
    fn unclean_shutdown_is_detected_and_checked() {
        let dir = tempfile::tempdir().unwrap();
        {
            let db = open(dir.path());
            assert!(
                !db.report().unclean_shutdown,
                "a new file is not an unclean shutdown"
            );
        } // dropped without mark_clean_shutdown: simulates a power cut
        let db = open(dir.path());
        assert!(db.report().unclean_shutdown);
        assert!(db.report().integrity_ok);
        db.mark_clean_shutdown().unwrap();
        drop(db);
        assert!(!open(dir.path()).report().unclean_shutdown);
    }

    #[test]
    fn scheduled_restore_replaces_the_database_on_next_start() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pos.db");
        let db = open(dir.path());
        db.write(|tx| meta_set(tx, "probe", "before")).unwrap();
        let chosen = db.backup(BackupKind::Manual).unwrap();
        db.write(|tx| meta_set(tx, "probe", "after")).unwrap();
        db.schedule_restore(&chosen.file_name, None).unwrap();
        assert!(
            db.list_backups()
                .unwrap()
                .iter()
                .any(|b| b.kind == BackupKind::PreRestore),
            "today's data must be saved before a restore"
        );
        db.mark_clean_shutdown().unwrap();
        drop(db);

        let restored = apply_pending_restore(&path).unwrap();
        assert_eq!(restored.as_deref(), Some(Path::new(&chosen.path)));
        assert!(!dir.path().join(RESTORE_MARKER).exists());
        let db = open(dir.path());
        assert!(!db.report().unclean_shutdown);
        assert_eq!(
            db.read(|c| meta_get(c, "probe")).unwrap().as_deref(),
            Some("before")
        );
    }

    #[test]
    fn restore_refuses_a_damaged_backup() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(dir.path());
        let backups = dir.path().join("backups");
        std::fs::create_dir_all(&backups).unwrap();
        std::fs::write(
            backups.join("pos-20260101-000000-000-manual.db"),
            vec![7_u8; 4_096],
        )
        .unwrap();
        let err = db
            .schedule_restore("pos-20260101-000000-000-manual.db", None)
            .unwrap_err();
        assert_eq!(err.code(), "backup_invalid");
        assert!(!dir.path().join(RESTORE_MARKER).exists());
        assert_eq!(
            db.schedule_restore("pos-missing.db", None)
                .unwrap_err()
                .code(),
            "backup_not_found"
        );
    }

    #[test]
    fn restore_swap_is_retried_with_backoff_then_given_up() {
        // The previous process releases the files after two refusals: the third attempt wins.
        let mut calls = 0;
        let started = std::time::Instant::now();
        let out = retry_with_backoff(Duration::from_millis(10), Duration::from_secs(1), || {
            calls += 1;
            if calls < 3 {
                Err("sharing violation")
            } else {
                Ok(calls)
            }
        });
        assert_eq!(out, Ok(3));
        assert!(
            started.elapsed() >= Duration::from_millis(30),
            "slept 10 ms then 20 ms"
        );
        // Never released: attempts after 0, 10, 30 and 70 ms, then the last error comes back.
        let mut calls = 0;
        let out: Result<(), &str> =
            retry_with_backoff(Duration::from_millis(10), Duration::from_millis(70), || {
                calls += 1;
                Err("sharing violation")
            });
        assert_eq!(out, Err("sharing violation"));
        assert_eq!(calls, 4);
    }

    #[test]
    fn failed_restore_keeps_the_current_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pos.db");
        let db = open(dir.path());
        db.write(|tx| meta_set(tx, "probe", "current")).unwrap();
        db.mark_clean_shutdown().unwrap();
        drop(db);
        let damaged = dir.path().join("damaged.db");
        std::fs::write(&damaged, vec![7_u8; 4_096]).unwrap();
        std::fs::write(
            dir.path().join(RESTORE_MARKER),
            damaged.to_string_lossy().as_bytes(),
        )
        .unwrap();

        assert_eq!(
            apply_pending_restore(&path).unwrap_err().code(),
            "backup_invalid"
        );
        assert!(!dir.path().join(RESTORE_MARKER).exists());
        assert!(dir.path().join(RESTORE_FAILED_MARKER).exists());
        let db = open(dir.path());
        assert_eq!(
            db.read(|c| meta_get(c, "probe")).unwrap().as_deref(),
            Some("current")
        );
    }

    #[test]
    fn prune_keeps_14_newest_and_one_per_week_for_8_weeks() {
        let day = 86_400_000_i64;
        let base = 1_790_596_800_000_i64; // 2026-09-28 12:00Z
        let items: Vec<(i64, PathBuf)> = (0..120)
            .map(|d| (base - d * day, PathBuf::from(format!("auto-{d}.db"))))
            .collect();
        // The M0 tiers alone (no monthly copies), on the day after the newest copy.
        let retention = Retention {
            monthly: 0,
            ..Retention::default()
        };
        let today = business_date(base + day);
        let pruned: HashSet<PathBuf> = select_to_prune(items.clone(), retention, today)
            .into_iter()
            .collect();
        let kept: Vec<&(i64, PathBuf)> =
            items.iter().filter(|(_, p)| !pruned.contains(p)).collect();
        assert_eq!(kept.len(), KEEP_DAILY + KEEP_WEEKLY);
        for d in 0..14 {
            assert!(!pruned.contains(&PathBuf::from(format!("auto-{d}.db"))));
        }
        let weeks: HashSet<(i32, u32)> = kept[KEEP_DAILY..]
            .iter()
            .map(|(at, _)| {
                let week = business_date(*at).iso_week();
                (week.year(), week.week())
            })
            .collect();
        assert_eq!(
            weeks.len(),
            KEEP_WEEKLY,
            "one kept backup per older ISO week"
        );
    }

    // ---- Beyond the brief: Task 4 carry-forward and the remaining public fns ----

    fn probe(db: &Db) -> Option<String> {
        db.read(|c| meta_get(c, "probe")).unwrap()
    }

    /// "before" is in a manual backup, "after" is live, and that backup is scheduled.
    fn scheduled_restore(dir: &Path) -> (Db, BackupInfo) {
        let db = open(dir);
        db.write(|tx| meta_set(tx, "probe", "before")).unwrap();
        let chosen = db.backup(BackupKind::Manual).unwrap();
        db.write(|tx| meta_set(tx, "probe", "after")).unwrap();
        db.schedule_restore(&chosen.file_name, None).unwrap();
        (db, chosen)
    }

    /// Files next to the database other than `pos.db` itself (WAL sidecars, staging copies).
    fn leftovers(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("pos.db") && name != "pos.db")
            .collect();
        names.sort();
        names
    }

    #[test]
    fn restore_never_replays_the_wal_left_by_a_power_cut() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pos.db");
        assert_eq!(
            apply_pending_restore(&path).unwrap(),
            None,
            "no marker: nothing to do"
        );
        let (db, chosen) = scheduled_restore(dir.path());
        // The power cut: `pos.db-wal` (which holds "after") and `pos.db-shm` stay on disk.
        let wal = std::fs::read(sidecar_path(&path, "-wal")).unwrap();
        let shm = std::fs::read(sidecar_path(&path, "-shm")).unwrap();
        assert!(!wal.is_empty());
        drop(db);
        std::fs::write(sidecar_path(&path, "-wal"), &wal).unwrap();
        std::fs::write(sidecar_path(&path, "-shm"), &shm).unwrap();

        let restored = apply_pending_restore(&path).unwrap();
        assert_eq!(restored.as_deref(), Some(Path::new(&chosen.path)));
        assert_eq!(leftovers(dir.path()), Vec::<String>::new());
        let db = open(dir.path());
        assert_eq!(probe(&db).as_deref(), Some("before"));
        assert!(!db.report().unclean_shutdown);
        assert!(db.integrity_check().unwrap());
    }

    /// Windows refuses to rename a file another connection holds open: the swap waits for it.
    #[cfg(windows)]
    #[test]
    fn restore_waits_for_the_previous_process_to_release_the_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pos.db");
        let (db, chosen) = scheduled_restore(dir.path());
        db.mark_clean_shutdown().unwrap();
        assert!(
            sidecar_path(&path, "-wal").exists(),
            "the previous process holds the WAL"
        );
        let started = std::time::Instant::now();
        let previous = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            drop(db);
        });

        let restored = apply_pending_restore(&path).unwrap();
        assert!(
            started.elapsed() >= Duration::from_millis(300),
            "the swap succeeded while the files were still open"
        );
        previous.join().unwrap();
        assert_eq!(restored.as_deref(), Some(Path::new(&chosen.path)));
        assert_eq!(leftovers(dir.path()), Vec::<String>::new());
        assert_eq!(probe(&open(dir.path())).as_deref(), Some("before"));
    }

    #[test]
    fn a_failed_swap_puts_the_wal_back_with_the_current_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pos.db");
        std::fs::write(&path, b"current").unwrap();
        std::fs::write(sidecar_path(&path, "-wal"), b"current wal").unwrap();
        std::fs::write(sidecar_path(&path, "-shm"), b"current shm").unwrap();
        // No staged copy: the final rename fails after both sidecars were moved aside.
        let err = swap_in(&sidecar_path(&path, STAGED_SUFFIX), &path).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
        assert_eq!(std::fs::read(&path).unwrap(), b"current");
        assert_eq!(
            std::fs::read(sidecar_path(&path, "-wal")).unwrap(),
            b"current wal"
        );
        assert_eq!(
            std::fs::read(sidecar_path(&path, "-shm")).unwrap(),
            b"current shm"
        );
        assert_eq!(leftovers(dir.path()), ["pos.db-shm", "pos.db-wal"]);
    }

    #[test]
    fn close_marks_a_clean_shutdown_and_empties_the_wal() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(dir.path());
        db.write(|tx| meta_set(tx, "probe", "kept")).unwrap();
        let other = db.clone();
        db.close().unwrap();
        // Another handle (the shell's managed state) keeps the connections open: the mark is
        // written and the WAL is checkpointed and truncated all the same.
        assert_eq!(
            std::fs::metadata(dir.path().join("pos.db-wal"))
                .unwrap()
                .len(),
            0
        );
        assert_eq!(
            other
                .read(|c| meta_get(c, META_CLEAN_SHUTDOWN))
                .unwrap()
                .as_deref(),
            Some("1")
        );
        other.close().unwrap();
        assert_eq!(
            leftovers(dir.path()),
            Vec::<String>::new(),
            "the last close removes -wal and -shm"
        );
        let db = open(dir.path());
        assert!(!db.report().unclean_shutdown);
        assert_eq!(probe(&db).as_deref(), Some("kept"));
        Db::open_memory().unwrap().close().unwrap();
    }

    #[test]
    fn writes_are_refused_once_the_clean_shutdown_mark_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(dir.path());
        let late = db.clone();
        db.close().unwrap();
        // The shell's managed handle outlives the close: a late command must not write behind
        // the mark, or the next start would skip the integrity check after a crash.
        let refused = late.write(|tx| meta_set(tx, "probe", "late")).unwrap_err();
        assert_eq!(refused.code(), "internal_error");
        assert_eq!(
            late.backup(BackupKind::Manual).unwrap_err().code(),
            "internal_error"
        );
        assert!(late.list_backups().unwrap().is_empty(), "no copy was made");
        assert_eq!(probe(&late), None, "reads still work");
        assert_eq!(
            late.read(|c| meta_get(c, META_CLEAN_SHUTDOWN))
                .unwrap()
                .as_deref(),
            Some("1")
        );
        late.close().unwrap(); // a second close (exit after a restore) is harmless
        let db = open(dir.path());
        assert!(!db.report().unclean_shutdown);
        db.write(|tx| meta_set(tx, "probe", "next run")).unwrap();

        let memory = Db::open_memory().unwrap();
        memory.mark_clean_shutdown().unwrap();
        assert!(memory.write(|tx| meta_set(tx, "probe", "late")).is_err());
        assert_eq!(memory.read(|c| meta_get(c, "probe")).unwrap(), None);
    }

    #[test]
    fn only_a_sound_pos_database_of_this_version_is_a_valid_backup() {
        let dir = tempfile::tempdir().unwrap();
        let good = open(dir.path()).backup(BackupKind::Manual).unwrap();
        validate_backup_file(Path::new(&good.path)).unwrap();
        let code = |path: &Path| validate_backup_file(path).unwrap_err().code();
        let empty = dir.path().join("empty.db");
        std::fs::write(&empty, b"").unwrap();
        assert_eq!(
            code(&empty),
            "backup_invalid",
            "SQLite opens an empty file as an empty database"
        );
        assert_eq!(code(&dir.path().join("missing.db")), "backup_invalid");
        let newer = dir.path().join("newer.db");
        std::fs::copy(&good.path, &newer).unwrap();
        Connection::open(&newer)
            .unwrap()
            .pragma_update(None, "user_version", 99)
            .unwrap();
        assert_eq!(code(&newer), "database_newer_than_app");
    }

    #[test]
    fn list_backups_is_newest_first_and_skips_other_files() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(dir.path());
        assert_eq!(
            db.list_backups().unwrap(),
            Vec::new(),
            "no backups folder yet"
        );
        let older = db.backup(BackupKind::Manual).unwrap();
        let newer = db.backup(BackupKind::Auto).unwrap();
        let backups = dir.path().join("backups");
        std::fs::write(backups.join("notes.txt"), b"x").unwrap();
        std::fs::write(backups.join("pos-20260101-000000-000-manual.db.tmp"), b"x").unwrap();
        std::fs::create_dir(backups.join("pos-20260101-000000-000-auto.db")).unwrap();
        let listed = db.list_backups().unwrap();
        let names: Vec<&str> = listed.iter().map(|b| b.file_name.as_str()).collect();
        assert_eq!(names, [newer.file_name.as_str(), older.file_name.as_str()]);
        assert_eq!(
            (listed[0].kind, listed[0].size, listed[0].path.as_str()),
            (BackupKind::Auto, newer.size, newer.path.as_str())
        );

        let memory = Db::open_memory().unwrap();
        assert_eq!(memory.list_backups().unwrap(), Vec::new());
        assert_eq!(
            memory.backup(BackupKind::Manual).unwrap_err().code(),
            "backup_dir_missing"
        );
        assert_eq!(
            memory
                .schedule_restore(&older.file_name, None)
                .unwrap_err()
                .code(),
            "memory_database"
        );
    }

    #[test]
    fn daily_backup_prunes_old_automatic_backups_only() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(dir.path());
        let backups = dir.path().join("backups");
        std::fs::create_dir_all(&backups).unwrap();
        let now = std::time::SystemTime::now();
        let old_file = |days_ago: u32, kind: BackupKind| {
            let at = now - Duration::from_secs(86_400) * days_ago;
            let ms = i64::try_from(at.duration_since(UNIX_EPOCH).unwrap().as_millis()).unwrap();
            let path = backups.join(file_name_for(ms, kind));
            std::fs::File::create(&path)
                .unwrap()
                .set_modified(at)
                .unwrap();
            path
        };
        for days_ago in 1..=40 {
            old_file(days_ago, BackupKind::Auto);
        }
        let manual = old_file(90, BackupKind::Manual);
        let mut autos: Vec<(i64, PathBuf)> = db
            .list_backups()
            .unwrap()
            .into_iter()
            .filter(|b| b.kind == BackupKind::Auto)
            .map(|b| (b.at, PathBuf::from(b.path)))
            .collect();

        let today = db.daily_backup_if_due().unwrap().unwrap();
        autos.push((today.at, PathBuf::from(&today.path)));
        let pruned: HashSet<PathBuf> = select_auto_backups_to_prune(autos.clone())
            .into_iter()
            .collect();
        assert!(!pruned.is_empty());
        for (_, path) in &autos {
            assert_eq!(path.exists(), !pruned.contains(path), "{}", path.display());
        }
        assert!(manual.exists(), "manual backups are never pruned");
    }

    #[test]
    fn pre_migration_copy_is_taken_and_logged_on_the_opening_connection() {
        let dir = tempfile::tempdir().unwrap();
        open(dir.path()).close().unwrap();
        let conn = Connection::open(dir.path().join("pos.db")).unwrap();
        let info = backup_connection(&conn, &dir.path().join("backups"), BackupKind::PreMigration)
            .unwrap();
        assert!(info.file_name.ends_with("-pre_migration.db"));
        validate_backup_file(Path::new(&info.path)).unwrap();
        let logged: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM backups_log WHERE kind = 'pre_migration' AND ok = 1 AND path = ?1",
                [&info.path],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(logged, 1);
    }

    #[test]
    fn backup_kind_and_info_serialize_for_the_ui() {
        for kind in BackupKind::ALL {
            assert_eq!(BackupKind::parse(kind.as_str()), Some(kind));
            assert_eq!(
                serde_json::to_value(kind).unwrap(),
                serde_json::json!(kind.as_str())
            );
        }
        assert_eq!(BackupKind::parse("weekly"), None);
        let info = BackupInfo {
            file_name: "f.db".into(),
            path: "p/f.db".into(),
            at: 1,
            size: 2,
            kind: BackupKind::PreRestore,
        };
        assert_eq!(
            serde_json::to_value(&info).unwrap(),
            serde_json::json!({ "fileName": "f.db", "path": "p/f.db", "at": 1, "size": 2, "kind": "pre_restore" })
        );
    }

    // ---- Review round 1 ----

    #[test]
    fn file_names_give_back_their_time() {
        for at in [1_790_596_800_123, 1_798_759_799_999, 1_798_759_800_000] {
            assert_eq!(time_of(&file_name_for(at, BackupKind::Auto)), Some(at));
        }
        assert_eq!(time_of("pos-2026-0928-130000-manual.db"), None);
        assert_eq!(time_of("pos-20261328-130000-000-auto.db"), None, "month 13");
        assert_eq!(time_of("notes.txt"), None);
    }

    fn days_ago(days: u64) -> std::time::SystemTime {
        std::time::SystemTime::now() - Duration::from_secs(86_400 * days)
    }

    fn set_modified(path: &Path, at: std::time::SystemTime) {
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(at)
            .unwrap();
    }

    /// `(seq, hash)` of the last audit row.
    fn tail_of(db: &Db) -> (i64, String) {
        db.read(|c| Ok(journal_tail_on(c)?)).unwrap()
    }

    fn journal_tail_on(c: &Connection) -> rusqlite::Result<(i64, String)> {
        c.query_row(
            "SELECT seq, hash FROM audit_log ORDER BY seq DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
    }

    #[test]
    fn a_restore_journals_its_requester_and_the_replaced_journal_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pos.db");
        let db = open(dir.path());
        let chosen = db.backup(BackupKind::Manual).unwrap();
        // The journal grows after the chosen backup: these rows are what the restore replaces.
        let owner = crate::ids::new_id();
        let actor = crate::audit::Actor {
            user_id: Some(owner.clone()),
            terminal_id: None,
        };
        db.write(move |tx| {
            for action in ["auth.login", "backup.restore_scheduled"] {
                crate::audit::append(
                    tx,
                    &actor.event(action, "backup", None, serde_json::json!({})),
                )?;
            }
            Ok(())
        })
        .unwrap();
        let replaced = tail_of(&db);
        assert_eq!(replaced.0, 2);
        db.schedule_restore(&chosen.file_name, Some(&owner))
            .unwrap();
        let saved = db
            .list_backups()
            .unwrap()
            .into_iter()
            .find(|b| b.kind == BackupKind::PreRestore)
            .unwrap();
        db.close().unwrap();
        let marker = std::fs::read_to_string(dir.path().join(RESTORE_MARKER)).unwrap();
        let lines: Vec<&str> = marker.lines().collect();
        assert_eq!(
            lines[2..6],
            [
                owner.as_str(),
                saved.file_name.as_str(),
                "2",
                replaced.1.as_str()
            ]
        );

        assert!(apply_pending_restore(&path).unwrap().is_some());
        let db = open(dir.path());
        assert!(
            db.read(|c| meta_get(c, META_RESTORE_JOURNAL))
                .unwrap()
                .is_some(),
            "the record travels with the restored data"
        );
        assert_eq!(
            db.write(|tx| journal_pending_restore(tx, Some("terminal-1")))
                .unwrap(),
            Some(Vec::new()),
            "nothing was numbered: nothing is issued twice"
        );
        assert_eq!(
            db.write(|tx| journal_pending_restore(tx, Some("terminal-1")))
                .unwrap(),
            None,
            "journaled once"
        );
        assert_eq!(
            db.read(|c| meta_get(c, META_RESTORE_JOURNAL)).unwrap(),
            None
        );
        let (user, terminal, entity, payload): (Option<String>, Option<String>, String, String) =
            db.read(|c| {
                Ok(c.query_row(
                    "SELECT user_id, terminal_id, entity_id, payload FROM audit_log
                     WHERE action = 'backup.restored'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )?)
            })
            .unwrap();
        let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(
            (user, terminal.as_deref()),
            (Some(owner.clone()), Some("terminal-1"))
        );
        assert_eq!(
            payload,
            serde_json::json!({
                "restoreId": lines[1],
                "requestedBy": owner,
                "source": chosen.path,
                "preRestoreFile": saved.file_name,
                "replacedLastSeq": 2,
                "replacedLastHash": replaced.1,
                "replacedMarks": { "counters": [], "chains": [], "closures": [] },
                "reissued": [],
            })
        );
        assert_eq!(entity, lines[1]);
        let report = db.read(crate::audit::verify_chain).unwrap();
        assert_eq!((report.rows, report.first_broken_seq), (1, None));
        // The anchor points at rows the pre-restore copy still holds.
        let copy = Connection::open(&saved.path).unwrap();
        assert_eq!(journal_tail_on(&copy).unwrap(), replaced);
    }

    #[test]
    fn markers_from_before_the_journal_anchor_still_parse() {
        let legacy = PendingRestore::parse("C:/b/pos-1.db\nid-1\n");
        assert_eq!(
            (legacy.source, legacy.id.as_deref(), legacy.requested_by),
            (PathBuf::from("C:/b/pos-1.db"), Some("id-1"), None)
        );
        assert_eq!(
            (legacy.pre_restore_file, legacy.replaced_last_seq),
            (None, None)
        );
        let full = PendingRestore {
            source: PathBuf::from("C:/b/pos-2.db"),
            id: Some("id-2".into()),
            requested_by: None,
            pre_restore_file: Some("pos-x-pre_restore.db".into()),
            replaced_last_seq: Some(41),
            replaced_last_hash: Some("ab".repeat(32)),
            replaced_marks: Some(HighWaterMarks {
                counters: vec![CounterMark {
                    doc_type: "ticket".into(),
                    series: "C1".into(),
                    fiscal_year: 2026,
                    last_number: 9,
                }],
                ..HighWaterMarks::default()
            }),
            damaged_folder: None,
        };
        let back = PendingRestore::parse(&full.to_text());
        assert_eq!(back.journal_payload(), full.journal_payload());
        assert_eq!(
            back.journal_payload()["requestedBy"],
            serde_json::Value::Null
        );
    }

    #[test]
    fn a_marker_with_a_damaged_folder_round_trips_and_older_markers_still_parse() {
        let pending = PendingRestore {
            source: PathBuf::from("C:/data/backups/pos-x-manual.db"),
            id: Some("r1".into()),
            requested_by: Some("u1".into()),
            pre_restore_file: None,
            replaced_last_seq: None,
            replaced_last_hash: None,
            replaced_marks: None,
            damaged_folder: Some(PathBuf::from("C:/data/backups/corrupt-20260928-120000")),
        };
        let parsed = PendingRestore::parse(&pending.to_text());
        assert_eq!(parsed.damaged_folder, pending.damaged_folder);
        let payload = parsed.journal_payload();
        assert_eq!(payload["guided"], serde_json::json!(true));
        let old = PendingRestore::parse("C:/b.db\nr2\nu2\npre.db\n7\nabc\n\n");
        assert_eq!(old.damaged_folder, None);
        assert!(
            old.journal_payload().get("damagedFolder").is_none(),
            "M0 payloads are unchanged"
        );
    }

    /// The reissued ranges print versement receipts as the customer holds them (`RV2026-…`) and
    /// supplier returns with their series; customer, supplier and product codes are not listed.
    #[test]
    fn reissued_ranges_print_every_series_and_leave_the_codes_out() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(dir.path());
        let mark = |doc_type: &str, series: &str, fiscal_year: i64, last_number: i64| CounterMark {
            doc_type: doc_type.into(),
            series: series.into(),
            fiscal_year,
            last_number,
        };
        let marks = HighWaterMarks {
            counters: vec![
                mark("credit_receipt", "", 2026, 3),
                mark("customer_code", "", 0, 9),
                mark("product_reference", "", 0, 40),
                mark("return_note", "F", 2026, 2),
                mark("goods_receipt", "", 0, 1),
            ],
            ..HighWaterMarks::default()
        };
        let ranges = db.read(move |c| reissued_ranges(c, &marks, 2026)).unwrap();
        assert_eq!(
            ranges
                .iter()
                .map(|r| (r.first.as_str(), r.last.as_str()))
                .collect::<Vec<_>>(),
            [
                ("RV2026-000001", "RV2026-000003"),
                ("BR2026-F-000001", "BR2026-F-000002"),
                ("RC2026-000001", "RC2026-000001"),
            ]
        );
    }

    /// Tickets and a Z numbered after the chosen backup: the restored copy's counters move past
    /// them (journaled skips) instead of numbering them again. The replaced high-water marks and
    /// the skipped ranges are journaled; nothing is left to be issued twice.
    #[test]
    fn a_restore_skips_the_numbers_the_replaced_database_issued() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pos.db");
        let db = open(dir.path());
        db.write(|tx| crate::numbering::next_number(tx, "ticket", "C1", 2026).map(drop))
            .unwrap();
        let chosen = db.backup(BackupKind::Manual).unwrap();
        db.write(|tx| {
            for _ in 0..3 {
                crate::numbering::next_number(tx, "ticket", "C1", 2026)?;
            }
            crate::numbering::next_number(tx, crate::numbering::Z_COUNTER, "C1", 0)?;
            Ok(())
        })
        .unwrap();
        db.schedule_restore(&chosen.file_name, None).unwrap();
        db.close().unwrap();
        assert!(apply_pending_restore(&path).unwrap().is_some());
        let db = open(dir.path());
        let reissued = db
            .write(|tx| journal_pending_restore(tx, None))
            .unwrap()
            .unwrap();
        assert!(reissued.is_empty(), "nothing is handed out twice");
        let (tickets, z) = db
            .read(|c| {
                Ok((
                    crate::numbering::journaled_skips(c, "ticket", "C1")?,
                    crate::numbering::journaled_skips(c, crate::numbering::Z_COUNTER, "C1")?,
                ))
            })
            .unwrap();
        let ranges = |skips: &[crate::numbering::Skip]| {
            skips
                .iter()
                .map(|s| (s.fiscal_year, s.from, s.to))
                .collect::<Vec<_>>()
        };
        assert_eq!(ranges(&tickets), [(2026, 2, 4)]);
        assert_eq!(ranges(&z), [(0, 1, 1)]);
        let payload: String = db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT payload FROM audit_log WHERE action = 'backup.restored'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(
            payload["replacedMarks"]["counters"][0],
            serde_json::json!({ "docType": "ticket", "series": "C1", "fiscalYear": 2026, "lastNumber": 4 })
        );
        assert_eq!(payload["reissued"], serde_json::json!([]));
        assert_eq!(
            payload["skipped"][0],
            serde_json::json!({
                "id": tickets[0].id, "docType": "ticket", "series": "C1", "fiscalYear": 2026,
                "from": 2, "to": 4, "first": "TK2026-C1-000002", "last": "TK2026-C1-000004",
            })
        );
        assert_eq!(payload["skipped"][1]["first"], "ZC1-000001");
        assert_eq!(
            db.write(|tx| crate::numbering::next_number(tx, "ticket", "C1", 2026))
                .unwrap(),
            5
        );
    }

    /// Both behaviours together: the restored counters jump past the replaced database's
    /// versement receipts and codes (journaled skips), and the journal prints each skip as it was
    /// printed (`RV2026-…`, a code as its number); nothing is left to be reissued.
    #[test]
    fn a_restore_skips_versement_receipts_and_codes_as_they_were_printed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pos.db");
        let db = open(dir.path());
        let chosen = db.backup(BackupKind::Manual).unwrap();
        db.write(|tx| {
            for _ in 0..2 {
                crate::numbering::next_number(tx, "credit_receipt", "", 2026)?;
            }
            crate::numbering::next_number(tx, "customer_code", "", 0)?;
            Ok(())
        })
        .unwrap();
        db.schedule_restore(&chosen.file_name, None).unwrap();
        db.close().unwrap();
        assert!(apply_pending_restore(&path).unwrap().is_some());
        let db = open(dir.path());
        let reissued = db
            .write(|tx| journal_pending_restore(tx, None))
            .unwrap()
            .unwrap();
        assert!(reissued.is_empty(), "nothing is handed out twice");
        let payload: String = db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT payload FROM audit_log WHERE action = 'backup.restored'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
        let skipped: Vec<(&str, &str, &str)> = payload["skipped"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                (
                    s["docType"].as_str().unwrap(),
                    s["first"].as_str().unwrap(),
                    s["last"].as_str().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            skipped,
            [
                ("credit_receipt", "RV2026-000001", "RV2026-000002"),
                ("customer_code", "1", "1"),
            ]
        );
        assert_eq!(
            db.read(crate::customers::receipts::verify)
                .unwrap()
                .first_broken_number,
            None,
            "the receipts the restore skipped are no gap"
        );
        assert_eq!(
            db.write(|tx| crate::numbering::next_number(tx, "credit_receipt", "", 2026))
                .unwrap(),
            3
        );
    }

    #[test]
    fn a_restore_already_in_the_live_database_is_not_applied_twice() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pos.db");
        let marker = dir.path().join(RESTORE_MARKER);
        let (db, chosen) = scheduled_restore(dir.path());
        db.close().unwrap();
        let pending = std::fs::read_to_string(&marker).unwrap();
        assert_eq!(
            pending.lines().collect::<Vec<_>>()[0],
            chosen.path,
            "first line: the backup"
        );
        assert!(
            crate::ids::is_valid_id(pending.lines().nth(1).unwrap()),
            "second line: the restore id"
        );
        assert!(apply_pending_restore(&path).unwrap().is_some());

        // The shop sells all day on the restored data, then the same marker is found again
        // (its removal failed after the swap): the sales must survive the next start.
        let db = open(dir.path());
        db.write(|tx| meta_set(tx, "probe", "sold today")).unwrap();
        db.close().unwrap();
        std::fs::write(&marker, &pending).unwrap();
        assert_eq!(apply_pending_restore(&path).unwrap(), None);
        assert!(!marker.exists());
        assert_eq!(probe(&open(dir.path())).as_deref(), Some("sold today"));
    }

    /// An antivirus scan holds the marker without FILE_SHARE_DELETE, so it cannot be removed.
    #[cfg(windows)]
    #[test]
    fn a_marker_that_cannot_be_removed_does_not_fail_a_finished_restore() {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ: u32 = 1;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pos.db");
        let marker = dir.path().join(RESTORE_MARKER);
        let (db, chosen) = scheduled_restore(dir.path());
        db.close().unwrap();
        let held = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(&marker)
            .unwrap();

        let restored = apply_pending_restore(&path).unwrap();
        assert_eq!(restored.as_deref(), Some(Path::new(&chosen.path)));
        assert!(marker.exists());
        drop(held);
        assert_eq!(
            apply_pending_restore(&path).unwrap(),
            None,
            "already applied"
        );
        assert!(!marker.exists());
        assert_eq!(probe(&open(dir.path())).as_deref(), Some("before"));
    }

    #[test]
    fn a_crash_between_setting_the_wal_aside_and_the_swap_is_recovered() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pos.db");
        let db = open(dir.path());
        db.write(|tx| meta_set(tx, "probe", "only in the wal"))
            .unwrap();
        let main = std::fs::read(&path).unwrap();
        let wal = std::fs::read(sidecar_path(&path, "-wal")).unwrap();
        let shm = std::fs::read(sidecar_path(&path, "-shm")).unwrap();
        drop(db);
        // Without its WAL, the database file lacks the last commit.
        let alone = tempfile::tempdir().unwrap();
        std::fs::write(alone.path().join("pos.db"), &main).unwrap();
        let conn = Connection::open(alone.path().join("pos.db")).unwrap();
        assert_eq!(meta_get(&conn, "probe").unwrap(), None);
        drop(conn);

        // What the crash left: pos.db as last checkpointed, its WAL and SHM set aside, and the
        // staged copy that never took pos.db's name, all from the previous run.
        std::fs::write(&path, &main).unwrap();
        std::fs::write(
            sidecar_path(&sidecar_path(&path, "-wal"), SET_ASIDE_SUFFIX),
            &wal,
        )
        .unwrap();
        std::fs::write(
            sidecar_path(&sidecar_path(&path, "-shm"), SET_ASIDE_SUFFIX),
            &shm,
        )
        .unwrap();
        let staged = sidecar_path(&path, STAGED_SUFFIX);
        std::fs::write(&staged, b"staged copy").unwrap();
        set_modified(&staged, days_ago(1));

        assert_eq!(apply_pending_restore(&path).unwrap(), None);
        assert_eq!(leftovers(dir.path()), ["pos.db-shm", "pos.db-wal"]);
        assert_eq!(probe(&open(dir.path())).as_deref(), Some("only in the wal"));
    }

    #[test]
    fn wal_files_set_aside_by_a_finished_swap_are_discarded() {
        // A crash right after the staged copy took pos.db's name: the set-aside files belong
        // to the replaced database and must never be replayed onto the restored one.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pos.db");
        let (db, _) = scheduled_restore(dir.path());
        let wal = std::fs::read(sidecar_path(&path, "-wal")).unwrap();
        db.close().unwrap();
        assert!(apply_pending_restore(&path).unwrap().is_some());
        std::fs::write(
            sidecar_path(&sidecar_path(&path, "-wal"), SET_ASIDE_SUFFIX),
            &wal,
        )
        .unwrap();

        assert_eq!(apply_pending_restore(&path).unwrap(), None);
        assert_eq!(leftovers(dir.path()), Vec::<String>::new());
        assert_eq!(probe(&open(dir.path())).as_deref(), Some("before"));
    }

    #[test]
    fn rotation_never_deletes_the_backup_it_just_made() {
        // A dead CMOS battery: the clock is behind, and 22 automatic backups (one a week) look
        // newer than the one made now.
        let dir = tempfile::tempdir().unwrap();
        let db = open(dir.path());
        let backups = dir.path().join("backups");
        std::fs::create_dir_all(&backups).unwrap();
        let week = Duration::from_secs(7 * 86_400);
        for weeks_ahead in 1..=22 {
            let at = std::time::SystemTime::now() + week * weeks_ahead;
            let ms = i64::try_from(at.duration_since(UNIX_EPOCH).unwrap().as_millis()).unwrap();
            let file = backups.join(file_name_for(ms, BackupKind::Auto));
            std::fs::write(&file, b"x").unwrap();
            set_modified(&file, at);
        }
        let made = db.daily_backup_if_due().unwrap().unwrap();
        assert!(Path::new(&made.path).exists());
    }

    #[test]
    fn the_daily_backup_goes_by_the_time_in_the_file_name() {
        // Today's automatic backup, copied back from a USB stick with last week's file date.
        let dir = tempfile::tempdir().unwrap();
        let db = open(dir.path());
        let backups = dir.path().join("backups");
        std::fs::create_dir_all(&backups).unwrap();
        let file = backups.join(file_name_for(now_ms(), BackupKind::Auto));
        std::fs::write(&file, b"x").unwrap();
        set_modified(&file, days_ago(7));
        assert_eq!(db.daily_backup_if_due().unwrap(), None);
    }

    #[test]
    fn leftovers_of_an_earlier_run_are_swept() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(dir.path());
        let backups = dir.path().join("backups");
        std::fs::create_dir_all(&backups).unwrap();
        let stale = backups.join("pos-20260101-000000-000-auto.db.tmp");
        let in_progress = backups.join("pos-20260101-000000-001-manual.db.tmp");
        std::fs::write(&stale, b"x").unwrap();
        std::fs::write(&in_progress, b"x").unwrap();
        set_modified(&stale, days_ago(1));
        set_modified(
            &in_progress,
            std::time::SystemTime::now() + Duration::from_secs(60),
        );
        db.daily_backup_if_due().unwrap();
        assert!(!stale.exists(), "a crashed run's temporary copy");
        assert!(in_progress.exists(), "a copy this run may still be writing");
    }

    #[test]
    fn only_files_in_use_are_waited_for() {
        let mut calls = 0;
        let out: Result<(), std::io::Error> = retry_with_backoff(
            Duration::from_millis(20),
            Duration::from_millis(200),
            || {
                calls += 1;
                Err(std::io::ErrorKind::NotFound.into())
            },
        );
        assert_eq!(out.unwrap_err().kind(), std::io::ErrorKind::NotFound);
        assert_eq!(calls, 1, "a missing file is not waited for");
        for os_error in [5, 32, 33] {
            let mut calls = 0;
            let out: Result<(), std::io::Error> =
                retry_with_backoff(Duration::from_millis(1), Duration::from_millis(3), || {
                    calls += 1;
                    Err(std::io::Error::from_raw_os_error(os_error))
                });
            assert_eq!(out.unwrap_err().raw_os_error(), Some(os_error));
            assert!(calls > 1, "os error {os_error} is waited for");
        }
    }

    #[test]
    fn a_vanished_backup_fails_at_once_and_the_reason_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pos.db");
        open(dir.path()).close().unwrap();
        let missing = dir
            .path()
            .join("backups")
            .join("pos-20260101-000000-000-manual.db");
        let id = crate::ids::new_id();
        std::fs::write(
            dir.path().join(RESTORE_MARKER),
            format!("{}\n{id}\n", missing.display()),
        )
        .unwrap();
        let started = std::time::Instant::now();
        assert_eq!(
            apply_pending_restore(&path).unwrap_err().code(),
            "backup_invalid"
        );
        assert!(started.elapsed() < Duration::from_secs(1));
        let failed = std::fs::read_to_string(dir.path().join(RESTORE_FAILED_MARKER)).unwrap();
        assert_eq!(
            failed.lines().collect::<Vec<_>>(),
            [
                missing.to_string_lossy().as_ref(),
                "backup_invalid",
                id.as_str()
            ],
            "the backup, the reason, the restore id"
        );
        assert!(!dir.path().join(RESTORE_MARKER).exists());
    }

    #[test]
    fn a_failed_restore_returns_its_own_error_even_if_it_cannot_be_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pos.db");
        open(dir.path()).close().unwrap();
        // `restore.failed` cannot be written: a folder already has that name.
        std::fs::create_dir(dir.path().join(RESTORE_FAILED_MARKER)).unwrap();
        let damaged = dir.path().join("damaged.db");
        std::fs::write(&damaged, vec![7_u8; 4_096]).unwrap();
        std::fs::write(
            dir.path().join(RESTORE_MARKER),
            damaged.to_string_lossy().as_bytes(),
        )
        .unwrap();
        assert_eq!(
            apply_pending_restore(&path).unwrap_err().code(),
            "backup_invalid"
        );
    }

    // ---- Review round 2 ----

    #[test]
    fn a_marker_already_gone_counts_as_removed() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join(RESTORE_MARKER);
        std::fs::write(&marker, b"x").unwrap();
        remove_marker(&marker).unwrap();
        assert!(!marker.exists());
        let started = std::time::Instant::now();
        remove_marker(&marker).unwrap();
        assert!(
            started.elapsed() < MARKER_RETRY_BUDGET,
            "not retried either"
        );
    }

    /// A restore scheduled, then interrupted between setting the WAL aside and the final
    /// rename; `block` then keeps the WAL from going back under its own name.
    fn interrupted_restore_with_a_stuck_wal(dir: &Path, block: impl FnOnce(&Path)) {
        let (db, _) = scheduled_restore(dir);
        db.close().unwrap();
        let path = dir.join("pos.db");
        std::fs::write(sidecar_path(&path, STAGED_SUFFIX), b"earlier staged copy").unwrap();
        let aside = sidecar_path(&sidecar_path(&path, "-wal"), SET_ASIDE_SUFFIX);
        std::fs::write(&aside, b"the current database's wal").unwrap();
        block(&aside);
    }

    fn assert_stopped_untouched(dir: &Path) {
        let path = dir.join("pos.db");
        assert_eq!(
            apply_pending_restore(&path).unwrap_err().code(),
            "restore_needs_attention"
        );
        let aside = sidecar_path(&sidecar_path(&path, "-wal"), SET_ASIDE_SUFFIX);
        assert_eq!(
            std::fs::read(&aside).unwrap(),
            b"the current database's wal"
        );
        assert_eq!(
            std::fs::read(sidecar_path(&path, STAGED_SUFFIX)).unwrap(),
            b"earlier staged copy",
            "nothing was staged over the signal that the swap never happened"
        );
        assert!(
            dir.join(RESTORE_MARKER).exists(),
            "the restore is still pending"
        );
        assert!(!dir.join(RESTORE_FAILED_MARKER).exists());
    }

    #[test]
    fn a_wal_that_cannot_go_back_stops_the_start() {
        // A folder has taken the WAL's name.
        let dir = tempfile::tempdir().unwrap();
        interrupted_restore_with_a_stuck_wal(dir.path(), |_| {
            std::fs::create_dir(dir.path().join("pos.db-wal")).unwrap();
        });
        assert_stopped_untouched(dir.path());
    }

    /// The put-back rename itself fails: another program holds the set-aside WAL.
    #[cfg(windows)]
    #[test]
    fn a_wal_held_by_another_program_stops_the_start() {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ: u32 = 1;
        let dir = tempfile::tempdir().unwrap();
        let mut held = None;
        interrupted_restore_with_a_stuck_wal(dir.path(), |aside| {
            held = Some(
                std::fs::OpenOptions::new()
                    .read(true)
                    .share_mode(FILE_SHARE_READ)
                    .open(aside)
                    .unwrap(),
            );
        });
        assert_stopped_untouched(dir.path());
        drop(held);
    }

    #[test]
    fn a_restore_recorded_as_failed_is_never_retried() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pos.db");
        let marker = dir.path().join(RESTORE_MARKER);
        let (db, chosen) = scheduled_restore(dir.path());
        db.close().unwrap();
        let pending = std::fs::read_to_string(&marker).unwrap();
        let id = pending.lines().nth(1).unwrap().to_owned();
        // The first attempt fails: the backup cannot be read at that moment.
        let good = std::fs::read(&chosen.path).unwrap();
        std::fs::write(&chosen.path, vec![7_u8; 4_096]).unwrap();
        assert_eq!(
            apply_pending_restore(&path).unwrap_err().code(),
            "backup_invalid"
        );
        let failed = std::fs::read_to_string(dir.path().join(RESTORE_FAILED_MARKER)).unwrap();
        assert_eq!(failed.lines().nth(2), Some(id.as_str()));

        // The marker outlived that failure (its removal failed), the backup is readable again
        // and the shop sold after the failure notice: the next start restores nothing.
        std::fs::write(&chosen.path, &good).unwrap();
        std::fs::write(&marker, &pending).unwrap();
        let db = open(dir.path());
        db.write(|tx| meta_set(tx, "probe", "sold after the notice"))
            .unwrap();
        db.close().unwrap();
        assert_eq!(apply_pending_restore(&path).unwrap(), None);
        assert!(!marker.exists());
        assert_eq!(
            probe(&open(dir.path())).as_deref(),
            Some("sold after the notice")
        );
    }

    /// An antivirus holds the marker for a moment while a failed restore is recorded.
    #[cfg(windows)]
    #[test]
    fn a_failed_restore_waits_briefly_to_remove_its_marker() {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ: u32 = 1;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pos.db");
        let marker = dir.path().join(RESTORE_MARKER);
        open(dir.path()).close().unwrap();
        let damaged = dir.path().join("damaged.db");
        std::fs::write(&damaged, vec![7_u8; 4_096]).unwrap();
        std::fs::write(
            &marker,
            format!("{}\n{}\n", damaged.display(), crate::ids::new_id()),
        )
        .unwrap();
        let held = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(&marker)
            .unwrap();
        let antivirus = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            drop(held);
        });

        assert_eq!(
            apply_pending_restore(&path).unwrap_err().code(),
            "backup_invalid"
        );
        antivirus.join().unwrap();
        assert!(!marker.exists(), "the removal waited for the marker");
        assert!(dir.path().join(RESTORE_FAILED_MARKER).exists());
    }

    // ---- M6: retention by day, week and month; any folder's listing; the weekly check ----

    use chrono::NaiveDate;

    fn local_ms(y: i32, m: u32, d: u32, h: u32) -> i64 {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_millis()
            - ALGIERS_OFFSET_MS
    }

    fn copy_on(y: i32, m: u32, d: u32, h: u32) -> (i64, PathBuf) {
        (
            local_ms(y, m, d, h),
            PathBuf::from(format!("{y:04}{m:02}{d:02}-{h:02}.db")),
        )
    }

    fn kept_names(items: &[(i64, PathBuf)], retention: Retention, today: NaiveDate) -> Vec<String> {
        let pruned = select_to_prune(items.to_vec(), retention, today);
        let mut kept: Vec<String> = items
            .iter()
            .filter(|(_, p)| !pruned.contains(p))
            .map(|(_, p)| p.to_string_lossy().into_owned())
            .collect();
        kept.sort();
        kept
    }

    #[test]
    fn retention_keeps_every_copy_of_today_and_the_newest_of_each_recent_day() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 28).unwrap();
        let items = vec![
            copy_on(2026, 9, 28, 9),
            copy_on(2026, 9, 28, 21), // today's Z close and the morning copy both stay
            copy_on(2026, 9, 27, 9),
            copy_on(2026, 9, 27, 21),
            copy_on(2026, 9, 26, 9),
            copy_on(2026, 9, 25, 9),
        ];
        let retention = Retention {
            daily: 2,
            weekly: 0,
            monthly: 0,
        };
        assert_eq!(
            kept_names(&items, retention, today),
            [
                "20260926-09.db",
                "20260927-21.db",
                "20260928-09.db",
                "20260928-21.db"
            ]
        );
    }

    #[test]
    fn older_copies_thin_out_to_one_a_week_then_one_a_month() {
        // 2026-09-28 is a Monday: the seven daily copies (21–27 September) fill ISO week 39.
        let today = NaiveDate::from_ymd_opt(2026, 9, 28).unwrap();
        let mut items = Vec::new();
        let mut day = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        while day < today {
            items.push(copy_on(day.year(), day.month(), day.day(), 20));
            day = day.succ_opt().unwrap();
        }
        let kept = kept_names(
            &items,
            Retention {
                daily: 7,
                weekly: 4,
                monthly: 3,
            },
            today,
        );
        assert_eq!(kept.len(), 7 + 4 + 3, "{kept:?}");
        for name in [
            "20260927-20.db", // daily
            "20260921-20.db", // daily
            "20260920-20.db", // weekly: week 38
            "20260913-20.db", // weekly: week 37
            "20260906-20.db", // weekly: week 36
            "20260830-20.db", // weekly: week 35
            "20260731-20.db", // monthly: July (August already holds a weekly copy)
            "20260630-20.db",
            "20260531-20.db",
        ] {
            assert!(
                kept.contains(&name.to_owned()),
                "{name} missing from {kept:?}"
            );
        }
        assert!(
            !kept.contains(&"20260829-20.db".to_owned()),
            "August is covered by 30 August"
        );
        assert!(
            !kept.contains(&"20260430-20.db".to_owned()),
            "beyond the monthly limit"
        );
    }

    #[test]
    fn copies_dated_after_today_are_never_pruned() {
        // A dead CMOS battery: the clock is behind the copies already made.
        let today = NaiveDate::from_ymd_opt(2026, 9, 28).unwrap();
        let items: Vec<_> = (1..=30).map(|d| copy_on(2026, 10, d, 8)).collect();
        assert!(
            select_to_prune(
                items,
                Retention {
                    daily: 1,
                    weekly: 0,
                    monthly: 0
                },
                today
            )
            .is_empty()
        );
    }

    proptest::proptest! {
        #[test]
        fn rotation_never_keeps_more_than_its_limits_and_never_drops_a_recent_day(
            offsets in proptest::collection::vec((0_i64..400, 0_u32..24), 0..200),
            daily in 1_u32..20,
            weekly in 0_u32..10,
            monthly in 0_u32..10,
        ) {
            let today = NaiveDate::from_ymd_opt(2026, 9, 28).unwrap();
            let items: Vec<(i64, PathBuf)> = offsets
                .iter()
                .enumerate()
                .map(|(i, (days_ago, hour))| {
                    let day = today - chrono::Duration::days(*days_ago);
                    (local_ms(day.year(), day.month(), day.day(), *hour), PathBuf::from(format!("{i}.db")))
                })
                .collect();
            let retention = Retention { daily, weekly, monthly };
            let pruned = select_to_prune(items.clone(), retention, today);
            let kept: Vec<&(i64, PathBuf)> = items.iter().filter(|(_, p)| !pruned.contains(p)).collect();
            let of_today = items.iter().filter(|(at, _)| business_date(*at) >= today).count();
            proptest::prop_assert!(kept.len() <= of_today + (daily + weekly + monthly) as usize);
            let mut days: Vec<NaiveDate> = items
                .iter()
                .map(|(at, _)| business_date(*at))
                .filter(|d| *d < today)
                .collect();
            days.sort_unstable_by(|a, b| b.cmp(a));
            days.dedup();
            for day in days.iter().take(daily as usize) {
                let newest = items
                    .iter()
                    .filter(|(at, _)| business_date(*at) == *day)
                    .max_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)))
                    .unwrap();
                proptest::prop_assert!(!pruned.contains(&newest.1), "newest of {day} pruned");
            }
        }
    }

    #[test]
    fn the_listing_of_any_folder_reads_backup_names_only() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path()
                .join(file_name_for(1_790_596_800_123, BackupKind::Auto)),
            b"x",
        )
        .unwrap();
        std::fs::write(dir.path().join("notes.txt"), b"x").unwrap();
        let listed = list_backups_in(dir.path()).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].kind, BackupKind::Auto);
        assert!(
            list_backups_in(&dir.path().join("missing"))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn the_weekly_integrity_check_runs_once_a_week_and_is_journaled() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(dir.path());
        let actor = crate::audit::Actor::system(None);
        let now = now_ms();
        assert_eq!(
            db.integrity_check_if_due(actor.clone(), now).unwrap(),
            Some(true),
            "never checked yet"
        );
        assert_eq!(
            db.integrity_check_if_due(actor.clone(), now + 86_400_000)
                .unwrap(),
            None
        );
        assert_eq!(
            db.integrity_check_if_due(actor.clone(), now + 7 * 86_400_000 + 1)
                .unwrap(),
            Some(true)
        );
        assert!(db.check_integrity(actor, false).unwrap());
        let rows: Vec<(String, String)> = db
            .read(|c| {
                let mut stmt = c.prepare(
                    "SELECT action, payload FROM audit_log WHERE action = 'db.integrity_checked' ORDER BY seq",
                )?;
                let rows = stmt
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .unwrap();
        assert_eq!(rows.len(), 3);
        assert!(rows[0].1.contains("\"scheduled\":true") && rows[0].1.contains("\"ok\":true"));
        assert!(rows[2].1.contains("\"scheduled\":false"));
    }

    #[test]
    fn the_daily_backup_uses_the_retention_it_is_given() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(dir.path());
        let backups = dir.path().join("backups");
        std::fs::create_dir_all(&backups).unwrap();
        for days_ago in 1..=10_u64 {
            let at = std::time::SystemTime::now() - Duration::from_secs(86_400 * days_ago);
            let ms = i64::try_from(at.duration_since(UNIX_EPOCH).unwrap().as_millis()).unwrap();
            let path = backups.join(file_name_for(ms, BackupKind::Auto));
            std::fs::write(&path, b"x").unwrap();
            set_modified(&path, at);
        }
        let made = db
            .daily_backup_if_due_with(Retention {
                daily: 3,
                weekly: 0,
                monthly: 0,
            })
            .unwrap()
            .unwrap();
        let autos: Vec<BackupInfo> = db
            .list_backups()
            .unwrap()
            .into_iter()
            .filter(|b| b.kind == BackupKind::Auto)
            .collect();
        assert_eq!(
            autos.len(),
            1 + 3,
            "today's copy and the three newest past days"
        );
        assert!(autos.iter().any(|b| b.file_name == made.file_name));
    }

    // ---- M8 Task 17: the hourly copies of past days are rotated (DAT-007 "hourly, keep 24") ----

    fn auto_names(db: &Db) -> Vec<String> {
        let mut names: Vec<String> = db
            .list_backups()
            .unwrap()
            .into_iter()
            .filter(|b| b.kind == BackupKind::Auto)
            .map(|b| b.file_name)
            .collect();
        names.sort();
        names
    }

    #[test]
    fn the_next_days_hourly_copy_thins_the_day_before_to_its_newest() {
        // A café open past midnight, or a PC never switched off: hourly copies at 21:00, 22:00
        // and 23:00 on 28 September, then 00:00 on the 29th, and no start in between to run
        // the daily copy and its rotation.
        let dir = tempfile::tempdir().unwrap();
        let db = open(dir.path());
        let noon = crate::testkit::NOON;
        let made: Vec<String> = (9..=12)
            .map(|h| {
                db.hourly_backup_if_due(noon + h * 3_600_000, 24)
                    .unwrap()
                    .unwrap()
                    .file_name
            })
            .collect();
        assert_eq!(
            auto_names(&db),
            [made[2].clone(), made[3].clone()],
            "28 September keeps its 23:00 copy only, the 29th its first"
        );
    }

    #[test]
    fn the_hourly_copy_rotates_past_days_by_the_shops_retention() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(dir.path());
        let backups = dir.path().join("backups");
        std::fs::create_dir_all(&backups).unwrap();
        let noon = crate::testkit::NOON;
        for days_ago in 1..=10 {
            let at = noon - days_ago * 86_400_000;
            std::fs::write(backups.join(file_name_for(at, BackupKind::Auto)), b"x").unwrap();
        }
        for (key, value) in [
            ("backup.keep_daily", 7),
            ("backup.keep_weekly", 0),
            ("backup.keep_monthly", 0),
        ] {
            let actor = Actor::system(None);
            db.write(move |tx| crate::settings::set_value(tx, &actor, key, json!(value)))
                .unwrap();
        }
        db.hourly_backup_if_due(noon, 24).unwrap().unwrap();
        assert_eq!(
            auto_names(&db).len(),
            1 + 7,
            "the new copy and the seven newest past days"
        );
    }

    #[test]
    fn the_daily_backup_rotates_even_when_todays_copy_exists() {
        // Today's copy was made before this start (an hourly copy after midnight): the hourly
        // copies of two days ago are still thinned to the newest.
        let dir = tempfile::tempdir().unwrap();
        let db = open(dir.path());
        let backups = dir.path().join("backups");
        std::fs::create_dir_all(&backups).unwrap();
        let copy = |at: i64| {
            let path = backups.join(file_name_for(at, BackupKind::Auto));
            std::fs::write(&path, b"x").unwrap();
            path
        };
        let day = business_date(now_ms())
            .checked_sub_days(chrono::Days::new(2))
            .unwrap();
        let past: Vec<PathBuf> = [9, 10, 11]
            .into_iter()
            .map(|h| copy(local_ms(day.year(), day.month(), day.day(), h)))
            .collect();
        let todays = copy(now_ms());
        assert_eq!(
            db.daily_backup_if_due().unwrap(),
            None,
            "today's copy exists"
        );
        assert!(todays.exists());
        assert_eq!(
            past.iter().map(|p| p.exists()).collect::<Vec<_>>(),
            [false, false, true],
            "two days ago keeps its 11:00 copy only"
        );
    }
}

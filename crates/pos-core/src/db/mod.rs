//! SQLite access (spec §4.3): one writer thread running `BEGIN IMMEDIATE` transactions,
//! a read-only pool, forward-only migrations, backups and a clean-shutdown marker.

mod backup;
#[cfg(test)]
mod hook_tests;
#[cfg(test)]
mod m1_tests;
#[cfg(test)]
mod m2_tests;
#[cfg(test)]
mod m3_tests;
#[cfg(test)]
mod m4_tests;
#[cfg(test)]
mod m5_tests;
#[cfg(test)]
mod m7_tests;
#[cfg(test)]
mod m8_0009_tests;
#[cfg(test)]
mod m8_tests;
pub mod migrations;
pub mod mirror;
pub mod recovery;
#[cfg(test)]
mod recovery_till_tests;
#[cfg(test)]
mod tests;

pub use backup::*;

use crate::error::AppError;
use crossbeam_channel::{Receiver, Sender, bounded, unbounded};
use rusqlite::config::DbConfig;
use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior};
use serde::Serialize;
use specta::Type;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::{JoinHandle, ThreadId};
use std::time::Duration;

pub const BUSY_TIMEOUT_MS: u64 = 5_000;
pub const META_CLEAN_SHUTDOWN: &str = "clean_shutdown";
pub const META_INSTALL_ID: &str = "install_id";

#[derive(Debug, Clone)]
pub struct DbOptions {
    pub readers: usize,
    /// Defaults to `<database folder>/backups`.
    pub backup_dir: Option<PathBuf>,
}

impl Default for DbOptions {
    fn default() -> Self {
        Self {
            readers: 4,
            backup_dir: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OpenReport {
    pub unclean_shutdown: bool,
    pub integrity_ok: bool,
    pub schema_version: i64,
    pub migrations_applied: i64,
    pub pre_migration_backup: Option<String>,
}

type Job = Box<dyn FnOnce(&mut Connection) + Send + 'static>;

/// Runs inside every `Db::write` of this database once set (the LAN, M5): after the closure
/// succeeded and before `COMMIT`, so what it writes commits with the business write or not at
/// all; its error rolls the whole write back. `after_commit` runs once the transaction is
/// durable, on the calling thread. `dry_run` and `write_replica` never call it.
pub trait WriteHook: Send + Sync + 'static {
    fn before_commit(&self, tx: &Transaction<'_>) -> Result<(), AppError>;
    fn after_commit(&self) {}
}

/// Handle to the database. Cheap to clone; the writer thread stops when the last clone drops.
#[derive(Clone)]
pub struct Db {
    inner: Arc<Inner>,
}

struct Inner {
    jobs: Option<Sender<Job>>,
    writer: Option<JoinHandle<()>>,
    writer_id: ThreadId,
    readers: Option<ReaderPool>,
    path: Option<PathBuf>,
    backup_dir: Option<PathBuf>,
    report: OpenReport,
    /// Set by `mark_clean_shutdown` on the writer thread. Every later write is refused, so
    /// nothing lands behind the mark (a crash then would skip the integrity check).
    closed: Arc<AtomicBool>,
    /// The LAN's write hook (M5), shared by every clone of the handle.
    hook: RwLock<Option<Arc<dyn WriteHook>>>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        // Readers close first so the writer is the last connection: only a writable last
        // connection checkpoints the WAL into the database file and deletes `-wal`/`-shm`.
        drop(self.readers.take());
        drop(self.jobs.take()); // closing the queue ends the writer loop
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
    }
}

impl fmt::Debug for Db {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Db")
            .field("path", &self.inner.path)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Writer,
    Reader,
}

impl Db {
    /// Opens (or creates) the database file: refuses a newer schema, PRAGMAs, integrity check
    /// after an unclean shutdown, backup before pending migrations, migrations, then the
    /// reader pool.
    pub fn open(path: &Path, opts: DbOptions) -> Result<Db, AppError> {
        Db::open_with(
            path,
            opts,
            &migrations::MIGRATIONS,
            migrations::LATEST_VERSION,
        )
    }

    /// `open` with the migrations `list` whose last one is `latest` (tests add one of their own).
    fn open_with(
        path: &Path,
        opts: DbOptions,
        list: &rusqlite_migration::Migrations<'_>,
        latest: i64,
    ) -> Result<Db, AppError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let existed = path.exists();
        let backup_dir = opts
            .backup_dir
            .clone()
            .or_else(|| path.parent().map(|p| p.join("backups")));
        let mut conn = Connection::open(path)?;
        // A newer app's database is refused before anything in it changes (journal mode,
        // markers): `user_version` only reads the file header.
        let version = schema_version(&conn)?;
        if version > latest {
            return Err(AppError::conflict("database_newer_than_app"));
        }
        configure(&conn, Role::Writer)?;
        let mut report = OpenReport {
            integrity_ok: true,
            schema_version: version,
            ..OpenReport::default()
        };
        if existed && version > 0 {
            report.unclean_shutdown = meta_get(&conn, META_CLEAN_SHUTDOWN)?.as_deref() != Some("1");
            if report.unclean_shutdown {
                report.integrity_ok = integrity_ok(&conn)?;
                if !report.integrity_ok {
                    return Err(AppError::conflict("database_corrupt"));
                }
            }
        }
        let pending = list.pending_migrations(&conn)?;
        if pending > 0
            && existed
            && version > 0
            && let Some(dir) = &backup_dir
        {
            let info = backup::backup_connection(&conn, dir, BackupKind::PreMigration)?;
            report.pre_migration_backup = Some(info.path);
        }
        if pending > 0 && version > 0 {
            drop_lan_triggers(&mut conn)?;
        }
        list.to_latest(&mut conn)?;
        report.migrations_applied = i64::from(pending);
        report.schema_version = schema_version(&conn)?;
        mark_running(&conn)?;
        let readers = ReaderPool::open(path, opts.readers.max(1))?;
        Db::start(
            conn,
            Some(readers),
            Some(path.to_path_buf()),
            backup_dir,
            report,
        )
    }

    /// In-memory database for tests; reads run on the writer thread.
    pub fn open_memory() -> Result<Db, AppError> {
        let mut conn = Connection::open_in_memory()?;
        configure(&conn, Role::Writer)?;
        let pending = migrations::MIGRATIONS.pending_migrations(&conn)?;
        migrations::MIGRATIONS.to_latest(&mut conn)?;
        mark_running(&conn)?;
        let report = OpenReport {
            integrity_ok: true,
            schema_version: schema_version(&conn)?,
            migrations_applied: i64::from(pending),
            ..OpenReport::default()
        };
        Db::start(conn, None, None, None, report)
    }

    fn start(
        conn: Connection,
        readers: Option<ReaderPool>,
        path: Option<PathBuf>,
        backup_dir: Option<PathBuf>,
        report: OpenReport,
    ) -> Result<Db, AppError> {
        let (jobs, queue) = unbounded::<Job>();
        let writer = std::thread::Builder::new()
            .name("pos-db-writer".into())
            .spawn(move || writer_loop(conn, queue))?;
        let writer_id = writer.thread().id();
        Ok(Db {
            inner: Arc::new(Inner {
                jobs: Some(jobs),
                writer: Some(writer),
                writer_id,
                readers,
                path,
                backup_dir,
                report,
                closed: Arc::new(AtomicBool::new(false)),
                hook: RwLock::new(None),
            }),
        })
    }

    pub fn report(&self) -> &OpenReport {
        &self.inner.report
    }

    pub fn path(&self) -> Option<&Path> {
        self.inner.path.as_deref()
    }

    pub fn backup_dir(&self) -> Option<&Path> {
        self.inner.backup_dir.as_deref()
    }

    /// Installs (or removes) the write hook for every clone of this handle.
    pub fn set_write_hook(&self, hook: Option<Arc<dyn WriteHook>>) {
        match self.inner.hook.write() {
            Ok(mut slot) => *slot = hook,
            Err(poisoned) => *poisoned.into_inner() = hook,
        }
    }

    pub fn has_write_hook(&self) -> bool {
        self.hook().is_some()
    }

    fn hook(&self) -> Option<Arc<dyn WriteHook>> {
        match self.inner.hook.read() {
            Ok(slot) => slot.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// Runs `f` inside `BEGIN IMMEDIATE … COMMIT` on the single writer thread, then the write
    /// hook's `before_commit` in the same transaction when one is set.
    /// An `Err` rolls the whole transaction back. A panic is not caught: the release profile
    /// aborts (`panic = "abort"`), and the next start sees the unclean-shutdown marker and runs
    /// the integrity check. Refused once `mark_clean_shutdown` has run.
    pub fn write<T, F>(&self, f: F) -> Result<T, AppError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Transaction<'_>) -> Result<T, AppError> + Send + 'static,
    {
        let closed = self.closed_flag();
        let hook = self.hook();
        let before = hook.clone();
        let out = self.run_on_writer(move |conn| {
            refuse_when_closed(&closed)?;
            // A replica write that could not turn its triggers back on closes the handle; this
            // check is the second line: no business write ever runs without its triggers.
            if !conn.db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER)? {
                return Err(AppError::Internal("triggers are off".into()));
            }
            let mut tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let result = f(&mut tx).and_then(|out| match &before {
                Some(hook) => hook.before_commit(&tx).map(|()| out),
                None => Ok(out),
            });
            match result {
                Ok(out) => {
                    tx.commit()?;
                    Ok(out)
                }
                Err(e) => {
                    if let Err(rollback) = tx.rollback() {
                        // The next job rolls the leftover transaction back (`writer_loop`).
                        tracing::warn!(error = %rollback, "rollback of a failed write failed");
                    }
                    Err(e)
                }
            }
        })?;
        if let Some(hook) = hook {
            hook.after_commit();
        }
        Ok(out)
    }

    /// Runs `f` in `BEGIN IMMEDIATE … COMMIT` on the writer with SQLite triggers disabled
    /// (`SQLITE_DBCONFIG_ENABLE_TRIGGER`): a till mirrors the main PC's rows as they are, without
    /// its append-only, state and cache triggers (M5). Triggers are turned back on and the
    /// statement cache flushed whatever happened (SQLite also expires every prepared statement
    /// when the flag changes, so cached statements recompile with their triggers). If they cannot
    /// be turned back on, this handle refuses every later write. Never calls the write hook.
    pub fn write_replica<T, F>(&self, f: F) -> Result<T, AppError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Transaction<'_>) -> Result<T, AppError> + Send + 'static,
    {
        let closed = self.closed_flag();
        self.run_on_writer(move |conn| {
            refuse_when_closed(&closed)?;
            conn.set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, false)?;
            let result = match conn.transaction_with_behavior(TransactionBehavior::Immediate) {
                Ok(mut tx) => match f(&mut tx) {
                    Ok(out) => tx.commit().map(|()| out).map_err(AppError::from),
                    Err(e) => {
                        if let Err(rollback) = tx.rollback() {
                            tracing::warn!(error = %rollback, "rollback of a replica write failed");
                        }
                        Err(e)
                    }
                },
                Err(e) => Err(e.into()),
            };
            let restored = conn.set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, true);
            conn.flush_prepared_statement_cache();
            match restored {
                Ok(true) => result,
                Ok(false) | Err(_) => {
                    closed.store(true, Ordering::SeqCst);
                    tracing::error!("triggers could not be turned back on: the database is closed");
                    Err(AppError::Internal("triggers could not be restored".into()))
                }
            }
        })
    }

    /// Runs `f` on the writer inside `BEGIN IMMEDIATE` and always rolls back: the exact result
    /// of an operation (a return's refund, with its numbers and audit rows) without keeping
    /// anything. Blocks other writes while it runs, like `write`.
    pub fn dry_run<T, F>(&self, f: F) -> Result<T, AppError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Transaction<'_>) -> Result<T, AppError> + Send + 'static,
    {
        let closed = self.closed_flag();
        self.run_on_writer(move |conn| {
            refuse_when_closed(&closed)?;
            let mut tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let out = f(&mut tx);
            tx.rollback()?;
            out
        })
    }

    /// Copies the WAL into the database file without waiting for readers (`PASSIVE`), after a
    /// Z close (CSH-011). Returns `(frames in the WAL, frames copied)`; `(0, 0)` in memory.
    pub fn checkpoint(&self) -> Result<(i64, i64), AppError> {
        if self.path().is_none() {
            return Ok((0, 0));
        }
        self.run_on_writer(|conn| {
            Ok(conn.query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |r| {
                Ok((r.get(1)?, r.get(2)?))
            })?)
        })
    }

    /// Runs `f` on a read-only connection. In-memory databases have no readers: `f` runs on
    /// the writer with `query_only` on, so a read that writes fails there too.
    pub fn read<T, F>(&self, f: F) -> Result<T, AppError>
    where
        T: Send + 'static,
        F: FnOnce(&Connection) -> Result<T, AppError> + Send + 'static,
    {
        match &self.inner.readers {
            Some(pool) => pool.run(f),
            None => self.run_on_writer(move |conn| {
                conn.pragma_update(None, "query_only", "ON")?;
                let out = f(conn);
                conn.pragma_update(None, "query_only", "OFF")?;
                out
            }),
        }
    }

    fn closed_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.inner.closed)
    }

    fn run_on_writer<T, F>(&self, f: F) -> Result<T, AppError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> Result<T, AppError> + Send + 'static,
    {
        if std::thread::current().id() == self.inner.writer_id {
            // Called from inside a job: the writer would wait for itself forever.
            return Err(AppError::Internal("re-entrant database call".into()));
        }
        let (done, result) = bounded::<Result<T, AppError>>(1);
        let job: Job = Box::new(move |conn: &mut Connection| {
            // A dropped receiver only means the caller stopped waiting; nothing to report.
            let _ = done.send(f(conn));
        });
        let jobs = self.inner.jobs.as_ref().ok_or_else(writer_stopped)?;
        jobs.send(job).map_err(|_| writer_stopped())?;
        result.recv().map_err(|_| writer_stopped())?
    }
}

/// Drops the LAN's generated triggers (M5) before migrations run, as `capture::uninstall` does:
/// a till's guards would refuse a migration's write to a shared table (`host_required`), and a
/// column dropped or a table rebuilt would break the triggers naming it. Every PC runs the same
/// migration, so its data writes need no capture; the start sequence's `capture::ensure`
/// generates the triggers again for the new schema.
fn drop_lan_triggers(conn: &mut Connection) -> Result<(), AppError> {
    let tx = conn.transaction()?;
    crate::sync::capture::uninstall(&tx)?;
    tx.commit()?;
    Ok(())
}

fn writer_loop(mut conn: Connection, queue: Receiver<Job>) {
    for job in queue.iter() {
        roll_back_leftover(&conn, "writer");
        job(&mut conn);
    }
}

/// Rolls back a transaction that a job left open (a closure that ran `BEGIN` itself), so it
/// cannot block the next `BEGIN IMMEDIATE` or pin an old read snapshot.
fn roll_back_leftover(conn: &Connection, connection: &'static str) {
    if conn.is_autocommit() {
        return;
    }
    tracing::warn!(connection, "a job left a transaction open; rolling it back");
    if let Err(e) = conn.execute_batch("ROLLBACK") {
        tracing::warn!(connection, error = %e, "rolling back a leftover transaction failed");
    }
}

fn writer_stopped() -> AppError {
    AppError::Internal("database writer stopped".into())
}

/// Checked inside writer jobs: jobs run in queue order, so every write queued after the
/// clean-shutdown mark sees the flag, whichever thread sent it.
fn refuse_when_closed(closed: &AtomicBool) -> Result<(), AppError> {
    if closed.load(Ordering::SeqCst) {
        return Err(AppError::Internal(
            "the database is closed for shutdown".into(),
        ));
    }
    Ok(())
}

struct ReaderPool {
    give: Sender<Connection>,
    take: Receiver<Connection>,
}

impl ReaderPool {
    fn open(path: &Path, size: usize) -> Result<ReaderPool, AppError> {
        let (give, take) = bounded(size);
        for _ in 0..size {
            let conn = Connection::open_with_flags(
                path,
                OpenFlags::SQLITE_OPEN_READ_ONLY
                    | OpenFlags::SQLITE_OPEN_NO_MUTEX
                    | OpenFlags::SQLITE_OPEN_URI,
            )?;
            configure(&conn, Role::Reader)?;
            give.send(conn).map_err(|_| pool_closed())?;
        }
        Ok(ReaderPool { give, take })
    }

    fn run<T>(&self, f: impl FnOnce(&Connection) -> Result<T, AppError>) -> Result<T, AppError> {
        let lease = Lease {
            conn: Some(self.take.recv().map_err(|_| pool_closed())?),
            home: &self.give,
        };
        match lease.conn.as_ref() {
            Some(conn) => f(conn),
            None => Err(pool_closed()),
        }
    }
}

/// Returns the connection to the pool, without an open transaction, even if the closure
/// panics.
struct Lease<'a> {
    conn: Option<Connection>,
    home: &'a Sender<Connection>,
}

impl Drop for Lease<'_> {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            roll_back_leftover(&conn, "reader");
            let _ = self.home.send(conn);
        }
    }
}

fn pool_closed() -> AppError {
    AppError::Internal("reader pool closed".into())
}

fn configure(conn: &Connection, role: Role) -> Result<(), AppError> {
    conn.busy_timeout(Duration::from_millis(BUSY_TIMEOUT_MS))?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    // REPLACE's implicit delete fires DELETE triggers (audit_log append-only) only with this.
    conn.pragma_update(None, "recursive_triggers", "ON")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    conn.pragma_update(None, "cache_size", -16_000)?;
    match role {
        Role::Writer => {
            let mode: String =
                conn.pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get(0))?;
            // In-memory databases have no file (`path` is empty) and answer "memory".
            let file_backed = conn.path().is_some_and(|p| !p.is_empty());
            if file_backed && !mode.eq_ignore_ascii_case("wal") {
                return Err(AppError::Db(format!(
                    "journal_mode is {mode}, expected wal"
                )));
            }
            conn.pragma_update(None, "synchronous", "FULL")?;
        }
        Role::Reader => conn.pragma_update(None, "query_only", "ON")?,
    }
    Ok(())
}

fn mark_running(conn: &Connection) -> Result<(), AppError> {
    meta_set(conn, META_CLEAN_SHUTDOWN, "0")?;
    if meta_get(conn, META_INSTALL_ID)?.is_none() {
        meta_set(conn, META_INSTALL_ID, &crate::ids::new_id())?;
    }
    Ok(())
}

pub(crate) fn schema_version(conn: &Connection) -> Result<i64, AppError> {
    Ok(conn.pragma_query_value(None, "user_version", |r| r.get(0))?)
}

pub(crate) fn integrity_ok(conn: &Connection) -> Result<bool, AppError> {
    let mut stmt = conn.prepare("PRAGMA integrity_check")?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows == ["ok"])
}

/// Reads a local key from `app_meta` (absent before migration 0001).
pub fn meta_get(conn: &Connection, key: &str) -> Result<Option<String>, AppError> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'app_meta')",
        [],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(None);
    }
    Ok(conn
        .query_row("SELECT value FROM app_meta WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .optional()?)
}

pub fn meta_set(conn: &Connection, key: &str, value: &str) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO app_meta (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        (key, value),
    )?;
    Ok(())
}

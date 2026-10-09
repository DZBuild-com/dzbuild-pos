//! The `.dzpos` backup file (a zip of `manifest.json` and `pos.db`) and the full data export
//! (a zip of one CSV per table, `manifest.json`, `LISEZ-MOI.txt`). Importing a `.dzpos` checks
//! its SHA-256 and the database itself, then places it in the backups folder where the M0
//! restore screen lists it: an import never replaces the live database by itself.

use crate::locales::text;
use pos_core::dataset::{DATASET, columns, dump};
use pos_core::db::{BackupKind, backup_file_name, validate_backup_file};
use pos_core::error::AppError;
use pos_core::lang::Lang;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use specta::Type;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub const BACKUP_EXTENSION: &str = "dzpos";
const DB_ENTRY: &str = "pos.db";
const MANIFEST_ENTRY: &str = "manifest.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupManifest {
    pub format: u32,
    pub app_version: String,
    pub schema_version: i64,
    pub created_at: i64,
    pub company_name: String,
    pub nif: Option<String>,
    pub db_sha256: String,
    pub db_size: u64,
    /// The demo shop's copy (finding 32): it never replaces a real shop, whatever its name says
    /// (the restore checks the copy's own mark as well).
    #[serde(default)]
    pub demo: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportInfo {
    pub app_version: String,
    pub created_at: i64,
    pub company_name: String,
    pub nif: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FullExport {
    pub tables: usize,
    pub rows: u64,
}

/// The manifest's `dbSha256` of a database copy's bytes (Task 20's `backup_export_file`).
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// The SHA-256 and the size of the file at `path`, read in blocks: a database copy is never held
/// in memory whole (4 GB shop PCs).
pub fn sha256_file(path: &Path) -> Result<(String, u64), AppError> {
    let mut file = File::open(path).map_err(io)?;
    let (mut hasher, mut size, mut buf) = (Sha256::new(), 0_u64, vec![0_u8; 64 * 1024]);
    loop {
        let n = file.read(&mut buf).map_err(io)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        size += n as u64;
    }
    Ok((hex::encode(hasher.finalize()), size))
}

fn invalid() -> AppError {
    AppError::conflict("backup_file_invalid")
}

fn io(e: impl std::fmt::Display) -> AppError {
    AppError::Internal(format!("archive: {e}"))
}

fn deflated() -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .large_file(true)
}

/// Writes `out` through a `.tmp` sibling renamed at the end: a failure leaves no half file.
fn write_atomically(
    out: &Path,
    fill: impl FnOnce(File) -> Result<(), AppError>,
) -> Result<(), AppError> {
    let tmp = out.with_extension("tmp");
    let result = File::create(&tmp)
        .map_err(io)
        .and_then(fill)
        .and_then(|()| std::fs::rename(&tmp, out).map_err(io));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Writes `bytes` to `out` through a sibling renamed once they are on the disk: a failed write
/// leaves the file that was there as it was.
pub fn write_file(out: &Path, bytes: &[u8]) -> Result<(), AppError> {
    write_atomically(out, |mut file| {
        file.write_all(bytes).map_err(io)?;
        file.sync_all().map_err(io)
    })
}

/// Writes the `.dzpos` file of the database copy `db_file` with its manifest.
pub fn export_backup(
    db_file: &Path,
    manifest: &BackupManifest,
    out: &Path,
) -> Result<(), AppError> {
    write_atomically(out, |file| {
        let mut zip = ZipWriter::new(file);
        zip.start_file(MANIFEST_ENTRY, deflated()).map_err(io)?;
        zip.write_all(&serde_json::to_vec_pretty(manifest)?)
            .map_err(io)?;
        zip.start_file(DB_ENTRY, deflated()).map_err(io)?;
        std::io::copy(&mut File::open(db_file).map_err(io)?, &mut zip).map_err(io)?;
        zip.finish().map_err(io)?.sync_all().map_err(io)
    })
}

/// The manifest of a `.dzpos` file (`backup_file_invalid` when it is not one).
pub fn read_manifest(file: &Path) -> Result<BackupManifest, AppError> {
    let mut archive =
        ZipArchive::new(File::open(file).map_err(|_| invalid())?).map_err(|_| invalid())?;
    let entry = archive.by_name(MANIFEST_ENTRY).map_err(|_| invalid())?;
    serde_json::from_reader(entry).map_err(|_| invalid())
}

/// Extracts the database of a `.dzpos` file into `backups_dir` as a manual backup taken `now`.
pub fn import_backup(file: &Path, backups_dir: &Path, now: i64) -> Result<PathBuf, AppError> {
    let manifest = read_manifest(file)?;
    let target = backups_dir.join(backup_file_name(now, BackupKind::Manual));
    let tmp = target.with_extension("db.tmp");
    let result = (|| -> Result<(), AppError> {
        let mut archive =
            ZipArchive::new(File::open(file).map_err(|_| invalid())?).map_err(|_| invalid())?;
        let mut entry = archive.by_name(DB_ENTRY).map_err(|_| invalid())?;
        let mut out = File::create(&tmp).map_err(io)?;
        let (mut hasher, mut size, mut buf) = (Sha256::new(), 0_u64, vec![0_u8; 64 * 1024]);
        loop {
            let n = entry.read(&mut buf).map_err(|_| invalid())?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            out.write_all(&buf[..n]).map_err(io)?;
            size += n as u64;
        }
        out.sync_all().map_err(io)?;
        drop(out);
        if hex::encode(hasher.finalize()) != manifest.db_sha256 || size != manifest.db_size {
            return Err(invalid());
        }
        validate_backup_file(&tmp).map_err(|e| {
            if e.code() == "backup_invalid" {
                invalid()
            } else {
                e
            }
        })?;
        std::fs::rename(&tmp, &target).map_err(io)
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(target)
}

/// One CSV per table (UTF-8 with BOM, `;`, raw values: amounts in centimes, quantities in
/// thousandths, instants in Unix milliseconds), a manifest and a trilingual readme.
pub fn full_export(
    conn: &Connection,
    lang: Lang,
    info: &ExportInfo,
    out: &Path,
) -> Result<FullExport, AppError> {
    let mut total_rows = 0_u64;
    let mut counts = Vec::with_capacity(DATASET.len());
    write_atomically(out, |file| {
        let mut zip = ZipWriter::new(file);
        for table in DATASET {
            zip.start_file(format!("{table}.csv"), deflated())
                .map_err(io)?;
            zip.write_all("\u{feff}".as_bytes()).map_err(io)?;
            let mut writer = csv::WriterBuilder::new()
                .delimiter(b';')
                .terminator(csv::Terminator::CRLF)
                .from_writer(&mut zip);
            writer.write_record(columns(conn, table)?).map_err(io)?;
            let mut rows = 0_u64;
            dump(conn, table, &mut |cells| {
                rows += 1;
                writer.write_record(cells).map_err(io)
            })?;
            writer.flush().map_err(io)?;
            drop(writer);
            total_rows += rows;
            counts.push(json!({ "table": table, "rows": rows }));
        }
        zip.start_file(MANIFEST_ENTRY, deflated()).map_err(io)?;
        let manifest = json!({
            "format": 1, "export": info, "language": lang.as_str(), "tables": counts,
            "units": { "money": "centimes", "quantity": "thousandths", "rate": "basis points",
                       "instant": "unix milliseconds" }
        });
        zip.write_all(&serde_json::to_vec_pretty(&manifest)?)
            .map_err(io)?;
        zip.start_file("LISEZ-MOI.txt", deflated()).map_err(io)?;
        let readme: Vec<String> = [Lang::Fr, Lang::Ar, Lang::En]
            .iter()
            .filter_map(|l| text(l.as_str(), "archive.readme"))
            .collect();
        zip.write_all(readme.join("\r\n\r\n").as_bytes())
            .map_err(io)?;
        zip.finish().map_err(io)?.sync_all().map_err(io)
    })?;
    Ok(FullExport {
        tables: DATASET.len(),
        rows: total_rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn manifest(db: &Path) -> BackupManifest {
        let bytes = std::fs::read(db).unwrap();
        BackupManifest {
            format: 1,
            app_version: "0.2.0".into(),
            schema_version: 3,
            created_at: 1_790_593_200_000,
            company_name: "Supérette El Baraka".into(),
            nif: Some("000016001234567".into()),
            db_sha256: sha256_hex(&bytes),
            db_size: bytes.len() as u64,
            demo: false,
        }
    }

    fn sqlite_file(dir: &Path) -> PathBuf {
        let path = dir.join("pos-20260928-120000-000-manual.db");
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE t (x INTEGER); INSERT INTO t VALUES (1); PRAGMA user_version = 3;",
        )
        .unwrap();
        drop(conn);
        path
    }

    #[test]
    fn a_backup_file_round_trips_into_the_backups_folder() {
        let dir = tempfile::tempdir().unwrap();
        let db = sqlite_file(dir.path());
        let out = dir.path().join("sauvegarde.dzpos");
        export_backup(&db, &manifest(&db), &out).unwrap();
        assert_eq!(
            read_manifest(&out).unwrap().company_name,
            "Supérette El Baraka"
        );
        let backups = dir.path().join("backups");
        std::fs::create_dir(&backups).unwrap();
        let placed = import_backup(&out, &backups, 1_790_596_800_123).unwrap();
        assert_eq!(
            placed.file_name().unwrap().to_str().unwrap(),
            "pos-20260928-130000-123-manual.db"
        );
        assert_eq!(std::fs::read(&placed).unwrap(), std::fs::read(&db).unwrap());
    }

    #[test]
    fn a_file_is_hashed_without_being_read_whole() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("copie.db");
        let bytes: Vec<u8> = (0..200_000_u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(
            sha256_file(&path).unwrap(),
            (sha256_hex(&bytes), bytes.len() as u64)
        );
        assert_eq!(
            sha256_file(&dir.path().join("absent.db"))
                .unwrap_err()
                .code(),
            "internal_error"
        );
    }

    #[test]
    fn a_tampered_or_foreign_file_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let db = sqlite_file(dir.path());
        let mut wrong = manifest(&db);
        wrong.db_sha256 = "00".repeat(32);
        let out = dir.path().join("x.dzpos");
        export_backup(&db, &wrong, &out).unwrap();
        let backups = dir.path().join("backups");
        std::fs::create_dir(&backups).unwrap();
        assert_eq!(
            import_backup(&out, &backups, 0).unwrap_err().code(),
            "backup_file_invalid"
        );
        let text = dir.path().join("notes.dzpos");
        std::fs::File::create(&text)
            .unwrap()
            .write_all(b"not a zip")
            .unwrap();
        assert_eq!(
            import_backup(&text, &backups, 0).unwrap_err().code(),
            "backup_file_invalid"
        );
        assert_eq!(
            std::fs::read_dir(&backups).unwrap().count(),
            0,
            "nothing left behind"
        );
    }

    #[test]
    fn the_full_export_has_one_csv_per_table_a_manifest_and_a_readme() {
        let dir = tempfile::tempdir().unwrap();
        let db = pos_core::db::Db::open_memory().unwrap();
        let out = dir.path().join("donnees.zip");
        let info = ExportInfo {
            app_version: "0.2.0".into(),
            created_at: 1_790_593_200_000,
            company_name: "Supérette El Baraka".into(),
            nif: None,
        };
        let done = db
            .read(move |c| full_export(c, Lang::Fr, &info, &out))
            .unwrap();
        assert_eq!(done.tables, pos_core::dataset::DATASET.len());
        let mut zip =
            zip::ZipArchive::new(std::fs::File::open(dir.path().join("donnees.zip")).unwrap())
                .unwrap();
        assert!(zip.by_name("manifest.json").is_ok());
        assert!(zip.by_name("LISEZ-MOI.txt").is_ok());
        let mut users = String::new();
        std::io::Read::read_to_string(&mut zip.by_name("users.csv").unwrap(), &mut users).unwrap();
        assert!(
            users.starts_with("\u{feff}id;username;display_name"),
            "{users}"
        );
        assert!(!users.contains("pin_hash"));
    }
}

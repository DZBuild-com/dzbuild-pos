//! The diagnostic zip (PLT-019): what support needs to understand a problem, and nothing
//! personal. Every text passes `sanitize` (the shop's customers and suppliers, then anything
//! that looks personal, then the Windows user folder); no database copy, no crash dump.

use crate::locales::text;
use pos_core::error::AppError;
use pos_core::redact::{DenyList, mask_personal};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

/// Only the end of each log file travels: 2 MiB is days of an ordinary shop.
pub const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;

pub struct DiagnosticInput<'a> {
    pub system: serde_json::Value,
    pub snapshot: serde_json::Value,
    pub hardware_json: Option<String>,
    pub local_json: Option<String>,
    /// Newest first; the shell passes at most seven.
    pub logs: Vec<PathBuf>,
    pub crash_log: Option<PathBuf>,
    pub deny: &'a DenyList,
    /// `C:\Users\<name>`: replaced by `C:\Users\<user>` everywhere.
    pub user_dir: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BundleSummary {
    pub entries: Vec<String>,
    pub bytes: u64,
}

pub fn sanitize(text: &str, deny: &DenyList, user_dir: Option<&str>) -> String {
    let mut out = mask_personal(&deny.mask(text));
    if let Some(dir) = user_dir.filter(|d| !d.is_empty()) {
        // As typed, as JSON escapes it, and with forward slashes; each keeps its own escaping.
        let forms = [
            (dir.replace('\\', "\\\\"), "C:\\\\Users\\\\<user>"),
            (dir.to_owned(), "C:\\Users\\<user>"),
            (dir.replace('\\', "/"), "C:/Users/<user>"),
        ];
        for (form, replacement) in forms {
            out = out.replace(&form, replacement);
        }
    }
    // The database file name tells nothing, but support must never be asked for it by name.
    out.replace("pos.db", "<database>")
}

fn io(e: impl std::fmt::Display) -> AppError {
    AppError::Internal(format!("diagnostic: {e}"))
}

/// The last `MAX_LOG_BYTES` of a text file, starting at a line.
fn tail(path: &Path) -> Result<String, AppError> {
    let mut file = std::fs::File::open(path).map_err(io)?;
    let len = file.metadata().map_err(io)?.len();
    let start = len.saturating_sub(MAX_LOG_BYTES);
    file.seek(SeekFrom::Start(start)).map_err(io)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(io)?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    if start == 0 {
        return Ok(text);
    }
    Ok(match text.find('\n') {
        Some(cut) => text[cut + 1..].to_owned(),
        None => String::new(),
    })
}

fn readme() -> String {
    let mut parts = Vec::new();
    for lang in ["fr", "ar", "en"] {
        for key in ["title", "contains", "excludes", "send"] {
            if let Some(line) = text(lang, &format!("diagnostics.readme.{key}")) {
                parts.push(line);
            }
        }
        parts.push(String::new());
    }
    parts.join("\r\n")
}

pub fn write_bundle(input: &DiagnosticInput<'_>, out: &Path) -> Result<BundleSummary, AppError> {
    let user_dir = input.user_dir.as_deref();
    let clean = |text: &str| sanitize(text, input.deny, user_dir);
    let mut files: Vec<(String, String)> = vec![
        ("LISEZ-MOI.txt".into(), readme()),
        (
            "system.json".into(),
            clean(&serde_json::to_string_pretty(&input.system)?),
        ),
        (
            "snapshot.json".into(),
            clean(&serde_json::to_string_pretty(&input.snapshot)?),
        ),
    ];
    if let Some(hardware) = &input.hardware_json {
        files.push(("hardware.json".into(), clean(hardware)));
    }
    if let Some(local) = &input.local_json {
        files.push(("local.json".into(), clean(local)));
    }
    for log in &input.logs {
        let Some(name) = log.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        files.push((format!("logs/{name}"), clean(&tail(log)?)));
    }
    if let Some(crash) = input.crash_log.as_ref().filter(|p| p.exists()) {
        files.push(("logs/crash.log".into(), clean(&tail(crash)?)));
    }
    let tmp = out.with_extension("zip.tmp");
    let written = (|| -> Result<(), AppError> {
        let mut zip = ZipWriter::new(std::fs::File::create(&tmp).map_err(io)?);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, content) in &files {
            zip.start_file(name.as_str(), options).map_err(io)?;
            zip.write_all(content.as_bytes()).map_err(io)?;
        }
        zip.finish().map_err(io)?.sync_all().map_err(io)?;
        std::fs::rename(&tmp, out).map_err(io)
    })();
    if let Err(error) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(error);
    }
    Ok(BundleSummary {
        entries: files.into_iter().map(|(name, _)| name).collect(),
        bytes: std::fs::metadata(out).map_err(io)?.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_core::audit::Actor;
    use pos_core::customers::{self, CustomerInput};
    use pos_core::db::Db;
    use pos_core::redact::DenyList;
    use serde_json::json;
    use std::io::Read;

    fn entries(zip: &Path) -> Vec<(String, String)> {
        let mut archive = zip::ZipArchive::new(std::fs::File::open(zip).unwrap()).unwrap();
        (0..archive.len())
            .map(|i| {
                let mut entry = archive.by_index(i).unwrap();
                let mut text = String::new();
                entry.read_to_string(&mut text).unwrap();
                (entry.name().to_owned(), text)
            })
            .collect()
    }

    #[test]
    fn the_bundle_holds_no_customer_data_no_pin_hash_and_no_token() {
        let db = Db::open_memory().unwrap();
        pos_core::setup::bootstrap(&db).unwrap();
        let pack = std::sync::Arc::new(pos_core::datapack::bundled().unwrap());
        db.write(move |tx| {
            let mut input = CustomerInput::named("Karim Benali");
            input.phone = Some("0550123456".into());
            input.email = Some("karim@example.dz".into());
            customers::create(tx, &Actor::system(None), &pack, input)?;
            Ok(())
        })
        .unwrap();
        let deny = db.read(DenyList::from_db).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("dzbuild-pos.2026-09-28.log");
        std::fs::write(
            &log,
            concat!(
                "INFO sale customer=\"Karim Benali\" phone=0550 12 34 56\n",
                "WARN owner hash $argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA\n",
                "DEBUG Authorization: Bearer eyJhbGciOi.payload.sig {\"refresh_token\":\"rt-123456\"}\n",
                "INFO backup to C:\\Users\\amine\\AppData\\Roaming\\dev.dzbuild.pos\\backups done\n",
                "INFO mail karim@example.dz\n",
            ),
        )
        .unwrap();
        let crash = dir.path().join("crash.log");
        std::fs::write(
            &crash,
            "[2026-09-28 Algiers] panic in thread 'main': customer 0550123456\n",
        )
        .unwrap();
        let out = dir.path().join("diagnostic.zip");
        let summary = write_bundle(
            &DiagnosticInput {
                system: json!({ "os": "Windows 10 22H2", "dataDir": "C:\\Users\\amine\\AppData\\Roaming\\dev.dzbuild.pos" }),
                snapshot: db.read(pos_core::health::snapshot).unwrap(),
                hardware_json: Some("{\"printers\":[{\"name\":\"XP-80\"}]}".into()),
                local_json: Some(
                    "{\"backupMirror\":{\"path\":\"C:\\\\Users\\\\amine\\\\Desktop\\\\copies\"}}"
                        .into(),
                ),
                logs: vec![log],
                crash_log: Some(crash),
                deny: &deny,
                user_dir: Some("C:\\Users\\amine".into()),
            },
            &out,
        )
        .unwrap();
        let files = entries(&out);
        let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            [
                "LISEZ-MOI.txt",
                "system.json",
                "snapshot.json",
                "hardware.json",
                "local.json",
                "logs/dzbuild-pos.2026-09-28.log",
                "logs/crash.log",
            ]
        );
        assert_eq!(summary.entries.len(), 7);
        for (name, text) in &files {
            for leaked in [
                "Karim",
                "0550123456",
                "0550 12 34 56",
                "karim@example.dz",
                "$argon2",
                "eyJhbGciOi",
                "rt-123456",
                "amine",
                "pos.db",
            ] {
                assert!(!text.contains(leaked), "{leaked} leaked in {name}:\n{text}");
            }
        }
        let readme = &files[0].1;
        assert!(
            readme.contains("Diagnostic") && readme.contains("التشخيص"),
            "{readme}"
        );
        assert!(!dir.path().join("diagnostic.zip.tmp").exists());
    }

    #[test]
    fn only_the_end_of_a_long_log_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("dzbuild-pos.log");
        let line = "INFO a line of the log\n";
        std::fs::write(
            &log,
            line.repeat((MAX_LOG_BYTES as usize / line.len()) + 1_000),
        )
        .unwrap();
        std::fs::write(dir.path().join("marker"), b"").unwrap();
        let out = dir.path().join("d.zip");
        let deny = DenyList::empty();
        write_bundle(
            &DiagnosticInput {
                system: json!({}),
                snapshot: json!({}),
                hardware_json: None,
                local_json: None,
                logs: vec![log],
                crash_log: None,
                deny: &deny,
                user_dir: None,
            },
            &out,
        )
        .unwrap();
        let files = entries(&out);
        let (_, text) = files.iter().find(|(n, _)| n.starts_with("logs/")).unwrap();
        assert!(text.len() as u64 <= MAX_LOG_BYTES);
        assert!(text.starts_with("INFO a line"), "cut at a line start");
    }
}

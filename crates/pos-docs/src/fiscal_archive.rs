//! The fiscal archive file: a zip of CSV parts (RFC 4180, UTF-8, `,`, a header row) streamed
//! through a SHA-256 hasher, `schema.sql`, `chains.json`, a trilingual `LISEZ-MOI.txt`, the
//! summary PDF and `manifest.json` (the SHA-256 of every file) last. `verify` recomputes every
//! hash, for the shop, its accountant or the tax office.

use crate::locales;
use pos_core::customers::receipts::ReceiptChainReport;
use pos_core::error::AppError;
use pos_core::fiscal::archive::{ArchivePart, ChainSummary};
use pos_core::lang::Lang;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

pub const ARCHIVE_FORMAT: &str = "dzbuild-pos-fiscal-archive/1";

const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;

fn io(e: impl std::fmt::Display) -> AppError {
    AppError::Io(e.to_string())
}

fn not_an_archive() -> AppError {
    AppError::conflict("backup_file_invalid")
}

/// One file of the archive as the manifest lists it; `rows` for the CSV parts (header
/// excluded), `bytes` and `sha256` over the uncompressed content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ManifestFile {
    pub name: String,
    pub sha256: String,
    pub bytes: u64,
    pub rows: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveCompany {
    pub name: String,
    pub nif: Option<String>,
    pub nis: Option<String>,
    pub rc: Option<String>,
}

/// `manifest.json`, the last file of the archive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveManifest {
    pub format: String,
    pub app_version: String,
    pub created_at: String,
    pub company: ArchiveCompany,
    pub from: String,
    pub to: String,
    pub files: Vec<ManifestFile>,
    #[specta(type = pos_core::json::JsonValue)]
    pub chains: serde_json::Value,
    pub algorithms: BTreeMap<String, String>,
}

/// What the caller knows of the export; the writer adds the files and the algorithms.
pub struct ManifestBase {
    pub app_version: String,
    pub created_at: String,
    pub company: ArchiveCompany,
    pub from: String,
    pub to: String,
    pub chains: serde_json::Value,
}

/// A writer that hashes and counts what goes through it.
struct Hashing<W: Write> {
    inner: W,
    hasher: Sha256,
    bytes: u64,
}

impl<W: Write> Write for Hashing<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.hasher.update(&buf[..n]);
        self.bytes += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// The rows of one CSV part, written straight into the zip entry.
pub struct CsvSink<'a> {
    writer: csv::Writer<Hashing<&'a mut ZipWriter<File>>>,
}

impl CsvSink<'_> {
    pub fn row(&mut self, cells: &[String]) -> Result<(), AppError> {
        self.writer.write_record(cells).map_err(io)
    }
}

/// The archive being written, into a sibling of the chosen file (`<name>.partial`): the chosen
/// file — an earlier archive of the period, anchored in the journal — is only replaced once the
/// new one is whole ([`FinishedArchive::persist`]). Dropped unfinished, the partial file goes.
pub struct ArchiveWriter {
    zip: Option<ZipWriter<File>>,
    files: Vec<ManifestFile>,
    partial: PathBuf,
    path: PathBuf,
}

impl Drop for ArchiveWriter {
    fn drop(&mut self) {
        if let Some(zip) = self.zip.take() {
            drop(zip);
            // Best effort: a partial file left behind never replaces the chosen one.
            let _ = std::fs::remove_file(&self.partial);
        }
    }
}

/// A whole archive on the disk, still beside the chosen file. [`FinishedArchive::persist`] puts
/// it in place; dropped without, it is removed and the chosen file is left as it was.
#[must_use]
pub struct FinishedArchive {
    partial: PathBuf,
    path: PathBuf,
    persisted: bool,
}

impl FinishedArchive {
    /// Renames the archive over the chosen file (one step on the same volume).
    pub fn persist(mut self) -> Result<(), AppError> {
        std::fs::rename(&self.partial, &self.path).map_err(io)?;
        self.persisted = true;
        Ok(())
    }
}

impl Drop for FinishedArchive {
    fn drop(&mut self) {
        if !self.persisted {
            let _ = std::fs::remove_file(&self.partial);
        }
    }
}

fn not_writing() -> AppError {
    AppError::Internal("fiscal archive: already finished".into())
}

/// Deflated; Zip64 when an entry passes 4 GiB (five years of a busy shop's journal).
fn options() -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .large_file(true)
}

impl ArchiveWriter {
    pub fn create(path: &Path) -> Result<ArchiveWriter, AppError> {
        let mut name = path.file_name().unwrap_or_default().to_os_string();
        name.push(".partial");
        let partial = path.with_file_name(name);
        Ok(ArchiveWriter {
            zip: Some(ZipWriter::new(File::create(&partial).map_err(io)?)),
            files: Vec::new(),
            partial,
            path: path.to_path_buf(),
        })
    }

    /// Adds `name` as a CSV (RFC 4180 quoting, UTF-8, `,`, `\n`): `header` first, then the rows
    /// `fill` writes; `fill` returns how many it wrote.
    pub fn add_csv(
        &mut self,
        name: &str,
        header: &[String],
        fill: impl FnOnce(&mut CsvSink<'_>) -> Result<i64, AppError>,
    ) -> Result<(), AppError> {
        let zip = self.zip.as_mut().ok_or_else(not_writing)?;
        zip.start_file(name, options()).map_err(io)?;
        let hashing = Hashing {
            inner: zip,
            hasher: Sha256::new(),
            bytes: 0,
        };
        let mut sink = CsvSink {
            writer: csv::WriterBuilder::new()
                .terminator(csv::Terminator::Any(b'\n'))
                .from_writer(hashing),
        };
        sink.row(header)?;
        let rows = fill(&mut sink)?;
        let hashing = sink.writer.into_inner().map_err(io)?;
        self.files.push(ManifestFile {
            name: name.to_owned(),
            sha256: hex::encode(hashing.hasher.finalize().as_slice()),
            bytes: hashing.bytes,
            rows: Some(rows),
        });
        Ok(())
    }

    pub fn add_bytes(&mut self, name: &str, bytes: &[u8]) -> Result<(), AppError> {
        let zip = self.zip.as_mut().ok_or_else(not_writing)?;
        zip.start_file(name, options()).map_err(io)?;
        zip.write_all(bytes).map_err(io)?;
        self.files.push(ManifestFile {
            name: name.to_owned(),
            sha256: hex::encode(Sha256::digest(bytes).as_slice()),
            bytes: bytes.len() as u64,
            rows: None,
        });
        Ok(())
    }

    /// Adds the trilingual read-me and the manifest, closes the zip and flushes it to the disk
    /// before the journal anchors it; returns the manifest, the SHA-256 of `manifest.json` (what
    /// the journal anchors) and the archive still to put in place.
    pub fn finish(
        mut self,
        base: ManifestBase,
    ) -> Result<(ArchiveManifest, String, FinishedArchive), AppError> {
        self.add_bytes("LISEZ-MOI.txt", readme().as_bytes())?;
        let manifest = ArchiveManifest {
            format: ARCHIVE_FORMAT.to_owned(),
            app_version: base.app_version,
            created_at: base.created_at,
            company: base.company,
            from: base.from,
            to: base.to,
            files: self.files.clone(),
            chains: base.chains,
            algorithms: algorithms(),
        };
        let text = serde_json::to_vec_pretty(&manifest)?;
        let zip = self.zip.as_mut().ok_or_else(not_writing)?;
        zip.start_file("manifest.json", options()).map_err(io)?;
        zip.write_all(&text).map_err(io)?;
        let zip = self.zip.take().ok_or_else(not_writing)?;
        // From here the partial file is the finished archive's to keep or remove.
        let finished = FinishedArchive {
            partial: self.partial.clone(),
            path: self.path.clone(),
            persisted: false,
        };
        zip.finish().map_err(io)?.sync_all().map_err(io)?;
        Ok((
            manifest,
            hex::encode(Sha256::digest(&text).as_slice()),
            finished,
        ))
    }
}

/// The formulas an auditor needs to recompute every hash (spec §5.3).
pub fn algorithms() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("auditLog".into(), "hash = hex(SHA-256(prev_hash ␟ seq ␟ at ␟ user_id ␟ terminal_id ␟ action ␟ entity_type ␟ entity_id ␟ payload)), ␟ = U+001F, NULL = empty, first prev_hash = 64 × '0'".into()),
        ("documents".into(), "hash = hex(SHA-256(prev_hash ␟ canonical_json(document))), one chain per (doc_type, series) linked by chain_seq; canonical_json = keys sorted, no whitespace; M3 fields under 'ext' only when present".into()),
        ("cashClosures".into(), "hash = hex(SHA-256(prev_hash ␟ canonical_json(z_report))), one chain per terminal, perpetual grand total".into()),
        ("receipts".into(), "receipts.csv (RV versement receipts) carry no hash of their own: each one equals the audit_log row credit.collected (a till's in lan_journal) whose payload has its receiptNumber, entryId (= id), customer (= customer_id), amount (= -amount), method, stampDuty, stampCharged and balanceAfter; numbers issued per (counter, series, fiscal_year) = counters.last_number minus the numbers of counter_skips.csv journaled by backup.restored or lan.promoted (a till made the main PC)".into()),
        ("lanJournal".into(), "lan_journal.csv: each till row keeps its own hash (as auditLog); merge_hash = hex(SHA-256(merge_prev ␟ terminal_id ␟ seq ␟ hash ␟ received_at)) chains every till's rows in arrival order".into()),
        ("manifest".into(), "sha256 = hex(SHA-256(file bytes)) for every file; the SHA-256 of manifest.json is written in audit_log as fiscal.archive_exported".into()),
    ])
}

/// `LISEZ-MOI.txt`: the same text in Arabic, French and English.
pub fn readme() -> String {
    [Lang::Ar, Lang::Fr, Lang::En]
        .iter()
        .map(|l| {
            let t = |k: &str| {
                locales::text(l.as_str(), &format!("fiscalArchive.readme.{k}")).unwrap_or_default()
            };
            [
                t("title"),
                t("content"),
                t("files"),
                t("hashes"),
                t("verify"),
            ]
            .join("\n\n")
        })
        .collect::<Vec<_>>()
        .join("\n\n----------------------------------------\n\n")
}

/// What [`verify`] found: the SHA-256 of the archive's `manifest.json` (to find in the journal's
/// `fiscal.archive_exported` rows) and the files whose content no longer matches the manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveCheck {
    pub manifest_sha256: String,
    pub bad_files: Vec<String>,
}

/// The SHA-256 of a zip entry, read in 64 KiB blocks (a part can weigh gigabytes).
fn entry_sha256(entry: &mut impl Read) -> std::io::Result<String> {
    let (mut hasher, mut buf) = (Sha256::new(), vec![0_u8; 64 * 1024]);
    loop {
        let n = entry.read(&mut buf)?;
        if n == 0 {
            return Ok(hex::encode(hasher.finalize().as_slice()));
        }
        hasher.update(&buf[..n]);
    }
}

/// Recomputes the hash of every file listed in the manifest (`backup_file_invalid` when the file
/// is not a zip with a manifest).
pub fn verify(path: &Path) -> Result<ArchiveCheck, AppError> {
    let mut zip =
        zip::ZipArchive::new(File::open(path).map_err(io)?).map_err(|_| not_an_archive())?;
    let mut text = Vec::new();
    // A manifest is a few kilobytes: a file chosen by the user never gets to fill the memory.
    zip.by_name("manifest.json")
        .map_err(|_| not_an_archive())?
        .take(MAX_MANIFEST_BYTES)
        .read_to_end(&mut text)
        .map_err(|_| not_an_archive())?;
    let manifest: ArchiveManifest = serde_json::from_slice(&text).map_err(|_| not_an_archive())?;
    let mut bad = Vec::new();
    for f in &manifest.files {
        let ok = match zip.by_name(&f.name) {
            Ok(mut entry) => entry_sha256(&mut entry).is_ok_and(|sha| sha == f.sha256),
            Err(_) => false,
        };
        if !ok {
            bad.push(f.name.clone());
        }
    }
    Ok(ArchiveCheck {
        manifest_sha256: hex::encode(Sha256::digest(&text).as_slice()),
        bad_files: bad,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SummaryPart {
    pub label: String,
    pub rows: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SummaryChain {
    pub label: String,
    pub series: String,
    pub rows: i64,
    pub intact: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SummaryLabels {
    pub period: String,
    pub created_at: String,
    pub parts: String,
    pub rows: String,
    pub chains: String,
    pub series: String,
    pub intact: String,
    pub broken: String,
    pub audit: String,
    pub last_hash: String,
    /// The company's identifiers (`company.nif`, `nis`, `rc`).
    pub nif: String,
    pub nis: String,
    pub rc: String,
}

/// The print model of `summary.pdf`, every word in `lang`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveSummary {
    pub lang: Lang,
    pub rtl: bool,
    pub title: String,
    pub company: ArchiveCompany,
    pub period: String,
    pub created_at: String,
    pub parts: Vec<SummaryPart>,
    pub chains: Vec<SummaryChain>,
    pub audit_rows: i64,
    pub audit_intact: bool,
    pub audit_last_hash: String,
    pub labels: SummaryLabels,
}

/// The versement receipts (`RV…`) as one more company-wide chain, once the shop has any.
fn receipts_chain(receipts: &ReceiptChainReport, lang: Lang) -> Option<SummaryChain> {
    let intact = receipts.first_broken_number.is_none();
    (receipts.rows > 0 || !intact).then(|| SummaryChain {
        label: crate::a4::labels::word(lang, "settlement.title"),
        series: String::new(),
        rows: receipts.rows,
        intact,
    })
}

/// The journals of the tills this main PC keeps (M5), then its chain over all of them.
fn till_chains(chains: &ChainSummary, lang: Lang) -> Vec<SummaryChain> {
    if chains.till_journals.is_empty() {
        return Vec::new();
    }
    let w = |k: &str| {
        locales::text(lang.as_str(), &format!("fiscalArchive.summary.{k}")).unwrap_or_default()
    };
    let mut out: Vec<SummaryChain> = chains
        .till_journals
        .iter()
        .map(|t| SummaryChain {
            label: w("tillJournal"),
            series: t.terminal_code.clone(),
            rows: t.report.rows,
            intact: t.report.broken == 0,
        })
        .collect();
    out.push(SummaryChain {
        label: w("tillMerge"),
        series: String::new(),
        rows: chains.till_journals.iter().map(|t| t.report.rows).sum(),
        intact: chains.till_merge_first_broken.is_none(),
    });
    out
}

pub fn build_summary(
    chains: &ChainSummary,
    parts: &[(ArchivePart, i64)],
    company: ArchiveCompany,
    from: &str,
    to: &str,
    lang: Lang,
    now: i64,
) -> ArchiveSummary {
    let w = |k: &str| {
        locales::text(lang.as_str(), &format!("fiscalArchive.summary.{k}")).unwrap_or_default()
    };
    let day = |d: &str| crate::a4::document::day(d);
    ArchiveSummary {
        lang,
        rtl: lang.is_rtl(),
        title: w("title"),
        company,
        period: format!("{} – {}", day(from), day(to)),
        created_at: pos_core::time::algiers_datetime(now)
            .format("%d/%m/%Y %H:%M")
            .to_string(),
        parts: parts
            .iter()
            .map(|(p, rows)| SummaryPart {
                label: p.file_name().to_owned(),
                rows: *rows,
            })
            .collect(),
        chains: chains
            .documents
            .iter()
            .map(|c| SummaryChain {
                label: crate::a4::labels::title(lang, c.doc_type),
                series: c.series.clone(),
                rows: c.rows,
                intact: c.first_broken_seq.is_none(),
            })
            .chain(receipts_chain(&chains.receipts, lang))
            .chain(till_chains(chains, lang))
            .collect(),
        audit_rows: chains.audit.rows,
        audit_intact: chains.audit.first_broken_seq.is_none(),
        audit_last_hash: chains.audit.last_hash.clone(),
        labels: SummaryLabels {
            period: w("period"),
            created_at: w("createdAt"),
            parts: w("parts"),
            rows: w("rows"),
            chains: w("chains"),
            series: w("series"),
            intact: w("intact"),
            broken: w("broken"),
            audit: w("audit"),
            last_hash: w("lastHash"),
            nif: crate::a4::labels::word(lang, "a4.nif"),
            nis: crate::a4::labels::word(lang, "a4.nis"),
            rc: crate::a4::labels::word(lang, "a4.rc"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    fn base() -> ManifestBase {
        ManifestBase {
            app_version: "0.3.0".into(),
            created_at: "2026-09-28T10:15:00+01:00".into(),
            company: ArchiveCompany {
                name: "Supérette El Amel".into(),
                nif: Some("000116001234567".into()),
                nis: None,
                rc: None,
            },
            from: "2026-01-01".into(),
            to: "2026-12-31".into(),
            chains: serde_json::json!({ "documents": [] }),
        }
    }

    fn write(path: &std::path::Path) -> (ArchiveManifest, String) {
        let mut w = ArchiveWriter::create(path).unwrap();
        w.add_csv("documents.csv", &["id".into(), "number".into()], |sink| {
            sink.row(&["d1".into(), "FA2026-000045".into()])?;
            sink.row(&["d2".into(), "FA2026-000046".into()])?;
            Ok(2)
        })
        .unwrap();
        w.add_bytes("schema.sql", b"CREATE TABLE documents (id TEXT);")
            .unwrap();
        let (manifest, sha, finished) = w.finish(base()).unwrap();
        finished.persist().unwrap();
        (manifest, sha)
    }

    #[test]
    fn a_failed_export_never_touches_the_archive_already_there() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("archive.zip");
        let (_, sha) = write(&path);
        let partial = dir.path().join("archive.zip.partial");
        // Failed while writing its parts.
        let mut w = ArchiveWriter::create(&path).unwrap();
        w.add_bytes("schema.sql", b"CREATE TABLE x (id TEXT);")
            .unwrap();
        assert!(
            partial.exists(),
            "the new archive is written beside the old one"
        );
        drop(w);
        // Finished, but failed before it was put in place (the journal refused it).
        let w = ArchiveWriter::create(&path).unwrap();
        let (_, other, finished) = w.finish(base()).unwrap();
        assert_ne!(other, sha);
        drop(finished);
        assert!(!partial.exists(), "nothing unfinished is left behind");
        assert_eq!(
            verify(&path).unwrap(),
            ArchiveCheck {
                manifest_sha256: sha,
                bad_files: Vec::new()
            },
            "the earlier archive is still there, whole"
        );
        let (_, again) = write(&path);
        assert_eq!(
            verify(&path).unwrap().manifest_sha256,
            again,
            "replaced once whole"
        );
        assert!(!partial.exists());
    }

    #[test]
    fn the_archive_manifest_hashes_every_file_and_is_anchored_in_the_journal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("archive.zip");
        let (manifest, sha) = write(&path);
        assert_eq!(manifest.format, ARCHIVE_FORMAT);
        assert_eq!(
            manifest
                .files
                .iter()
                .map(|f| (f.name.as_str(), f.rows))
                .collect::<Vec<_>>(),
            [
                ("documents.csv", Some(2)),
                ("schema.sql", None),
                ("LISEZ-MOI.txt", None)
            ]
        );
        let check = verify(&path).unwrap();
        assert_eq!((check.manifest_sha256, check.bad_files.len()), (sha, 0));
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let mut csv = String::new();
        zip.by_name("documents.csv")
            .unwrap()
            .read_to_string(&mut csv)
            .unwrap();
        assert_eq!(csv, "id,number\nd1,FA2026-000045\nd2,FA2026-000046\n");
        let mut readme = String::new();
        zip.by_name("LISEZ-MOI.txt")
            .unwrap()
            .read_to_string(&mut readme)
            .unwrap();
        assert!(
            readme.contains("SHA-256") && readme.contains("الأرشيف") && readme.contains("archive")
        );
    }

    #[test]
    fn a_tampered_file_no_longer_matches_its_manifest_hash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("archive.zip");
        write(&path);
        let tampered = dir.path().join("tampered.zip");
        let mut src = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let mut out = zip::ZipWriter::new(std::fs::File::create(&tampered).unwrap());
        for i in 0..src.len() {
            let mut f = src.by_index(i).unwrap();
            let name = f.name().to_owned();
            let mut bytes = Vec::new();
            f.read_to_end(&mut bytes).unwrap();
            if name == "documents.csv" {
                bytes = String::from_utf8(bytes)
                    .unwrap()
                    .replace("FA2026-000046", "FA2026-000047")
                    .into_bytes();
            }
            out.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            out.write_all(&bytes).unwrap();
        }
        out.finish().unwrap();
        assert_eq!(verify(&tampered).unwrap().bad_files, ["documents.csv"]);
    }

    #[test]
    fn a_file_that_is_not_an_archive_is_refused_and_the_writer_travels_to_the_reader() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.zip");
        std::fs::write(&path, b"not a zip").unwrap();
        assert_eq!(verify(&path).unwrap_err().code(), "backup_file_invalid");
        // The command moves the writer into the database reader and back, part by part.
        fn send<T: Send>() {}
        send::<ArchiveWriter>();
    }

    #[test]
    fn the_summary_speaks_the_language_of_the_page() {
        use pos_core::audit::{ChainReport, GENESIS_HASH};
        use pos_core::documents::{DocChainReport, DocType};
        let report = |doc_type, series: &str, broken: Option<i64>| DocChainReport {
            doc_type,
            series: series.into(),
            rows: 3,
            first_broken_seq: broken,
            first_broken_number: None,
            last_seq: 3,
            last_hash: "a".repeat(64),
        };
        let chains = ChainSummary {
            documents: vec![
                report(DocType::Invoice, "", None),
                report(DocType::Ticket, "C1", Some(2)),
            ],
            audit: ChainReport {
                rows: 12,
                first_broken_seq: None,
                last_seq: 12,
                last_hash: GENESIS_HASH.into(),
            },
            closures: Vec::new(),
            receipts: Default::default(),
            till_journals: Vec::new(),
            till_merge_first_broken: None,
        };
        let company = base().company;
        let parts = [(ArchivePart::Documents, 5), (ArchivePart::AuditLog, 12)];
        // 2026-09-28 12:00 in Algiers.
        let ar = build_summary(
            &chains,
            &parts,
            company.clone(),
            "2026-01-01",
            "2026-12-31",
            Lang::Ar,
            1_790_593_200_000,
        );
        assert!(ar.rtl);
        assert_eq!(ar.title, "الأرشيف الجبائي — ملخص");
        assert_eq!(ar.period, "01/01/2026 – 31/12/2026");
        assert_eq!(ar.created_at, "28/09/2026 12:00");
        assert_eq!(
            ar.parts[1],
            SummaryPart {
                label: "audit_log.csv".into(),
                rows: 12
            }
        );
        assert_eq!(
            ar.chains
                .iter()
                .map(|c| (c.series.as_str(), c.intact))
                .collect::<Vec<_>>(),
            [("", true), ("C1", false)]
        );
        assert_eq!((ar.audit_rows, ar.audit_intact), (12, true));
        let fr = build_summary(
            &chains,
            &parts,
            company,
            "2026-01-01",
            "2026-12-31",
            Lang::Fr,
            0,
        );
        assert!(!fr.rtl);
        assert_eq!(
            (fr.labels.intact.as_str(), fr.labels.broken.as_str()),
            ("Intègre", "Altérée")
        );
        assert_eq!(
            fr.chains[0].label,
            crate::a4::labels::title(Lang::Fr, DocType::Invoice)
        );
    }

    #[test]
    fn the_summary_lists_the_tills_journals_and_their_merge_chain() {
        use pos_core::audit::{ChainReport, GENESIS_HASH};
        use pos_core::fiscal::archive::TillJournalChain;
        use pos_core::sync::journal::JournalReport;
        let journal = |code: &str, broken: i64| TillJournalChain {
            terminal_id: format!("id-{code}"),
            terminal_code: code.into(),
            report: JournalReport {
                rows: 5,
                broken,
                first_broken_seq: (broken > 0).then_some(3),
                last_seq: 5,
                last_hash: GENESIS_HASH.into(),
            },
        };
        let chains = ChainSummary {
            documents: Vec::new(),
            audit: ChainReport {
                rows: 0,
                first_broken_seq: None,
                last_seq: 0,
                last_hash: GENESIS_HASH.into(),
            },
            closures: Vec::new(),
            receipts: ReceiptChainReport::default(),
            till_journals: vec![journal("C2", 0), journal("C3", 1)],
            till_merge_first_broken: Some(7),
        };
        let listed = build_summary(
            &chains,
            &[],
            base().company,
            "2026-01-01",
            "2026-12-31",
            Lang::Fr,
            0,
        )
        .chains;
        assert_eq!(
            listed
                .iter()
                .map(|c| (c.label.as_str(), c.series.as_str(), c.rows, c.intact))
                .collect::<Vec<_>>(),
            [
                ("Journal du poste", "C2", 5, true),
                ("Journal du poste", "C3", 5, false),
                ("Chaîne des journaux des postes", "", 10, false),
            ]
        );
    }

    #[test]
    fn the_summary_lists_the_versement_receipts_once_there_are_some() {
        use pos_core::audit::{ChainReport, GENESIS_HASH};
        let mut chains = ChainSummary {
            documents: Vec::new(),
            audit: ChainReport {
                rows: 0,
                first_broken_seq: None,
                last_seq: 0,
                last_hash: GENESIS_HASH.into(),
            },
            closures: Vec::new(),
            receipts: ReceiptChainReport::default(),
            till_journals: Vec::new(),
            till_merge_first_broken: None,
        };
        let summary = |chains: &ChainSummary, lang| {
            build_summary(
                chains,
                &[],
                base().company,
                "2026-01-01",
                "2026-12-31",
                lang,
                0,
            )
            .chains
        };
        assert_eq!(summary(&chains, Lang::Fr), [], "a shop without versements");
        chains.receipts = ReceiptChainReport {
            rows: 4,
            first_broken_number: Some("RV2026-000003".into()),
        };
        assert_eq!(
            summary(&chains, Lang::Fr),
            [SummaryChain {
                label: "Reçu de versement".into(),
                series: String::new(),
                rows: 4,
                intact: false,
            }]
        );
        assert_eq!(summary(&chains, Lang::Ar)[0].label, "وصل دفع");
    }
}

//! Joining a main PC: pair with the code, send what this PC still owes as a till, download and
//! check the copy, stage it for the next start. Runs on a thread of its own (no engine exists
//! yet, and Tauri's runtime must not block).

use super::api::{HostApi, normalize_address};
use crate::host::routes::blocking;
use crate::wire::HealthInfo;
use pos_core::db::Db;
use pos_core::error::AppError;
use pos_core::sync::devices::PairRequest;
use pos_core::sync::outbox::{self, ItemResult, ItemStatus, WireItem};
use pos_core::sync::replica::{self, LAN_DIR, PendingReplica};
use pos_core::time::now_ms;
use serde_json::json;
use std::future::Future;
use std::path::Path;
use std::sync::Arc;

/// Items per replay request when a former till sends its kept outbox.
const REPLAY_BATCH: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinRequest {
    pub address: String,
    pub code: String,
    pub terminal_code: String,
    pub terminal_name: String,
    pub app_version: String,
    pub schema_version: i64,
    pub install_id: String,
    /// The terminal this PC was before its damaged copy was set aside (`local.json`): sent so
    /// the main PC lets it take over its own terminal and series (`devices::register`). A PC
    /// still holding its till's copy proves it from that copy instead.
    pub former_terminal: Option<String>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct Joined {
    pub pending: PendingReplica,
    pub token: String,
}

impl std::fmt::Debug for Joined {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Joined")
            .field("pending", &self.pending)
            .field("token", &"<hidden>")
            .finish()
    }
}

/// Runs `future` on a fresh thread with its own current-thread runtime and waits for it.
pub(crate) fn oneshot<T, F>(future: F) -> Result<T, AppError>
where
    F: Future<Output = Result<T, AppError>> + Send + 'static,
    T: Send + 'static,
{
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(AppError::from)?
            .block_on(future)
    })
    .join()
    .map_err(|_| AppError::Internal("the LAN task panicked".into()))?
}

/// Who answers at `address` (the wizard shows the shop's name before asking for the code).
pub fn probe(address: &str) -> Result<HealthInfo, AppError> {
    let base = normalize_address(address)?;
    oneshot(async move {
        let api = HostApi::new(&base, None, env!("CARGO_PKG_VERSION"), 0)?;
        api.health().await.map(|(info, _)| info)
    })
}

/// How many items of `batch` the main PC took, in order (at least one). A refused item fails the
/// join. A short answer means the main PC stopped at an item (a hole in the kept items, an
/// error): the caller resends from that item, so the main PC answers with its error.
fn answered(batch: &[WireItem], results: &[ItemResult]) -> Result<usize, AppError> {
    if results.is_empty() || results.len() > batch.len() {
        return Err(AppError::conflict("replay_out_of_order"));
    }
    for (item, result) in batch.iter().zip(results) {
        if result.local_seq != item.local_seq {
            return Err(AppError::conflict("replay_out_of_order"));
        }
        if result.status == ItemStatus::Rejected {
            return Err(AppError::conflict(
                result.code.as_deref().unwrap_or("replay_invalid"),
            ));
        }
    }
    Ok(results.len())
}

/// Pairs with `request.code`; a PC that already was a till (`existing`, whose line must not be
/// stopped at a refused item: `lan_outbox_not_empty`) then sends its pending items and those
/// sent in the last 30 days (the main PC skips what it has) and goes on only once every one of
/// them is answered; the copy is downloaded, checked against its hash and staged under
/// `data_dir` for the next start.
pub fn join(
    existing: Option<Db>,
    data_dir: &Path,
    request: JoinRequest,
    progress: Arc<dyn Fn(u64, Option<u64>) + Send + Sync>,
) -> Result<Joined, AppError> {
    let base = normalize_address(&request.address)?;
    let data_dir = data_dir.to_path_buf();
    oneshot(async move {
        // A till joining again proves it is its terminal: only it continues that series.
        let mut own_terminal = request.former_terminal.clone();
        if let Some(db) = existing.clone() {
            // A line stopped at a refused item can never be replayed whole (the items after it
            // wait behind it), and a copy staged without it would reissue its numbers: refused,
            // like leaving the network, before the code is spent.
            let (depth, local) = blocking(move || {
                Ok((db.read(outbox::depth)?, db.read(pos_core::terminal::local)?))
            })
            .await?;
            if depth.rejected > 0 {
                return Err(AppError::rejected(
                    json!({ "pending": depth.pending, "rejected": depth.rejected }),
                    "lan_outbox_not_empty",
                ));
            }
            own_terminal = local.map(|t| t.id);
        }
        let anonymous = HostApi::new(&base, None, &request.app_version, request.schema_version)?;
        let paired = anonymous
            .pair(&PairRequest {
                code: request.code.clone(),
                terminal_code: request.terminal_code.clone(),
                terminal_name: request.terminal_name.clone(),
                app_version: request.app_version.clone(),
                schema_version: request.schema_version,
                install_id: request.install_id.clone(),
                terminal_id: own_terminal,
            })
            .await?;
        let api = HostApi::new(
            &base,
            Some(paired.token.clone()),
            &request.app_version,
            request.schema_version,
        )?;
        if let Some(db) = existing {
            let since = now_ms() - outbox::KEEP_SENT_MS;
            let items = blocking(move || db.read(move |c| outbox::kept_since(c, since))).await?;
            let mut rest = items.as_slice();
            while !rest.is_empty() {
                let batch = &rest[..rest.len().min(REPLAY_BATCH)];
                let results = api.replay(batch.to_vec(), 0, None).await?;
                rest = &rest[answered(batch, &results)?..];
            }
        }
        let dir = data_dir.join(LAN_DIR);
        tokio::fs::create_dir_all(&dir).await?;
        let part = dir.join(replica::DOWNLOAD_PART);
        let report = progress.clone();
        let (seq, sha) = api
            .download(&part, &move |received, total| report(received, total))
            .await?;
        let checked = part.clone();
        blocking(move || pos_core::sync::snapshot::verify_download(&checked, &sha, seq)).await?;
        let pending = PendingReplica {
            seq,
            host_id: paired.host_id.clone(),
            host_name: paired.shop_name.clone(),
            host_address: base.trim_start_matches("http://").to_owned(),
            terminal_id: paired.terminal.id.clone(),
            terminal_code: paired.terminal.code.clone(),
            terminal_name: paired.terminal.name.clone(),
            paired_at: now_ms(),
        };
        replica::stage(&data_dir, &part, &pending)?;
        Ok(Joined {
            pending,
            token: paired.token,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;
    use pos_core::db::{Db, DbOptions};

    #[test]
    fn addresses_are_normalised() {
        assert_eq!(
            normalize_address("192.168.1.10").unwrap(),
            "http://192.168.1.10:7345"
        );
        assert_eq!(
            normalize_address(" 192.168.1.10:8000 ").unwrap(),
            "http://192.168.1.10:8000"
        );
        assert_eq!(
            normalize_address("http://caisse-1.local:7345/").unwrap(),
            "http://caisse-1.local:7345"
        );
        assert_eq!(
            normalize_address("ftp://x").unwrap_err().code(),
            "lan_address_invalid"
        );
        assert_eq!(
            normalize_address("").unwrap_err().code(),
            "lan_address_invalid"
        );
    }

    #[test]
    fn a_new_till_joins_downloads_and_adopts() {
        let dir = tempfile::tempdir().unwrap();
        let host = testing::host(dir.path());
        host.product("Lait", 10_000);
        let health = probe(&host.base()).unwrap();
        assert_eq!(health.shop_name, "Supérette El Amel");
        let code = host
            .desk
            .lock()
            .unwrap()
            .issue(pos_core::time::now_ms())
            .unwrap()
            .code;
        let till_dir = dir.path().join("till");
        let seen = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let progress = seen.clone();
        let joined = join(
            None,
            &till_dir,
            JoinRequest {
                address: host.base(),
                code,
                terminal_code: "C2".into(),
                terminal_name: "Caisse 2".into(),
                app_version: "0.5.0".into(),
                schema_version: pos_core::db::migrations::LATEST_VERSION,
                install_id: pos_core::ids::new_id(),
                former_terminal: None,
            },
            std::sync::Arc::new(move |received, _| {
                progress.store(received, std::sync::atomic::Ordering::SeqCst)
            }),
        )
        .unwrap();
        assert!(
            seen.load(std::sync::atomic::Ordering::SeqCst) > 0,
            "progress reported"
        );
        assert_eq!(joined.pending.terminal_code, "C2");
        assert!(pos_core::sync::replica::has_pending(&till_dir));
        let db_path = till_dir.join("pos.db");
        pos_core::sync::replica::install_pending(&till_dir, &db_path, &till_dir.join("backups"), 0)
            .unwrap();
        let db = Db::open(
            &db_path,
            DbOptions {
                readers: 1,
                backup_dir: Some(till_dir.join("backups")),
            },
        )
        .unwrap();
        pos_core::sync::replica::adopt(&db, &till_dir, &joined.pending, 0).unwrap();
        let n: i64 = db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT COUNT(*) FROM products WHERE name_fr = 'Lait'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(n, 1);
        assert!(!format!("{joined:?}").contains(&joined.token));
    }
}

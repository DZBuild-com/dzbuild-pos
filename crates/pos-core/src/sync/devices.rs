//! The tills paired with this main PC (spec §4.6, LAN-009): each gets a random 256-bit token at
//! pairing, kept only as its SHA-256 here and in Windows Credential Manager on the till; every
//! call but `/health` and `/pair` presents it; the owner revokes a till at once.

use super::feed;
use super::notices::{self, NoticeKind};
use super::pairing::PairingDesk;
use crate::audit::{self, Actor};
use crate::db::Db;
use crate::error::AppError;
use crate::ids::new_id;
use crate::terminal::{self, Terminal, TerminalRole};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fmt;
use std::sync::Mutex;

pub const NAME_MAX: usize = 40;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairRequest {
    pub code: String,
    pub terminal_code: String,
    pub terminal_name: String,
    pub app_version: String,
    pub schema_version: i64,
    pub install_id: String,
    /// The terminal this PC already is, when it was a till (or a main PC) of this shop: its
    /// database's own terminal id. Only that PC may take over an existing terminal and its
    /// numbering series (`register`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_id: Option<String>,
}

impl fmt::Debug for PairRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PairRequest")
            .field("code", &"<hidden>")
            .field("terminal_code", &self.terminal_code)
            .field("terminal_name", &self.terminal_name)
            .field("app_version", &self.app_version)
            .field("schema_version", &self.schema_version)
            .field("terminal_id", &self.terminal_id)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Paired {
    pub terminal: Terminal,
    pub token: String,
}

impl fmt::Debug for Paired {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Paired")
            .field("terminal", &self.terminal)
            .field("token", &"<hidden>")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub terminal_id: String,
    pub code: String,
    pub name: String,
    pub last_replayed_seq: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Touch {
    pub address: Option<String>,
    pub app_version: Option<String>,
    pub schema_version: Option<i64>,
    pub outbox_pending: Option<i64>,
    pub skew_ms: Option<i64>,
}

pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()).as_slice())
}

pub fn new_token() -> Result<(String, String), AppError> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| AppError::Internal(format!("random: {e}")))?;
    let token = hex::encode(bytes);
    let hash = hash_token(&token);
    Ok((token, hash))
}

pub fn valid_code(code: &str) -> bool {
    (1..=8).contains(&code.len())
        && code
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        && code.bytes().next().is_some_and(|b| b.is_ascii_uppercase())
}

pub fn suggest_code(conn: &Connection) -> Result<String, AppError> {
    for n in 2..1_000 {
        let code = format!("C{n}");
        let taken: bool = conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM terminals WHERE code = ?1)",
            [&code],
            |r| r.get(0),
        )?;
        if !taken {
            return Ok(code);
        }
    }
    Err(AppError::conflict("terminal_code_taken"))
}

/// Redeems the code first (a refused request still spends it), then registers the till.
pub fn pair(
    db: &Db,
    desk: &Mutex<PairingDesk>,
    request: PairRequest,
    host_schema: i64,
    actor: &Actor,
    now: i64,
) -> Result<Paired, AppError> {
    {
        let mut desk = desk
            .lock()
            .map_err(|_| AppError::Internal("pairing desk poisoned".into()))?;
        desk.redeem(&request.code, now)?;
    }
    let actor = actor.clone();
    db.write(move |tx| register(tx, &actor, &request, host_schema, now))
}

/// Registers the till of `request`: a new terminal, or an existing one taken over by the PC
/// that is that terminal (`request.terminal_id`: a revoked till pairing again, a till whose
/// token was lost, a till joining a promoted main PC). Its tickets, return notes and Z
/// continue a series only that PC knows the last printed number of: any other PC is refused
/// with `terminal_code_taken` (the wizard offers the main PC's suggested code). One exception:
/// the same install trying again after a join that failed once paired ([`is_retry`]).
pub fn register(
    tx: &Transaction<'_>,
    actor: &Actor,
    request: &PairRequest,
    host_schema: i64,
    now: i64,
) -> Result<Paired, AppError> {
    if request.schema_version != host_schema {
        return Err(AppError::conflict("lan_version_mismatch"));
    }
    let code = request.terminal_code.trim().to_uppercase();
    if !valid_code(&code) {
        return Err(AppError::validation(
            "terminalCode",
            "terminal_code_invalid",
        ));
    }
    let name = request.terminal_name.trim().to_owned();
    if name.is_empty() {
        return Err(AppError::validation("terminalName", "required"));
    }
    if name.chars().count() > NAME_MAX {
        return Err(AppError::validation("terminalName", "too_long"));
    }
    let own = terminal::local(tx)?.map(|t| t.id);
    let existing: Option<(String, Option<i64>, bool)> = tx
        .query_row(
            "SELECT t.id, d.revoked_at, d.terminal_id IS NOT NULL
             FROM terminals t LEFT JOIN lan_devices d ON d.terminal_id = t.id WHERE t.code = ?1",
            [&code],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let (token, hash) = new_token()?;
    let by = actor.user_id.clone();
    let (id, repaired) = match existing {
        Some((id, _, _)) if Some(&id) == own.as_ref() => {
            return Err(AppError::validation("terminalCode", "terminal_code_taken"));
        }
        Some((id, _, _))
            if request.terminal_id.as_deref() != Some(id.as_str())
                && !is_retry(tx, &id, &request.install_id)? =>
        {
            return Err(AppError::validation("terminalCode", "terminal_code_taken"));
        }
        Some((id, _, has_device)) => {
            tx.execute(
                "UPDATE terminals SET name = ?2, role = 'client', paired_at = ?3 WHERE id = ?1",
                params![id, name, now],
            )?;
            if has_device {
                tx.execute(
                    "UPDATE lan_devices SET name = ?2, token_hash = ?3, paired_at = ?4, paired_by = ?5,
                            revoked_at = NULL, revoked_by = NULL, last_replayed_seq = NULL,
                            app_version = ?6, schema_version = ?7
                     WHERE terminal_id = ?1",
                    params![
                        id,
                        name,
                        hash,
                        now,
                        by,
                        request.app_version,
                        request.schema_version
                    ],
                )?;
            } else {
                insert_device(tx, &id, &name, &hash, now, by.as_deref(), request)?;
            }
            (id, true)
        }
        None => {
            let id = new_id();
            tx.execute(
                "INSERT INTO terminals (id, code, name, role, paired_at, created_at) VALUES (?1, ?2, ?3, 'client', ?4, ?4)",
                params![id, code, name, now],
            )?;
            insert_device(tx, &id, &name, &hash, now, by.as_deref(), request)?;
            (id, false)
        }
    };
    feed::register_till(tx, &id, now)?;
    audit::append(
        tx,
        &actor.event(
            "lan.till_paired",
            "terminal",
            Some(&id),
            json!({
                "code": code,
                "name": name,
                "appVersion": request.app_version,
                "repaired": repaired,
                "installId": request.install_id,
            }),
        ),
    )?;
    Ok(Paired {
        terminal: Terminal {
            id,
            code,
            name,
            role: TerminalRole::Client,
        },
        token,
    })
}

/// The same install pairs again after a join that failed once paired (the copy could not be
/// downloaded, checked or kept): its device is not revoked, has sent nothing yet (no item
/// replayed, no journal row) and its last pairing was this install's. The old token is replaced,
/// so no ghost device pins the change feed and the till's code stays its own.
fn is_retry(tx: &Transaction<'_>, terminal_id: &str, install_id: &str) -> Result<bool, AppError> {
    if install_id.is_empty() {
        return Ok(false);
    }
    let silent: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM lan_devices WHERE terminal_id = ?1 AND revoked_at IS NULL
                          AND last_replayed_seq IS NULL)
            AND NOT EXISTS (SELECT 1 FROM lan_journal WHERE terminal_id = ?1)",
        [terminal_id],
        |r| r.get(0),
    )?;
    if !silent {
        return Ok(false);
    }
    let last: Option<String> = tx
        .query_row(
            "SELECT json_extract(payload, '$.installId') FROM audit_log
             WHERE action = 'lan.till_paired' AND entity_type = 'terminal' AND entity_id = ?1
             ORDER BY seq DESC LIMIT 1",
            [terminal_id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    Ok(last.as_deref() == Some(install_id))
}

fn insert_device(
    tx: &Transaction<'_>,
    id: &str,
    name: &str,
    hash: &str,
    now: i64,
    by: Option<&str>,
    request: &PairRequest,
) -> Result<(), AppError> {
    tx.execute(
        "INSERT INTO lan_devices (terminal_id, name, token_hash, paired_at, paired_by, app_version, schema_version)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id,
            name,
            hash,
            now,
            by,
            request.app_version,
            request.schema_version
        ],
    )?;
    Ok(())
}

pub fn authenticate(conn: &Connection, token: &str) -> Result<Device, AppError> {
    conn.query_row(
        "SELECT d.terminal_id, t.code, d.name, d.last_replayed_seq
         FROM lan_devices d JOIN terminals t ON t.id = d.terminal_id
         WHERE d.token_hash = ?1 AND d.revoked_at IS NULL",
        [hash_token(token)],
        |r| {
            Ok(Device {
                terminal_id: r.get(0)?,
                code: r.get(1)?,
                name: r.get(2)?,
                last_replayed_seq: r.get(3)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| AppError::auth("lan_token_invalid"))
}

pub fn revoke(
    tx: &Transaction<'_>,
    actor: &Actor,
    terminal_id: &str,
    now: i64,
) -> Result<(), AppError> {
    let by = actor.user_id.clone().unwrap_or_else(|| "system".into());
    let changed = tx.execute(
        "UPDATE lan_devices SET revoked_at = ?2, revoked_by = ?3 WHERE terminal_id = ?1 AND revoked_at IS NULL",
        params![terminal_id, now, by],
    )?;
    if changed == 0 {
        return Ok(());
    }
    feed::unregister_till(tx, terminal_id)?;
    feed::prune(tx)?;
    audit::append(
        tx,
        &actor.event("lan.till_revoked", "terminal", Some(terminal_id), json!({})),
    )?;
    Ok(())
}

pub fn touch(
    tx: &Transaction<'_>,
    terminal_id: &str,
    info: &Touch,
    host_schema: i64,
    now: i64,
) -> Result<(), AppError> {
    tx.execute(
        "UPDATE lan_devices SET last_seen_at = ?2, last_address = COALESCE(?3, last_address),
                app_version = COALESCE(?4, app_version), schema_version = COALESCE(?5, schema_version),
                outbox_pending = COALESCE(?6, outbox_pending), skew_ms = COALESCE(?7, skew_ms)
         WHERE terminal_id = ?1",
        params![
            terminal_id,
            now,
            info.address,
            info.app_version,
            info.schema_version,
            info.outbox_pending.map(|p| p.max(0)),
            info.skew_ms
        ],
    )?;
    if let Some(schema) = info.schema_version
        && schema != host_schema
    {
        notices::raise(
            tx,
            NoticeKind::VersionMismatch,
            Some(terminal_id),
            terminal_id,
            json!({ "tillSchema": schema, "hostSchema": host_schema, "appVersion": info.app_version }),
            now,
        )?;
    }
    Ok(())
}

/// A new copy for the till (`snapshot::export`): its journal anchors at the copy's journal tail,
/// and its outbox numbers start again (the copy carries no outbox and no outbox sequence), so
/// the replay order check forgets the numbers of the till's previous line.
pub fn set_journal_anchor(
    tx: &Transaction<'_>,
    terminal_id: &str,
    seq: i64,
    hash: &str,
) -> Result<(), AppError> {
    tx.execute(
        "UPDATE lan_devices SET journal_anchor_seq = ?2, journal_anchor_hash = ?3, last_replayed_seq = NULL
         WHERE terminal_id = ?1",
        params![terminal_id, seq, hash],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::sync::pairing::PairingDesk;
    use crate::testkit::{NOON, shop};
    use std::sync::Mutex;

    fn request(code: &str, till: &str) -> PairRequest {
        PairRequest {
            code: code.into(),
            terminal_code: till.into(),
            terminal_name: "Caisse 2".into(),
            app_version: "0.5.0".into(),
            schema_version: crate::db::migrations::LATEST_VERSION,
            install_id: crate::ids::new_id(),
            terminal_id: None,
        }
    }

    fn pair_with(db: &Db, desk: &Mutex<PairingDesk>, till: &str) -> Result<Paired, AppError> {
        pair_as(db, desk, till, None)
    }

    /// Pairs as the PC whose own terminal is `terminal_id` (a former till of this shop).
    fn pair_as(
        db: &Db,
        desk: &Mutex<PairingDesk>,
        till: &str,
        terminal_id: Option<&str>,
    ) -> Result<Paired, AppError> {
        let code = desk.lock().unwrap().issue(NOON).unwrap().code;
        let mut req = request(&code, till);
        req.terminal_id = terminal_id.map(str::to_owned);
        pair(
            db,
            desk,
            req,
            crate::db::migrations::LATEST_VERSION,
            &Actor::default(),
            NOON,
        )
    }

    #[test]
    fn a_replayed_pairing_request_finds_its_code_spent() {
        let shop = shop();
        let desk = Mutex::new(PairingDesk::default());
        let code = desk.lock().unwrap().issue(NOON).unwrap().code;
        let req = request(&code, "C2");
        let paired = pair(
            &shop.db,
            &desk,
            req.clone(),
            crate::db::migrations::LATEST_VERSION,
            &Actor::default(),
            NOON,
        )
        .unwrap();
        assert_eq!(paired.terminal.code, "C2");
        let replay = pair(
            &shop.db,
            &desk,
            req,
            crate::db::migrations::LATEST_VERSION,
            &Actor::default(),
            NOON + 1,
        )
        .unwrap_err();
        assert_eq!(replay.code(), "pairing_code_invalid");
        assert_eq!(shop.count("SELECT COUNT(*) FROM lan_devices"), 1);
        assert!(
            !format!("{paired:?}").contains(&paired.token),
            "Debug hides the token"
        );
        assert!(
            !format!("{:?}", request("123456", "C2")).contains("123456"),
            "Debug hides the code"
        );
    }

    #[test]
    fn tokens_are_stored_hashed_and_revocation_is_immediate() {
        let shop = shop();
        let desk = Mutex::new(PairingDesk::default());
        let paired = pair_with(&shop.db, &desk, "C2").unwrap();
        assert_eq!(paired.token.len(), 64);
        let stored: String = shop
            .db
            .read(|c| Ok(c.query_row("SELECT token_hash FROM lan_devices", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(stored, hash_token(&paired.token));
        assert_ne!(stored, paired.token);
        let token = paired.token.clone();
        let device = shop.db.read(move |c| authenticate(c, &token)).unwrap();
        assert_eq!(
            (device.code.as_str(), device.last_replayed_seq),
            ("C2", None)
        );
        let consumer = crate::sync::changelog::lan_consumer(&device.terminal_id);
        assert!(
            shop.db
                .read({
                    let c = consumer.clone();
                    move |conn| crate::sync::changelog::position(conn, &c)
                })
                .unwrap()
                .is_some()
        );
        let (id, actor) = (device.terminal_id.clone(), shop.owner.actor());
        shop.db
            .write(move |tx| revoke(tx, &actor, &id, NOON))
            .unwrap();
        let token = paired.token.clone();
        assert_eq!(
            shop.db
                .read(move |c| authenticate(c, &token))
                .unwrap_err()
                .code(),
            "lan_token_invalid"
        );
        assert!(
            shop.db
                .read(move |conn| crate::sync::changelog::position(conn, &consumer))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            pair_with(&shop.db, &desk, "C2").unwrap_err().code(),
            "terminal_code_taken",
            "another PC never continues a revoked till's series"
        );
        let again = pair_as(&shop.db, &desk, "C2", Some(&device.terminal_id)).unwrap();
        assert_eq!(
            again.terminal.id, device.terminal_id,
            "a re-paired till keeps its terminal"
        );
        let old = paired.token.clone();
        assert_eq!(
            shop.db
                .read(move |c| authenticate(c, &old))
                .unwrap_err()
                .code(),
            "lan_token_invalid"
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM audit_log WHERE action IN ('lan.till_paired', 'lan.till_revoked')"),
            3
        );
    }

    /// A join that failed once paired (the copy never arrived): the same install pairs again
    /// with a new code and keeps its till code; the ghost device's token is replaced. Another
    /// install, or a till that already sent something, never takes the code that way.
    #[test]
    fn the_same_install_pairs_again_after_a_join_that_failed() {
        let shop = shop();
        let desk = Mutex::new(PairingDesk::default());
        let install = crate::ids::new_id();
        let pair_install = |install: &str| {
            let code = desk.lock().unwrap().issue(NOON).unwrap().code;
            let mut req = request(&code, "C2");
            req.install_id = install.to_owned();
            pair(
                &shop.db,
                &desk,
                req,
                crate::db::migrations::LATEST_VERSION,
                &Actor::default(),
                NOON,
            )
        };
        let first = pair_install(&install).unwrap();
        assert_eq!(
            pair_install(&crate::ids::new_id()).unwrap_err().code(),
            "terminal_code_taken",
            "another PC"
        );
        let again = pair_install(&install).unwrap();
        assert_eq!(again.terminal.id, first.terminal.id);
        let old = first.token.clone();
        assert_eq!(
            shop.db
                .read(move |c| authenticate(c, &old))
                .unwrap_err()
                .code(),
            "lan_token_invalid",
            "the ghost's token is replaced"
        );
        assert_eq!(shop.count("SELECT COUNT(*) FROM lan_devices"), 1);
        let id = again.terminal.id.clone();
        shop.db
            .write(move |tx| {
                tx.execute(
                    "UPDATE lan_devices SET last_replayed_seq = 1 WHERE terminal_id = ?1",
                    [&id],
                )?;
                Ok(())
            })
            .unwrap();
        assert_eq!(
            pair_install(&install).unwrap_err().code(),
            "terminal_code_taken",
            "a till that sent something is a real till"
        );
    }

    #[test]
    fn till_codes_are_checked_and_unique() {
        let shop = shop();
        let desk = Mutex::new(PairingDesk::default());
        assert_eq!(
            pair_with(&shop.db, &desk, "2C").unwrap_err().code(),
            "terminal_code_invalid"
        );
        assert_eq!(
            pair_with(&shop.db, &desk, "C1").unwrap_err().code(),
            "terminal_code_taken",
            "the main PC's own"
        );
        assert_eq!(
            pair_with(&shop.db, &desk, "c2").unwrap().terminal.code,
            "C2"
        );
        assert_eq!(
            pair_with(&shop.db, &desk, "C2").unwrap_err().code(),
            "terminal_code_taken"
        );
        assert_eq!(shop.db.read(suggest_code).unwrap(), "C3");
        assert!(
            valid_code("CAISSE1")
                && !valid_code("")
                && !valid_code("C-2")
                && !valid_code("ABCDEFGHI")
        );
    }

    #[test]
    fn a_till_of_another_version_is_refused_and_flagged_when_it_calls() {
        let shop = shop();
        let desk = Mutex::new(PairingDesk::default());
        let code = desk.lock().unwrap().issue(NOON).unwrap().code;
        let mut req = request(&code, "C2");
        req.schema_version -= 1;
        let err = pair(
            &shop.db,
            &desk,
            req,
            crate::db::migrations::LATEST_VERSION,
            &Actor::default(),
            NOON,
        )
        .unwrap_err();
        assert_eq!(err.code(), "lan_version_mismatch");
        let paired = pair_with(&shop.db, &desk, "C2").unwrap();
        let id = paired.terminal.id.clone();
        let touch_info = Touch {
            address: Some("192.168.1.21".into()),
            app_version: Some("0.4.9".into()),
            schema_version: Some(crate::db::migrations::LATEST_VERSION - 1),
            outbox_pending: Some(3),
            skew_ms: Some(1_200),
        };
        shop.db
            .write(move |tx| {
                touch(
                    tx,
                    &id,
                    &touch_info,
                    crate::db::migrations::LATEST_VERSION,
                    NOON + 5,
                )
            })
            .unwrap();
        assert_eq!(shop.count("SELECT outbox_pending FROM lan_devices"), 3);
        assert_eq!(shop.count("SELECT last_seen_at FROM lan_devices"), NOON + 5);
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM lan_notices WHERE kind = 'version_mismatch'"),
            1
        );
    }
}

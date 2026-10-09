//! What the shell shows about the LAN (UX-024): on a till, the link and what it still owes; on
//! the main PC, the server and its tills. Filled by the runtime of Task 19 from these queries.

use super::outbox;
use super::role;
use super::tills::ONLINE_WITHIN_MS;
use crate::error::AppError;
use crate::terminal::TerminalRole;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum LinkState {
    Connecting,
    Online,
    Offline,
    Replaying,
    Behind,
    Attention,
    VersionMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ClientStatus {
    pub host_name: String,
    pub host_address: String,
    pub link: LinkState,
    pub pending: i64,
    pub rejected: i64,
    pub oldest_pending_at: Option<i64>,
    pub behind_by: i64,
    pub applied_seq: i64,
    pub last_contact_at: Option<i64>,
    pub skew_ms: Option<i64>,
    /// An error code the UI translates (`errors:<code>`).
    pub last_error: Option<String>,
    /// This till's host entries (registers, serials, ledgers) and money calls the main PC refused:
    /// the shell's banner points to Settings › Réseau while there are some.
    #[serde(default)]
    pub entries_refused: i64,
    /// This till's regulatory data pack against its main PC's (final review): `restart`,
    /// `behind` or `ahead` (`datapack_update::gap`); `None` when they match.
    #[serde(default)]
    pub datapack_gap: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HostStatus {
    pub listening: bool,
    pub port: u16,
    pub addresses: Vec<String>,
    pub tills_total: i64,
    pub tills_online: i64,
    pub notices_open: i64,
    pub pairing_open_until: Option<i64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LanStatus {
    pub role: TerminalRole,
    pub host_id: Option<String>,
    pub host: Option<HostStatus>,
    pub client: Option<ClientStatus>,
}

pub fn client_status(
    conn: &Connection,
    link: LinkState,
    head: Option<i64>,
    last_error: Option<String>,
) -> Result<ClientStatus, AppError> {
    let role = role::current(conn)?;
    let depth = outbox::depth(conn)?;
    Ok(ClientStatus {
        host_name: role.host_name.unwrap_or_default(),
        host_address: role.host_address.unwrap_or_default(),
        link,
        pending: depth.pending,
        rejected: depth.rejected,
        oldest_pending_at: depth.oldest_pending_at,
        behind_by: head.map_or(0, |h| (h - role.applied_seq).max(0)),
        applied_seq: role.applied_seq,
        last_contact_at: role.last_contact_at,
        skew_ms: role.skew_ms,
        last_error,
        entries_refused: entries_refused(conn)?,
        datapack_gap: crate::datapack_update::gap(conn)?,
    })
}

/// How many of this till's host entries and money calls the main PC refused
/// (`sales::extensions::HostRegisters::refused`, `sync::answers::PendingCall::refused`).
pub fn entries_refused(conn: &Connection) -> Result<i64, AppError> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM app_meta
         WHERE (substr(key, 1, 15) = 'host_registers:' OR substr(key, 1, 12) = 'rpc_pending:')
           AND json_valid(value) AND json_extract(value, '$.refused') IS NOT NULL",
        [],
        |r| r.get(0),
    )?)
}

/// `(tills paired, tills seen within a minute, open notices)` on the main PC.
pub fn host_counts(conn: &Connection, now: i64) -> Result<(i64, i64, i64), AppError> {
    Ok(conn.query_row(
        "SELECT (SELECT COUNT(*) FROM lan_devices WHERE revoked_at IS NULL),
                (SELECT COUNT(*) FROM lan_devices WHERE revoked_at IS NULL AND last_seen_at >= ?1),
                (SELECT COUNT(*) FROM lan_notices WHERE acknowledged_at IS NULL)",
        [now - ONLINE_WITHIN_MS],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::testkit::lan_pair;

    #[test]
    fn the_status_of_a_till_counts_what_it_owes_and_how_far_behind_it_is() {
        let lan = lan_pair();
        lan.open_till_session(0);
        let head = lan.host.db.read(crate::sync::feed::head).unwrap();
        let status = lan
            .till
            .read(move |c| client_status(c, LinkState::Offline, Some(head + 7), None))
            .unwrap();
        assert_eq!(
            (status.pending, status.rejected, status.behind_by),
            (1, 0, 7)
        );
        assert_eq!(status.host_name, "Supérette El Amel");
        assert_eq!(status.link, LinkState::Offline);
        let (tills, online, notices) = lan
            .host
            .db
            .read(|c| host_counts(c, crate::testkit::NOON))
            .unwrap();
        assert_eq!((tills, online, notices), (1, 0, 0));
        let json = serde_json::to_value(LinkState::VersionMismatch).unwrap();
        assert_eq!(json, "version_mismatch");
    }
}

//! What the owner must see on the main PC (M5): a till sold stock that was not there under the
//! `block` policy, a kridi went over its limit or store credit was spent twice, an allocation
//! could not be kept, a chain was rebased, a till's journal arrived broken, a Z was closed with
//! operations still to send, a till's clock is off, an item was refused, a till needs an update,
//! a Z does not link to the previous one. One open notice per (kind, till, subject); acknowledged
//! by the owner, never deleted.

use crate::audit::{self, Actor};
use crate::error::AppError;
use crate::ids::new_id;
use crate::sqlenum::sql_text_enum;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum NoticeKind {
    Oversold,
    CreditOverLimit,
    StoreCreditOverdrawn,
    AllocationSkipped,
    ChainRebased,
    JournalBroken,
    OfflineZ,
    ClockSkew,
    ReplayRejected,
    VersionMismatch,
    ClosureUnlinked,
}

impl NoticeKind {
    pub const ALL: [NoticeKind; 11] = [
        NoticeKind::Oversold,
        NoticeKind::CreditOverLimit,
        NoticeKind::StoreCreditOverdrawn,
        NoticeKind::AllocationSkipped,
        NoticeKind::ChainRebased,
        NoticeKind::JournalBroken,
        NoticeKind::OfflineZ,
        NoticeKind::ClockSkew,
        NoticeKind::ReplayRejected,
        NoticeKind::VersionMismatch,
        NoticeKind::ClosureUnlinked,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            NoticeKind::Oversold => "oversold",
            NoticeKind::CreditOverLimit => "credit_over_limit",
            NoticeKind::StoreCreditOverdrawn => "store_credit_overdrawn",
            NoticeKind::AllocationSkipped => "allocation_skipped",
            NoticeKind::ChainRebased => "chain_rebased",
            NoticeKind::JournalBroken => "journal_broken",
            NoticeKind::OfflineZ => "offline_z",
            NoticeKind::ClockSkew => "clock_skew",
            NoticeKind::ReplayRejected => "replay_rejected",
            NoticeKind::VersionMismatch => "version_mismatch",
            NoticeKind::ClosureUnlinked => "closure_unlinked",
        }
    }

    pub fn parse(s: &str) -> Option<NoticeKind> {
        NoticeKind::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

sql_text_enum!(NoticeKind);

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub id: String,
    pub kind: NoticeKind,
    pub terminal_id: Option<String>,
    pub terminal_code: Option<String>,
    pub subject: String,
    #[specta(type = crate::json::JsonValue)]
    pub details: Value,
    pub created_at: i64,
    pub acknowledged_at: Option<i64>,
    pub acknowledged_by: Option<String>,
}

/// Opens a notice unless the same one is already open; returns its id when it opened one.
pub fn raise(
    tx: &Transaction<'_>,
    kind: NoticeKind,
    terminal_id: Option<&str>,
    subject: &str,
    details: Value,
    now: i64,
) -> Result<Option<String>, AppError> {
    let id = new_id();
    let changed = tx.execute(
        "INSERT INTO lan_notices (id, kind, terminal_id, subject, details, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT DO NOTHING",
        params![id, kind, terminal_id, subject, details.to_string(), now],
    )?;
    Ok((changed == 1).then_some(id))
}

pub fn list(
    conn: &Connection,
    include_acknowledged: bool,
    limit: u32,
) -> Result<Vec<Notice>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT n.id, n.kind, n.terminal_id, t.code, n.subject, n.details, n.created_at,
                n.acknowledged_at, n.acknowledged_by
         FROM lan_notices n LEFT JOIN terminals t ON t.id = n.terminal_id
         WHERE ?1 OR n.acknowledged_at IS NULL
         ORDER BY n.created_at DESC, n.id DESC LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(
            params![include_acknowledged, i64::from(limit.clamp(1, 500))],
            |r| {
                let details: String = r.get(5)?;
                Ok(Notice {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    terminal_id: r.get(2)?,
                    terminal_code: r.get(3)?,
                    subject: r.get(4)?,
                    details: serde_json::from_str(&details).unwrap_or(Value::Null),
                    created_at: r.get(6)?,
                    acknowledged_at: r.get(7)?,
                    acknowledged_by: r.get(8)?,
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn open_count(conn: &Connection) -> Result<i64, AppError> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM lan_notices WHERE acknowledged_at IS NULL",
        [],
        |r| r.get(0),
    )?)
}

pub fn acknowledge(
    tx: &Transaction<'_>,
    actor: &Actor,
    id: &str,
    now: i64,
) -> Result<(), AppError> {
    let state: Option<(String, Option<i64>)> = tx
        .query_row(
            "SELECT kind, acknowledged_at FROM lan_notices WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((kind, acknowledged)) = state else {
        return Err(AppError::not_found("record"));
    };
    if acknowledged.is_some() {
        return Ok(());
    }
    let by = actor.user_id.clone().unwrap_or_else(|| "system".into());
    tx.execute(
        "UPDATE lan_notices SET acknowledged_at = ?2, acknowledged_by = ?3 WHERE id = ?1",
        params![id, now, by],
    )?;
    audit::append(
        tx,
        &actor.event(
            "lan.notice_acknowledged",
            "lan_notice",
            Some(id),
            json!({ "kind": kind }),
        ),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::Actor;
    use crate::db::Db;
    use serde_json::json;

    #[test]
    fn one_open_notice_per_subject_until_acknowledged() {
        let db = Db::open_memory().unwrap();
        let raise_one = |db: &Db| {
            db.write(|tx| {
                raise(
                    tx,
                    NoticeKind::Oversold,
                    None,
                    "p-lait",
                    json!({ "qty": -1000 }),
                    10,
                )
            })
            .unwrap()
        };
        let first = raise_one(&db).unwrap();
        assert!(raise_one(&db).is_none(), "already open");
        assert_eq!(db.read(open_count).unwrap(), 1);
        let owner = Actor {
            user_id: Some("u-owner".into()),
            terminal_id: None,
        };
        let id = first.clone();
        db.write(move |tx| acknowledge(tx, &owner, &id, 20))
            .unwrap();
        let owner = Actor {
            user_id: Some("u-owner".into()),
            terminal_id: None,
        };
        let id = first.clone();
        db.write(move |tx| acknowledge(tx, &owner, &id, 30))
            .unwrap(); // idempotent
        assert_eq!(db.read(open_count).unwrap(), 0);
        assert!(raise_one(&db).is_some(), "a new episode");
        let all = db.read(|c| list(c, true, 50)).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(
            all[1].acknowledged_at,
            Some(20),
            "the first acknowledgement is kept"
        );
        assert_eq!(all[0].details["qty"], -1000);
        let journaled: i64 = db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT COUNT(*) FROM audit_log WHERE action = 'lan.notice_acknowledged'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(journaled, 1);
        let unknown = db
            .write(|tx| {
                acknowledge(
                    tx,
                    &Actor::default(),
                    "00000000-0000-7000-8000-00000000dead",
                    1,
                )
            })
            .unwrap_err();
        assert_eq!(unknown.code(), "record_not_found");
    }

    #[test]
    fn kinds_match_the_table_check() {
        for kind in NoticeKind::ALL {
            assert_eq!(NoticeKind::parse(kind.as_str()), Some(kind));
        }
        let db = Db::open_memory().unwrap();
        for kind in NoticeKind::ALL {
            db.write(move |tx| raise(tx, kind, None, "s", json!({}), 1))
                .unwrap();
        }
        assert_eq!(
            db.read(open_count).unwrap(),
            i64::try_from(NoticeKind::ALL.len()).unwrap()
        );
    }
}

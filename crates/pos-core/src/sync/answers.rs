//! Final review: a till's host service that moves money (a deposit, a voucher sold, a contract
//! signed, an advance…) carries an idempotency key. The main PC keeps its answer per till and key
//! ([`keep`], [`kept`]) and answers the same key again from it, so a call sent again after a lost
//! answer never writes twice. The till keeps such a call whose answer was lost ([`PendingCall`])
//! and sends it again with the same key until the main PC answers, then puts the money in its own
//! drawer. Both live in this PC's `app_meta` (`LocalOnly`, never copied).

use crate::error::AppError;
use rusqlite::{Connection, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `app_meta` prefix of the main PC's kept answers: `rpc_result:<till terminal>:<key>`.
const RESULT_PREFIX: &str = "rpc_result:";
/// `app_meta` prefix of a till's calls waiting for their answer: `rpc_pending:<key>`.
const PENDING_PREFIX: &str = "rpc_pending:";
/// How long the main PC keeps an answer: a till sends a lost one again within the hour (its
/// maintenance loop) and at its next call.
pub const KEPT_MS: i64 = 30 * 24 * 60 * 60 * 1_000;

/// A till's key: 1 to 64 letters, digits or dashes (a UUID).
pub fn key_is_valid(key: &str) -> bool {
    !key.is_empty() && key.len() <= 64 && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn result_key(till: &str, key: &str) -> String {
    format!("{RESULT_PREFIX}{till}:{key}")
}

/// On the main PC: the answer given to the till `till` for `key`, when it was answered.
pub fn kept(conn: &Connection, till: &str, key: &str) -> Result<Option<Value>, AppError> {
    let Some(json) = crate::db::meta_get(conn, &result_key(till, key))? else {
        return Ok(None);
    };
    let kept: Value = serde_json::from_str(&json)?;
    Ok(kept.get("output").cloned())
}

/// On the main PC: keeps `output` of `service` for the till's `key` and forgets the answers
/// older than [`KEPT_MS`].
pub fn keep(
    tx: &Transaction<'_>,
    till: &str,
    key: &str,
    service: &str,
    output: &Value,
    now: i64,
) -> Result<(), AppError> {
    if !key_is_valid(key) {
        return Err(AppError::validation("idempotencyKey", "value_invalid"));
    }
    let value = serde_json::json!({ "at": now, "service": service, "output": output });
    crate::db::meta_set(tx, &result_key(till, key), &value.to_string())?;
    tx.execute(
        "DELETE FROM app_meta WHERE substr(key, 1, length(?1)) = ?1
           AND json_valid(value) AND json_extract(value, '$.at') < ?2",
        params![RESULT_PREFIX, now - KEPT_MS],
    )?;
    Ok(())
}

/// On a till: a money call whose answer was lost, sent again with its key until the main PC
/// answers; then its drawer share goes into this till's drawer and it leaves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingCall {
    pub key: String,
    pub service: String,
    /// The user who made the call (the main PC runs it as them again; the drawer share is theirs).
    pub user_id: String,
    /// The manager approvals sent with it (the wire's grants, as sent).
    #[serde(default)]
    pub approvals: Value,
    pub input: Value,
    pub at: i64,
    /// The main PC's answer, once it came, while its cash is not in this till's drawer yet (the
    /// drawer was closed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<Value>,
    /// The code of the main PC's refusal (the call wrote nothing there).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refused: Option<String>,
}

fn pending_key(key: &str) -> String {
    format!("{PENDING_PREFIX}{key}")
}

/// Keeps (or updates) `call` on this till.
pub fn save_call(tx: &Transaction<'_>, call: &PendingCall) -> Result<(), AppError> {
    crate::db::meta_set(tx, &pending_key(&call.key), &serde_json::to_string(call)?)
}

/// The call is settled (its cash kept, or given up by the owner): it leaves this till.
pub fn call_done(tx: &Transaction<'_>, key: &str) -> Result<(), AppError> {
    tx.execute("DELETE FROM app_meta WHERE key = ?1", [pending_key(key)])?;
    Ok(())
}

/// This till's calls waiting for their answer, oldest first.
pub fn pending_calls(conn: &Connection) -> Result<Vec<PendingCall>, AppError> {
    let mut stmt =
        conn.prepare("SELECT value FROM app_meta WHERE substr(key, 1, length(?1)) = ?1")?;
    let rows = stmt
        .query_map([PENDING_PREFIX], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut calls = rows
        .iter()
        .map(|json| Ok(serde_json::from_str::<PendingCall>(json)?))
        .collect::<Result<Vec<_>, AppError>>()?;
    calls.sort_by(|a, b| a.at.cmp(&b.at).then_with(|| a.key.cmp(&b.key)));
    Ok(calls)
}

/// A refused call is cleared of its mark to go again at once (« Réessayer »). Returns how many.
pub fn retry_calls(tx: &Transaction<'_>) -> Result<u32, AppError> {
    let mut cleared = 0;
    for mut call in pending_calls(tx)? {
        if call.refused.take().is_some() {
            save_call(tx, &call)?;
            cleared += 1;
        }
    }
    Ok(cleared)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{NOON, shop};
    use serde_json::json;

    #[test]
    fn the_main_pc_answers_a_key_again_from_what_it_answered_and_forgets_old_answers() {
        let shop = shop();
        shop.db
            .write(|tx| {
                keep(
                    tx,
                    "till-1",
                    "k-1",
                    "deposits.take",
                    &json!({ "id": "d1" }),
                    NOON - KEPT_MS - 1,
                )
            })
            .unwrap();
        assert_eq!(
            shop.db.read(|c| kept(c, "till-1", "k-1")).unwrap(),
            Some(json!({ "id": "d1" }))
        );
        assert_eq!(
            shop.db.read(|c| kept(c, "till-2", "k-1")).unwrap(),
            None,
            "a key is the till's own"
        );
        shop.db
            .write(|tx| keep(tx, "till-1", "k-2", "deposits.take", &json!(2), NOON))
            .unwrap();
        assert_eq!(
            shop.db.read(|c| kept(c, "till-1", "k-1")).unwrap(),
            None,
            "older than a month: forgotten"
        );
        assert_eq!(
            shop.db.read(|c| kept(c, "till-1", "k-2")).unwrap(),
            Some(json!(2))
        );
        let bad = shop
            .db
            .write(|tx| keep(tx, "till-1", "k 3; DROP", "x", &json!(0), NOON))
            .unwrap_err();
        assert_eq!(bad.code(), "value_invalid");
    }

    #[test]
    fn a_tills_pending_call_is_kept_marked_retried_and_settled() {
        let shop = shop();
        let call = |key: &str, at: i64| PendingCall {
            key: key.into(),
            service: "deposits.take".into(),
            user_id: "u1".into(),
            approvals: json!([]),
            input: json!({ "input": { "amount": 100 } }),
            at,
            output: None,
            refused: None,
        };
        let (late, early) = (call("b", NOON + 1), call("a", NOON));
        shop.db
            .write(move |tx| {
                save_call(tx, &late)?;
                save_call(tx, &early)
            })
            .unwrap();
        let calls = shop.db.read(pending_calls).unwrap();
        assert_eq!(
            calls.iter().map(|c| c.key.as_str()).collect::<Vec<_>>(),
            ["a", "b"],
            "oldest first"
        );
        let mut refused = calls[0].clone();
        refused.refused = Some("deposit_insufficient".into());
        shop.db.write(move |tx| save_call(tx, &refused)).unwrap();
        assert_eq!(shop.db.write(|tx| retry_calls(tx)).unwrap(), 1);
        assert!(
            shop.db
                .read(pending_calls)
                .unwrap()
                .iter()
                .all(|c| c.refused.is_none())
        );
        shop.db.write(|tx| call_done(tx, "a")).unwrap();
        assert_eq!(shop.db.read(pending_calls).unwrap().len(), 1);
    }
}

//! This PC as a till (spec §4.6). M0 only uses the standalone role; codes are C1, C2, …

use crate::db::{meta_get, meta_set};
use crate::error::AppError;
use crate::ids::new_id;
use crate::sqlenum::sql_text_enum;
use crate::time::now_ms;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use specta::Type;

/// `app_meta` key holding this PC's terminal id (local, never replicated).
pub const META_TERMINAL_ID: &str = "terminal_id";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum TerminalRole {
    #[default]
    Standalone,
    Host,
    Client,
}

impl TerminalRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            TerminalRole::Standalone => "standalone",
            TerminalRole::Host => "host",
            TerminalRole::Client => "client",
        }
    }

    pub fn parse(s: &str) -> Option<TerminalRole> {
        [
            TerminalRole::Standalone,
            TerminalRole::Host,
            TerminalRole::Client,
        ]
        .into_iter()
        .find(|r| r.as_str() == s)
    }
}

sql_text_enum!(TerminalRole);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Terminal {
    pub id: String,
    pub code: String,
    pub name: String,
    pub role: TerminalRole,
}

pub fn get(conn: &Connection, id: &str) -> Result<Option<Terminal>, AppError> {
    let terminal = conn
        .query_row(
            "SELECT id, code, name, role FROM terminals WHERE id = ?1",
            [id],
            |r| {
                Ok(Terminal {
                    id: r.get(0)?,
                    code: r.get(1)?,
                    name: r.get(2)?,
                    role: r.get(3)?,
                })
            },
        )
        .optional()?;
    Ok(terminal)
}

pub fn local(conn: &Connection) -> Result<Option<Terminal>, AppError> {
    match meta_get(conn, META_TERMINAL_ID)? {
        Some(id) => get(conn, &id),
        None => Ok(None),
    }
}

/// Returns this PC's terminal, creating `C<n+1>` on first launch.
pub fn ensure_local(tx: &Transaction<'_>) -> Result<Terminal, AppError> {
    if let Some(terminal) = local(tx)? {
        return Ok(terminal);
    }
    let existing: i64 = tx.query_row("SELECT COUNT(*) FROM terminals", [], |r| r.get(0))?;
    let code = format!("C{}", existing + 1);
    let terminal = Terminal {
        id: new_id(),
        name: code.clone(),
        code,
        role: TerminalRole::Standalone,
    };
    tx.execute(
        "INSERT INTO terminals (id, code, name, role, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            terminal.id,
            terminal.code,
            terminal.name,
            terminal.role,
            now_ms()
        ],
    )?;
    meta_set(tx, META_TERMINAL_ID, &terminal.id)?;
    Ok(terminal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    #[test]
    fn local_terminal_is_created_once_as_c1() {
        let db = Db::open_memory().unwrap();
        assert_eq!(db.read(local).unwrap(), None);
        let first = db.write(|tx| ensure_local(tx)).unwrap();
        let again = db.write(|tx| ensure_local(tx)).unwrap();
        assert_eq!(first, again);
        assert_eq!(
            (first.code.as_str(), first.role),
            ("C1", TerminalRole::Standalone)
        );
        assert_eq!(db.read(local).unwrap(), Some(first));
    }
}

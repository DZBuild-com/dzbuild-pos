//! Paper and PDF copies of a document (DOC-017/018): the first output of a validated M3
//! document is the original; every later one is a DUPLICATA. A ticket or a return note had its
//! original on the receipt printer, so its A4 or PDF copy is always a DUPLICATA. Each output is
//! journaled once it has succeeded (`document.printed`, `document.reprinted`,
//! `document.exported`); M1b's thermal reprints (`document.reprinted`) count too.

use super::{DocStatus, DocType};
use crate::audit::{self, Actor};
use crate::error::AppError;
use rusqlite::{Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use specta::Type;

pub const OUTPUT_ACTIONS: [&str; 3] = [
    "document.printed",
    "document.reprinted",
    "document.exported",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OutputKind {
    Print,
    Pdf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    /// 1 for the first copy ever made of the document.
    pub copy_no: i64,
    pub duplicate: bool,
}

/// What the next copy of `id` is.
pub fn next_output(conn: &Connection, id: &str) -> Result<Output, AppError> {
    let row: Option<(DocType, DocStatus)> = conn
        .query_row(
            "SELECT doc_type, status FROM documents WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (doc_type, status) = row.ok_or_else(|| AppError::not_found("document"))?;
    if status != DocStatus::Validated {
        return Err(AppError::conflict("document_not_validated"));
    }
    let done: i64 = conn.query_row(
        "SELECT COUNT(*) FROM audit_log WHERE entity_type = 'document' AND entity_id = ?1
           AND action IN ('document.printed', 'document.reprinted', 'document.exported')",
        [id],
        |r| r.get(0),
    )?;
    let thermal_original = matches!(doc_type, DocType::Ticket | DocType::ReturnNote);
    Ok(Output {
        copy_no: done + 1,
        duplicate: thermal_original || done > 0,
    })
}

/// Journals a copy that was made (`detail`: printer, copies, language, paper).
pub fn record_output(
    tx: &Transaction<'_>,
    actor: &Actor,
    id: &str,
    kind: OutputKind,
    output: &Output,
    detail: Value,
) -> Result<(), AppError> {
    let action = match (kind, output.duplicate) {
        (OutputKind::Pdf, _) => "document.exported",
        (OutputKind::Print, false) => "document.printed",
        (OutputKind::Print, true) => "document.reprinted",
    };
    audit::append(
        tx,
        &actor.event(
            action,
            "document",
            Some(id),
            json!({ "copy": output.copy_no, "duplicate": output.duplicate, "detail": detail }),
        ),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::documents::tests_support::validated_of;
    use crate::testkit::shop;

    #[test]
    fn the_first_output_is_the_original_then_duplicates_are_counted() {
        let shop = shop();
        let invoice = validated_of(&shop.db, DocType::Invoice);
        let id = invoice.clone();
        let first = shop.db.read(move |c| next_output(c, &id)).unwrap();
        assert_eq!(
            first,
            Output {
                copy_no: 1,
                duplicate: false
            }
        );
        let (id, actor) = (invoice.clone(), shop.owner.actor());
        shop.db
            .write(move |tx| {
                record_output(
                    tx,
                    &actor,
                    &id,
                    OutputKind::Print,
                    &first,
                    json!({ "printer": "HP" }),
                )
            })
            .unwrap();
        let id = invoice.clone();
        let second = shop.db.read(move |c| next_output(c, &id)).unwrap();
        assert_eq!(
            second,
            Output {
                copy_no: 2,
                duplicate: true
            }
        );
        let (id, actor) = (invoice.clone(), shop.owner.actor());
        shop.db
            .write(move |tx| record_output(tx, &actor, &id, OutputKind::Pdf, &second, json!({})))
            .unwrap();
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM audit_log WHERE action IN ('document.printed', 'document.exported')"),
            2
        );
        let ticket = validated_of(&shop.db, DocType::Ticket);
        let out = shop.db.read(move |c| next_output(c, &ticket)).unwrap();
        assert!(out.duplicate, "a ticket's original was its receipt");
        let draft = crate::documents::tests_support::invoice_draft(&shop.db);
        let err = shop.db.read(move |c| next_output(c, &draft)).unwrap_err();
        assert_eq!(err.code(), "document_not_validated");
    }
}

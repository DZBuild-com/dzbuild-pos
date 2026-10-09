//! Which document was made from which (DOC-031, SAL-054, ORD-001, RET-012): append-only rows
//! the archive, the states and the printed references read. A delivery note and a ticket are
//! invoiced once (checked here for a clear message, enforced by a unique index of 0004).

use super::{DocStatus, DocType};
use crate::error::AppError;
use crate::ids::new_id;
use crate::money::Money;
use crate::sqlenum::sql_text_enum;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum LinkKind {
    /// A quote or proforma became another document (a draft at first).
    Converted,
    /// A delivery note was invoiced.
    Delivered,
    /// A till ticket was invoiced.
    InvoicedTicket,
    /// An invoice was corrected by a credit note.
    Credited,
    /// A quote was sold at the till.
    SoldFromQuote,
}

impl LinkKind {
    pub const ALL: [LinkKind; 5] = [
        LinkKind::Converted,
        LinkKind::Delivered,
        LinkKind::InvoicedTicket,
        LinkKind::Credited,
        LinkKind::SoldFromQuote,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            LinkKind::Converted => "converted",
            LinkKind::Delivered => "delivered",
            LinkKind::InvoicedTicket => "invoiced_ticket",
            LinkKind::Credited => "credited",
            LinkKind::SoldFromQuote => "sold_from_quote",
        }
    }

    pub fn parse(s: &str) -> Option<LinkKind> {
        LinkKind::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

sql_text_enum!(LinkKind);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DocumentLink {
    pub id: String,
    pub from_id: String,
    pub to_id: String,
    pub kind: LinkKind,
    pub created_at: i64,
}

/// A link with the document at its other end.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LinkedDocument {
    pub link: DocumentLink,
    pub doc_type: DocType,
    pub number: Option<String>,
    pub status: DocStatus,
    pub business_date: String,
    pub total_due: Money,
}

/// Records that `to_id` was made from `from_id`. A delivery note or a ticket that already has
/// its invoice is refused (`delivery_already_invoiced`, `ticket_already_invoiced`).
pub fn add(
    tx: &Transaction<'_>,
    from_id: &str,
    to_id: &str,
    kind: LinkKind,
    now: i64,
) -> Result<DocumentLink, AppError> {
    if matches!(kind, LinkKind::Delivered | LinkKind::InvoicedTicket)
        && invoiced(tx, from_id)?.is_some()
    {
        return Err(AppError::conflict(match kind {
            LinkKind::Delivered => "delivery_already_invoiced",
            _ => "ticket_already_invoiced",
        }));
    }
    let link = DocumentLink {
        id: new_id(),
        from_id: from_id.to_owned(),
        to_id: to_id.to_owned(),
        kind,
        created_at: now,
    };
    tx.prepare_cached(
        "INSERT INTO document_links (id, from_id, to_id, kind, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
    )?
    .execute(params![link.id, link.from_id, link.to_id, link.kind, link.created_at])?;
    Ok(link)
}

fn linked(conn: &Connection, sql: &str, id: &str) -> Result<Vec<LinkedDocument>, AppError> {
    let mut stmt = conn.prepare_cached(sql)?;
    let rows = stmt
        .query_map([id], |r| {
            Ok(LinkedDocument {
                link: DocumentLink {
                    id: r.get(0)?,
                    from_id: r.get(1)?,
                    to_id: r.get(2)?,
                    kind: r.get(3)?,
                    created_at: r.get(4)?,
                },
                doc_type: r.get(5)?,
                number: r.get(6)?,
                status: r.get(7)?,
                business_date: r.get(8)?,
                total_due: r.get(9)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// The documents made from `from_id`, oldest first.
pub fn targets(conn: &Connection, from_id: &str) -> Result<Vec<LinkedDocument>, AppError> {
    linked(
        conn,
        "SELECT k.id, k.from_id, k.to_id, k.kind, k.created_at, d.doc_type, d.number, d.status,
                d.business_date, d.total_due
         FROM document_links k JOIN documents d ON d.id = k.to_id
         WHERE k.from_id = ?1 ORDER BY k.created_at, k.id",
        from_id,
    )
}

/// The documents `to_id` was made from, oldest first.
pub fn sources(conn: &Connection, to_id: &str) -> Result<Vec<LinkedDocument>, AppError> {
    linked(
        conn,
        "SELECT k.id, k.from_id, k.to_id, k.kind, k.created_at, d.doc_type, d.number, d.status,
                d.business_date, d.total_due
         FROM document_links k JOIN documents d ON d.id = k.from_id
         WHERE k.to_id = ?1 ORDER BY k.created_at, k.id",
        to_id,
    )
}

/// The invoice of a delivery note or a ticket, when there is one.
pub fn invoiced(conn: &Connection, from_id: &str) -> Result<Option<LinkedDocument>, AppError> {
    let to_id: Option<String> = conn
        .query_row(
            "SELECT to_id FROM document_links WHERE from_id = ?1 AND kind IN ('delivered', 'invoiced_ticket')",
            [from_id],
            |r| r.get(0),
        )
        .optional()?;
    match to_id {
        None => Ok(None),
        Some(to) => Ok(targets(conn, from_id)?
            .into_iter()
            .find(|l| l.link.to_id == to)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::documents::tests_support::{invoice_draft, validated_of};
    use crate::testkit::{NOON, shop};

    #[test]
    fn a_delivery_note_and_a_ticket_are_invoiced_once() {
        let shop = shop();
        let bl = validated_of(&shop.db, DocType::DeliveryNote);
        let (a, b) = (invoice_draft(&shop.db), invoice_draft(&shop.db));
        let (from, to) = (bl.clone(), a.clone());
        let link = shop
            .db
            .write(move |tx| add(tx, &from, &to, LinkKind::Delivered, NOON))
            .unwrap();
        assert_eq!(link.kind, LinkKind::Delivered);
        let (from, to) = (bl.clone(), b.clone());
        let err = shop
            .db
            .write(move |tx| add(tx, &from, &to, LinkKind::Delivered, NOON))
            .unwrap_err();
        assert_eq!(err.code(), "delivery_already_invoiced");
        let ticket = validated_of(&shop.db, DocType::Ticket);
        let (from, to) = (ticket.clone(), a.clone());
        shop.db
            .write(move |tx| add(tx, &from, &to, LinkKind::InvoicedTicket, NOON))
            .unwrap();
        let (from, to) = (ticket.clone(), b.clone());
        let err = shop
            .db
            .write(move |tx| add(tx, &from, &to, LinkKind::InvoicedTicket, NOON))
            .unwrap_err();
        assert_eq!(err.code(), "ticket_already_invoiced");
        let seen = shop.db.read(move |c| invoiced(c, &bl)).unwrap().unwrap();
        assert_eq!(
            (seen.link.to_id, seen.doc_type),
            (a.clone(), DocType::Invoice)
        );
        let sources = shop.db.read(move |c| sources(c, &a)).unwrap();
        assert_eq!(sources.len(), 2);
    }
}

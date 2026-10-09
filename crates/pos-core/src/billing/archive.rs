//! The document archive (DOC-017, DAT-022): every document of every kind, drafts included,
//! newest first, searchable by number, name, amount, customer, kind and period, with its state
//! and how many copies were made. Nothing is ever deleted from it.

use super::state::{DocState, state_of};
use crate::catalog::normalize;
use crate::documents::{self, DocOrigin, DocStatus, DocType};
use crate::error::AppError;
use crate::money::Money;
use crate::table::{Cell, ColumnKind, Table, TagDomain};
use rusqlite::{Connection, ToSql};
use serde::{Deserialize, Serialize};
use specta::Type;

pub const MAX_LIMIT: u32 = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveCursor {
    pub sort_at: i64,
    pub id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct ArchiveQuery {
    pub kinds: Vec<DocType>,
    pub status: Option<DocStatus>,
    pub text: Option<String>,
    pub customer_id: Option<String>,
    pub supplier_id: Option<String>,
    /// Business days, `YYYY-MM-DD`, both included.
    pub from: Option<String>,
    pub to: Option<String>,
    pub min_total: Option<Money>,
    pub max_total: Option<Money>,
    pub before: Option<ArchiveCursor>,
    /// 1..=200, default 100.
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveRow {
    pub id: String,
    pub doc_type: DocType,
    pub number: Option<String>,
    pub status: DocStatus,
    pub business_date: String,
    pub sort_at: i64,
    pub party_name: Option<String>,
    pub total_ttc: Money,
    pub total_due: Money,
    pub origin: Option<DocOrigin>,
    pub parent_number: Option<String>,
    pub state: DocState,
    /// Copies printed or exported so far.
    pub outputs: i64,
}

/// Which return notes a search shows: the till's, to customers (`sales.view_all`), and those to
/// suppliers, which carry purchase costs (`purchases.view`). Both are `return_note`s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Returns {
    pub sales: bool,
    pub supplier: bool,
}

impl Returns {
    pub const ALL: Returns = Returns {
        sales: true,
        supplier: true,
    };

    /// Whether a return note to a supplier (`supplier`) or to a customer is shown.
    pub const fn shows(self, supplier: bool) -> bool {
        if supplier { self.supplier } else { self.sales }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArchivePage {
    pub rows: Vec<ArchiveRow>,
    pub next: Option<ArchiveCursor>,
}

/// What the search box holds, as a `LIKE` pattern.
enum Text {
    /// The start of a number: two letters then a digit (`FA2026-00`, `tk2026-c1`).
    Number(String),
    /// Digits only: the end of a number (`45` finds `FA2026-000045`).
    Digits(String),
    /// Anything else: a customer, supplier or buyer name as its normalised words (spec §8), a
    /// JSON array; every word must appear, in any order, like the customer directory search.
    Name(String),
}

fn classify(text: &str) -> Result<Option<Text>, AppError> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(None);
    }
    let upper = t.to_uppercase();
    let mut chars = upper.chars();
    let number_prefix = chars.next().is_some_and(|c| c.is_ascii_uppercase())
        && chars.next().is_some_and(|c| c.is_ascii_uppercase())
        && chars.next().is_some_and(|c| c.is_ascii_digit());
    if number_prefix {
        return Ok(Some(Text::Number(format!("{upper}%"))));
    }
    if t.chars().all(|c| c.is_ascii_digit()) {
        return Ok(Some(Text::Digits(format!("%{t}"))));
    }
    // Normalised words hold only letters and digits, never a `LIKE` wildcard.
    let words = normalize::words(t);
    if words.is_empty() {
        return Ok(None);
    }
    Ok(Some(Text::Name(serde_json::to_string(&words)?)))
}

const SORT_AT: &str = "COALESCE(d.validated_at, d.created_at)";

/// One page of the archive, newest first (`COALESCE(validated_at, created_at)`, then id, both
/// descending); `next` is the cursor of the following page when there is one. A return note is
/// listed when `returns` shows its side (to a customer or to a supplier).
pub fn search(
    conn: &Connection,
    q: &ArchiveQuery,
    today: &str,
    returns: Returns,
) -> Result<ArchivePage, AppError> {
    if q.kinds.is_empty() {
        return Ok(ArchivePage {
            rows: Vec::new(),
            next: None,
        });
    }
    let limit = q.limit.unwrap_or(100).clamp(1, MAX_LIMIT);
    let kinds = serde_json::to_string(&q.kinds)?;
    let text = match q.text.as_deref() {
        Some(t) => classify(t)?,
        None => None,
    };
    let (number, digits, words) = match text {
        Some(Text::Number(p)) => (Some(p), None, None),
        Some(Text::Digits(p)) => (None, Some(p), None),
        Some(Text::Name(p)) => (None, None, Some(p)),
        None => (None, None, None),
    };
    let before_at = q.before.as_ref().map(|c| c.sort_at);
    let before_id = q.before.as_ref().map(|c| c.id.as_str());
    let fetch = i64::from(limit) + 1;
    let sql = format!(
        "SELECT d.id, d.doc_type, d.number, d.status, d.business_date, {SORT_AT},
                COALESCE(bp.name, c.name, s.name), d.total_ttc, d.total_due, d.origin, p.number,
                (SELECT COUNT(*) FROM audit_log a WHERE a.entity_type = 'document' AND a.entity_id = d.id
                   AND a.action IN ('document.printed', 'document.reprinted', 'document.exported'))
         FROM documents d
         LEFT JOIN document_parties bp ON bp.document_id = d.id AND bp.role = 'buyer'
         LEFT JOIN customers c ON c.id = d.customer_id
         LEFT JOIN suppliers s ON s.id = d.supplier_id
         LEFT JOIN documents p ON p.id = d.parent_id
         WHERE d.doc_type IN (SELECT value FROM json_each(:kinds))
           AND (d.doc_type <> 'return_note'
                OR (d.supplier_id IS NULL AND :sales_returns)
                OR (d.supplier_id IS NOT NULL AND :supplier_returns))
           AND (:status IS NULL OR d.status = :status)
           AND (:customer_id IS NULL OR d.customer_id = :customer_id)
           AND (:supplier_id IS NULL OR d.supplier_id = :supplier_id)
           AND (:day_from IS NULL OR d.business_date >= :day_from)
           AND (:day_to IS NULL OR d.business_date <= :day_to)
           AND (:min_total IS NULL OR d.total_due >= :min_total)
           AND (:max_total IS NULL OR d.total_due <= :max_total)
           AND (:number IS NULL OR d.number LIKE :number)
           AND (:digits IS NULL OR d.number LIKE :digits)
           AND (:words IS NULL OR NOT EXISTS (
                 SELECT 1 FROM json_each(:words) w
                 WHERE NOT (COALESCE(c.search_text, '') LIKE '%' || w.value || '%'
                         OR COALESCE(s.search_text, '') LIKE '%' || w.value || '%'
                         OR COALESCE(bp.name, '') LIKE '%' || w.value || '%')))
           AND (:before_at IS NULL OR {SORT_AT} < :before_at OR ({SORT_AT} = :before_at AND d.id < :before_id))
         ORDER BY {SORT_AT} DESC, d.id DESC
         LIMIT :fetch"
    );
    let params: [(&str, &dyn ToSql); 16] = [
        (":kinds", &kinds),
        (":sales_returns", &returns.sales),
        (":supplier_returns", &returns.supplier),
        (":status", &q.status),
        (":customer_id", &q.customer_id),
        (":supplier_id", &q.supplier_id),
        (":day_from", &q.from),
        (":day_to", &q.to),
        (":min_total", &q.min_total),
        (":max_total", &q.max_total),
        (":number", &number),
        (":digits", &digits),
        (":words", &words),
        (":before_at", &before_at),
        (":before_id", &before_id),
        (":fetch", &fetch),
    ];
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt
        .query_map(params.as_slice(), |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, DocType>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, DocStatus>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, Money>(7)?,
                r.get::<_, Money>(8)?,
                r.get::<_, Option<DocOrigin>>(9)?,
                r.get::<_, Option<String>>(10)?,
                r.get::<_, i64>(11)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let page_len = usize::try_from(limit).unwrap_or(usize::MAX);
    let more = rows.len() > page_len;
    rows.truncate(page_len);
    let mut out = Vec::with_capacity(rows.len());
    for (
        id,
        doc_type,
        number,
        status,
        business_date,
        sort_at,
        party_name,
        total_ttc,
        total_due,
        origin,
        parent_number,
        outputs,
    ) in rows
    {
        let doc = documents::get(conn, &id)?;
        out.push(ArchiveRow {
            state: state_of(conn, &doc, today)?,
            id,
            doc_type,
            number,
            status,
            business_date,
            sort_at,
            party_name,
            total_ttc,
            total_due,
            origin,
            parent_number,
            outputs,
        });
    }
    let next = if more {
        out.last().map(|r| ArchiveCursor {
            sort_at: r.sort_at,
            id: r.id.clone(),
        })
    } else {
        None
    };
    Ok(ArchivePage { rows: out, next })
}

/// Archive rows as the table of their export (xlsx, csv, PDF): number, kind, date, customer or
/// supplier, TTC, net to pay and state, the kind and the state as codes translated by the sheet.
pub fn table(rows: &[ArchiveRow]) -> Result<Table, AppError> {
    let mut table = Table::new(&[
        ("number", ColumnKind::Code),
        ("docType", ColumnKind::Tag),
        ("businessDate", ColumnKind::Date),
        ("party", ColumnKind::Text),
        ("totalTtc", ColumnKind::Money),
        ("totalDue", ColumnKind::Money),
        ("state", ColumnKind::Tag),
    ])
    .tag("docType", TagDomain::DocType)
    .tag("state", TagDomain::DocState);
    for row in rows {
        table.push(vec![
            Cell::opt_text(row.number.clone()),
            Cell::text(row.doc_type.as_str()),
            Cell::text(row.business_date.clone()),
            Cell::opt_text(row.party_name.clone()),
            Cell::money(row.total_ttc),
            Cell::money(row.total_due),
            Cell::text(row.state.tag()),
        ])?;
    }
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing::drafts::tests::{cart, fixture, input, save};
    use crate::billing::validate::{ValidateInput, validate};
    use crate::customers::CustomerInput;
    use crate::documents::Document;
    use crate::money::Qty;
    use crate::sales::cart::CartLine;
    use crate::testkit::{Shop, shop};

    fn validated(shop: &Shop, kind: DocType, customer: &str, product: &str, qty: i64) -> Document {
        let draft = save(
            shop,
            None,
            input(kind, {
                let mut c = cart(None, vec![CartLine::product(product, Qty(qty))]);
                c.customer_id = Some(customer.to_owned());
                c
            }),
        )
        .unwrap();
        let ctx = shop.owner_till();
        shop.db
            .write(move |tx| {
                validate(
                    tx,
                    &ctx,
                    ValidateInput {
                        id: draft.id,
                        payments: Vec::new(),
                    },
                )
            })
            .unwrap()
            .document
    }

    fn find(shop: &Shop, q: ArchiveQuery) -> ArchivePage {
        shop.db
            .read(move |c| search(c, &q, "2026-09-28", Returns::ALL))
            .unwrap()
    }

    fn all_kinds() -> ArchiveQuery {
        ArchiveQuery {
            kinds: crate::billing::BILLING_KINDS.to_vec(),
            ..ArchiveQuery::default()
        }
    }

    #[test]
    fn the_archive_finds_by_number_name_amount_kind_and_pages() {
        let shop = shop();
        let (amine, cement, sand) = fixture(&shop);
        let (actor, pack) = (shop.owner.actor(), shop.pack.clone());
        let arab = shop
            .db
            .write(move |tx| {
                let mut i = CustomerInput::named("مؤسسة الأمل للبناء");
                i.address = Some("حي النصر، سطيف".into());
                crate::customers::create(tx, &actor, &pack, i)
            })
            .unwrap();
        let q1 = validated(&shop, DocType::Quote, &amine.id, &cement.id, 1_000);
        let q2 = validated(&shop, DocType::Quote, &arab.id, &sand.id, 3_000);
        let bl = validated(&shop, DocType::DeliveryNote, &amine.id, &cement.id, 2_000);
        let draft = save(
            &shop,
            None,
            input(
                DocType::Proforma,
                cart(
                    Some(&amine),
                    vec![CartLine::product(&cement.id, Qty(9_000))],
                ),
            ),
        )
        .unwrap();

        let page = find(&shop, all_kinds());
        let mut ids: Vec<&str> = page.rows.iter().map(|r| r.id.as_str()).collect();
        let mut expected = vec![
            draft.id.as_str(),
            bl.id.as_str(),
            q2.id.as_str(),
            q1.id.as_str(),
        ];
        ids.sort_unstable();
        expected.sort_unstable();
        assert_eq!(ids, expected, "every kind asked, drafts included");
        let row = |id: &str| page.rows.iter().find(|r| r.id == id).unwrap();
        assert_eq!(row(&bl.id).state, DocState::ToInvoice);
        assert_eq!(row(&q1.id).party_name.as_deref(), Some("SARL Amine"));
        assert_eq!(row(&draft.id).state, DocState::Draft);

        let by = |text: &str| {
            find(
                &shop,
                ArchiveQuery {
                    text: Some(text.into()),
                    ..all_kinds()
                },
            )
            .rows
            .into_iter()
            .map(|r| r.number)
            .collect::<Vec<_>>()
        };
        assert_eq!(by("dv2026-000002"), [Some("DV2026-000002".to_owned())]);
        assert_eq!(
            by("2"),
            [Some("DV2026-000002".to_owned())],
            "digits end a number"
        );
        assert_eq!(
            by("موسسة"),
            [Some("DV2026-000002".to_owned())],
            "Arabic normalised: ؤ → و"
        );
        assert_eq!(by("amine").len(), 3);
        assert_eq!(
            by("مؤسسة الأمل للبناء"),
            [Some("DV2026-000002".to_owned())],
            "the full name, although the search text holds امل right after الامل"
        );
        assert_eq!(by("amine sarl").len(), 3, "every word, in any order");
        assert_eq!(by("« — »").len(), 4, "nothing searchable typed: no filter");

        let bl_only = find(
            &shop,
            ArchiveQuery {
                kinds: vec![DocType::DeliveryNote],
                ..ArchiveQuery::default()
            },
        );
        assert_eq!(bl_only.rows.len(), 1);
        let amounts = find(
            &shop,
            ArchiveQuery {
                min_total: Some(Money(1_000_000)),
                ..all_kinds()
            },
        );
        assert_eq!(
            amounts.rows.len(),
            2,
            "the 9-bag proforma draft and the 3 m3 of sand"
        );
        let nothing = find(&shop, ArchiveQuery::default());
        assert!(nothing.rows.is_empty(), "no kind, nothing");

        let first = find(
            &shop,
            ArchiveQuery {
                limit: Some(2),
                ..all_kinds()
            },
        );
        assert_eq!(first.rows.len(), 2);
        let second = find(
            &shop,
            ArchiveQuery {
                limit: Some(2),
                before: first.next.clone(),
                ..all_kinds()
            },
        );
        assert_eq!(second.rows.len(), 2);
        assert!(second.next.is_none());
        let mut paged: Vec<&str> = first
            .rows
            .iter()
            .chain(&second.rows)
            .map(|r| r.id.as_str())
            .collect();
        paged.sort_unstable();
        assert_eq!(
            paged, expected,
            "the pages cover every row once, even at the same instant"
        );
    }

    #[test]
    fn archive_rows_export_as_a_table_of_codes_and_centimes() {
        let shop = shop();
        let (amine, cement, _) = fixture(&shop);
        let quote = validated(&shop, DocType::Quote, &amine.id, &cement.id, 1_000);
        let draft = save(
            &shop,
            None,
            input(
                DocType::Proforma,
                cart(
                    Some(&amine),
                    vec![CartLine::product(&cement.id, Qty(2_000))],
                ),
            ),
        )
        .unwrap();
        let page = find(&shop, all_kinds());
        let t = table(&page.rows).unwrap();
        let keys: Vec<&str> = t.columns.iter().map(|c| c.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "number",
                "docType",
                "businessDate",
                "party",
                "totalTtc",
                "totalDue",
                "state"
            ]
        );
        assert_eq!(t.columns[1].tag, Some(TagDomain::DocType));
        assert_eq!(t.columns[6].tag, Some(TagDomain::DocState));
        let row = |id: &str| &t.rows[page.rows.iter().position(|r| r.id == id).unwrap()];
        assert_eq!(
            row(&quote.id),
            &vec![
                Cell::text(quote.number.clone().unwrap()),
                Cell::text("quote"),
                Cell::text(quote.business_date.clone()),
                Cell::text("SARL Amine"),
                Cell::money(quote.total_ttc),
                Cell::money(quote.total_due),
                Cell::text("open"),
            ]
        );
        assert_eq!(row(&draft.id)[0], Cell::NULL, "a draft has no number yet");
        assert_eq!(row(&draft.id)[6], Cell::text("draft"));
    }
}

//! Serial numbers and IMEIs (PRD-050, STK-004): registered when goods come in, linked to the
//! ticket line that sold them (the till prompt is SAL-025, a phone-pack feature), returned by a
//! customer or sent back to the supplier. A register, not a ledger: status changes are audited.

use crate::audit::{self, Actor};
use crate::catalog::extras;
use crate::documents::{self, DocStatus, DocType};
use crate::error::AppError;
use crate::ids::new_id;
use crate::sqlenum::sql_text_enum;
use crate::time::now_ms;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SerialStatus {
    InStock,
    Sold,
    Returned,
    ReturnedToSupplier,
}

impl SerialStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            SerialStatus::InStock => "in_stock",
            SerialStatus::Sold => "sold",
            SerialStatus::Returned => "returned",
            SerialStatus::ReturnedToSupplier => "returned_to_supplier",
        }
    }

    pub fn parse(s: &str) -> Option<SerialStatus> {
        [
            SerialStatus::InStock,
            SerialStatus::Sold,
            SerialStatus::Returned,
            SerialStatus::ReturnedToSupplier,
        ]
        .into_iter()
        .find(|v| v.as_str() == s)
    }
}

sql_text_enum!(SerialStatus);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Serial {
    pub id: String,
    pub product_id: String,
    pub serial_no: String,
    pub status: SerialStatus,
    pub location_id: Option<String>,
    pub received_document_id: Option<String>,
    pub sale_line_id: Option<String>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SerialQuery {
    pub product_id: Option<String>,
    pub status: Option<SerialStatus>,
    pub text: String,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SerialLink {
    pub line_id: String,
    pub serial_no: String,
}

const SELECT: &str =
    "SELECT id, product_id, serial_no, status, location_id, received_document_id, sale_line_id,
        updated_at FROM serials";

fn map(r: &Row<'_>) -> rusqlite::Result<Serial> {
    Ok(Serial {
        id: r.get(0)?,
        product_id: r.get(1)?,
        serial_no: r.get(2)?,
        status: r.get(3)?,
        location_id: r.get(4)?,
        received_document_id: r.get(5)?,
        sale_line_id: r.get(6)?,
        updated_at: r.get(7)?,
    })
}

/// A 15-digit IMEI whose last digit is its Luhn check digit (VRT-070 at the sale).
pub fn imei_ok(s: &str) -> bool {
    let digits: Vec<u32> = s.chars().filter_map(|c| c.to_digit(10)).collect();
    if digits.len() != 15 || s.chars().count() != 15 {
        return false;
    }
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, d)| {
            if i % 2 == 1 {
                let x = d * 2;
                if x > 9 { x - 9 } else { x }
            } else {
                *d
            }
        })
        .sum();
    sum.is_multiple_of(10)
}

/// The number as it is stored: trimmed, Western digits, no inner spaces (a scanned or typed
/// `3569 3803…` is the same serial).
pub(crate) fn clean_no(serial_no: &str) -> Result<String, AppError> {
    let s: String = crate::party::western_digits(serial_no.trim())
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if s.is_empty() || s.chars().count() > 40 || s.chars().any(char::is_control) {
        return Err(AppError::validation("serials", "value_invalid"));
    }
    Ok(s)
}

fn by_id(conn: &Connection, id: &str) -> Result<Serial, AppError> {
    conn.prepare_cached(&format!("{SELECT} WHERE id = ?1"))?
        .query_row([id], map)
        .optional()?
        .ok_or_else(|| AppError::not_found("serial"))
}

pub fn register(
    tx: &Transaction<'_>,
    product_id: &str,
    serial_no: &str,
    location_id: Option<&str>,
    received_document_id: Option<&str>,
    now: i64,
) -> Result<Serial, AppError> {
    let no = clean_no(serial_no)?;
    let taken: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM serials WHERE product_id = ?1 AND serial_no = ?2)",
        params![product_id, no],
        |r| r.get(0),
    )?;
    if taken {
        return Err(AppError::validation("serials", "serial_taken"));
    }
    let id = new_id();
    tx.execute(
        "INSERT INTO serials (id, product_id, serial_no, location_id, received_document_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        params![id, product_id, no, location_id, received_document_id, now],
    )?;
    by_id(tx, &id)
}

/// The serial numbers of pieces already on hand at `location_id` (the default location when
/// `None`) that came in without them (opening stock, an adjustment): only for a product tracked
/// by serial number, and never more serials in stock there than its whole pieces on hand
/// (`serials_count_mismatch`). One `serial.registered` row lists them.
pub fn register_on_hand(
    tx: &Transaction<'_>,
    actor: &Actor,
    product_id: &str,
    location_id: Option<&str>,
    numbers: &[String],
    now: i64,
) -> Result<Vec<Serial>, AppError> {
    crate::catalog::products::get(tx, product_id)?;
    if !extras::get(tx, product_id)?.track_serials {
        return Err(AppError::validation("productId", "serials_count_mismatch"));
    }
    if numbers.is_empty() {
        return Err(AppError::validation("numbers", "required"));
    }
    let location = match location_id {
        Some(id) => {
            super::location(tx, id)
                .map_err(|_| AppError::validation("locationId", "stock_location_not_found"))?
                .id
        }
        None => super::default_location(tx)?.id,
    };
    let pieces = super::level(tx, product_id, &location)?.0 / 1_000;
    let registered: i64 = tx.query_row(
        "SELECT COUNT(*) FROM serials WHERE product_id = ?1 AND location_id = ?2 AND status = 'in_stock'",
        params![product_id, location],
        |r| r.get(0),
    )?;
    let wanted = i64::try_from(numbers.len()).unwrap_or(i64::MAX);
    if registered.saturating_add(wanted) > pieces {
        return Err(AppError::validation("numbers", "serials_count_mismatch"));
    }
    let mut made = Vec::with_capacity(numbers.len());
    for (i, number) in numbers.iter().enumerate() {
        let serial =
            register(tx, product_id, number, Some(&location), None, now).map_err(|e| match e {
                AppError::Validation { code, .. } => {
                    AppError::validation(&format!("numbers.{i}"), &code)
                }
                other => other,
            })?;
        made.push(serial);
    }
    let serials: Vec<&str> = made.iter().map(|s| s.serial_no.as_str()).collect();
    audit::append(
        tx,
        &actor.event(
            "serial.registered",
            "product",
            Some(product_id),
            json!({ "locationId": location, "serials": serials }),
        ),
    )?;
    Ok(made)
}

pub fn list(conn: &Connection, q: &SerialQuery) -> Result<Vec<Serial>, AppError> {
    let text = q.text.trim();
    let mut stmt = conn.prepare_cached(&format!(
        "{SELECT} WHERE (?1 IS NULL OR product_id = ?1) AND (?2 IS NULL OR status = ?2)
           AND (?3 = '' OR serial_no LIKE '%' || ?3 || '%') ORDER BY updated_at DESC LIMIT ?4"
    ))?;
    let rows = stmt
        .query_map(
            params![
                q.product_id,
                q.status,
                text,
                q.limit.unwrap_or(500).clamp(1, 5_000)
            ],
            map,
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn find(conn: &Connection, serial_no: &str) -> Result<Vec<Serial>, AppError> {
    let no = clean_no(serial_no)?;
    let mut stmt = conn.prepare_cached(&format!("{SELECT} WHERE serial_no = ?1"))?;
    let rows = stmt.query_map([no], map)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Links serials to the lines of a validated ticket that sold them: only lines of a product
/// tracked by serial number, and never more serials on a line than its whole pieces (those
/// linked by earlier calls and earlier in this one included), else `serials_count_mismatch`.
pub fn link_sale(
    tx: &Transaction<'_>,
    actor: &Actor,
    document_id: &str,
    links: Vec<SerialLink>,
) -> Result<Vec<Serial>, AppError> {
    let doc = documents::get(tx, document_id)?;
    if doc.doc_type != DocType::Ticket || doc.status != DocStatus::Validated {
        return Err(AppError::validation("documentId", "document_not_validated"));
    }
    let mut out = Vec::new();
    for (i, link) in links.iter().enumerate() {
        let f = |n: &str| format!("links.{i}.{n}");
        let line = doc
            .lines
            .iter()
            .find(|l| l.id == link.line_id)
            .ok_or_else(|| AppError::validation(&f("lineId"), "return_line_invalid"))?;
        let product_id = match &line.product_id {
            Some(id) if extras::get(tx, id)?.track_serials => id.clone(),
            _ => return Err(AppError::validation(&f("lineId"), "serials_count_mismatch")),
        };
        // Links made earlier in this call are already written, so they are in the count.
        let linked: i64 = tx.query_row(
            "SELECT COUNT(*) FROM serials WHERE sale_line_id = ?1",
            [&line.id],
            |r| r.get(0),
        )?;
        if linked >= line.qty.0 / 1_000 {
            return Err(AppError::validation(
                &f("serialNo"),
                "serials_count_mismatch",
            ));
        }
        let no = clean_no(&link.serial_no)?;
        let serial = tx
            .prepare_cached(&format!(
                "{SELECT} WHERE product_id = ?1 AND serial_no = ?2"
            ))?
            .query_row(params![product_id, no], map)
            .optional()?
            .ok_or_else(|| AppError::validation(&f("serialNo"), "serial_not_found"))?;
        if serial.status != SerialStatus::InStock {
            return Err(AppError::validation(
                &f("serialNo"),
                "serial_status_invalid",
            ));
        }
        tx.execute(
            "UPDATE serials SET status = 'sold', sale_line_id = ?2, location_id = NULL, updated_at = ?3 WHERE id = ?1",
            params![serial.id, line.id, now_ms()],
        )?;
        audit::append(
            tx,
            &actor.event(
                "serial.sold",
                "serial",
                Some(&serial.id),
                json!({ "serialNo": serial.serial_no, "documentId": document_id, "lineId": line.id }),
            ),
        )?;
        out.push(by_id(tx, &serial.id)?);
    }
    Ok(out)
}

/// What [`link_sale`] refuses, without linking (a till's ticket, Its serials are the
/// main PC's to link): the same checks, the earlier links of `links` counted on their line and
/// a serial named twice refused. `waiting` are the links of this till's tickets the main PC has
/// not made yet (Task 31 review): the till's copy still shows those serials in stock, and they
/// are refused as sold.
pub fn check_links(
    conn: &Connection,
    document_id: &str,
    links: &[SerialLink],
    waiting: &[SerialLink],
) -> Result<(), AppError> {
    let doc = documents::get(conn, document_id)?;
    if doc.doc_type != DocType::Ticket || doc.status != DocStatus::Validated {
        return Err(AppError::validation("documentId", "document_not_validated"));
    }
    let mut taken: Vec<(String, String)> = Vec::new();
    for link in waiting {
        let serial: Option<String> = conn
            .prepare_cached(
                "SELECT s.id FROM serials s JOIN document_lines l ON l.product_id = s.product_id
                 WHERE l.id = ?1 AND s.serial_no = ?2",
            )?
            .query_row(params![link.line_id, clean_no(&link.serial_no)?], |r| {
                r.get(0)
            })
            .optional()?;
        taken.extend(serial.map(|id| (link.line_id.clone(), id)));
    }
    for (i, link) in links.iter().enumerate() {
        let f = |n: &str| format!("links.{i}.{n}");
        let line = doc
            .lines
            .iter()
            .find(|l| l.id == link.line_id)
            .ok_or_else(|| AppError::validation(&f("lineId"), "return_line_invalid"))?;
        let product_id = match &line.product_id {
            Some(id) if extras::get(conn, id)?.track_serials => id.clone(),
            _ => return Err(AppError::validation(&f("lineId"), "serials_count_mismatch")),
        };
        let linked: i64 = conn.query_row(
            "SELECT COUNT(*) FROM serials WHERE sale_line_id = ?1",
            [&line.id],
            |r| r.get(0),
        )?;
        let earlier =
            i64::try_from(taken.iter().filter(|(l, _)| *l == line.id).count()).unwrap_or(i64::MAX);
        if linked.saturating_add(earlier) >= line.qty.0 / 1_000 {
            return Err(AppError::validation(
                &f("serialNo"),
                "serials_count_mismatch",
            ));
        }
        let no = clean_no(&link.serial_no)?;
        let serial = conn
            .prepare_cached(&format!(
                "{SELECT} WHERE product_id = ?1 AND serial_no = ?2"
            ))?
            .query_row(params![product_id, no], map)
            .optional()?
            .ok_or_else(|| AppError::validation(&f("serialNo"), "serial_not_found"))?;
        if serial.status != SerialStatus::InStock || taken.iter().any(|(_, s)| *s == serial.id) {
            return Err(AppError::validation(
                &f("serialNo"),
                "serial_status_invalid",
            ));
        }
        taken.push((line.id.clone(), serial.id));
    }
    Ok(())
}

/// A sold serial comes back (a return): back in stock, or kept aside as returned.
pub fn mark_returned(
    tx: &Transaction<'_>,
    actor: &Actor,
    serial_id: &str,
    restock: bool,
) -> Result<Serial, AppError> {
    let serial = by_id(tx, serial_id)?;
    if serial.status != SerialStatus::Sold {
        return Err(AppError::validation("serialId", "serial_status_invalid"));
    }
    let (status, location) = if restock {
        ("in_stock", Some(super::SHOP_LOCATION_ID))
    } else {
        ("returned", None)
    };
    tx.execute(
        "UPDATE serials SET status = ?2, location_id = ?3, updated_at = ?4 WHERE id = ?1",
        params![serial_id, status, location, now_ms()],
    )?;
    audit::append(
        tx,
        &actor.event(
            "serial.returned",
            "serial",
            Some(serial_id),
            json!({ "serialNo": serial.serial_no, "restock": restock }),
        ),
    )?;
    by_id(tx, serial_id)
}

/// A serial in stock moves with a transfer to another location (the transfer document audits).
pub fn relocate(
    tx: &Transaction<'_>,
    serial_id: &str,
    location_id: &str,
    now: i64,
) -> Result<Serial, AppError> {
    let serial = by_id(tx, serial_id)?;
    if serial.status != SerialStatus::InStock {
        return Err(AppError::validation("serials", "serial_status_invalid"));
    }
    tx.execute(
        "UPDATE serials SET location_id = ?2, updated_at = ?3 WHERE id = ?1",
        params![serial_id, location_id, now],
    )?;
    by_id(tx, serial_id)
}

/// A serial leaves with a supplier return (the return document audits).
pub fn send_back(tx: &Transaction<'_>, serial_id: &str, now: i64) -> Result<Serial, AppError> {
    let serial = by_id(tx, serial_id)?;
    if serial.status != SerialStatus::InStock {
        return Err(AppError::validation("serials", "serial_status_invalid"));
    }
    tx.execute(
        "UPDATE serials SET status = 'returned_to_supplier', location_id = NULL, updated_at = ?2 WHERE id = ?1",
        params![serial_id, now],
    )?;
    by_id(tx, serial_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::extras::{self, ProductExtrasInput};
    use crate::lang::Lang;
    use crate::money::{Money, Qty};
    use crate::payments::{PaymentInput, PaymentMethod};
    use crate::sales::{Cart, CartLine, SaleInput, create_sale};
    use crate::stock::{ReceiveLine, SHOP_LOCATION_ID, receive};
    use crate::testkit::shop;

    #[test]
    fn serials_come_in_with_the_receipt_go_out_with_a_ticket_and_come_back() {
        let shop = shop();
        let phone = shop.product("Redmi Note 13", Money(4_500_000), &shop.tva19);
        let actor = shop.owner.actor();
        let id = phone.id.clone();
        shop.db
            .write(move |tx| {
                extras::set(
                    tx,
                    &actor,
                    &id,
                    ProductExtrasInput {
                        track_serials: true,
                        ..ProductExtrasInput::default()
                    },
                )
            })
            .unwrap();
        let line = |serials: Vec<&str>| ReceiveLine {
            product_id: phone.id.clone(),
            location_id: SHOP_LOCATION_ID.into(),
            qty: Qty(2_000),
            unit_cost: Money(3_800_000),
            lot: None,
            serials: serials.into_iter().map(str::to_owned).collect(),
            supplier_id: None,
            // Opening stock: no receiving document here (`received_document_id` is a foreign key).
            ref_type: "opening".into(),
            ref_id: "rc1".into(),
            note: None,
        };
        let actor = shop.owner.actor();
        let short = line(vec!["356789101112131"]);
        assert_eq!(
            shop.db
                .write(move |tx| receive(tx, &actor, &short, 1))
                .unwrap_err(),
            AppError::validation("serials", "serials_count_mismatch")
        );
        let actor = shop.owner.actor();
        let full = line(vec!["356789101112131", "356789101112149"]);
        shop.db
            .write(move |tx| receive(tx, &actor, &full, 1))
            .unwrap();
        let actor = shop.owner.actor();
        let again = line(vec!["356789101112131", "356789101112156"]);
        assert_eq!(
            shop.db
                .write(move |tx| receive(tx, &actor, &again, 1))
                .unwrap_err(),
            AppError::validation("serials", "serial_taken")
        );
        shop.open_till(0);
        let ctx = shop.till();
        let input = SaleInput {
            cart: Cart::new(Lang::Fr, vec![CartLine::product(&phone.id, Qty(1_000))]),
            // 45 000 DA in cash: 450 tranches × 1.5 DA = 675 DA of duty.
            payments: vec![PaymentInput::cash(Money(4_567_500))],
        };
        let sale = shop
            .db
            .write(move |tx| create_sale(tx, &ctx, input))
            .unwrap();
        let links = vec![SerialLink {
            line_id: sale.document.lines[0].id.clone(),
            serial_no: "356789101112131".into(),
        }];
        let actor = shop.owner.actor();
        let doc = sale.document.id.clone();
        let sold = shop
            .db
            .write(move |tx| link_sale(tx, &actor, &doc, links))
            .unwrap();
        assert_eq!(sold[0].status, SerialStatus::Sold);
        let found = shop.db.read(|c| find(c, "356789101112131")).unwrap();
        assert_eq!(
            found[0].sale_line_id.as_deref(),
            Some(sale.document.lines[0].id.as_str())
        );
        let actor = shop.owner.actor();
        let serial = found[0].id.clone();
        let back = shop
            .db
            .write(move |tx| mark_returned(tx, &actor, &serial, true))
            .unwrap();
        assert_eq!(back.status, SerialStatus::InStock);
        assert_eq!(
            shop.count(
                "SELECT COUNT(*) FROM audit_log WHERE action IN ('serial.sold', 'serial.returned')"
            ),
            2
        );
    }

    #[test]
    fn a_line_takes_no_more_serials_than_its_pieces_and_only_for_a_tracked_product() {
        let shop = shop();
        let phone = shop.product("Redmi Note 13", Money(4_500_000), &shop.tva19);
        let actor = shop.owner.actor();
        let id = phone.id.clone();
        shop.db
            .write(move |tx| {
                extras::set(
                    tx,
                    &actor,
                    &id,
                    ProductExtrasInput {
                        track_serials: true,
                        ..ProductExtrasInput::default()
                    },
                )
            })
            .unwrap();
        let received = ReceiveLine {
            product_id: phone.id.clone(),
            location_id: SHOP_LOCATION_ID.into(),
            qty: Qty(2_000),
            unit_cost: Money(3_800_000),
            lot: None,
            serials: vec!["356789101112131".into(), "356789101112149".into()],
            supplier_id: None,
            ref_type: "opening".into(),
            ref_id: "rc1".into(),
            note: None,
        };
        let actor = shop.owner.actor();
        shop.db
            .write(move |tx| receive(tx, &actor, &received, 1))
            .unwrap();
        let case = shop.product("Coque Redmi", Money(50_000), &shop.tva19);
        shop.stock_in(&case.id, 5_000);
        shop.open_till(0);
        let ctx = shop.till();
        let input = SaleInput {
            cart: Cart::new(
                Lang::Fr,
                vec![
                    CartLine::product(&phone.id, Qty(1_000)),
                    CartLine::product(&case.id, Qty(1_000)),
                ],
            ),
            payments: vec![PaymentInput::electronic(
                PaymentMethod::CardCib,
                Money(4_550_000),
                Some("A1"),
            )],
        };
        let sale = shop
            .db
            .write(move |tx| create_sale(tx, &ctx, input))
            .unwrap();
        let line_of = |product: &str| {
            sale.document
                .lines
                .iter()
                .find(|l| l.product_id.as_deref() == Some(product))
                .map(|l| l.id.clone())
                .unwrap()
        };
        let (phone_line, case_line) = (line_of(&phone.id), line_of(&case.id));
        let link = |links: Vec<(&str, &str)>| {
            let links: Vec<SerialLink> = links
                .into_iter()
                .map(|(line_id, serial_no)| SerialLink {
                    line_id: line_id.into(),
                    serial_no: serial_no.into(),
                })
                .collect();
            let actor = shop.owner.actor();
            let doc = sale.document.id.clone();
            shop.db.write(move |tx| link_sale(tx, &actor, &doc, links))
        };
        // One phone sold: two serials on its line are refused, and nothing is sold.
        assert_eq!(
            link(vec![
                (&phone_line, "356789101112131"),
                (&phone_line, "356789101112149")
            ])
            .unwrap_err(),
            AppError::validation("links.1.serialNo", "serials_count_mismatch")
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM serials WHERE status = 'in_stock'"),
            2
        );
        link(vec![(&phone_line, "356789101112131")]).unwrap();
        // The line already has its serial: a second call cannot add another.
        assert_eq!(
            link(vec![(&phone_line, "356789101112149")]).unwrap_err(),
            AppError::validation("links.0.serialNo", "serials_count_mismatch")
        );
        // A product that is not tracked by serial number takes none.
        assert_eq!(
            link(vec![(&case_line, "356789101112149")]).unwrap_err(),
            AppError::validation("links.0.lineId", "serials_count_mismatch")
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM serials WHERE status = 'in_stock'"),
            1
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'serial.sold'"),
            1
        );
    }

    /// Phones counted into the opening stock have no receipt: their IMEIs are recorded later,
    /// never more than the whole pieces on hand at the location, in one journaled operation.
    #[test]
    fn serials_of_stock_already_on_hand_are_registered_up_to_its_pieces() {
        let shop = shop();
        let phone = shop.product("Redmi Note 13", Money(4_500_000), &shop.tva19);
        let case = shop.product("Coque Redmi", Money(50_000), &shop.tva19);
        let actor = shop.owner.actor();
        let id = phone.id.clone();
        shop.db
            .write(move |tx| {
                extras::set(
                    tx,
                    &actor,
                    &id,
                    ProductExtrasInput {
                        track_serials: true,
                        ..ProductExtrasInput::default()
                    },
                )
            })
            .unwrap();
        shop.stock_in(&phone.id, 2_000);
        shop.stock_in(&case.id, 3_000);
        let register = |product_id: &str, numbers: &[&str]| {
            let actor = shop.owner.actor();
            let product_id = product_id.to_owned();
            let numbers: Vec<String> = numbers.iter().map(|n| (*n).to_owned()).collect();
            shop.db
                .write(move |tx| register_on_hand(tx, &actor, &product_id, None, &numbers, 1))
        };
        assert_eq!(
            register("00000000-0000-7000-8000-00000000dead", &["C-1"]).unwrap_err(),
            AppError::not_found("product")
        );
        assert_eq!(
            register(&case.id, &["C-1"]).unwrap_err(),
            AppError::validation("productId", "serials_count_mismatch"),
            "only a product tracked by serial number"
        );
        assert_eq!(
            register(&phone.id, &[]).unwrap_err(),
            AppError::validation("numbers", "required")
        );
        assert_eq!(
            register(&phone.id, &["356789101112131", "356789101112131"]).unwrap_err(),
            AppError::validation("numbers.1", "serial_taken"),
            "the same number twice"
        );
        let made = register(&phone.id, &["356789101112131", " 3567 8910 1112 149 "]).unwrap();
        assert_eq!(made.len(), 2);
        assert_eq!(made[1].serial_no, "356789101112149");
        assert_eq!(made[0].status, SerialStatus::InStock);
        assert_eq!(made[0].location_id.as_deref(), Some(SHOP_LOCATION_ID));
        assert_eq!(made[0].received_document_id, None);
        assert_eq!(
            register(&phone.id, &["356789101112156"]).unwrap_err(),
            AppError::validation("numbers", "serials_count_mismatch"),
            "two pieces on hand hold two serials, not three"
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'serial.registered'"),
            1,
            "one row per operation, none for a refused one"
        );
        assert_eq!(shop.count("SELECT COUNT(*) FROM serials"), 2);
    }
}

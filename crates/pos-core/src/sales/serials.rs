//! Serials and IMEIs at the till (SAL-025, PRD-050, VRT-070) and on returns: with the serials
//! module on, a line of a serial-tracked product names one serial per whole piece, each linked to
//! its ticket line in the sale's transaction (M2 `link_sale`, the one place a serial is sold); a
//! 15-digit number is an IMEI and passes the Luhn check; a warranted item needs its buyer
//! (SAL-043, the M3 warranty certificate names him). A return brings back serials sold on the
//! returned line only (M2 `mark_returned`): all of them when it returns every piece they cover,
//! else the ones it names (required while the serials module is on).

use super::SaleInput;
use super::extensions::{ReturnSerial, SaleSerial};
use crate::catalog::extras;
use crate::documents::Document;
use crate::error::AppError;
use crate::packs::{self, Module};
use crate::returns::ReturnInput;
use crate::stock::serials::{self, SerialLink, imei_ok};
use crate::till::TillContext;
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::json;

/// The stored form of a typed or scanned number; a 15-digit one must be a valid IMEI.
fn number(field: &str, raw: &str) -> Result<String, AppError> {
    let no = serials::clean_no(raw).map_err(|_| AppError::validation(field, "value_invalid"))?;
    if no.len() == 15 && no.bytes().all(|b| b.is_ascii_digit()) && !imei_ok(&no) {
        return Err(AppError::validation(field, "imei_invalid"));
    }
    Ok(no)
}

/// Whole pieces of a line (a serial is one piece).
fn pieces(qty: crate::money::Qty) -> usize {
    usize::try_from(qty.0 / 1_000).unwrap_or(0)
}

/// Checks the cart's serials and buyer, then links the serials to their ticket lines. On a till
/// (`on_till` holds the links its tickets still waiting for the main PC named; The
/// serials are the main PC's) they are only checked: the links come back for the main PC to make
/// once the ticket reached it (`extensions::record_host_registers`).
pub(crate) fn capture(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: &SaleInput,
    given: &[SaleSerial],
    document: &Document,
    main_line_ids: &[String],
    on_till: Option<&[SerialLink]>,
) -> Result<Vec<SerialLink>, AppError> {
    if packs::enabled(tx, Module::Serials)? {
        let mut missing = Vec::new();
        for (i, line) in input.cart.lines.iter().enumerate() {
            let Some(pid) = &line.product_id else {
                continue;
            };
            let e = extras::get(tx, pid)?;
            if e.warranty_months.is_some() && input.cart.customer_id.is_none() {
                return Err(AppError::validation("customerId", "buyer_required"));
            }
            if e.track_serials {
                let named = given.iter().filter(|s| s.line as usize == i).count();
                if named != pieces(line.qty) {
                    missing.push(i);
                }
            }
        }
        if !missing.is_empty() {
            return Err(AppError::rejected(
                json!({ "lines": missing }),
                "serials_required",
            ));
        }
    }
    let mut links = Vec::with_capacity(given.len());
    for (k, s) in given.iter().enumerate() {
        let serial_no = number(&format!("serials.{k}.serialNo"), &s.serial_no)?;
        let line_id = main_line_ids
            .get(s.line as usize)
            .ok_or_else(|| AppError::validation(&format!("serials.{k}.line"), "value_invalid"))?;
        links.push(SerialLink {
            line_id: line_id.clone(),
            serial_no,
        });
    }
    if links.is_empty() {
        return Ok(Vec::new());
    }
    // `link_sale` names its links as `links.{k}`; they are the cart's serials in the same order.
    let as_serials = |e: AppError| match e {
        AppError::Validation { field, code } => {
            AppError::validation(&field.replacen("links.", "serials.", 1), &code)
        }
        other => other,
    };
    if let Some(waiting) = on_till {
        serials::check_links(tx, &document.id, &links, waiting).map_err(as_serials)?;
        return Ok(links);
    }
    serials::link_sale(tx, &ctx.actor(), &document.id, links).map_err(as_serials)?;
    Ok(Vec::new())
}

/// The serials a return brings back, each in stock or aside with the line's `restock`. A returned
/// line that names no serial and returns as many whole pieces as serials still sold on its origin
/// line brings them all back (nothing to choose). Otherwise, with the serials module on, the line
/// names as many serials as whole pieces returned (`serials_required`, like the sale); with it
/// off (M2: serials linked after the sale), only the named serials come back. A named serial is
/// always one sold on that very line. The number of serials brought back.
pub(crate) fn bring_back(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: &ReturnInput,
    given: &[ReturnSerial],
) -> Result<u32, AppError> {
    let required = packs::enabled(tx, Module::Serials)?;
    let mut missing = Vec::new();
    let mut whole_lines = Vec::new();
    for (i, line) in input.lines.iter().enumerate() {
        let Some(origin) = &line.origin_line_id else {
            continue;
        };
        let sold: Vec<String> = tx
            .prepare_cached(
                "SELECT id FROM serials WHERE sale_line_id = ?1 AND status = 'sold' ORDER BY id",
            )?
            .query_map([origin], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        if sold.is_empty() {
            continue;
        }
        let named = given.iter().filter(|s| s.line as usize == i).count();
        if named == 0 && pieces(line.qty) == sold.len() {
            whole_lines.push((line.restock, sold));
        } else if required && named != pieces(line.qty) {
            missing.push(i);
        }
    }
    if !missing.is_empty() {
        return Err(AppError::rejected(
            json!({ "lines": missing }),
            "serials_required",
        ));
    }
    let actor = ctx.actor();
    let mut done = 0;
    for (restock, ids) in whole_lines {
        for id in ids {
            serials::mark_returned(tx, &actor, &id, restock)?;
            done += 1;
        }
    }
    for (k, s) in given.iter().enumerate() {
        let field = format!("serials.{k}.serialNo");
        let line = input
            .lines
            .get(s.line as usize)
            .ok_or_else(|| AppError::validation(&format!("serials.{k}.line"), "value_invalid"))?;
        let no = serials::clean_no(&s.serial_no)
            .map_err(|_| AppError::validation(&field, "value_invalid"))?;
        let origin = line.origin_line_id.as_deref().unwrap_or_default();
        let id: Option<String> = tx
            .prepare_cached(
                "SELECT id FROM serials WHERE serial_no = ?1 AND sale_line_id = ?2 AND status = 'sold'",
            )?
            .query_row(params![no, origin], |r| r.get(0))
            .optional()?;
        let Some(id) = id else {
            return Err(AppError::validation(&field, "serial_not_on_line"));
        };
        serials::mark_returned(tx, &actor, &id, line.restock)?;
        done += 1;
    }
    Ok(done)
}

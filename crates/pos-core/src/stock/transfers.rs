//! Transfers between locations (STK-005, DOC-009): a `BT…` document valued at the CMUP, the
//! goods leaving the origin (from the named lot, never beyond what it holds there, else FEFO)
//! and entering the destination in the same lots, their serial numbers with them, in one
//! transaction; the driver and the vehicle are kept for the printed bon de transfert (M3).

use super::lots::{self, LotTake};
use super::serials::{self, SerialStatus};
use super::{
    MovementReason, NewMovement, Shortage, StockNeed, append, append_with_lot, check_available,
    location,
};
use crate::audit;
use crate::catalog::products;
use crate::company::{self, PriceMode, TaxRegime};
use crate::documents::{self, DocType, Document, DraftDocument, DraftLine};
use crate::error::AppError;
use crate::ids::new_id;
use crate::money::{Money, Qty, Rate};
use crate::sales::cart::at_line;
use crate::tax::timbre::StampMode;
use crate::tax::tva::{self, Discount, LineInput};
use crate::till::TillContext;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TransferLine {
    pub product_id: String,
    pub qty: Qty,
    pub lot_number: Option<String>,
    /// The serial numbers of the pieces that move, each in stock at the origin: one per piece
    /// for a product tracked by serial number, never more than the pieces otherwise.
    #[serde(default)]
    pub serials: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TransferInput {
    pub from_location_id: String,
    pub to_location_id: String,
    pub lines: Vec<TransferLine>,
    pub driver_name: Option<String>,
    pub vehicle: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TransferDone {
    pub document: Document,
    pub from_location_id: String,
    pub to_location_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TransferSummary {
    pub id: String,
    pub number: Option<String>,
    pub business_date: String,
    pub from_location_id: String,
    pub to_location_id: String,
    pub lines: i64,
    pub value: Money,
    pub validated_at: Option<i64>,
    pub user_id: String,
}

pub fn transfer(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: TransferInput,
) -> Result<TransferDone, AppError> {
    if input.from_location_id == input.to_location_id {
        return Err(AppError::validation(
            "toLocationId",
            "transfer_same_location",
        ));
    }
    let from = location(tx, &input.from_location_id)
        .map_err(|_| AppError::validation("fromLocationId", "stock_location_not_found"))?;
    let to = location(tx, &input.to_location_id)
        .map_err(|_| AppError::validation("toLocationId", "stock_location_not_found"))?;
    if input.lines.is_empty() {
        return Err(AppError::validation("lines", "required"));
    }
    let company = company::get(tx)?.ok_or_else(|| AppError::conflict("setup_required"))?;
    let mut draft_lines = Vec::with_capacity(input.lines.len());
    let mut needs = Vec::with_capacity(input.lines.len());
    for (i, l) in input.lines.iter().enumerate() {
        let f = |n: &str| format!("lines.{i}.{n}");
        let p = products::get(tx, &l.product_id)
            .map_err(|_| AppError::validation(&f("productId"), "product_not_found"))?;
        if !p.track_stock {
            return Err(AppError::validation(&f("productId"), "product_not_stocked"));
        }
        if p.archived {
            return Err(AppError::validation(&f("productId"), "product_archived"));
        }
        if !l.qty.is_positive() || !l.qty.fits_decimals(p.unit_decimals) {
            return Err(AppError::validation(&f("qty"), "qty_invalid"));
        }
        // A product tracked by serial number moves exactly one serial per whole piece; no line
        // ever moves more serials than its whole pieces.
        let pieces = usize::try_from(l.qty.0 / Qty::ONE.0).unwrap_or(usize::MAX);
        let exact = crate::catalog::extras::get(tx, &p.id)?.track_serials;
        if l.serials.len() > pieces
            || (exact && (l.qty.0 % Qty::ONE.0 != 0 || l.serials.len() != pieces))
        {
            return Err(AppError::validation(
                &f("serials"),
                "serials_count_mismatch",
            ));
        }
        let amounts = tva::compute_line(
            PriceMode::Ht,
            &LineInput {
                unit_price: p.cost_price,
                qty: l.qty,
                discount: Discount::None,
                rate: Rate::ZERO,
            },
        )?;
        draft_lines.push(DraftLine {
            id: new_id(),
            product_id: Some(p.id.clone()),
            description: p.name_in(company.default_doc_lang).to_owned(),
            qty: l.qty,
            unit_code: p.unit_code.clone(),
            unit_price: p.cost_price,
            price_overridden: false,
            tax_rate_bp: Rate::ZERO,
            amounts,
            cost_price: p.cost_price,
            parent_line_id: None,
            origin_line_id: None,
            restock_location_id: Some(to.id.clone()),
            reason_code: None,
            note: l.lot_number.clone(),
        });
        needs.push(StockNeed {
            product_id: p.id.clone(),
            location_id: from.id.clone(),
            qty: l.qty,
        });
    }
    check_available(tx, &needs)?;
    let draft = DraftDocument {
        doc_type: DocType::Transfer,
        terminal_code: ctx.session.terminal_code.clone(),
        business_date: ctx.business_date(),
        customer_id: None,
        supplier_id: None,
        user_id: ctx.session.user_id.clone(),
        terminal_id: ctx.session.terminal_id.clone(),
        cash_session_id: None,
        price_level_id: None,
        lang: company.default_doc_lang,
        tax_regime: TaxRegime::Reel,
        price_mode: PriceMode::Ht,
        stamp_mode: StampMode::Off,
        stamp_duty: Money::ZERO,
        stamp_charged: Money::ZERO,
        rounding: Money::ZERO,
        paid_total: Money::ZERO,
        change_given: Money::ZERO,
        credit_amount: Money::ZERO,
        note: crate::catalog::clean_opt(input.note.as_deref(), "note", 500)?,
        parent_id: None,
        source_ref: None,
        lines: draft_lines,
        payments: Vec::new(),
    };
    let id = documents::insert_draft(tx, &draft, ctx.now)?;
    let document = documents::validate(tx, &id, ctx.now)?;
    tx.execute(
        "INSERT INTO stock_transfers (document_id, from_location_id, to_location_id, driver_name, vehicle)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            document.id,
            from.id,
            to.id,
            crate::catalog::clean_opt(input.driver_name.as_deref(), "driverName", 80)?,
            crate::catalog::clean_opt(input.vehicle.as_deref(), "vehicle", 80)?
        ],
    )?;
    let actor = ctx.actor();
    let today = ctx.business_day();
    let mut moved_serials: Vec<String> = Vec::new();
    for (i, l) in input.lines.iter().enumerate() {
        let f = |n: &str| format!("lines.{i}.{n}");
        let p = products::get(tx, &l.product_id)?;
        let movement = |location_id: &str, qty: Qty, reason: MovementReason| NewMovement {
            product_id: l.product_id.clone(),
            location_id: location_id.to_owned(),
            qty_delta: qty,
            reason,
            reason_code: None,
            ref_type: Some("document".into()),
            ref_id: Some(document.id.clone()),
            unit_cost: Some(p.cost_price),
            note: None,
        };
        let out = movement(&from.id, Qty(-l.qty.0), MovementReason::TransferOut);
        let takes: Vec<LotTake> = match &l.lot_number {
            Some(number) => {
                let lot = lots::find(tx, &l.product_id, number)?
                    .ok_or_else(|| AppError::validation(&f("lotNumber"), "lot_not_found"))?;
                // The named lot leaves only what it holds at the origin (earlier lines of this
                // transfer included): a lot never goes below zero, nor appears where it never was.
                let held: Option<Qty> = tx
                    .prepare_cached(
                        "SELECT qty FROM lot_levels WHERE lot_id = ?1 AND location_id = ?2",
                    )?
                    .query_row(params![lot.id, from.id], |r| r.get(0))
                    .optional()?;
                let held =
                    held.ok_or_else(|| AppError::validation(&f("lotNumber"), "lot_not_found"))?;
                if held < l.qty {
                    let shortage = Shortage {
                        product_id: p.id.clone(),
                        name_ar: p.name_ar.clone(),
                        name_fr: p.name_fr.clone(),
                        name_en: p.name_en.clone(),
                        location_id: from.id.clone(),
                        available: held,
                        needed: l.qty,
                    };
                    return Err(AppError::rejected(
                        json!({ "shortages": [shortage], "lotNumber": lot.lot_number }),
                        "stock_insufficient",
                    ));
                }
                append_with_lot(tx, &actor, &out, Some(&lot.id), ctx.now)?;
                vec![LotTake {
                    lot_id: Some(lot.id),
                    lot_number: Some(lot.lot_number),
                    qty: l.qty,
                    expired: false,
                }]
            }
            None => lots::take(tx, &actor, &out, &today, ctx.now)?,
        };
        if takes.is_empty() {
            append(
                tx,
                &actor,
                &movement(&to.id, l.qty, MovementReason::TransferIn),
                ctx.now,
            )?;
        }
        for t in takes {
            append_with_lot(
                tx,
                &actor,
                &movement(&to.id, t.qty, MovementReason::TransferIn),
                t.lot_id.as_deref(),
                ctx.now,
            )?;
        }
        // Each serial goes with its piece: in stock at the origin, then at the destination (a
        // serial named twice is no longer at the origin the second time).
        for serial_no in &l.serials {
            let serial = serials::find(tx, serial_no)
                .map_err(|e| at_line(i, e))?
                .into_iter()
                .find(|s| s.product_id == l.product_id)
                .ok_or_else(|| AppError::validation(&f("serials"), "serial_not_found"))?;
            if serial.status == SerialStatus::InStock
                && serial.location_id.as_deref() != Some(from.id.as_str())
            {
                return Err(AppError::validation(
                    &f("serials"),
                    "serial_not_at_location",
                ));
            }
            let moved =
                serials::relocate(tx, &serial.id, &to.id, ctx.now).map_err(|e| at_line(i, e))?;
            moved_serials.push(moved.serial_no);
        }
    }
    audit::append(
        tx,
        &actor.event(
            "stock.transferred",
            "document",
            Some(&document.id),
            json!({ "number": document.number, "from": from.id, "to": to.id, "lines": input.lines.len(),
                    "value": document.total_ttc, "serials": moved_serials }),
        ),
    )?;
    Ok(TransferDone {
        document,
        from_location_id: from.id,
        to_location_id: to.id,
    })
}

pub fn list(conn: &Connection, limit: u32) -> Result<Vec<TransferSummary>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT d.id, d.number, d.business_date, t.from_location_id, t.to_location_id,
                (SELECT COUNT(*) FROM document_lines l WHERE l.document_id = d.id), d.total_ttc, d.validated_at, d.user_id
         FROM stock_transfers t JOIN documents d ON d.id = t.document_id
         ORDER BY d.validated_at DESC LIMIT ?1",
    )?;
    let rows = stmt
        .query_map([limit.clamp(1, 500)], |r| {
            Ok(TransferSummary {
                id: r.get(0)?,
                number: r.get(1)?,
                business_date: r.get(2)?,
                from_location_id: r.get(3)?,
                to_location_id: r.get(4)?,
                lines: r.get(5)?,
                value: r.get(6)?,
                validated_at: r.get(7)?,
                user_id: r.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stock::{LocationInput, SHOP_LOCATION_ID, create_location, level};
    use crate::testkit::shop;

    #[test]
    fn a_transfer_moves_both_sides_in_one_transaction() {
        let shop = shop();
        let p = shop.product("Ciment CPJ 45", Money(95_000), &shop.tva19);
        shop.stock_in(&p.id, 40_000);
        let actor = shop.owner.actor();
        let depot = shop
            .db
            .write(move |tx| {
                create_location(
                    tx,
                    &actor,
                    LocationInput {
                        code: "DEPOT".into(),
                        name_ar: "المستودع".into(),
                        name_fr: "Dépôt".into(),
                        name_en: "Warehouse".into(),
                    },
                )
            })
            .unwrap();
        let input = |from: &str, to: &str, qty: i64| TransferInput {
            from_location_id: from.into(),
            to_location_id: to.into(),
            lines: vec![TransferLine {
                product_id: p.id.clone(),
                qty: Qty(qty),
                lot_number: None,
                serials: Vec::new(),
            }],
            driver_name: Some("Rachid".into()),
            vehicle: Some("Hyundai H100 16-12345".into()),
            note: None,
        };
        let ctx = shop.owner_till();
        let same = input(SHOP_LOCATION_ID, SHOP_LOCATION_ID, 1_000);
        assert_eq!(
            shop.db
                .write(move |tx| transfer(tx, &ctx, same))
                .unwrap_err(),
            AppError::validation("toLocationId", "transfer_same_location")
        );
        let ctx = shop.owner_till();
        let move_ten = input(SHOP_LOCATION_ID, &depot.id, 10_000);
        let done = shop
            .db
            .write(move |tx| transfer(tx, &ctx, move_ten))
            .unwrap();
        assert_eq!(done.document.number.as_deref(), Some("BT2026-000001"));
        let at = |loc: &str| {
            let (p, loc) = (p.id.clone(), loc.to_owned());
            shop.db.read(move |c| level(c, &p, &loc)).unwrap()
        };
        assert_eq!(
            (at(SHOP_LOCATION_ID), at(&depot.id)),
            (Qty(30_000), Qty(10_000))
        );
        assert_eq!(
            shop.count(
                "SELECT COUNT(*) FROM stock_movements WHERE reason IN ('transfer_in', 'transfer_out')"
            ),
            2
        );
        let listed = shop.db.read(|c| list(c, 10)).unwrap();
        assert_eq!(
            (listed[0].lines, listed[0].value),
            (1, Money(0)),
            "valued at the CMUP (0 in this shop)"
        );
    }

    fn depot(shop: &crate::testkit::Shop) -> String {
        let actor = shop.owner.actor();
        let input = LocationInput {
            code: "DEPOT".into(),
            name_fr: "Dépôt".into(),
            ..LocationInput::default()
        };
        shop.db
            .write(move |tx| create_location(tx, &actor, input))
            .unwrap()
            .id
    }

    fn track(shop: &crate::testkit::Shop, product_id: &str, lots: bool, serials: bool) {
        let actor = shop.owner.actor();
        let id = product_id.to_owned();
        shop.db
            .write(move |tx| {
                crate::catalog::extras::set(
                    tx,
                    &actor,
                    &id,
                    crate::catalog::extras::ProductExtrasInput {
                        track_lots: lots,
                        track_serials: serials,
                        ..crate::catalog::extras::ProductExtrasInput::default()
                    },
                )
            })
            .unwrap();
    }

    /// Opening stock of `qty` at `location`, in `lot` and with `serials` when given.
    fn bring(
        shop: &crate::testkit::Shop,
        product_id: &str,
        location: &str,
        qty: i64,
        lot: Option<&str>,
        serials: &[&str],
    ) {
        let line = crate::stock::ReceiveLine {
            product_id: product_id.into(),
            location_id: location.into(),
            qty: Qty(qty),
            unit_cost: Money(20_000),
            lot: lot.map(|n| crate::stock::lots::LotInput {
                lot_number: n.into(),
                expiry_date: Some("2027-01-31".into()),
            }),
            serials: serials.iter().map(|s| (*s).to_owned()).collect(),
            supplier_id: None,
            ref_type: "opening".into(),
            ref_id: "rc1".into(),
            note: None,
        };
        let actor = shop.owner.actor();
        shop.db
            .write(move |tx| crate::stock::receive(tx, &actor, &line, 1))
            .unwrap();
    }

    fn line(product_id: &str, qty: i64, lot: Option<&str>, serials: &[&str]) -> TransferLine {
        TransferLine {
            product_id: product_id.into(),
            qty: Qty(qty),
            lot_number: lot.map(str::to_owned),
            serials: serials.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    fn send(
        shop: &crate::testkit::Shop,
        from: &str,
        to: &str,
        lines: Vec<TransferLine>,
    ) -> Result<TransferDone, AppError> {
        let input = TransferInput {
            from_location_id: from.into(),
            to_location_id: to.into(),
            lines,
            driver_name: None,
            vehicle: None,
            note: None,
        };
        let ctx = shop.owner_till();
        shop.db.write(move |tx| transfer(tx, &ctx, input))
    }

    #[test]
    fn a_named_lot_leaves_only_what_it_holds_at_the_origin() {
        let shop = shop();
        let syrup = shop.product("Sirop Toplexil", Money(40_000), &shop.tva9);
        track(&shop, &syrup.id, true, false);
        let depot = depot(&shop);
        bring(&shop, &syrup.id, SHOP_LOCATION_ID, 10_000, Some("L1"), &[]);
        bring(&shop, &syrup.id, &depot, 5_000, Some("L2"), &[]);
        // Stock without a lot too: the shop holds 15, so only the lot can refuse.
        shop.stock_in(&syrup.id, 5_000);
        let shop_to_depot = |lines| send(&shop, SHOP_LOCATION_ID, &depot, lines);
        assert_eq!(
            shop_to_depot(vec![line(&syrup.id, 1_000, Some("L2"), &[])]).unwrap_err(),
            AppError::validation("lines.0.lotNumber", "lot_not_found"),
            "L2 is only in the depot"
        );
        assert_eq!(
            shop_to_depot(vec![line(&syrup.id, 1_000, Some("L9"), &[])]).unwrap_err(),
            AppError::validation("lines.0.lotNumber", "lot_not_found")
        );
        let more = shop_to_depot(vec![line(&syrup.id, 12_000, Some("L1"), &[])]).unwrap_err();
        let payload = more.payload();
        assert_eq!(payload.code, "stock_insufficient");
        let details = payload.details.unwrap();
        assert_eq!(
            (
                details["shortages"][0]["available"].as_i64(),
                details["shortages"][0]["needed"].as_i64(),
                details["lotNumber"].as_str()
            ),
            (Some(10_000), Some(12_000), Some("L1"))
        );
        // Two lines of the same lot add up.
        let twice = shop_to_depot(vec![
            line(&syrup.id, 6_000, Some("L1"), &[]),
            line(&syrup.id, 6_000, Some("L1"), &[]),
        ])
        .unwrap_err();
        assert_eq!(
            twice.payload().details.unwrap()["shortages"][0]["available"].as_i64(),
            Some(4_000)
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM stock_movements WHERE reason = 'transfer_out'"),
            0,
            "a refused transfer moves nothing"
        );
        shop_to_depot(vec![line(&syrup.id, 4_000, Some("L1"), &[])]).unwrap();
        let lot_at = |lot: &str, location: &str| {
            let (lot, location) = (lot.to_owned(), location.to_owned());
            shop.db
                .read(move |c| {
                    Ok(c.query_row(
                        "SELECT ll.qty FROM lot_levels ll JOIN lots l ON l.id = ll.lot_id
                         WHERE l.lot_number = ?1 AND ll.location_id = ?2",
                        params![lot, location],
                        |r| r.get::<_, i64>(0),
                    )?)
                })
                .unwrap()
        };
        assert_eq!(
            (lot_at("L1", SHOP_LOCATION_ID), lot_at("L1", &depot)),
            (6_000, 4_000)
        );
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM lot_levels WHERE qty < 0"),
            0
        );
        assert_eq!(
            shop.count(
                "SELECT COUNT(*) FROM lot_levels WHERE location_id = '00000000-0000-7000-8000-000000000301'"
            ),
            1,
            "no L2 row appeared in the shop"
        );
    }

    #[test]
    fn serials_follow_the_goods_to_the_destination() {
        use crate::purchases::returns::{
            SupplierReturnInput, SupplierReturnLine, return_to_supplier,
        };
        use crate::stock::serials::{self, SerialStatus};
        let shop = shop();
        let phone = shop.product("Redmi Note 13", Money(4_500_000), &shop.tva19);
        track(&shop, &phone.id, false, true);
        let depot = depot(&shop);
        bring(
            &shop,
            &phone.id,
            SHOP_LOCATION_ID,
            2_000,
            None,
            &["IMEI-A1", "IMEI-A2"],
        );
        let shop_to_depot = |serials: &[&str]| {
            send(
                &shop,
                SHOP_LOCATION_ID,
                &depot,
                vec![line(&phone.id, 1_000, None, serials)],
            )
        };
        assert_eq!(
            shop_to_depot(&[]).unwrap_err(),
            AppError::validation("lines.0.serials", "serials_count_mismatch"),
            "one serial per piece"
        );
        assert_eq!(
            shop_to_depot(&["IMEI-A1", "IMEI-A2"]).unwrap_err(),
            AppError::validation("lines.0.serials", "serials_count_mismatch")
        );
        assert_eq!(
            shop_to_depot(&["IMEI-X9"]).unwrap_err(),
            AppError::validation("lines.0.serials", "serial_not_found")
        );
        shop_to_depot(&["IMEI-A1"]).unwrap();
        let where_is = |no: &str| {
            let no = no.to_owned();
            shop.db.read(move |c| serials::find(c, &no)).unwrap()[0].clone()
        };
        assert_eq!(
            where_is("IMEI-A1").location_id.as_deref(),
            Some(depot.as_str())
        );
        assert_eq!(
            where_is("IMEI-A2").location_id.as_deref(),
            Some(SHOP_LOCATION_ID)
        );
        // IMEI-A2 stayed in the shop: it cannot leave the depot.
        assert_eq!(
            send(
                &shop,
                &depot,
                SHOP_LOCATION_ID,
                vec![line(&phone.id, 1_000, None, &["IMEI-A2"])]
            )
            .unwrap_err(),
            AppError::validation("lines.0.serials", "serial_not_at_location")
        );
        // The phone that went to the depot goes back to the supplier from there.
        let s = shop.supplier("Condor");
        let input = SupplierReturnInput {
            supplier_id: s.id.clone(),
            location_id: Some(depot.clone()),
            lines: vec![SupplierReturnLine {
                product_id: phone.id.clone(),
                qty: Qty(1_000),
                unit_cost: None,
                lot_number: None,
                serials: vec!["IMEI-A1".into()],
                note: None,
            }],
            credit_supplier: false,
            note: None,
        };
        let ctx = shop.owner_till();
        shop.db
            .write(move |tx| return_to_supplier(tx, &ctx, input))
            .unwrap();
        assert_eq!(where_is("IMEI-A1").status, SerialStatus::ReturnedToSupplier);
        assert_eq!(where_is("IMEI-A2").status, SerialStatus::InStock);
    }
}

//! Jewellery (master spec §30 VRT-140…144, research 09 §4.3 and §5.4): a piece is a product with
//! its metal, fineness (‰), weight (mg), making charge (façon), stones and hallmark (poinçon). A
//! piece priced by weight costs weight × the board's price per gram for its metal and fineness +
//! its making charge, live at the till. Every ticket selling a piece writes the special client
//! register of CII art. 359 (LF 2026 art. 62) in the same transaction: numbered per year without
//! gaps, never edited or deleted, with the buyer's name, identity document and address. The
//! identity lives in the register only; the journal records counts.
//!
//! LAN: the register's numbering (year, seq) is shop-wide, so `register` runs on the
//! host with the sale; `set_metal_price` and `set_piece` are shared master data.

use crate::audit::{self, Actor};
use crate::catalog::clean_opt;
use crate::catalog::products::{self, ProductInput, ProductKind};
use crate::documents::Document;
use crate::error::AppError;
use crate::ids::new_id;
use crate::money::Money;
use crate::packs::{self, Module};
use crate::payments::PaymentMethod;
use crate::sales::SaleInput;
use crate::sqlenum::sql_text_enum;
use crate::till::TillContext;
use chrono::Datelike;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

/// The longest register designation (`jeweller_register.nature`).
const NATURE_MAX: usize = 160;
/// The most rows `history` and `entries` return.
const PAGE_MAX: u32 = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Metal {
    Gold,
    Silver,
    Platinum,
}

impl Metal {
    pub const ALL: [Metal; 3] = [Metal::Gold, Metal::Silver, Metal::Platinum];

    pub const fn as_str(self) -> &'static str {
        match self {
            Metal::Gold => "gold",
            Metal::Silver => "silver",
            Metal::Platinum => "platinum",
        }
    }

    pub fn parse(s: &str) -> Option<Metal> {
        Metal::ALL.into_iter().find(|m| m.as_str() == s)
    }
}

sql_text_enum!(Metal);

/// A buyer's identity document (tag domain `id_kind`). Named `BuyerIdKind` because specta's
/// type names are global and `pos_docs::a4::IdKind` (the shop's RC/NIF/… ids) came first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum BuyerIdKind {
    Cni,
    Passport,
    DrivingLicence,
    Other,
}

impl BuyerIdKind {
    pub const ALL: [BuyerIdKind; 4] = [
        BuyerIdKind::Cni,
        BuyerIdKind::Passport,
        BuyerIdKind::DrivingLicence,
        BuyerIdKind::Other,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            BuyerIdKind::Cni => "cni",
            BuyerIdKind::Passport => "passport",
            BuyerIdKind::DrivingLicence => "driving_licence",
            BuyerIdKind::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<BuyerIdKind> {
        BuyerIdKind::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

sql_text_enum!(BuyerIdKind);

/// What the owner sets on a product to make it a piece. The inputs and results of this module
/// serialize both ways: a till's writes run on the main PC as host services.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PieceInput {
    pub metal: Metal,
    /// Per mille: 750 = 18 carats.
    pub titre_permille: u16,
    pub weight_mg: i64,
    /// The making charge, added to the metal's price.
    pub facon: Money,
    pub stones: Option<String>,
    pub hallmark: Option<String>,
    pub priced_by_weight: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Piece {
    pub product_id: String,
    pub metal: Metal,
    pub titre_permille: u16,
    pub weight_mg: i64,
    pub facon: Money,
    pub stones: Option<String>,
    pub hallmark: Option<String>,
    pub priced_by_weight: bool,
    /// Weight × the board's price + the making charge, for a piece priced by weight whose metal
    /// and fineness have a price on the board.
    pub live_price: Option<Money>,
    pub updated_at: i64,
}

/// A price per gram on the board, for one metal and fineness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MetalPrice {
    pub metal: Metal,
    pub titre_permille: u16,
    pub price_per_gram: Money,
    pub effective_at: i64,
    pub user_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RepriceReport {
    /// Pieces priced by weight with a price on the board.
    pub pieces: u32,
    /// Those whose catalogue price moved.
    pub changed: u32,
}

/// The buyer written in the jeweller register.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BuyerIdentity {
    pub name: String,
    pub id_kind: BuyerIdKind,
    pub id_number: String,
    pub address: String,
}

/// One entry of the jeweller register. Exported as `JewellerRegisterEntry`: specta's type names
/// are global and the pharmacy's controlled register has its own `RegisterEntry`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename = "JewellerRegisterEntry", rename_all = "camelCase")]
pub struct RegisterEntry {
    pub year: i32,
    pub seq: u32,
    pub business_date: String,
    pub document_number: String,
    pub nature: String,
    pub pieces: u32,
    pub weight_mg: i64,
    pub titre_permille: u16,
    pub price: Money,
    pub buyer_name: String,
    pub buyer_id_kind: BuyerIdKind,
    pub buyer_id_number: String,
    pub buyer_address: String,
    pub user_name: String,
}

fn check_titre(titre: u16) -> Result<(), AppError> {
    if (1..=1000).contains(&titre) {
        Ok(())
    } else {
        Err(AppError::validation("titrePermille", "jewel_piece_invalid"))
    }
}

const PIECE_SELECT: &str = "SELECT product_id, metal, titre_permille, weight_mg, facon, stones,
                                   hallmark, priced_by_weight, updated_at
                            FROM jewellery_pieces";

fn map_piece(r: &Row<'_>) -> rusqlite::Result<Piece> {
    Ok(Piece {
        product_id: r.get(0)?,
        metal: r.get(1)?,
        titre_permille: r.get(2)?,
        weight_mg: r.get(3)?,
        facon: r.get(4)?,
        stones: r.get(5)?,
        hallmark: r.get(6)?,
        priced_by_weight: r.get(7)?,
        live_price: None,
        updated_at: r.get(8)?,
    })
}

/// The piece row of a product, without its live price.
fn piece_row(conn: &Connection, product_id: &str) -> Result<Option<Piece>, AppError> {
    Ok(conn
        .prepare_cached(&format!("{PIECE_SELECT} WHERE product_id = ?1"))?
        .query_row([product_id], map_piece)
        .optional()?)
}

/// The piece details of a product (`None` when it is not a piece).
pub fn piece(conn: &Connection, product_id: &str) -> Result<Option<Piece>, AppError> {
    let Some(mut p) = piece_row(conn, product_id)? else {
        return Ok(None);
    };
    if p.priced_by_weight {
        p.live_price = price_of(conn, &p)?;
    }
    Ok(Some(p))
}

/// Makes a product a piece, or changes its details (VRT-140, VRT-143).
pub fn set_piece(
    tx: &Transaction<'_>,
    actor: &Actor,
    product_id: &str,
    input: PieceInput,
) -> Result<Piece, AppError> {
    packs::require(tx, Module::Jewellery)?;
    let product = products::get(tx, product_id)?;
    // A piece is stocked one by one.
    if !matches!(product.kind, ProductKind::Simple | ProductKind::Variant) {
        return Err(AppError::validation("productId", "jewel_piece_invalid"));
    }
    check_titre(input.titre_permille)?;
    if input.weight_mg <= 0 {
        return Err(AppError::validation("weightMg", "jewel_piece_invalid"));
    }
    if input.facon.is_negative() {
        return Err(AppError::validation("facon", "amount_invalid"));
    }
    let stones = clean_opt(input.stones.as_deref(), "stones", 200)?;
    let hallmark = clean_opt(input.hallmark.as_deref(), "hallmark", 40)?;
    tx.execute(
        "INSERT INTO jewellery_pieces (product_id, metal, titre_permille, weight_mg, facon, stones,
                                       hallmark, priced_by_weight, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT (product_id) DO UPDATE SET
           metal = excluded.metal, titre_permille = excluded.titre_permille,
           weight_mg = excluded.weight_mg, facon = excluded.facon, stones = excluded.stones,
           hallmark = excluded.hallmark, priced_by_weight = excluded.priced_by_weight,
           updated_at = excluded.updated_at",
        params![
            product.id,
            input.metal,
            input.titre_permille,
            input.weight_mg,
            input.facon,
            stones,
            hallmark,
            input.priced_by_weight,
            crate::time::now_ms(),
        ],
    )?;
    audit::append(
        tx,
        &actor.event(
            "jewellery.piece_set",
            "product",
            Some(&product.id),
            json!({
                "metal": input.metal,
                "titre": input.titre_permille,
                "weightMg": input.weight_mg,
                "pricedByWeight": input.priced_by_weight,
            }),
        ),
    )?;
    piece(tx, &product.id)?.ok_or_else(|| AppError::not_found("product"))
}

/// Puts a new price per gram for `metal` at `titre` on the board (the old one stays in the
/// history).
pub fn set_metal_price(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    metal: Metal,
    titre: u16,
    price_per_gram: Money,
) -> Result<MetalPrice, AppError> {
    packs::require(tx, Module::Jewellery)?;
    check_titre(titre)?;
    if !price_per_gram.is_positive() {
        return Err(AppError::validation("pricePerGram", "amount_invalid"));
    }
    let id = new_id();
    tx.execute(
        "INSERT INTO metal_prices (id, metal, titre_permille, price_per_gram, effective_at,
                                   created_at, user_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6)",
        params![
            id,
            metal,
            titre,
            price_per_gram,
            ctx.now,
            ctx.session.user_id
        ],
    )?;
    audit::append(
        tx,
        &ctx.actor().event(
            "jewellery.metal_price_set",
            "metal_price",
            Some(&id),
            json!({ "metal": metal, "titre": titre, "pricePerGram": price_per_gram }),
        ),
    )?;
    board(tx)?
        .into_iter()
        .find(|p| p.metal == metal && p.titre_permille == titre)
        .ok_or_else(|| AppError::not_found("record"))
}

fn map_price(r: &Row<'_>) -> rusqlite::Result<MetalPrice> {
    Ok(MetalPrice {
        metal: r.get(0)?,
        titre_permille: r.get(1)?,
        price_per_gram: r.get(2)?,
        effective_at: r.get(3)?,
        user_name: r.get(4)?,
    })
}

/// The current price of each metal and fineness.
pub fn board(conn: &Connection) -> Result<Vec<MetalPrice>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT m.metal, m.titre_permille, m.price_per_gram, m.effective_at,
                COALESCE(u.display_name, '')
         FROM (SELECT p.*, ROW_NUMBER() OVER (PARTITION BY p.metal, p.titre_permille
                                             ORDER BY p.effective_at DESC, p.created_at DESC,
                                                      p.rowid DESC) AS rn
               FROM metal_prices p) m
         LEFT JOIN users u ON u.id = m.user_id
         WHERE m.rn = 1
         ORDER BY m.metal, m.titre_permille DESC",
    )?;
    let rows = stmt.query_map([], map_price)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// The prices of `metal` at `titre`, newest first (at most 200).
pub fn history(conn: &Connection, metal: Metal, titre: u16) -> Result<Vec<MetalPrice>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT p.metal, p.titre_permille, p.price_per_gram, p.effective_at,
                COALESCE(u.display_name, '')
         FROM metal_prices p LEFT JOIN users u ON u.id = p.user_id
         WHERE p.metal = ?1 AND p.titre_permille = ?2
         ORDER BY p.effective_at DESC, p.created_at DESC, p.rowid DESC
         LIMIT 200",
    )?;
    let rows = stmt.query_map(params![metal, titre], map_price)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn board_price(conn: &Connection, metal: Metal, titre: u16) -> Result<Option<Money>, AppError> {
    Ok(conn
        .prepare_cached(
            "SELECT price_per_gram FROM metal_prices WHERE metal = ?1 AND titre_permille = ?2
             ORDER BY effective_at DESC, created_at DESC, rowid DESC LIMIT 1",
        )?
        .query_row(params![metal, titre], |r| r.get(0))
        .optional()?)
}

/// Weight (mg) × the board's price per gram / 1000, rounded once to the centime, + the making
/// charge; `None` without a price on the board for the piece's metal and fineness.
pub fn price_of(conn: &Connection, piece: &Piece) -> Result<Option<Money>, AppError> {
    let Some(per_gram) = board_price(conn, piece.metal, piece.titre_permille)? else {
        return Ok(None);
    };
    let too_large = || AppError::validation("weightMg", "amount_invalid");
    let metal = per_gram
        .mul_div(piece.weight_mg, 1_000)
        .ok_or_else(too_large)?;
    metal
        .checked_add(piece.facon)
        .map(Some)
        .ok_or_else(too_large)
}

/// The till's price of a piece priced by weight while the module is on (`None` for any other
/// product); `metal_price_missing` when its metal and fineness have no price on the board.
pub(crate) fn live_price(conn: &Connection, product_id: &str) -> Result<Option<Money>, AppError> {
    // The piece row first: one primary-key lookup for every product of a shop without pieces.
    let Some(p) = piece_row(conn, product_id)? else {
        return Ok(None);
    };
    if !p.priced_by_weight || !packs::enabled(conn, Module::Jewellery)? {
        return Ok(None);
    }
    match price_of(conn, &p)? {
        Some(price) => Ok(Some(price)),
        None => Err(AppError::rejected(
            json!({ "metal": p.metal, "titre": p.titre_permille }),
            "metal_price_missing",
        )),
    }
}

/// Writes the live prices of the pieces priced by weight into the catalogue (labels, lists,
/// exports).
pub fn reprice(tx: &Transaction<'_>, ctx: &TillContext) -> Result<RepriceReport, AppError> {
    packs::require(tx, Module::Jewellery)?;
    let pieces: Vec<Piece> = {
        let mut stmt = tx.prepare(
            "SELECT j.product_id, j.metal, j.titre_permille, j.weight_mg, j.facon, j.stones,
                    j.hallmark, j.priced_by_weight, j.updated_at
             FROM jewellery_pieces j JOIN products p ON p.id = j.product_id
             WHERE j.priced_by_weight = 1 AND p.archived_at IS NULL
             ORDER BY j.product_id",
        )?;
        let rows = stmt.query_map([], map_piece)?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    let actor = ctx.actor();
    let mut report = RepriceReport {
        pieces: 0,
        changed: 0,
    };
    for p in &pieces {
        let Some(price) = price_of(tx, p)? else {
            continue;
        };
        report.pieces += 1;
        let product = products::get(tx, &p.product_id)?;
        if product.sell_price != price {
            products::update(
                tx,
                &actor,
                &product.id,
                ProductInput {
                    sell_price: price,
                    ..ProductInput::from(&product)
                },
            )?;
            report.changed += 1;
        }
    }
    audit::append(
        tx,
        &actor.event(
            "jewellery.repriced",
            "product",
            None,
            json!({ "pieces": report.pieces, "changed": report.changed }),
        ),
    )?;
    Ok(report)
}

/// A required buyer text: trimmed, 1..=`max` characters.
fn buyer_text(value: &str, field: &str, max: usize) -> Result<String, AppError> {
    clean_opt(Some(value), field, max)?
        .ok_or_else(|| AppError::validation(field, "buyer_identity_required"))
}

/// The buyer given at the till, else the cart's customer when the file holds a NIN and an
/// address (a national ID card).
fn buyer_of(
    tx: &Transaction<'_>,
    input: &SaleInput,
    given: Option<&BuyerIdentity>,
) -> Result<BuyerIdentity, AppError> {
    if let Some(b) = given {
        return Ok(BuyerIdentity {
            name: buyer_text(&b.name, "buyer.name", 120)?,
            id_kind: b.id_kind,
            id_number: buyer_text(&b.id_number, "buyer.idNumber", 40)?,
            address: buyer_text(&b.address, "buyer.address", 200)?,
        });
    }
    if let Some(id) = input.cart.customer_id.as_deref() {
        let c = crate::customers::get(tx, id)?;
        let nin = c.nin.filter(|s| !s.trim().is_empty());
        let address = c.address.filter(|s| !s.trim().is_empty());
        if let (Some(nin), Some(address)) = (nin, address) {
            return Ok(BuyerIdentity {
                name: buyer_text(&c.name, "buyer.name", 120)?,
                id_kind: BuyerIdKind::Cni,
                id_number: buyer_text(&nin, "buyer.idNumber", 40)?,
                address: buyer_text(&address, "buyer.address", 200)?,
            });
        }
    }
    Err(AppError::validation("buyer", "buyer_identity_required"))
}

/// The register's designation of a line: its description, with the stones.
fn nature_of(description: &str, stones: Option<&str>) -> String {
    let text = match stones {
        Some(s) => format!("{description} ({s})"),
        None => description.to_owned(),
    };
    text.chars().take(NATURE_MAX).collect()
}

/// One entry of the jeweller register as the sale writes it down, before its number (a
/// till's sale sends its entries to the main PC, which keeps the register).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JewelRow {
    pub business_date: String,
    pub document_id: String,
    pub line_id: String,
    pub product_id: String,
    pub nature: String,
    pub pieces: i64,
    pub weight_mg: i64,
    pub titre_permille: u16,
    pub price: Money,
    pub buyer_name: String,
    pub buyer_id_kind: BuyerIdKind,
    pub buyer_id_number: String,
    pub buyer_address: String,
    pub created_at: i64,
    pub user_id: String,
    pub terminal_id: String,
}

/// Whether `product_id` is a precious-metal piece (its sale needs a register entry).
pub(crate) fn is_piece(conn: &Connection, product_id: &str) -> Result<bool, AppError> {
    Ok(piece_row(conn, product_id)?.is_some())
}

/// The register entries of a ticket, one per precious-metal line, checked and written down but
/// not numbered yet (VRT-141); refuses a ticket without its buyer's identity or taking more cash
/// than `jewellery.max_cash` (VRT-144). Runs inside the sale (`after_sale`).
pub(crate) fn plan_register(
    tx: &Transaction<'_>,
    ctx: &TillContext,
    input: &SaleInput,
    given: Option<&BuyerIdentity>,
    document: &Document,
    main_line_ids: &[String],
) -> Result<Vec<JewelRow>, AppError> {
    if !packs::enabled(tx, Module::Jewellery)? {
        return Ok(Vec::new());
    }
    let mut pieces = Vec::new();
    for (line_id, cart_line) in main_line_ids.iter().zip(&input.cart.lines) {
        let Some(product_id) = cart_line.product_id.as_deref() else {
            continue;
        };
        if let Some(p) = piece_row(tx, product_id)? {
            let line = document
                .lines
                .iter()
                .find(|l| &l.id == line_id)
                .ok_or_else(|| AppError::not_found("document"))?;
            pieces.push((p, line));
        }
    }
    if pieces.is_empty() {
        return Ok(Vec::new());
    }
    let buyer = buyer_of(tx, input, given)?;
    if let Some(max) = crate::settings::get::<Option<i64>>(tx, "jewellery.max_cash")? {
        let tendered = Money::checked_sum(
            input
                .payments
                .iter()
                .filter(|p| p.method == PaymentMethod::Cash)
                .map(|p| p.amount),
        )
        .unwrap_or(Money(i64::MAX));
        let cash = tendered
            .checked_sub(document.change_given)
            .unwrap_or(Money::ZERO);
        if cash > Money(max) {
            return Err(AppError::rejected(
                json!({ "max": max, "cash": cash }),
                "cash_above_limit",
            ));
        }
    }
    Ok(pieces
        .iter()
        .map(|(p, line)| {
            let count = (line.qty.0 / crate::money::QTY_SCALE).max(1);
            JewelRow {
                business_date: ctx.business_day(),
                document_id: document.id.clone(),
                line_id: line.id.clone(),
                product_id: p.product_id.clone(),
                nature: nature_of(&line.description, p.stones.as_deref()),
                pieces: count,
                weight_mg: p.weight_mg.saturating_mul(count),
                titre_permille: p.titre_permille,
                price: line.total_ttc,
                buyer_name: buyer.name.clone(),
                buyer_id_kind: buyer.id_kind,
                buyer_id_number: buyer.id_number.clone(),
                buyer_address: buyer.address.clone(),
                created_at: ctx.now,
                user_id: ctx.session.user_id.clone(),
                terminal_id: ctx.session.terminal_id.clone(),
            }
        })
        .collect())
}

/// Numbers and writes register entries after the year's last one (the year of their business
/// date). Returns how many.
pub(crate) fn write_register(tx: &Transaction<'_>, rows: &[JewelRow]) -> Result<u32, AppError> {
    for row in rows {
        let year = chrono::NaiveDate::parse_from_str(&row.business_date, "%Y-%m-%d")
            .map_err(|_| AppError::validation("businessDate", "value_invalid"))?
            .year();
        let seq: i64 = tx.query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM jeweller_register WHERE year = ?1",
            [year],
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT INTO jeweller_register (id, year, seq, business_date, document_id, line_id,
                                            product_id, nature, pieces, weight_mg, titre_permille,
                                            price, buyer_name, buyer_id_kind, buyer_id_number,
                                            buyer_address, created_at, user_id, terminal_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                     ?18, ?19)",
            params![
                new_id(),
                year,
                seq,
                row.business_date,
                row.document_id,
                row.line_id,
                row.product_id,
                row.nature,
                row.pieces,
                row.weight_mg,
                row.titre_permille,
                row.price,
                row.buyer_name,
                row.buyer_id_kind,
                row.buyer_id_number,
                row.buyer_address,
                row.created_at,
                row.user_id,
                row.terminal_id
            ],
        )?;
    }
    Ok(u32::try_from(rows.len()).unwrap_or(u32::MAX))
}

/// The register of `year` from entry `from_seq`, in order (at most `limit`, ≤ 1000).
pub fn entries(
    conn: &Connection,
    year: i32,
    from_seq: u32,
    limit: u32,
) -> Result<Vec<RegisterEntry>, AppError> {
    let mut stmt = conn.prepare_cached(
        "SELECT r.year, r.seq, r.business_date, COALESCE(d.number, ''), r.nature, r.pieces,
                r.weight_mg, r.titre_permille, r.price, r.buyer_name, r.buyer_id_kind,
                r.buyer_id_number, r.buyer_address, COALESCE(u.display_name, '')
         FROM jeweller_register r
         JOIN documents d ON d.id = r.document_id
         LEFT JOIN users u ON u.id = r.user_id
         WHERE r.year = ?1 AND r.seq >= ?2
         ORDER BY r.seq LIMIT ?3",
    )?;
    let rows = stmt.query_map(params![year, from_seq, limit.min(PAGE_MAX)], |r| {
        Ok(RegisterEntry {
            year: r.get(0)?,
            seq: r.get(1)?,
            business_date: r.get(2)?,
            document_number: r.get(3)?,
            nature: r.get(4)?,
            pieces: r.get(5)?,
            weight_mg: r.get(6)?,
            titre_permille: r.get(7)?,
            price: r.get(8)?,
            buyer_name: r.get(9)?,
            buyer_id_kind: r.get(10)?,
            buyer_id_number: r.get(11)?,
            buyer_address: r.get(12)?,
            user_name: r.get(13)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

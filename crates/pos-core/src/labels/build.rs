//! Label contents: names in the label language(s), the price the customer pays now (TTC, with
//! the running promotion: LBL-005), the price before the promotion (PRM-021), unit, lot and
//! expiry, barcode, shop name; weighed labels with the weight, the amount and a weight barcode.
//! Every amount is the till's: the tax module's line computation with the automatic discount the
//! cart would apply, so a label never shows a price the till does not charge.

use super::template::{
    self, ArabicGuard, LabelLang, LabelTemplate, SymbologyChoice, WEIGHED_TEMPLATE,
};
use crate::audit::{self, Actor};
use crate::catalog::extras;
use crate::catalog::products::{self, Product};
use crate::catalog::scale_codes::{self, ScaleValue};
use crate::company::{self, Company, PriceMode};
use crate::documents::{self, DocStatus};
use crate::error::AppError;
use crate::lang::Lang;
use crate::money::{Money, QTY_SCALE, Qty, Rate};
use crate::numbering::DocType;
use crate::promotions::{self, ActivePromotion, LineFacts, PricingRules};
use crate::tax::tva::{self, Discount, LineInput};
use crate::time::now_ms;
use chrono::{Days, NaiveDate};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

pub const MAX_COPIES: u16 = 999;
pub const MAX_LABELS_PER_BATCH: u32 = 5_000;
/// The lot the designer previews a lot-tracked product with before it has a lot of its own.
pub const SAMPLE_LOT_NUMBER: &str = "LOT-000001";
/// The sample lot expires this many days after the business day.
const SAMPLE_LOT_DAYS: u64 = 365;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum LabelSymbology {
    Ean13,
    Ean8,
    UpcA,
    Code128,
    Qr,
    /// LBL-004: capitals, digits and `- . space $ / + %`, at most 43 characters.
    Code39,
    /// LBL-004: Interleaved 2 of 5, an even number of digits (2–30).
    Itf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LabelBarcode {
    pub symbology: LabelSymbology,
    /// Digits with their check digit for EAN/UPC; printable ASCII without `"`, `\`, `^`, `~`
    /// else.
    pub data: String,
}

impl LabelBarcode {
    /// The data fits the symbology (Code 39 and ITF, LBL-004; the others are checked by M4's
    /// builder as before).
    pub fn check(&self) -> Result<(), AppError> {
        let fits = match self.symbology {
            LabelSymbology::Code39 => {
                !self.data.is_empty()
                    && self.data.len() <= 43
                    && self.data.bytes().all(|b| {
                        b.is_ascii_uppercase() || b.is_ascii_digit() || b" -.$/+%".contains(&b)
                    })
            }
            LabelSymbology::Itf => {
                (2..=30).contains(&self.data.len())
                    && self.data.len().is_multiple_of(2)
                    && self.data.bytes().all(|b| b.is_ascii_digit())
            }
            _ => true,
        };
        if fits {
            Ok(())
        } else {
            Err(AppError::validation("barcode", "label_barcode_invalid"))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LabelData {
    pub product_id: Option<String>,
    pub copies: u16,
    pub lang: LabelLang,
    pub name: String,
    pub name_second: Option<String>,
    pub reference: Option<String>,
    /// TTC, what the customer pays now: per unit, or for the weight of a weighed label.
    pub price: Money,
    pub old_price: Option<Money>,
    pub unit_code: String,
    /// Weighed labels: the price per kg.
    pub unit_price: Option<Money>,
    pub weight: Option<Qty>,
    pub lot_number: Option<String>,
    /// `YYYY-MM-DD`.
    pub expiry: Option<String>,
    pub packed_on: Option<String>,
    pub shop_name: Option<String>,
    pub barcode: Option<LabelBarcode>,
    /// Printed from the demo shop's database: the name line says "DÉMO" (finding 39).
    #[serde(default)]
    pub demo: bool,
    /// Jewellery (VRT-143): a piece's metal, fineness, weight and hallmark.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub jewel: Option<JewelLabel>,
}

/// The jewel line of a piece's label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct JewelLabel {
    pub metal: crate::jewellery::Metal,
    pub titre_permille: u16,
    pub weight_mg: i64,
    pub hallmark: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LabelItem {
    pub product_id: String,
    pub copies: u16,
    pub lot_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum LabelSource {
    Products {
        items: Vec<LabelItem>,
    },
    /// One label per piece received (LBL-006), with the lot of the line.
    GoodsReceipt {
        document_id: String,
    },
    /// One label per piece counted.
    Count {
        count_id: String,
    },
    /// The reprint queue, one label each (LBL-007).
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LabelRequest {
    pub source: LabelSource,
    pub template_id: Option<String>,
    /// Overrides the template's language (UX-004).
    pub lang: Option<LabelLang>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WeighedLabelRequest {
    pub product_id: String,
    pub net: Qty,
    pub copies: u16,
    pub template_id: Option<String>,
    pub lang: Option<LabelLang>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum LabelWarningCode {
    /// DOC-034: a guarded category on a label without Arabic.
    ArabicRequired,
    /// DOC-034: a guarded category on an Arabic label, but the product has no Arabic name, so
    /// the label shows its French (or English) name only. Switching templates does not help: the
    /// product needs its Arabic name.
    ArabicNameMissing,
    /// The template wants a barcode, the product has none (LBL-010: assign an internal EAN-13).
    NoBarcode,
    NoPrice,
    /// Décret 13-378: the label is too small to print whole the lot or the expiry date it
    /// carries, so the layout left it out (`pos-hw` `Placed::lines`). The print path, which lays
    /// the labels out, raises it with [`dropped_details`].
    LotOrExpiryDropped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LabelWarning {
    pub product_id: String,
    pub name: String,
    pub code: LabelWarningCode,
}

/// The labels of one template in a batch, printed as one job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LabelGroup {
    pub template: LabelTemplate,
    pub labels: Vec<LabelData>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LabelBatch {
    pub groups: Vec<LabelGroup>,
    pub warnings: Vec<LabelWarning>,
    /// Labels to print, copies included.
    pub total: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AssignedBarcode {
    pub product_id: String,
    pub barcode: String,
}

/// One unit at `price` and `rate` as the till charges it in the company's price mode, TTC
/// (LBL-005: the price shown is the TTC paid); `None` for an amount the tax module refuses.
pub fn ttc(mode: PriceMode, price: Money, rate: Rate) -> Option<Money> {
    let line = LineInput {
        unit_price: price,
        qty: Qty::ONE,
        discount: Discount::None,
        rate,
    };
    tva::compute_line(mode, &line).ok().map(|a| a.total_ttc)
}

/// GS1 check digit of `body` (EAN-8, UPC-A, EAN-13 without their last digit): weights 3, 1, 3…
/// from the rightmost body digit.
pub fn gtin_check_digit(body: &str) -> Option<u8> {
    if body.is_empty() || !body.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let sum: u32 = body
        .bytes()
        .rev()
        .enumerate()
        .map(|(i, b)| u32::from(b - b'0') * if i % 2 == 0 { 3 } else { 1 })
        .sum();
    u8::try_from((10 - sum % 10) % 10).ok()
}

fn gtin_ok(code: &str) -> bool {
    code.len() >= 8
        && code.bytes().all(|b| b.is_ascii_digit())
        && code
            .get(..code.len() - 1)
            .and_then(gtin_check_digit)
            .is_some_and(|d| code.as_bytes().last() == Some(&(b'0' + d)))
}

/// Printer-safe text: printable ASCII without the characters TSPL (`"`, `\`) and ZPL (`^`, `~`)
/// read as commands, 1–40 characters (`pos-hw` refuses the same characters).
fn printer_safe(data: &str) -> bool {
    (1..=40).contains(&data.len())
        && data
            .bytes()
            .all(|b| (0x20..0x7F).contains(&b) && !matches!(b, b'"' | b'\\' | b'^' | b'~'))
}

pub fn auto_barcode(data: &str) -> Option<LabelBarcode> {
    let symbology = match data.len() {
        13 if gtin_ok(data) => LabelSymbology::Ean13,
        8 if gtin_ok(data) => LabelSymbology::Ean8,
        12 if gtin_ok(data) => LabelSymbology::UpcA,
        _ if printer_safe(data) => LabelSymbology::Code128,
        _ => return None,
    };
    Some(LabelBarcode {
        symbology,
        data: data.to_owned(),
    })
}

/// The product's piece barcode (else its first one, else its reference) in the template's
/// symbology; Code 39 prints the reference and ITF the barcode digits, and data their
/// symbology cannot carry gives no barcode (the `NoBarcode` warning).
fn barcode_for(product: &Product, choice: SymbologyChoice) -> Option<LabelBarcode> {
    let barcode = product
        .barcodes
        .iter()
        .find(|b| b.pack_qty == Qty::ONE)
        .or(product.barcodes.first())
        .map(|b| b.barcode.clone());
    let any = || barcode.clone().or_else(|| product.reference.clone());
    let (symbology, data) = match choice {
        SymbologyChoice::None => return None,
        SymbologyChoice::Auto => return auto_barcode(&any()?),
        SymbologyChoice::Code128 => (LabelSymbology::Code128, any()?),
        SymbologyChoice::Qr => (LabelSymbology::Qr, any()?),
        SymbologyChoice::Code39 => (LabelSymbology::Code39, product.reference.clone()?),
        SymbologyChoice::Itf => (LabelSymbology::Itf, barcode.clone()?),
    };
    let label = LabelBarcode { symbology, data };
    (printer_safe(&label.data) && label.check().is_ok()).then_some(label)
}

pub(crate) fn company_of(conn: &Connection) -> Result<Company, AppError> {
    company::get(conn)?.ok_or_else(|| AppError::conflict("setup_required"))
}

fn shop_name(company: &Company, lang: Lang) -> String {
    let preferred = match lang {
        Lang::Ar => &company.name_ar,
        Lang::Fr => &company.name_fr,
        Lang::En => &company.name_en,
    };
    [
        preferred,
        &company.name_fr,
        &company.name_ar,
        &company.name_en,
    ]
    .into_iter()
    .find(|n| !n.is_empty())
    .cloned()
    .unwrap_or_default()
}

/// What the till charges, TTC, for `qty` of `product` at its catalogue price, as the cart prices
/// a line: the tax module's line amount after the best automatic discount (no customer). With
/// such a discount, the amount without it comes second.
fn charged(
    mode: PriceMode,
    active: &[ActivePromotion],
    product: &Product,
    qty: Qty,
    lang: Lang,
) -> Result<(Money, Option<Money>), AppError> {
    let line = |discount| LineInput {
        unit_price: product.sell_price,
        qty,
        discount,
        rate: product.tax_rate_bp,
    };
    let full = tva::compute_line(mode, &line(Discount::None))?;
    let rules = PricingRules {
        promos: active,
        customer_discount: Rate::ZERO,
    };
    let facts = LineFacts {
        product_id: &product.id,
        category_id: product.category_id.as_deref(),
        unit_price: product.sell_price,
        qty,
        gross: full.gross,
    };
    match promotions::best_discount(&rules, &facts, lang) {
        Some(discount) if discount.amount.is_positive() => {
            let lowered = tva::compute_line(mode, &line(Discount::Amount(discount.amount)))?;
            Ok((lowered.total_ttc, Some(full.total_ttc)))
        }
        _ => Ok((full.total_ttc, None)),
    }
}

/// The TTC price per unit now and the price before a running automatic promotion.
pub(crate) fn price_now(
    mode: PriceMode,
    active: &[ActivePromotion],
    product: &Product,
    lang: Lang,
) -> Result<(Money, Option<Money>), AppError> {
    charged(mode, active, product, Qty::ONE, lang)
}

struct Item {
    product_id: String,
    copies: Option<u16>,
    qty: Option<Qty>,
    lot_id: Option<String>,
}

fn items_of(conn: &Connection, source: &LabelSource, day: &str) -> Result<Vec<Item>, AppError> {
    match source {
        LabelSource::Products { items } => items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                if !(1..=MAX_COPIES).contains(&item.copies) {
                    return Err(AppError::validation(
                        &format!("items.{i}.copies"),
                        "value_invalid",
                    ));
                }
                Ok(Item {
                    product_id: item.product_id.clone(),
                    copies: Some(item.copies),
                    qty: None,
                    lot_id: item.lot_id.clone(),
                })
            })
            .collect(),
        LabelSource::GoodsReceipt { document_id } => {
            let not_printable = || AppError::conflict("label_source_not_printable");
            let doc = match documents::get(conn, document_id) {
                Err(AppError::NotFound { .. }) => return Err(not_printable()),
                other => other?,
            };
            if doc.doc_type != DocType::GoodsReceipt || doc.status != DocStatus::Validated {
                return Err(not_printable());
            }
            Ok(doc
                .lines
                .into_iter()
                .filter_map(|l| {
                    l.product_id.map(|product_id| Item {
                        product_id,
                        copies: None,
                        qty: Some(l.qty),
                        lot_id: l.lot_id,
                    })
                })
                .collect())
        }
        LabelSource::Count { count_id } => {
            let exists: bool = conn.query_row(
                "SELECT EXISTS (SELECT 1 FROM stock_counts WHERE id = ?1)",
                [count_id],
                |r| r.get(0),
            )?;
            if !exists {
                return Err(AppError::not_found("count"));
            }
            let mut stmt = conn.prepare(
                "SELECT product_id, counted_qty, lot_id FROM stock_count_lines
                 WHERE count_id = ?1 AND counted_qty > 0 ORDER BY rowid",
            )?;
            let rows = stmt
                .query_map([count_id], |r| {
                    Ok(Item {
                        product_id: r.get(0)?,
                        copies: None,
                        qty: Some(r.get(1)?),
                        lot_id: r.get(2)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        }
        LabelSource::Stale => Ok(super::queue::stale(conn, day, MAX_LABELS_PER_BATCH)?
            .into_iter()
            .map(|s| Item {
                product_id: s.product_id,
                copies: Some(1),
                qty: None,
                lot_id: None,
            })
            .collect()),
    }
}

/// One label per piece; one for a product sold by weight or length.
fn copies_for(qty: Qty, unit_decimals: u8) -> u16 {
    if unit_decimals > 0 {
        return 1;
    }
    let pieces = u64::try_from(qty.0)
        .unwrap_or(0)
        .div_ceil(QTY_SCALE.unsigned_abs());
    u16::try_from(pieces.clamp(1, u64::from(MAX_COPIES))).unwrap_or(MAX_COPIES)
}

/// The lot's number and expiry date, when `lot_id` is a lot of `product_id`.
fn lot_of(
    conn: &Connection,
    product_id: &str,
    lot_id: Option<&str>,
) -> Result<Option<(String, Option<String>)>, AppError> {
    let Some(id) = lot_id else {
        return Ok(None);
    };
    Ok(conn
        .query_row(
            "SELECT lot_number, expiry_date FROM lots WHERE id = ?1 AND product_id = ?2",
            [id, product_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?)
}

struct Context {
    company: Company,
    active: Vec<ActivePromotion>,
    guard: ArabicGuard,
}

/// What `label_for` prints: the template, the language, the copies and the lot.
struct Print<'a> {
    template: &'a LabelTemplate,
    lang: LabelLang,
    copies: u16,
    lot_id: Option<&'a str>,
}

fn label_for(
    conn: &Connection,
    cx: &Context,
    product: &Product,
    print: Print<'_>,
    warnings: &mut Vec<LabelWarning>,
) -> Result<LabelData, AppError> {
    let Print {
        template: tpl,
        lang,
        copies,
        lot_id,
    } = print;
    let primary = lang.primary();
    let name = product.name_in(primary).to_owned();
    let name_second = lang
        .second()
        .map(|l| product.name_in(l).to_owned())
        .filter(|n| *n != name && !n.is_empty());
    // VRT-140: a piece priced by weight is labelled at the till's live price.
    let (price, old_price) = match crate::jewellery::live_price(conn, &product.id)? {
        Some(live) => {
            let piece = Product {
                sell_price: live,
                ..product.clone()
            };
            price_now(cx.company.price_mode, &cx.active, &piece, primary)?
        }
        None => price_now(cx.company.price_mode, &cx.active, product, primary)?,
    };
    let jewel = if tpl.fields.jewel && crate::packs::enabled(conn, crate::packs::Module::Jewellery)?
    {
        crate::jewellery::piece(conn, &product.id)?.map(|p| JewelLabel {
            metal: p.metal,
            titre_permille: p.titre_permille,
            weight_mg: p.weight_mg,
            hallmark: p.hallmark,
        })
    } else {
        None
    };
    let lot = lot_of(conn, &product.id, lot_id)?;
    let barcode = if tpl.fields.barcode {
        barcode_for(product, tpl.symbology)
    } else {
        None
    };
    let mut warn = |code| {
        warnings.push(LabelWarning {
            product_id: product.id.clone(),
            name: name.clone(),
            code,
        });
    };
    if cx.guard.required(product.category_id.as_deref()) {
        if !lang.has_arabic() {
            warn(LabelWarningCode::ArabicRequired);
        } else if product.name_ar.trim().is_empty() {
            // `name_in(Ar)` fell back to the French name and the second name, equal to it, was
            // dropped: the label is French (or English) only.
            warn(LabelWarningCode::ArabicNameMissing);
        }
    }
    if tpl.fields.barcode && tpl.symbology != SymbologyChoice::None && barcode.is_none() {
        warn(LabelWarningCode::NoBarcode);
    }
    if tpl.fields.price && !price.is_positive() {
        warn(LabelWarningCode::NoPrice);
    }
    Ok(LabelData {
        product_id: Some(product.id.clone()),
        copies,
        lang,
        name_second,
        reference: product.reference.clone().filter(|_| tpl.fields.reference),
        price,
        old_price: old_price.filter(|_| tpl.fields.promo),
        unit_code: product.unit_code.clone(),
        unit_price: None,
        weight: None,
        lot_number: lot.as_ref().filter(|_| tpl.fields.lot).map(|l| l.0.clone()),
        expiry: lot.and_then(|l| l.1).filter(|_| tpl.fields.expiry),
        packed_on: None,
        shop_name: tpl.fields.shop.then(|| shop_name(&cx.company, primary)),
        barcode,
        jewel,
        name,
        demo: cx.company.demo,
    })
}

fn push(groups: &mut Vec<LabelGroup>, template: &LabelTemplate, label: LabelData) {
    match groups.iter_mut().find(|g| g.template.id == template.id) {
        Some(group) => group.labels.push(label),
        None => groups.push(LabelGroup {
            template: template.clone(),
            labels: vec![label],
        }),
    }
}

fn context(conn: &Connection, day: &str) -> Result<Context, AppError> {
    Ok(Context {
        company: company_of(conn)?,
        active: promotions::active_on(conn, day)?,
        guard: ArabicGuard::load(conn)?,
    })
}

fn bounded(total: u32) -> Result<u32, AppError> {
    if total == 0 {
        return Err(AppError::conflict("labels_empty"));
    }
    if total > MAX_LABELS_PER_BATCH {
        return Err(AppError::rejected(
            json!({ "total": total, "max": MAX_LABELS_PER_BATCH }),
            "labels_too_many",
        ));
    }
    Ok(total)
}

/// The labels of `request` on business day `day` (`YYYY-MM-DD`), grouped by template.
pub fn build(conn: &Connection, request: &LabelRequest, day: &str) -> Result<LabelBatch, AppError> {
    let items = items_of(conn, &request.source, day)?;
    let cx = context(conn, day)?;
    let templates = template::templates(conn)?;
    let by_category = template::category_templates(conn)?;
    let (mut groups, mut warnings, mut total) = (Vec::new(), Vec::new(), 0_u32);
    for item in items {
        let product = products::get(conn, &item.product_id)?;
        let tpl = template::pick(
            &templates,
            &by_category,
            request.template_id.as_deref(),
            product.category_id.as_deref(),
        )?;
        let copies = item
            .copies
            .unwrap_or_else(|| copies_for(item.qty.unwrap_or(Qty::ONE), product.unit_decimals));
        total = total.saturating_add(u32::from(copies));
        let print = Print {
            template: tpl,
            lang: request.lang.unwrap_or(tpl.lang),
            copies,
            lot_id: item.lot_id.as_deref(),
        };
        let label = label_for(conn, &cx, &product, print, &mut warnings)?;
        push(&mut groups, tpl, label);
    }
    Ok(LabelBatch {
        groups,
        warnings,
        total: bounded(total)?,
    })
}

/// The weight barcode of `net` for `plu`: the first weight layout (no verifier) whose code the
/// till reads back as exactly `net`, so the label never charges another weight than it shows.
fn weight_barcode(conn: &Connection, plu: u32, net: Qty) -> Result<Option<String>, AppError> {
    let layouts = scale_codes::layouts(conn)?;
    Ok(layouts
        .iter()
        .filter(|l| l.value == ScaleValue::Weight && !l.verifier)
        .filter_map(|l| scale_codes::encode(l, plu, net.0))
        .find(|code| {
            scale_codes::parse(code, &layouts)
                .is_some_and(|c| c.plu == plu && c.value == ScaleValue::Weight && c.raw == net.0)
        }))
}

/// A price label for goods weighed at the counter: net weight, price per kg, the amount the till
/// charges for that weight (the sale line's, promotion included), packed on `day`, expiry after
/// the scale item's shelf life, and a weight barcode that reads back as the printed weight.
pub fn weighed(
    conn: &Connection,
    request: &WeighedLabelRequest,
    day: &str,
) -> Result<LabelBatch, AppError> {
    if !(1..=99_999).contains(&request.net.0) {
        return Err(AppError::validation("net", "value_invalid"));
    }
    if !(1..=MAX_COPIES).contains(&request.copies) {
        return Err(AppError::validation("copies", "value_invalid"));
    }
    let product = products::get(conn, &request.product_id)?;
    if !product.is_weighed || product.unit_code != "kg" {
        return Err(AppError::rejected(
            json!({ "plu": product.plu, "productId": product.id }),
            "scale_product_not_weighed",
        ));
    }
    let cx = context(conn, day)?;
    let templates = template::templates(conn)?;
    let tpl = match request.template_id.as_deref() {
        Some(id) => templates.iter().find(|t| t.id == id),
        None => templates
            .iter()
            .find(|t| t.id == WEIGHED_TEMPLATE)
            .or_else(|| templates.iter().find(|t| t.fields.weight)),
    }
    .ok_or_else(|| AppError::not_found("label_template"))?;
    let lang = request.lang.unwrap_or(tpl.lang);
    let mut warnings = Vec::new();
    let print = Print {
        template: tpl,
        lang,
        copies: request.copies,
        lot_id: None,
    };
    let mut label = label_for(conn, &cx, &product, print, &mut warnings)?;
    warnings.retain(|w| w.code != LabelWarningCode::NoBarcode);
    let (amount, _) = charged(
        cx.company.price_mode,
        &cx.active,
        &product,
        request.net,
        lang.primary(),
    )?;
    label.unit_price = Some(label.price);
    label.old_price = None;
    label.weight = Some(request.net);
    label.price = amount;
    label.packed_on = tpl.fields.packed_on.then(|| day.to_owned());
    let shelf_life: Option<u16> = conn
        .query_row(
            "SELECT shelf_life_days FROM scale_items WHERE product_id = ?1",
            [&product.id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    label.expiry = match (tpl.fields.expiry, shelf_life) {
        (true, Some(days)) => NaiveDate::parse_from_str(day, "%Y-%m-%d")
            .ok()
            .and_then(|d| d.checked_add_days(Days::new(u64::from(days))))
            .map(|d| d.format("%Y-%m-%d").to_string()),
        _ => None,
    };
    label.barcode = match (tpl.fields.barcode, product.plu) {
        (true, Some(plu)) => weight_barcode(conn, plu, request.net)?.map(|data| LabelBarcode {
            symbology: LabelSymbology::Ean13,
            data,
        }),
        _ => None,
    };
    if tpl.fields.barcode && label.barcode.is_none() {
        warnings.push(LabelWarning {
            product_id: product.id.clone(),
            name: label.name.clone(),
            code: LabelWarningCode::NoBarcode,
        });
    }
    Ok(LabelBatch {
        groups: vec![LabelGroup {
            template: tpl.clone(),
            labels: vec![label],
        }],
        warnings,
        total: u32::from(request.copies),
    })
}

/// The label `template` (saved or not) makes for `product_id`, or for the first product with a
/// barcode: the designer's preview (Task 16). A lot-tracked product carries a lot.
pub fn preview_label(
    conn: &Connection,
    template: &LabelTemplate,
    product_id: Option<&str>,
    lang: Option<LabelLang>,
    day: &str,
) -> Result<LabelData, AppError> {
    if let Some(field) = template.problem() {
        return Err(AppError::validation(
            &format!("template.{field}"),
            "value_invalid",
        ));
    }
    let id: String = match product_id {
        Some(id) => id.to_owned(),
        None => conn
            .query_row(
                "SELECT p.id FROM products p WHERE p.archived_at IS NULL
                 ORDER BY EXISTS (SELECT 1 FROM product_barcodes b WHERE b.product_id = p.id) DESC,
                          p.rowid
                 LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| AppError::conflict("labels_empty"))?,
    };
    let product = products::get(conn, &id)?;
    let cx = context(conn, day)?;
    let mut warnings = Vec::new();
    let print = Print {
        template,
        lang: lang.unwrap_or(template.lang),
        copies: 1,
        lot_id: None,
    };
    let mut label = label_for(conn, &cx, &product, print, &mut warnings)?;
    if extras::get(conn, &product.id)?.track_lots {
        let (number, expiry) = preview_lot(conn, &product.id, day)?;
        label.lot_number = template.fields.lot.then_some(number);
        label.expiry = expiry.filter(|_| template.fields.expiry);
    }
    Ok(label)
}

/// The lot a lot-tracked product is previewed with: its newest lot, else [`SAMPLE_LOT_NUMBER`]
/// expiring a year after `day`, so the designer shows where the lot and the expiry date go and
/// whether the label has room for them (décret 13-378).
fn preview_lot(
    conn: &Connection,
    product_id: &str,
    day: &str,
) -> Result<(String, Option<String>), AppError> {
    let newest = conn
        .query_row(
            "SELECT lot_number, expiry_date FROM lots WHERE product_id = ?1
             ORDER BY created_at DESC, rowid DESC LIMIT 1",
            [product_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(newest.unwrap_or_else(|| {
        let expiry = NaiveDate::parse_from_str(day, "%Y-%m-%d")
            .ok()
            .and_then(|d| d.checked_add_days(Days::new(SAMPLE_LOT_DAYS)))
            .map(|d| d.format("%Y-%m-%d").to_string());
        (SAMPLE_LOT_NUMBER.to_owned(), expiry)
    }))
}

/// DOC-034: refuses a batch whose labels of guarded products would print without Arabic unless
/// the user confirmed: `label_arabic_required` for the labels whose template has no Arabic (pick
/// an Arabic template), else `label_arabic_name_missing` for the products without an Arabic name
/// (give them one). Both carry `details: { productIds }`; one confirmation covers both.
pub fn check_arabic(batch: &LabelBatch, override_: bool) -> Result<(), AppError> {
    if override_ {
        return Ok(());
    }
    let ids = |code: LabelWarningCode| -> Vec<&str> {
        batch
            .warnings
            .iter()
            .filter(|w| w.code == code)
            .map(|w| w.product_id.as_str())
            .collect()
    };
    for (code, error) in [
        (LabelWarningCode::ArabicRequired, "label_arabic_required"),
        (
            LabelWarningCode::ArabicNameMissing,
            "label_arabic_name_missing",
        ),
    ] {
        let ids = ids(code);
        if !ids.is_empty() {
            return Err(AppError::rejected(json!({ "productIds": ids }), error));
        }
    }
    Ok(())
}

/// A warning that `check_arabic` refuses without the user's confirmation.
fn arabic_warning(code: LabelWarningCode) -> bool {
    matches!(
        code,
        LabelWarningCode::ArabicRequired | LabelWarningCode::ArabicNameMissing
    )
}

/// Décret 13-378: the `LotOrExpiryDropped` warning for `label` when its layout (`pos-hw`
/// `Placed::lines`) printed without the lot or the expiry date it carries; `lot_printed` and
/// `expiry_printed` say which of the two made it onto the label.
pub fn dropped_details(
    label: &LabelData,
    lot_printed: bool,
    expiry_printed: bool,
) -> Option<LabelWarning> {
    let lost =
        (label.lot_number.is_some() && !lot_printed) || (label.expiry.is_some() && !expiry_printed);
    lost.then(|| LabelWarning {
        product_id: label.product_id.clone().unwrap_or_default(),
        name: label.name.clone(),
        code: LabelWarningCode::LotOrExpiryDropped,
    })
}

/// LBL-010: an in-store EAN-13 (M2 PRD-003) for each product without any barcode.
pub fn assign_missing_barcodes(
    tx: &Transaction<'_>,
    actor: &Actor,
    product_ids: &[String],
) -> Result<Vec<AssignedBarcode>, AppError> {
    let mut assigned = Vec::new();
    for id in product_ids {
        let has_one: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM product_barcodes WHERE product_id = ?1)",
            [id],
            |r| r.get(0),
        )?;
        if has_one {
            continue;
        }
        products::get(tx, id)?;
        let barcode = crate::catalog::codes::next_internal_ean13(tx)?;
        let now = now_ms();
        tx.execute(
            "INSERT INTO product_barcodes (id, product_id, barcode, pack_qty, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![crate::ids::new_id(), id, barcode, Qty::ONE, now],
        )?;
        tx.execute(
            "UPDATE products SET updated_at = ?2 WHERE id = ?1",
            params![id, now],
        )?;
        products::refresh_search_text(tx, id)?;
        assigned.push(AssignedBarcode {
            product_id: id.clone(),
            barcode,
        });
    }
    if !assigned.is_empty() {
        audit::append(
            tx,
            &actor.event(
                "labels.barcodes_assigned",
                "labels",
                None,
                json!({ "assigned": assigned }),
            ),
        )?;
    }
    Ok(assigned)
}

/// Keeps what each product's label now says (the reprint queue compares with it) and journals
/// the batch. Weighed labels (one per package) are journaled only.
pub fn record_printed(
    tx: &Transaction<'_>,
    actor: &Actor,
    batch: &LabelBatch,
    arabic_override: bool,
    now: i64,
) -> Result<(), AppError> {
    let mut labels = 0_u32;
    for group in &batch.groups {
        for label in &group.labels {
            labels = labels.saturating_add(1);
            let Some(product_id) = label
                .product_id
                .as_deref()
                .filter(|_| label.weight.is_none())
            else {
                continue;
            };
            tx.execute(
                "INSERT INTO label_prints (product_id, printed_at, user_id, template_id, lang,
                     price, old_price, name, barcode, copies)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT (product_id) DO UPDATE SET printed_at = excluded.printed_at,
                   user_id = excluded.user_id, template_id = excluded.template_id,
                   lang = excluded.lang, price = excluded.price, old_price = excluded.old_price,
                   name = excluded.name, barcode = excluded.barcode, copies = excluded.copies",
                params![
                    product_id,
                    now,
                    actor.user_id,
                    group.template.id,
                    label.lang.as_str(),
                    label.price,
                    label.old_price,
                    label.name,
                    label.barcode.as_ref().map(|b| b.data.as_str()),
                    label.copies,
                ],
            )?;
        }
    }
    let overridden = arabic_override && batch.warnings.iter().any(|w| arabic_warning(w.code));
    let templates: Vec<&str> = batch
        .groups
        .iter()
        .map(|g| g.template.id.as_str())
        .collect();
    audit::append(
        tx,
        &actor.event(
            "labels.printed",
            "labels",
            None,
            json!({
                "labels": labels,
                "copies": batch.total,
                "templates": templates,
                "arabicWarningOverridden": overridden,
            }),
        ),
    )?;
    Ok(())
}

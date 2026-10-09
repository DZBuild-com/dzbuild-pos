//! Label templates (LBL-001, LBL-005): size, columns, language, fields and barcode, stored as the
//! shop setting `labels.templates`; one template per category through
//! `labels.category_templates`. Sizes are tenths of a millimetre (`_dmm`).

use crate::company::ShopActivity;
use crate::error::AppError;
use crate::lang::Lang;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_TEMPLATES: usize = 50;
pub const WEIGHED_TEMPLATE: &str = "builtin.weighed_58x40";

/// (width, height) in millimetres: spec §9.3's 40×25 and the LBL-001 presets.
pub const SIZE_PRESETS: [(u16, u16); 13] = [
    (40, 25),
    (50, 30),
    (58, 40),
    (20, 40),
    (40, 20),
    (30, 20),
    (40, 30),
    (40, 60),
    (45, 58),
    (50, 80),
    (60, 100),
    (100, 100),
    (100, 150),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum LabelLang {
    Ar,
    Fr,
    En,
    ArFr,
    ArEn,
    FrEn,
}

impl LabelLang {
    pub const ALL: [LabelLang; 6] = [
        LabelLang::Ar,
        LabelLang::Fr,
        LabelLang::En,
        LabelLang::ArFr,
        LabelLang::ArEn,
        LabelLang::FrEn,
    ];

    /// The language of the big name and of the words; Arabic first on bilingual labels.
    pub const fn primary(self) -> Lang {
        match self {
            LabelLang::Ar | LabelLang::ArFr | LabelLang::ArEn => Lang::Ar,
            LabelLang::Fr | LabelLang::FrEn => Lang::Fr,
            LabelLang::En => Lang::En,
        }
    }

    pub const fn second(self) -> Option<Lang> {
        match self {
            LabelLang::ArFr => Some(Lang::Fr),
            LabelLang::ArEn | LabelLang::FrEn => Some(Lang::En),
            LabelLang::Ar | LabelLang::Fr | LabelLang::En => None,
        }
    }

    pub const fn has_arabic(self) -> bool {
        matches!(self, LabelLang::Ar | LabelLang::ArFr | LabelLang::ArEn)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            LabelLang::Ar => "ar",
            LabelLang::Fr => "fr",
            LabelLang::En => "en",
            LabelLang::ArFr => "ar_fr",
            LabelLang::ArEn => "ar_en",
            LabelLang::FrEn => "fr_en",
        }
    }

    pub fn parse(text: &str) -> Option<LabelLang> {
        LabelLang::ALL.into_iter().find(|l| l.as_str() == text)
    }
}

/// `Auto` picks EAN-13/EAN-8/UPC-A for a valid GTIN, else Code 128 (LBL-009: 1D by default).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SymbologyChoice {
    Auto,
    Code128,
    Qr,
    None,
    /// LBL-004: the product's reference in Code 39.
    Code39,
    /// LBL-004: the product's barcode digits in Interleaved 2 of 5.
    Itf,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct LabelFields {
    pub name: bool,
    pub price: bool,
    /// "/kg" after the price of a product sold by weight or length.
    pub unit: bool,
    pub barcode: bool,
    pub reference: bool,
    pub lot: bool,
    pub expiry: bool,
    /// The price before a running promotion, struck through (PRM-021).
    pub promo: bool,
    pub shop: bool,
    /// Weighed labels: net weight and price per kg.
    pub weight: bool,
    pub packed_on: bool,
    /// Jewellery (VRT-143): metal, fineness, weight and hallmark of a piece.
    pub jewel: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LabelTemplate {
    /// `builtin.*` for the shipped ones; the UI shows `labels:builtin.<id>` when `name` is empty.
    pub id: String,
    pub name: String,
    pub width_dmm: u16,
    pub height_dmm: u16,
    pub gap_dmm: u16,
    /// Labels side by side on the roll (LBL-001 "2-up").
    pub columns: u8,
    pub column_gap_dmm: u16,
    pub lang: LabelLang,
    pub fields: LabelFields,
    pub symbology: SymbologyChoice,
    pub name_lines: u8,
}

impl LabelTemplate {
    /// Width of one printed row: the labels side by side and the gaps between them.
    pub fn row_width_dmm(&self) -> u32 {
        u32::from(self.width_dmm) * u32::from(self.columns)
            + u32::from(self.column_gap_dmm) * u32::from(self.columns.saturating_sub(1))
    }

    /// The field at fault, if any: 15–110 mm wide, 10–150 mm high, a row within 110 mm. A size
    /// within these bounds can still be too small for what a label carries: `pos-hw` refuses
    /// such a label with `printer_setting_invalid` on `widthDmm` or `heightDmm`.
    pub fn problem(&self) -> Option<&'static str> {
        let id_ok = (1..=64).contains(&self.id.len())
            && self.id.bytes().all(|b| {
                b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
            });
        if !id_ok {
            return Some("id");
        }
        if self.name.chars().count() > 40 || self.name.chars().any(char::is_control) {
            return Some("name");
        }
        if !(150..=1_100).contains(&self.width_dmm) {
            return Some("widthDmm");
        }
        if !(100..=1_500).contains(&self.height_dmm) {
            return Some("heightDmm");
        }
        if self.gap_dmm > 100 {
            return Some("gapDmm");
        }
        if !(1..=3).contains(&self.columns) {
            return Some("columns");
        }
        if self.column_gap_dmm > 100 || self.row_width_dmm() > 1_100 {
            return Some("columnGapDmm");
        }
        if !(1..=3).contains(&self.name_lines) {
            return Some("nameLines");
        }
        None
    }
}

fn builtin(id: &str, (w, h): (u16, u16), fields: LabelFields, name_lines: u8) -> LabelTemplate {
    LabelTemplate {
        id: id.to_owned(),
        name: String::new(),
        width_dmm: w * 10,
        height_dmm: h * 10,
        gap_dmm: 20,
        columns: 1,
        column_gap_dmm: 0,
        lang: LabelLang::ArFr,
        fields,
        symbology: SymbologyChoice::Auto,
        name_lines,
    }
}

/// The jeweller's 60 × 12 mm "butterfly" tag: name, price, barcode and the jewel line (metal,
/// fineness, weight, hallmark), Code 128.
pub fn builtin_jewel() -> LabelTemplate {
    LabelTemplate {
        gap_dmm: 30,
        symbology: SymbologyChoice::Code128,
        ..builtin(
            "builtin.jewel",
            (60, 12),
            LabelFields {
                name: true,
                price: true,
                barcode: true,
                jewel: true,
                ..LabelFields::default()
            },
            1,
        )
    }
}

/// Arabic + French (DOC-034 safe by default).
pub fn builtin_templates() -> Vec<LabelTemplate> {
    let base = LabelFields {
        name: true,
        price: true,
        unit: true,
        barcode: true,
        ..LabelFields::default()
    };
    vec![
        builtin(
            "builtin.price_40x25",
            (40, 25),
            LabelFields {
                promo: true,
                ..base
            },
            1,
        ),
        builtin(
            "builtin.product_50x30",
            (50, 30),
            LabelFields {
                reference: true,
                ..base
            },
            2,
        ),
        builtin(
            "builtin.shelf_58x40",
            (58, 40),
            LabelFields {
                promo: true,
                shop: true,
                ..base
            },
            2,
        ),
        builtin(
            WEIGHED_TEMPLATE,
            (58, 40),
            LabelFields {
                weight: true,
                packed_on: true,
                expiry: true,
                shop: true,
                ..base
            },
            1,
        ),
        builtin_jewel(),
    ]
}

/// `labels.templates`: 1 to 50 valid templates with distinct ids.
pub fn valid_templates(value: &Value) -> bool {
    let Ok(list) = serde_json::from_value::<Vec<LabelTemplate>>(value.clone()) else {
        return false;
    };
    let mut ids = BTreeSet::new();
    !list.is_empty()
        && list.len() <= MAX_TEMPLATES
        && list
            .iter()
            .all(|t| t.problem().is_none() && ids.insert(t.id.clone()))
}

/// `labels.category_templates`: `{ categoryId: templateId }`.
pub fn valid_category_templates(value: &Value) -> bool {
    value.as_object().is_some_and(|map| {
        map.len() <= 1_000
            && map.iter().all(|(k, v)| {
                k.len() <= 64 && v.as_str().is_some_and(|s| (1..=64).contains(&s.len()))
            })
    })
}

/// `labels.arabic_categories`: `null` (the shop's activity decides) or category ids.
pub fn valid_arabic_categories(value: &Value) -> bool {
    value.is_null()
        || value.as_array().is_some_and(|a| {
            a.len() <= 1_000
                && a.iter()
                    .all(|v| v.as_str().is_some_and(|s| (1..=64).contains(&s.len())))
        })
}

pub fn templates(conn: &Connection) -> Result<Vec<LabelTemplate>, AppError> {
    crate::settings::get(conn, "labels.templates")
}

pub fn category_templates(conn: &Connection) -> Result<BTreeMap<String, String>, AppError> {
    crate::settings::get(conn, "labels.category_templates")
}

/// The requested template, else the category's, else the first one.
pub fn pick<'a>(
    templates: &'a [LabelTemplate],
    by_category: &BTreeMap<String, String>,
    requested: Option<&str>,
    category_id: Option<&str>,
) -> Result<&'a LabelTemplate, AppError> {
    let not_found = || AppError::not_found("label_template");
    if let Some(id) = requested {
        return templates.iter().find(|t| t.id == id).ok_or_else(not_found);
    }
    category_id
        .and_then(|c| by_category.get(c))
        .and_then(|id| templates.iter().find(|t| &t.id == id))
        .or_else(|| templates.first())
        .ok_or_else(not_found)
}

/// DOC-034 (décret 13-378): categories whose labels must carry Arabic. The setting
/// `labels.arabic_categories` lists them; unset, every category is guarded when the shop sells
/// food or regulated goods (`guards_every_category` decides per activity).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArabicGuard {
    all: bool,
    categories: BTreeSet<String>,
}

impl ArabicGuard {
    pub fn load(conn: &Connection) -> Result<ArabicGuard, AppError> {
        let listed: Option<Vec<String>> = crate::settings::get(conn, "labels.arabic_categories")?;
        Ok(match listed {
            Some(ids) => ArabicGuard {
                all: false,
                categories: ids.into_iter().collect(),
            },
            None => {
                let activity: ShopActivity = crate::settings::get(conn, "shop.activity")?;
                ArabicGuard {
                    all: guards_every_category(activity),
                    categories: BTreeSet::new(),
                }
            }
        })
    }

    pub fn required(&self, category_id: Option<&str>) -> bool {
        self.all || category_id.is_some_and(|c| self.categories.contains(c))
    }
}

/// Whether an activity sells food or regulated goods to consumers, so that every label must
/// carry Arabic (DOC-034) until the owner lists categories. The match has no wildcard: a new
/// activity must be decided here.
const fn guards_every_category(activity: ShopActivity) -> bool {
    match activity {
        // Food, medicines, cosmetics and hygiene, tobacco and gas: consumer information that
        // décret 13-378 wants "essentiellement en langue arabe". An importer, a wholesaler and a
        // van seller put such goods on the market and label them for the shops they supply.
        ShopActivity::General
        | ShopActivity::Butcher
        | ShopActivity::Bakery
        | ShopActivity::FoodService
        | ShopActivity::Parapharmacy
        | ShopActivity::Pharmacy
        | ShopActivity::Cosmetics
        | ShopActivity::Wholesale
        | ShopActivity::Kiosk
        | ShopActivity::Containers
        | ShopActivity::Importer
        | ShopActivity::VanSales => true,
        // Durable goods and services: the owner lists the categories to guard, if any.
        ShopActivity::Hardware
        | ShopActivity::AutoParts
        | ShopActivity::Clothing
        | ShopActivity::Phones
        | ShopActivity::Stationery
        | ShopActivity::Instalments
        | ShopActivity::Optics
        | ShopActivity::Pressing
        | ShopActivity::Salon
        | ShopActivity::Jewellery
        | ShopActivity::Gym
        | ShopActivity::Workshop
        | ShopActivity::Other => false,
    }
}

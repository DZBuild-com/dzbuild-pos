//! Regulatory data pack (spec §6): tax rates, stamp duty brackets, IFU thresholds, nisab and
//! zakat rate, price ceilings of regulated staples, mobile operators, wilayas. Loaded at startup; replaceable by a signed pack later. Nothing regulatory is
//! hard-coded anywhere else.

use crate::error::AppError;
use crate::money::{BP_SCALE, Money};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeMap;

pub const BUNDLED_JSON: &str = include_str!("../data/dz-datapack-2026.json");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Pack {
    pub id: String,
    pub version: u32,
    pub valid_from: String,
    pub notes: Vec<String>,
    pub currency: Currency,
    /// Accepted NIF digit counts (15, or 18 for a secondary establishment).
    pub nif_lengths: Vec<u32>,
    pub vat: Vat,
    pub stamp_duty: StampDuty,
    pub ifu: Ifu,
    pub zakat: Zakat,
    /// Legal consumer price ceilings of subsidised staples (PRC-012, M8).
    pub regulated: Regulated,
    /// The mobile operators and their number prefixes (flexy top-ups, M8).
    pub mobile_operators: Vec<MobileOperator>,
    pub fiscal: Fiscal,
    pub wilayas: Vec<Wilaya>,
    /// EAN-13 weight/price layouts of the restricted range 2x (spec §8, HW-030).
    #[serde(default)]
    pub scale_barcodes: ScaleBarcodes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Currency {
    pub code: String,
    pub minor_units: u32,
    pub symbol_ar: String,
    pub symbol_fr: String,
    pub symbol_en: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Vat {
    pub rates: Vec<VatRate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VatRate {
    pub code: String,
    pub rate_bp: i32,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StampDuty {
    pub exempt_up_to: Money,
    pub tranche: Money,
    pub minimum: Money,
    pub brackets: Vec<StampBracket>,
    pub progressive: bool,
    pub exempt_tenders: Vec<String>,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StampBracket {
    /// Upper bound of the whole cash amount, inclusive; `None` for the last bracket.
    pub up_to: Option<Money>,
    pub per_tranche: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Ifu {
    pub threshold: Money,
    pub auto_entrepreneur_threshold: Money,
    pub goods_rate_bp: i32,
    pub services_rate_bp: i32,
    pub auto_entrepreneur_rate_bp: i32,
    pub minimum: Money,
    pub auto_entrepreneur_minimum: Money,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Zakat {
    pub nisab: Money,
    pub hijri_year: u32,
    /// The zakat rate on trade goods, basis points (250 = 2.5 %).
    pub rate_bp: i32,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Regulated {
    pub source: String,
    pub items: Vec<RegulatedItem>,
}

/// A regulated staple (PRC-012): its legal consumer ceiling TTC and, for semolina, the retail
/// margin the decree allows. `unit` is the M1a unit code the ceiling is per (`pc` or `kg`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RegulatedItem {
    pub code: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub unit: String,
    pub ceiling: Money,
    pub retail_margin: Option<Money>,
    pub tax_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MobileOperator {
    pub code: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    /// Leading digits of the operator's 10-digit national numbers (`06` for Mobilis).
    pub prefixes: Vec<String>,
}

/// The G50 lines the helper fills, by key (their boxes come from the pack).
pub const G50_KEYS: [&str; 13] = [
    "caHt9",
    "exempt9",
    "taxable9",
    "tva9",
    "caHt19",
    "exempt19",
    "taxable19",
    "tva19",
    "caHtTotal",
    "exemptTotal",
    "taxableTotal",
    "tvaTotal",
    "stampDuty",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct G50Form {
    pub form: String,
    /// Line key (`G50_KEYS`) → box printed on the form (`"caHt9"` → `"1"`).
    pub boxes: BTreeMap<String, String>,
    /// The block (`"9"` or `"19"`) that receives exempt turnover (TAX-009).
    pub exempt_block: String,
    pub source: String,
}

/// Fiscal thresholds and forms (M3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Fiscal {
    pub g50: G50Form,
    /// CTCA art. 30: the TVA of a purchase invoice above this TTC amount paid in cash is not
    /// deductible (TAX-013).
    pub cash_deduction_limit: Money,
    /// CPF art. 64: years the books and vouchers are kept (the app never deletes them anyway).
    pub retention_years: u32,
    pub source: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ScaleBarcodes {
    pub source: String,
    pub default_layouts: Vec<crate::catalog::scale_codes::ScaleBarcodeLayout>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Wilaya {
    pub code: String,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub since: u32,
    pub split_from: Option<String>,
}

fn invalid(what: &str) -> AppError {
    tracing::error!(what, "regulatory data pack is invalid");
    AppError::conflict("datapack_invalid")
}

impl Pack {
    /// Structural checks only, so that a new Loi de Finances needs a new JSON file and no code:
    /// at least 69 wilayas with sequential codes and three names, at least one VAT rate with
    /// unique codes, every rate within 0-100 %, positive timbre amounts, ascending timbre
    /// brackets that end with an open one, at least one NIF length, a box for every G50 line
    /// (`G50_KEYS`) and an exempt block of "9" or "19", a positive cash deduction limit and at
    /// least 10 years of retention, valid scale barcode layouts with distinct prefixes, regulated
    /// staples with unique codes, three names, a positive ceiling, a unit of `pc` or `kg` and a
    /// known tax code, mobile operators with unique codes and prefixes of 2-4 digits, a zakat
    /// rate within 0.01-100 % and a positive nisab. The 2026 values themselves are pinned by the
    /// tests below, not here.
    pub fn validate(&self) -> Result<(), AppError> {
        if self.wilayas.len() < 69 {
            return Err(invalid("wilaya count"));
        }
        if self.nif_lengths.is_empty() || self.nif_lengths.contains(&0) {
            return Err(invalid("nif lengths"));
        }
        for (index, w) in self.wilayas.iter().enumerate() {
            if w.code != format!("{:02}", index + 1)
                || w.name_ar.is_empty()
                || w.name_fr.is_empty()
                || w.name_en.is_empty()
            {
                return Err(invalid("wilaya entry"));
            }
        }
        let mut codes = std::collections::BTreeSet::new();
        if self.vat.rates.is_empty()
            || !self
                .vat
                .rates
                .iter()
                .all(|r| !r.code.is_empty() && codes.insert(r.code.as_str()))
        {
            return Err(invalid("vat rates"));
        }
        let ifu = &self.ifu;
        let mut rates = self.vat.rates.iter().map(|r| r.rate_bp).chain([
            ifu.goods_rate_bp,
            ifu.services_rate_bp,
            ifu.auto_entrepreneur_rate_bp,
        ]);
        if rates.any(|bp| !(0..=BP_SCALE).contains(&i64::from(bp))) {
            return Err(invalid("rate out of range"));
        }
        let stamp = &self.stamp_duty;
        if stamp.exempt_up_to < Money::ZERO
            || stamp.tranche <= Money::ZERO
            || stamp.minimum < Money::ZERO
            || stamp.brackets.iter().any(|b| b.per_tranche <= Money::ZERO)
        {
            return Err(invalid("stamp duty amounts"));
        }
        let brackets = &stamp.brackets;
        if brackets.is_empty() {
            return Err(invalid("stamp duty brackets"));
        }
        let mut floor = stamp.exempt_up_to;
        for (index, bracket) in brackets.iter().enumerate() {
            let last = index + 1 == brackets.len();
            match bracket.up_to {
                Some(up_to) if !last && up_to > floor => floor = up_to,
                None if last => {}
                _ => return Err(invalid("stamp duty brackets")),
            }
        }
        let fiscal = &self.fiscal;
        if G50_KEYS
            .iter()
            .any(|k| fiscal.g50.boxes.get(*k).is_none_or(|b| b.trim().is_empty()))
            || !matches!(fiscal.g50.exempt_block.as_str(), "9" | "19")
        {
            return Err(invalid("g50 form"));
        }
        if !fiscal.cash_deduction_limit.is_positive() || fiscal.retention_years < 10 {
            return Err(invalid("fiscal thresholds"));
        }
        crate::catalog::scale_codes::validate_layouts(&self.scale_barcodes.default_layouts)
            .map_err(|_| invalid("scale barcode layouts"))?;
        let mut regulated = std::collections::BTreeSet::new();
        for item in &self.regulated.items {
            if item.code.is_empty()
                || !regulated.insert(item.code.as_str())
                || item.ceiling <= Money::ZERO
                || item.retail_margin.is_some_and(|m| m < Money::ZERO)
                || !matches!(item.unit.as_str(), "pc" | "kg")
                || !self.vat.rates.iter().any(|r| r.code == item.tax_code)
                || item.name_ar.is_empty()
                || item.name_fr.is_empty()
                || item.name_en.is_empty()
            {
                return Err(invalid("regulated item"));
            }
        }
        let mut operators = std::collections::BTreeSet::new();
        let digits_ok =
            |p: &String| (2..=4).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit());
        for op in &self.mobile_operators {
            if op.code.is_empty()
                || !operators.insert(op.code.as_str())
                || op.prefixes.is_empty()
                || !op.prefixes.iter().all(digits_ok)
            {
                return Err(invalid("mobile operator"));
            }
        }
        if !(1..=10_000).contains(&self.zakat.rate_bp) || self.zakat.nisab <= Money::ZERO {
            return Err(invalid("zakat"));
        }
        Ok(())
    }

    pub fn wilaya(&self, code: &str) -> Option<&Wilaya> {
        self.wilayas.iter().find(|w| w.code == code)
    }
}

pub fn parse(json: &str) -> Result<Pack, AppError> {
    let pack: Pack = serde_json::from_str(json).map_err(|e| invalid(&e.to_string()))?;
    pack.validate()?;
    Ok(pack)
}

/// The pack compiled into this build.
pub fn bundled() -> Result<Pack, AppError> {
    parse(BUNDLED_JSON)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_pack_is_valid() {
        let pack = bundled().unwrap();
        assert_eq!(pack.id, "dz-2026.1");
        assert_eq!(pack.wilayas.len(), 69);
        let rates: Vec<i32> = pack.vat.rates.iter().map(|r| r.rate_bp).collect();
        assert_eq!(rates, [1_900, 900, 0]);
    }

    #[test]
    fn wilayas_are_correct_in_arabic_and_french() {
        let pack = bundled().unwrap();
        let alger = pack.wilaya("16").unwrap();
        assert_eq!(
            (
                alger.name_fr.as_str(),
                alger.name_ar.as_str(),
                alger.name_en.as_str()
            ),
            ("Alger", "الجزائر", "Algiers")
        );
        let ater = pack.wilaya("62").unwrap();
        assert_eq!(
            (ater.name_fr.as_str(), ater.name_ar.as_str()),
            ("Bir El Ater", "بئر العاتر")
        );
        assert_eq!((ater.since, ater.split_from.as_deref()), (2026, Some("12")));
        assert_eq!(pack.wilaya("65").unwrap().name_fr, "Aïn Ouessara");
        assert_eq!(pack.wilaya("58").unwrap().name_fr, "El Meniaa");
        assert_eq!(pack.wilaya("49").unwrap().split_from.as_deref(), Some("01"));
        assert!(pack.wilaya("70").is_none());
    }

    #[test]
    fn fiscal_values_match_the_research() {
        let pack = bundled().unwrap();
        assert_eq!(pack.ifu.threshold, Money(800_000_000));
        assert_eq!(pack.ifu.auto_entrepreneur_threshold, Money(500_000_000));
        assert_eq!(pack.zakat.nisab, Money(229_500_000));
        assert_eq!(pack.nif_lengths, [15, 18]);
        let stamp = &pack.stamp_duty;
        assert_eq!(
            (stamp.exempt_up_to, stamp.tranche, stamp.minimum),
            (Money(30_000), Money(10_000), Money(500))
        );
        let brackets: Vec<(Option<Money>, Money)> = stamp
            .brackets
            .iter()
            .map(|b| (b.up_to, b.per_tranche))
            .collect();
        assert_eq!(
            brackets,
            [
                (Some(Money(3_000_000)), Money(100)),
                (Some(Money(10_000_000)), Money(150)),
                (None, Money(200))
            ]
        );
        assert!(!stamp.progressive);
    }

    #[test]
    fn validation_rejects_broken_packs() {
        let mut pack = bundled().unwrap();
        pack.wilayas.swap(0, 1);
        assert_eq!(pack.validate().unwrap_err().code(), "datapack_invalid");
        let mut pack = bundled().unwrap();
        pack.vat.rates.clear();
        assert_eq!(pack.validate().unwrap_err().code(), "datapack_invalid");
        let mut pack = bundled().unwrap();
        pack.nif_lengths.clear();
        assert_eq!(pack.validate().unwrap_err().code(), "datapack_invalid");
        assert_eq!(parse("{").unwrap_err().code(), "datapack_invalid");
    }

    #[test]
    fn remaining_regulatory_values_are_pinned() {
        let pack = bundled().unwrap();
        let ifu = &pack.ifu;
        let rates = (
            ifu.goods_rate_bp,
            ifu.services_rate_bp,
            ifu.auto_entrepreneur_rate_bp,
        );
        assert_eq!(rates, (500, 1_200, 50));
        let minimums = (ifu.minimum, ifu.auto_entrepreneur_minimum);
        assert_eq!(minimums, (Money(3_000_000), Money(1_000_000)));
        assert_eq!(pack.zakat.hijri_year, 1447);
        assert_eq!(
            pack.stamp_duty.exempt_tenders,
            [
                "card_cib",
                "edahabia",
                "baridimob",
                "cheque",
                "transfer",
                "credit"
            ]
        );
        let codes: Vec<&str> = pack.vat.rates.iter().map(|r| r.code.as_str()).collect();
        assert_eq!(codes, ["TVA19", "TVA9", "EXO"]);
        assert_eq!(
            (pack.currency.code.as_str(), pack.currency.minor_units),
            ("DZD", 2)
        );
        assert!(
            pack.notes
                .iter()
                .any(|n| n.contains("01-58") && n.contains("verify")),
            "names 01-58 must stay flagged for verification"
        );
    }

    #[test]
    fn wilaya_history_is_consistent() {
        let pack = bundled().unwrap();
        // Names and seats of 59-69: décret présidentiel 26-206 (research 09 §5.10).
        let new: Vec<&str> = pack.wilayas[58..]
            .iter()
            .map(|w| w.name_fr.as_str())
            .collect();
        assert_eq!(
            new,
            [
                "Aflou",
                "Barika",
                "El Kantara",
                "Bir El Ater",
                "El Aricha",
                "Ksar Chellala",
                "Aïn Ouessara",
                "Messaad",
                "Ksar El Boukhari",
                "Bou Saâda",
                "El Abiodh Sidi Cheikh",
            ]
        );
        for w in &pack.wilayas {
            let number: u32 = w.code.parse().unwrap();
            let since = match number {
                1..=31 => 1974,
                32..=48 => 1984,
                49..=58 => 2019,
                _ => 2026,
            };
            assert_eq!(w.since, since, "wilaya {}", w.code);
            // The original 48 were never split from another; every later one names an older parent.
            match &w.split_from {
                None => assert!(number <= 48, "wilaya {} has no parent", w.code),
                Some(parent) => {
                    assert!(number > 48, "wilaya {} cannot have a parent", w.code);
                    let parent = pack.wilaya(parent).unwrap();
                    assert!(parent.since < w.since, "wilaya {}", w.code);
                }
            }
        }
    }

    /// Breaks a copy of the bundled pack with `break_it` and expects `validate` to refuse it.
    fn assert_rejected(what: &str, break_it: impl FnOnce(&mut Pack)) {
        let mut pack = bundled().unwrap();
        break_it(&mut pack);
        let code = pack.validate().map_err(|e| e.code());
        assert_eq!(code, Err("datapack_invalid".to_owned()), "{what}");
    }

    /// A copy of the first VAT rate, code included, with another rate.
    fn extra_rate(pack: &Pack, rate_bp: i32) -> VatRate {
        VatRate {
            rate_bp,
            ..pack.vat.rates[0].clone()
        }
    }

    #[test]
    fn validation_rejects_each_malformed_shape() {
        assert_rejected("fewer than 69 wilayas", |p| p.wilayas.truncate(68));
        assert_rejected("a gap in the codes", |p| {
            p.wilayas.remove(30);
        });
        assert_rejected("a duplicate code", |p| p.wilayas[1].code = "01".into());
        assert_rejected("an unpadded code", |p| p.wilayas[0].code = "1".into());
        assert_rejected("an empty Arabic name", |p| p.wilayas[15].name_ar.clear());
        assert_rejected("an empty French name", |p| p.wilayas[15].name_fr.clear());
        assert_rejected("an empty English name", |p| p.wilayas[15].name_en.clear());
        assert_rejected("a zero NIF length", |p| p.nif_lengths.push(0));
        assert_rejected("no VAT rate", |p| p.vat.rates.clear());
        assert_rejected("two VAT rates with one code", |p| {
            let rate = extra_rate(p, 700);
            p.vat.rates.push(rate);
        });
        assert_rejected("a VAT rate without a code", |p| p.vat.rates[1].code.clear());
        // In place: an added copy would already be refused for its duplicate code.
        assert_rejected("a VAT rate above 100 %", |p| {
            p.vat.rates[0].rate_bp = 10_001
        });
        assert_rejected("a negative VAT rate", |p| p.vat.rates[0].rate_bp = -1);
        assert_rejected("an IFU goods rate above 100 %", |p| {
            p.ifu.goods_rate_bp = 10_001;
        });
        assert_rejected("a negative IFU services rate", |p| {
            p.ifu.services_rate_bp = -1;
        });
        assert_rejected("a negative auto-entrepreneur rate", |p| {
            p.ifu.auto_entrepreneur_rate_bp = -50;
        });
        assert_rejected("no stamp brackets", |p| p.stamp_duty.brackets.clear());
        assert_rejected("brackets out of order", |p| {
            p.stamp_duty.brackets.swap(0, 1)
        });
        assert_rejected("a first bracket inside the exemption", |p| {
            p.stamp_duty.brackets[0].up_to = Some(Money(30_000));
        });
        assert_rejected("two brackets with one bound", |p| {
            p.stamp_duty.brackets[1].up_to = Some(Money(3_000_000));
        });
        assert_rejected("an open bracket before the last", |p| {
            p.stamp_duty.brackets[1].up_to = None;
        });
        assert_rejected("a bounded last bracket", |p| {
            p.stamp_duty.brackets[2].up_to = Some(Money(50_000_000));
        });
        assert_rejected("a zero tranche", |p| p.stamp_duty.tranche = Money::ZERO);
        assert_rejected("a zero duty per tranche", |p| {
            p.stamp_duty.brackets[0].per_tranche = Money::ZERO;
        });
        assert_rejected("a negative minimum", |p| p.stamp_duty.minimum = Money(-500));
        assert_rejected("a negative exemption", |p| {
            p.stamp_duty.exempt_up_to = Money(-1);
        });
    }

    #[test]
    fn parse_rejects_missing_fields_and_invalid_values() {
        assert_eq!(parse("{}").unwrap_err().code(), "datapack_invalid");
        let without_zakat = BUNDLED_JSON.replace("\"zakat\"", "\"zakaat\"");
        assert_eq!(
            parse(&without_zakat).unwrap_err().code(),
            "datapack_invalid"
        );
        let progressive = BUNDLED_JSON.replace("\"progressive\": false", "\"progressive\": true");
        assert_ne!(
            progressive, BUNDLED_JSON,
            "precondition: the replacement applied"
        );
        assert!(
            parse(&progressive).unwrap().stamp_duty.progressive,
            "a progressive scale is a legal choice of a future law, not a broken pack"
        );
    }

    #[test]
    fn a_new_finance_law_only_needs_a_new_pack() {
        // Values change with each Loi de Finances: a pack with other rates, another scale and a
        // 70th wilaya is structurally valid; the 2026 values are pinned by the tests above.
        let mut pack = bundled().unwrap();
        pack.vat.rates.retain(|r| r.rate_bp != 900);
        pack.vat.rates[0].rate_bp = 1_700;
        // The staples taxed at the dropped rate move with it (their tax code must be known).
        for item in &mut pack.regulated.items {
            if item.tax_code == "TVA9" {
                item.tax_code = "TVA19".into();
            }
        }
        pack.stamp_duty.progressive = true;
        pack.stamp_duty.brackets[0].per_tranche = Money(120);
        let mut seventieth = pack.wilayas[68].clone();
        seventieth.code = "70".into();
        pack.wilayas.push(seventieth);
        assert_eq!(pack.validate(), Ok(()));
        assert_eq!(parse(&serde_json::to_string(&pack).unwrap()).unwrap(), pack);
    }

    #[test]
    fn the_2026_pack_names_the_g50_boxes_and_the_cash_deduction_limit() {
        let pack = bundled().unwrap();
        for key in G50_KEYS {
            assert!(pack.fiscal.g50.boxes.contains_key(key), "{key}");
        }
        assert_eq!(pack.fiscal.g50.boxes["caHt9"], "1");
        assert_eq!(pack.fiscal.g50.boxes["tva19"], "8");
        assert_eq!(pack.fiscal.g50.exempt_block, "19");
        assert_eq!(pack.fiscal.cash_deduction_limit, Money(100_000_000));
        assert_eq!(pack.fiscal.retention_years, 10);
    }

    #[test]
    fn a_pack_without_a_g50_box_or_with_a_short_retention_is_refused() {
        let mut value: serde_json::Value = serde_json::from_str(BUNDLED_JSON).unwrap();
        value["fiscal"]["g50"]["boxes"]
            .as_object_mut()
            .unwrap()
            .remove("tva9");
        assert_eq!(
            parse(&value.to_string()).unwrap_err().code(),
            "datapack_invalid"
        );
        let mut value: serde_json::Value = serde_json::from_str(BUNDLED_JSON).unwrap();
        value["fiscal"]["retentionYears"] = serde_json::json!(5);
        assert_eq!(
            parse(&value.to_string()).unwrap_err().code(),
            "datapack_invalid"
        );
        let mut value: serde_json::Value = serde_json::from_str(BUNDLED_JSON).unwrap();
        value["fiscal"]["g50"]["exemptBlock"] = serde_json::json!("7");
        assert_eq!(
            parse(&value.to_string()).unwrap_err().code(),
            "datapack_invalid"
        );
    }

    #[test]
    fn serialization_reproduces_the_bundled_json() {
        // Every field of the file is modelled, with the same camelCase names and integer
        // centimes, so the pack reaches the UI unchanged.
        let pack = bundled().unwrap();
        let file: serde_json::Value = serde_json::from_str(BUNDLED_JSON).unwrap();
        assert_eq!(serde_json::to_value(&pack).unwrap(), file);
        assert_eq!(parse(&serde_json::to_string(&pack).unwrap()).unwrap(), pack);
    }

    #[test]
    fn the_2026_pack_carries_the_scale_layouts() {
        let pack = bundled().unwrap();
        let prefixes: Vec<&str> = pack
            .scale_barcodes
            .default_layouts
            .iter()
            .map(|l| l.prefix.as_str())
            .collect();
        assert_eq!(prefixes, ["21", "22", "27", "28", "29"]);
        assert!(!pack.scale_barcodes.source.is_empty());
        let mut broken = pack.clone();
        broken.scale_barcodes.default_layouts[0].value_digits = 6;
        assert_eq!(broken.validate().unwrap_err().code(), "datapack_invalid");
    }

    #[test]
    fn the_2026_ceilings_operators_and_zakat_rate_are_pinned() {
        let pack = bundled().unwrap();
        let ceiling = |code: &str| {
            pack.regulated
                .items
                .iter()
                .find(|i| i.code == code)
                .map(|i| i.ceiling)
        };
        assert_eq!(ceiling("oil_5l"), Some(Money(65_000)));
        assert_eq!(ceiling("oil_1l"), Some(Money(12_500)));
        assert_eq!(ceiling("sugar_bulk_kg"), Some(Money(9_000)));
        assert_eq!(ceiling("sugar_packed_kg"), Some(Money(9_500)));
        assert_eq!(ceiling("milk_sachet_1l"), Some(Money(2_500)));
        assert_eq!(ceiling("semolina_extra_10kg"), Some(Money(41_000)));
        assert_eq!(pack.regulated.items.len(), 14);
        let extra_1kg = pack
            .regulated
            .items
            .iter()
            .find(|i| i.code == "semolina_extra_1kg")
            .unwrap();
        assert_eq!(extra_1kg.retail_margin, Some(Money(300)));
        let prefixes: Vec<(&str, &str)> = pack
            .mobile_operators
            .iter()
            .flat_map(|o| {
                o.prefixes
                    .iter()
                    .map(move |p| (o.code.as_str(), p.as_str()))
            })
            .collect();
        assert_eq!(
            prefixes,
            [("ooredoo", "05"), ("mobilis", "06"), ("djezzy", "07")]
        );
        assert_eq!(pack.zakat.rate_bp, 250);
    }

    #[test]
    fn broken_ceilings_or_operators_are_refused() {
        let mut v: serde_json::Value = serde_json::from_str(BUNDLED_JSON).unwrap();
        v["regulated"]["items"][0]["ceiling"] = serde_json::json!(0);
        assert!(parse(&v.to_string()).is_err());
        let mut v: serde_json::Value = serde_json::from_str(BUNDLED_JSON).unwrap();
        v["mobileOperators"][1]["prefixes"] = serde_json::json!(["6x"]);
        assert!(parse(&v.to_string()).is_err());
        let mut v: serde_json::Value = serde_json::from_str(BUNDLED_JSON).unwrap();
        v["regulated"]["items"][1]["code"] = serde_json::json!("oil_1l");
        assert!(parse(&v.to_string()).is_err(), "codes are unique");
    }
}

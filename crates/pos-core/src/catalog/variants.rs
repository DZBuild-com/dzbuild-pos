//! Variant matrix (PRD-020): from a variant model and 1–3 axes (size, colour, season), every
//! missing combination becomes a `variant` product with its reference, price, cost, optional
//! internal barcode and its axis values as attributes. Existing combinations are skipped.

use crate::audit::{self, Actor};
use crate::catalog::extras::{self, ProductExtrasInput};
use crate::catalog::products::{self, Barcode, Product, ProductInput, ProductKind};
use crate::catalog::{CODE_MAX, NAME_MAX};
use crate::error::AppError;
use crate::money::{Money, Qty};
use rusqlite::Transaction;
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VariantAxis {
    pub name: String,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VariantsInput {
    pub axes: Vec<VariantAxis>,
    /// Default: the model's sale price.
    pub sell_price: Option<Money>,
    /// Default: the model's cost.
    pub cost_price: Option<Money>,
    pub internal_barcodes: bool,
}

fn invalid() -> AppError {
    AppError::validation("axes", "variant_axes_invalid")
}

fn clean_axes(axes: &[VariantAxis]) -> Result<Vec<(String, Vec<String>)>, AppError> {
    if axes.is_empty() || axes.len() > 3 {
        return Err(invalid());
    }
    let mut out = Vec::new();
    let mut combinations = 1_usize;
    for axis in axes {
        let name = axis.name.trim().to_owned();
        let mut values: Vec<String> = Vec::new();
        for v in &axis.values {
            let v = v.trim().to_owned();
            if !v.is_empty() && v.chars().count() <= 30 && !values.contains(&v) {
                values.push(v);
            }
        }
        if name.is_empty() || name.chars().count() > 30 || values.is_empty() || values.len() > 30 {
            return Err(invalid());
        }
        combinations = combinations.saturating_mul(values.len());
        out.push((name, values));
    }
    if combinations > 200 {
        return Err(invalid());
    }
    Ok(out)
}

fn combinations(axes: &[(String, Vec<String>)]) -> Vec<Vec<(String, String)>> {
    let mut out: Vec<Vec<(String, String)>> = vec![Vec::new()];
    for (name, values) in axes {
        out = out
            .into_iter()
            .flat_map(|prefix| {
                values.iter().map(move |v| {
                    let mut next = prefix.clone();
                    next.push((name.clone(), v.clone()));
                    next
                })
            })
            .collect();
    }
    out
}

fn slug(values: &[String]) -> String {
    values
        .iter()
        .map(|v| {
            v.chars()
                .filter(char::is_ascii_alphanumeric)
                .collect::<String>()
                .to_ascii_uppercase()
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// The model's name followed by the values. Too long for a product name is the axes' fault
/// (`axes` / `too_long`), not a name error on a field the variants form does not show.
pub(crate) fn named(base: &str, values: &[String]) -> Result<String, AppError> {
    if base.is_empty() {
        return Ok(String::new());
    }
    let name = format!("{base} {}", values.join(" "));
    if name.chars().count() > NAME_MAX {
        return Err(AppError::validation("axes", "too_long"));
    }
    Ok(name)
}

/// At most `max` characters, without a dash or a space left dangling by the cut (a trailing
/// space would be trimmed by `products::create`, which would then store another reference than
/// the one found free).
fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let head: String = text.chars().take(max).collect();
    match head.trim_end_matches(|c: char| c == '-' || c.is_whitespace()) {
        "" => head,
        trimmed => trimmed.to_owned(),
    }
}

/// A free reference for a variant: `{model}-{slug}` (or the model's own reference when the
/// values leave nothing to slug, such as Arabic values), cut to `CODE_MAX`, then `-2`, `-3`, …
/// while it is taken. Variants made earlier in the same call are already rows of this
/// transaction, so one lookup covers both.
pub(crate) fn reference(
    tx: &Transaction<'_>,
    model: &str,
    values: &[String],
) -> Result<String, AppError> {
    let s = slug(values);
    let stem = if s.is_empty() {
        model.to_owned()
    } else {
        format!("{model}-{s}")
    };
    let taken = |candidate: &str| -> Result<bool, AppError> {
        Ok(tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM products WHERE reference = ?1)",
            [candidate],
            |r| r.get(0),
        )?)
    };
    let first = cut(&stem, CODE_MAX);
    if !taken(&first)? {
        return Ok(first);
    }
    for n in 2..=10_000_u32 {
        let suffix = format!("-{n}");
        let candidate = format!("{}{suffix}", cut(&first, CODE_MAX - suffix.len()));
        if !taken(&candidate)? {
            return Ok(candidate);
        }
    }
    Err(AppError::Internal("no free variant reference".into()))
}

pub fn generate(
    tx: &Transaction<'_>,
    actor: &Actor,
    parent_id: &str,
    input: VariantsInput,
) -> Result<Vec<Product>, AppError> {
    let parent = products::get(tx, parent_id)?;
    if parent.kind != ProductKind::VariantParent {
        return Err(AppError::validation("parentId", "variant_parent_required"));
    }
    let axes = clean_axes(&input.axes)?;
    let existing: BTreeSet<BTreeMap<String, String>> = {
        let mut stmt = tx.prepare(
            "SELECT e.attributes FROM products p JOIN product_extras e ON e.product_id = p.id
             WHERE p.parent_id = ?1",
        )?;
        let rows = stmt
            .query_map([parent_id], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        rows.iter()
            .filter_map(|a| serde_json::from_str(a).ok())
            .collect()
    };
    let mut made = Vec::new();
    for combo in combinations(&axes) {
        let attributes: BTreeMap<String, String> = combo.iter().cloned().collect();
        if existing.contains(&attributes) {
            continue;
        }
        let values: Vec<String> = combo.iter().map(|(_, v)| v.clone()).collect();
        let mut p = ProductInput::from(&parent);
        p.kind = ProductKind::Variant;
        p.parent_id = Some(parent.id.clone());
        p.track_stock = true;
        p.name_ar = named(&parent.name_ar, &values)?;
        p.name_fr = named(&parent.name_fr, &values)?;
        p.name_en = named(&parent.name_en, &values)?;
        p.reference = match &parent.reference {
            Some(r) => Some(reference(tx, r, &values)?),
            None => None,
        };
        p.sell_price = input.sell_price.unwrap_or(parent.sell_price);
        p.cost_price = input.cost_price.unwrap_or(parent.cost_price);
        p.plu = None;
        p.image = None;
        p.components = Vec::new();
        p.barcodes = if input.internal_barcodes {
            vec![Barcode {
                barcode: super::codes::next_internal_ean13(tx)?,
                pack_qty: Qty(1_000),
                label: None,
            }]
        } else {
            Vec::new()
        };
        let variant = products::create(tx, actor, p)?;
        extras::set_quiet(
            tx,
            &variant.id,
            ProductExtrasInput {
                attributes,
                ..ProductExtrasInput::default()
            },
        )?;
        made.push(products::get(tx, &variant.id)?);
    }
    audit::append(
        tx,
        &actor.event(
            "product.variants_generated",
            "product",
            Some(parent_id),
            json!({ "axes": input.axes, "created": made.iter().map(|p| &p.id).collect::<Vec<_>>() }),
        ),
    )?;
    Ok(made)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::products::{ProductInput, ProductKind};
    use crate::testkit::shop;

    #[test]
    fn a_size_by_colour_matrix_creates_each_variant_once() {
        let shop = shop();
        let mut input = ProductInput::simple(
            "T-shirt col rond",
            crate::catalog::UNIT_PIECE_ID,
            &shop.tva19,
            Money(150_000),
        );
        input.kind = ProductKind::VariantParent;
        input.track_stock = false;
        input.reference = Some("TS01".into());
        let parent = shop.product_with(input);
        let axes = vec![
            VariantAxis {
                name: "Taille".into(),
                values: vec!["S".into(), "M".into(), "L".into()],
            },
            VariantAxis {
                name: "Couleur".into(),
                values: vec!["Noir".into(), "Blanc".into()],
            },
        ];
        let actor = shop.owner.actor();
        let id = parent.id.clone();
        let first = VariantsInput {
            axes: axes.clone(),
            sell_price: None,
            cost_price: Some(Money(80_000)),
            internal_barcodes: true,
        };
        let made = shop
            .db
            .write(move |tx| generate(tx, &actor, &id, first))
            .unwrap();
        assert_eq!(made.len(), 6);
        assert_eq!(made[0].name_fr, "T-shirt col rond S Noir");
        assert_eq!(made[0].reference.as_deref(), Some("TS01-S-NOIR"));
        assert_eq!(
            (made[0].kind, made[0].sell_price, made[0].cost_price),
            (ProductKind::Variant, Money(150_000), Money(80_000))
        );
        assert!(crate::catalog::codes::is_valid_ean13(
            &made[0].barcodes[0].barcode
        ));
        let actor = shop.owner.actor();
        let id = parent.id.clone();
        let more = VariantsInput {
            axes: vec![
                VariantAxis {
                    name: "Taille".into(),
                    values: vec!["S".into(), "XL".into()],
                },
                VariantAxis {
                    name: "Couleur".into(),
                    values: vec!["Noir".into()],
                },
            ],
            sell_price: Some(Money(160_000)),
            cost_price: None,
            internal_barcodes: false,
        };
        let added = shop
            .db
            .write(move |tx| generate(tx, &actor, &id, more))
            .unwrap();
        assert_eq!(
            added.iter().map(|p| p.name_fr.as_str()).collect::<Vec<_>>(),
            ["T-shirt col rond XL Noir"]
        );
        let actor = shop.owner.actor();
        let id = parent.id.clone();
        let empty = VariantsInput {
            axes: vec![],
            sell_price: None,
            cost_price: None,
            internal_barcodes: false,
        };
        assert_eq!(
            shop.db
                .write(move |tx| generate(tx, &actor, &id, empty))
                .unwrap_err(),
            AppError::validation("axes", "variant_axes_invalid")
        );
    }

    fn model(shop: &crate::testkit::Shop, name: &str, reference: &str) -> Product {
        let mut input = ProductInput::simple(
            name,
            crate::catalog::UNIT_PIECE_ID,
            &shop.tva19,
            Money(150_000),
        );
        input.kind = ProductKind::VariantParent;
        input.track_stock = false;
        input.reference = Some(reference.into());
        shop.product_with(input)
    }

    fn run(
        shop: &crate::testkit::Shop,
        parent: &Product,
        axes: &[(&str, &[&str])],
    ) -> Result<Vec<Product>, AppError> {
        let actor = shop.owner.actor();
        let id = parent.id.clone();
        let input = VariantsInput {
            axes: axes
                .iter()
                .map(|(name, values)| VariantAxis {
                    name: (*name).into(),
                    values: values.iter().map(|v| (*v).into()).collect(),
                })
                .collect(),
            sell_price: None,
            cost_price: None,
            internal_barcodes: false,
        };
        shop.db.write(move |tx| generate(tx, &actor, &id, input))
    }

    fn references(made: &[Product]) -> Vec<&str> {
        made.iter().filter_map(|p| p.reference.as_deref()).collect()
    }

    #[test]
    fn arabic_values_get_unique_references_across_calls() {
        let shop = shop();
        let shirt = model(&shop, "T-shirt col rond", "TS01");
        let colours: &[&str] = &["أسود", "أحمر"];
        let made = run(
            &shop,
            &shirt,
            &[("Taille", &["S", "M"]), ("Couleur", colours)],
        )
        .unwrap();
        assert_eq!(made.len(), 4);
        assert_eq!(made[0].name_fr, "T-shirt col rond S أسود");
        assert_eq!(
            references(&made),
            ["TS01-S", "TS01-S-2", "TS01-M", "TS01-M-2"]
        );
        let white: &[&str] = &["أبيض"];
        let more = run(
            &shop,
            &shirt,
            &[("Taille", &["S", "M"]), ("Couleur", white)],
        )
        .unwrap();
        assert_eq!(references(&more), ["TS01-S-3", "TS01-M-3"]);
        // Values with nothing to slug: the model's reference numbered after what already exists.
        let dress = model(&shop, "Robe d'été", "RB01");
        let first = run(&shop, &dress, &[("Couleur", colours)]).unwrap();
        assert_eq!(references(&first), ["RB01-2", "RB01-3"]);
        let next = run(&shop, &dress, &[("Couleur", white)]).unwrap();
        assert_eq!(references(&next), ["RB01-4"]);
        // Values that differ only by case or punctuation.
        let cap = model(&shop, "Casquette", "CQ01");
        let sizes: &[&str] = &["S", "s", "S+"];
        let made = run(&shop, &cap, &[("Taille", sizes)]).unwrap();
        assert_eq!(references(&made), ["CQ01-S", "CQ01-S-2", "CQ01-S-3"]);
    }

    #[test]
    fn long_references_are_cut_and_long_names_are_an_axes_error() {
        let shop = shop();
        let long_ref = "R".repeat(44);
        let parent = model(&shop, "Pantalon", &long_ref);
        let made = run(&shop, &parent, &[("Taille", &["XXXL", "XXXLL", "XXXLLL"])]).unwrap();
        let refs = references(&made);
        assert_eq!(
            refs,
            [
                format!("{long_ref}-XXX"),
                format!("{long_ref}-X-2"),
                format!("{long_ref}-X-3")
            ]
        );
        assert!(
            refs.iter()
                .all(|r| r.chars().count() <= crate::catalog::CODE_MAX)
        );
        let long_name = model(&shop, &"Pantalon cargo ".repeat(6), "PC01");
        let v = "x".repeat(30);
        let err = run(
            &shop,
            &long_name,
            &[
                ("A", &[v.as_str()]),
                ("B", &[v.as_str()]),
                ("C", &[v.as_str()]),
            ],
        )
        .unwrap_err();
        assert_eq!(err, AppError::validation("axes", "too_long"));
    }
}

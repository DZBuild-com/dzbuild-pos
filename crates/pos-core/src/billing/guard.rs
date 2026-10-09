//! DOC-034 (décret 13-378 art. 7: consumer information "essentiellement en langue arabe"): a
//! consumer document or label printed without Arabic gets a warning suggesting AR + FR. The shop
//! chooses the scope: every consumer document, only some categories (food, cosmetics…), or off.

use crate::error::AppError;
use crate::lang::Lang;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GuardInput {
    pub lang: Lang,
    #[serde(default)]
    #[specta(optional)]
    pub second_lang: Option<Lang>,
    #[serde(default)]
    pub product_ids: Vec<String>,
    /// A ticket, or an invoice to a private person.
    pub consumer: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArabicWarning {
    pub suggested: Lang,
    /// The listed categories that triggered it (empty in `consumer` mode).
    pub category_ids: Vec<String>,
}

/// A category and its parents, closest first (a loop in the data stops after ten levels).
fn lineage(conn: &Connection, category_id: &str) -> Result<Vec<String>, AppError> {
    let mut out = vec![category_id.to_owned()];
    let mut current = category_id.to_owned();
    for _ in 0..10 {
        let parent: Option<String> = conn
            .query_row(
                "SELECT parent_id FROM categories WHERE id = ?1",
                [&current],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        match parent {
            Some(p) if !out.contains(&p) => {
                out.push(p.clone());
                current = p;
            }
            _ => break,
        }
    }
    Ok(out)
}

/// The warning to show before printing a consumer document without Arabic, per the setting
/// `documents.arabic_guard` (`off`, `consumer`, `categories`).
pub fn arabic_guard(
    conn: &Connection,
    input: &GuardInput,
) -> Result<Option<ArabicWarning>, AppError> {
    if !input.consumer || input.lang == Lang::Ar || input.second_lang == Some(Lang::Ar) {
        return Ok(None);
    }
    let mode: String = crate::settings::get(conn, "documents.arabic_guard")?;
    match mode.as_str() {
        "consumer" => Ok(Some(ArabicWarning {
            suggested: Lang::Ar,
            category_ids: Vec::new(),
        })),
        "categories" => {
            let listed: Vec<String> =
                crate::settings::get(conn, "documents.arabic_guard_categories")?;
            let mut hit: Vec<String> = Vec::new();
            for product_id in &input.product_ids {
                let category: Option<String> = conn
                    .query_row(
                        "SELECT category_id FROM products WHERE id = ?1",
                        [product_id],
                        |r| r.get(0),
                    )
                    .optional()?
                    .flatten();
                let Some(category) = category else { continue };
                for id in lineage(conn, &category)? {
                    if listed.contains(&id) && !hit.contains(&id) {
                        hit.push(id);
                    }
                }
            }
            Ok((!hit.is_empty()).then_some(ArabicWarning {
                suggested: Lang::Ar,
                category_ids: hit,
            }))
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::categories::{CategoryInput, create};
    use crate::money::Money;
    use crate::testkit::shop;
    use serde_json::json;

    #[test]
    fn consumer_documents_without_arabic_are_warned_per_setting() {
        let shop = shop();
        let milk = shop.product("Lait", Money(12_000), &shop.tva9);
        let q = |lang, second, consumer| GuardInput {
            lang,
            second_lang: second,
            product_ids: vec![milk.id.clone()],
            consumer,
        };
        let run = |i: GuardInput| shop.db.read(move |c| arabic_guard(c, &i)).unwrap();
        assert_eq!(
            run(q(Lang::Fr, None, true)).map(|w| w.suggested),
            Some(Lang::Ar)
        );
        assert_eq!(
            run(q(Lang::Fr, Some(Lang::Ar), true)),
            None,
            "bilingual AR + FR is fine"
        );
        assert_eq!(run(q(Lang::Ar, None, true)), None);
        assert_eq!(
            run(q(Lang::Fr, None, false)),
            None,
            "a B2B invoice is not consumer information"
        );
        shop.set("documents.arabic_guard", json!("off"));
        assert_eq!(run(q(Lang::En, None, true)), None);

        let actor = shop.owner.actor();
        let food = shop
            .db
            .write(move |tx| {
                create(
                    tx,
                    &actor,
                    CategoryInput {
                        name_fr: "Alimentation".into(),
                        ..CategoryInput::default()
                    },
                )
            })
            .unwrap();
        let actor = shop.owner.actor();
        let dairy_parent = food.id.clone();
        let dairy = shop
            .db
            .write(move |tx| {
                create(
                    tx,
                    &actor,
                    CategoryInput {
                        name_fr: "Laitiers".into(),
                        parent_id: Some(dairy_parent),
                        ..CategoryInput::default()
                    },
                )
            })
            .unwrap();
        let yoghurt = shop.product_with({
            let mut p = crate::catalog::products::ProductInput::simple(
                "Yaourt",
                crate::catalog::UNIT_PIECE_ID,
                &shop.tva9,
                Money(3_000),
            );
            p.category_id = Some(dairy.id.clone());
            p
        });
        shop.set("documents.arabic_guard", json!("categories"));
        shop.set("documents.arabic_guard_categories", json!([food.id]));
        let w = shop
            .db
            .read(move |c| {
                arabic_guard(
                    c,
                    &GuardInput {
                        lang: Lang::Fr,
                        second_lang: None,
                        product_ids: vec![yoghurt.id],
                        consumer: true,
                    },
                )
            })
            .unwrap();
        assert_eq!(
            w.map(|w| w.category_ids),
            Some(vec![food.id.clone()]),
            "a sub-category counts"
        );
        assert_eq!(
            run(q(Lang::Fr, None, true)),
            None,
            "milk has no listed category"
        );
    }
}

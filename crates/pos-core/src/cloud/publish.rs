//! Which products the owner shows in the online store (DZB-010): the list with search, and the
//! switch per product (published, or published as a draft that the storefront hides).

use super::store::MapEntity;
use crate::audit::{self, Actor};
use crate::catalog::{normalize, products};
use crate::error::AppError;
use crate::money::Money;
use rusqlite::{Connection, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PublishQuery {
    pub text: Option<String>,
    pub only_published: bool,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PublishRow {
    pub product_id: String,
    pub name: String,
    pub reference: Option<String>,
    pub price: Money,
    pub publish: bool,
    pub online_status: String,
    /// DZBuild's id once the product was sent.
    pub remote_id: Option<String>,
    pub has_image: bool,
    pub archived: bool,
}

/// The products (variant models and simple products, never a variant line) with their online
/// switch, published first; `text` searches like the product search (every word, folded). A
/// product without a switch of its own follows `cloud.publish.mode`, as the sync does
/// (`derive::publish_decision`): in `all` mode it shows checked, and unchecking excludes it.
pub fn list(
    conn: &Connection,
    store_id: &str,
    query: &PublishQuery,
) -> Result<Vec<PublishRow>, AppError> {
    let words = query
        .text
        .as_deref()
        .map(normalize::words)
        .unwrap_or_default();
    let pattern = |i: usize| {
        words
            .get(i)
            .map(|w| format!("%{w}%"))
            .unwrap_or_else(|| "%".to_owned())
    };
    let limit = i64::from(query.limit.unwrap_or(500).clamp(1, 5_000));
    let all = crate::settings::get::<String>(conn, "cloud.publish.mode")? == "all";
    let mut stmt = conn.prepare(
        "SELECT p.id, CASE WHEN p.name_fr <> '' THEN p.name_fr WHEN p.name_ar <> '' THEN p.name_ar ELSE p.name_en END,
                p.reference, p.sell_price, COALESCE(c.publish, ?8), COALESCE(c.online_status, 'active'), m.remote_id,
                p.image IS NOT NULL, p.archived_at IS NOT NULL
         FROM products p
         LEFT JOIN cloud_publish c ON c.product_id = p.id
         LEFT JOIN cloud_map m ON m.store_id = ?1 AND m.entity_type = ?2 AND m.local_id = p.id
         WHERE p.parent_id IS NULL
           AND (?3 = 0 OR COALESCE(c.publish, ?8) = 1)
           AND p.search_text LIKE ?4 AND p.search_text LIKE ?5 AND p.search_text LIKE ?6
         ORDER BY COALESCE(c.publish, ?8) DESC, p.name_fr, p.name_ar
         LIMIT ?7",
    )?;
    let rows = stmt
        .query_map(
            params![
                store_id,
                MapEntity::Product.as_str(),
                query.only_published,
                pattern(0),
                pattern(1),
                pattern(2),
                limit,
                all
            ],
            |r| {
                Ok(PublishRow {
                    product_id: r.get(0)?,
                    name: r.get(1)?,
                    reference: r.get(2)?,
                    price: r.get(3)?,
                    publish: r.get(4)?,
                    online_status: r.get(5)?,
                    remote_id: r.get(6)?,
                    has_image: r.get(7)?,
                    archived: r.get(8)?,
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Publishes (or withdraws) `ids`; `draft` keeps a published product off the storefront. The
/// change feed carries it to the next sync (Task 7 derives the upsert). One audit row when
/// anything changed; an unknown product refuses the whole change.
pub fn set(
    tx: &Transaction<'_>,
    actor: &Actor,
    ids: &[String],
    publish: bool,
    draft: bool,
    now: i64,
) -> Result<usize, AppError> {
    let status = if draft { "draft" } else { "active" };
    let mut changed = 0;
    for id in ids {
        products::get(tx, id)?;
        changed += tx.execute(
            "INSERT INTO cloud_publish (product_id, publish, online_status, updated_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (product_id) DO UPDATE SET publish = excluded.publish, online_status = excluded.online_status,
                 updated_at = excluded.updated_at
             WHERE cloud_publish.publish <> excluded.publish OR cloud_publish.online_status <> excluded.online_status",
            params![id, publish, status, now],
        )?;
    }
    if changed > 0 {
        audit::append(
            tx,
            &actor.event(
                "cloud.publish_changed",
                "product",
                None,
                json!({ "count": changed, "publish": publish, "draft": draft }),
            ),
        )?;
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::testkit::linked_shop;
    use crate::money::Money;
    use crate::testkit::NOON;

    #[test]
    fn publishing_is_listed_searched_and_journaled() {
        let (shop, _) = linked_shop();
        let milk = shop.product("Lait Candia 1 L", Money(11_000), &shop.tva19);
        let oil = shop.product("Huile Elio 5 L", Money(95_000), &shop.tva19);
        let actor = shop.owner.actor();
        let ids = vec![milk.id.clone(), oil.id.clone()];
        assert_eq!(
            shop.db
                .write(move |tx| set(tx, &actor, &ids, true, false, NOON))
                .unwrap(),
            2
        );
        let actor = shop.owner.actor();
        let ids = vec![oil.id.clone()];
        shop.db
            .write(move |tx| set(tx, &actor, &ids, true, true, NOON))
            .unwrap();
        let rows = shop
            .db
            .read(|c| {
                list(
                    c,
                    "141",
                    &PublishQuery {
                        text: None,
                        only_published: true,
                        limit: None,
                    },
                )
            })
            .unwrap();
        assert_eq!(rows.len(), 2);
        let oil_row = rows.iter().find(|r| r.name.starts_with("Huile")).unwrap();
        assert_eq!(
            (oil_row.publish, oil_row.online_status.as_str()),
            (true, "draft")
        );
        let found = shop
            .db
            .read(|c| {
                list(
                    c,
                    "141",
                    &PublishQuery {
                        text: Some("candia".into()),
                        only_published: false,
                        limit: None,
                    },
                )
            })
            .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(
            shop.count("SELECT COUNT(*) FROM audit_log WHERE action = 'cloud.publish_changed'"),
            2
        );
    }

    /// In `all` mode a product with no switch of its own is published (the sync sends it): it
    /// shows checked and under "Publiés seulement"; unchecking it excludes it.
    #[test]
    fn in_all_mode_every_product_without_its_own_switch_shows_published() {
        let (shop, _) = linked_shop();
        let milk = shop.product("Lait Candia", Money(10_000), &shop.tva19);
        let oil = shop.product("Huile Elio 5 L", Money(95_000), &shop.tva19);
        shop.set("cloud.publish.mode", json!("all"));
        let listed = |only_published: bool| {
            shop.db
                .read(move |c| {
                    list(
                        c,
                        "141",
                        &PublishQuery {
                            text: None,
                            only_published,
                            limit: None,
                        },
                    )
                })
                .unwrap()
        };
        let rows = listed(true);
        assert_eq!(rows.len(), 2, "both are published in all mode");
        assert!(rows.iter().all(|r| r.publish));
        let actor = shop.owner.actor();
        let ids = vec![oil.id.clone()];
        shop.db
            .write(move |tx| set(tx, &actor, &ids, false, false, NOON))
            .unwrap();
        let rows = listed(true);
        assert_eq!(
            rows.iter()
                .map(|r| r.product_id.as_str())
                .collect::<Vec<_>>(),
            [milk.id.as_str()],
            "unchecked: excluded"
        );
    }

    #[test]
    fn an_unknown_product_is_refused() {
        let (shop, _) = linked_shop();
        let actor = shop.owner.actor();
        let error = shop
            .db
            .write(move |tx| {
                set(
                    tx,
                    &actor,
                    &["00000000-0000-7000-8000-00000000dead".to_owned()],
                    true,
                    false,
                    NOON,
                )
            })
            .unwrap_err();
        assert_eq!(error.code(), "product_not_found");
    }
}

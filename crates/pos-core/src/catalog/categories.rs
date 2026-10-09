//! Hierarchical categories (PRD-001): families and sub-families, with a colour and an image for
//! the sale screen's grid.

use super::{clean_names, clean_opt};
use crate::audit::{self, Actor};
use crate::error::AppError;
use crate::ids::new_id;
use crate::time::now_ms;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: String,
    pub parent_id: Option<String>,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    /// `#RRGGBB`.
    pub color: Option<String>,
    pub sort_order: i64,
    /// Path of the image under the app's `media` folder.
    pub image: Option<String>,
    pub archived: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CategoryInput {
    pub parent_id: Option<String>,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub color: Option<String>,
    pub sort_order: i64,
    pub image: Option<String>,
}

impl From<&Category> for CategoryInput {
    fn from(c: &Category) -> CategoryInput {
        CategoryInput {
            parent_id: c.parent_id.clone(),
            name_ar: c.name_ar.clone(),
            name_fr: c.name_fr.clone(),
            name_en: c.name_en.clone(),
            color: c.color.clone(),
            sort_order: c.sort_order,
            image: c.image.clone(),
        }
    }
}

const SELECT: &str = "SELECT id, parent_id, name_ar, name_fr, name_en, color, sort_order, image,
        archived_at IS NOT NULL FROM categories";

fn map(row: &Row<'_>) -> rusqlite::Result<Category> {
    Ok(Category {
        id: row.get(0)?,
        parent_id: row.get(1)?,
        name_ar: row.get(2)?,
        name_fr: row.get(3)?,
        name_en: row.get(4)?,
        color: row.get(5)?,
        sort_order: row.get(6)?,
        image: row.get(7)?,
        archived: row.get(8)?,
    })
}

/// Categories by display order; archived ones only when asked.
pub fn list(conn: &Connection, include_archived: bool) -> Result<Vec<Category>, AppError> {
    let mut stmt = conn.prepare(&format!(
        "{SELECT} WHERE ?1 OR archived_at IS NULL ORDER BY sort_order, name_fr, name_ar, rowid"
    ))?;
    let categories = stmt
        .query_map([include_archived], map)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(categories)
}

pub fn get(conn: &Connection, id: &str) -> Result<Category, AppError> {
    conn.query_row(&format!("{SELECT} WHERE id = ?1"), [id], map)
        .optional()?
        .ok_or_else(|| AppError::not_found("category"))
}

fn is_hex_colour(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value.bytes().skip(1).all(|b| b.is_ascii_hexdigit())
}

fn clean(
    conn: &Connection,
    id: Option<&str>,
    input: &CategoryInput,
) -> Result<CategoryInput, AppError> {
    let (name_ar, name_fr, name_en) = clean_names(&input.name_ar, &input.name_fr, &input.name_en)?;
    let color = clean_opt(input.color.as_deref(), "color", 7)?;
    if color.as_deref().is_some_and(|c| !is_hex_colour(c)) {
        return Err(AppError::validation("color", "value_invalid"));
    }
    let parent_id = clean_opt(input.parent_id.as_deref(), "parentId", 36)?;
    if let Some(parent) = &parent_id {
        let found = get(conn, parent)
            .map_err(|_| AppError::validation("parentId", "category_not_found"))?;
        if found.archived {
            return Err(AppError::validation("parentId", "category_not_found"));
        }
        if let Some(id) = id
            && is_descendant_or_self(conn, parent, id)?
        {
            return Err(AppError::validation("parentId", "category_cycle"));
        }
    }
    Ok(CategoryInput {
        parent_id,
        name_ar,
        name_fr,
        name_en,
        color: color.map(|c| c.to_ascii_uppercase()),
        sort_order: input.sort_order,
        image: clean_opt(input.image.as_deref(), "image", 260)?,
    })
}

/// True when `candidate` is `ancestor` or lies below it.
fn is_descendant_or_self(
    conn: &Connection,
    candidate: &str,
    ancestor: &str,
) -> Result<bool, AppError> {
    let found: bool = conn.query_row(
        "WITH RECURSIVE up (id, parent_id) AS (
             SELECT id, parent_id FROM categories WHERE id = ?1
             UNION SELECT c.id, c.parent_id FROM categories c JOIN up ON c.id = up.parent_id)
         SELECT EXISTS (SELECT 1 FROM up WHERE id = ?2)",
        params![candidate, ancestor],
        |r| r.get(0),
    )?;
    Ok(found)
}

pub fn create(
    tx: &Transaction<'_>,
    actor: &Actor,
    input: CategoryInput,
) -> Result<Category, AppError> {
    let c = clean(tx, None, &input)?;
    let id = new_id();
    let now = now_ms();
    tx.execute(
        "INSERT INTO categories (id, parent_id, name_ar, name_fr, name_en, color, sort_order, image,
                                 created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
        params![id, c.parent_id, c.name_ar, c.name_fr, c.name_en, c.color, c.sort_order, c.image, now],
    )?;
    let category = get(tx, &id)?;
    audit::append(
        tx,
        &actor.event(
            "category.created",
            "category",
            Some(&id),
            json!({ "after": category }),
        ),
    )?;
    Ok(category)
}

pub fn update(
    tx: &Transaction<'_>,
    actor: &Actor,
    id: &str,
    input: CategoryInput,
) -> Result<Category, AppError> {
    let before = get(tx, id)?;
    let c = clean(tx, Some(id), &input)?;
    tx.execute(
        "UPDATE categories SET parent_id = ?2, name_ar = ?3, name_fr = ?4, name_en = ?5, color = ?6,
                sort_order = ?7, image = ?8, updated_at = ?9 WHERE id = ?1",
        params![id, c.parent_id, c.name_ar, c.name_fr, c.name_en, c.color, c.sort_order, c.image, now_ms()],
    )?;
    let after = get(tx, id)?;
    audit::append(
        tx,
        &actor.event(
            "category.updated",
            "category",
            Some(id),
            json!({ "before": before, "after": after }),
        ),
    )?;
    Ok(after)
}

/// Archives a category without active sub-categories (`category_has_children`). Its products
/// keep pointing at it; the grid stops showing it.
pub fn archive(tx: &Transaction<'_>, actor: &Actor, id: &str) -> Result<(), AppError> {
    let category = get(tx, id)?;
    let children: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM categories WHERE parent_id = ?1 AND archived_at IS NULL)",
        [id],
        |r| r.get(0),
    )?;
    if children {
        return Err(AppError::conflict("category_has_children"));
    }
    tx.execute(
        "UPDATE categories SET archived_at = ?2, updated_at = ?2 WHERE id = ?1",
        params![id, now_ms()],
    )?;
    audit::append(
        tx,
        &actor.event(
            "category.archived",
            "category",
            Some(id),
            json!({ "nameAr": category.name_ar, "nameFr": category.name_fr, "nameEn": category.name_en }),
        ),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn named(fr: &str, parent: Option<&str>) -> CategoryInput {
        CategoryInput {
            parent_id: parent.map(str::to_owned),
            name_fr: fr.into(),
            name_ar: "مواد غذائية".into(),
            ..CategoryInput::default()
        }
    }

    fn add(db: &Db, input: CategoryInput) -> Result<Category, AppError> {
        db.write(move |tx| create(tx, &Actor::default(), input))
    }

    #[test]
    fn families_and_sub_families() {
        let db = Db::open_memory().unwrap();
        let food = add(
            &db,
            CategoryInput {
                color: Some("#ffc300".into()),
                ..named("Alimentation", None)
            },
        )
        .unwrap();
        assert_eq!(food.color.as_deref(), Some("#FFC300"));
        let dairy = add(&db, named("Laitiers", Some(&food.id))).unwrap();
        assert_eq!(dairy.parent_id.as_deref(), Some(food.id.as_str()));
        assert_eq!(db.read(|c| list(c, false)).unwrap().len(), 2);
        let bad = add(
            &db,
            CategoryInput {
                color: Some("yellow".into()),
                ..named("X", None)
            },
        )
        .unwrap_err();
        assert_eq!(bad.payload().field.as_deref(), Some("color"));
        let orphan = add(
            &db,
            named("Y", Some("00000000-0000-7000-8000-000000000999")),
        )
        .unwrap_err();
        assert_eq!(
            (orphan.code(), orphan.payload().field),
            ("category_not_found".into(), Some("parentId".into()))
        );
    }

    #[test]
    fn a_category_cannot_move_below_itself() {
        let db = Db::open_memory().unwrap();
        let food = add(&db, named("Alimentation", None)).unwrap();
        let dairy = add(&db, named("Laitiers", Some(&food.id))).unwrap();
        let yogurt = add(&db, named("Yaourts", Some(&dairy.id))).unwrap();
        let (food_id, yogurt_id) = (food.id.clone(), yogurt.id.clone());
        let err = db
            .write(move |tx| {
                update(
                    tx,
                    &Actor::default(),
                    &food_id,
                    named("Alimentation", Some(&yogurt_id)),
                )
            })
            .unwrap_err();
        assert_eq!(err.code(), "category_cycle");
        let food_id = food.id.clone();
        let err = db
            .write(move |tx| {
                update(
                    tx,
                    &Actor::default(),
                    &food_id.clone(),
                    named("A", Some(&food_id)),
                )
            })
            .unwrap_err();
        assert_eq!(err.code(), "category_cycle");
    }

    #[test]
    fn archive_needs_the_children_archived_first() {
        let db = Db::open_memory().unwrap();
        let food = add(&db, named("Alimentation", None)).unwrap();
        let dairy = add(&db, named("Laitiers", Some(&food.id))).unwrap();
        let id = food.id.clone();
        let err = db
            .write(move |tx| archive(tx, &Actor::default(), &id))
            .unwrap_err();
        assert_eq!(err.code(), "category_has_children");
        let (d, f) = (dairy.id.clone(), food.id.clone());
        db.write(move |tx| {
            archive(tx, &Actor::default(), &d)?;
            archive(tx, &Actor::default(), &f)
        })
        .unwrap();
        assert!(db.read(|c| list(c, false)).unwrap().is_empty());
        assert_eq!(db.read(|c| list(c, true)).unwrap().len(), 2);
        let actions: String = db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT group_concat(action) FROM (SELECT action FROM audit_log ORDER BY seq)",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(
            actions,
            "category.created,category.created,category.archived,category.archived"
        );
    }
}

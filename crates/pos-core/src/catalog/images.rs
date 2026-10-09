//! Product photos (PRD-004): files in `<app_data>/media/products/`, named
//! `<product id>-<version>.jpg` (a new name on every change, so the webview never shows a stale
//! cached photo) with a `.thumb.jpg` beside it. The database keeps the relative name only; the
//! shell writes the files (Task 19) and serves them through the asset protocol scoped to
//! `media/`.

use crate::audit::{self, Actor};
use crate::catalog::products;
use crate::error::AppError;
use crate::ids::is_valid_id;
use crate::time::now_ms;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde_json::json;

pub const DIR: &str = "products";

pub fn file_name(product_id: &str, version: i64) -> String {
    format!("{DIR}/{product_id}-{version}.jpg")
}

pub fn thumb_name(image: &str) -> Option<String> {
    image
        .strip_suffix(".jpg")
        .filter(|_| is_valid_name(image))
        .map(|stem| format!("{stem}.thumb.jpg"))
}

/// `products/<uuid>-<digits>.jpg` and nothing else (no path can leave the media folder).
pub fn is_valid_name(image: &str) -> bool {
    let Some(rest) = image.strip_prefix("products/") else {
        return false;
    };
    let Some(stem) = rest.strip_suffix(".jpg") else {
        return false;
    };
    match stem.rsplit_once('-') {
        Some((id, version)) => {
            is_valid_id(id) && !version.is_empty() && version.bytes().all(|b| b.is_ascii_digit())
        }
        None => false,
    }
}

/// Sets (or clears) the photo; returns the previous name so the caller deletes its files after
/// the commit.
pub fn set(
    tx: &Transaction<'_>,
    actor: &Actor,
    product_id: &str,
    image: Option<&str>,
) -> Result<Option<String>, AppError> {
    if image.is_some_and(|i| !is_valid_name(i)) {
        return Err(AppError::validation("image", "value_invalid"));
    }
    let before = products::get(tx, product_id)?.image;
    tx.execute(
        "UPDATE products SET image = ?2, updated_at = ?3 WHERE id = ?1",
        params![product_id, image, now_ms()],
    )?;
    audit::append(
        tx,
        &actor.event(
            "product.image_changed",
            "product",
            Some(product_id),
            json!({ "before": before, "after": image }),
        ),
    )?;
    Ok(before)
}

/// The product whose reference (any case) or barcode is `code`: bulk photo import by file name.
pub fn find_by_code(conn: &Connection, code: &str) -> Result<Option<String>, AppError> {
    Ok(conn
        .prepare_cached(
            "SELECT id FROM products WHERE reference = ?1 COLLATE NOCASE AND archived_at IS NULL
             UNION ALL
             SELECT p.id FROM product_barcodes b JOIN products p ON p.id = b.product_id
             WHERE b.barcode = ?1 AND p.archived_at IS NULL
             LIMIT 1",
        )?
        .query_row([code], |r| r.get(0))
        .optional()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Money;
    use crate::testkit::shop;

    #[test]
    fn image_names_are_versioned_and_checked() {
        let name = file_name("0192f0c4-7a1b-7c3d-8e4f-5a6b7c8d9e0f", 1_790_593_200_000);
        assert_eq!(
            name,
            "products/0192f0c4-7a1b-7c3d-8e4f-5a6b7c8d9e0f-1790593200000.jpg"
        );
        assert!(is_valid_name(&name));
        assert_eq!(
            thumb_name(&name).as_deref(),
            Some("products/0192f0c4-7a1b-7c3d-8e4f-5a6b7c8d9e0f-1790593200000.thumb.jpg")
        );
        for bad in [
            "../pos.db",
            "products/x.jpg",
            "C:\\Windows\\a.jpg",
            "products/0192f0c4-7a1b-7c3d-8e4f-5a6b7c8d9e0f-1.png",
        ] {
            assert!(!is_valid_name(bad), "{bad}");
        }
    }

    #[test]
    fn setting_an_image_returns_the_previous_one_and_finds_products_by_code() {
        let shop = shop();
        let mut input = crate::catalog::products::ProductInput::simple(
            "Savon Palmolive",
            crate::catalog::UNIT_PIECE_ID,
            &shop.tva19,
            Money(12_000),
        );
        input.reference = Some("SAV-PALM".into());
        let p = shop.product_with(input);
        let first = file_name(&p.id, 1);
        let actor = shop.owner.actor();
        let (id, name) = (p.id.clone(), first.clone());
        assert_eq!(
            shop.db
                .write(move |tx| set(tx, &actor, &id, Some(&name)))
                .unwrap(),
            None
        );
        let actor = shop.owner.actor();
        let id = p.id.clone();
        assert_eq!(
            shop.db.write(move |tx| set(tx, &actor, &id, None)).unwrap(),
            Some(first)
        );
        assert_eq!(
            shop.db.read(|c| find_by_code(c, "sav-palm")).unwrap(),
            Some(p.id)
        );
        let actor = shop.owner.actor();
        let err = shop
            .db
            .write(move |tx| set(tx, &actor, "nope", Some("../x.jpg")))
            .unwrap_err();
        assert_eq!(err, AppError::validation("image", "value_invalid"));
    }
}

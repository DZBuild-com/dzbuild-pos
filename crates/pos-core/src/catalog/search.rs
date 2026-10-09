//! Product search for the till and the back office (spec §8, SAL-011/012): an exact code
//! (barcode, reference, PLU) short-circuits; otherwise every typed word is a prefix over the
//! normalised names, reference and barcodes (FTS5). Budget: < 30 ms on 50 000 products.

use super::normalize;
use super::products::{self, Product, ProductKind};
use crate::error::AppError;
use crate::money::{Money, Qty};
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::{Deserialize, Serialize};
use specta::Type;

/// One line of a result list: enough to show a tile or a row and to add the product.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProductHit {
    pub id: String,
    pub reference: Option<String>,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub kind: ProductKind,
    pub category_id: Option<String>,
    pub unit_code: String,
    pub sell_price: Money,
    /// The first barcode, for display.
    pub barcode: Option<String>,
    /// Stock over the storage locations (damaged goods excluded).
    pub stock: Qty,
    pub image: Option<String>,
    pub archived: bool,
    pub available_at_till: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchQuery {
    pub text: String,
    /// 1..=100, default 20.
    pub limit: Option<u32>,
    pub category_id: Option<String>,
    pub include_archived: bool,
}

/// What a scanned or typed code designates, and how many units one scan adds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ScanHit {
    pub product: Product,
    pub pack_qty: Qty,
    /// Set when the code is a weight/price/count label of a scale (spec §8): `pack_qty` is then
    /// the quantity the label means.
    pub scale: Option<super::scale_codes::ScaleScan>,
}

const HIT_COLUMNS: &str =
    "p.id, p.reference, p.name_ar, p.name_fr, p.name_en, p.kind, p.category_id,
    u.code, p.sell_price,
    (SELECT b.barcode FROM product_barcodes b WHERE b.product_id = p.id ORDER BY b.rowid LIMIT 1),
    COALESCE((SELECT SUM(l.qty) FROM stock_levels l JOIN stock_locations s ON s.id = l.location_id
              WHERE l.product_id = p.id AND s.kind = 'storage'), 0),
    p.image, p.archived_at IS NOT NULL, p.available_at_till";

fn map_hit(row: &Row<'_>) -> rusqlite::Result<ProductHit> {
    Ok(ProductHit {
        id: row.get(0)?,
        reference: row.get(1)?,
        name_ar: row.get(2)?,
        name_fr: row.get(3)?,
        name_en: row.get(4)?,
        kind: row.get(5)?,
        category_id: row.get(6)?,
        unit_code: row.get(7)?,
        sell_price: row.get(8)?,
        barcode: row.get(9)?,
        stock: row.get(10)?,
        image: row.get(11)?,
        archived: row.get(12)?,
        available_at_till: row.get(13)?,
    })
}

fn limit_of(query: &SearchQuery) -> i64 {
    i64::from(query.limit.unwrap_or(20).clamp(1, 100))
}

/// Exact code matches first (barcode, then reference, then PLU), then prefix matches ranked by
/// relevance. Archived products only with `include_archived`. An empty text lists the category.
pub fn search(conn: &Connection, query: &SearchQuery) -> Result<Vec<ProductHit>, AppError> {
    let limit = limit_of(query);
    let text = query.text.trim();
    if text.is_empty() {
        return list_by_category(conn, query.category_id.as_deref(), limit);
    }
    let mut hits: Vec<ProductHit> = Vec::new();
    let exact_sql = format!(
        "SELECT {HIT_COLUMNS} FROM products p JOIN units u ON u.id = p.unit_id
         WHERE (?2 OR p.archived_at IS NULL) AND (?3 IS NULL OR p.category_id = ?3) AND p.id IN (
             SELECT product_id FROM product_barcodes WHERE barcode = ?1
             UNION SELECT id FROM products WHERE reference = ?1
             UNION SELECT id FROM products WHERE plu = CAST(?1 AS INTEGER) AND ?1 NOT GLOB '*[^0-9]*')"
    );
    let mut stmt = conn.prepare_cached(&exact_sql)?;
    let exact = stmt
        .query_map(
            params![text, query.include_archived, query.category_id],
            map_hit,
        )?
        .collect::<Result<Vec<_>, _>>()?;
    hits.extend(exact);
    let Some(fts) = normalize::fts_query(text) else {
        return Ok(hits);
    };
    let fts_sql = format!(
        "SELECT {HIT_COLUMNS} FROM products_fts f
         JOIN products p ON p.rowid = f.rowid JOIN units u ON u.id = p.unit_id
         WHERE products_fts MATCH ?1 AND (?2 OR p.archived_at IS NULL)
           AND (?3 IS NULL OR p.category_id = ?3)
         ORDER BY f.rank LIMIT ?4"
    );
    let mut stmt = conn.prepare_cached(&fts_sql)?;
    let ranked = stmt.query_map(
        params![fts, query.include_archived, query.category_id, limit],
        map_hit,
    )?;
    for hit in ranked {
        let hit = hit?;
        if !hits.iter().any(|h| h.id == hit.id) {
            hits.push(hit);
        }
    }
    hits.truncate(usize::try_from(limit).unwrap_or(20));
    Ok(hits)
}

/// Sellable products of a category (or of every category) for the sale screen's grid, by name.
pub fn list_by_category(
    conn: &Connection,
    category_id: Option<&str>,
    limit: i64,
) -> Result<Vec<ProductHit>, AppError> {
    let sql = format!(
        "SELECT {HIT_COLUMNS} FROM products p JOIN units u ON u.id = p.unit_id
         WHERE p.archived_at IS NULL AND p.available_at_till = 1 AND p.kind <> 'variant'
           AND (?1 IS NULL OR p.category_id = ?1)
         ORDER BY p.name_fr, p.name_ar, p.name_en LIMIT ?2"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let hits = stmt
        .query_map(params![category_id, limit.clamp(1, 500)], map_hit)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(hits)
}

/// Which products the back-office list shows, by their archived state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveFilter {
    /// On sale (not archived), at the till or not.
    #[default]
    Active,
    Archived,
    All,
}

/// The last row of a catalogue page: the next page starts after it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CatalogCursor {
    pub name_fr: String,
    pub name_ar: String,
    pub name_en: String,
    pub id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CatalogQuery {
    pub category_id: Option<String>,
    pub archived: ArchiveFilter,
    pub after: Option<CatalogCursor>,
    /// 1..=200, default 100.
    pub limit: Option<u32>,
}

/// The back-office product list (M1 products screen): every product of the catalogue, sold at
/// the till or not, archived ones on demand, by name, a page at a time (keyset on the names and
/// the id, so a page never skips or repeats a row).
pub fn catalog(conn: &Connection, query: &CatalogQuery) -> Result<Vec<ProductHit>, AppError> {
    let archived = match query.archived {
        ArchiveFilter::Active => Some(false),
        ArchiveFilter::Archived => Some(true),
        ArchiveFilter::All => None,
    };
    let after = query.after.as_ref();
    let sql = format!(
        "SELECT {HIT_COLUMNS} FROM products p JOIN units u ON u.id = p.unit_id
         WHERE (?1 IS NULL OR p.category_id = ?1)
           AND (?2 IS NULL OR (p.archived_at IS NOT NULL) = ?2)
           AND (?3 IS NULL OR (p.name_fr, p.name_ar, p.name_en, p.id) > (?3, ?4, ?5, ?6))
         ORDER BY p.name_fr, p.name_ar, p.name_en, p.id LIMIT ?7"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let hits = stmt
        .query_map(
            params![
                query.category_id,
                archived,
                after.map(|a| &a.name_fr),
                after.map(|a| &a.name_ar),
                after.map(|a| &a.name_en),
                after.map(|a| &a.id),
                i64::from(query.limit.unwrap_or(100).clamp(1, 200))
            ],
            map_hit,
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(hits)
}

/// The sizes/colours of a variant model, for the variant picker (SAL-023).
pub fn variants_of(conn: &Connection, parent_id: &str) -> Result<Vec<ProductHit>, AppError> {
    let sql = format!(
        "SELECT {HIT_COLUMNS} FROM products p JOIN units u ON u.id = p.unit_id
         WHERE p.parent_id = ?1 AND p.archived_at IS NULL ORDER BY p.rowid"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let hits = stmt
        .query_map([parent_id], map_hit)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(hits)
}

/// A scanned or typed code: a barcode (with its pack quantity), else a reference, else a PLU
/// number, else a weight/price label of a scale (`scale_codes::resolve`); archived products are
/// not found (SAL-012).
pub fn find_by_code(conn: &Connection, code: &str) -> Result<Option<ScanHit>, AppError> {
    let code = code.trim();
    if code.is_empty() {
        return Ok(None);
    }
    let found: Option<(String, Qty)> = conn
        .query_row(
            "SELECT product_id, pack_qty FROM product_barcodes WHERE barcode = ?1
             UNION ALL SELECT id, 1000 FROM products WHERE reference = ?1
             UNION ALL SELECT id, 1000 FROM products WHERE plu = CAST(?1 AS INTEGER) AND ?1 NOT GLOB '*[^0-9]*'
             LIMIT 1",
            [code],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((id, pack_qty)) = found else {
        return Ok(super::scale_codes::resolve(conn, code)?.map(|hit| ScanHit {
            product: hit.product,
            pack_qty: hit.qty,
            scale: Some(hit.scan),
        }));
    };
    let product = products::get(conn, &id)?;
    if product.archived {
        return Ok(None);
    }
    Ok(Some(ScanHit {
        product,
        pack_qty,
        scale: None,
    }))
}

/// Rebuilds the full-text index from `products` (repair after a VACUUM or a restore of an old
/// copy); callers audit it as part of their operation.
pub fn rebuild_index(tx: &Transaction<'_>) -> Result<(), AppError> {
    tx.execute(
        "INSERT INTO products_fts (products_fts) VALUES ('rebuild')",
        [],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::Actor;
    use crate::catalog::products::{Barcode, ProductInput};
    use crate::catalog::{UNIT_KG_ID, UNIT_PIECE_ID, tax_rates};
    use crate::db::Db;

    fn shop() -> (Db, String) {
        let db = Db::open_memory().unwrap();
        let pack = crate::datapack::bundled().unwrap();
        db.write(move |tx| crate::catalog::ensure_defaults(tx, &pack))
            .unwrap();
        let tva = db
            .read(|c| tax_rates::by_code(c, "TVA19"))
            .unwrap()
            .unwrap()
            .id;
        for (ar, fr, en, barcode, reference) in [
            (
                "الحليب",
                "Lait demi-écrémé",
                "Semi-skimmed milk",
                "6130000000017",
                "LT-1",
            ),
            (
                "قهوة",
                "Café moulu",
                "Ground coffee",
                "6130000000024",
                "CF-1",
            ),
            ("سكر", "Sucre", "Sugar", "6130000000031", "SC-1"),
            ("شاي", "Thé vert", "Green tea", "6130000000048", "TH-1"),
        ] {
            let mut input = ProductInput::simple(fr, UNIT_PIECE_ID, &tva, Money(10_000));
            input.name_ar = ar.into();
            input.name_en = en.into();
            input.reference = Some(reference.into());
            input.barcodes = vec![Barcode {
                barcode: barcode.into(),
                pack_qty: Qty(1_000),
                label: None,
            }];
            db.write(move |tx| products::create(tx, &Actor::default(), input))
                .unwrap();
        }
        (db, tva)
    }

    fn names(db: &Db, text: &str) -> Vec<String> {
        let query = SearchQuery {
            text: text.into(),
            ..SearchQuery::default()
        };
        db.read(move |c| search(c, &query))
            .unwrap()
            .into_iter()
            .map(|h| h.name_fr)
            .collect()
    }

    #[test]
    fn prefix_words_in_any_language_and_without_accents() {
        let (db, _) = shop();
        assert_eq!(names(&db, "lait"), ["Lait demi-écrémé"]);
        assert_eq!(names(&db, "CAFE mou"), ["Café moulu"]);
        assert_eq!(names(&db, "ecre"), ["Lait demi-écrémé"]);
        assert_eq!(
            names(&db, "حليب"),
            ["Lait demi-écrémé"],
            "without the article"
        );
        assert_eq!(names(&db, "الحل"), ["Lait demi-écrémé"]);
        assert_eq!(names(&db, "قهوه"), ["Café moulu"], "ta marbuta typed as ha");
        assert_eq!(names(&db, "green"), ["Thé vert"]);
        assert!(names(&db, "zzz").is_empty());
        assert!(names(&db, "\"*").is_empty());
    }

    #[test]
    fn exact_codes_come_first() {
        let (db, _) = shop();
        assert_eq!(names(&db, "6130000000031"), ["Sucre"]);
        assert_eq!(names(&db, "th-1"), ["Thé vert"], "reference, any case");
        let mut prefix = names(&db, "613000000");
        prefix.sort();
        assert_eq!(
            prefix,
            ["Café moulu", "Lait demi-écrémé", "Sucre", "Thé vert"]
        );
        // An exact barcode is listed before prefix matches of other products.
        let query = SearchQuery {
            text: "6130000000048".into(),
            ..SearchQuery::default()
        };
        let first = db.read(move |c| search(c, &query)).unwrap();
        assert_eq!(first[0].name_fr, "Thé vert");
    }

    #[test]
    fn archived_products_are_hidden_unless_asked() {
        let (db, _) = shop();
        let id = db
            .read(|c| find_by_code(c, "SC-1"))
            .unwrap()
            .unwrap()
            .product
            .id;
        db.write(move |tx| products::archive(tx, &Actor::default(), &id))
            .unwrap();
        assert!(names(&db, "sucre").is_empty());
        let query = SearchQuery {
            text: "sucre".into(),
            include_archived: true,
            ..SearchQuery::default()
        };
        let hits = db.read(move |c| search(c, &query)).unwrap();
        assert!(hits[0].archived);
        assert!(
            db.read(|c| find_by_code(c, "6130000000031"))
                .unwrap()
                .is_none()
        );
    }

    /// The back-office list shows what the till grid hides (not sold at the till, archived on
    /// demand) and pages by name without skipping or repeating a row.
    #[test]
    fn the_catalogue_pages_every_product_by_name() {
        let (db, tva) = shop();
        let mut back_room = ProductInput::simple("Carton vide", UNIT_PIECE_ID, &tva, Money(0));
        back_room.available_at_till = false;
        db.write(move |tx| products::create(tx, &Actor::default(), back_room))
            .unwrap();
        let sugar = db
            .read(|c| find_by_code(c, "SC-1"))
            .unwrap()
            .unwrap()
            .product
            .id;
        db.write(move |tx| products::archive(tx, &Actor::default(), &sugar))
            .unwrap();
        let list = |archived: ArchiveFilter, after: Option<CatalogCursor>, limit: u32| {
            let query = CatalogQuery {
                archived,
                after,
                limit: Some(limit),
                ..CatalogQuery::default()
            };
            db.read(move |c| catalog(c, &query)).unwrap()
        };
        let names =
            |hits: &[ProductHit]| hits.iter().map(|h| h.name_fr.clone()).collect::<Vec<_>>();
        let first = list(ArchiveFilter::Active, None, 2);
        assert_eq!(names(&first), ["Café moulu", "Carton vide"]);
        assert!(
            !first[1].available_at_till,
            "not sold at the till, still listed"
        );
        let last = &first[1];
        let cursor = CatalogCursor {
            name_fr: last.name_fr.clone(),
            name_ar: last.name_ar.clone(),
            name_en: last.name_en.clone(),
            id: last.id.clone(),
        };
        let second = list(ArchiveFilter::Active, Some(cursor), 2);
        assert_eq!(names(&second), ["Lait demi-écrémé", "Thé vert"]);
        let archived = list(ArchiveFilter::Archived, None, 100);
        assert_eq!(names(&archived), ["Sucre"]);
        assert!(archived[0].archived);
        assert_eq!(list(ArchiveFilter::All, None, 100).len(), 5);
    }

    #[test]
    fn scan_codes_carry_the_pack_quantity_and_plu() {
        let (db, tva) = shop();
        let mut tomato = ProductInput::simple("Tomate", UNIT_KG_ID, &tva, Money(12_000));
        tomato.plu = Some(42);
        tomato.is_weighed = true;
        tomato.barcodes = vec![Barcode {
            barcode: "CARTON-T".into(),
            pack_qty: Qty(24_000),
            label: None,
        }];
        db.write(move |tx| products::create(tx, &Actor::default(), tomato))
            .unwrap();
        let carton = db.read(|c| find_by_code(c, " CARTON-T ")).unwrap().unwrap();
        assert_eq!(
            (carton.product.name_fr.as_str(), carton.pack_qty),
            ("Tomate", Qty(24_000))
        );
        let plu = db.read(|c| find_by_code(c, "42")).unwrap().unwrap();
        assert_eq!((plu.product.plu, plu.pack_qty), (Some(42), Qty(1_000)));
        assert!(db.read(|c| find_by_code(c, "4x2")).unwrap().is_none());
        assert!(
            db.read(|c| find_by_code(c, "42x")).unwrap().is_none(),
            "a PLU is digits only"
        );
        assert!(names(&db, "42 kg").is_empty());
        assert!(db.read(|c| find_by_code(c, "")).unwrap().is_none());
    }

    #[test]
    fn grid_and_rebuild() {
        let (db, _) = shop();
        let grid = db.read(|c| list_by_category(c, None, 50)).unwrap();
        let names: Vec<&str> = grid.iter().map(|h| h.name_fr.as_str()).collect();
        assert_eq!(
            names,
            ["Café moulu", "Lait demi-écrémé", "Sucre", "Thé vert"]
        );
        assert_eq!(grid[0].stock, Qty::ZERO);
        db.write(|tx| rebuild_index(tx)).unwrap();
        assert_eq!(self::names(&db, "sucr"), ["Sucre"]);
    }

    /// Spec §2 budget: product search < 30 ms on 50 000 products (reference PC: Celeron J1900,
    /// HDD). Run on demand, in release:
    /// `cargo test -p pos-core --release search_is_fast_on_50k_products -- --ignored --nocapture`
    #[test]
    #[ignore = "benchmark: seeds 50 000 products"]
    fn search_is_fast_on_50k_products() {
        let (db, tva) = shop();
        db.write(move |tx| {
            let mut insert = tx.prepare(
                "INSERT INTO products (id, reference, name_ar, name_fr, search_text, unit_id, tax_rate_id,
                                       sell_price, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 10000, 0, 0)",
            )?;
            let ar = ["حليب", "قهوة", "سكر", "زيت", "خبز", "جبن", "ماء", "شاي", "أرز", "عصير"];
            let fr = ["Lait", "Café", "Sucre", "Huile", "Pain", "Fromage", "Eau", "Thé", "Riz", "Jus"];
            let brands = ["Candia", "Soummam", "Cevital", "Hamoud", "Ifri", "Elio", "Safia", "Ramdy", "Danone", "Tchina"];
            for i in 0..50_000_usize {
                let name_ar = format!("ال{} {} {i}", ar[i % 10], brands[i / 10 % 10]);
                let name_fr = format!("{} {} {}g", fr[i % 10], brands[i / 10 % 10], i % 997);
                let reference = format!("REF-{i:06}");
                let barcode = format!("200{i:010}");
                let st = normalize::search_text([name_ar.as_str(), name_fr.as_str(), reference.as_str(), barcode.as_str()]);
                insert.execute(params![crate::ids::new_id(), reference, name_ar, name_fr, st, UNIT_PIECE_ID, tva])?;
            }
            Ok(())
        })
        .unwrap();
        for text in [
            "la",
            "lait",
            "lait cand",
            "حليب",
            "الحليب",
            "حل",
            "ref-0001",
            "cafe so",
            "12",
            "REF-049999",
        ] {
            let query = SearchQuery {
                text: text.into(),
                ..SearchQuery::default()
            };
            let (hits, elapsed) = db
                .read(move |c| {
                    search(c, &query)?; // warm the statement cache
                    let start = std::time::Instant::now();
                    for _ in 0..10 {
                        search(c, &query)?;
                    }
                    Ok((search(c, &query)?.len(), start.elapsed() / 10))
                })
                .unwrap();
            println!("search {text:>12}: {hits:>3} hits in {elapsed:?}");
            assert!(
                elapsed < std::time::Duration::from_millis(30),
                "{text}: {elapsed:?}"
            );
        }
    }
}

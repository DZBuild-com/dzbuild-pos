//! The variant children of migration 0010: each child's canonical key and its row in
//! `cloud_variant_map`. The parent stays in `cloud_map` (entity product).

use crate::catalog::extras;
use crate::error::AppError;
use crate::money::Money;
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use std::collections::BTreeMap;
use unicode_normalization::UnicodeNormalization;

/// The canonical key of a variant (wire contract v1.12): per axis the name and the value
/// trimmed, NFC, lower case, `=` and `|` encoded as `%3D` and `%7C`, pairs `name=value`
/// sorted by the encoded name (byte order) and joined with `|`, e.g. `couleur=noir|taille=41`.
/// The platform computes the same key in PHP (`mb_strtolower`, same rules, byte-order sort).
pub fn variant_key(attributes: &BTreeMap<String, String>) -> String {
    let fold = |s: &str| {
        s.trim()
            .nfc()
            .collect::<String>()
            .to_lowercase()
            .replace('=', "%3D")
            .replace('|', "%7C")
    };
    let mut pairs: Vec<(String, String)> =
        attributes.iter().map(|(n, v)| (fold(n), fold(v))).collect();
    pairs.sort();
    pairs
        .iter()
        .map(|(n, v)| format!("{n}={v}"))
        .collect::<Vec<_>>()
        .join("|")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantMapRow {
    pub local_id: String,
    pub parent_id: String,
    pub remote_product_id: String,
    pub variant_key: String,
    pub price_online: Option<Money>,
    pub updated_at: i64,
}

const ROW_SELECT: &str = "SELECT local_id, parent_id, remote_product_id, variant_key, price_online,
        updated_at FROM cloud_variant_map";

fn map_row(r: &Row<'_>) -> rusqlite::Result<VariantMapRow> {
    Ok(VariantMapRow {
        local_id: r.get(0)?,
        parent_id: r.get(1)?,
        remote_product_id: r.get(2)?,
        variant_key: r.get(3)?,
        price_online: r.get(4)?,
        updated_at: r.get(5)?,
    })
}

pub fn map_put(tx: &Transaction<'_>, store_id: &str, row: &VariantMapRow) -> Result<(), AppError> {
    tx.execute(
        "INSERT INTO cloud_variant_map (store_id, local_id, parent_id, remote_product_id, variant_key,
             price_online, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT (store_id, local_id) DO UPDATE SET parent_id = excluded.parent_id,
             remote_product_id = excluded.remote_product_id, variant_key = excluded.variant_key,
             price_online = excluded.price_online, updated_at = excluded.updated_at",
        params![
            store_id,
            row.local_id,
            row.parent_id,
            row.remote_product_id,
            row.variant_key,
            row.price_online,
            row.updated_at
        ],
    )?;
    Ok(())
}

pub fn map_get(
    conn: &Connection,
    store_id: &str,
    local_id: &str,
) -> Result<Option<VariantMapRow>, AppError> {
    Ok(conn
        .query_row(
            &format!("{ROW_SELECT} WHERE store_id = ?1 AND local_id = ?2"),
            params![store_id, local_id],
            map_row,
        )
        .optional()?)
}

/// The local child mapped to `variant_key` under the remote product, if any.
pub fn map_local(
    conn: &Connection,
    store_id: &str,
    remote_product_id: &str,
    variant_key: &str,
) -> Result<Option<String>, AppError> {
    Ok(conn
        .query_row(
            "SELECT local_id FROM cloud_variant_map
             WHERE store_id = ?1 AND remote_product_id = ?2 AND variant_key = ?3",
            params![store_id, remote_product_id, variant_key],
            |r| r.get(0),
        )
        .optional()?)
}

/// The mapped children of a local parent, by key.
pub fn map_children(
    conn: &Connection,
    store_id: &str,
    parent_id: &str,
) -> Result<Vec<VariantMapRow>, AppError> {
    let mut stmt = conn.prepare(&format!(
        "{ROW_SELECT} WHERE store_id = ?1 AND parent_id = ?2 ORDER BY variant_key"
    ))?;
    let rows = stmt
        .query_map(params![store_id, parent_id], map_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn map_forget(tx: &Transaction<'_>, store_id: &str, local_id: &str) -> Result<(), AppError> {
    tx.execute(
        "DELETE FROM cloud_variant_map WHERE store_id = ?1 AND local_id = ?2",
        params![store_id, local_id],
    )?;
    Ok(())
}

/// A child's axis values (`product_extras.attributes`); empty for a product without extras.
pub fn attributes_of(
    conn: &Connection,
    product_id: &str,
) -> Result<BTreeMap<String, String>, AppError> {
    Ok(extras::get(conn, product_id)?.attributes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::products::{ProductInput, ProductKind};
    use crate::catalog::variants::{VariantAxis, VariantsInput, generate};
    use crate::testkit::{Shop, shop};

    fn key(pairs: &[(&str, &str)]) -> String {
        variant_key(
            &pairs
                .iter()
                .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
                .collect(),
        )
    }

    #[test]
    fn the_key_folds_case_and_spaces_and_sorts_the_axes() {
        assert_eq!(
            key(&[("Taille", "41"), ("Couleur", "Noir")]),
            "couleur=noir|taille=41"
        );
        assert_eq!(
            key(&[(" Couleur ", "NOIR  "), ("TAILLE", " 41")]),
            "couleur=noir|taille=41"
        );
        assert_eq!(key(&[]), "");
    }

    #[test]
    fn the_key_sorts_by_the_folded_name_not_the_insertion_order() {
        let upper_first = key(&[("Zone", "A"), ("couleur", "Noir")]);
        let lower_first = key(&[("couleur", "Noir"), ("Zone", "A")]);
        assert_eq!(upper_first, "couleur=noir|zone=a");
        assert_eq!(upper_first, lower_first);
    }

    #[test]
    fn arabic_values_keep_their_letters() {
        assert_eq!(
            key(&[("المقاس", " 41 "), ("اللون", "أسود")]),
            "اللون=أسود|المقاس=41"
        );
    }

    #[test]
    fn the_key_is_nfc() {
        assert_eq!(key(&[("Couleur", "Cre\u{0300}me")]), "couleur=crème");
        assert_eq!(
            key(&[("Couleur", "CRE\u{0300}ME")]),
            key(&[("couleur", "crème")])
        );
    }

    #[test]
    fn separators_inside_a_name_or_a_value_are_encoded() {
        assert_eq!(key(&[("Taille", "4=1")]), "taille=4%3D1");
        assert_eq!(key(&[("A|B", "x|y")]), "a%7Cb=x%7Cy");
    }

    /// A T-shirt family made the way the till makes one: sizes S and M, colours Noir and Blanc.
    fn family(shop: &Shop) -> (String, Vec<String>) {
        let mut input = ProductInput::simple(
            "T-shirt",
            crate::catalog::UNIT_PIECE_ID,
            &shop.tva19,
            Money(150_000),
        );
        input.kind = ProductKind::VariantParent;
        input.track_stock = false;
        input.reference = Some("TS01".into());
        let parent = shop.product_with(input).id;
        let (actor, id) = (shop.owner.actor(), parent.clone());
        let children = shop
            .db
            .write(move |tx| {
                generate(
                    tx,
                    &actor,
                    &id,
                    VariantsInput {
                        axes: vec![
                            VariantAxis {
                                name: "Taille".into(),
                                values: vec!["S".into(), "M".into()],
                            },
                            VariantAxis {
                                name: "Couleur".into(),
                                values: vec!["Noir".into(), "Blanc".into()],
                            },
                        ],
                        sell_price: None,
                        cost_price: None,
                        internal_barcodes: false,
                    },
                )
            })
            .unwrap();
        (parent, children.into_iter().map(|c| c.id).collect())
    }

    fn row(local_id: &str, parent_id: &str, key: &str) -> VariantMapRow {
        VariantMapRow {
            local_id: local_id.to_owned(),
            parent_id: parent_id.to_owned(),
            remote_product_id: "88121".into(),
            variant_key: key.to_owned(),
            price_online: None,
            updated_at: 1,
        }
    }

    #[test]
    fn a_generated_child_reads_its_axis_values() {
        let shop = shop();
        let (parent, children) = family(&shop);
        let child = children[0].clone();
        let attributes = shop.db.read(move |c| attributes_of(c, &child)).unwrap();
        assert_eq!(
            attributes,
            BTreeMap::from([
                ("Couleur".to_owned(), "Noir".to_owned()),
                ("Taille".to_owned(), "S".to_owned()),
            ])
        );
        assert_eq!(variant_key(&attributes), "couleur=noir|taille=s");
        assert!(
            shop.db
                .read(move |c| attributes_of(c, &parent))
                .unwrap()
                .is_empty(),
            "the parent has no extras row"
        );
    }

    #[test]
    fn the_variant_map_round_trips_per_store() {
        let shop = shop();
        let (parent, children) = family(&shop);
        let (first, second) = (children[0].clone(), children[1].clone());
        let rows = [
            row(&first, &parent, "couleur=noir|taille=s"),
            row(&second, &parent, "couleur=blanc|taille=s"),
        ];
        let put = rows.clone();
        shop.db
            .write(move |tx| put.iter().try_for_each(|r| map_put(tx, "141", r)))
            .unwrap();
        let id = first.clone();
        assert_eq!(
            shop.db.read(move |c| map_get(c, "141", &id)).unwrap(),
            Some(rows[0].clone())
        );
        let id = first.clone();
        assert!(
            shop.db
                .read(move |c| map_get(c, "152", &id))
                .unwrap()
                .is_none(),
            "per store"
        );
        assert_eq!(
            shop.db
                .read(|c| map_local(c, "141", "88121", "couleur=noir|taille=s"))
                .unwrap(),
            Some(first.clone())
        );
        assert_eq!(
            shop.db
                .read(|c| map_local(c, "141", "88121", "couleur=rouge|taille=s"))
                .unwrap(),
            None
        );
        let p = parent.clone();
        assert_eq!(
            shop.db.read(move |c| map_children(c, "141", &p)).unwrap(),
            [rows[1].clone(), rows[0].clone()],
            "ordered by key"
        );
    }

    #[test]
    fn a_second_put_replaces_the_row_and_forget_removes_it() {
        let shop = shop();
        let (parent, children) = family(&shop);
        let child = children[0].clone();
        let mut updated = row(&child, &parent, "couleur=noir|taille=s");
        updated.price_online = Some(Money(455_000));
        updated.updated_at = 2;
        let (first, again) = (
            row(&child, &parent, "couleur=noir|taille=s"),
            updated.clone(),
        );
        shop.db
            .write(move |tx| {
                map_put(tx, "141", &first)?;
                map_put(tx, "141", &again)
            })
            .unwrap();
        let id = child.clone();
        assert_eq!(
            shop.db.read(move |c| map_get(c, "141", &id)).unwrap(),
            Some(updated)
        );
        let id = child.clone();
        shop.db
            .write(move |tx| {
                map_forget(tx, "141", &id)?;
                map_forget(tx, "141", &id)
            })
            .unwrap();
        let id = child.clone();
        assert!(
            shop.db
                .read(move |c| map_get(c, "141", &id))
                .unwrap()
                .is_none()
        );
        let p = parent.clone();
        assert!(
            shop.db
                .read(move |c| map_children(c, "141", &p))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_map_row_needs_a_real_product() {
        let shop = shop();
        let (parent, _) = family(&shop);
        let ghost = row("00000000-0000-7000-8000-0000000000ff", &parent, "taille=s");
        let err = shop
            .db
            .write(move |tx| map_put(tx, "141", &ghost))
            .unwrap_err();
        assert_eq!(err.code(), "constraint_violation");
    }
}

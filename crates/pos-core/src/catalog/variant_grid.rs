//! The stock-by-variant grid (VRT-060, SAL-023): a variant model's variants laid out by two of
//! their axes (size × colour), each cell the variant's stock (at a location or everywhere), price,
//! reference and barcode. The axes are the attribute names M2's generator wrote (alphabetical:
//! they are JSON object keys); `row_axis` picks the rows (default: the first name), the next axis
//! gives the columns, any third axis is listed in `otherAxes` (its values show in the reference).

use crate::catalog::products;
use crate::error::AppError;
use crate::money::{Money, Qty};
use rusqlite::Connection;
use serde::Serialize;
use specta::Type;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GridCell {
    pub variant_id: String,
    pub row_value: String,
    /// `None` for a one-axis model.
    pub col_value: Option<String>,
    pub reference: Option<String>,
    /// The variant's first barcode.
    pub barcode: Option<String>,
    pub on_hand: Qty,
    pub price: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VariantGrid {
    pub parent_id: String,
    pub row_axis: String,
    /// `None` for a one-axis model.
    pub col_axis: Option<String>,
    /// The row axis values, in the variants' order.
    pub rows: Vec<String>,
    pub cols: Vec<String>,
    pub cells: Vec<GridCell>,
    /// The axes beyond the two of the grid.
    pub other_axes: Vec<String>,
}

/// One live variant as read: id, reference, price, axis values, barcode, stock.
type VariantRow = (
    String,
    Option<String>,
    Money,
    BTreeMap<String, String>,
    Option<String>,
    Qty,
);

pub fn grid(
    conn: &Connection,
    parent_id: &str,
    location_id: Option<&str>,
    row_axis: Option<&str>,
) -> Result<VariantGrid, AppError> {
    products::get(conn, parent_id)?;
    let mut stmt = conn.prepare(
        "SELECT p.id, p.reference, p.sell_price, e.attributes,
                (SELECT b.barcode FROM product_barcodes b WHERE b.product_id = p.id
                 ORDER BY b.rowid LIMIT 1),
                COALESCE((SELECT SUM(l.qty) FROM stock_levels l
                          WHERE l.product_id = p.id AND (?2 IS NULL OR l.location_id = ?2)), 0)
         FROM products p LEFT JOIN product_extras e ON e.product_id = p.id
         WHERE p.parent_id = ?1 AND p.archived_at IS NULL
         ORDER BY p.rowid",
    )?;
    let found = stmt
        .query_map(rusqlite::params![parent_id, location_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Money>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Qty>(5)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut axes: Vec<String> = Vec::new();
    let mut variants: Vec<VariantRow> = Vec::with_capacity(found.len());
    for (id, reference, price, attrs, barcode, on_hand) in found {
        let values: BTreeMap<String, String> = attrs
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?
            .unwrap_or_default();
        for k in values.keys() {
            if !axes.contains(k) {
                axes.push(k.clone());
            }
        }
        variants.push((id, reference, price, values, barcode, on_hand));
    }
    let row_axis = match row_axis {
        Some(a) if axes.iter().any(|x| x == a) => a.to_owned(),
        Some(_) => return Err(AppError::validation("rowAxis", "variant_axes_invalid")),
        None => axes
            .first()
            .cloned()
            .ok_or_else(|| AppError::validation("parentId", "variant_axes_invalid"))?,
    };
    let col_axis = axes.iter().find(|a| **a != row_axis).cloned();
    let other_axes = axes
        .iter()
        .filter(|a| **a != row_axis && Some(*a) != col_axis.as_ref())
        .cloned()
        .collect();
    let (mut rows, mut cols): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    let mut cells = Vec::with_capacity(variants.len());
    for (variant_id, reference, price, values, barcode, on_hand) in variants {
        let row_value = values.get(&row_axis).cloned().unwrap_or_default();
        let col_value = col_axis.as_ref().and_then(|c| values.get(c).cloned());
        if !rows.contains(&row_value) {
            rows.push(row_value.clone());
        }
        if let Some(c) = &col_value
            && !cols.contains(c)
        {
            cols.push(c.clone());
        }
        cells.push(GridCell {
            variant_id,
            row_value,
            col_value,
            reference,
            barcode,
            on_hand,
            price,
        });
    }
    Ok(VariantGrid {
        parent_id: parent_id.to_owned(),
        row_axis,
        col_axis,
        rows,
        cols,
        cells,
        other_axes,
    })
}

//! A4 label sheets (LBL-013): the common planches and where each label goes, starting at any
//! position of a partly used sheet. Positions are tenths of a millimetre from the top and the
//! start edge (the right edge of an Arabic sheet); the print host draws the cells (Task 17).

use super::build::MAX_LABELS_PER_BATCH;
use crate::error::AppError;
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LabelSheetFormat {
    pub id: String,
    pub columns: u8,
    pub rows: u8,
    pub label_w_dmm: u16,
    pub label_h_dmm: u16,
    pub margin_top_dmm: u16,
    pub margin_start_dmm: u16,
    pub gap_x_dmm: u16,
    pub gap_y_dmm: u16,
}

fn format(
    id: &str,
    columns: u8,
    rows: u8,
    (w, h): (u16, u16),
    (top, start): (u16, u16),
    gap_x: u16,
) -> LabelSheetFormat {
    LabelSheetFormat {
        id: id.to_owned(),
        columns,
        rows,
        label_w_dmm: w,
        label_h_dmm: h,
        margin_top_dmm: top,
        margin_start_dmm: start,
        gap_x_dmm: gap_x,
        gap_y_dmm: 0,
    }
}

/// 3×7 (70 × 42.3 mm), 3×8 (70 × 37 mm), 4×10 (48.5 × 25.4 mm), 5×13 (38.1 × 21.2 mm): the
/// sheets sold in Algerian stationery shops; the print dialog's offsets absorb small drifts.
pub fn sheet_formats() -> Vec<LabelSheetFormat> {
    vec![
        format("a4_21_70x42", 3, 7, (700, 423), (4, 0), 0),
        format("a4_24_70x37", 3, 8, (700, 370), (5, 0), 0),
        format("a4_40_48x25", 4, 10, (485, 254), (215, 80), 0),
        format("a4_65_38x21", 5, 13, (381, 212), (107, 47), 25),
    ]
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LabelSheetCell {
    pub page: u16,
    pub row: u8,
    pub column: u8,
    pub x_dmm: u16,
    pub y_dmm: u16,
    /// Index of the label in the flattened batch.
    pub label: u32,
}

/// Cells for `copies[i]` copies of label `i`, from position `start` (1-based, row by row).
pub fn plan(
    format: &LabelSheetFormat,
    start: u16,
    copies: &[u16],
) -> Result<Vec<LabelSheetCell>, AppError> {
    let columns = u32::from(format.columns.max(1));
    let per_page = columns * u32::from(format.rows.max(1));
    if start == 0 || u32::from(start) > per_page {
        return Err(AppError::validation("start", "value_invalid"));
    }
    let total: u32 = copies.iter().map(|c| u32::from(*c)).sum();
    if total == 0 {
        return Err(AppError::conflict("labels_empty"));
    }
    if total > MAX_LABELS_PER_BATCH {
        return Err(AppError::rejected(
            json!({ "total": total, "max": MAX_LABELS_PER_BATCH }),
            "labels_too_many",
        ));
    }
    let overflow = || AppError::Internal("sheet position overflow".into());
    let step_x = u32::from(format.label_w_dmm) + u32::from(format.gap_x_dmm);
    let step_y = u32::from(format.label_h_dmm) + u32::from(format.gap_y_dmm);
    let mut cells = Vec::with_capacity(total as usize);
    let mut slot = u32::from(start) - 1;
    for (index, n) in copies.iter().enumerate() {
        let label = u32::try_from(index).map_err(|_| overflow())?;
        for _ in 0..*n {
            let (page, pos) = (slot / per_page, slot % per_page);
            let (row, column) = (pos / columns, pos % columns);
            let x = u32::from(format.margin_start_dmm) + column * step_x;
            let y = u32::from(format.margin_top_dmm) + row * step_y;
            cells.push(LabelSheetCell {
                page: u16::try_from(page).map_err(|_| overflow())?,
                row: u8::try_from(row).map_err(|_| overflow())?,
                column: u8::try_from(column).map_err(|_| overflow())?,
                x_dmm: u16::try_from(x).map_err(|_| overflow())?,
                y_dmm: u16::try_from(y).map_err(|_| overflow())?,
                label,
            });
            slot += 1;
        }
    }
    Ok(cells)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format(id: &str) -> LabelSheetFormat {
        sheet_formats().into_iter().find(|f| f.id == id).unwrap()
    }

    #[test]
    fn the_four_a4_formats_fit_the_page() {
        for f in sheet_formats() {
            let width = u32::from(f.margin_start_dmm)
                + u32::from(f.columns) * u32::from(f.label_w_dmm)
                + u32::from(f.columns - 1) * u32::from(f.gap_x_dmm);
            let height = u32::from(f.margin_top_dmm)
                + u32::from(f.rows) * u32::from(f.label_h_dmm)
                + u32::from(f.rows - 1) * u32::from(f.gap_y_dmm);
            assert!(width <= 2_100 && height <= 2_970, "{}", f.id);
        }
        let ids: Vec<String> = sheet_formats().into_iter().map(|f| f.id).collect();
        assert_eq!(
            ids,
            ["a4_21_70x42", "a4_24_70x37", "a4_40_48x25", "a4_65_38x21"]
        );
    }

    #[test]
    fn a_partly_used_sheet_starts_at_the_chosen_position_and_overflows_to_a_new_page() {
        let f = format("a4_24_70x37");
        let cells = plan(&f, 23, &[2, 3]).unwrap();
        let places: Vec<(u16, u8, u8, u32)> = cells
            .iter()
            .map(|c| (c.page, c.row, c.column, c.label))
            .collect();
        assert_eq!(
            places,
            [
                (0, 7, 1, 0),
                (0, 7, 2, 0),
                (1, 0, 0, 1),
                (1, 0, 1, 1),
                (1, 0, 2, 1)
            ]
        );
        assert_eq!((cells[0].x_dmm, cells[0].y_dmm), (700, 5 + 7 * 370));
        assert_eq!(
            plan(&f, 0, &[1]).unwrap_err().payload().field.as_deref(),
            Some("start")
        );
        assert_eq!(
            plan(&f, 25, &[1]).unwrap_err().payload().field.as_deref(),
            Some("start")
        );
        assert_eq!(plan(&f, 1, &[]).unwrap_err().code(), "labels_empty");
    }
}

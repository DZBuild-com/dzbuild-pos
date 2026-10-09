//! TSPL/TSPL2 for Xprinter, TSC and Smart POS label printers (research 05 §2.3): the texts of a
//! row of labels as one `BITMAP`, barcodes as `BARCODE`/`QRCODE`, `PRINT 1,<rows>`.

use super::layout::{BarcodeBox, dropped_from, place};
use super::model::{LabelData, LabelGroup, LabelSymbology, LabelTemplate};
use super::profile::{LabelMedia, LabelProfile};
use super::{body, column_x, mm, printer_safe, row_bitmap};
use crate::error::HwError;
use crate::raster::Typesetter;
use pos_core::labels::LabelWarning;

/// The wide bars of Code 39 and ITF for narrow bars of `m` dots, in a ratio the TSPL manual
/// lists for them (1:2, 1:3, 2:5): 2:5 for an even `m` (5 dots at 2), else 1:3.
fn wide_bars(m: u32) -> u32 {
    if m.is_multiple_of(2) {
        m * 5 / 2
    } else {
        m * 3
    }
}

/// Code 39 for `data`: `"39"` is full ASCII on TSPL2 printers and standard on TSPL ones, which
/// print every capital, digit, `-`, `.` and space alike; `$ / + %` are shift characters in full
/// ASCII, so data carrying them is sent as standard Code 39 (`"39S"`), as the preview draws it.
fn code39_type(data: &str) -> &'static str {
    if data.bytes().any(|b| b"$/+%".contains(&b)) {
        "39S"
    } else {
        "39"
    }
}

pub(crate) fn barcode(b: &BarcodeBox, x: u32) -> Result<String, HwError> {
    let data = printer_safe(&b.data)?;
    crate::barcode::check(b.symbology, data)?;
    let (y, h, m) = (b.y, b.height, b.module);
    Ok(match b.symbology {
        LabelSymbology::Ean13 => format!(
            "BARCODE {x},{y},\"EAN13\",{h},1,0,{m},{m},\"{}\"",
            body(data, 12)?
        ),
        LabelSymbology::Ean8 => format!(
            "BARCODE {x},{y},\"EAN8\",{h},1,0,{m},{m},\"{}\"",
            body(data, 7)?
        ),
        LabelSymbology::UpcA => format!(
            "BARCODE {x},{y},\"UPCA\",{h},1,0,{m},{m},\"{}\"",
            body(data, 11)?
        ),
        LabelSymbology::Code128 => format!("BARCODE {x},{y},\"128\",{h},1,0,{m},{m},\"{data}\""),
        LabelSymbology::Qr => format!("QRCODE {x},{y},M,{m},A,0,\"{data}\""),
        LabelSymbology::Code39 => format!(
            "BARCODE {x},{y},\"{}\",{h},1,0,{m},{},\"{data}\"",
            code39_type(data),
            wide_bars(m)
        ),
        LabelSymbology::Itf => format!(
            "BARCODE {x},{y},\"25\",{h},1,0,{m},{},\"{data}\"",
            wide_bars(m)
        ),
    })
}

/// One command and its line end.
fn line(out: &mut Vec<u8>, text: &str) {
    out.extend_from_slice(text.as_bytes());
    out.extend_from_slice(b"\r\n");
}

pub fn job(
    group: &LabelGroup,
    profile: &LabelProfile,
    ts: &mut Typesetter,
) -> Result<Vec<u8>, HwError> {
    job_noting(group, profile, ts, &mut Vec::new())
}

/// [`job`], noting in `warnings` each label whose layout leaves out its lot or expiry date.
pub(crate) fn job_noting(
    group: &LabelGroup,
    profile: &LabelProfile,
    ts: &mut Typesetter,
    warnings: &mut Vec<LabelWarning>,
) -> Result<Vec<u8>, HwError> {
    let tpl: &LabelTemplate = &group.template;
    let d = profile.dpi.dots_per_mm();
    let mut out: Vec<u8> = Vec::new();
    line(
        &mut out,
        &format!(
            "SIZE {} mm,{} mm",
            mm(tpl.row_width_dmm()),
            mm(u32::from(tpl.height_dmm))
        ),
    );
    line(
        &mut out,
        &match profile.media {
            LabelMedia::Gap => format!("GAP {} mm,0 mm", mm(u32::from(tpl.gap_dmm))),
            LabelMedia::BlackMark => format!("BLINE {} mm,0 mm", mm(u32::from(tpl.gap_dmm))),
            LabelMedia::Continuous => "GAP 0 mm,0 mm".to_owned(),
        },
    );
    line(&mut out, "DIRECTION 1");
    line(
        &mut out,
        &format!(
            "REFERENCE {},{}",
            u32::from(profile.offset_x_dmm) * d / 10,
            u32::from(profile.offset_y_dmm) * d / 10
        ),
    );
    line(&mut out, &format!("DENSITY {}", profile.darkness));
    line(&mut out, &format!("SPEED {}", profile.speed));
    for label in &group.labels {
        write_label(&mut out, tpl, label, profile, ts, warnings)?;
    }
    Ok(out)
}

fn write_label(
    out: &mut Vec<u8>,
    tpl: &LabelTemplate,
    label: &LabelData,
    profile: &LabelProfile,
    ts: &mut Typesetter,
    warnings: &mut Vec<LabelWarning>,
) -> Result<(), HwError> {
    let d = profile.dpi.dots_per_mm();
    let placed = place(tpl, label, d, ts)?;
    warnings.extend(dropped_from(tpl, label, &placed));
    let commands = match &placed.barcode {
        Some(b) => (0..u32::from(tpl.columns))
            .map(|c| barcode(b, b.x + column_x(tpl, c, d)))
            .collect::<Result<Vec<_>, _>>()?,
        None => Vec::new(),
    };
    let row = row_bitmap(&placed, tpl, d);
    line(out, "CLS");
    out.extend_from_slice(format!("BITMAP 0,0,{},{},0,", row.row_bytes(), row.height()).as_bytes());
    let data = row.rows(0, row.height());
    if profile.bitmap_black_is_zero {
        out.extend(data.iter().map(|b| !b));
    } else {
        out.extend_from_slice(data);
    }
    out.extend_from_slice(b"\r\n");
    for command in commands {
        line(out, &command);
    }
    let rows = u32::from(label.copies.max(1)).div_ceil(u32::from(tpl.columns.max(1)));
    line(out, &format!("PRINT 1,{rows}"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::labels::dump::dump;
    use crate::labels::model::{LabelBarcode, LabelGroup, LabelLang, LabelSymbology};
    use crate::labels::profile::{LabelDpi, LabelPreset, LabelProfile};
    use crate::testdata::{milk_label, oil_shelf_label};

    fn group(pair: (LabelTemplate, LabelData)) -> LabelGroup {
        LabelGroup {
            template: pair.0,
            labels: vec![pair.1],
        }
    }

    fn payload(bytes: &[u8]) -> Vec<u8> {
        let start = bytes.windows(7).position(|w| w == b"BITMAP ").unwrap();
        let header_end = start
            + bytes[start..]
                .iter()
                .enumerate()
                .filter(|(_, b)| **b == b',')
                .nth(4)
                .unwrap()
                .0
            + 1;
        let end = bytes[header_end..]
            .windows(5)
            .position(|w| w == b"\r\nBAR")
            .unwrap()
            + header_end;
        bytes[header_end..end].to_vec()
    }

    #[test]
    fn tspl_40x25_fr_ean13() {
        let bytes = job(
            &group(milk_label(LabelLang::Fr)),
            &LabelProfile::preset(LabelPreset::Xprinter),
            &mut Typesetter::new(),
        )
        .unwrap();
        let text = dump(&bytes);
        for part in [
            "SIZE 40 mm,25 mm\r\n",
            "GAP 2 mm,0 mm\r\n",
            "DENSITY 8\r\n",
            "SPEED 4\r\n",
            "CLS\r\n",
            "BITMAP 0,0,40,200,0,",
            "\"EAN13\"",
            "\"613012345678\"",
            "PRINT 1,3\r\n",
        ] {
            assert!(text.contains(part), "{part} in\n{text}");
        }
        insta::assert_snapshot!("tspl_40x25_fr_ean13", text);
    }

    #[test]
    fn tspl_bitmap_polarity_follows_the_profile() {
        let g = group(milk_label(LabelLang::Fr));
        let tsc = LabelProfile::preset(LabelPreset::Tsc);
        let plain = LabelProfile {
            bitmap_black_is_zero: false,
            ..tsc.clone()
        };
        let (inverted, straight) = (
            payload(&job(&g, &tsc, &mut Typesetter::new()).unwrap()),
            payload(&job(&g, &plain, &mut Typesetter::new()).unwrap()),
        );
        assert_eq!(inverted.len(), 40 * 200);
        assert!(inverted.iter().zip(&straight).all(|(a, b)| *a == !*b));
    }

    #[test]
    fn two_up_rows_repeat_the_label_and_round_the_copies_up() {
        let (mut tpl, mut label) = milk_label(LabelLang::Fr);
        tpl.width_dmm = 400;
        tpl.columns = 2;
        tpl.column_gap_dmm = 20;
        label.copies = 3;
        let text = dump(
            &job(
                &group((tpl, label)),
                &LabelProfile::preset(LabelPreset::Xprinter),
                &mut Typesetter::new(),
            )
            .unwrap(),
        );
        assert!(text.contains("SIZE 82 mm,25 mm\r\n"));
        assert_eq!(text.matches("BARCODE ").count(), 2);
        assert!(
            text.contains("PRINT 1,2\r\n"),
            "3 copies on 2-up stock: 2 rows"
        );
    }

    #[test]
    fn barcode_data_that_would_break_a_command_is_refused() {
        let (tpl, mut label) = milk_label(LabelLang::Fr);
        label.barcode = Some(LabelBarcode {
            symbology: LabelSymbology::Code128,
            data: "REF\"12".into(),
        });
        let error = job(
            &group((tpl, label)),
            &LabelProfile::preset(LabelPreset::Xprinter),
            &mut Typesetter::new(),
        )
        .unwrap_err();
        assert_eq!(error.code(), "barcode_invalid");
    }

    #[test]
    fn a_300_dpi_head_gets_twelve_dots_a_millimetre() {
        let profile = LabelProfile {
            dpi: LabelDpi::D300,
            ..LabelProfile::preset(LabelPreset::Tsc)
        };
        let text = dump(&job(&group(oil_shelf_label()), &profile, &mut Typesetter::new()).unwrap());
        assert!(
            text.contains("SIZE 58 mm,40 mm\r\n") && text.contains("BITMAP 0,0,87,480,0,"),
            "{text}"
        );
    }
}

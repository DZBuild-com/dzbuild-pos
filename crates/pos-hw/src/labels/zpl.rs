//! ZPL II for Zebra and Godex (ZPL emulation) printers: the texts as `^GFA` hex, barcodes as
//! `^BE`/`^B8`/`^BU`/`^BC` (automatic mode) / `^BQ`, `^PQ<rows>`.

use super::layout::{BarcodeBox, dropped_from, place};
use super::model::{LabelGroup, LabelSymbology};
use super::profile::{LabelMedia, LabelProfile};
use super::{body, column_x, printer_safe, row_bitmap};
use crate::error::HwError;
use crate::raster::Typesetter;
use pos_core::labels::LabelWarning;
use std::fmt::Write as _;

pub(crate) fn barcode(b: &BarcodeBox, x: u32) -> Result<String, HwError> {
    let data = printer_safe(&b.data)?;
    crate::barcode::check(b.symbology, data)?;
    let (y, h, m) = (b.y, b.height, b.module);
    Ok(match b.symbology {
        LabelSymbology::Ean13 => {
            format!("^FO{x},{y}^BY{m}^BEN,{h},Y,N^FD{}^FS", body(data, 12)?)
        }
        LabelSymbology::Ean8 => format!("^FO{x},{y}^BY{m}^B8N,{h},Y,N^FD{}^FS", body(data, 7)?),
        LabelSymbology::UpcA => {
            format!("^FO{x},{y}^BY{m}^BUN,{h},Y,N,Y^FD{}^FS", body(data, 11)?)
        }
        LabelSymbology::Code128 => format!("^FO{x},{y}^BY{m}^BCN,{h},Y,N,N,A^FD{data}^FS"),
        LabelSymbology::Qr => format!("^FO{x},{y}^BQN,2,{m}^FDMA,{data}^FS"),
        // standard Code 39 without check character, wide bars 2.5 times the narrow ones
        LabelSymbology::Code39 => format!("^FO{x},{y}^BY{m},2.5^B3N,N,{h},Y,N^FD{data}^FS"),
        // Interleaved 2 of 5 at ^BY's 3:1, as the preview draws it, without check digit
        LabelSymbology::Itf => format!("^FO{x},{y}^BY{m}^B2N,{h},Y,N,N^FD{data}^FS"),
    })
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
    let tpl = &group.template;
    let d = profile.dpi.dots_per_mm();
    let media = match profile.media {
        LabelMedia::Gap => 'Y',
        LabelMedia::BlackMark => 'M',
        LabelMedia::Continuous => 'N',
    };
    let mut out = format!("~SD{:02}\r\n", u32::from(profile.darkness) * 2);
    for label in &group.labels {
        let placed = place(tpl, label, d, ts)?;
        warnings.extend(dropped_from(tpl, label, &placed));
        let commands = match &placed.barcode {
            Some(b) => (0..u32::from(tpl.columns))
                .map(|c| barcode(b, b.x + column_x(tpl, c, d)))
                .collect::<Result<Vec<_>, _>>()?,
            None => Vec::new(),
        };
        let row = row_bitmap(&placed, tpl, d);
        let total = row.row_bytes() * row.height() as usize;
        let _ = write!(
            out,
            "^XA\r\n^CI28\r\n^PW{}\r\n^LL{}\r\n^LH{},{}\r\n^MN{media}\r\n^PR{}\r\n^FO0,0^GFA,{total},{total},{},",
            row.width(),
            row.height(),
            u32::from(profile.offset_x_dmm) * d / 10,
            u32::from(profile.offset_y_dmm) * d / 10,
            profile.speed,
            row.row_bytes(),
        );
        for byte in row.rows(0, row.height()) {
            let _ = write!(out, "{byte:02X}");
        }
        out.push_str("^FS\r\n");
        for command in commands {
            out.push_str(&command);
            out.push_str("\r\n");
        }
        let rows = u32::from(label.copies.max(1)).div_ceil(u32::from(tpl.columns.max(1)));
        let _ = write!(out, "^PQ{rows}\r\n^XZ\r\n");
    }
    Ok(out.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::labels::dump::dump;
    use crate::labels::model::{LabelBarcode, LabelGroup, LabelLang, LabelSymbology};
    use crate::labels::profile::{LabelPreset, LabelProfile};
    use crate::testdata::{milk_label, oil_shelf_label};

    #[test]
    fn zpl_58x40_ar_fr_promo() {
        let (template, label) = oil_shelf_label();
        let group = LabelGroup {
            template,
            labels: vec![label],
        };
        let bytes = job(
            &group,
            &LabelProfile::preset(LabelPreset::Zebra),
            &mut Typesetter::new(),
        )
        .unwrap();
        assert!(bytes.is_ascii(), "ZPL is plain text, the bitmap in hex");
        let text = dump(&bytes);
        for part in [
            "~SD16",
            "^XA",
            "^CI28",
            "^PW464",
            "^LL320",
            "^MNY",
            "^GFA,18560,18560,58,",
            "^BEN,",
            "^FD613012345678^FS",
            "^PQ1",
            "^XZ",
        ] {
            assert!(text.contains(part), "{part} in\n{text}");
        }
        insta::assert_snapshot!("zpl_58x40_ar_fr_promo", text);
    }

    #[test]
    fn code128_and_qr_use_their_zpl_commands_and_carets_are_refused() {
        let (tpl, mut label) = milk_label(LabelLang::Fr);
        label.barcode = Some(LabelBarcode {
            symbology: LabelSymbology::Code128,
            data: "P000123".into(),
        });
        let group = LabelGroup {
            template: tpl.clone(),
            labels: vec![label.clone()],
        };
        let text = dump(
            &job(
                &group,
                &LabelProfile::preset(LabelPreset::Zebra),
                &mut Typesetter::new(),
            )
            .unwrap(),
        );
        assert!(
            text.contains("^BCN,") && text.contains(",Y,N,N,A^FDP000123^FS"),
            "{text}"
        );
        label.barcode = Some(LabelBarcode {
            symbology: LabelSymbology::Code128,
            data: "A^B".into(),
        });
        let group = LabelGroup {
            template: tpl,
            labels: vec![label],
        };
        assert_eq!(
            job(
                &group,
                &LabelProfile::preset(LabelPreset::Zebra),
                &mut Typesetter::new()
            )
            .unwrap_err()
            .code(),
            "barcode_invalid"
        );
    }
}

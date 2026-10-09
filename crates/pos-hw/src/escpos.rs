//! ESC/POS command bytes for Epson-compatible receipt printers (Xprinter, Rongta, HPRT, Sunmi,
//! Epson TM): spec §9.1, HW-005 (raster bands), HW-006 (Chinese mode), HW-007 (barcode, cut),
//! HW-040 (drawer). Own builder rather than the `escpos` crate: exact bytes for the snapshots,
//! and the drivers (winspool RAW first) are ours anyway.

use crate::bitmap::Bitmap;
use crate::error::HwError;
use crate::profile::{BAND_ROWS, DrawerPin};

pub const ESC: u8 = 0x1B;
pub const GS: u8 = 0x1D;
pub const FS: u8 = 0x1C;
pub const LF: u8 = 0x0A;

/// The widest `GS v 0` row: 128 bytes, 1 024 dots.
pub const RASTER_MAX_ROW_BYTES: usize = 128;

/// Kitchen printers' buzzer: `ESC B n t` — `n` beeps of `t` × 100 ms (Xprinter, Rongta and
/// Epson-compatible kitchen models; research 05 §2.4). Printers without a buzzer ignore it.
pub const fn buzzer(times: u8, tenths: u8) -> [u8; 4] {
    [ESC, b'B', times, tenths]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Justify {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cut {
    Full,
    Partial,
}

/// One print job's bytes, built command by command.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Builder {
    bytes: Vec<u8>,
}

fn le(value: usize) -> (u8, u8) {
    let [lo, hi] = u16::try_from(value).unwrap_or(u16::MAX).to_le_bytes();
    (lo, hi)
}

impl Builder {
    pub fn new() -> Builder {
        Builder::default()
    }

    /// `ESC @` (reset), then `FS .` when the firmware boots in Chinese double-byte mode:
    /// `ESC @` restores the power-on modes, Chinese mode included, so leaving it must come after
    /// the reset and before any text (HW-006).
    pub fn init(&mut self, exit_chinese_mode: bool) -> &mut Self {
        self.bytes.extend([ESC, b'@']);
        if exit_chinese_mode {
            self.bytes.extend([FS, b'.']);
        }
        self
    }

    /// `ESC t n`.
    pub fn code_page(&mut self, selector: u8) -> &mut Self {
        self.bytes.extend([ESC, b't', selector]);
        self
    }

    /// `ESC a n`.
    pub fn justify(&mut self, justify: Justify) -> &mut Self {
        let n = match justify {
            Justify::Left => 0,
            Justify::Center => 1,
            Justify::Right => 2,
        };
        self.bytes.extend([ESC, b'a', n]);
        self
    }

    /// `ESC E n`.
    pub fn bold(&mut self, on: bool) -> &mut Self {
        self.bytes.extend([ESC, b'E', u8::from(on)]);
        self
    }

    /// `GS ! n`: width and height multipliers, 1 to 8 each.
    pub fn size(&mut self, width: u8, height: u8) -> &mut Self {
        let w = width.clamp(1, 8) - 1;
        let h = height.clamp(1, 8) - 1;
        self.bytes.extend([GS, b'!', (w << 4) | h]);
        self
    }

    /// Bytes already encoded in the printer's code page.
    pub fn text(&mut self, encoded: &[u8]) -> &mut Self {
        self.bytes.extend_from_slice(encoded);
        self
    }

    /// Encoded bytes and a line feed.
    pub fn line(&mut self, encoded: &[u8]) -> &mut Self {
        self.bytes.extend_from_slice(encoded);
        self.bytes.push(LF);
        self
    }

    /// `ESC d n`.
    pub fn feed(&mut self, lines: u8) -> &mut Self {
        if lines > 0 {
            self.bytes.extend([ESC, b'd', lines]);
        }
        self
    }

    /// `GS V 65|66 n`: feed `feed` dot rows, then a full or partial cut.
    pub fn cut(&mut self, cut: Cut, feed: u8) -> &mut Self {
        let m = match cut {
            Cut::Full => 65,
            Cut::Partial => 66,
        };
        self.bytes.extend([GS, b'V', m, feed]);
        self
    }

    /// `ESC p m 25 250`: a 50 ms pulse on the drawer connector (spec §9.1).
    pub fn drawer_kick(&mut self, pin: DrawerPin) -> &mut Self {
        let m = match pin {
            DrawerPin::Pin2 => 0,
            DrawerPin::Pin5 => 1,
        };
        self.bytes.extend([ESC, b'p', m, 25, 250]);
        self
    }

    /// A Code 128 barcode (`GS k 73`) with its text printed below, bars as wide as fit in
    /// `dots` (DOC-016: the ticket number, scanned back for returns).
    pub fn code128(&mut self, data: &str, dots: u16, height: u8) -> Result<&mut Self, HwError> {
        let payload = code128_payload(data)?;
        let width = code128_module_width(&payload, dots);
        let n = u8::try_from(payload.len()).map_err(|_| HwError::invalid("barcode"))?;
        self.bytes.extend([
            GS,
            b'h',
            height.max(1),
            GS,
            b'w',
            width,
            GS,
            b'H',
            2,
            GS,
            b'f',
            0,
            GS,
            b'k',
            73,
            n,
        ]);
        self.bytes.extend_from_slice(&payload);
        Ok(self)
    }

    /// `GS v 0` in bands of `band_rows` rows (HW-005): printers with little memory need small
    /// bands. The command carries 1 to [`RASTER_MAX_ROW_BYTES`] bytes a row, and bands stay in
    /// [`BAND_ROWS`]: a wider image (a logo that skipped the scaling) or an odd band is refused
    /// rather than sent as bytes the printer would misread.
    pub fn raster(&mut self, bitmap: &Bitmap, band_rows: u16) -> Result<&mut Self, HwError> {
        if !BAND_ROWS.contains(&band_rows) {
            return Err(HwError::invalid("profile.bandRows"));
        }
        if bitmap.height() == 0 {
            return Ok(self);
        }
        if !(1..=RASTER_MAX_ROW_BYTES).contains(&bitmap.row_bytes()) {
            return Err(HwError::Render(format!(
                "a {}-dot image row does not fit GS v 0",
                bitmap.width()
            )));
        }
        let (xl, xh) = le(bitmap.row_bytes());
        let band = u32::from(band_rows);
        let mut top = 0;
        while top < bitmap.height() {
            let rows = band.min(bitmap.height() - top);
            let (yl, yh) = le(rows as usize);
            self.bytes.extend([GS, b'v', b'0', 0, xl, xh, yl, yh]);
            self.bytes.extend_from_slice(bitmap.rows(top, rows));
            top += rows;
        }
        Ok(self)
    }

    pub fn raw(&mut self, bytes: &[u8]) -> &mut Self {
        self.bytes.extend_from_slice(bytes);
        self
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// The `GS k 73` data: code set B, switching to set C (two digits per symbol) for runs of
/// four digits or more, so a 16-character ticket number fits a 58 mm head at 2 dots a module.
pub fn code128_payload(data: &str) -> Result<Vec<u8>, HwError> {
    let chars = data.as_bytes();
    if chars.is_empty() || chars.len() > 64 || chars.iter().any(|b| !(32..=126).contains(b)) {
        return Err(HwError::invalid("barcode"));
    }
    let digits_at = |i: usize| {
        chars
            .iter()
            .skip(i)
            .take_while(|b| b.is_ascii_digit())
            .count()
    };
    let mut out = Vec::with_capacity(chars.len() + 8);
    let mut set_c = false;
    let mut i = 0;
    if digits_at(0) >= 4 {
        out.extend(b"{C");
        set_c = true;
    } else {
        out.extend(b"{B");
    }
    while i < chars.len() {
        let run = digits_at(i);
        if run >= 4 {
            let even = run - run % 2;
            if !set_c {
                out.extend(b"{C");
                set_c = true;
            }
            for pair in chars[i..i + even].chunks(2) {
                out.push((pair[0] - b'0') * 10 + (pair[1] - b'0'));
            }
            i += even;
        } else {
            if set_c {
                out.extend(b"{B");
                set_c = false;
            }
            if chars[i] == b'{' {
                out.extend(b"{{");
            } else {
                out.push(chars[i]);
            }
            i += 1;
        }
    }
    Ok(out)
}

/// Modules of the symbol, quiet zones excluded: 11 per symbol (start, data, set switches,
/// check) and 13 for the stop.
pub fn code128_modules(payload: &[u8]) -> usize {
    let mut symbols = 1; // check symbol
    let mut i = 0;
    while i < payload.len() {
        // "{X" (start, switch, or an escaped brace) is one symbol
        i += if payload[i] == b'{' { 2 } else { 1 };
        symbols += 1;
    }
    symbols * 11 + 13
}

/// 3 dots a module when it fits, else 2, else 1.
pub fn code128_module_width(payload: &[u8], dots: u16) -> u8 {
    let modules = code128_modules(payload);
    (1..=3_u8)
        .rev()
        .find(|w| modules * usize::from(*w) <= usize::from(dots))
        .unwrap_or(1)
}

/// Bytes as readable text for snapshots: printable ASCII as is, other bytes as `<1B 40>` runs,
/// a line break after each LF.
pub fn dump(bytes: &[u8]) -> String {
    fn flush(out: &mut String, hex: &mut Vec<String>) {
        if !hex.is_empty() {
            out.push('<');
            out.push_str(&hex.join(" "));
            out.push('>');
            hex.clear();
        }
    }
    let mut out = String::new();
    let mut hex: Vec<String> = Vec::new();
    for &b in bytes {
        if (0x20..=0x7E).contains(&b) && b != b'<' && b != b'>' {
            flush(&mut out, &mut hex);
            out.push(char::from(b));
        } else {
            hex.push(format!("{b:02X}"));
            if b == LF {
                flush(&mut out, &mut hex);
                out.push('\n');
            }
        }
    }
    flush(&mut out, &mut hex);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{DrawerPin, PaperWidth, Preset, Profile};

    #[test]
    fn styled_line_bytes() {
        let mut b = Builder::new();
        b.init(true)
            .code_page(19)
            .justify(Justify::Center)
            .bold(true)
            .size(2, 2)
            .line(b"TOTAL")
            .size(1, 1)
            .bold(false)
            .feed(3)
            .cut(Cut::Partial, 0);
        assert_eq!(
            b.bytes(),
            [
                ESC, b'@', FS, b'.', ESC, b't', 19, ESC, b'a', 1, ESC, b'E', 1, GS, b'!', 0x11,
                b'T', b'O', b'T', b'A', b'L', LF, GS, b'!', 0, ESC, b'E', 0, ESC, b'd', 3, GS,
                b'V', 66, 0,
            ]
        );
        assert_eq!(
            dump(b.bytes()),
            "<1B>@<1C>.<1B>t<13 1B>a<01 1B>E<01 1D>!<11>TOTAL<0A>\n<1D>!<00 1B>E<00 1B>d<03 1D>VB<00>"
        );
    }

    /// `ESC @` puts a printer that boots in Chinese mode straight back into it: `FS .` must come
    /// after the last reset and before the code page and the first text.
    #[test]
    fn chinese_mode_is_left_after_the_reset() {
        let mut b = Builder::new();
        b.init(true).code_page(19).line(b"A");
        let bytes = b.bytes();
        let at = |needle: &[u8]| bytes.windows(needle.len()).position(|w| w == needle);
        let last_reset = bytes.windows(2).rposition(|w| w == [ESC, b'@']).unwrap();
        let leave = at(&[FS, b'.']).unwrap();
        let code_page = at(&[ESC, b't']).unwrap();
        assert!(last_reset < leave && leave < code_page, "{bytes:?}");
        let mut plain = Builder::new();
        plain.init(false);
        assert_eq!(plain.bytes(), [ESC, b'@']);
    }

    #[test]
    fn drawer_kick_on_both_pins() {
        let mut b = Builder::new();
        b.drawer_kick(DrawerPin::Pin2).drawer_kick(DrawerPin::Pin5);
        assert_eq!(b.bytes(), [ESC, b'p', 0, 25, 250, ESC, b'p', 1, 25, 250]);
    }

    #[test]
    fn code128_uses_set_c_for_digit_runs_and_fits_58mm() {
        let payload = code128_payload("TK2026-C1-000123").unwrap();
        assert_eq!(payload, b"{BTK{C\x14\x1A{B-C1-{C\x00\x01\x17");
        // start + 2 + switch + 2 + switch + 4 + switch + 3 + check = 16 symbols of 11 modules, stop 13
        assert_eq!(code128_modules(&payload), 189);
        assert_eq!(
            code128_module_width(&payload, 384),
            2,
            "378 dots fit a 58 mm head"
        );
        assert_eq!(code128_module_width(&payload, 576), 3);
        assert_eq!(
            code128_payload("A{B").unwrap(),
            b"{BA{{B",
            "a literal brace is doubled"
        );
        assert!(code128_payload("").is_err());
        assert!(code128_payload("été").is_err(), "Code 128 B is ASCII only");
    }

    #[test]
    fn ticket_number_fits_every_58mm_profile_at_two_dots_a_module() {
        let payload = code128_payload("TK2026-C1-000123").unwrap();
        for preset in [
            Preset::Generic,
            Preset::Xprinter,
            Preset::Epson,
            Preset::Rongta,
        ] {
            let dots = Profile::preset(preset, PaperWidth::Mm58).dots;
            let width = code128_module_width(&payload, dots);
            assert_eq!(width, 2, "{preset:?}");
            assert!(
                code128_modules(&payload) * usize::from(width) <= usize::from(dots),
                "{preset:?}: the symbol is wider than the head"
            );
        }
    }

    #[test]
    fn code128_command_carries_height_width_and_hri() {
        let mut b = Builder::new();
        b.code128("1234", 576, 60).unwrap();
        assert_eq!(
            b.bytes(),
            [
                GS, b'h', 60, GS, b'w', 3, GS, b'H', 2, GS, b'f', 0, GS, b'k', 73, 4, b'{', b'C',
                12, 34
            ]
        );
    }

    #[test]
    fn raster_is_sent_in_bands() {
        let mut bitmap = Bitmap::new(16, 20);
        bitmap.set(0, 0, true);
        bitmap.set(15, 19, true);
        let mut b = Builder::new();
        b.raster(&bitmap, 8).unwrap();
        let bytes = b.into_bytes();
        // three bands: 8 + 8 + 4 rows of 2 bytes, each with an 8-byte header
        assert_eq!(bytes.len(), 3 * 8 + 20 * 2);
        assert_eq!(&bytes[..8], [GS, b'v', b'0', 0, 2, 0, 8, 0]);
        assert_eq!(bytes[8], 0b1000_0000);
        assert_eq!(&bytes[24..32], [GS, b'v', b'0', 0, 2, 0, 8, 0]);
        assert_eq!(&bytes[48..56], [GS, b'v', b'0', 0, 2, 0, 4, 0]);
        assert_eq!(bytes[bytes.len() - 1], 0b0000_0001);
    }

    /// `GS v 0` takes 1 to 128 bytes a row; bands stay within what a profile allows.
    #[test]
    fn raster_refuses_what_gs_v_0_cannot_carry() {
        let mut b = Builder::new();
        b.raster(&Bitmap::new(1024, 2), 128).unwrap();
        assert_eq!(&b.bytes()[..8], [GS, b'v', b'0', 0, 128, 0, 2, 0]);
        let err = Builder::new()
            .raster(&Bitmap::new(1032, 2), 128)
            .unwrap_err();
        assert_eq!(err.code(), "render_failed", "129 bytes a row");
        assert_eq!(
            Builder::new()
                .raster(&Bitmap::new(0, 2), 128)
                .unwrap_err()
                .code(),
            "render_failed",
            "no bytes a row"
        );
        for rows in [0, 7, 1025] {
            let err = Builder::new()
                .raster(&Bitmap::new(16, 2), rows)
                .unwrap_err();
            assert_eq!(err.field(), Some("profile.bandRows"), "{rows} rows a band");
        }
        let mut empty = Builder::new();
        empty.raster(&Bitmap::new(0, 0), 128).unwrap();
        assert!(empty.bytes().is_empty(), "nothing to print, no command");
    }
}

//! Arabic and bilingual receipts as images (spec §9.1, DOC-013): text shaped by `cosmic-text`
//! (HarfRust shaping, Unicode bidi) with the bundled Cairo font only, drawn as ink coverage,
//! then cut to 1 bit. Printer code pages never see Arabic.

use crate::bitmap::Bitmap;
use crate::error::HwError;
use cosmic_text::{
    Align as TextAlign, Attrs, Buffer, Color, Family, FontSystem, LineIter, Metrics, Shaping,
    SwashCache, Weight, Wrap, fontdb,
};
use serde::Serialize;

/// Cairo (SIL Open Font License 1.1, `fonts/OFL.txt`), Arabic and Latin in one family.
pub const CAIRO: &[u8] = include_bytes!("../fonts/Cairo-Variable.ttf");
pub const FAMILY: &str = "Cairo";

/// Coverage at or above this prints as a black dot (half ink: crisp text, no dithering).
pub const INK_THRESHOLD: u8 = 128;

/// Line height as a multiple of the font size (Cairo has tall Arabic ascenders).
const LINE_HEIGHT: f32 = 1.35;

/// Start and end follow the document direction given to [`Typesetter::draw`] (`rtl`): start is
/// the right side in Arabic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    Start,
    Center,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    pub size_px: f32,
    pub bold: bool,
}

/// Ink coverage per dot (0 paper … 255 full ink); grows downwards as rows are drawn.
#[derive(Debug, Clone)]
pub struct Canvas {
    width: u32,
    height: u32,
    ink: Vec<u8>,
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Canvas {
        Canvas {
            width,
            height,
            ink: vec![0; width as usize * height as usize],
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn grow_to(&mut self, height: u32) {
        if height > self.height {
            self.height = height;
            self.ink.resize(self.width as usize * height as usize, 0);
        }
    }

    /// Darkest wins: overlapping glyph edges never lighten a dot.
    pub fn blend(&mut self, x: i32, y: i32, coverage: u8) {
        let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
            return;
        };
        if x >= self.width || y >= self.height {
            return;
        }
        if let Some(dot) = self
            .ink
            .get_mut(y as usize * self.width as usize + x as usize)
        {
            *dot = (*dot).max(coverage);
        }
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: u32, h: u32) {
        for dy in 0..h as i32 {
            for dx in 0..w as i32 {
                self.blend(x + dx, y + dy, 255);
            }
        }
    }

    pub fn paste(&mut self, bitmap: &Bitmap, x: u32, y: u32) {
        self.grow_to(y + bitmap.height());
        for by in 0..bitmap.height() {
            for bx in 0..bitmap.width() {
                if bitmap.get(bx, by) {
                    self.blend((x + bx) as i32, (y + by) as i32, 255);
                }
            }
        }
    }

    pub fn to_bitmap(&self, threshold: u8) -> Bitmap {
        let mut bitmap = Bitmap::new(self.width, self.height);
        for (i, dot) in self.ink.iter().enumerate() {
            if *dot >= threshold {
                let x = (i % self.width as usize) as u32;
                let y = (i / self.width as usize) as u32;
                bitmap.set(x, y, true);
            }
        }
        bitmap
    }
}

/// Shapes and draws text. Built once and kept by the registry (loading the font takes a few ms).
pub struct Typesetter {
    fonts: FontSystem,
    cache: SwashCache,
}

impl Default for Typesetter {
    fn default() -> Self {
        Typesetter::new()
    }
}

fn text_align(align: Align, rtl: bool) -> TextAlign {
    match (align, rtl) {
        (Align::Center, _) => TextAlign::Center,
        (Align::Start, false) | (Align::End, true) => TextAlign::Left,
        (Align::Start, true) | (Align::End, false) => TextAlign::Right,
    }
}

/// Letters of the right-to-left scripts (Hebrew, Arabic, Syriac, Thaana, N'Ko…, their
/// supplements and presentation forms). Arabic-Indic digits are not letters.
fn is_rtl_letter(c: char) -> bool {
    c.is_alphabetic()
        && matches!(c,
            '\u{0590}'..='\u{08FF}'
            | '\u{FB1D}'..='\u{FDFF}'
            | '\u{FE70}'..='\u{FEFF}'
            | '\u{10800}'..='\u{10FFF}'
            | '\u{1E800}'..='\u{1EFFF}')
}

/// Sets the bidi direction of every paragraph that mixes right-to-left and left-to-right
/// letters to the document's (`rtl`), with an invisible zero-width mark (RLM or LRM) at its
/// start. Without it, cosmic-text takes the direction from the first letter, so on an Arabic
/// receipt "Candia حليب 1L" would read left to right with "1L" split around the Arabic word.
/// Paragraphs in one script and figures keep their own order: forced into the other direction,
/// the bidi algorithm would print "500,00 4" for "4 500,00" and "kg × 450,00 1,25" for
/// "1,25 kg × 450,00" on an Arabic receipt.
/// Paragraph separators inside a line (U+001C–U+001E, U+0085, U+2029) become line feeds, so
/// each paragraph is its own cosmic-text line: one line with two directions panics in its
/// shaper.
fn directed(text: &str, rtl: bool) -> String {
    let mark = if rtl { '\u{200F}' } else { '\u{200E}' };
    let text: String = text
        .chars()
        .map(|c| match c {
            '\u{1C}'..='\u{1E}' | '\u{85}' | '\u{2029}' => '\n',
            c => c,
        })
        .collect();
    let mut out = String::with_capacity(text.len() + 3);
    for (range, ending) in LineIter::new(&text) {
        let line = &text[range];
        let right_to_left = line.chars().any(is_rtl_letter);
        let left_to_right = line.chars().any(|c| c.is_alphabetic() && !is_rtl_letter(c));
        if right_to_left && left_to_right {
            out.push(mark);
        }
        out.push_str(line);
        out.push_str(ending.as_str());
    }
    out
}

/// Characters that end a line or a paragraph: an isolate never spans one.
fn is_break(c: char) -> bool {
    matches!(
        c,
        '\n' | '\r' | '\u{0B}' | '\u{0C}' | '\u{85}' | '\u{2028}' | '\u{2029}'
    ) || ('\u{1C}'..='\u{1E}').contains(&c)
}

/// A whitespace-separated word, for [`isolated`].
#[derive(Clone, Copy, PartialEq, Eq)]
enum Word {
    /// Has a right-to-left letter: ends a left-to-right run.
    RightToLeft,
    /// A figure, a code or a Latin word: opens or extends a run.
    LeftToRight,
    /// Punctuation or symbols alone: inside a run only between two left-to-right words.
    Neutral,
}

fn word_kind(word: &str) -> Word {
    if word.chars().any(is_rtl_letter) {
        Word::RightToLeft
    } else if word.chars().any(char::is_alphanumeric) {
        Word::LeftToRight
    } else {
        Word::Neutral
    }
}

/// Wraps the left-to-right runs (figures, codes, Latin words) of every line that has
/// right-to-left letters in an LRI…PDI isolate, so the bidi algorithm moves each run as one
/// block. Without it, a run is split at its spaces and hyphens around an Arabic word:
/// "1 078,50 دج" printed "دج 078,50 1", a phone number "56 34 12 0550", an RC
/// "B 22 1234567-16/00" — and the reordered amount no longer fitted the width `measure` gave
/// it. A run goes from a left-to-right word to the last one before the next right-to-left
/// word, so punctuation alone at either edge (":", "/") stays outside. Lines without
/// right-to-left letters are left as they are.
fn isolated(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 12);
    for line in text.split_inclusive(is_break) {
        if !line.chars().any(is_rtl_letter) {
            out.push_str(line);
            continue;
        }
        let mut runs: Vec<(usize, usize)> = Vec::new();
        let mut open: Option<(usize, usize)> = None;
        let mut word_start = None;
        // a sentinel space closes the last word
        let chars = line
            .char_indices()
            .chain(std::iter::once((line.len(), ' ')));
        for (i, c) in chars {
            match (c.is_whitespace(), word_start) {
                (false, None) => word_start = Some(i),
                (true, Some(s)) => {
                    word_start = None;
                    match word_kind(&line[s..i]) {
                        Word::RightToLeft => runs.extend(open.take()),
                        Word::LeftToRight => {
                            open = Some(open.map_or((s, i), |(from, _)| (from, i)));
                        }
                        Word::Neutral => {}
                    }
                }
                _ => {}
            }
        }
        runs.extend(open);
        let mut at = 0;
        for (from, to) in runs {
            out.push_str(&line[at..from]);
            out.push('\u{2066}');
            out.push_str(&line[from..to]);
            out.push('\u{2069}');
            at = to;
        }
        out.push_str(&line[at..]);
    }
    out
}

impl Typesetter {
    /// Only the bundled Cairo: the same dots on every PC, whatever fonts Windows has.
    pub fn new() -> Typesetter {
        let mut db = fontdb::Database::new();
        db.load_font_data(CAIRO.to_vec());
        db.set_sans_serif_family(FAMILY);
        Typesetter {
            fonts: FontSystem::new_with_locale_and_db("ar-DZ".to_owned(), db),
            cache: SwashCache::new(),
        }
    }

    pub fn faces(&self) -> usize {
        self.fonts.db().faces().count()
    }

    /// The first family name of every loaded face (only "Cairo").
    pub fn families(&self) -> Vec<String> {
        self.fonts
            .db()
            .faces()
            .filter_map(|face| face.families.first().map(|(name, _)| name.clone()))
            .collect()
    }

    fn buffer(
        &mut self,
        text: &str,
        style: TextStyle,
        width: Option<f32>,
        align: Option<TextAlign>,
    ) -> Buffer {
        let metrics = Metrics::new(style.size_px, (style.size_px * LINE_HEIGHT).round());
        let mut buffer = Buffer::new(&mut self.fonts, metrics);
        let weight = if style.bold {
            Weight::BOLD
        } else {
            Weight::NORMAL
        };
        let attrs = Attrs::new().family(Family::Name(FAMILY)).weight(weight);
        {
            let mut b = buffer.borrow_with(&mut self.fonts);
            b.set_wrap(if width.is_some() {
                Wrap::Word
            } else {
                Wrap::None
            });
            b.set_size(width, None);
            b.set_text(text, &attrs, Shaping::Advanced, align);
            b.shape_until_scroll(true);
        }
        buffer
    }

    /// The width in dots of `text` on one line (of its widest paragraph). The direction does not
    /// change the width; [`directed`] still splits the paragraphs and [`isolated`] keeps the
    /// figures whole, as in [`Typesetter::draw`].
    pub fn measure(&mut self, text: &str, style: TextStyle) -> u32 {
        let buffer = self.buffer(&directed(&isolated(text), false), style, None, None);
        let width = buffer
            .layout_runs()
            .map(|run| run.line_w)
            .fold(0.0_f32, f32::max);
        width.ceil().max(0.0) as u32
    }

    /// Draws `text` wrapped to `width` dots, its box's left edge at `x` and top at `top`;
    /// returns the height used and grows the canvas to it. `rtl` is the document direction: it
    /// places [`Align`] start on the right and sets the reading order of every paragraph that
    /// mixes Arabic and Latin letters (see [`directed`]); a paragraph in one script keeps its
    /// own order, and figures next to Arabic words print whole (see [`isolated`]).
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        canvas: &mut Canvas,
        text: &str,
        style: TextStyle,
        x: i32,
        top: i32,
        width: u32,
        align: Align,
        rtl: bool,
    ) -> u32 {
        if text.trim().is_empty() {
            return 0;
        }
        let mut buffer = self.buffer(
            &directed(&isolated(text), rtl),
            style,
            Some(width as f32),
            Some(text_align(align, rtl)),
        );
        let bottom = buffer
            .layout_runs()
            .map(|run| run.line_top + run.line_height)
            .fold(0.0_f32, f32::max);
        let height = bottom.ceil().max(0.0) as u32;
        canvas.grow_to(top.max(0) as u32 + height);
        buffer.draw(
            &mut self.fonts,
            &mut self.cache,
            Color::rgb(0, 0, 0),
            |gx, gy, w, h, color| {
                let coverage = color.a();
                if coverage == 0 {
                    return;
                }
                for dy in 0..h as i32 {
                    for dx in 0..w as i32 {
                        canvas.blend(x + gx + dx, top + gy + dy, coverage);
                    }
                }
            },
        );
        height
    }
}

/// The shop logo for the receipt header: PNG or JPEG, transparent parts white, scaled to fit
/// `max_width` × `max_height` (ratio kept, never enlarged), Floyd–Steinberg dithered.
pub fn logo_bitmap(bytes: &[u8], max_width: u32, max_height: u32) -> Result<Bitmap, HwError> {
    let image = image::load_from_memory(bytes).map_err(|e| HwError::Logo(e.to_string()))?;
    let rgba = image.to_luma_alpha8();
    let (w, h) = rgba.dimensions();
    if w == 0 || h == 0 {
        return Err(HwError::Logo("empty image".into()));
    }
    let scale = f64::min(
        1.0,
        f64::min(
            f64::from(max_width) / f64::from(w),
            f64::from(max_height) / f64::from(h),
        ),
    );
    let tw = ((f64::from(w) * scale).round() as u32).max(1);
    let th = ((f64::from(h) * scale).round() as u32).max(1);
    let flat = image::GrayImage::from_fn(w, h, |x, y| {
        let p = rgba.get_pixel(x, y);
        let (luma, alpha) = (u32::from(p[0]), u32::from(p[1]));
        image::Luma([((luma * alpha + 255 * (255 - alpha)) / 255) as u8])
    });
    let scaled = image::imageops::resize(&flat, tw, th, image::imageops::FilterType::Triangle);
    let mut error: Vec<i32> = scaled.pixels().map(|p| i32::from(p[0])).collect();
    let mut bitmap = Bitmap::new(tw, th);
    let (tw_i, th_i) = (tw as usize, th as usize);
    for y in 0..th_i {
        for x in 0..tw_i {
            let i = y * tw_i + x;
            let old = error[i];
            let black = old < 128;
            bitmap.set(x as u32, y as u32, black);
            let diff = old - if black { 0 } else { 255 };
            let mut spread = |dx: isize, dy: usize, weight: i32| {
                let nx = x as isize + dx;
                if nx >= 0 && (nx as usize) < tw_i && y + dy < th_i {
                    error[(y + dy) * tw_i + nx as usize] += diff * weight / 16;
                }
            };
            spread(1, 0, 7);
            spread(-1, 1, 3);
            spread(0, 1, 5);
            spread(1, 1, 1);
        }
    }
    Ok(bitmap)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::golden::assert_golden;

    const NORMAL: TextStyle = TextStyle {
        size_px: 26.0,
        bold: false,
    };

    fn ink_in_columns(bitmap: &Bitmap, from: u32, to: u32) -> usize {
        (0..bitmap.height())
            .map(|y| (from..to).filter(|x| bitmap.get(*x, y)).count())
            .sum()
    }

    /// First and last index where `has_ink` holds.
    fn span(range: std::ops::Range<u32>, has_ink: impl Fn(u32) -> bool) -> Option<(u32, u32)> {
        let mut hits = range.filter(|i| has_ink(*i));
        let first = hits.next()?;
        Some((first, hits.last().unwrap_or(first)))
    }

    fn ink_columns(bitmap: &Bitmap) -> (u32, u32) {
        span(0..bitmap.width(), |x| {
            (0..bitmap.height()).any(|y| bitmap.get(x, y))
        })
        .expect("some ink")
    }

    fn ink_rows(bitmap: &Bitmap) -> (u32, u32) {
        span(0..bitmap.height(), |y| {
            (0..bitmap.width()).any(|x| bitmap.get(x, y))
        })
        .expect("some ink")
    }

    /// Columns with ink strictly under `row`.
    fn ink_columns_below(bitmap: &Bitmap, row: u32) -> Option<(u32, u32)> {
        span(0..bitmap.width(), |x| {
            (row + 1..bitmap.height()).any(|y| bitmap.get(x, y))
        })
    }

    fn render(ts: &mut Typesetter, text: &str, align: Align, rtl: bool) -> Bitmap {
        let mut canvas = Canvas::new(576, 0);
        ts.draw(&mut canvas, text, NORMAL, 0, 0, 576, align, rtl);
        canvas.to_bitmap(INK_THRESHOLD)
    }

    /// "Candia" has no descender, so the ink under its baseline in `mixed` is the Arabic word's
    /// (the dots under ي and ب). Asserts the Latin word sits right of all of it.
    fn assert_latin_right_of_arabic(ts: &mut Typesetter, mixed: &Bitmap, rtl: bool) {
        let latin = render(ts, "Candia", Align::Start, rtl);
        let (latin_left, latin_right) = ink_columns(&latin);
        let (_, baseline) = ink_rows(&latin);
        let (_, line_right) = ink_columns(mixed);
        let (arabic_left, arabic_right) =
            ink_columns_below(mixed, baseline).expect("the Arabic word hangs under the baseline");
        let latin_starts = line_right - (latin_right - latin_left);
        assert!(
            arabic_right < latin_starts,
            "Arabic ink {arabic_left}..={arabic_right} must be left of the Latin word \
             (from column {latin_starts} to {line_right})"
        );
    }

    #[test]
    fn a_latin_first_line_on_an_arabic_receipt_reads_right_to_left() {
        let mut ts = Typesetter::new();
        let mixed = render(&mut ts, "Candia حليب", Align::Start, true);
        assert!(ink_columns(&mixed).1 >= 566, "the line starts on the right");
        assert_latin_right_of_arabic(&mut ts, &mixed, true);
        assert_golden(
            "ar_mixed_line_80",
            &render(&mut ts, "Candia حليب 1L", Align::Start, true),
        );
    }

    #[test]
    fn an_arabic_first_line_on_a_french_receipt_reads_left_to_right() {
        let mut ts = Typesetter::new();
        let mixed = render(&mut ts, "حليب Candia", Align::Start, false);
        assert!(ink_columns(&mixed).0 <= 10, "the line starts on the left");
        assert_latin_right_of_arabic(&mut ts, &mixed, false);
    }

    #[test]
    fn single_direction_lines_keep_their_own_order() {
        let mut ts = Typesetter::new();
        // Forcing these into the document direction would print "500,00 4" for an amount or
        // "kg × 450,00 1,25" for a weighed line on an Arabic receipt.
        for line in [
            "4 500,00",
            "1 234 567,00",
            "  2 × 120,00",
            "  1,25 kg × 450,00",
            "Supérette El Amel",
            "حليب كانديا 1 لتر",
        ] {
            assert!(
                render(&mut ts, line, Align::Center, true)
                    == render(&mut ts, line, Align::Center, false),
                "{line:?} reads the same on an Arabic and a French receipt"
            );
        }
    }

    #[test]
    fn every_mixed_paragraph_gets_the_document_direction() {
        assert_eq!(
            directed("Candia حليب\r\n4 500,00\nحليب\u{2029}حليب Candia", true),
            "\u{200F}Candia حليب\r\n4 500,00\nحليب\n\u{200F}حليب Candia"
        );
        assert_eq!(
            directed("حليب Candia\n  1,25 kg × 450,00", false),
            "\u{200E}حليب Candia\n  1,25 kg × 450,00"
        );
        assert_eq!(
            directed("١٢٣ Candia", true),
            "١٢٣ Candia",
            "digits are no letters"
        );
    }

    #[test]
    fn a_paragraph_separator_inside_a_line_never_panics() {
        let mut ts = Typesetter::new();
        let mut canvas = Canvas::new(576, 0);
        // pasted separators must not reach cosmic-text's one-direction-per-line assertion
        let text = "4 500,00\u{2029}Candia حليب\u{85}حليب Candia\r\nCandia حليب";
        let height = ts.draw(&mut canvas, text, NORMAL, 0, 0, 576, Align::Start, true);
        assert!(height >= 4 * 26, "four lines: {height}");
        assert!(ts.measure(text, NORMAL) > 0);
    }

    #[test]
    fn left_to_right_runs_of_arabic_paragraphs_are_isolated() {
        let (lri, pdi) = ('\u{2066}', '\u{2069}');
        for (text, expected) in [
            ("1 078,50 دج", format!("{lri}1 078,50{pdi} دج")),
            (
                "الهاتف 0550 12 34 56",
                format!("الهاتف {lri}0550 12 34 56{pdi}"),
            ),
            (
                "السجل التجاري 16/00-1234567 B 22",
                format!("السجل التجاري {lri}16/00-1234567 B 22{pdi}"),
            ),
            (
                "الرسم 19% (518,91)",
                format!("الرسم {lri}19% (518,91){pdi}"),
            ),
            (
                "Net à payer / الصافي للدفع",
                format!("{lri}Net à payer{pdi} / الصافي للدفع"),
            ),
            (
                "Candia حليب 1L\n4 500,00",
                format!("{lri}Candia{pdi} حليب {lri}1L{pdi}\n4 500,00"),
            ),
        ] {
            assert_eq!(isolated(text), expected, "{text:?}");
        }
        for untouched in [
            "4 500,00",
            "  1,25 kg × 450,00",
            "Supérette El Amel",
            "حليب كانديا",
            "أوقف هذا المبلغ على : ألف",
        ] {
            assert_eq!(isolated(untouched), untouched, "{untouched:?}");
        }
    }

    /// Share of the dots of `columns` where two images differ, of the dots inked in either.
    fn difference(a: &Bitmap, b: &Bitmap, columns: std::ops::RangeInclusive<u32>) -> f64 {
        let (mut differ, mut inked) = (0_u32, 0_u32);
        for y in 0..a.height().max(b.height()) {
            for x in columns.clone() {
                let (da, db) = (a.get(x, y), b.get(x, y));
                inked += u32::from(da || db);
                differ += u32::from(da != db);
            }
        }
        f64::from(differ) / f64::from(inked.max(1))
    }

    /// Without isolation the bidi algorithm splits a figure at its spaces around an Arabic word:
    /// "1 078,50 دج" printed "دج 078,50 1", a phone "56 34 12 0550", an RC "B 22 1234567-16/00".
    #[test]
    fn figures_next_to_arabic_words_print_whole() {
        let mut ts = Typesetter::new();
        for (figure, line, align) in [
            ("1 078,50", "1 078,50 دج", Align::Start),
            ("0550 12 34 56", "الهاتف 0550 12 34 56", Align::End),
            ("16/00-1234567 B 22", "السجل 16/00-1234567 B 22", Align::End),
        ] {
            let alone = render(&mut ts, figure, align, true);
            let in_line = render(&mut ts, line, align, true);
            let (from, to) = ink_columns(&alone);
            let d = difference(&alone, &in_line, from..=to);
            assert!(
                d < 0.05,
                "{line:?}: {:.0} % of the figure differs",
                d * 100.0
            );
        }
    }

    #[test]
    fn isolates_print_nothing() {
        let mut ts = Typesetter::new();
        for rtl in [true, false] {
            assert!(
                render(&mut ts, "\u{2066}Total 4 500,00\u{2069}", Align::Start, rtl)
                    == render(&mut ts, "Total 4 500,00", Align::Start, rtl),
                "rtl {rtl}"
            );
        }
    }

    #[test]
    fn an_arabic_amount_fits_what_measure_says() {
        let mut ts = Typesetter::new();
        let one_line = (NORMAL.size_px * LINE_HEIGHT).round() as u32;
        for text in ["1 078,50 دج", "4 500,00 دج", "الهاتف 0550 12 34 56"] {
            let width = ts.measure(text, NORMAL) + 2;
            let mut canvas = Canvas::new(width, 0);
            let height = ts.draw(&mut canvas, text, NORMAL, 0, 0, width, Align::End, true);
            assert_eq!(height, one_line, "{text:?} fits in {width} dots");
        }
    }

    #[test]
    fn the_direction_mark_takes_no_room() {
        let mut ts = Typesetter::new();
        let one_line = (NORMAL.size_px * LINE_HEIGHT).round() as u32;
        for text in ["Candia حليب 1L", "حليب Candia"] {
            // a receipt pair gives its figure exactly what `measure` says (plus 2 dots)
            let width = ts.measure(text, NORMAL) + 2;
            for rtl in [true, false] {
                let mut canvas = Canvas::new(width, 0);
                let height = ts.draw(&mut canvas, text, NORMAL, 0, 0, width, Align::End, rtl);
                assert_eq!(
                    height, one_line,
                    "{text:?} (rtl {rtl}) fits in {width} dots"
                );
            }
        }
    }

    #[test]
    fn only_the_bundled_font_is_loaded() {
        let ts = Typesetter::new();
        assert!(ts.faces() >= 1);
        assert!(
            ts.families().iter().all(|f| f == FAMILY),
            "no system fonts: renders are the same on every PC ({:?})",
            ts.families()
        );
    }

    #[test]
    fn arabic_is_shaped_and_starts_on_the_right() {
        let mut ts = Typesetter::new();
        let mut canvas = Canvas::new(576, 0);
        let height = ts.draw(
            &mut canvas,
            "حليب كانديا 1 لتر",
            NORMAL,
            0,
            0,
            576,
            Align::Start,
            true,
        );
        assert!(height >= 26, "one line of text: {height}");
        let bitmap = canvas.to_bitmap(INK_THRESHOLD);
        assert!(
            ink_in_columns(&bitmap, 400, 576) > 0,
            "RTL start is the right side"
        );
        assert_eq!(
            ink_in_columns(&bitmap, 0, 150),
            0,
            "a short line leaves the left side blank"
        );
        assert_golden("ar_line_80", &bitmap);
    }

    #[test]
    fn latin_text_starts_on_the_left_and_centres() {
        let mut ts = Typesetter::new();
        let mut canvas = Canvas::new(384, 0);
        let first = ts.draw(
            &mut canvas,
            "Supérette El Amel",
            NORMAL,
            0,
            0,
            384,
            Align::Start,
            false,
        );
        ts.draw(
            &mut canvas,
            "4 500,00 DA",
            TextStyle {
                size_px: 38.0,
                bold: true,
            },
            0,
            first as i32,
            384,
            Align::Center,
            false,
        );
        let bitmap = canvas.to_bitmap(INK_THRESHOLD);
        assert!(ink_in_columns(&bitmap, 0, 40) > 0);
        assert_golden("fr_lines_58", &bitmap);
    }

    #[test]
    fn bold_is_heavier_than_regular() {
        let mut ts = Typesetter::new();
        let mut regular = Canvas::new(576, 0);
        let mut bold = Canvas::new(576, 0);
        ts.draw(
            &mut regular,
            "المجموع Total",
            NORMAL,
            0,
            0,
            576,
            Align::Start,
            false,
        );
        ts.draw(
            &mut bold,
            "المجموع Total",
            TextStyle {
                bold: true,
                ..NORMAL
            },
            0,
            0,
            576,
            Align::Start,
            false,
        );
        assert!(
            bold.to_bitmap(INK_THRESHOLD).black_dots()
                > regular.to_bitmap(INK_THRESHOLD).black_dots()
        );
    }

    #[test]
    fn long_text_wraps_at_words() {
        let mut ts = Typesetter::new();
        let mut canvas = Canvas::new(384, 0);
        let one = ts.measure("x", NORMAL);
        assert!(one > 0);
        let height = ts.draw(
            &mut canvas,
            "زيت المائدة إليو خمسة لترات قارورة بلاستيكية كبيرة الحجم",
            NORMAL,
            0,
            0,
            384,
            Align::Start,
            true,
        );
        assert!(height >= 2 * 26, "wrapped on two lines or more: {height}");
        assert_eq!(canvas.height(), height);
    }

    #[test]
    fn a_png_logo_is_scaled_and_dithered() {
        // a 200×100 horizontal gradient
        let mut png_bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png_bytes, 200, 100);
            encoder.set_color(png::ColorType::Grayscale);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            let data: Vec<u8> = (0..100)
                .flat_map(|_| (0..200).map(|x| (x * 255 / 199) as u8))
                .collect();
            writer.write_image_data(&data).unwrap();
        }
        let logo = logo_bitmap(&png_bytes, 150, 120).unwrap();
        assert_eq!(
            (logo.width(), logo.height()),
            (150, 75),
            "scaled to the width, ratio kept"
        );
        let left = ink_in_columns(&logo, 0, 30);
        let right = ink_in_columns(&logo, 120, 150);
        assert!(
            left > right * 3,
            "the dark side is denser: {left} vs {right}"
        );
        assert!(right > 0 || left > 0);
        assert_eq!(
            logo_bitmap(b"not an image", 150, 120).unwrap_err().code(),
            "logo_unreadable"
        );
    }

    #[test]
    fn threshold_packs_the_canvas() {
        let mut canvas = Canvas::new(16, 2);
        canvas.blend(0, 0, 200);
        canvas.blend(1, 0, 100);
        canvas.fill_rect(8, 1, 8, 1);
        let bitmap = canvas.to_bitmap(INK_THRESHOLD);
        assert!(bitmap.get(0, 0) && !bitmap.get(1, 0));
        assert_eq!(bitmap.rows(1, 1), [0, 0xFF]);
    }

    #[test]
    fn a_long_bilingual_receipt_renders_well_under_the_budget() {
        let started = std::time::Instant::now();
        let mut ts = Typesetter::new();
        let mut canvas = Canvas::new(576, 0);
        let mut top = 0;
        for i in 0..40 {
            let (text, rtl) = if i % 2 == 0 {
                ("حليب كانديا 1 لتر 2 x 120,00", true)
            } else {
                ("Supérette El Amel 4 500,00 DA", false)
            };
            top += ts.draw(
                &mut canvas,
                text,
                NORMAL,
                0,
                top as i32,
                576,
                Align::Start,
                rtl,
            );
        }
        let bitmap = canvas.to_bitmap(INK_THRESHOLD);
        let elapsed = started.elapsed();
        println!(
            "40 lines, {} rows, rendered in {elapsed:?}",
            bitmap.height()
        );
        assert!(
            elapsed < std::time::Duration::from_millis(1500),
            "the receipt budget is 1.5 s: {elapsed:?}"
        );
    }
}

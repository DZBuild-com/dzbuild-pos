//! A [`Layout`] to ESC/POS bytes (spec §9.1): text mode with the profile's code page for Latin
//! documents; one raster (`GS v 0` bands) for Arabic or bilingual ones, or as soon as the code
//! page lacks a character. Barcodes stay native in both modes.

use crate::bitmap::Bitmap;
use crate::codepage::CodePage;
use crate::error::HwError;
use crate::escpos::{Builder, Cut, Justify};
use crate::layout::{Layout, Row};
use crate::profile::{CutMode, PaperWidth, Profile, RenderMode};
use crate::raster::{Align, Canvas, INK_THRESHOLD, TextStyle, Typesetter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawerKick {
    None,
    BeforePrint,
    AfterPrint,
}

pub struct Job<'a> {
    pub layout: &'a Layout,
    pub logo: Option<&'a Bitmap>,
    pub drawer: DrawerKick,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    pub bytes: Vec<u8>,
    pub raster: bool,
}

const MARGIN: u32 = 4;
const GAP: u32 = 16;
const BARCODE_HEIGHT: u8 = 60;

/// Dots per QR module: 4 keeps a 33-module code at 164 dots (≈ 20 mm on 203 dpi), scannable
/// by phones on 58 mm paper.
const QR_SCALE: u32 = 4;

/// The QR with a 4-module quiet zone, centred in a band `width` dots wide. Modules past
/// `size × size` are ignored.
pub(crate) fn qr_bitmap(size: u32, modules: &str, width: u32) -> Bitmap {
    let side = (size + 8) * QR_SCALE;
    let mut band = Bitmap::new(width, side);
    let left = width.saturating_sub(side) / 2;
    let cells = usize::try_from(size).map_or(0, |s| s.saturating_mul(s));
    for (i, c) in modules.chars().take(cells).enumerate() {
        if c != '1' {
            continue;
        }
        let i = u32::try_from(i).unwrap_or(u32::MAX);
        let (mx, my) = (i % size.max(1), i / size.max(1));
        for dy in 0..QR_SCALE {
            for dx in 0..QR_SCALE {
                band.set(
                    left + (mx + 4) * QR_SCALE + dx,
                    (my + 4) * QR_SCALE + dy,
                    true,
                );
            }
        }
    }
    band
}

/// Normal and large text in dots. 30 on 58 mm keeps a bilingual amount-due label
/// ("Net à payer / الصافي للدفع", 354 dots in bold) on one 376-dot line; at 32 it needs 377.
pub(crate) fn font_sizes(width: PaperWidth) -> (f32, f32) {
    match width {
        PaperWidth::Mm58 => (22.0, 30.0),
        PaperWidth::Mm80 => (26.0, 38.0),
    }
}

/// Whether text mode prints `text` as written: every word in the code page. Line feeds, tabs
/// and other spaces never reach the printer as characters (text mode breaks and pads the lines
/// itself), so they never force a raster.
fn printable(text: &str, page: CodePage) -> bool {
    text.split_whitespace().all(|word| page.can_encode(word))
}

/// Raster for RTL layouts, for `RenderMode::Raster`, and whenever a text is not printable in the
/// code page — even under `RenderMode::Text`: a receipt never prints question marks.
pub fn uses_raster(layout: &Layout, profile: &Profile) -> bool {
    profile.render == RenderMode::Raster
        || layout.rtl
        || !layout.texts().all(|t| printable(t, profile.code_page))
}

pub fn render(
    job: &Job<'_>,
    profile: &Profile,
    typesetter: &mut Typesetter,
) -> Result<Rendered, HwError> {
    profile.validate()?;
    let raster = uses_raster(job.layout, profile);
    let mut b = Builder::new();
    b.init(profile.exit_chinese_mode);
    if job.drawer == DrawerKick::BeforePrint {
        b.drawer_kick(profile.drawer_pin);
    }
    if raster {
        for segment in segments(job, profile, typesetter) {
            match segment {
                Segment::Image(image) => {
                    b.raster(&image, profile.band_rows)?;
                }
                Segment::Barcode(data) => barcode(&mut b, &data, profile)?,
            }
        }
    } else {
        text_body(&mut b, job, profile)?;
    }
    finish(&mut b, profile, job.drawer);
    Ok(Rendered {
        bytes: b.into_bytes(),
        raster,
    })
}

pub(crate) fn finish(b: &mut Builder, profile: &Profile, drawer: DrawerKick) {
    b.feed(profile.feed_before_cut);
    match profile.cut {
        CutMode::Full => {
            b.cut(Cut::Full, 0);
        }
        CutMode::Partial => {
            b.cut(Cut::Partial, 0);
        }
        CutMode::None => {}
    }
    if drawer == DrawerKick::AfterPrint {
        b.drawer_kick(profile.drawer_pin);
    }
}

pub(crate) fn barcode(b: &mut Builder, data: &str, profile: &Profile) -> Result<(), HwError> {
    b.justify(Justify::Center);
    b.code128(data, profile.dots, BARCODE_HEIGHT)?;
    b.line(&[]).justify(Justify::Left);
    Ok(())
}

fn justify(align: Align) -> Justify {
    match align {
        Align::Start => Justify::Left,
        Align::Center => Justify::Center,
        Align::End => Justify::Right,
    }
}

/// Word wrap by characters, keeping the leading indent of the text on every line.
pub(crate) fn wrap(text: &str, width: usize) -> Vec<String> {
    let indent: String = text.chars().take_while(|c| *c == ' ').collect();
    let width = width.saturating_sub(indent.chars().count()).max(1);
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let mut chars: Vec<char> = word.chars().collect();
        while chars.len() > width {
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            lines.push(chars.drain(..width).collect());
        }
        let word: String = chars.into_iter().collect();
        let len = current.chars().count();
        if len == 0 {
            current = word;
        } else if len + 1 + word.chars().count() <= width {
            current.push(' ');
            current.push_str(&word);
        } else {
            lines.push(std::mem::replace(&mut current, word));
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines.into_iter().map(|l| format!("{indent}{l}")).collect()
}

/// "label ...... figure" on one line when it fits, else the label wrapped and the figure alone,
/// flush right.
pub(crate) fn pair_lines(start: &str, end: &str, width: usize) -> Vec<String> {
    let (s, e) = (start.chars().count(), end.chars().count());
    if s + 1 + e <= width {
        return vec![format!("{start}{}{end}", " ".repeat(width - s - e))];
    }
    let mut lines = wrap(start, width);
    lines.push(format!("{end:>width$}"));
    lines
}

/// Every kind of space as a plain one: a pair prints on one line.
fn one_line(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect()
}

fn text_body(b: &mut Builder, job: &Job<'_>, profile: &Profile) -> Result<(), HwError> {
    let cols = usize::from(profile.columns);
    let page = profile.code_page;
    b.code_page(profile.code_page_selector);
    for row in &job.layout.rows {
        match row {
            Row::Logo => {
                if let Some(logo) = job.logo {
                    let dots = u32::from(profile.dots);
                    let mut band = Bitmap::new(dots, logo.height());
                    band.paste(logo, dots.saturating_sub(logo.width()) / 2, 0);
                    b.raster(&band, profile.band_rows)?;
                }
            }
            Row::Text {
                text,
                align,
                large,
                bold,
            } => {
                let width = if *large { cols / 2 } else { cols };
                b.justify(justify(*align)).bold(*bold);
                if *large {
                    b.size(2, 2);
                }
                // the owner's line breaks stay; the code page never sees a line feed
                for paragraph in text.lines() {
                    for line in wrap(paragraph, width) {
                        b.line(&page.encode(&line));
                    }
                }
                if *large {
                    b.size(1, 1);
                }
                b.bold(false).justify(Justify::Left);
            }
            Row::Pair {
                start,
                end,
                large,
                bold,
            } => {
                let width = if *large { cols / 2 } else { cols };
                b.bold(*bold);
                if *large {
                    b.size(2, 2);
                }
                for line in pair_lines(&one_line(start), &one_line(end), width) {
                    b.line(&page.encode(&line));
                }
                if *large {
                    b.size(1, 1);
                }
                b.bold(false);
            }
            Row::Rule => {
                b.line(&vec![b'-'; cols]);
            }
            Row::Feed { lines } => {
                b.feed(*lines);
            }
            Row::Barcode { data } => barcode(b, data, profile)?,
            Row::Qr { size, modules } => {
                b.raster(
                    &qr_bitmap(*size, modules, u32::from(profile.dots)),
                    profile.band_rows,
                )?;
            }
        }
    }
    Ok(())
}

pub(crate) enum Segment {
    Image(Bitmap),
    Barcode(String),
}

fn line_height(size: f32) -> u32 {
    (size * 1.35).round() as u32
}

/// The layout drawn as images, cut where a native barcode goes.
pub(crate) fn segments(job: &Job<'_>, profile: &Profile, ts: &mut Typesetter) -> Vec<Segment> {
    let width = u32::from(profile.dots);
    let inner = width.saturating_sub(2 * MARGIN);
    let (normal, big) = font_sizes(profile.width);
    let rtl = job.layout.rtl;
    let style = |large: bool, bold: bool| TextStyle {
        size_px: if large { big } else { normal },
        bold,
    };
    let mut out = Vec::new();
    let mut canvas = Canvas::new(width, 0);
    let mut y = 0_u32;
    for row in &job.layout.rows {
        match row {
            Row::Logo => {
                if let Some(logo) = job.logo {
                    canvas.paste(logo, width.saturating_sub(logo.width()) / 2, y);
                    y += logo.height() + 8;
                    canvas.grow_to(y);
                }
            }
            Row::Text {
                text,
                align,
                large,
                bold,
            } => {
                y += ts.draw(
                    &mut canvas,
                    text,
                    style(*large, *bold),
                    MARGIN as i32,
                    y as i32,
                    inner,
                    *align,
                    rtl,
                );
            }
            Row::Pair {
                start,
                end,
                large,
                bold,
            } => {
                let st = style(*large, *bold);
                let end_w = (ts.measure(end, st) + 2).min(inner);
                let start_w = inner.saturating_sub(end_w + GAP).max(1);
                if ts.measure(start, st) + 2 > start_w {
                    // as in text mode: the label on the whole width, the figure under it at
                    // the end, rather than the label squeezed into a column beside it
                    for (part, align) in [(start, Align::Start), (end, Align::End)] {
                        y += ts.draw(
                            &mut canvas,
                            part,
                            st,
                            MARGIN as i32,
                            y as i32,
                            inner,
                            align,
                            rtl,
                        );
                    }
                    continue;
                }
                let (start_x, end_x) = if rtl {
                    (MARGIN + end_w + GAP, MARGIN)
                } else {
                    (MARGIN, MARGIN + inner - end_w)
                };
                let top = y as i32;
                let h_start = ts.draw(
                    &mut canvas,
                    start,
                    st,
                    start_x as i32,
                    top,
                    start_w,
                    Align::Start,
                    rtl,
                );
                let h_end = ts.draw(
                    &mut canvas,
                    end,
                    st,
                    end_x as i32,
                    top,
                    end_w,
                    Align::End,
                    rtl,
                );
                y += h_start.max(h_end);
            }
            Row::Rule => {
                canvas.grow_to(y + 12);
                let mut x = MARGIN;
                while x + 6 <= width - MARGIN {
                    canvas.fill_rect(x as i32, (y + 5) as i32, 6, 2);
                    x += 10;
                }
                y += 12;
            }
            Row::Feed { lines } => {
                y += u32::from(*lines) * line_height(normal);
                canvas.grow_to(y);
            }
            Row::Qr { size, modules } => {
                let block = qr_bitmap(*size, modules, width);
                canvas.paste(&block, 0, y);
                y += block.height() + 8;
                canvas.grow_to(y);
            }
            Row::Barcode { data } => {
                if y > 0 {
                    canvas.grow_to(y);
                    out.push(Segment::Image(canvas.to_bitmap(INK_THRESHOLD)));
                }
                canvas = Canvas::new(width, 0);
                y = 0;
                out.push(Segment::Barcode(data.clone()));
            }
        }
    }
    if y > 0 {
        canvas.grow_to(y);
        out.push(Segment::Image(canvas.to_bitmap(INK_THRESHOLD)));
    }
    out
}

/// The whole document as one image (a barcode is a blank band): golden tests, later previews.
pub fn preview(job: &Job<'_>, profile: &Profile, ts: &mut Typesetter) -> Bitmap {
    let gap = u32::from(BARCODE_HEIGHT) + 30;
    let parts = segments(job, profile, ts);
    let height = parts
        .iter()
        .map(|s| match s {
            Segment::Image(image) => image.height(),
            Segment::Barcode(_) => gap,
        })
        .sum();
    let mut out = Bitmap::new(u32::from(profile.dots), height);
    let mut y = 0;
    for part in parts {
        match part {
            Segment::Image(image) => {
                out.paste(&image, 0, y);
                y += image.height();
            }
            Segment::Barcode(_) => y += gap,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::Preset;

    fn height(layout: &Layout, profile: &Profile) -> u32 {
        let job = Job {
            layout,
            logo: None,
            drawer: DrawerKick::None,
        };
        preview(&job, profile, &mut Typesetter::new()).height()
    }

    /// As in text mode: a label and its figure share a line when they fit; otherwise the label
    /// takes the whole width and the figure goes under it, at the end side.
    #[test]
    fn a_pair_too_wide_for_one_line_puts_its_figure_under_the_label() {
        let profile = Profile::preset(Preset::Generic, PaperWidth::Mm58);
        let line = line_height(font_sizes(PaperWidth::Mm58).1);
        for rtl in [false, true] {
            let due = |start: &str| Layout {
                rtl,
                rows: vec![Row::Pair {
                    start: start.into(),
                    end: "1 078,50 DA".into(),
                    large: true,
                    bold: true,
                }],
            };
            assert_eq!(height(&due("Total"), &profile), line, "side by side");
            assert_eq!(
                height(&due("Net à payer / الصافي للدفع"), &profile),
                2 * line,
                "the label on one line, the figure on the next (rtl {rtl})"
            );
        }
    }

    /// A tab or a line feed in a name never reaches the printer inside a text-mode pair: it
    /// would print as "?" or break the line under its figure.
    #[test]
    fn text_mode_pairs_print_every_space_as_a_space() {
        let layout = Layout {
            rtl: false,
            rows: vec![crate::layout::pair("Caissier", "Yacine\tB.\nK.")],
        };
        let profile = Profile::preset(Preset::Generic, PaperWidth::Mm58);
        let job = Job {
            layout: &layout,
            logo: None,
            drawer: DrawerKick::None,
        };
        let out = render(&job, &profile, &mut Typesetter::new()).unwrap();
        assert!(!out.raster);
        let text = crate::escpos::dump(&out.bytes);
        assert!(
            text.contains(&format!("Caissier{}Yacine B. K.<0A>", " ".repeat(12))),
            "{text}"
        );
    }

    #[test]
    fn a_qr_row_prints_as_a_centred_raster_block_in_both_modes() {
        let qr = pos_docs::a4::qr::qr("https://amel.dzbuild.app").unwrap();
        let layout = Layout {
            rtl: false,
            rows: vec![Row::Qr {
                size: qr.size,
                modules: qr.modules.clone(),
            }],
        };
        let block = qr_bitmap(qr.size, &qr.modules, 384);
        assert_eq!(block.height(), (qr.size + 8) * 4);
        assert!(block.black_dots() > 0);
        assert!(
            !block.get(0, 0) && !block.get(383, 0),
            "a quiet zone around it"
        );
        for mode in [RenderMode::Text, RenderMode::Raster] {
            let mut profile = Profile::preset(Preset::Generic, PaperWidth::Mm58);
            profile.render = mode;
            let job = Job {
                layout: &layout,
                logo: None,
                drawer: DrawerKick::None,
            };
            let out = render(&job, &profile, &mut Typesetter::new()).unwrap();
            assert!(
                out.bytes.windows(3).any(|w| w == [0x1d, b'v', b'0']),
                "GS v 0 raster in {mode:?}"
            );
        }
    }
}

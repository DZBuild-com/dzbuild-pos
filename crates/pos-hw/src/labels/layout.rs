//! One layout for every label output: the boxes of a label's texts and barcode in printer dots.
//! The TSPL and ZPL generators send the text part as one bitmap and the barcode as a native
//! command at its box; the preview draws both from the same boxes, so the screen shows the label
//! the printer makes. Rules: 1.2 mm margins; the barcode centred at the bottom with quiet zones
//! of 10 modules and its human-readable line, module width 4 → 1 dots, the largest that fits, bars
//! a third of the label high between 5 mm and 10 mm (12 mm on labels over 40 mm); the price row
//! just above it (price at the end side, the struck old price at the start side when it fits
//! beside the price); the other texts from the top (shop, name on `name_lines` lines,
//! second-language name, weight line, details), no text ever below the top of the price row. When
//! they do not fit, the reference goes first, then the packing date, the second name, the shop and
//! the name lines down to one; then the bars come down to 5 mm, the price down to the name's size,
//! and the name moves beside the price on one row; only then do a jewel's line, the lot and, last
//! of all, the expiry date go (décret 13-378). The shop and the second name are cut with "…";
//! the weight and the details are printed whole or not at all. A label too small for its barcode,
//! weight and one row of name and price is refused. Every text is start-aligned in the label
//! language's direction, and [`Placed::lines`] says which texts were printed where.

use super::model::{LabelData, LabelFields, LabelSymbology, LabelTemplate};
use super::words::{self, word};
use crate::barcode::{self, QUIET_MODULES, Symbol};
use crate::bitmap::Bitmap;
use crate::error::HwError;
use crate::fmt::{amount, currency, date, qty};
use crate::raster::{Align, Canvas, INK_THRESHOLD, TextStyle, Typesetter};
use pos_core::labels::{JewelLabel, LabelWarning, dropped_details};
use pos_core::lang::Lang;
use pos_core::money::Money;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarcodeBox {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub module: u32,
    pub symbology: LabelSymbology,
    pub data: String,
    /// The digits printed under the bars (linear symbols).
    pub hri: bool,
}

/// What a text of the label says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextRole {
    Shop,
    Name,
    NameSecond,
    Weight,
    /// A jewel's metal, fineness, weight and hallmark (VRT-143).
    Jewel,
    Reference,
    Lot,
    PackedOn,
    Expiry,
    Price,
    OldPrice,
}

/// One text of the label and the box it is drawn in, in dots from the top-left corner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextBox {
    pub role: TextRole,
    pub text: String,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    pub width: u32,
    pub height: u32,
    /// Every text, rule and strike line; no barcode.
    pub text: Bitmap,
    pub barcode: Option<BarcodeBox>,
    /// Every text drawn and its box: the head lines top to bottom, then the price row (with the
    /// name when it sits beside the price). A text missing here did not fit: a batch warns when
    /// the lot or the expiry date it asked for is not among them.
    pub lines: Vec<TextBox>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LabelTexts {
    pub rtl: bool,
    pub shop: Option<String>,
    pub name: Option<String>,
    pub name_second: Option<String>,
    pub price: Option<String>,
    pub old_price: Option<String>,
    pub weight: Option<String>,
    pub details: Vec<String>,
    pub hri: Option<String>,
}

fn money(value: Money, lang: Lang) -> String {
    format!("{} {}", amount(value, lang), currency(lang))
}

fn unit_word(lang: Lang, code: &str) -> String {
    words::text(lang, &format!("label.units.{code}")).unwrap_or_else(|| code.to_owned())
}

/// `mg` as grams with two decimals, rounded half up ("5,20" for 5 200 mg; "12.35" in English for
/// 12 345 mg).
fn grams(mg: i64, lang: Lang) -> String {
    let centigrams = mg.saturating_add(5).div_euclid(10);
    amount(Money(centigrams), lang)
}

/// VRT-143: "Or 750 ‰ — 5,20 g — Poinçon Tête de Massinissa" in `lang`.
fn jewel_line(jewel: &JewelLabel, lang: Lang) -> String {
    let l = |key: &str| word(lang, key);
    let mut line = format!(
        "{} {} ‰ — {} {}",
        l(&format!("label.jewel.metal.{}", jewel.metal.as_str())),
        jewel.titre_permille,
        grams(jewel.weight_mg, lang),
        unit_word(lang, "g")
    );
    if let Some(hallmark) = jewel.hallmark.as_deref().filter(|h| !h.trim().is_empty()) {
        line.push_str(&format!(
            " — {} {}",
            l("label.jewel.hallmark"),
            hallmark.trim()
        ));
    }
    line
}

/// The detail lines in the order they are printed: the jewel line when `fields` asks for it,
/// reference, lot, packing date, expiry.
fn detail_lines(label: &LabelData, fields: &LabelFields, lang: Lang) -> Vec<(TextRole, String)> {
    let l = |key: &str| word(lang, key);
    let mut details = Vec::new();
    if let Some(jewel) = label.jewel.as_ref().filter(|_| fields.jewel) {
        details.push((TextRole::Jewel, jewel_line(jewel, lang)));
    }
    if let Some(r) = &label.reference {
        details.push((TextRole::Reference, format!("{} {r}", l("label.reference"))));
    }
    if let Some(lot) = &label.lot_number {
        details.push((TextRole::Lot, format!("{} {lot}", l("label.lot"))));
    }
    if let Some(d) = &label.packed_on {
        details.push((
            TextRole::PackedOn,
            format!("{} {}", l("label.packedOn"), date(d)),
        ));
    }
    if let Some(d) = &label.expiry {
        details.push((
            TextRole::Expiry,
            format!("{} {}", l("label.expiry"), date(d)),
        ));
    }
    details
}

/// The words of a label in its main language (the second language only adds the name); the A4
/// sheet prints the same words.
pub fn texts(label: &LabelData, fields: &LabelFields) -> LabelTexts {
    let lang = label.lang.primary();
    let l = |key: &str| word(lang, key);
    let per_unit = fields.unit && label.unit_code != "pc" && label.weight.is_none();
    let price = fields.price.then(|| {
        let p = money(label.price, lang);
        if per_unit {
            format!("{p}/{}", unit_word(lang, &label.unit_code))
        } else {
            p
        }
    });
    let kg = unit_word(lang, "kg");
    let weight = match (label.weight, label.unit_price) {
        (Some(w), Some(unit)) if fields.weight => Some(format!(
            "{} {} {kg} × {}/{kg}",
            l("label.weight"),
            qty(w, lang),
            money(unit, lang)
        )),
        _ => None,
    };
    let details = detail_lines(label, fields, lang)
        .into_iter()
        .map(|(_, text)| text)
        .collect();
    LabelTexts {
        rtl: lang == Lang::Ar,
        shop: label.shop_name.clone(),
        // Finding 39: the first name line always stays on the label, so the demo's mark leads it
        // (a label without the name field gets the mark alone).
        name: match (fields.name, label.demo) {
            (true, true) => Some(format!("{} · {}", l("label.demo"), label.name)),
            (false, true) => Some(l("label.demo")),
            (true, false) => Some(label.name.clone()),
            (false, false) => None,
        }
        .filter(|n| !n.is_empty()),
        name_second: label.name_second.clone(),
        price,
        old_price: label.old_price.map(|p| money(p, lang)),
        weight,
        details,
        hri: label
            .barcode
            .as_ref()
            .filter(|b| b.symbology != LabelSymbology::Qr)
            .map(|b| b.data.clone()),
    }
}

fn line_height(style: TextStyle) -> u32 {
    (style.size_px * 1.35).round() as u32
}

/// Small, name and price text sizes in dots (sizes are rendering, not money: f32 as in M1b).
fn sizes(dots_per_mm: u32, height_dmm: u16) -> (TextStyle, TextStyle, TextStyle) {
    let d = dots_per_mm as f32;
    let price = if height_dmm >= 300 { 5.6 * d } else { 4.2 * d };
    (
        TextStyle {
            size_px: 2.3 * d,
            bold: false,
        },
        TextStyle {
            size_px: 3.1 * d,
            bold: true,
        },
        TextStyle {
            size_px: price,
            bold: true,
        },
    )
}

fn ellipsize(ts: &mut Typesetter, line: &mut String, style: TextStyle, width: u32) {
    loop {
        let with = format!("{}…", line.trim_end());
        if ts.measure(&with, style) <= width || line.chars().count() <= 1 {
            *line = with;
            return;
        }
        line.pop();
    }
}

/// `text` broken into at most `max_lines` lines of `width` dots, the last one ending with "…"
/// when something was cut.
pub fn fit_lines(
    ts: &mut Typesetter,
    text: &str,
    style: TextStyle,
    width: u32,
    max_lines: usize,
) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut cut = false;
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_owned()
        } else {
            format!("{current} {word}")
        };
        if current.is_empty() || ts.measure(&candidate, style) <= width {
            current = candidate;
        } else {
            lines.push(std::mem::replace(&mut current, word.to_owned()));
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.len() > max_lines.max(1) {
        lines.truncate(max_lines.max(1));
        cut = true;
    }
    let last = lines.len().saturating_sub(1);
    for (i, line) in lines.iter_mut().enumerate() {
        if ts.measure(line, style) > width || (cut && i == last) {
            ellipsize(ts, line, style, width);
        }
    }
    lines
}

/// The order in which head lines give way on a full label: the reference, the packing date, the
/// second-language name, the shop, then the name lines after the first. Only then do the bars come
/// down to their 5 mm floor, the price down to the name's size, and the name beside the price on
/// one row; a jewel's line, the lot and, last of all, the expiry date go only when even that row
/// does not fit (décret 13-378: lot and DLC/DDM on food). The first name line and the weight
/// always stay.
const GIVE_WAY: [TextRole; 5] = [
    TextRole::Reference,
    TextRole::PackedOn,
    TextRole::NameSecond,
    TextRole::Shop,
    TextRole::Name,
];
const GIVE_WAY_LAST: [TextRole; 3] = [TextRole::Jewel, TextRole::Lot, TextRole::Expiry];

/// One line of text drawn from the top.
struct Line {
    role: TextRole,
    text: String,
    style: TextStyle,
    align: Align,
}

/// The texts above the price row, each fitted to its lines: the shop and the second name on one
/// line (cut with "…"), the name on `name_lines`, the weight and every detail whole on two lines
/// at most (a lot, a date or a price per kg cut short would say something else).
struct Head {
    small: TextStyle,
    name_style: TextStyle,
    shop: Option<String>,
    /// The whole name, fitted again when its lines give way.
    name: Option<String>,
    names: Vec<String>,
    second: Option<String>,
    weight: Vec<String>,
    details: Vec<(TextRole, Vec<String>)>,
}

impl Head {
    fn weight_style(&self) -> TextStyle {
        TextStyle {
            bold: true,
            ..self.small
        }
    }

    /// The dots the lines take, one line height each.
    fn used(&self) -> u32 {
        let small_lines = usize::from(self.shop.is_some())
            + usize::from(self.second.is_some())
            + self.weight.len()
            + self.details.iter().map(|(_, d)| d.len()).sum::<usize>();
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        count(small_lines) * line_height(self.small)
            + count(self.names.len()) * line_height(self.name_style)
    }

    /// Takes away one line of `role`; false when there is none to take.
    fn give_way(&mut self, role: TextRole, ts: &mut Typesetter, inner: u32) -> bool {
        match role {
            TextRole::Shop => self.shop.take().is_some(),
            TextRole::NameSecond => self.second.take().is_some(),
            TextRole::Name if self.names.len() > 1 => {
                let keep = self.names.len() - 1;
                let name = self.name.clone().unwrap_or_default();
                self.names = fit_lines(ts, &name, self.name_style, inner, keep);
                true
            }
            TextRole::Jewel
            | TextRole::Reference
            | TextRole::Lot
            | TextRole::PackedOn
            | TextRole::Expiry => {
                let before = self.details.len();
                self.details.retain(|(r, _)| *r != role);
                self.details.len() < before
            }
            _ => false,
        }
    }

    /// The lines in printing order.
    fn lines(&self) -> Vec<Line> {
        let line = |role, text: &String, style, align| Line {
            role,
            text: text.clone(),
            style,
            align,
        };
        let mut out = Vec::new();
        out.extend(
            self.shop
                .iter()
                .map(|s| line(TextRole::Shop, s, self.small, Align::Center)),
        );
        out.extend(
            self.names
                .iter()
                .map(|n| line(TextRole::Name, n, self.name_style, Align::Start)),
        );
        out.extend(
            self.second
                .iter()
                .map(|s| line(TextRole::NameSecond, s, self.small, Align::Start)),
        );
        let weight_style = self.weight_style();
        out.extend(
            self.weight
                .iter()
                .map(|w| line(TextRole::Weight, w, weight_style, Align::Start)),
        );
        out.extend(
            self.details
                .iter()
                .flat_map(|(role, d)| d.iter().map(|d| line(*role, d, self.small, Align::Start))),
        );
        out
    }
}

/// The barcode's symbol and how it can shrink.
struct Bars {
    linear: bool,
    /// The symbol's width in modules, quiet zones excluded.
    modules: u32,
    /// The widest module that fits the label (4 → 1 dots).
    widest: u32,
    /// The digits under linear bars.
    hri_h: u32,
    preferred: u32,
    floor: u32,
}

impl Bars {
    /// The module and the symbol's height for bars `bar_h` dots high (a QR code is square).
    fn size(&self, bar_h: u32) -> (u32, u32) {
        if self.linear {
            (self.widest, bar_h)
        } else {
            let module = self.widest.min((bar_h / self.modules.max(1)).max(1));
            (module, self.modules * module)
        }
    }
}

/// `text` on at most `max_lines` lines of `width` dots, or `None` when it would have to be cut.
fn fit_whole(
    ts: &mut Typesetter,
    text: &str,
    style: TextStyle,
    width: u32,
    max_lines: usize,
) -> Option<Vec<String>> {
    let lines = fit_lines(ts, text, style, width, max_lines);
    let words: Vec<&str> = text.split_whitespace().collect();
    (lines.join(" ") == words.join(" ")).then_some(lines)
}

/// `style` made smaller, not below `min_px`, until `text` fits `width` dots on one line.
fn narrow(
    ts: &mut Typesetter,
    text: &str,
    style: TextStyle,
    min_px: f32,
    width: u32,
) -> Option<TextStyle> {
    let wide = ts.measure(text, style);
    if wide <= width {
        return Some(style);
    }
    // the width follows the size: start from the proportional size, then step down
    let mut size_px = (style.size_px * width as f32 / wide as f32).floor();
    while size_px >= min_px {
        let s = TextStyle { size_px, ..style };
        if ts.measure(text, s) <= width {
            return Some(s);
        }
        size_px -= 1.0;
    }
    None
}

/// `style` as large as one line of it fits `room` dots, from its own size down to `min_px`.
fn shrink(style: TextStyle, min_px: f32, room: u32) -> Option<TextStyle> {
    [style.size_px, room as f32 / 1.35, min_px]
        .into_iter()
        .map(|size_px| TextStyle { size_px, ..style })
        .filter(|s| (min_px..=style.size_px).contains(&s.size_px))
        .find(|s| line_height(*s) <= room)
}

/// The box of `text` drawn in `style` from `top` on a line of `inner` dots after `margin`.
#[allow(clippy::too_many_arguments)]
fn text_box(
    ts: &mut Typesetter,
    role: TextRole,
    text: &str,
    style: TextStyle,
    align: Align,
    rtl: bool,
    (margin, inner): (u32, u32),
    (top, height): (u32, u32),
) -> TextBox {
    let width = ts.measure(text, style);
    let free = inner.saturating_sub(width);
    let x = match (align, rtl) {
        (Align::Center, _) => margin + free / 2,
        (Align::Start, false) | (Align::End, true) => margin,
        (Align::Start, true) | (Align::End, false) => margin + free,
    };
    TextBox {
        role,
        text: text.to_owned(),
        x,
        y: top,
        width,
        height,
    }
}

fn dots(dmm: u16, dots_per_mm: u32) -> u32 {
    u32::from(dmm) * dots_per_mm / 10
}

/// The label's boxes at `dots_per_mm` (8 at 203 dpi, 12 at 300 dpi): the texts as one bitmap, the
/// box of every text and the barcode's box. A barcode wider than the label is `barcode_invalid`;
/// a label too low (or too narrow) for its barcode and one row of name and price is
/// `printer_setting_invalid` on `heightDmm` (or `widthDmm`).
pub fn place(
    template: &LabelTemplate,
    label: &LabelData,
    dots_per_mm: u32,
    ts: &mut Typesetter,
) -> Result<Placed, HwError> {
    let (width, height) = (
        dots(template.width_dmm, dots_per_mm),
        dots(template.height_dmm, dots_per_mm),
    );
    let margin = 12 * dots_per_mm / 10;
    let inner = width.saturating_sub(2 * margin);
    let gap = dots_per_mm / 2;
    let t = texts(label, &template.fields);
    let (small, name_style, mut price_style) = sizes(dots_per_mm, template.height_dmm);
    let bottom = height.saturating_sub(margin);
    let bars = match &label.barcode {
        Some(b) => {
            let symbol = barcode::encode(b.symbology, &b.data)?;
            let linear = matches!(symbol, Symbol::Linear(_));
            let hri_h = if linear { line_height(small) } else { 0 };
            let modules = barcode::width_modules(&symbol);
            let quiet = if linear { 2 * QUIET_MODULES } else { 0 };
            let widest = (1..=4_u32)
                .rev()
                .find(|m| (modules + quiet) * m <= inner)
                .ok_or_else(|| HwError::Barcode("the barcode is wider than the label".into()))?;
            let cap_mm = if template.height_dmm <= 400 { 10 } else { 12 };
            let tallest = bottom.saturating_sub(margin + hri_h);
            Some(Bars {
                linear,
                modules,
                widest,
                hri_h,
                preferred: (height / 3)
                    .clamp(5 * dots_per_mm, cap_mm * dots_per_mm)
                    .min(tallest),
                floor: (5 * dots_per_mm).min(tallest),
            })
        }
        None => None,
    };
    // where the price row ends: above the bars, their digits and a gap
    let row_end = |bar_h: u32| {
        bars.as_ref().map_or(bottom, |b| {
            bottom.saturating_sub(b.hri_h + b.size(bar_h).1 + gap)
        })
    };
    let weight_style = TextStyle {
        bold: true,
        ..small
    };
    let mut head = Head {
        small,
        name_style,
        shop: t
            .shop
            .as_deref()
            .and_then(|s| fit_lines(ts, s, small, inner, 1).pop()),
        name: t.name.clone(),
        names: t
            .name
            .as_deref()
            .map(|n| fit_lines(ts, n, name_style, inner, usize::from(template.name_lines)))
            .unwrap_or_default(),
        second: t
            .name_second
            .as_deref()
            .and_then(|s| fit_lines(ts, s, small, inner, 1).pop()),
        weight: match t.weight.as_deref() {
            Some(w) => fit_whole(ts, w, weight_style, inner, 2)
                .ok_or_else(|| HwError::invalid("widthDmm"))?,
            None => Vec::new(),
        },
        // a detail that cannot be printed whole is left out, as if it had given way
        details: detail_lines(label, &template.fields, label.lang.primary())
            .into_iter()
            .filter_map(|(role, d)| fit_whole(ts, &d, small, inner, 2).map(|d| (role, d)))
            .collect(),
    };
    let fits = |head: &Head, bar_h: u32, row_h: u32| margin + head.used() + row_h <= row_end(bar_h);
    let full_h = t.price.as_ref().map_or(0, |_| line_height(price_style));
    // the price may come down to the name's size before a legal line gives way
    let least_h = full_h.min(t.price.as_ref().map_or(0, |_| line_height(name_style)));
    let mut bar_h = bars.as_ref().map_or(0, |b| b.preferred);
    'give_way: while !fits(&head, bar_h, full_h) {
        for role in GIVE_WAY {
            if head.give_way(role, ts, inner) {
                continue 'give_way;
            }
        }
        break;
    }
    let floor = bars.as_ref().map_or(0, |b| b.floor);
    while !fits(&head, bar_h, full_h) && bar_h > floor {
        bar_h -= 1;
    }
    let mut one_row = None;
    if fits(&head, bar_h, least_h) {
        // the price as large as the room above the bars allows
        let room = row_end(bar_h).saturating_sub(margin + head.used());
        if let Some(style) = shrink(price_style, name_style.size_px, room) {
            price_style = style;
        }
    } else {
        // the name beside the price on one row; the lot, then the expiry give way only when even
        // that row does not fit
        head.names.clear();
        let style = loop {
            let room = row_end(bar_h).saturating_sub(margin + head.used());
            if let Some(style) = shrink(name_style, small.size_px, room) {
                break style;
            }
            if !GIVE_WAY_LAST
                .into_iter()
                .any(|role| head.give_way(role, ts, inner))
            {
                return Err(HwError::invalid("heightDmm"));
            }
        };
        price_style = style;
        one_row = Some(style);
    }
    let row_h = match one_row {
        Some(style) => line_height(style),
        None => t.price.as_ref().map_or(0, |_| line_height(price_style)),
    };
    if let Some(price) = &t.price {
        price_style = narrow(ts, price, price_style, small.size_px, inner)
            .ok_or_else(|| HwError::invalid("widthDmm"))?;
    }
    let rtl = t.rtl;
    let mut canvas = Canvas::new(width, height);
    let mut lines = Vec::new();
    let mut y = margin;
    for Line {
        role,
        text,
        style,
        align,
    } in head.lines()
    {
        let used = ts
            .draw(
                &mut canvas,
                &text,
                style,
                margin as i32,
                y as i32,
                inner,
                align,
                rtl,
            )
            .max(line_height(style));
        lines.push(text_box(
            ts,
            role,
            &text,
            style,
            align,
            rtl,
            (margin, inner),
            (y, used),
        ));
        y += used;
    }
    let top = row_end(bar_h).saturating_sub(row_h);
    let price_w = t
        .price
        .as_deref()
        .map_or(0, |p| ts.measure(p, price_style).min(inner));
    if let (Some(style), Some(name)) = (one_row, t.name.as_deref()) {
        // the name at the start side of the price row, cut to the room the price leaves
        let room = inner.saturating_sub(price_w + if price_w > 0 { dots_per_mm } else { 0 });
        let name = fit_lines(ts, name, style, room, 1)
            .pop()
            .filter(|n| ts.measure(n, style) <= room)
            .ok_or_else(|| HwError::invalid("widthDmm"))?;
        ts.draw(
            &mut canvas,
            &name,
            style,
            margin as i32,
            top as i32,
            inner,
            Align::Start,
            rtl,
        );
        lines.push(text_box(
            ts,
            TextRole::Name,
            &name,
            style,
            Align::Start,
            rtl,
            (margin, inner),
            (top, row_h),
        ));
    }
    if let Some(price) = &t.price {
        ts.draw(
            &mut canvas,
            price,
            price_style,
            margin as i32,
            top as i32,
            inner,
            Align::End,
            rtl,
        );
        lines.push(text_box(
            ts,
            TextRole::Price,
            price,
            price_style,
            Align::End,
            rtl,
            (margin, inner),
            (top, row_h),
        ));
        // the struck old price at the start side, when the row has room for it beside the price
        let old = t.old_price.as_deref().filter(|old| {
            one_row.is_none() && ts.measure(old, small) + dots_per_mm + price_w <= inner
        });
        if let Some(old) = old {
            let old_top = top + row_h.saturating_sub(line_height(small)) / 2;
            ts.draw(
                &mut canvas,
                old,
                small,
                margin as i32,
                old_top as i32,
                inner,
                Align::Start,
                rtl,
            );
            let struck = text_box(
                ts,
                TextRole::OldPrice,
                old,
                small,
                Align::Start,
                rtl,
                (margin, inner),
                (old_top, line_height(small)),
            );
            canvas.fill_rect(
                struck.x as i32,
                (old_top + line_height(small) / 2) as i32,
                struck.width,
                (dots_per_mm / 4).max(2),
            );
            lines.push(struck);
        }
    }
    let placed_barcode = match (&bars, &label.barcode) {
        (Some(b), Some(data)) => {
            let (module, bh) = b.size(bar_h);
            let bw = b.modules * module;
            Some(BarcodeBox {
                x: width.saturating_sub(bw) / 2,
                y: bottom.saturating_sub(b.hri_h + bh),
                width: bw,
                height: bh,
                module,
                symbology: data.symbology,
                data: data.data.clone(),
                hri: b.linear,
            })
        }
        _ => None,
    };
    let mut text = Bitmap::new(width, height);
    text.paste(&canvas.to_bitmap(INK_THRESHOLD), 0, 0);
    if let Some(b) = &placed_barcode {
        // nothing of the texts may cover the bars
        for y in b.y..b.y + b.height {
            for x in b.x..b.x + b.width {
                text.set(x, y, false);
            }
        }
    }
    Ok(Placed {
        width,
        height,
        text,
        barcode: placed_barcode,
        lines,
    })
}

/// The label as the printer will make it: the texts, the bars and their digits.
pub fn preview(
    template: &LabelTemplate,
    label: &LabelData,
    dots_per_mm: u32,
    ts: &mut Typesetter,
) -> Result<Bitmap, HwError> {
    let placed = place(template, label, dots_per_mm, ts)?;
    let mut out = placed.text.clone();
    if let Some(b) = &placed.barcode {
        let symbol = barcode::encode(b.symbology, &b.data)?;
        out.paste(&barcode::draw(&symbol, b.module, b.height), b.x, b.y);
        if b.hri {
            let (small, _, _) = sizes(dots_per_mm, template.height_dmm);
            let mut digits = Canvas::new(placed.width, line_height(small));
            ts.draw(
                &mut digits,
                &b.data,
                small,
                0,
                0,
                placed.width,
                Align::Center,
                false,
            );
            out.paste(&digits.to_bitmap(INK_THRESHOLD), 0, b.y + b.height);
        }
    }
    Ok(out)
}

/// Décret 13-378: the warning for `label` when its layout on `template` leaves out the lot or
/// the expiry date the template prints (they give way last, on a label too small for them). A
/// label carrying neither is not laid out.
pub fn dropped(
    template: &LabelTemplate,
    label: &LabelData,
    dots_per_mm: u32,
    ts: &mut Typesetter,
) -> Result<Option<LabelWarning>, HwError> {
    let lot = label.lot_number.is_some() && template.fields.lot;
    let expiry = label.expiry.is_some() && template.fields.expiry;
    if !lot && !expiry {
        return Ok(None);
    }
    let placed = place(template, label, dots_per_mm, ts)?;
    Ok(dropped_from(template, label, &placed))
}

/// [`dropped`] of a label already laid out (`placed`): the print job notes it as it goes, so a
/// batch is laid out once.
pub(crate) fn dropped_from(
    template: &LabelTemplate,
    label: &LabelData,
    placed: &Placed,
) -> Option<LabelWarning> {
    let lot = label.lot_number.is_some() && template.fields.lot;
    let expiry = label.expiry.is_some() && template.fields.expiry;
    if !lot && !expiry {
        return None;
    }
    let printed = |role: TextRole| placed.lines.iter().any(|l| l.role == role);
    dropped_details(
        label,
        !lot || printed(TextRole::Lot),
        !expiry || printed(TextRole::Expiry),
    )
}

/// `data:image/png;base64,…` for an `<img>` (the CSP allows `data:` images).
pub fn data_url(png: &[u8]) -> String {
    use base64::Engine;
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    )
}

/// A grayscale PNG (black ink on white) for the screen.
pub fn to_png(bitmap: &Bitmap) -> Result<Vec<u8>, HwError> {
    let image = image::GrayImage::from_fn(bitmap.width(), bitmap.height(), |x, y| {
        image::Luma([if bitmap.get(x, y) { 0 } else { 255 }])
    });
    let mut out = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| HwError::Render(e.to_string()))?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::golden::assert_golden;
    use crate::labels::model::{LabelBarcode, LabelLang};
    use crate::testdata::{milk_label, oil_shelf_label, tomato_weighed_label};

    #[test]
    fn a_40x25_label_fits_name_price_and_ean13() {
        let (tpl, label) = milk_label(LabelLang::Fr);
        let mut ts = Typesetter::new();
        let placed = place(&tpl, &label, 8, &mut ts).unwrap();
        assert_eq!((placed.width, placed.height), (320, 200));
        let b = placed.barcode.clone().unwrap();
        assert_eq!(
            (b.symbology, b.module, b.hri),
            (LabelSymbology::Ean13, 2, true)
        );
        assert!(b.x + b.width <= 320 && b.y + b.height <= 200);
        assert!(b.x >= QUIET_MODULES * b.module, "a quiet zone on each side");
        let ink_in_bars = (b.y..b.y + b.height)
            .flat_map(|y| (b.x..b.x + b.width).map(move |x| (x, y)))
            .filter(|(x, y)| placed.text.get(*x, *y))
            .count();
        assert_eq!(ink_in_bars, 0, "no text over the bars");
        assert!(
            placed.text.black_dots() > 500,
            "the name and the price are drawn"
        );
    }

    #[test]
    fn texts_are_formatted_in_the_label_language() {
        let (tpl, milk) = milk_label(LabelLang::Fr);
        let t = texts(&milk, &tpl.fields);
        assert_eq!(
            (t.rtl, t.price.as_deref(), t.hri.as_deref()),
            (false, Some("150,00 DA"), Some("6130123456782"))
        );
        let (tpl, tomato) = tomato_weighed_label();
        let t = texts(&tomato, &tpl.fields);
        assert!(t.rtl);
        assert_eq!(t.price.as_deref(), Some("150,00 دج"));
        let weight = t.weight.unwrap();
        assert!(
            weight.contains("1,25") && weight.contains("120,00 دج"),
            "{weight}"
        );
        assert!(
            t.details.iter().any(|d| d.contains("01/10/2026")),
            "expiry as dd/mm/yyyy"
        );
        let (tpl, oil) = oil_shelf_label();
        let t = texts(&oil, &tpl.fields);
        assert_eq!(t.old_price.as_deref(), Some("595,00 دج"));
        assert_eq!(t.name_second.as_deref(), Some("Huile Elio 5 L"));
    }

    /// Finding 39: a label of the demo shop leads its first name line (which always stays) with
    /// the mark, in the label's language.
    #[test]
    fn a_demo_label_says_demo_on_its_first_line() {
        let (tpl, mut milk) = milk_label(LabelLang::Fr);
        milk.demo = true;
        let t = texts(&milk, &tpl.fields);
        assert!(t.name.as_deref().unwrap().starts_with("DÉMO · "), "{t:?}");
        let (tpl, mut tomato) = tomato_weighed_label();
        tomato.demo = true;
        assert!(
            texts(&tomato, &tpl.fields)
                .name
                .as_deref()
                .unwrap()
                .starts_with("تجريبي")
        );
        let mut ts = Typesetter::new();
        let (tpl, milk) = milk_label(LabelLang::Fr);
        let mut demo = milk.clone();
        demo.demo = true;
        let plain = place(&tpl, &milk, 8, &mut ts).unwrap();
        let marked = place(&tpl, &demo, 8, &mut ts).unwrap();
        assert_ne!(
            plain.text.black_dots(),
            marked.text.black_dots(),
            "the mark is drawn"
        );
    }

    /// VRT-143: a piece's metal, fineness, weight and hallmark in the label's language, when the
    /// template prints the jewel line and the label carries it.
    #[test]
    fn a_jewel_label_names_its_metal_fineness_weight_and_hallmark() {
        use pos_core::jewellery::Metal;
        use pos_core::labels::JewelLabel;
        let (mut tpl, mut ring) = milk_label(LabelLang::Fr);
        tpl.fields.jewel = true;
        ring.name = "Bague Fatima".into();
        ring.jewel = Some(JewelLabel {
            metal: Metal::Gold,
            titre_permille: 750,
            weight_mg: 5_200,
            hallmark: Some("Tête de Massinissa".into()),
        });
        let line = "Or 750 ‰ — 5,20 g — Poinçon Tête de Massinissa";
        assert!(
            texts(&ring, &tpl.fields).details.contains(&line.to_owned()),
            "{:?}",
            texts(&ring, &tpl.fields)
        );
        let mut silver = ring.clone();
        silver.lang = LabelLang::En;
        silver.jewel = Some(JewelLabel {
            metal: Metal::Silver,
            titre_permille: 925,
            weight_mg: 12_345,
            hallmark: None,
        });
        assert!(
            texts(&silver, &tpl.fields)
                .details
                .contains(&"Silver 925 ‰ — 12.35 g".to_owned())
        );
        let mut ar = ring.clone();
        ar.lang = LabelLang::Ar;
        assert!(
            texts(&ar, &tpl.fields)
                .details
                .contains(&"ذهب 750 ‰ — 5,20 غ — الدمغة Tête de Massinissa".to_owned())
        );
        // the field off, or a product that is no piece: no jewel line
        let mut off = tpl.clone();
        off.fields.jewel = false;
        assert!(
            !texts(&ring, &off.fields)
                .details
                .iter()
                .any(|d| d.contains('‰'))
        );
        let (_, milk) = milk_label(LabelLang::Fr);
        assert!(
            !texts(&milk, &tpl.fields)
                .details
                .iter()
                .any(|d| d.contains('‰'))
        );
        // printed on the label like the other details
        let mut ts = Typesetter::new();
        let mut tpl = crate::testdata::template("builtin.shelf_58x40", LabelLang::Fr);
        tpl.fields.jewel = true;
        let mut off = tpl.clone();
        off.fields.jewel = false;
        let placed = place(&tpl, &ring, 8, &mut ts).unwrap();
        assert!(
            placed
                .lines
                .iter()
                .any(|l| l.role == TextRole::Jewel && l.text.contains("750")),
            "{:?}",
            placed.lines
        );
        assert!(
            !place(&off, &ring, 8, &mut ts)
                .unwrap()
                .lines
                .iter()
                .any(|l| l.role == TextRole::Jewel)
        );
    }

    #[test]
    fn long_names_are_cut_to_the_template_lines() {
        let mut ts = Typesetter::new();
        let style = TextStyle {
            size_px: 24.0,
            bold: true,
        };
        let lines = fit_lines(
            &mut ts,
            "Fromage fondu en portions La Vache qui rit boîte de 24 portions",
            style,
            300,
            2,
        );
        assert_eq!(lines.len(), 2);
        assert!(lines[1].ends_with('…'));
        assert!(lines.iter().all(|l| ts.measure(l, style) <= 300));
        assert_eq!(fit_lines(&mut ts, "Sucre", style, 300, 2), ["Sucre"]);
    }

    fn roles(placed: &Placed) -> Vec<TextRole> {
        placed.lines.iter().map(|l| l.role).collect()
    }

    fn line(placed: &Placed, role: TextRole) -> &TextBox {
        placed
            .lines
            .iter()
            .find(|l| l.role == role)
            .unwrap_or_else(|| panic!("no {role:?} in {:?}", roles(placed)))
    }

    /// Every text inside the label, above the bars, and no two texts on the same dots.
    fn assert_laid_out(placed: &Placed, what: &str) {
        for l in &placed.lines {
            assert!(
                l.x + l.width <= placed.width && l.y + l.height <= placed.height,
                "{what}: {l:?} leaves the label"
            );
            if let Some(b) = &placed.barcode {
                assert!(l.y + l.height <= b.y, "{what}: {l:?} runs into the bars");
            }
        }
        for (i, a) in placed.lines.iter().enumerate() {
            for b in &placed.lines[i + 1..] {
                let apart = a.y + a.height <= b.y
                    || b.y + b.height <= a.y
                    || a.x + a.width <= b.x
                    || b.x + b.width <= a.x;
                assert!(apart, "{what}: {a:?} overlaps {b:?}");
            }
        }
    }

    /// The lowest line above the price row ends at or above the row's top.
    fn assert_head_above_price(placed: &Placed, what: &str) {
        let price = line(placed, TextRole::Price);
        let head_end = placed
            .lines
            .iter()
            .filter(|l| !matches!(l.role, TextRole::Price | TextRole::OldPrice))
            .map(|l| l.y + l.height)
            .max()
            .unwrap_or(0);
        assert!(
            head_end <= price.y,
            "{what}: the head ends at {head_end}, the price row starts at {}",
            price.y
        );
    }

    #[test]
    fn a_crowded_label_drops_details_before_the_name_and_the_price() {
        let (mut tpl, mut label) = milk_label(LabelLang::ArFr);
        tpl.height_dmm = 150; // 15 mm
        label.reference = Some("P000123".into());
        label.lot_number = Some("L2609".into());
        let mut ts = Typesetter::new();
        let placed = place(&tpl, &label, 8, &mut ts).unwrap();
        assert!(placed.barcode.is_some());
        assert!(placed.text.black_dots() > 300);
        let kept = roles(&placed);
        assert!(
            kept.contains(&TextRole::Name) && kept.contains(&TextRole::Price),
            "{kept:?}"
        );
        assert!(
            !kept.contains(&TextRole::Reference) && !kept.contains(&TextRole::NameSecond),
            "{kept:?}"
        );
        assert_laid_out(&placed, "15 mm");
        assert!(line(&placed, TextRole::Name).text.starts_with("حليب"));
    }

    #[test]
    fn a_40x20_label_keeps_its_name_above_the_price() {
        let (mut tpl, mut label) = milk_label(LabelLang::ArFr);
        (tpl.width_dmm, tpl.height_dmm) = (400, 200);
        label.reference = Some("P000123".into());
        let mut ts = Typesetter::new();
        let placed = place(&tpl, &label, 8, &mut ts).unwrap();
        assert_laid_out(&placed, "40x20");
        assert_head_above_price(&placed, "40x20");
        assert_eq!(line(&placed, TextRole::Name).text, "حليب كانديا 1 لتر");
        assert_eq!(line(&placed, TextRole::Price).text, "150,00 دج");
    }

    #[test]
    fn no_text_overlaps_another_or_the_bars_on_any_preset_size() {
        let mut ts = Typesetter::new();
        let mut refused = Vec::new();
        for (w, h) in [
            (40, 25),
            (50, 30),
            (58, 40),
            (20, 40),
            (40, 20),
            (30, 20),
            (40, 30),
            (40, 60),
            (45, 58),
            (50, 80),
            (60, 100),
            (100, 100),
            (100, 150),
        ] {
            for (mut tpl, mut label) in [
                milk_label(LabelLang::ArFr),
                milk_label(LabelLang::Fr),
                oil_shelf_label(),
                tomato_weighed_label(),
            ] {
                (tpl.width_dmm, tpl.height_dmm) = (w * 10, h * 10);
                label.reference = Some("P000123".into());
                label.lot_number = Some("L2609".into());
                label.old_price = label.old_price.or(Some(Money(17_500)));
                let what = format!("{w}x{h} {}", label.name);
                match place(&tpl, &label, 8, &mut ts) {
                    Ok(placed) => {
                        assert_laid_out(&placed, &what);
                        let kept = roles(&placed);
                        assert!(
                            kept.contains(&TextRole::Price) && kept.contains(&TextRole::Name),
                            "{what}: {kept:?}"
                        );
                    }
                    Err(HwError::Invalid { field }) => refused.push(format!("{what} {field}")),
                    Err(e) => panic!("{what}: {e:?}"),
                }
            }
        }
        // a weighed label (weight and price per kg whole, name, price, weight barcode) needs more
        // than 20 mm: those templates are refused on the field at fault, never printed overlapping
        assert_eq!(
            refused,
            [
                "20x40 طماطم widthDmm",
                "40x20 طماطم heightDmm",
                "30x20 طماطم heightDmm"
            ]
        );
    }

    #[test]
    fn the_weighed_label_keeps_its_packing_and_expiry_dates() {
        let (tpl, label) = tomato_weighed_label();
        let mut ts = Typesetter::new();
        let placed = place(&tpl, &label, 8, &mut ts).unwrap();
        assert_eq!(
            roles(&placed),
            [
                TextRole::Shop,
                TextRole::Name,
                TextRole::Weight,
                TextRole::PackedOn,
                TextRole::Expiry,
                TextRole::Price
            ]
        );
        assert!(line(&placed, TextRole::Expiry).text.contains("01/10/2026"));
        assert!(
            line(&placed, TextRole::PackedOn)
                .text
                .contains("28/09/2026")
        );
        assert_eq!(
            placed.barcode.as_ref().unwrap().height,
            80,
            "10 mm bars on a 40 mm label"
        );
        assert_laid_out(&placed, "weighed");
        assert_head_above_price(&placed, "weighed");
    }

    #[test]
    fn the_lot_and_the_expiry_outlive_the_other_details_the_shop_and_the_bars() {
        let (mut tpl, mut label) = oil_shelf_label();
        (tpl.fields.lot, tpl.fields.expiry) = (true, true);
        label.lot_number = Some("L2609".into());
        label.expiry = Some("2027-03-31".into());
        let mut ts = Typesetter::new();
        let placed = place(&tpl, &label, 8, &mut ts).unwrap();
        assert_eq!(
            roles(&placed),
            [
                TextRole::Shop,
                TextRole::Name,
                TextRole::NameSecond,
                TextRole::Lot,
                TextRole::Expiry,
                TextRole::Price,
                TextRole::OldPrice
            ],
            "the 58x40 shelf label holds its lot and expiry"
        );
        assert_laid_out(&placed, "58x40 shelf");
        (tpl.width_dmm, tpl.height_dmm) = (500, 300);
        label.reference = Some("P000123".into());
        label.packed_on = Some("2026-09-28".into());
        let placed = place(&tpl, &label, 8, &mut ts).unwrap();
        assert_eq!(
            roles(&placed),
            [
                TextRole::Name,
                TextRole::Lot,
                TextRole::Expiry,
                TextRole::Price,
                TextRole::OldPrice
            ]
        );
        let bars = placed.barcode.clone().unwrap().height;
        assert!(
            (5 * 8..10 * 8).contains(&bars),
            "the bars gave way to their 5 mm floor first: {bars}"
        );
        assert_laid_out(&placed, "50x30");
        assert_head_above_price(&placed, "50x30");
    }

    #[test]
    fn long_texts_keep_to_their_lines_and_a_value_is_never_cut() {
        let (mut tpl, mut label) = tomato_weighed_label();
        tpl.width_dmm = 400;
        label.shop_name = Some("سوبيرات الأمل حي 1000 مسكن باب الزوار الجزائر العاصمة".into());
        label.lot_number = Some("L2609-ALGER-BAB-EZZOUAR-000123-A".into());
        let mut ts = Typesetter::new();
        let placed = place(&tpl, &label, 8, &mut ts).unwrap();
        let small = sizes(8, tpl.height_dmm).0;
        let shop = line(&placed, TextRole::Shop);
        assert!(shop.text.ends_with('…'), "{shop:?}");
        for l in &placed.lines {
            if !matches!(
                l.role,
                TextRole::Name | TextRole::Price | TextRole::OldPrice
            ) {
                assert_eq!(l.height, line_height(small), "drawn on one line: {l:?}");
            }
        }
        let weight: Vec<&str> = placed
            .lines
            .iter()
            .filter(|l| l.role == TextRole::Weight)
            .map(|l| l.text.as_str())
            .collect();
        assert_eq!(weight.len(), 2, "the weight wraps on a 40 mm label");
        assert_eq!(
            Some(weight.join(" ")),
            texts(&label, &tpl.fields).weight,
            "the weight and the price per kg are printed whole"
        );
        assert!(line(&placed, TextRole::Expiry).text.ends_with("01/10/2026"));
        assert!(
            !roles(&placed).contains(&TextRole::Lot),
            "a lot that cannot be printed whole is left out, not cut"
        );
        assert_laid_out(&placed, "40 mm wide weighed");
        assert_head_above_price(&placed, "40 mm wide weighed");
    }

    #[test]
    fn a_label_too_low_for_its_barcode_and_one_row_of_text_is_refused() {
        let (mut tpl, mut label) = milk_label(LabelLang::Fr);
        tpl.height_dmm = 100; // 10 mm: the bars and their digits take it all
        let refused = place(&tpl, &label, 8, &mut Typesetter::new()).err();
        assert_eq!(refused, Some(HwError::invalid("heightDmm")));
        label.barcode = None;
        let placed = place(&tpl, &label, 8, &mut Typesetter::new()).unwrap();
        assert_laid_out(&placed, "10 mm, no barcode");
        assert!(roles(&placed).contains(&TextRole::Price));
    }

    #[test]
    fn previews_match_their_golden_images_and_become_pngs() {
        let mut ts = Typesetter::new();
        for (name, (tpl, label)) in [
            ("label_40x25_fr", milk_label(LabelLang::Fr)),
            ("label_58x40_ar_fr_promo", oil_shelf_label()),
            ("label_58x40_weighed_ar", tomato_weighed_label()),
        ] {
            let bitmap = preview(&tpl, &label, 8, &mut ts).unwrap();
            assert_golden(name, &bitmap);
        }
        let (tpl, label) = oil_shelf_label();
        let png = to_png(&preview(&tpl, &label, 8, &mut ts).unwrap()).unwrap();
        assert_eq!(&png[..4], &[0x89, b'P', b'N', b'G']);
        let image = image::load_from_memory(&png).unwrap();
        assert_eq!((image.width(), image.height()), (464, 320));
        assert!(data_url(&png).starts_with("data:image/png;base64,iVBORw0KGgo"));
    }

    #[test]
    fn a_barcode_wider_than_the_label_is_refused() {
        let (mut tpl, mut label) = milk_label(LabelLang::Fr);
        tpl.width_dmm = 150;
        label.barcode = Some(LabelBarcode {
            symbology: LabelSymbology::Code128,
            data: "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123".into(),
        });
        let error = place(&tpl, &label, 8, &mut Typesetter::new()).unwrap_err();
        assert_eq!(error.code(), "barcode_invalid");
    }
}

//! What a printed document says, row by row, before it becomes text-mode bytes or a raster.
//! Receipts, Z/X reports and test pages all produce a [`Layout`]; `render` prints any of them.

pub use crate::raster::Align;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "row", rename_all = "snake_case")]
pub enum Row {
    /// The shop logo, when the receipt options ask for it and the shop has one.
    Logo,
    Text {
        text: String,
        align: Align,
        large: bool,
        bold: bool,
    },
    /// A label at the start and a figure at the end ("Total TTC ....... 4 500,00").
    Pair {
        start: String,
        end: String,
        large: bool,
        bold: bool,
    },
    Rule,
    Feed {
        lines: u8,
    },
    /// Code 128, printed by the printer itself in both modes.
    Barcode {
        data: String,
    },
    /// A QR code (the online store): `size × size` modules, `1` dark, printed as a raster block.
    Qr {
        size: u32,
        modules: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Layout {
    /// Arabic documents: start is the right side.
    pub rtl: bool,
    pub rows: Vec<Row>,
}

impl Layout {
    /// Every piece of text, for the code page check.
    pub fn texts(&self) -> impl Iterator<Item = &str> {
        self.rows.iter().flat_map(|row| match row {
            Row::Text { text, .. } => vec![text.as_str()],
            Row::Pair { start, end, .. } => vec![start.as_str(), end.as_str()],
            Row::Barcode { data } => vec![data.as_str()],
            Row::Logo | Row::Rule | Row::Feed { .. } | Row::Qr { .. } => Vec::new(),
        })
    }
}

pub(crate) fn text(text: impl Into<String>, align: Align) -> Row {
    Row::Text {
        text: text.into(),
        align,
        large: false,
        bold: false,
    }
}

pub(crate) fn strong(text: impl Into<String>, align: Align) -> Row {
    Row::Text {
        text: text.into(),
        align,
        large: false,
        bold: true,
    }
}

pub(crate) fn pair(start: impl Into<String>, end: impl Into<String>) -> Row {
    Row::Pair {
        start: start.into(),
        end: end.into(),
        large: false,
        bold: false,
    }
}

pub(crate) fn strong_pair(start: impl Into<String>, end: impl Into<String>) -> Row {
    Row::Pair {
        start: start.into(),
        end: end.into(),
        large: false,
        bold: true,
    }
}

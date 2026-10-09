//! xlsx and csv files of every `Table` (spec §12 "every table view exports its current rows",
//! UX-043 export language). Labels come from this crate's catalogues (`columns.<key>`,
//! `tags.<domain>.<value>`); money is written as dinars with two decimals — `centimes / 100` in
//! a spreadsheet cell is the only float of M2, presentation only. An Arabic sheet reads right to
//! left. CSV files open in Excel on French and Arabic Windows (`;`, decimal comma, BOM).

use crate::locales::text;
use chrono::{Datelike, NaiveDateTime, Timelike};
use pos_core::billing::state::DocState;
use pos_core::error::AppError;
use pos_core::lang::Lang;
use pos_core::table::{Cell, ColumnKind, Table, TableColumn, TagDomain};
use pos_core::time::algiers_datetime;
use rust_xlsxwriter::{Color, ExcelDateTime, Format, FormatBorder, Workbook, Worksheet, XlsxError};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SheetFormat {
    Xlsx,
    Csv,
}

impl SheetFormat {
    pub const fn extension(self) -> &'static str {
        match self {
            SheetFormat::Xlsx => "xlsx",
            SheetFormat::Csv => "csv",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SheetMeta {
    pub title: String,
    pub subtitle: Option<String>,
    pub lang: Lang,
    /// Labels of columns whose key is not in the catalogue (a price level's name for
    /// `price.<code>`).
    pub extra_labels: BTreeMap<String, String>,
}

/// Every value of every closed list, checked for labels by the tests.
pub const TAG_VALUES: &[(TagDomain, &[&str])] = &[
    (
        TagDomain::PaymentMethod,
        &[
            "cash",
            "card_cib",
            "edahabia",
            "baridimob",
            "cheque",
            "transfer",
            "credit",
            "voucher",
            "change",
        ],
    ),
    (
        TagDomain::DocType,
        &[
            "ticket",
            "invoice",
            "proforma",
            "quote",
            "delivery_note",
            "credit_note",
            "return_note",
            "customer_order",
            "purchase_order",
            "goods_receipt",
            "supplier_invoice",
            "transfer",
        ],
    ),
    (
        TagDomain::EntryKind,
        &[
            "sale_credit",
            "voucher_redeemed",
            "purchase",
            "payment",
            "refund",
            "return",
            "adjustment",
            "opening",
        ],
    ),
    (
        TagDomain::MovementReason,
        &[
            "sale",
            "return",
            "purchase",
            "adjustment",
            "count",
            "transfer_in",
            "transfer_out",
            "loss",
            "production",
            "opening",
        ],
    ),
    (
        TagDomain::CashMovementKind,
        &[
            "float",
            "sale",
            "refund",
            "paid_in",
            "paid_out",
            "expense",
            "credit_payment",
            "drop",
        ],
    ),
    (
        TagDomain::ReturnReason,
        &[
            "defective",
            "wrong_item",
            "changed_mind",
            "damaged",
            "expired",
            "other",
            // Credit note lines (M3): their reason, or a financial credit.
            "price_adjustment",
            "commercial_gesture",
            "cancellation",
            "credit_financial",
        ],
    ),
    (TagDomain::ExpiryStatus, &["expired", "soon"]),
    (
        TagDomain::OrderStatus,
        &["draft", "sent", "partial", "received", "cancelled"],
    ),
    (TagDomain::CountStatus, &["open", "closed", "cancelled"]),
    (
        TagDomain::CustomerType,
        &[
            "individual",
            "reseller",
            "garage",
            "contractor",
            "restaurant",
            "company",
            "other",
        ],
    ),
    (
        TagDomain::Campaign,
        &["promotion", "solde", "liquidation", "deballage"],
    ),
    (
        TagDomain::PnlLine,
        &[
            "revenue",
            "cost_of_goods",
            "gross_margin",
            "stock_losses",
            "credit_adjustments",
            "stamp_absorbed",
            "expense",
            "expenses_total",
            "net_profit",
        ],
    ),
    (
        TagDomain::Metric,
        &[
            "net_sales",
            "net_ht",
            "tax",
            "discounts",
            "returns",
            "average_ticket",
            "credit_sales",
        ],
    ),
    (
        TagDomain::ExpenseGroup,
        &[
            "rent",
            "electricity",
            "water",
            "wages",
            "transport",
            "supplies",
            "maintenance",
            "other",
        ],
    ),
    (TagDomain::YesNo, &["yes", "no"]),
    (
        TagDomain::G50Line,
        &[
            "caHt9",
            "exempt9",
            "taxable9",
            "tva9",
            "caHt19",
            "exempt19",
            "taxable19",
            "tva19",
            "caHtTotal",
            "exemptTotal",
            "taxableTotal",
            "tvaTotal",
            "tvaDeductible",
            "tvaDue",
            "tvaCredit",
            "stampDuty",
        ],
    ),
    (TagDomain::DeductionReason, &["cash_over_limit"]),
    (TagDomain::DocState, &DocState::TAGS),
    (TagDomain::DiningMode, &["dine_in", "takeaway", "delivery"]),
    (
        TagDomain::AdjustReason,
        &[
            "opening",
            "correction",
            "damage",
            "loss",
            "theft",
            "expiry",
            "own_use",
            "sample",
        ],
    ),
    (
        TagDomain::ServiceStatus,
        &[
            "received",
            "diagnosing",
            "waiting_parts",
            "in_progress",
            "ready",
            "delivered",
            "returned_unrepaired",
            "cancelled",
        ],
    ),
    (
        TagDomain::ServiceKind,
        &["repair", "pressing", "tailoring", "other"],
    ),
    (TagDomain::RoundStatus, &["planned", "out", "closed"]),
    (TagDomain::StopStatus, &["pending", "delivered", "failed"]),
    (TagDomain::VoucherKind, &["gift", "exchange", "loyalty"]),
    (TagDomain::VoucherStatus, &["active", "spent", "expired"]),
    (TagDomain::Metal, &["gold", "silver", "platinum"]),
    (
        TagDomain::IdKind,
        &["cni", "passport", "driving_licence", "other"],
    ),
    (
        TagDomain::AuditAction,
        &[
            "privacy.accessed",
            "privacy.exported",
            "privacy.anonymised",
            "privacy.consent_set",
        ],
    ),
    (
        TagDomain::PrivacyPurpose,
        &["view", "export", "print", "sms", "whatsapp", "loyalty"],
    ),
];

pub fn column_label(lang: Lang, key: &str) -> Option<String> {
    text(lang.as_str(), &format!("columns.{key}"))
}

pub fn tag_label(lang: Lang, domain: TagDomain, value: &str) -> Option<String> {
    text(lang.as_str(), &format!("tags.{}.{value}", domain.as_str()))
}

pub fn report_title(lang: Lang, kind: &str) -> Option<String> {
    text(lang.as_str(), &format!("reports.{kind}"))
}

pub fn group_title(lang: Lang, group: &str) -> Option<String> {
    text(lang.as_str(), &format!("reportGroups.{group}"))
}

fn day_label(lang: Lang, day: &str) -> String {
    match (lang, chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d")) {
        (Lang::En, Ok(d)) => d.format("%Y-%m-%d").to_string(),
        (_, Ok(d)) => d.format("%d/%m/%Y").to_string(),
        (_, Err(_)) => day.to_owned(),
    }
}

pub fn range_subtitle(lang: Lang, from: &str, to: &str) -> String {
    text(lang.as_str(), "sheet.range")
        .unwrap_or_default()
        .replace("{{from}}", &day_label(lang, from))
        .replace("{{to}}", &day_label(lang, to))
}

pub fn as_of_subtitle(lang: Lang, day: &str) -> String {
    text(lang.as_str(), "sheet.asOf")
        .unwrap_or_default()
        .replace("{{day}}", &day_label(lang, day))
}

/// Header labels: the caller's extra labels, then the catalogue, then the key itself.
pub fn labels(table: &Table, lang: Lang, extra: &BTreeMap<String, String>) -> Vec<String> {
    table
        .columns
        .iter()
        .map(|c| {
            extra
                .get(&c.key)
                .cloned()
                .or_else(|| column_label(lang, &c.key))
                .unwrap_or_else(|| c.key.clone())
        })
        .collect()
}

/// Labels of the tag values present in `table`, keyed `"<domain>.<value>"` (the screens use it
/// to show the same words as the files).
pub fn tag_labels(table: &Table, lang: Lang) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (i, column) in table.columns.iter().enumerate() {
        let Some(domain) = column.tag else { continue };
        for row in table.rows.iter().chain(table.totals.iter()) {
            if let Some(value) = row.get(i).and_then(Cell::as_str) {
                out.entry(format!("{}.{value}", domain.as_str()))
                    .or_insert_with(|| {
                        tag_label(lang, domain, value).unwrap_or_else(|| value.to_owned())
                    });
            }
        }
    }
    out
}

/// A worksheet name Excel accepts (it also refuses a name that starts or ends with `'`, which a
/// French title cut at 31 characters can do: `… d'`).
pub(crate) fn sheet_name(title: &str) -> String {
    let clean: String = title
        .chars()
        .filter(|c| !"[]:*?/\\".contains(*c))
        .take(31)
        .collect();
    let clean = clean
        .trim_matches(|c: char| c == '\'' || c.is_whitespace())
        .to_owned();
    if clean.is_empty() {
        "DZBuild POS".to_owned()
    } else {
        clean
    }
}

/// `<stem>-<day>.<ext>` with the stem reduced to ASCII letters, digits and dashes.
pub fn file_name(stem: &str, day: &str, format: SheetFormat) -> String {
    let mut clean = String::new();
    for c in stem.chars() {
        let c = if c == '_' || c == ' ' { '-' } else { c };
        if c.is_ascii_alphanumeric() || (c == '-' && !clean.is_empty() && !clean.ends_with('-')) {
            clean.push(c.to_ascii_lowercase());
        }
    }
    let clean = clean.trim_matches('-');
    let stem = if clean.is_empty() { "export" } else { clean };
    format!("{stem}-{day}.{}", format.extension())
}

struct Tags {
    lang: Lang,
    cache: HashMap<(TagDomain, String), String>,
}

impl Tags {
    fn label(&mut self, domain: TagDomain, value: &str) -> String {
        let lang = self.lang;
        self.cache
            .entry((domain, value.to_owned()))
            .or_insert_with(|| tag_label(lang, domain, value).unwrap_or_else(|| value.to_owned()))
            .clone()
    }
}

fn xerr(e: XlsxError) -> AppError {
    AppError::Internal(format!("xlsx: {e}"))
}

/// `v / 10^scale` as text: fixed decimals for money, trailing zeros trimmed otherwise.
fn decimal(v: i64, scale: u32, fixed: bool, comma: bool) -> String {
    let unit = 10_u64.pow(scale);
    let a = v.unsigned_abs();
    let sign = if v < 0 { "-" } else { "" };
    let mut out = format!("{sign}{}", a / unit);
    let width = usize::try_from(scale).unwrap_or(0);
    let mut frac = format!("{:0width$}", a % unit);
    if !fixed {
        while frac.ends_with('0') {
            frac.pop();
        }
    }
    if !frac.is_empty() {
        out.push(if comma { ',' } else { '.' });
        out.push_str(&frac);
    }
    out
}

fn instant(ms: i64) -> NaiveDateTime {
    algiers_datetime(ms)
}

fn width(kind: ColumnKind) -> f64 {
    match kind {
        ColumnKind::Text => 32.0,
        ColumnKind::Code => 16.0,
        ColumnKind::Money => 14.0,
        ColumnKind::Qty | ColumnKind::Int | ColumnKind::Percent => 11.0,
        ColumnKind::Date => 12.0,
        ColumnKind::DateTime => 17.0,
        ColumnKind::Tag => 20.0,
    }
}

/// The first characters that make Excel, LibreOffice or Google Sheets read a cell as a formula
/// (OWASP "CSV injection"): a customer named `=HYPERLINK(…)` must stay a name when the owner
/// opens the export.
const FORMULA_STARTS: [char; 6] = ['=', '+', '-', '@', '\t', '\r'];

fn runs_as_formula(text: &str) -> bool {
    text.starts_with(&FORMULA_STARTS[..])
}

/// A CSV text cell: a leading `'` keeps a formula-like text a text.
fn csv_safe(text: String) -> String {
    if runs_as_formula(&text) {
        format!("'{text}")
    } else {
        text
    }
}

struct Formats {
    plain: [Format; 2],
    /// `plain` with Excel's quote prefix: the text shows as it is and editing it never turns it
    /// into a formula.
    quoted: [Format; 2],
    money: [Format; 2],
    qty: [Format; 2],
    percent: [Format; 2],
    date: [Format; 2],
    datetime: [Format; 2],
}

impl Formats {
    fn new(lang: Lang) -> Formats {
        let both = |f: Format| [f.clone(), f.set_bold()];
        let (date, datetime) = match lang {
            Lang::En => ("yyyy-mm-dd", "yyyy-mm-dd hh:mm"),
            Lang::Ar | Lang::Fr => ("dd/mm/yyyy", "dd/mm/yyyy hh:mm"),
        };
        Formats {
            plain: both(Format::new()),
            quoted: both(Format::new().set_quote_prefix()),
            money: both(Format::new().set_num_format("#,##0.00")),
            qty: both(Format::new().set_num_format("#,##0.###")),
            percent: both(Format::new().set_num_format("0.00%")),
            date: both(Format::new().set_num_format(date)),
            datetime: both(Format::new().set_num_format(datetime)),
        }
    }

    /// The format of the text cell `text`.
    fn text(&self, text: &str, bold: usize) -> &Format {
        if runs_as_formula(text) {
            &self.quoted[bold]
        } else {
            &self.plain[bold]
        }
    }
}

fn excel_instant(ms: i64) -> Result<ExcelDateTime, XlsxError> {
    let t = instant(ms);
    let (year, month, day) = (
        u16::try_from(t.year()).unwrap_or(1900),
        t.month() as u8,
        t.day() as u8,
    );
    ExcelDateTime::from_ymd(year, month, day)?.and_hms(
        t.hour() as u16,
        t.minute() as u8,
        t.second(),
    )
}

#[allow(clippy::too_many_arguments)]
fn write_cell(
    sheet: &mut Worksheet,
    row: u32,
    col: u16,
    column: &TableColumn,
    cell: &Cell,
    f: &Formats,
    bold: usize,
    tags: &mut Tags,
) -> Result<(), XlsxError> {
    match (column.kind, cell) {
        (_, Cell::Null(())) => {}
        (ColumnKind::Money, Cell::Int(v)) => {
            sheet.write_number_with_format(row, col, *v as f64 / 100.0, &f.money[bold])?;
        }
        (ColumnKind::Qty, Cell::Int(v)) => {
            sheet.write_number_with_format(row, col, *v as f64 / 1_000.0, &f.qty[bold])?;
        }
        (ColumnKind::Percent, Cell::Int(v)) => {
            sheet.write_number_with_format(row, col, *v as f64 / 10_000.0, &f.percent[bold])?;
        }
        (ColumnKind::DateTime, Cell::Int(ms)) => {
            sheet.write_datetime_with_format(row, col, &excel_instant(*ms)?, &f.datetime[bold])?;
        }
        (ColumnKind::Date, Cell::Text(day)) => match ExcelDateTime::parse_from_str(day) {
            Ok(date) => {
                sheet.write_datetime_with_format(row, col, &date, &f.date[bold])?;
            }
            Err(_) => {
                sheet.write_string_with_format(row, col, day, f.text(day, bold))?;
            }
        },
        (ColumnKind::Tag, Cell::Text(value)) => {
            let label = match column.tag {
                Some(domain) => tags.label(domain, value),
                None => value.clone(),
            };
            sheet.write_string_with_format(row, col, &label, f.text(&label, bold))?;
        }
        (_, Cell::Text(s)) => {
            sheet.write_string_with_format(row, col, s, f.text(s, bold))?;
        }
        (_, Cell::Int(v)) => {
            sheet.write_number_with_format(row, col, *v as f64, &f.plain[bold])?;
        }
    }
    Ok(())
}

const HEADER_ROW: u32 = 3;

fn write_xlsx(table: &Table, meta: &SheetMeta) -> Result<Vec<u8>, AppError> {
    let mut book = Workbook::new();
    let sheet = book.add_worksheet();
    sheet.set_name(sheet_name(&meta.title)).map_err(xerr)?;
    if meta.lang.is_rtl() {
        sheet.set_right_to_left(true);
    }
    let formats = Formats::new(meta.lang);
    let title = Format::new().set_bold().set_font_size(14);
    let header = Format::new()
        .set_bold()
        .set_background_color(Color::RGB(0xF3F1EB))
        .set_border_bottom(FormatBorder::Thin);
    // A header can be a name the shop typed (a price level): it stays text too.
    let header_quoted = header.clone().set_quote_prefix();
    sheet
        .write_string_with_format(0, 0, &meta.title, &title)
        .map_err(xerr)?;
    if let Some(subtitle) = &meta.subtitle {
        sheet.write_string(1, 0, subtitle).map_err(xerr)?;
    }
    let too_wide = || AppError::Internal("too many columns for a sheet".into());
    for (i, (column, label)) in table
        .columns
        .iter()
        .zip(labels(table, meta.lang, &meta.extra_labels))
        .enumerate()
    {
        let col = u16::try_from(i).map_err(|_| too_wide())?;
        let format = if runs_as_formula(&label) {
            &header_quoted
        } else {
            &header
        };
        sheet
            .write_string_with_format(HEADER_ROW, col, &label, format)
            .map_err(xerr)?;
        sheet
            .set_column_width(col, width(column.kind))
            .map_err(xerr)?;
    }
    let mut tags = Tags {
        lang: meta.lang,
        cache: HashMap::new(),
    };
    let too_long = || AppError::Internal("too many rows for a sheet".into());
    let mut next = HEADER_ROW + 1;
    for row in &table.rows {
        for (i, (column, cell)) in table.columns.iter().zip(row).enumerate() {
            let col = u16::try_from(i).map_err(|_| too_wide())?;
            write_cell(sheet, next, col, column, cell, &formats, 0, &mut tags).map_err(xerr)?;
        }
        next = next.checked_add(1).ok_or_else(too_long)?;
    }
    let last_data = next - 1;
    if let Some(totals) = &table.totals {
        let mut labelled = false;
        for (i, (column, cell)) in table.columns.iter().zip(totals).enumerate() {
            let col = u16::try_from(i).map_err(|_| too_wide())?;
            if !labelled && *cell == Cell::NULL {
                let word = text(meta.lang.as_str(), "sheet.total").unwrap_or_default();
                sheet
                    .write_string_with_format(next, col, &word, &formats.plain[1])
                    .map_err(xerr)?;
                labelled = true;
                continue;
            }
            labelled = true;
            write_cell(sheet, next, col, column, cell, &formats, 1, &mut tags).map_err(xerr)?;
        }
    }
    sheet.set_freeze_panes(HEADER_ROW + 1, 0).map_err(xerr)?;
    if !table.rows.is_empty() && !table.columns.is_empty() {
        let last_col = u16::try_from(table.columns.len() - 1).map_err(|_| too_wide())?;
        sheet
            .autofilter(HEADER_ROW, 0, last_data, last_col)
            .map_err(xerr)?;
    }
    book.save_to_buffer().map_err(xerr)
}

fn csv_text(column: &TableColumn, cell: &Cell, comma: bool, tags: &mut Tags) -> String {
    match (column.kind, cell) {
        (_, Cell::Null(())) => String::new(),
        (ColumnKind::Money, Cell::Int(v)) => decimal(*v, 2, true, comma),
        (ColumnKind::Qty, Cell::Int(v)) => decimal(*v, 3, false, comma),
        (ColumnKind::Percent, Cell::Int(v)) => decimal(*v, 2, false, comma),
        (ColumnKind::DateTime, Cell::Int(ms)) => instant(*ms).format("%Y-%m-%d %H:%M").to_string(),
        (ColumnKind::Tag, Cell::Text(value)) => csv_safe(match column.tag {
            Some(domain) => tags.label(domain, value),
            None => value.clone(),
        }),
        (_, Cell::Text(s)) => csv_safe(s.clone()),
        (_, Cell::Int(v)) => v.to_string(),
    }
}

fn write_csv(table: &Table, meta: &SheetMeta) -> Result<Vec<u8>, AppError> {
    let (delimiter, comma) = match meta.lang {
        Lang::En => (b',', false),
        Lang::Ar | Lang::Fr => (b';', true),
    };
    let fail = |e: csv::Error| AppError::Internal(format!("csv: {e}"));
    let mut out: Vec<u8> = "\u{feff}".as_bytes().to_vec();
    {
        let mut writer = csv::WriterBuilder::new()
            .delimiter(delimiter)
            .terminator(csv::Terminator::CRLF)
            .from_writer(&mut out);
        let header = labels(table, meta.lang, &meta.extra_labels);
        writer
            .write_record(header.into_iter().map(csv_safe))
            .map_err(fail)?;
        let mut tags = Tags {
            lang: meta.lang,
            cache: HashMap::new(),
        };
        for row in &table.rows {
            let record: Vec<String> = table
                .columns
                .iter()
                .zip(row)
                .map(|(c, cell)| csv_text(c, cell, comma, &mut tags))
                .collect();
            writer.write_record(record).map_err(fail)?;
        }
        if let Some(totals) = &table.totals {
            let mut labelled = false;
            let mut record = Vec::with_capacity(totals.len());
            for (column, cell) in table.columns.iter().zip(totals) {
                if !labelled && *cell == Cell::NULL {
                    record.push(text(meta.lang.as_str(), "sheet.total").unwrap_or_default());
                } else {
                    record.push(csv_text(column, cell, comma, &mut tags));
                }
                labelled = true;
            }
            writer.write_record(record).map_err(fail)?;
        }
        writer
            .flush()
            .map_err(|e| AppError::Internal(format!("csv: {e}")))?;
    }
    Ok(out)
}

/// The file bytes of `table` in `format`.
pub fn render(table: &Table, meta: &SheetMeta, format: SheetFormat) -> Result<Vec<u8>, AppError> {
    match format {
        SheetFormat::Xlsx => write_xlsx(table, meta),
        SheetFormat::Csv => write_csv(table, meta),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calamine::{Data, Reader, open_workbook_auto_from_rs};
    use pos_core::table::{Cell, ColumnKind as K, Table, TagDomain};
    use std::io::Cursor;

    fn sample() -> Table {
        let mut t = Table::new(&[
            ("date", K::Date),
            ("name", K::Text),
            ("method", K::Tag),
            ("qty", K::Qty),
            ("netTtc", K::Money),
            ("share", K::Percent),
            ("reference", K::Code),
        ])
        .tag("method", TagDomain::PaymentMethod);
        t.push(vec![
            Cell::text("2026-09-28"),
            Cell::text("Huile 5 L"),
            Cell::text("cash"),
            Cell::int(1_500),
            Cell::int(119_000),
            Cell::int(1_615),
            Cell::text("0012"),
        ])
        .unwrap();
        t.push(vec![
            Cell::text("2026-09-28"),
            Cell::text("Sucre 1 kg"),
            Cell::text("card_cib"),
            Cell::int(3_000),
            Cell::int(-39_000),
            Cell::int(8_385),
            Cell::text("0100"),
        ])
        .unwrap();
        t.set_totals(vec![
            Cell::NULL,
            Cell::NULL,
            Cell::NULL,
            Cell::NULL,
            Cell::int(80_000),
            Cell::NULL,
            Cell::NULL,
        ])
        .unwrap();
        t
    }

    fn meta(lang: Lang) -> SheetMeta {
        SheetMeta {
            title: "Ventes par produit".into(),
            subtitle: Some(range_subtitle(lang, "2026-09-01", "2026-09-30")),
            lang,
            extra_labels: BTreeMap::new(),
        }
    }

    fn show(d: &Data) -> String {
        match d {
            Data::Empty => String::new(),
            Data::String(s) => s.clone(),
            Data::Float(f) => format!("{f}"),
            Data::Int(i) => format!("{i}"),
            Data::DateTime(dt) => format!("{}", dt.as_f64()),
            other => format!("{other:?}"),
        }
    }

    fn read_back(bytes: Vec<u8>) -> Vec<Vec<String>> {
        let mut book = open_workbook_auto_from_rs(Cursor::new(bytes)).unwrap();
        let name = book.sheet_names()[0].clone();
        let range = book.worksheet_range(&name).unwrap();
        range.rows().map(|r| r.iter().map(show).collect()).collect()
    }

    #[test]
    fn an_xlsx_sheet_reads_back_with_labels_numbers_and_dates() {
        let rows = read_back(render(&sample(), &meta(Lang::Fr), SheetFormat::Xlsx).unwrap());
        assert_eq!(rows[0][0], "Ventes par produit");
        assert_eq!(rows[1][0], "Du 01/09/2026 au 30/09/2026");
        assert_eq!(
            rows[3],
            vec![
                "Date",
                "Nom",
                "Mode de paiement",
                "Quantité",
                "Net TTC",
                "Part",
                "Référence"
            ]
        );
        // 2026-09-28 is Excel day 46293; money in dinars, quantities in units, percents as ratios;
        // the reference keeps its leading zeros.
        assert_eq!(
            rows[4],
            vec![
                "46293",
                "Huile 5 L",
                "Espèces",
                "1.5",
                "1190",
                "0.1615",
                "0012"
            ]
        );
        assert_eq!(
            rows[5],
            vec![
                "46293",
                "Sucre 1 kg",
                "Carte CIB",
                "3",
                "-390",
                "0.8385",
                "0100"
            ]
        );
        assert_eq!(rows[6], vec!["Total", "", "", "", "800", "", ""]);
    }

    #[test]
    fn an_arabic_sheet_is_translated() {
        let rows = read_back(render(&sample(), &meta(Lang::Ar), SheetFormat::Xlsx).unwrap());
        assert_eq!(
            rows[3][..3],
            [
                "التاريخ".to_owned(),
                "الاسم".to_owned(),
                "طريقة الدفع".to_owned()
            ]
        );
        assert_eq!(rows[4][2], "نقدا");
        assert_eq!(rows[6][0], "المجموع");
    }

    #[test]
    fn csv_uses_the_semicolon_and_the_decimal_comma_in_french_and_arabic() {
        let fr = String::from_utf8(render(&sample(), &meta(Lang::Fr), SheetFormat::Csv).unwrap())
            .unwrap();
        assert!(
            fr.starts_with('\u{feff}'),
            "Excel needs the BOM to read UTF-8"
        );
        let lines: Vec<&str> = fr.trim_start_matches('\u{feff}').split("\r\n").collect();
        assert_eq!(
            lines[0],
            "Date;Nom;Mode de paiement;Quantité;Net TTC;Part;Référence"
        );
        assert_eq!(
            lines[1],
            "2026-09-28;Huile 5 L;Espèces;1,5;1190,00;16,15;0012"
        );
        assert_eq!(
            lines[2],
            "2026-09-28;Sucre 1 kg;Carte CIB;3;-390,00;83,85;0100"
        );
        assert_eq!(lines[3], "Total;;;;800,00;;");
        let en = String::from_utf8(render(&sample(), &meta(Lang::En), SheetFormat::Csv).unwrap())
            .unwrap();
        let lines: Vec<&str> = en.trim_start_matches('\u{feff}').split("\r\n").collect();
        assert_eq!(
            lines[0],
            "Date,Name,Payment method,Quantity,Net incl. VAT,Share,Reference"
        );
        assert_eq!(lines[1], "2026-09-28,Huile 5 L,Cash,1.5,1190.00,16.15,0012");
    }

    #[test]
    fn instants_are_written_in_algiers_time() {
        let mut t = Table::new(&[("at", K::DateTime)]);
        // 2026-09-28 11:00 UTC = 12:00 in Algiers.
        t.push(vec![Cell::int(1_790_593_200_000)]).unwrap();
        let csv =
            String::from_utf8(render(&t, &meta(Lang::Fr), SheetFormat::Csv).unwrap()).unwrap();
        assert!(csv.contains("2026-09-28 12:00"), "{csv}");
        let rows = read_back(render(&t, &meta(Lang::Fr), SheetFormat::Xlsx).unwrap());
        assert_eq!(rows[4][0], "46293.5");
    }

    #[test]
    fn every_tag_value_has_a_label_in_the_three_languages() {
        for lang in Lang::ALL {
            for (domain, values) in TAG_VALUES {
                for value in *values {
                    assert!(
                        tag_label(lang, *domain, value).is_some(),
                        "{} tags.{}.{value}",
                        lang.as_str(),
                        domain.as_str()
                    );
                }
            }
            for key in ["total", "range", "asOf"] {
                assert!(text(lang.as_str(), &format!("sheet.{key}")).is_some());
            }
        }
    }

    #[test]
    fn names_and_file_names_are_safe() {
        assert_eq!(
            sheet_name("Achats [2026]: fournisseurs/produits"),
            "Achats 2026 fournisseursproduit"
        );
        assert_eq!(sheet_name(""), "DZBuild POS");
        // Cut at 31 characters, this title ends with `'`, which Excel refuses.
        let title = "Ventes du magasin central ab d'Alger";
        assert_eq!(sheet_name(title), "Ventes du magasin central ab d");
        assert_eq!(sheet_name("'Soldes'"), "Soldes");
        let meta = SheetMeta {
            title: title.into(),
            ..SheetMeta::default()
        };
        assert!(render(&sample(), &meta, SheetFormat::Xlsx).is_ok());
        assert_eq!(
            file_name("sales_by_product", "2026-09-28", SheetFormat::Xlsx),
            "sales-by-product-2026-09-28.xlsx"
        );
        assert_eq!(
            file_name("../x", "2026-09-28", SheetFormat::Csv),
            "x-2026-09-28.csv"
        );
    }

    /// A customer named `=HYPERLINK(…)` must not run as a formula when the owner opens the
    /// export (OWASP "CSV injection"): a CSV text cell starting with `=`, `+`, `-` or `@` gets a
    /// leading `'`; an xlsx cell keeps its text and carries Excel's quote prefix, so editing it
    /// never turns it into a formula. Amounts stay numbers, signs included.
    #[test]
    fn text_that_could_run_as_a_formula_is_escaped() {
        let table = |names: &[(&str, &str)]| {
            let mut t = Table::new(&[
                ("name", K::Text),
                ("reference", K::Code),
                ("netTtc", K::Money),
            ]);
            for (name, code) in names {
                t.push(vec![
                    Cell::text(*name),
                    Cell::text(*code),
                    Cell::int(-39_000),
                ])
                .unwrap();
            }
            t
        };
        let risky = table(&[
            ("=HYPERLINK(\"http://x.example\",\"clic\")", "0012"),
            ("+213 555 12 34 56", "-7"),
            ("@SUM(A1)", "A1"),
            ("Karim", "C-1"),
        ]);
        let csv =
            String::from_utf8(render(&risky, &meta(Lang::Fr), SheetFormat::Csv).unwrap()).unwrap();
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(
            lines[1],
            "\"'=HYPERLINK(\"\"http://x.example\"\",\"\"clic\"\")\";0012;-390,00"
        );
        assert_eq!(lines[2], "'+213 555 12 34 56;'-7;-390,00");
        assert_eq!(lines[3], "'@SUM(A1);A1;-390,00");
        assert_eq!(lines[4], "Karim;C-1;-390,00");

        let part = |bytes: &[u8], name: &str| {
            use std::io::Read;
            let mut zip = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).unwrap();
            let mut text = String::new();
            zip.by_name(name)
                .unwrap()
                .read_to_string(&mut text)
                .unwrap();
            text
        };
        let xlsx = render(&risky, &meta(Lang::Fr), SheetFormat::Xlsx).unwrap();
        let rows = read_back(xlsx.clone());
        assert_eq!(rows[4][0], "=HYPERLINK(\"http://x.example\",\"clic\")");
        assert_eq!(
            (rows[5][0].as_str(), rows[5][1].as_str()),
            ("+213 555 12 34 56", "-7")
        );
        assert_eq!(rows[5][2], "-390", "an amount stays a number");
        assert!(!part(&xlsx, "xl/worksheets/sheet1.xml").contains("<f>"));
        assert!(part(&xlsx, "xl/styles.xml").contains("quotePrefix=\"1\""));
        let plain = render(
            &table(&[("Karim", "C-1")]),
            &meta(Lang::Fr),
            SheetFormat::Xlsx,
        )
        .unwrap();
        assert!(
            !part(&plain, "xl/styles.xml").contains("quotePrefix"),
            "only the risky cells carry the prefix"
        );
    }

    #[test]
    fn screens_get_the_same_words_as_the_files() {
        let t = sample();
        let tags = tag_labels(&t, Lang::Fr);
        assert_eq!(
            tags.into_iter().collect::<Vec<_>>(),
            [
                ("payment_method.card_cib".to_owned(), "Carte CIB".to_owned()),
                ("payment_method.cash".to_owned(), "Espèces".to_owned()),
            ]
        );
        let mut coded = Table::new(&[("method", K::Tag), ("price.gros", K::Money)])
            .tag("method", TagDomain::PaymentMethod);
        coded
            .push(vec![Cell::text("bitcoin"), Cell::int(100)])
            .unwrap();
        assert_eq!(
            tag_labels(&coded, Lang::En)["payment_method.bitcoin"],
            "bitcoin",
            "an unknown code shows as itself"
        );
        let extra = BTreeMap::from([("price.gros".to_owned(), "Gros".to_owned())]);
        assert_eq!(
            labels(&coded, Lang::Fr, &extra),
            ["Mode de paiement", "Gros"]
        );
        assert_eq!(labels(&coded, Lang::Fr, &BTreeMap::new())[1], "price.gros");
        assert_eq!(
            report_title(Lang::Ar, "sales_by_product").as_deref(),
            Some("المبيعات حسب المنتج")
        );
        assert_eq!(
            group_title(Lang::En, "customers").as_deref(),
            Some("Customers and credit")
        );
        assert_eq!(as_of_subtitle(Lang::En, "2026-09-28"), "As of 2026-09-28");
        assert_eq!(as_of_subtitle(Lang::Fr, "2026-09-28"), "Au 28/09/2026");
        assert_eq!(SheetFormat::Csv.extension(), "csv");
    }

    /// Spec §10.6 and `src/locales/GLOSSARY.md`: a report, its file and its screen use the
    /// glossary's word for a business concept, the same word as the receipt, the Z report and
    /// the other screens.
    #[test]
    fn report_words_follow_the_glossary() {
        // key → [ar, fr, en]; the comment after each entry is its GLOSSARY.md row.
        let words: &[(&str, [&str; 3])] = &[
            (
                "columns.stampDuty",
                ["حقوق الطابع", "Droit de timbre", "Stamp duty"],
            ), // 16
            (
                "tags.pnl_line.stamp_absorbed",
                [
                    "حقوق الطابع على عاتق المحل",
                    "Droit de timbre pris en charge",
                    "Stamp duty paid by the shop",
                ],
            ), // 16
            (
                "columns.nif",
                ["رقم التعريف الجبائي", "NIF", "Tax ID (NIF)"],
            ), // 17
            (
                "columns.nis",
                ["رقم التعريف الإحصائي", "NIS", "Statistical ID (NIS)"],
            ), // 18
            (
                "columns.rc",
                ["رقم السجل التجاري", "RC", "Trade register (RC)"],
            ), // 19
            (
                "columns.ai",
                ["رقم المادة الضريبية", "AI", "Tax article (AI)"],
            ), // 20
            (
                "columns.openingFloat",
                ["رصيد الصندوق الافتتاحي", "Fond de caisse", "Opening float"],
            ), // 9
            (
                "tags.cash_movement_kind.float",
                ["رصيد الصندوق الافتتاحي", "Fond de caisse", "Opening float"],
            ), // 9
            (
                "tags.payment_method.credit",
                ["الكريدي", "Crédit client", "Customer credit"],
            ), // 7
            (
                "tags.entry_kind.sale_credit",
                ["بيع بالكريدي", "Vente à crédit", "Credit sale"],
            ), // 7
            ("columns.onCredit", ["بالكريدي", "À crédit", "On credit"]), // 7
            (
                "columns.soldOnCredit",
                ["المباع بالكريدي", "Vendu à crédit", "Sold on credit"],
            ), // 7
            (
                "tags.metric.credit_sales",
                ["المبيعات بالكريدي", "Ventes à crédit", "Credit sales"],
            ), // 7
            (
                "reportGroups.customers",
                [
                    "الزبائن والكريدي",
                    "Clients et crédit",
                    "Customers and credit",
                ],
            ), // 7
            (
                "reports.credit_activity",
                [
                    "حركة كريدي الزبائن",
                    "Mouvements du crédit client",
                    "Customer credit activity",
                ],
            ), // 7
            (
                "columns.collections",
                ["الدفعات", "Versements", "Credit payments"],
            ), // 8
            (
                "columns.creditLimit",
                ["سقف الكريدي", "Plafond de crédit", "Credit limit"],
            ), // 65
            (
                "tags.payment_method.voucher",
                ["رصيد لدى المحل", "Avoir", "Store credit"],
            ), // 64
            (
                "tags.entry_kind.voucher_redeemed",
                ["استعمال رصيد المحل", "Avoir utilisé", "Store credit used"],
            ), // 64
            (
                "columns.storeCreditUsed",
                ["استعمال رصيد المحل", "Avoir utilisé", "Store credit used"],
            ), // 64
            (
                "columns.netHt",
                ["الصافي خارج الرسوم", "Net HT", "Net excl. VAT"],
            ), // 24
            (
                "columns.purchasedHt",
                ["المشتريات خارج الرسوم", "Acheté HT", "Purchased excl. VAT"],
            ), // 24
            (
                "tags.metric.net_ht",
                [
                    "صافي المبيعات خارج الرسوم",
                    "Ventes nettes HT",
                    "Net sales excl. VAT",
                ],
            ), // 24
            (
                "tags.metric.tax",
                [
                    "الرسم على القيمة المضافة المحصل",
                    "TVA collectée",
                    "VAT collected",
                ],
            ), // 15
            (
                "tags.doc_type.ticket",
                ["تذكرة الصندوق", "Ticket de caisse", "Receipt"],
            ), // 1
            ("columns.tickets", ["التذاكر", "Tickets", "Receipts"]),     // 1
            (
                "tags.doc_type.purchase_order",
                ["وصل الطلب", "Bon de commande", "Purchase order"],
            ), // 32
            (
                "tags.doc_type.goods_receipt",
                ["وصل الاستلام", "Bon de réception", "Goods receipt"],
            ), // 33
            (
                "columns.receipts",
                ["الاستلامات", "Réceptions", "Goods receipts"],
            ), // 33
            (
                "columns.lastReceipt",
                ["آخر استلام", "Dernière réception", "Last goods receipt"],
            ), // 33
            (
                "tags.doc_type.transfer",
                ["وصل تحويل", "Bon de transfert", "Transfer note"],
            ), // 69
            (
                "columns.returnNotes",
                ["وصولات الإرجاع", "Bons de retour", "Return notes"],
            ), // 41
            ("columns.terminal", ["الجهاز", "Poste", "Terminal"]),       // 51
            (
                "tags.campaign.promotion",
                ["ترويج", "Promotion", "Promotion"],
            ), // 74
            (
                "columns.promotionDiscounts",
                [
                    "تخفيضات الترويج",
                    "Remises promotions",
                    "Promotion discounts",
                ],
            ), // 74
            (
                "tags.campaign.solde",
                ["التخفيضات الموسمية", "Soldes", "Seasonal sale"],
            ), // 75
            (
                "reports.promotion_declaration",
                [
                    "تصريح التخفيضات الموسمية",
                    "Déclaration de soldes",
                    "Seasonal sale declaration",
                ],
            ), // 75
        ];
        for (key, [ar, fr, en]) in words {
            for (lang, word) in [("ar", ar), ("fr", fr), ("en", en)] {
                assert_eq!(text(lang, key).as_deref(), Some(*word), "{lang} {key}");
            }
        }
        // A tag reads like the document, the statement line or the receipt line with the same
        // code in this catalogue.
        let same: &[(&str, &str)] = &[
            ("tags.doc_type.ticket", "doc.ticket"),
            ("tags.doc_type.invoice", "doc.invoice"),
            ("tags.doc_type.proforma", "doc.proforma"),
            ("tags.doc_type.quote", "doc.quote"),
            ("tags.doc_type.delivery_note", "doc.deliveryNote"),
            ("tags.doc_type.credit_note", "doc.creditNote"),
            ("tags.doc_type.return_note", "doc.returnNote"),
            ("tags.entry_kind.sale_credit", "statement.kinds.sale_credit"),
            (
                "tags.entry_kind.voucher_redeemed",
                "statement.kinds.voucher_redeemed",
            ),
            ("columns.stampDuty", "receipt.stampDuty"),
        ];
        for (tag, twin) in same {
            for lang in ["ar", "fr", "en"] {
                assert!(text(lang, tag).is_some(), "{lang} {tag}");
                assert_eq!(text(lang, tag), text(lang, twin), "{lang} {tag} ≠ {twin}");
            }
        }
    }
}

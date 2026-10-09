//! Reading the files people bring (IMX-001…012): Excel (xlsx, xls, ods) through calamine, and
//! CSV or text in any delimiter and encoding — UTF-8 with or without BOM, UTF-16 with BOM,
//! legacy Windows-1256 (Arabic) and Windows-1252 (French) found by chardetng. Every cell becomes
//! text; typed parsing and checks are pos-core's (`imports::parse`), so a dry run explains every
//! refusal. Also the import templates and the error report, in the user's language.

use crate::locales::text;
use crate::sheet::{TAG_VALUES, column_label, tag_label};
use calamine::{Data, Range, Reader, Sheets, open_workbook_auto_from_rs};
use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};
use pos_core::catalog::normalize::normalize;
use pos_core::error::AppError;
use pos_core::imports::parse;
use pos_core::imports::spec::{FieldKind, FieldSpec, ImportEntity, Problem};
use pos_core::lang::Lang;
use rust_xlsxwriter::{Color, Format, Workbook, Worksheet, XlsxError};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::Path;

pub const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;
pub const MAX_ROWS: usize = 200_000;
/// Columns of a sheet: far beyond any import (a product sheet has about twenty).
pub const MAX_COLUMNS: usize = 512;
/// Cells kept in memory for one sheet (rows × the columns up to each row's last value), so a
/// workbook whose values sit far apart (A1 and XFD2000) is refused instead of filling the 4 GB
/// of the reference PC.
pub const MAX_CELLS: usize = 10_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FileFormat {
    Xlsx,
    Xls,
    Ods,
    Csv,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawSheet {
    pub name: String,
    /// Rows as the spreadsheet numbers them (index 0 = row 1), cells as text.
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawFile {
    pub format: FileFormat,
    pub sheets: Vec<RawSheet>,
    pub encoding: Option<String>,
    pub delimiter: Option<char>,
}

fn unreadable(file_name: &str) -> AppError {
    AppError::rejected(json!({ "fileName": file_name }), "import_file_unreadable")
}

pub(crate) fn check_size(len: u64) -> Result<(), AppError> {
    if len > MAX_FILE_BYTES {
        return Err(AppError::rejected(
            json!({ "maxBytes": MAX_FILE_BYTES }),
            "import_file_too_large",
        ));
    }
    Ok(())
}

fn too_many_rows() -> AppError {
    AppError::rejected(
        json!({ "maxRows": MAX_ROWS, "maxColumns": MAX_COLUMNS }),
        "import_file_too_large",
    )
}

pub fn read_path(path: &Path) -> Result<RawFile, AppError> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_owned();
    let meta = std::fs::metadata(path).map_err(|_| unreadable(&name))?;
    check_size(meta.len())?;
    let bytes = std::fs::read(path).map_err(|_| unreadable(&name))?;
    read_bytes(&bytes, &name)
}

pub fn read_bytes(bytes: &[u8], file_name: &str) -> Result<RawFile, AppError> {
    check_size(u64::try_from(bytes.len()).unwrap_or(u64::MAX))?;
    if bytes.is_empty() {
        return Err(AppError::rejected(
            json!({ "fileName": file_name }),
            "import_file_empty",
        ));
    }
    let ext = Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let container =
        bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]);
    let file = if container || matches!(ext.as_str(), "xlsx" | "xlsm" | "xlsb" | "xls" | "ods") {
        workbook(bytes, &ext, file_name)?
    } else {
        delimited(bytes, file_name)?
    };
    if file
        .sheets
        .iter()
        .all(|s| s.rows.iter().all(|r| r.iter().all(String::is_empty)))
    {
        return Err(AppError::rejected(
            json!({ "fileName": file_name }),
            "import_file_empty",
        ));
    }
    Ok(file)
}

fn float_text(f: f64) -> String {
    if f.is_finite() && f.fract() == 0.0 && f.abs() < 1e15 {
        return format!("{}", f as i64);
    }
    let fixed = format!("{f:.6}");
    fixed.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// The `YYYY-MM-DD` of an Excel serial day (1900 system).
fn excel_day(serial: f64) -> String {
    let base = chrono::NaiveDate::from_ymd_opt(1899, 12, 30);
    match base.and_then(|b| b.checked_add_signed(chrono::Duration::days(serial.floor() as i64))) {
        Some(day) => day.format("%Y-%m-%d").to_string(),
        None => float_text(serial),
    }
}

fn cell_text(d: &Data) -> String {
    match d {
        Data::Empty => String::new(),
        // `#N/A`, `#REF!`…: kept as text, never an empty cell, so the dry run refuses and shows it.
        Data::Error(e) => e.to_string(),
        Data::String(s) | Data::DateTimeIso(s) | Data::DurationIso(s) => s.trim().to_owned(),
        Data::Int(i) => i.to_string(),
        Data::Float(f) => float_text(*f),
        Data::Bool(b) => (if *b { "true" } else { "false" }).to_owned(),
        Data::DateTime(dt) => excel_day(dt.as_f64()),
    }
}

fn trimmed(mut row: Vec<String>) -> Vec<String> {
    while row.last().is_some_and(String::is_empty) {
        row.pop();
    }
    row.shrink_to_fit();
    row
}

/// A sheet's rows built cell by cell, within `MAX_ROWS`, `MAX_COLUMNS` and `MAX_CELLS`: only the
/// cells holding a value take room.
#[derive(Default)]
struct Grid {
    rows: Vec<Vec<String>>,
    cells: usize,
}

impl Grid {
    fn put(&mut self, (row, col): (usize, usize), text: String) -> Result<(), AppError> {
        if text.is_empty() {
            return Ok(());
        }
        if row >= MAX_ROWS || col >= MAX_COLUMNS {
            return Err(too_many_rows());
        }
        if self.rows.len() <= row {
            self.rows.resize_with(row + 1, Vec::new);
        }
        let cells = &mut self.rows[row];
        if cells.len() <= col {
            self.cells += col + 1 - cells.len();
            if self.cells > MAX_CELLS {
                return Err(too_many_rows());
            }
            cells.resize_with(col + 1, String::new);
        }
        cells[col] = text;
        Ok(())
    }

    fn into_rows(self) -> Vec<Vec<String>> {
        let mut rows: Vec<Vec<String>> = self.rows.into_iter().map(trimmed).collect();
        while rows.last().is_some_and(Vec::is_empty) {
            rows.pop();
        }
        rows
    }
}

/// An xlsx sheet read cell by cell from its XML: the reader never builds the dense grid of the
/// sheet's extent (rows × 16 384 columns for a stray value in column XFD). The extent the sheet
/// declares is not trusted: styled empty cells count in it.
fn xlsx_sheet<RS: std::io::Read + std::io::Seek>(
    book: &mut calamine::Xlsx<RS>,
    name: &str,
    file_name: &str,
) -> Result<Vec<Vec<String>>, AppError> {
    let mut reader = book
        .worksheet_cells_reader(name)
        .map_err(|_| unreadable(file_name))?;
    let mut grid = Grid::default();
    while let Some(cell) = reader.next_cell().map_err(|_| unreadable(file_name))? {
        let (row, col) = cell.get_position();
        let text = cell_text(&Data::from(cell.get_value().clone()));
        grid.put((row as usize, col as usize), text)?;
    }
    Ok(grid.into_rows())
}

/// An xls or ods sheet, which calamine reads whole: its extent is checked before a cell is
/// copied.
fn range_sheet(range: &Range<Data>) -> Result<Vec<Vec<String>>, AppError> {
    let Some((top, left)) = range.start() else {
        return Ok(Vec::new());
    };
    let (height, width) = range.get_size();
    let (bottom, right) = (top as usize + height, left as usize + width);
    if bottom > MAX_ROWS || right > MAX_COLUMNS || bottom.saturating_mul(right) > MAX_CELLS {
        return Err(too_many_rows());
    }
    let mut grid = Grid::default();
    for (row, cells) in range.rows().enumerate() {
        for (col, value) in cells.iter().enumerate() {
            grid.put((top as usize + row, left as usize + col), cell_text(value))?;
        }
    }
    Ok(grid.into_rows())
}

fn workbook(bytes: &[u8], ext: &str, file_name: &str) -> Result<RawFile, AppError> {
    let mut book = open_workbook_auto_from_rs(Cursor::new(bytes.to_vec()))
        .map_err(|_| unreadable(file_name))?;
    let mut sheets = Vec::new();
    for name in book.sheet_names() {
        let rows = match &mut book {
            Sheets::Xlsx(xlsx) => xlsx_sheet(xlsx, &name, file_name)?,
            other => {
                let range = other
                    .worksheet_range(&name)
                    .map_err(|_| unreadable(file_name))?;
                range_sheet(&range)?
            }
        };
        sheets.push(RawSheet { name, rows });
    }
    let format = match ext {
        "xls" => FileFormat::Xls,
        "ods" => FileFormat::Ods,
        _ => FileFormat::Xlsx,
    };
    Ok(RawFile {
        format,
        sheets,
        encoding: None,
        delimiter: None,
    })
}

/// Text of a CSV/TXT file and the name of its encoding.
pub fn decode(bytes: &[u8]) -> (String, &'static str) {
    if let Some(rest) = bytes.strip_prefix(b"\xEF\xBB\xBF") {
        return (String::from_utf8_lossy(rest).into_owned(), "UTF-8");
    }
    if bytes.starts_with(b"\xFF\xFE") {
        let (text, _) = encoding_rs::UTF_16LE.decode_with_bom_removal(bytes);
        return (text.into_owned(), "UTF-16LE");
    }
    if bytes.starts_with(b"\xFE\xFF") {
        let (text, _) = encoding_rs::UTF_16BE.decode_with_bom_removal(bytes);
        return (text.into_owned(), "UTF-16BE");
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        return (text.to_owned(), "UTF-8");
    }
    let mut detector = EncodingDetector::new(Iso2022JpDetection::Deny);
    detector.feed(bytes, true);
    let encoding = detector.guess(None, Utf8Detection::Allow);
    let (text, _, _) = encoding.decode(bytes);
    (text.into_owned(), encoding.name())
}

/// Occurrences of `delimiter` outside double quotes.
fn count_outside_quotes(line: &str, delimiter: char) -> usize {
    let mut quoted = false;
    let mut n = 0;
    for c in line.chars() {
        if c == '"' {
            quoted = !quoted;
        } else if c == delimiter && !quoted {
            n += 1;
        }
    }
    n
}

/// The delimiter used the same number of times on the most of the first 20 lines.
pub fn sniff_delimiter(text: &str) -> u8 {
    let lines: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(20)
        .collect();
    let mut best = (0_usize, b';');
    for delimiter in *b";,\t|" {
        let counts: Vec<usize> = lines
            .iter()
            .map(|l| count_outside_quotes(l, char::from(delimiter)))
            .collect();
        let Some(&first) = counts.first() else {
            continue;
        };
        if first == 0 {
            continue;
        }
        let steady = counts.iter().filter(|&&n| n == first).count();
        let score = steady * 1_000 + first;
        if score > best.0 {
            best = (score, delimiter);
        }
    }
    best.1
}

fn delimited(bytes: &[u8], file_name: &str) -> Result<RawFile, AppError> {
    let (text, encoding) = decode(bytes);
    let delimiter = sniff_delimiter(&text);
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes());
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|_| unreadable(file_name))?;
        rows.push(trimmed(
            record.iter().map(|c| c.trim().to_owned()).collect(),
        ));
        if rows.len() > MAX_ROWS {
            return Err(too_many_rows());
        }
    }
    while rows.last().is_some_and(Vec::is_empty) {
        rows.pop();
    }
    Ok(RawFile {
        format: FileFormat::Csv,
        sheets: vec![RawSheet {
            name: String::new(),
            rows,
        }],
        encoding: Some(encoding.to_owned()),
        delimiter: Some(char::from(delimiter)),
    })
}

fn is_numeric(cell: &str) -> bool {
    parse::money(cell).is_ok_and(|v| v.is_some())
}

/// The first of the first 20 rows with at least two filled cells, mostly words.
pub fn header_row(sheet: &RawSheet) -> usize {
    for (i, row) in sheet.rows.iter().take(20).enumerate() {
        let filled: Vec<&String> = row.iter().filter(|c| !c.is_empty()).collect();
        let words = filled.iter().filter(|c| !is_numeric(c)).count();
        if filled.len() >= 2 && words * 2 >= filled.len() {
            return i;
        }
    }
    0
}

/// Normalised header text: folded case, accents and Arabic variants, punctuation as spaces.
fn header_key(text: &str) -> String {
    let folded: String = normalize(text)
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    folded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Other names shops give to the columns (FR, AR, EN, and the usual abbreviations).
const SYNONYMS: &[(&str, &[&str])] = &[
    (
        "reference",
        &["ref", "code article", "code produit", "sku", "مرجع"],
    ),
    (
        "barcode",
        &[
            "code barre",
            "code barres",
            "codebarre",
            "ean",
            "ean13",
            "gencod",
            "barcode",
            "باركود",
            "رمز شريطي",
        ],
    ),
    (
        "nameFr",
        &[
            "designation",
            "libelle",
            "article",
            "produit",
            "nom",
            "description",
            "name",
            "product",
            "المنتج",
            "التسمية",
        ],
    ),
    (
        "nameAr",
        &[
            "nom arabe",
            "designation arabe",
            "arabic name",
            "الاسم بالعربية",
        ],
    ),
    ("nameEn", &["nom anglais", "english name"]),
    (
        "category",
        &[
            "famille",
            "rayon",
            "categorie",
            "groupe",
            "family",
            "العائلة",
        ],
    ),
    ("unit", &["unite", "u", "uom", "الوحدة"]),
    (
        "taxRate",
        &["tva", "taux tva", "tva %", "vat", "tax", "الرسم"],
    ),
    (
        "sellPrice",
        &[
            "prix",
            "prix vente",
            "pv",
            "prix ttc",
            "prix de vente ttc",
            "price",
            "selling price",
            "السعر",
        ],
    ),
    (
        "costPrice",
        &[
            "pa",
            "prix achat",
            "prix d achat",
            "cout",
            "cmup",
            "cost",
            "purchase price",
            "سعر الشراء",
        ],
    ),
    (
        "minStock",
        &["stock min", "stock minimum", "seuil", "stock alerte", "min"],
    ),
    (
        "trackStock",
        &["gerer stock", "gestion stock", "suivi stock"],
    ),
    ("brand", &["marque", "العلامة"]),
    ("supplierRef", &["ref fournisseur", "reference fournisseur"]),
    ("qty", &["quantite", "qte", "stock", "quantity", "الكمية"]),
    (
        "counted",
        &["compte", "qte comptee", "quantite comptee", "المعدود"],
    ),
    ("unitCost", &["cout unitaire", "prix unitaire", "unit cost"]),
    ("location", &["emplacement", "depot", "magasin", "الموقع"]),
    ("code", &["code client", "code fournisseur", "numero", "n"]),
    (
        "name",
        &[
            "nom",
            "raison sociale",
            "client",
            "fournisseur",
            "nom et prenom",
        ],
    ),
    (
        "phone",
        &["tel", "telephone", "mobile", "portable", "gsm", "الهاتف"],
    ),
    ("email", &["mail", "e mail", "courriel"]),
    ("address", &["adresse"]),
    (
        "amount",
        &[
            "solde",
            "montant",
            "dette",
            "credit",
            "reste a payer",
            "الدين",
            "الرصيد",
        ],
    ),
    ("creditLimit", &["plafond", "limite credit"]),
    ("discount", &["remise"]),
    (
        "notes",
        &["remarque", "remarques", "observation", "commentaire"],
    ),
    (
        "note",
        &["remarque", "observation", "commentaire", "libelle"],
    ),
    ("customerType", &["type", "type client", "categorie client"]),
    ("paymentTermsDays", &["delai", "delai paiement", "echeance"]),
];

/// The field each header most likely means: first its key or its label in any language, then a
/// synonym; each field at most once; `None` for the rest (the user maps them or leaves them out).
pub fn suggest(headers: &[String], fields: &[FieldSpec]) -> Vec<Option<String>> {
    let names: Vec<Vec<String>> = fields
        .iter()
        .map(|f| {
            let mut own = vec![header_key(&f.key.replace('.', " "))];
            for lang in Lang::ALL {
                if let Some(label) = column_label(lang, &f.key) {
                    own.push(header_key(&label));
                }
            }
            own
        })
        .collect();
    let synonyms: Vec<Vec<String>> = fields
        .iter()
        .map(|f| {
            SYNONYMS
                .iter()
                .filter(|(key, _)| *key == f.key)
                .flat_map(|(_, words)| words.iter().map(|w| header_key(w)))
                .collect()
        })
        .collect();
    let keys: Vec<String> = headers.iter().map(|h| header_key(h)).collect();
    let mut out: Vec<Option<String>> = vec![None; headers.len()];
    let mut used = vec![false; fields.len()];
    for pool in [&names, &synonyms] {
        for (h, key) in keys.iter().enumerate() {
            if out[h].is_some() || key.is_empty() {
                continue;
            }
            if let Some(i) =
                (0..fields.len()).find(|i| !used[*i] && pool[*i].iter().any(|n| n == key))
            {
                used[i] = true;
                out[h] = Some(fields[i].key.clone());
            }
        }
    }
    out
}

pub fn field_label(lang: Lang, field: &FieldSpec, extra: &BTreeMap<String, String>) -> String {
    extra
        .get(&field.key)
        .cloned()
        .or_else(|| column_label(lang, &field.key))
        .unwrap_or_else(|| field.key.clone())
}

/// `"<domain>:<normalised code or label>"` → code, for every closed list in the three languages
/// (`imports::parse::choice`).
pub fn choice_labels() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (domain, values) in TAG_VALUES {
        for value in *values {
            out.insert(
                format!("{}:{}", domain.as_str(), normalize(value)),
                (*value).to_owned(),
            );
            for lang in Lang::ALL {
                if let Some(label) = tag_label(lang, *domain, value) {
                    out.insert(
                        format!("{}:{}", domain.as_str(), normalize(&label)),
                        (*value).to_owned(),
                    );
                }
            }
        }
    }
    out
}

fn word(lang: Lang, key: &str) -> String {
    text(lang.as_str(), key).unwrap_or_else(|| key.to_owned())
}

fn xerr(e: XlsxError) -> AppError {
    AppError::Internal(format!("xlsx: {e}"))
}

fn header_format() -> Format {
    Format::new()
        .set_bold()
        .set_background_color(Color::RGB(0xF3F1EB))
}

fn write_header(sheet: &mut Worksheet, labels: &[String], width: f64) -> Result<(), AppError> {
    let bold = header_format();
    for (i, label) in labels.iter().enumerate() {
        let col = u16::try_from(i).map_err(|_| AppError::Internal("too many columns".into()))?;
        sheet
            .write_string_with_format(0, col, label, &bold)
            .map_err(xerr)?;
        sheet.set_column_width(col, width).map_err(xerr)?;
    }
    sheet.set_freeze_panes(1, 0).map_err(xerr)?;
    Ok(())
}

/// The template of an import (IMX-002): a data sheet with the translated headers only, and a
/// help sheet saying for each column whether it is required, whether it finds existing records,
/// its format and an example.
pub fn template(
    _entity: ImportEntity,
    fields: &[FieldSpec],
    lang: Lang,
    extra: &BTreeMap<String, String>,
) -> Result<Vec<u8>, AppError> {
    let mut book = Workbook::new();
    let labels: Vec<String> = fields.iter().map(|f| field_label(lang, f, extra)).collect();
    {
        let data = book.add_worksheet();
        data.set_name(word(lang, "import.template.data"))
            .map_err(xerr)?;
        data.set_right_to_left(lang.is_rtl());
        write_header(data, &labels, 20.0)?;
    }
    let help = book.add_worksheet();
    help.set_name(word(lang, "import.template.help"))
        .map_err(xerr)?;
    help.set_right_to_left(lang.is_rtl());
    let heads: Vec<String> = ["column", "required", "identifies", "format", "example"]
        .iter()
        .map(|k| word(lang, &format!("import.template.{k}")))
        .collect();
    write_header(help, &heads, 28.0)?;
    let yes_no = |b: bool| {
        tag_label(
            lang,
            pos_core::table::TagDomain::YesNo,
            if b { "yes" } else { "no" },
        )
        .unwrap_or_default()
    };
    for (i, (field, label)) in fields.iter().zip(&labels).enumerate() {
        let row = u32::try_from(i + 1).map_err(|_| AppError::Internal("too many fields".into()))?;
        let kind = field.kind.as_str();
        let (format, example) = match (field.kind, field.choice) {
            (FieldKind::Choice, Some(domain)) => {
                let values: Vec<String> = TAG_VALUES
                    .iter()
                    .filter(|(d, _)| *d == domain)
                    .flat_map(|(_, values)| {
                        values.iter().filter_map(|v| tag_label(lang, domain, v))
                    })
                    .collect();
                (
                    word(lang, "import.kinds.choice").replace("{{values}}", &values.join(", ")),
                    values.first().cloned().unwrap_or_default(),
                )
            }
            _ => (
                word(lang, &format!("import.kinds.{kind}")),
                word(lang, &format!("import.examples.{kind}")),
            ),
        };
        for (col, value) in [
            label.clone(),
            yes_no(field.required),
            yes_no(field.identifies),
            format,
            example,
        ]
        .iter()
        .enumerate()
        {
            let col =
                u16::try_from(col).map_err(|_| AppError::Internal("too many columns".into()))?;
            help.write_string(row, col, value).map_err(xerr)?;
        }
    }
    book.save_to_buffer().map_err(xerr)
}

/// The problems of a dry run or an import as a sheet (IMX-007): row, column, value, problem.
pub fn error_report(
    problems: &[Problem],
    fields: &[FieldSpec],
    lang: Lang,
    extra: &BTreeMap<String, String>,
) -> Result<Vec<u8>, AppError> {
    let mut book = Workbook::new();
    let sheet = book.add_worksheet();
    sheet
        .set_name(word(lang, "import.report.title"))
        .map_err(xerr)?;
    sheet.set_right_to_left(lang.is_rtl());
    let heads: Vec<String> = ["row", "column", "value", "problem"]
        .iter()
        .map(|k| word(lang, &format!("import.report.{k}")))
        .collect();
    write_header(sheet, &heads, 24.0)?;
    sheet.set_column_width(3, 70.0).map_err(xerr)?;
    for (i, p) in problems.iter().enumerate() {
        let row =
            u32::try_from(i + 1).map_err(|_| AppError::Internal("too many problems".into()))?;
        let column = p
            .field
            .as_ref()
            .map(|key| {
                fields
                    .iter()
                    .find(|f| &f.key == key)
                    .map_or_else(|| key.clone(), |f| field_label(lang, f, extra))
            })
            .unwrap_or_default();
        let problem = text(lang.as_str(), &format!("import.problems.{}", p.code))
            .unwrap_or_else(|| p.code.clone());
        sheet.write_number(row, 0, f64::from(p.row)).map_err(xerr)?;
        sheet.write_string(row, 1, column).map_err(xerr)?;
        sheet
            .write_string(row, 2, p.value.clone().unwrap_or_default())
            .map_err(xerr)?;
        sheet.write_string(row, 3, problem).map_err(xerr)?;
    }
    book.save_to_buffer().map_err(xerr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_core::imports::spec::{ImportEntity, Problem, base_fields};
    use rust_xlsxwriter::{ExcelDateTime, Format, Formula, Workbook};

    #[test]
    fn windows_1256_arabic_csv_is_decoded_and_the_delimiter_sniffed() {
        let text = "المرجع;الاسم;سعر البيع\r\nA1;زيت المائدة 5 لتر;1190,00\r\nA2;سكر أبيض 1 كغ;130,00\r\nA3;حليب مبستر;50,00\r\n";
        let (bytes, _, _) = encoding_rs::WINDOWS_1256.encode(text);
        let file = read_bytes(&bytes, "articles.csv").unwrap();
        assert_eq!(file.format, FileFormat::Csv);
        assert_eq!(file.encoding.as_deref(), Some("windows-1256"));
        assert_eq!(file.delimiter, Some(';'));
        assert_eq!(file.sheets[0].rows[0], vec!["المرجع", "الاسم", "سعر البيع"]);
        assert_eq!(
            file.sheets[0].rows[1],
            vec!["A1", "زيت المائدة 5 لتر", "1190,00"]
        );
    }

    #[test]
    fn a_french_windows_1252_csv_with_commas_and_quotes_is_read() {
        let text = "Référence,Désignation,Prix de vente\r\nC1,Café moulu été,\"1190,50\"\r\nC2,Thé à la menthe,\"250,00\"\r\n";
        let (bytes, _, _) = encoding_rs::WINDOWS_1252.encode(text);
        let file = read_bytes(&bytes, "produits.CSV").unwrap();
        assert_eq!(file.encoding.as_deref(), Some("windows-1252"));
        assert_eq!(file.delimiter, Some(','));
        assert_eq!(
            file.sheets[0].rows[1],
            vec!["C1", "Café moulu été", "1190,50"]
        );
    }

    #[test]
    fn utf8_with_a_bom_and_tabs_is_read() {
        let bytes = "\u{feff}Code\tNom\n001\tKarim\n".as_bytes().to_vec();
        let file = read_bytes(&bytes, "clients.txt").unwrap();
        assert_eq!(
            (file.encoding.as_deref(), file.delimiter),
            (Some("UTF-8"), Some('\t'))
        );
        assert_eq!(
            file.sheets[0].rows,
            vec![vec!["Code", "Nom"], vec!["001", "Karim"]]
        );
    }

    #[test]
    fn an_xlsx_file_gives_text_cells_numbers_without_noise_and_days() {
        let mut book = Workbook::new();
        let sheet = book.add_worksheet();
        let day = Format::new().set_num_format("dd/mm/yyyy");
        for (col, head) in ["Référence", "Prix", "Qté", "Date", "Taux"]
            .iter()
            .enumerate()
        {
            sheet.write_string(0, col as u16, *head).unwrap();
        }
        sheet.write_string(1, 0, "0012").unwrap();
        sheet.write_number(1, 1, 1190.5).unwrap();
        sheet.write_number(1, 2, 3).unwrap();
        sheet
            .write_datetime_with_format(1, 3, ExcelDateTime::from_ymd(2026, 9, 28).unwrap(), &day)
            .unwrap();
        sheet.write_number(1, 4, 0.1 + 0.2).unwrap();
        let bytes = book.save_to_buffer().unwrap();
        let file = read_bytes(&bytes, "stock.xlsx").unwrap();
        assert_eq!(file.format, FileFormat::Xlsx);
        assert_eq!(
            file.sheets[0].rows[1],
            vec!["0012", "1190.5", "3", "2026-09-28", "0.3"]
        );
    }

    #[test]
    fn excel_error_cells_keep_their_text_so_the_dry_run_can_refuse_them() {
        let mut book = Workbook::new();
        let sheet = book.add_worksheet();
        for (col, head) in ["Référence", "Prix", "Nom", "TVA"].iter().enumerate() {
            sheet.write_string(0, col as u16, *head).unwrap();
        }
        sheet.write_string(1, 0, "A1").unwrap();
        sheet
            .write_formula(
                1,
                1,
                Formula::new("=VLOOKUP(A2,Tarifs!A:B,2,FALSE)").set_result("#N/A"),
            )
            .unwrap();
        sheet
            .write_formula(1, 2, Formula::new("=Noms!Z0").set_result("#REF!"))
            .unwrap();
        sheet
            .write_formula(1, 3, Formula::new("=1/0").set_result("#DIV/0!"))
            .unwrap();
        let bytes = book.save_to_buffer().unwrap();
        let file = read_bytes(&bytes, "tarifs.xlsx").unwrap();
        let row = &file.sheets[0].rows[1];
        assert_eq!(row, &vec!["A1", "#N/A", "#REF!", "#DIV/0!"]);
        // Never an empty cell ("not provided"): the typed parsers refuse them with the value shown.
        assert_eq!(parse::money(&row[1]), Err("number_invalid"));
        assert_eq!(parse::percent_bp(&row[3]), Err("number_invalid"));
        assert!(parse::excel_error(&row[2]), "a text field refuses it too");
    }

    #[test]
    fn headers_are_found_and_mapped_in_any_language() {
        let rows = vec![
            vec!["Inventaire du 28/09".to_owned()],
            vec![],
            [
                "Désignation",
                "Prix de vente",
                "Code barre",
                "TVA",
                "المرجع",
                "Colonne libre",
            ]
            .map(str::to_owned)
            .to_vec(),
            ["Café", "250", "6130000000015", "19", "C1", "x"]
                .map(str::to_owned)
                .to_vec(),
        ];
        let sheet = RawSheet {
            name: "Feuil1".into(),
            rows,
        };
        let at = header_row(&sheet);
        assert_eq!(at, 2);
        let fields = base_fields(ImportEntity::Products);
        let mapped = suggest(&sheet.rows[at], &fields);
        assert_eq!(
            mapped,
            vec![
                Some("nameFr".into()),
                Some("sellPrice".into()),
                Some("barcode".into()),
                Some("taxRate".into()),
                Some("reference".into()),
                None
            ]
        );
    }

    #[test]
    fn empty_and_oversized_files_are_refused() {
        assert_eq!(
            read_bytes(&[], "vide.csv").unwrap_err().code(),
            "import_file_empty"
        );
        assert_eq!(
            check_size(MAX_FILE_BYTES + 1).unwrap_err().code(),
            "import_file_too_large"
        );
        assert_eq!(
            read_bytes(b"PK\x03\x04garbage", "casse.xlsx")
                .unwrap_err()
                .code(),
            "import_file_unreadable"
        );
        assert!(check_size(MAX_FILE_BYTES).is_ok());
    }

    /// Values far apart (A1 and XFD2000) would span 2 000 × 16 384 cells: refused as too large,
    /// never allocated. Values within the caps, blank rows and columns between them included, are
    /// read where the spreadsheet has them.
    #[test]
    fn a_sheet_whose_values_sit_far_apart_is_refused_not_allocated() {
        let mut book = Workbook::new();
        let sheet = book.add_worksheet();
        sheet.write_string(0, 0, "Référence").unwrap();
        sheet.write_string(1999, 16_383, "x").unwrap();
        let bytes = book.save_to_buffer().unwrap();
        assert_eq!(
            read_bytes(&bytes, "loin.xlsx").unwrap_err().code(),
            "import_file_too_large"
        );

        let mut book = Workbook::new();
        let sheet = book.add_worksheet();
        sheet.write_string(2, 1, "Référence").unwrap();
        sheet.write_string(2, 3, "Prix").unwrap();
        sheet.write_string(4, 1, "A1").unwrap();
        sheet.write_number(4, 3, 1190).unwrap();
        // A styled cell without a value takes no room.
        sheet
            .write_blank(60_000, 500, &Format::new().set_bold())
            .unwrap();
        let bytes = book.save_to_buffer().unwrap();
        let file = read_bytes(&bytes, "decale.xlsx").unwrap();
        assert_eq!(
            file.sheets[0].rows,
            vec![
                vec![],
                vec![],
                vec!["", "Référence", "", "Prix"],
                vec![],
                vec!["", "A1", "", "1190"],
            ]
        );
    }

    #[test]
    fn an_xls_or_ods_grid_beyond_the_caps_is_refused_before_it_is_copied() {
        let far = Range::from_sparse(vec![
            calamine::Cell::new((0, 0), Data::String("Référence".into())),
            calamine::Cell::new((1_999, 16_383), Data::String("x".into())),
        ]);
        assert_eq!(
            range_sheet(&far).unwrap_err().code(),
            "import_file_too_large"
        );
        let near = Range::from_sparse(vec![
            calamine::Cell::new((1, 1), Data::String("Code".into())),
            calamine::Cell::new((2, 2), Data::Int(7)),
        ]);
        assert_eq!(
            range_sheet(&near).unwrap(),
            vec![vec![], vec!["", "Code"], vec!["", "", "7"]]
        );
    }

    #[test]
    fn a_template_reads_back_into_its_own_mapping() {
        for lang in Lang::ALL {
            let fields = base_fields(ImportEntity::Products);
            let bytes = template(ImportEntity::Products, &fields, lang, &BTreeMap::new()).unwrap();
            let file = read_bytes(&bytes, "modele.xlsx").unwrap();
            assert_eq!(file.sheets.len(), 2, "data and help sheets");
            let at = header_row(&file.sheets[0]);
            let mapped = suggest(&file.sheets[0].rows[at], &fields);
            let keys: Vec<Option<String>> = fields.iter().map(|f| Some(f.key.clone())).collect();
            assert_eq!(mapped, keys, "{}", lang.as_str());
        }
    }

    #[test]
    fn the_error_report_names_row_column_value_and_problem() {
        let fields = base_fields(ImportEntity::Products);
        let problems = vec![Problem {
            row: 3,
            field: Some("sellPrice".into()),
            value: Some("1.234,5".into()),
            code: "ambiguous_separator".into(),
        }];
        let bytes = error_report(&problems, &fields, Lang::Fr, &BTreeMap::new()).unwrap();
        let file = read_bytes(&bytes, "erreurs.xlsx").unwrap();
        let rows = &file.sheets[0].rows;
        assert_eq!(rows[0], vec!["Ligne", "Colonne", "Valeur", "Problème"]);
        assert_eq!(
            rows[1][..3],
            [
                "3".to_owned(),
                "Prix de vente".to_owned(),
                "1.234,5".to_owned()
            ]
        );
        assert!(rows[1][3].contains("milliers"), "{}", rows[1][3]);
    }
}

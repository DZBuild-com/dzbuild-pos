//! Data-collector batch files (STK-012, research 05 §3.1): one `code;quantity` per line from the
//! handheld terminals shops count with (POSLUX DT-92 class). The rows go to a stock count through
//! M2's `counts::import_lines`; bad lines are reported, never guessed.

use crate::error::HwError;
use crate::scale::protocol::parse_weight;
use pos_core::money::{QTY_SCALE, Qty};
use serde::Serialize;
use specta::Type;

pub const MAX_FILE_BYTES: usize = 5 * 1024 * 1024;
/// Non-blank lines read, good or bad (the header aside): a bigger file is refused whole.
pub const MAX_ROWS: usize = 100_000;
const MAX_UNITS: i64 = 1_000_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CollectorRow {
    pub code: String,
    pub qty: Qty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CollectorProblem {
    /// 1-based line number in the file.
    pub line: u32,
    /// `code_invalid` or `qty_invalid` (translated under `stock:collector.problems.*`).
    pub code: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CollectorFile {
    pub rows: Vec<CollectorRow>,
    pub problems: Vec<CollectorProblem>,
}

fn clean_code(raw: &str) -> Option<String> {
    let code = raw.trim().trim_matches('"').trim();
    let ok = (1..=64).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_graphic());
    ok.then(|| code.to_owned())
}

fn quantity(raw: &str) -> Option<Qty> {
    let raw = raw.trim().trim_matches('"').trim();
    if raw.is_empty() {
        return Some(Qty(QTY_SCALE));
    }
    let decimals = raw.find([',', '.']).map_or(0, |i| raw.len() - i - 1);
    if decimals > 3 || raw.starts_with(['-', '+']) {
        return None;
    }
    parse_weight(raw, None).filter(|q| q.0 > 0 && q.0 <= MAX_UNITS * QTY_SCALE)
}

fn split(line: &str) -> (&str, &str) {
    for separator in [';', '\t', ','] {
        if let Some((code, qty)) = line.split_once(separator) {
            return (code, qty);
        }
    }
    let line = line.trim();
    match line.split_once(char::is_whitespace) {
        Some((code, qty)) => (code, qty),
        None => (line, ""),
    }
}

pub fn parse(bytes: &[u8]) -> Result<CollectorFile, HwError> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(HwError::Collector("the file is larger than 5 MB".into()));
    }
    let text = String::from_utf8_lossy(bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut file = CollectorFile::default();
    for (index, line) in text.lines().enumerate() {
        let number = u32::try_from(index + 1).unwrap_or(u32::MAX);
        if line.trim().is_empty() {
            continue;
        }
        let (raw_code, raw_qty) = split(line);
        let code = clean_code(raw_code);
        let qty = quantity(raw_qty);
        let header = index == 0
            && qty.is_none()
            && !raw_code
                .trim()
                .trim_matches('"')
                .bytes()
                .all(|b| b.is_ascii_digit());
        match (code, qty) {
            _ if header => {}
            (Some(code), Some(qty)) => file.rows.push(CollectorRow { code, qty }),
            (None, _) => file.problems.push(CollectorProblem {
                line: number,
                code: "code_invalid".into(),
            }),
            (Some(_), None) => file.problems.push(CollectorProblem {
                line: number,
                code: "qty_invalid".into(),
            }),
        }
        // bad lines count too: a garbage file never grows an endless problem list for the UI
        if file.rows.len() + file.problems.len() > MAX_ROWS {
            return Err(HwError::Collector("more than 100 000 lines".into()));
        }
    }
    if file.rows.is_empty() && file.problems.is_empty() {
        return Err(HwError::Collector("no line to import".into()));
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_core::money::Qty;

    fn rows(file: &CollectorFile) -> Vec<(&str, i64)> {
        file.rows
            .iter()
            .map(|r| (r.code.as_str(), r.qty.0))
            .collect()
    }

    #[test]
    fn every_common_layout_is_read() {
        let text = "\u{feff}Code;Quantité\r\n6130123456782;12\r\n96385074\t3\r\nP000123,2,5\r\n036000291452 7\r\n6130123456782\r\n\"2000000000008\";1.25\r\n";
        let file = parse(text.as_bytes()).unwrap();
        assert_eq!(
            rows(&file),
            [
                ("6130123456782", 12_000),
                ("96385074", 3_000),
                ("P000123", 2_500),
                ("036000291452", 7_000),
                ("6130123456782", 1_000),
                ("2000000000008", 1_250),
            ]
        );
        assert!(file.problems.is_empty());
    }

    #[test]
    fn bad_lines_are_reported_and_the_rest_is_kept() {
        let file =
            parse(b"6130123456782;-3\r\n61 30;2\r\nabc;x\r\n96385074;1,2345\r\n96385074;4\r\n")
                .unwrap();
        assert_eq!(rows(&file), [("96385074", 4_000)]);
        let problems: Vec<(u32, &str)> = file
            .problems
            .iter()
            .map(|p| (p.line, p.code.as_str()))
            .collect();
        assert_eq!(
            problems,
            [
                (1, "qty_invalid"),
                (2, "code_invalid"),
                (3, "qty_invalid"),
                (4, "qty_invalid")
            ]
        );
        assert_eq!(parse(b"").unwrap_err().code(), "collector_file_invalid");
        assert_eq!(
            parse(&vec![b'1'; MAX_FILE_BYTES + 1]).unwrap_err().code(),
            "collector_file_invalid"
        );
        assert_eq!(Qty(4_000), file.rows[0].qty);
    }

    #[test]
    fn a_file_of_more_than_100_000_lines_is_refused_bad_lines_included() {
        let full = "6130123456782;1\n".repeat(MAX_ROWS / 2) + &"61 30;2\n".repeat(MAX_ROWS / 2);
        let file = parse(full.as_bytes()).unwrap();
        assert_eq!(
            (file.rows.len(), file.problems.len()),
            (MAX_ROWS / 2, MAX_ROWS / 2)
        );
        let code = |text: String| parse(text.as_bytes()).err().map(|e| e.code());
        assert_eq!(
            code("6130123456782;1\n".repeat(MAX_ROWS + 1)),
            Some("collector_file_invalid")
        );
        assert_eq!(
            code("61 30;2\n".repeat(MAX_ROWS + 1)),
            Some("collector_file_invalid"),
            "a file of bad lines never grows an endless problem list"
        );
    }
}

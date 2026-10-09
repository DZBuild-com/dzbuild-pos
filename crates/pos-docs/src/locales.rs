//! Rust-side document words (receipt labels, Z report, invoice mentions), spec §11.
//! Checked for ar/fr/en parity by `scripts/i18n/check-i18n.mjs` and by the tests below.

use serde_json::Value;
use std::collections::BTreeSet;

pub const CATALOGUES: [(&str, &str); 3] = [
    ("ar", include_str!("../locales/ar.json")),
    ("fr", include_str!("../locales/fr.json")),
    ("en", include_str!("../locales/en.json")),
];

fn catalogue(lang: &str) -> Option<Value> {
    let (_, json) = CATALOGUES.iter().find(|(l, _)| *l == lang)?;
    serde_json::from_str(json).ok()
}

/// Dotted-key lookup, e.g. `text("fr", "doc.ticket")`.
pub fn text(lang: &str, key: &str) -> Option<String> {
    let mut node = catalogue(lang)?;
    for part in key.split('.') {
        node = node.get(part)?.clone();
    }
    node.as_str().map(str::to_owned)
}

fn collect(prefix: &str, value: &Value, out: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                collect(&key, v, out);
            }
        }
        _ => {
            out.insert(prefix.to_owned());
        }
    }
}

/// Every dotted key of one catalogue.
pub fn keys(lang: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    if let Some(value) = catalogue(lang) {
        collect("", &value, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_catalogues_have_the_same_keys() {
        let reference = keys("fr");
        assert!(!reference.is_empty());
        for (lang, _) in CATALOGUES {
            assert_eq!(keys(lang), reference, "{lang}.json differs from fr.json");
        }
    }

    #[test]
    fn lookups_return_the_document_words() {
        assert_eq!(
            text("fr", "doc.ticket").as_deref(),
            Some("Ticket de caisse")
        );
        assert_eq!(text("ar", "doc.invoice").as_deref(), Some("فاتورة"));
        assert_eq!(text("en", "doc.creditNote").as_deref(), Some("Credit note"));
        assert_eq!(text("de", "doc.ticket"), None);
    }
}

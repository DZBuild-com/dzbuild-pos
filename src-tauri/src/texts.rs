//! The few words Rust shows without the webview (the tray, the native fatal box), read from the
//! same catalogues as the UI (`src/locales/<lang>/common.json`), so they stay in the three
//! languages and in the parity check.

use pos_core::lang::Lang;
use serde_json::Value;
use std::sync::LazyLock;

static CATALOGUES: LazyLock<[(Lang, Value); 3]> = LazyLock::new(|| {
    let parse = |text: &str| serde_json::from_str(text).unwrap_or(Value::Null);
    [
        (
            Lang::Ar,
            parse(include_str!("../../src/locales/ar/common.json")),
        ),
        (
            Lang::Fr,
            parse(include_str!("../../src/locales/fr/common.json")),
        ),
        (
            Lang::En,
            parse(include_str!("../../src/locales/en/common.json")),
        ),
    ]
});

/// `common:<path>` in `lang`.
pub fn catalogue_text(lang: Lang, path: &[&str]) -> Option<String> {
    let (_, catalogue) = CATALOGUES.iter().find(|(l, _)| *l == lang)?;
    let mut node = catalogue;
    for part in path {
        node = node.get(*part)?;
    }
    node.as_str().map(str::to_owned)
}

/// Replaces `{{name}}` by its value (i18next interpolation).
pub fn fill(template: &str, values: &[(&str, &str)]) -> String {
    values
        .iter()
        .fold(template.to_owned(), |text, (name, value)| {
            text.replace(&format!("{{{{{name}}}}}"), value)
        })
}

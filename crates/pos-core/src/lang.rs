//! The three first-class languages (spec §1, §11).

use crate::sqlenum::sql_text_enum;
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    Ar,
    #[default]
    Fr,
    En,
}

impl Lang {
    pub const ALL: [Lang; 3] = [Lang::Ar, Lang::Fr, Lang::En];

    pub const fn as_str(self) -> &'static str {
        match self {
            Lang::Ar => "ar",
            Lang::Fr => "fr",
            Lang::En => "en",
        }
    }

    pub fn parse(s: &str) -> Option<Lang> {
        Lang::ALL.into_iter().find(|l| l.as_str() == s)
    }

    pub const fn is_rtl(self) -> bool {
        matches!(self, Lang::Ar)
    }
}

sql_text_enum!(Lang);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_serde_and_direction() {
        for lang in Lang::ALL {
            assert_eq!(Lang::parse(lang.as_str()), Some(lang));
            assert_eq!(
                serde_json::to_string(&lang).unwrap(),
                format!("\"{}\"", lang.as_str())
            );
        }
        assert_eq!(Lang::parse("de"), None);
        assert!(Lang::Ar.is_rtl() && !Lang::Fr.is_rtl());
        assert_eq!(Lang::default(), Lang::Fr);
    }

    #[test]
    fn sqlite_roundtrip_and_rejection() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let lang: Lang = conn
            .query_row("SELECT ?1", [Lang::Ar], |r| r.get(0))
            .unwrap();
        assert_eq!(lang, Lang::Ar);
        let bad = conn.query_row("SELECT 'xx'", [], |r| r.get::<_, Lang>(0));
        assert!(bad.is_err());
    }

    #[test]
    fn codes_are_exact_lower_case() {
        assert_eq!(
            Lang::ALL.map(Lang::as_str),
            ["ar", "fr", "en"],
            "codes are the i18next language keys"
        );
        assert!(!Lang::En.is_rtl());
        assert_eq!(Lang::parse("AR"), None);
        assert_eq!(serde_json::from_str::<Lang>("\"en\"").unwrap(), Lang::En);
        assert!(serde_json::from_str::<Lang>("\"EN\"").is_err());
        assert!(serde_json::from_str::<Lang>("\"de\"").is_err());
    }

    #[test]
    fn stored_as_text_and_non_text_is_rejected() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let (kind, text): (String, String) = conn
            .query_row("SELECT typeof(?1), ?1", [Lang::En], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((kind.as_str(), text.as_str()), ("text", "en"));
        assert!(
            conn.query_row("SELECT 1", [], |r| r.get::<_, Lang>(0))
                .is_err()
        );
        let bad = conn
            .query_row("SELECT 'xx'", [], |r| r.get::<_, Lang>(0))
            .unwrap_err();
        assert!(bad.to_string().contains("invalid Lang value: xx"), "{bad}");
    }
}

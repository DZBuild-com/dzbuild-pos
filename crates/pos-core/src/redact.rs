//! Personal data out of any text that leaves the app (the diagnostic zip) or reaches a log file
//! (PLT-018, loi 18-07): what looks personal whatever the shop (`mask_personal`), and what this
//! shop's own customers and suppliers are called and where they live (`DenyList`).

use crate::error::AppError;
use aho_corasick::{AhoCorasick, MatchKind};
use regex::Regex;
use rusqlite::Connection;
use std::sync::LazyLock;

pub const PHONE: &str = "<phone>";
pub const EMAIL: &str = "<email>";
pub const SECRET: &str = "<secret>";
pub const ID: &str = "<id>";
pub const PERSONAL: &str = "<personal>";

struct Rules {
    /// `code` fields (an OAuth authorization code, a LAN pairing code), masked unless the value
    /// is an error code or a till code ([`keeps_code`]).
    codes: Option<Regex>,
    ordered: Vec<(Regex, &'static str)>,
}

fn compiled(pattern: &str) -> Option<Regex> {
    match Regex::new(pattern) {
        Ok(re) => Some(re),
        Err(error) => {
            tracing::error!(%error, pattern, "a redaction pattern does not compile");
            None
        }
    }
}

/// An error code (`backup_invalid`: lowercase words, no digit, unlike a hex code) or a till code
/// (`C2`): what support reads, never a secret.
fn keeps_code(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return true;
    };
    let rest: Vec<char> = chars.collect();
    let error_code =
        first.is_ascii_lowercase() && rest.iter().all(|c| c.is_ascii_lowercase() || *c == '_');
    let till_code = first.is_ascii_uppercase()
        && rest.len() < 8
        && rest
            .iter()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
    error_code || till_code
}

static RULES: LazyLock<Rules> = LazyLock::new(|| {
    // A key's quotes may come escaped (`\"refresh_token\":\"…\"` in a JSON body that a log line
    // quotes): `\\?"?` around the key and the value, and the value stops at a backslash.
    let table: [(&str, &'static str); 10] = [
        // M9: a licence key never reaches a log; if one ever did, it is masked here too.
        (
            r"\bDZL1\.[A-Za-z0-9_-]+(?:\.[A-Za-z0-9_-]+)?",
            "DZL1.<secret>",
        ),
        (r#"(?i)\$argon2[a-z]*\$[^\s"'}]*"#, SECRET),
        (r"(?i)\bbearer\s+[A-Za-z0-9._~+/=-]+", "Bearer <secret>"),
        (
            r#"(?i)("?\b(?:access_token|refresh_token|id_token|client_secret|code_verifier|password|pin|token|device_code|user_code|api_key|secret)\b\\?"?\s*[:=]\s*\\?"?)[^"\s&,}\\]+"#,
            "${1}<secret>",
        ),
        // The DZBuild link's logs never carry a query (`cloud::http`): the OAuth redirect's
        // `code` and `state` included.
        (
            r#"(?i)(\bhttps?://[^\s?#"'<>\\]+)\?[^\s"'#\\]+"#,
            "${1}?<secret>",
        ),
        (r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}", EMAIL),
        (
            r"(?:\+|\b00)213[\s.-]?(?:[5-7](?:[\s.-]?\d){8}|[1-49](?:[\s.-]?\d){7})\b",
            PHONE,
        ),
        (r"\b0[5-7](?:[\s.-]?\d){8}\b", PHONE),
        (r"\b0[1-49](?:[\s.-]?\d){7}\b", PHONE),
        (r"\b\d{15,}\b", ID),
    ];
    let ordered = table
        .into_iter()
        .filter_map(|(pattern, replacement)| Some((compiled(pattern)?, replacement)))
        .collect();
    Rules {
        codes: compiled(
            r#"("?\b(?i:code|pairing_code|auth_code|authorization_code)\b\\?"?\s*[:=]\s*\\?"?)([^"\s&,})\\]+)"#,
        ),
        ordered,
    }
});

/// Masks what looks personal in any shop: secrets first (codes — an OAuth authorization code,
/// a LAN pairing code —, PIN hashes, bearer tokens, token, PIN and password fields, URL queries),
/// then e-mails, Algerian phone numbers (international, mobile, landline, with spaces, dots or
/// dashes), then runs of 15 digits or more (NIN, NIF, RIB). Barcodes (8–13 digits), prices,
/// dates, times, UUIDs, error codes and till codes are left alone.
pub fn mask_personal(text: &str) -> String {
    let mut out = text.to_owned();
    if let Some(codes) = &RULES.codes
        && codes.is_match(&out)
    {
        out = codes
            .replace_all(&out, |caps: &regex::Captures<'_>| {
                if keeps_code(&caps[2]) {
                    caps[0].to_owned()
                } else {
                    format!("{}{SECRET}", &caps[1])
                }
            })
            .into_owned();
    }
    for (re, replacement) in &RULES.ordered {
        if re.is_match(&out) {
            out = re.replace_all(&out, *replacement).into_owned();
        }
    }
    out
}

/// The identities of this shop's customers and suppliers, matched ASCII-case-insensitively
/// (Arabic has no case) wherever they appear in a text.
pub struct DenyList {
    matcher: Option<AhoCorasick>,
}

/// The identities [`DenyList::from_db`] masks, read from `conn`.
fn identities(conn: &Connection) -> Result<Vec<String>, AppError> {
    const SOURCES: [(&str, &str); 2] = [
        (
            "customers",
            "SELECT name, phone, phone2, email, address, nif, nis, rc, ai, nin FROM customers",
        ),
        (
            "suppliers",
            "SELECT name, phone, phone2, email, address, nif, nis, rc, ai, nin, rib FROM suppliers",
        ),
    ];
    let mut values: Vec<String> = Vec::new();
    for (table, sql) in SOURCES {
        let exists: bool = conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [table],
            |r| r.get(0),
        )?;
        if !exists {
            continue;
        }
        let mut stmt = conn.prepare(sql)?;
        let columns = stmt.column_count();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            for index in 0..columns {
                if let Some(value) = row.get::<_, Option<String>>(index)? {
                    let value = value.trim().to_owned();
                    let digits: String = value.chars().filter(char::is_ascii_digit).collect();
                    if digits.len() >= 9 && digits.len() != value.len() {
                        values.push(digits);
                    }
                    if value.chars().count() >= 4 {
                        values.push(value);
                    }
                }
            }
        }
    }
    Ok(values)
}

impl DenyList {
    pub fn empty() -> DenyList {
        DenyList { matcher: None }
    }

    /// Names, phones (as typed and digits only), e-mails, addresses and identifiers of every
    /// customer and supplier, archived ones included; values shorter than 4 characters are
    /// skipped (they would mask ordinary words).
    pub fn from_db(conn: &Connection) -> Result<DenyList, AppError> {
        DenyList::from_dbs(&[conn])
    }

    /// [`DenyList::from_db`] over several databases: the demo shop's diagnostic masks the real
    /// shop's customers too, since both write to this PC's logs (finding 32).
    pub fn from_dbs(conns: &[&Connection]) -> Result<DenyList, AppError> {
        let mut values: Vec<String> = Vec::new();
        for conn in conns {
            values.extend(identities(conn)?);
        }
        values.sort();
        values.dedup();
        if values.is_empty() {
            return Ok(DenyList::empty());
        }
        let matcher = AhoCorasick::builder()
            .ascii_case_insensitive(true)
            .match_kind(MatchKind::LeftmostLongest)
            .build(&values)
            .map_err(|e| AppError::Internal(format!("deny list: {e}")))?;
        Ok(DenyList {
            matcher: Some(matcher),
        })
    }

    /// [`DenyList::from_dbs`] of `conn` and the database file at `other`, opened read-only.
    pub fn from_db_and_file(
        conn: &Connection,
        other: &std::path::Path,
    ) -> Result<DenyList, AppError> {
        let other = Connection::open_with_flags(other, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        DenyList::from_dbs(&[conn, &other])
    }

    pub fn mask(&self, text: &str) -> String {
        let Some(matcher) = &self.matcher else {
            return text.to_owned();
        };
        let mut out = String::with_capacity(text.len());
        matcher.replace_all_with(text, &mut out, |_, _, dst| {
            dst.push_str(PERSONAL);
            true
        });
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::Actor;
    use crate::customers::{self, CustomerInput};
    use crate::testkit::shop;

    #[test]
    fn phones_in_every_usual_shape_are_masked() {
        for phone in [
            "0550123456",
            "0550 12 34 56",
            "05 50 12 34 56",
            "0770-12-34-56",
            "0661.12.34.56",
            "+213 550 12 34 56",
            "00213550123456",
            "021 23 45 67",
        ] {
            let masked = mask_personal(&format!("client tel {phone} ok"));
            assert_eq!(masked, format!("client tel {PHONE} ok"), "{phone}");
        }
    }

    #[test]
    fn barcodes_prices_dates_and_ids_of_the_app_are_kept() {
        for kept in [
            "6130001234567",
            "0612345678905",
            "2000000000015",
            "prix 4 500,00 DA",
            "2026-09-28 12:00:00",
            "00000000-0000-7000-8000-000000000502",
            "1790593200000",
            "qty 3",
        ] {
            assert_eq!(mask_personal(kept), kept);
        }
        assert_eq!(mask_personal("NIN 109870123456789012"), format!("NIN {ID}"));
        assert_eq!(mask_personal("qty 3 0550123456"), format!("qty 3 {PHONE}"));
    }

    #[test]
    fn a_licence_key_is_masked() {
        let line = "activated DZL1.eyJ2IjoxfQ.c2lnbmF0dXJlX2J5dGVz with the till";
        assert_eq!(mask_personal(line), "activated DZL1.<secret> with the till");
    }

    #[test]
    fn secrets_and_emails_are_masked() {
        let line = r#"login pin=1234 hash $argon2id$v=19$m=19456,t=2,p=1$abc$def mail karim.b@example.dz Authorization: Bearer eyJhbGci.abc-def {"refresh_token":"rt-99","access_token": "at-1"}"#;
        let masked = mask_personal(line);
        for leaked in [
            "1234",
            "$argon2",
            "karim.b@example.dz",
            "eyJhbGci",
            "rt-99",
            "at-1",
        ] {
            assert!(!masked.contains(leaked), "{leaked} in {masked}");
        }
        assert!(
            masked.contains(EMAIL) && masked.contains("Bearer <secret>"),
            "{masked}"
        );
    }

    /// The DZBuild link's token material (M7: authorization and device codes, tokens, the
    /// redirect's query) and the LAN's pairing codes and till tokens (M5) are masked wherever a
    /// log line or a debug print carries them, quotes escaped or not; the error codes and the
    /// till codes support reads stay.
    #[test]
    fn link_tokens_and_pairing_codes_are_masked_but_error_codes_are_kept() {
        let token = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
        let lines = [
            r#"request=JoinRequest { address: "192.168.1.20:7345", code: "482913", terminal_code: "C2" }"#.to_owned(),
            "pairing refused code=482913".to_owned(),
            format!("till token={token}"),
            format!(r#"Paired {{ terminal: C2, token: "{token}" }}"#),
            r#"DeviceAuthorization { user_code: "WDJB-MJHT", device_code: "GmRhmhcxhwAzkoEqiMEg_DnyEysNkuNhszIySk9eS" }"#.to_owned(),
            "redirect http://127.0.0.1:53682/callback?code=SplxlOBeZQQYbYS6WxSbIA&state=xyzABC123 received".to_owned(),
            r#"body="{\"refresh_token\":\"rt_SECRET_REFRESH\",\"access_token\":\"at_SECRET\",\"code\":\"AC_SECRET\"}""#.to_owned(),
        ];
        for line in &lines {
            let masked = mask_personal(line);
            for leaked in [
                "482913",
                "9f86d081",
                "WDJB-MJHT",
                "GmRhmhcx",
                "SplxlOBe",
                "xyzABC123",
                "rt_SECRET",
                "at_SECRET",
                "AC_SECRET",
            ] {
                assert!(!masked.contains(leaked), "{leaked} in {masked}");
            }
        }
        for kept in [
            "sync stopped code=backup_invalid",
            r#"code="pairing_code_invalid""#,
            r#"Terminal { code: "C2", name: "Caisse 2" }"#,
        ] {
            assert_eq!(mask_personal(kept), kept);
        }
    }

    #[test]
    fn the_deny_list_hides_this_shops_customers_and_suppliers() {
        let shop = shop();
        let pack = shop.pack.clone();
        shop.db
            .write(move |tx| {
                let mut input = CustomerInput::named("Karim Benali");
                input.address = Some("Cité 500 logements, Bab Ezzouar".into());
                input.nif = Some("000116009876543".into());
                customers::create(tx, &Actor::system(None), &pack, input)?;
                Ok(())
            })
            .unwrap();
        shop.supplier("Grossiste Boualem");
        let deny = shop.db.read(DenyList::from_db).unwrap();
        let text = "sale for KARIM BENALI, delivery to cité 500 logements, bab ezzouar; supplier Grossiste Boualem";
        let masked = deny.mask(text);
        assert_eq!(
            masked,
            format!("sale for {PERSONAL}, delivery to {PERSONAL}; supplier {PERSONAL}")
        );
        assert_eq!(DenyList::empty().mask(text), text);
    }

    /// Finding 32: the demo's diagnostic masks the real shop's customers as well (both write to
    /// this PC's logs), read from the real database's file.
    #[test]
    fn the_deny_list_of_two_databases_masks_both_shops() {
        let dir = tempfile::tempdir().unwrap();
        let real_path = dir.path().join("pos.db");
        let real = crate::db::Db::open(&real_path, crate::db::DbOptions::default()).unwrap();
        crate::setup::bootstrap(&real).unwrap();
        let pack = std::sync::Arc::new(crate::datapack::bundled().unwrap());
        real.write(move |tx| {
            customers::create(
                tx,
                &Actor::system(None),
                &pack,
                CustomerInput::named("Samia Haddad"),
            )
            .map(drop)
        })
        .unwrap();
        real.mark_clean_shutdown().unwrap();
        drop(real);
        let demo = shop();
        demo.supplier("Grossiste Boualem");
        let deny = demo
            .db
            .read(move |c| DenyList::from_db_and_file(c, &real_path))
            .unwrap();
        assert_eq!(
            deny.mask("Samia Haddad bought from Grossiste Boualem"),
            format!("{PERSONAL} bought from {PERSONAL}")
        );
    }
}

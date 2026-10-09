//! The licence key as text: `DZL1.<base64url payload>.<base64url signature>` (no padding) on one
//! line, or a `.dzlic` file holding it (lines starting with `#` are comments).

use crate::codes;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use pos_core::error::AppError;

pub const PREFIX: &str = "DZL1.";
/// A pasted key longer than this is not a key (a v1 key is about 500 characters).
pub const MAX_KEY_LEN: usize = 4_096;
/// A `.dzlic` file is a few hundred bytes; a larger file is refused unread.
pub const MAX_FILE_LEN: u64 = 8 * 1024;

/// The two decoded parts of a key.
pub struct Envelope {
    pub payload: Vec<u8>,
    pub signature: [u8; 64],
}

pub fn encode(payload: &[u8], signature: &[u8; 64]) -> String {
    format!(
        "{PREFIX}{}.{}",
        URL_SAFE_NO_PAD.encode(payload),
        URL_SAFE_NO_PAD.encode(signature)
    )
}

/// The key inside `text`, whatever a chat or a mail did to it: `#` comment lines, line breaks,
/// spaces, quotes and guillemets, and the invisible marks an Arabic keyboard or phone adds around
/// pasted Latin text (direction marks and isolates, zero-width spaces and joiners, the Arabic
/// letter mark, the byte-order mark, the soft hyphen), removed.
pub fn normalise(text: &str) -> String {
    text.lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .flat_map(|line| line.chars())
        .filter(|c| {
            !c.is_whitespace()
                && !matches!(
                    c,
                    '"' | '\'' | '`' | '«' | '»' | '“' | '”' | '\u{200b}'..='\u{200f}'
                        | '\u{202a}'..='\u{202e}' | '\u{2060}' | '\u{2066}'..='\u{2069}'
                        | '\u{061c}' | '\u{feff}' | '\u{00ad}'
                )
        })
        .collect()
}

/// A signature is 64 bytes: 86 base64url symbols without padding.
const SIGNATURE_LEN: usize = 86;

/// Splits a key into its payload bytes and signature; anything else is `license_invalid`.
pub fn decode(text: &str) -> Result<Envelope, AppError> {
    let invalid = || AppError::conflict(codes::INVALID);
    if text.len() > MAX_KEY_LEN {
        return Err(invalid());
    }
    let compact = normalise(text);
    // The key may sit inside a message ("Voici votre clé : DZL1.… Bonne journée"): it starts at
    // the last `DZL1.` and its signature has a fixed length, so the words around it fall away.
    let start = compact.rfind(PREFIX).ok_or_else(invalid)?;
    let rest = &compact[start + PREFIX.len()..];
    let (payload, signature) = rest.split_once('.').ok_or_else(invalid)?;
    let signature = signature.get(..SIGNATURE_LEN).ok_or_else(invalid)?;
    let payload = URL_SAFE_NO_PAD.decode(payload).map_err(|_| invalid())?;
    let signature: [u8; 64] = URL_SAFE_NO_PAD
        .decode(signature)
        .map_err(|_| invalid())?
        .try_into()
        .map_err(|_| invalid())?;
    Ok(Envelope { payload, signature })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_then_decode_round_trips() {
        let text = encode(b"{\"v\":1}", &[9_u8; 64]);
        assert!(text.starts_with(PREFIX) && !text.contains('='), "{text}");
        let envelope = decode(&text).unwrap();
        assert_eq!(envelope.payload, b"{\"v\":1}");
        assert_eq!(envelope.signature, [9_u8; 64]);
    }

    #[test]
    fn what_is_not_a_key_is_invalid() {
        let good = encode(b"{}", &[1_u8; 64]);
        let short = encode(b"{}", &[1_u8; 64]).replace(
            &URL_SAFE_NO_PAD.encode([1_u8; 64]),
            &URL_SAFE_NO_PAD.encode([1_u8; 63]),
        );
        let padded = format!(
            "{PREFIX}{}.{}",
            base64::engine::general_purpose::URL_SAFE.encode(b"{}"),
            URL_SAFE_NO_PAD.encode([1_u8; 64])
        );
        let long = format!("{good}{}", "A".repeat(MAX_KEY_LEN));
        for junk in [
            "",
            "DZL1.",
            "DZL1.abc",
            "DZL2.e30.AAAA",
            "e30.AAAA",
            short.as_str(),
            padded.as_str(),
            long.as_str(),
        ] {
            assert_eq!(
                decode(junk).err().map(|e| e.code()).as_deref(),
                Some(crate::codes::INVALID),
                "{junk:.40}"
            );
        }
    }

    #[test]
    fn normalise_keeps_only_the_key() {
        let pasted = "# DZBuild POS licence\n\u{200f}« DZL1.ab\ncd .ef\u{2069} »\r\n\"\n";
        assert_eq!(normalise(pasted), "DZL1.abcd.ef");
    }
}

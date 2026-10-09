//! Checking a key offline: its shape, its Ed25519 signature (strict) against the key its `key_id`
//! names, the canonical form of the signed bytes, the v1 rules, the revocation list and this PC's
//! device code. Dates are the caller's (against the high-water, `state.rs`).

use crate::codes;
use crate::key;
use crate::keys::{Keyring, REVOKED};
use crate::payload::{Payload, SIGNING_CONTEXT};
use ed25519_dalek::Signature;
use pos_core::error::AppError;

/// A key that verified, with its canonical text (what is stored). `Debug` prints no key.
#[derive(Clone, PartialEq, Eq)]
pub struct Verified {
    pub payload: Payload,
    pub key: String,
}

impl std::fmt::Debug for Verified {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Verified")
            .field("payload", &self.payload)
            .finish_non_exhaustive()
    }
}

/// [`verify_with`] the revocation list of this release.
pub fn verify(text: &str, keyring: &Keyring, device_code: &str) -> Result<Verified, AppError> {
    verify_with(text, keyring, device_code, REVOKED)
}

pub fn verify_with(
    text: &str,
    keyring: &Keyring,
    device_code: &str,
    revoked: &[&str],
) -> Result<Verified, AppError> {
    let invalid = || AppError::conflict(codes::INVALID);
    let envelope = key::decode(text)?;
    // Read before the signature is checked only to pick the key; nothing else is trusted yet.
    let payload: Payload = serde_json::from_slice(&envelope.payload).map_err(|_| invalid())?;
    let public = keyring.get(&payload.key_id).ok_or_else(invalid)?;
    let mut message = SIGNING_CONTEXT.to_vec();
    message.extend_from_slice(&envelope.payload);
    public
        .verify_strict(&message, &Signature::from_bytes(&envelope.signature))
        .map_err(|_| invalid())?;
    // One encoding per payload: the signed bytes are the canonical ones.
    if payload.canonical_bytes()? != envelope.payload {
        return Err(invalid());
    }
    payload.validate()?;
    if revoked.contains(&payload.id.as_str()) {
        return Err(AppError::conflict(codes::REVOKED));
    }
    if payload.device_code != device_code {
        return Err(AppError::conflict(codes::WRONG_DEVICE));
    }
    Ok(Verified {
        key: key::encode(&envelope.payload, &envelope.signature),
        payload,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codes::{INVALID, REVOKED, WRONG_DEVICE};
    use crate::key::{self, PREFIX};
    use crate::payload::{DAY_MS, LicenseKind, Payload};
    use crate::testkit;
    use ed25519_dalek::{Signer, SigningKey};
    use pos_core::time::now_ms;

    fn code(result: Result<Verified, AppError>) -> String {
        result.err().map(|e| e.code()).unwrap_or_default()
    }

    #[test]
    fn a_signed_key_verifies_and_round_trips_through_its_text() {
        let device = testkit::device();
        let now = now_ms();
        let text = testkit::trial(&device, now);
        assert!(text.starts_with(PREFIX));
        let verified = verify(&text, &testkit::keyring(), device.code()).unwrap();
        assert_eq!(verified.key, text);
        assert_eq!(verified.payload.kind, LicenseKind::Trial);
        assert_eq!(verified.payload.expires_at, Some(now + 7 * DAY_MS));
        assert!(!format!("{verified:?}").contains(&text[PREFIX.len()..PREFIX.len() + 20]));
    }

    #[test]
    fn a_key_survives_whatsapp_line_breaks_quotes_and_bidi_marks() {
        let device = testkit::device();
        let text = testkit::lifetime(&device, now_ms());
        let (a, rest) = text.split_at(40);
        let (b, c) = rest.split_at(120);
        let pasted = format!("\u{200f}« {a}\n{b} \r\n  {c}\u{2069} »\n");
        let from_chat = verify(&pasted, &testkit::keyring(), device.code()).unwrap();
        assert_eq!(
            from_chat.key, text,
            "the canonical text is kept, not the pasted one"
        );
        // final review: zero-width spaces, the Arabic letter mark, a soft hyphen, the key inside
        // the seller's message
        let marked = format!("\u{200b}{a}\u{061c}\n\u{00ad}{b}\u{2060}{c}\u{200b}\u{200d}");
        assert_eq!(
            verify(&marked, &testkit::keyring(), device.code())
                .unwrap()
                .key,
            text
        );
        let message = format!("Voici votre clé :\n{text}\nBonne journée !\nمرحبا");
        assert_eq!(
            verify(&message, &testkit::keyring(), device.code())
                .unwrap()
                .key,
            text
        );
        let dzlic = format!("# DZBuild POS licence lifetime\n{a}\n{b}{c}\n");
        assert_eq!(
            verify(&dzlic, &testkit::keyring(), device.code())
                .unwrap()
                .key,
            text
        );
    }

    #[test]
    fn a_key_for_another_device_is_refused() {
        let text = testkit::lifetime(&testkit::other_device(), now_ms());
        assert_eq!(
            code(verify(&text, &testkit::keyring(), testkit::device().code())),
            WRONG_DEVICE
        );
    }

    #[test]
    fn a_changed_byte_another_signer_or_an_unknown_key_id_is_refused() {
        let device = testkit::device();
        let ring = testkit::keyring();
        let now = now_ms();
        // a trial stretched by 30 days, the signature kept
        let envelope = key::decode(&testkit::trial(&device, now)).unwrap();
        let mut longer: Payload = serde_json::from_slice(&envelope.payload).unwrap();
        longer.expires_at = longer.expires_at.map(|end| end + 30 * DAY_MS);
        let stretched = key::encode(&longer.canonical_bytes().unwrap(), &envelope.signature);
        assert_eq!(code(verify(&stretched, &ring, device.code())), INVALID);
        // a keygen of someone else's, under the owner's key id
        let impostor = SigningKey::from_bytes(&[7_u8; 32]);
        let payload = testkit::payload(LicenseKind::Lifetime, &device, now);
        let forged = testkit::sign_bytes(&impostor, &payload.canonical_bytes().unwrap());
        assert_eq!(code(verify(&forged, &ring, device.code())), INVALID);
        // a key id this build does not hold
        let mut unknown = testkit::payload(LicenseKind::Lifetime, &device, now);
        unknown.key_id = "k9".into();
        assert_eq!(
            code(verify(&testkit::sign(&unknown), &ring, device.code())),
            INVALID
        );
        // the production keyring alone never trusts the test key
        let test_key = testkit::lifetime(&device, now);
        assert_eq!(
            code(verify(&test_key, &Keyring::production(), device.code())),
            INVALID
        );
    }

    #[test]
    fn a_non_canonical_or_extended_payload_is_refused_even_when_signed() {
        let device = testkit::device();
        let payload = testkit::payload(LicenseKind::Lifetime, &device, now_ms());
        let pretty = serde_json::to_vec_pretty(&payload).unwrap();
        let signed_pretty = testkit::sign_bytes(&testkit::signing_key(), &pretty);
        assert_eq!(
            code(verify(&signed_pretty, &testkit::keyring(), device.code())),
            INVALID
        );
        let mut value = serde_json::to_value(&payload).unwrap();
        value["max_tills"] = serde_json::json!(3);
        let extended = pos_core::audit::canonical_json(&value).into_bytes();
        let signed_extended = testkit::sign_bytes(&testkit::signing_key(), &extended);
        assert_eq!(
            code(verify(&signed_extended, &testkit::keyring(), device.code())),
            INVALID
        );
    }

    #[test]
    fn a_revoked_key_is_refused() {
        let device = testkit::device();
        let payload = testkit::payload(LicenseKind::Lifetime, &device, now_ms());
        let text = testkit::sign(&payload);
        let revoked = [payload.id.as_str()];
        assert_eq!(
            code(verify_with(
                &text,
                &testkit::keyring(),
                device.code(),
                &revoked
            )),
            REVOKED
        );
        verify_with(&text, &testkit::keyring(), device.code(), &[]).unwrap();
    }

    #[test]
    fn the_signing_context_separates_licences_from_other_messages() {
        let device = testkit::device();
        let bytes = testkit::payload(LicenseKind::Lifetime, &device, now_ms())
            .canonical_bytes()
            .unwrap();
        let bare = testkit::signing_key().sign(&bytes); // no `SIGNING_CONTEXT`
        let text = key::encode(&bytes, &bare.to_bytes());
        assert_eq!(
            code(verify(&text, &testkit::keyring(), device.code())),
            INVALID
        );
    }
}

//! The public keys licences are checked against, built into the app. The owner's private
//! key never ships: `dzpos-license keygen` prints the lines to paste here, and
//! docs/release/licensing.md tells how to rotate a key.

use ed25519_dalek::VerifyingKey;
use sha2::{Digest, Sha256};

pub struct PublicKey {
    pub id: &'static str,
    pub bytes: [u8; 32],
}

/// The owner's licence keys, newest last (an old key stays until its licences were re-issued).
/// Pasted from `dzpos-license keygen` (the owner makes the first one, `k1`: docs/release/licensing.md
/// § Once). While it is empty a release build accepts no licence.
pub const PUBLIC_KEYS: &[PublicKey] = &[PublicKey {
    id: "k1",
    bytes: [
        0x1d, 0x2f, 0x8a, 0x1b, 0x46, 0xf9, 0xec, 0x8f, 0x1a, 0x63, 0xb1, 0xfc, 0x8c, 0xec, 0x35,
        0xdb, 0x4d, 0x91, 0x1b, 0x8b, 0x30, 0xcc, 0x81, 0xd4, 0xb3, 0xdd, 0x20, 0x50, 0x3c, 0x32,
        0x0d, 0xad,
    ],
}];

/// SHA-256 of [`PUBLIC_KEYS`] ([`keyring_digest`]): a key changed in the binary without this
/// constant makes the state `Tampered` at start; `the_public_keys_match_their_digest` prints the
/// value to set after a rotation.
pub const KEYRING_SHA256: &str = "a9b991ff5fc89662b5806fe88046c20b64bc6496fab103458bf6eae2537b957e";

/// Licence ids the owner revoked (a replaced PC, a refund): refused from the release that lists
/// them (`license_revoked`). `dzpos-license revoke` prints the line to add.
pub const REVOKED: &[&str] = &[];

/// SHA-256 over `id` 0x1F `bytes` 0x1E of every key, in order, lower-case hex.
pub fn keyring_digest_of(keys: &[(&str, [u8; 32])]) -> String {
    let mut hasher = Sha256::new();
    for (id, bytes) in keys {
        hasher.update(id.as_bytes());
        hasher.update([0x1f]);
        hasher.update(bytes);
        hasher.update([0x1e]);
    }
    hex::encode(hasher.finalize().as_slice())
}

pub fn keyring_digest(keys: &[PublicKey]) -> String {
    let pairs: Vec<(&str, [u8; 32])> = keys.iter().map(|k| (k.id, k.bytes)).collect();
    keyring_digest_of(&pairs)
}

/// The built-in keys are the ones this build was released with.
pub fn keyring_intact() -> bool {
    keyring_digest(PUBLIC_KEYS) == KEYRING_SHA256
}

/// The keys a build trusts, by id. `Debug` prints the ids only.
#[derive(Clone, Default)]
pub struct Keyring {
    keys: Vec<(String, VerifyingKey)>,
}

impl std::fmt::Debug for Keyring {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Keyring({:?})", self.ids())
    }
}

impl Keyring {
    /// The owner's keys only: what a release build trusts.
    pub fn production() -> Keyring {
        Keyring::from_keys(PUBLIC_KEYS)
    }

    /// What this build trusts: [`production`](Self::production), plus the development key in a
    /// debug build (`pnpm tauri dev`); never in a release build.
    pub fn embedded() -> Keyring {
        #[cfg(debug_assertions)]
        {
            Keyring::production().with(dev::KEY_ID, dev::verifying_key())
        }
        #[cfg(not(debug_assertions))]
        {
            Keyring::production()
        }
    }

    /// The valid keys of `keys` (a byte string that is not a curve point is skipped).
    pub fn from_keys(keys: &[PublicKey]) -> Keyring {
        Keyring {
            keys: keys
                .iter()
                .filter_map(|k| {
                    VerifyingKey::from_bytes(&k.bytes)
                        .ok()
                        .map(|v| (k.id.to_owned(), v))
                })
                .collect(),
        }
    }

    #[must_use]
    pub fn with(mut self, id: &str, key: VerifyingKey) -> Keyring {
        self.keys.push((id.to_owned(), key));
        self
    }

    pub fn get(&self, id: &str) -> Option<&VerifyingKey> {
        self.keys.iter().find(|(k, _)| k == id).map(|(_, v)| v)
    }

    pub fn ids(&self) -> Vec<String> {
        self.keys.iter().map(|(k, _)| k.clone()).collect()
    }
}

/// The development key (debug builds only). Its secret is in this file on purpose, so no release
/// build can ever trust it; it signs the developers' own licences (`dzpos-license issue --dev` in a
/// debug build of the tool).
#[cfg(debug_assertions)]
pub mod dev {
    use ed25519_dalek::{SigningKey, VerifyingKey};

    pub const KEY_ID: &str = "dev";
    pub const SECRET: [u8; 32] = *b"DZBuild POS development key 0001";

    pub fn signing_key() -> SigningKey {
        SigningKey::from_bytes(&SECRET)
    }

    pub fn verifying_key() -> VerifyingKey {
        signing_key().verifying_key()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_public_keys_match_their_digest() {
        let digest = keyring_digest(PUBLIC_KEYS);
        assert_eq!(
            digest, KEYRING_SHA256,
            "PUBLIC_KEYS changed: set KEYRING_SHA256 to \"{digest}\" (docs/release/licensing.md § Rotation)"
        );
        assert!(keyring_intact());
        assert_eq!(
            Keyring::production().ids().len(),
            PUBLIC_KEYS.len(),
            "every production key is a valid Ed25519 point"
        );
    }

    #[test]
    fn the_digest_covers_every_id_and_byte() {
        let one = keyring_digest_of(&[("k1", [1_u8; 32])]);
        assert_ne!(one, keyring_digest_of(&[("k2", [1_u8; 32])]));
        assert_ne!(one, keyring_digest_of(&[("k1", [2_u8; 32])]));
        assert_ne!(
            one,
            keyring_digest_of(&[("k1", [1_u8; 32]), ("k2", [1_u8; 32])])
        );
        assert_eq!(
            keyring_digest_of(&[]),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn the_release_keyring_never_holds_the_development_key() {
        assert!(Keyring::production().get("dev").is_none());
        assert!(PUBLIC_KEYS.iter().all(|k| k.id != "dev"));
        #[cfg(debug_assertions)]
        {
            assert!(
                Keyring::embedded().get(dev::KEY_ID).is_some(),
                "a debug build trusts the development key"
            );
            let dev_bytes = dev::verifying_key().to_bytes();
            assert!(
                PUBLIC_KEYS.iter().all(|k| k.bytes != dev_bytes),
                "the development key is never a production key"
            );
        }
        #[cfg(not(debug_assertions))]
        assert!(
            Keyring::embedded().get("dev").is_none(),
            "a release build never trusts it"
        );
    }

    #[test]
    fn a_keyring_finds_its_keys_by_id_and_prints_no_key() {
        let ring = crate::testkit::keyring();
        assert!(ring.get(crate::testkit::KEY_ID).is_some());
        assert!(ring.get("k9").is_none());
        let printed = format!("{ring:?}");
        assert!(printed.contains("test"), "{printed}");
        assert!(!printed.contains(&hex::encode(
            crate::testkit::signing_key().verifying_key().to_bytes()
        )));
    }
}

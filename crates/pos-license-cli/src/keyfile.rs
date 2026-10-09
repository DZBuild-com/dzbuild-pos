//! The licence signing key on the owner's disk: the 32-byte seed sealed with age (scrypt) under a
//! passphrase, beside its public half. Never inside a git work tree, never overwritten.

use crate::CliError;
use age::secrecy::SecretString;
use ed25519_dalek::{SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const SECRET_FILE: &str = "licence-signing.key";
pub const PUBLIC_FILE: &str = "licence-signing.pub";
pub const MIN_PASSPHRASE_CHARS: usize = 12;
/// The most work a sealed key may ask for when it is opened (a damaged or hostile file cannot make
/// the tool spin for hours).
const MAX_WORK_FACTOR: u8 = 22;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Sealed {
    v: u32,
    key_id: String,
    seed: String,
    created_at: i64,
}

fn damaged() -> CliError {
    CliError::new("the key files are damaged")
}

/// `dir` or one of its parents is a git work tree (this repository included).
pub fn inside_git_work_tree(dir: &Path) -> bool {
    let absolute = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
    absolute.ancestors().any(|d| d.join(".git").exists())
}

pub fn key_id_ok(id: &str) -> bool {
    (1..=16).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Refuses a folder inside a git work tree or one that already holds a key (asked before the
/// passphrase).
pub fn check_target(dir: &Path) -> Result<(), CliError> {
    if inside_git_work_tree(dir) {
        return Err(CliError::new(format!(
            "{} is inside a git folder: keep the licence key outside any repository",
            dir.display()
        )));
    }
    if dir.join(SECRET_FILE).exists() {
        return Err(CliError::new(format!(
            "{} already holds a licence key: it is never overwritten",
            dir.display()
        )));
    }
    Ok(())
}

/// Makes the key in `dir` and returns its public half.
pub fn create(
    dir: &Path,
    key_id: &str,
    passphrase: &str,
    work_factor: u8,
    now: i64,
) -> Result<VerifyingKey, CliError> {
    check_target(dir)?;
    if passphrase.chars().count() < MIN_PASSPHRASE_CHARS {
        return Err(CliError::new(format!(
            "the passphrase needs {MIN_PASSPHRASE_CHARS} characters or more"
        )));
    }
    if !key_id_ok(key_id) {
        return Err(CliError::new(
            "--id: 1 to 16 lower-case letters, digits or dashes",
        ));
    }
    let mut seed = [0_u8; 32];
    getrandom::fill(&mut seed)
        .map_err(|e| CliError::new(format!("Windows gave no randomness: {e}")))?;
    let signing = SigningKey::from_bytes(&seed);
    let sealed = Sealed {
        v: 1,
        key_id: key_id.to_owned(),
        seed: hex::encode(seed),
        created_at: now,
    };
    let plain = serde_json::to_vec(&sealed).map_err(|_| damaged())?;
    let mut recipient = age::scrypt::Recipient::new(SecretString::from(passphrase.to_owned()));
    recipient.set_work_factor(work_factor);
    let wrapped = age::encrypt(&recipient, &plain)
        .map_err(|_| CliError::new("the key could not be sealed"))?;
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join(SECRET_FILE), wrapped)?;
    let public = signing.verifying_key();
    std::fs::write(
        dir.join(PUBLIC_FILE),
        format!("{key_id} {}\n", hex::encode(public.to_bytes())),
    )?;
    Ok(public)
}

/// Opens the key with the passphrase; its public half must match.
pub fn load(dir: &Path, passphrase: &str) -> Result<(String, SigningKey), CliError> {
    let wrapped = std::fs::read(dir.join(SECRET_FILE)).map_err(|_| {
        CliError::new(format!(
            "no licence key in {} (dzpos-license keygen, or --keys <folder>)",
            dir.display()
        ))
    })?;
    let mut identity = age::scrypt::Identity::new(SecretString::from(passphrase.to_owned()));
    identity.set_max_work_factor(MAX_WORK_FACTOR);
    let plain = age::decrypt(&identity, &wrapped)
        .map_err(|_| CliError::new("wrong passphrase (or a damaged key file)"))?;
    let sealed: Sealed = serde_json::from_slice(&plain).map_err(|_| damaged())?;
    let seed: [u8; 32] = hex::decode(&sealed.seed)
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or_else(damaged)?;
    let signing = SigningKey::from_bytes(&seed);
    let (id, public) = read_public(dir)?;
    if id != sealed.key_id || public != signing.verifying_key() {
        return Err(CliError::new(
            "the public key file does not match the private key",
        ));
    }
    Ok((sealed.key_id, signing))
}

pub fn read_public(dir: &Path) -> Result<(String, VerifyingKey), CliError> {
    let text = std::fs::read_to_string(dir.join(PUBLIC_FILE))
        .map_err(|_| CliError::new(format!("no {PUBLIC_FILE} in {}", dir.display())))?;
    let (id, hex_key) = text.trim().split_once(' ').ok_or_else(damaged)?;
    let bytes: [u8; 32] = hex::decode(hex_key)
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or_else(damaged)?;
    let public = VerifyingKey::from_bytes(&bytes).map_err(|_| damaged())?;
    Ok((id.to_owned(), public))
}

/// The lines to paste into `crates/pos-license/src/keys.rs` for a keyring of this one key (a
/// rotation appends the new entry and takes the digest `the_public_keys_match_their_digest` prints).
pub fn rust_snippet(
    key_id: &str,
    public: &VerifyingKey,
    existing: &[(String, [u8; 32])],
) -> String {
    let bytes = public.to_bytes();
    let rows: Vec<String> = bytes
        .chunks(8)
        .map(|row| {
            row.iter()
                .map(|b| format!("0x{b:02x}"))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .collect();
    // Final review: a rotation keeps the release's keys and appends the new one; the digest is
    // over all of them, in order.
    let mut ring: Vec<(&str, [u8; 32])> = existing
        .iter()
        .map(|(id, bytes)| (id.as_str(), *bytes))
        .collect();
    ring.push((key_id, bytes));
    let digest = pos_license::keys::keyring_digest_of(&ring);
    if existing.is_empty() {
        return format!(
            "pub const PUBLIC_KEYS: &[PublicKey] = &[PublicKey {{\n    id: \"{key_id}\",\n    bytes: [\n        {},\n    ],\n}}];\n\npub const KEYRING_SHA256: &str = \"{digest}\";\n",
            rows.join(",\n        ")
        );
    }
    format!(
        "// the last entry of PUBLIC_KEYS (after {}):\n    PublicKey {{\n        id: \"{key_id}\",\n        bytes: [\n            {},\n        ],\n    }},\n\npub const KEYRING_SHA256: &str = \"{digest}\";\n",
        existing
            .iter()
            .map(|(id, _)| id.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        rows.join(",\n            ")
    )
}

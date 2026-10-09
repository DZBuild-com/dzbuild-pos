//! The `license_*` error codes: one family, translated in `src/locales/*/errors.json` and listed in
//! `pos_core::error::KNOWN_CODES`. Every code this crate emits is one of these constants
//! (`every_code_is_known_and_no_literal_escapes` keeps it so).

/// No licence on this PC: a write was refused, or the shop cannot be set up yet.
pub const REQUIRED: &str = "license_required";
/// The trial ended (against the newest time this PC has shown, not its clock alone).
pub const EXPIRED: &str = "license_expired";
/// The licence copies of this PC were edited by hand or disagree, or the app's keys were altered.
pub const TAMPERED: &str = "license_tampered";
/// Not a licence key, a wrong signature, an unknown signing key or a payload outside the rules.
pub const INVALID: &str = "license_invalid";
/// The key was issued for another device code.
pub const WRONG_DEVICE: &str = "license_wrong_device";
/// This PC already used its trial (another trial key).
pub const TRIAL_USED: &str = "license_trial_used";
/// This PC's date is before the key's issue date: set the date first.
pub const CLOCK_BEHIND: &str = "license_clock_behind";
/// The owner revoked the key (a replaced PC, a refund); refused from the release that lists it.
pub const REVOKED: &str = "license_revoked";
/// A trial cannot replace the lifetime licence of this PC.
pub const DOWNGRADE: &str = "license_downgrade";
/// Windows did not give this PC's identity, so no key can be checked.
pub const DEVICE_UNREADABLE: &str = "license_device_unreadable";

pub const ALL: [&str; 10] = [
    REQUIRED,
    EXPIRED,
    TAMPERED,
    INVALID,
    WRONG_DEVICE,
    TRIAL_USED,
    CLOCK_BEHIND,
    REVOKED,
    DOWNGRADE,
    DEVICE_UNREADABLE,
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }

    #[test]
    fn every_code_is_known_and_no_literal_escapes() {
        assert_eq!(ALL.len(), 10);
        for code in ALL {
            assert!(code.starts_with("license_"), "{code}");
            assert!(
                pos_core::error::KNOWN_CODES.contains(&code),
                "{code} is missing from pos_core::error::KNOWN_CODES"
            );
        }
        let mut files = Vec::new();
        rust_files(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut files,
        );
        assert!(files.len() >= 2, "the scan reads the crate");
        // Built at run time so that this line is not a match itself.
        let needle = ["\"", "license_"].concat();
        for file in files.iter().filter(|f| !f.ends_with("codes.rs")) {
            let text = std::fs::read_to_string(file).unwrap();
            assert!(
                !text.contains(&needle),
                "{}: name the code through pos_license::codes",
                file.display()
            );
        }
    }
}

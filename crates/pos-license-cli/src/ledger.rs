//! The issuance ledger beside the key (`ledger.json`): every licence issued, so that a second trial
//! for a PC is refused and a lifetime licence moves to a new PC only on purpose (`--replace`). It
//! holds customers' names and phones: it stays in the key folder, never in the repository.

use crate::CliError;
use pos_license::payload::{LicenseCustomer, LicenseKind};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

pub const LEDGER_FILE: &str = "ledger.json";
pub const ISSUED_DIR: &str = "issued";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Issued {
    pub id: String,
    pub kind: LicenseKind,
    pub device_code: String,
    pub customer: Option<LicenseCustomer>,
    pub issued_at: i64,
    pub expires_at: Option<i64>,
    pub key: String,
    pub replaces: Option<String>,
    pub replaced_by: Option<String>,
    pub revoked_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Ledger {
    pub v: u32,
    pub key_id: String,
    pub licences: Vec<Issued>,
}

impl Ledger {
    pub fn new(key_id: &str) -> Ledger {
        Ledger {
            v: 1,
            key_id: key_id.to_owned(),
            licences: Vec::new(),
        }
    }

    pub fn load(dir: &Path) -> Result<Ledger, CliError> {
        let text = std::fs::read_to_string(dir.join(LEDGER_FILE)).map_err(|_| {
            CliError::new(format!(
                "no {LEDGER_FILE} in {} (dzpos-license keygen, or --keys <folder>)",
                dir.display()
            ))
        })?;
        serde_json::from_str(&text)
            .map_err(|_| CliError::new(format!("{LEDGER_FILE} is damaged: restore it from a copy")))
    }

    /// `ledger.json.tmp`, sync, rename: a power cut never leaves half a ledger.
    pub fn save(&self, dir: &Path) -> Result<(), CliError> {
        let text = serde_json::to_string_pretty(self)
            .map_err(|_| CliError::new("the ledger could not be written"))?;
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join(format!("{LEDGER_FILE}.tmp"));
        let written = std::fs::File::create(&tmp).and_then(|mut file| {
            file.write_all(text.as_bytes())?;
            file.sync_all()
        });
        let result = written.and_then(|()| std::fs::rename(&tmp, dir.join(LEDGER_FILE)));
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        Ok(result?)
    }

    pub fn trial_of(&self, device: &str) -> Option<&Issued> {
        self.licences
            .iter()
            .find(|l| l.kind == LicenseKind::Trial && l.device_code == device)
    }

    pub fn live_lifetime_of(&self, device: &str) -> Option<&Issued> {
        self.licences.iter().find(|l| {
            l.kind == LicenseKind::Lifetime
                && l.device_code == device
                && l.revoked_at.is_none()
                && l.replaced_by.is_none()
        })
    }

    pub fn get(&self, id: &str) -> Option<&Issued> {
        self.licences.iter().find(|l| l.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Issued> {
        self.licences.iter_mut().find(|l| l.id == id)
    }
}

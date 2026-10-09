//! A signing key and a PC for tests: this crate's, and the shell's through the `testkit` feature
//! (src-tauri's dev-dependencies only). The secret is public on purpose; only tests that pass
//! [`keyring`] explicitly trust it.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::device::{Components, DeviceId};
use crate::key;
use crate::keys::Keyring;
use crate::payload::{
    DAY_MS, LicenseCustomer, LicenseKind, Payload, SIGNING_CONTEXT, TRIAL_DAYS, VERSION,
};
use ed25519_dalek::{Signer, SigningKey};

pub const KEY_ID: &str = "test";
const SECRET: [u8; 32] = *b"DZBuild POS licence test key 01!";

pub fn signing_key() -> SigningKey {
    SigningKey::from_bytes(&SECRET)
}

/// The production keys and the test key.
pub fn keyring() -> Keyring {
    Keyring::production().with(KEY_ID, signing_key().verifying_key())
}

pub fn components() -> Components {
    Components {
        machine_guid: "6f1c3a52-9d3e-4b7a-8c21-0e5d7f9a1b24".into(),
        volume_serial: 0x1A2B_3C4D,
        cpu: "GenuineIntel|Intel(R) Celeron(R) CPU J1900 @ 1.99GHz|Intel64 Family 6 Model 55 Stepping 8"
            .into(),
        product_id: "00330-80000-00000-AA123".into(),
        smbios_uuid: "4c4c4544-0042-3510-8051-b4c04f4d4a32".into(),
        disk_serial: "S3Z1NB0K123456A".into(),
    }
}

pub fn device() -> DeviceId {
    DeviceId::from_components(&components()).expect("the test device")
}

/// The same machine with another disk: another PC for the licence.
pub fn other_device() -> DeviceId {
    let mut components = components();
    components.volume_serial ^= 0xFFFF;
    DeviceId::from_components(&components).expect("another device")
}

pub fn payload(kind: LicenseKind, device: &DeviceId, issued_at: i64) -> Payload {
    Payload {
        v: VERSION,
        id: pos_core::ids::new_id(),
        kind,
        device_code: device.code().to_owned(),
        issued_at,
        expires_at: (kind == LicenseKind::Trial).then_some(issued_at + TRIAL_DAYS * DAY_MS),
        customer: Some(LicenseCustomer {
            name: Some("Supérette El Amel".into()),
            phone: Some("0550 12 34 56".into()),
        }),
        features: Vec::new(),
        key_id: KEY_ID.to_owned(),
    }
}

/// `payload_bytes` signed by `key` with the licence context, as key text.
pub fn sign_bytes(key: &SigningKey, payload_bytes: &[u8]) -> String {
    let mut message = SIGNING_CONTEXT.to_vec();
    message.extend_from_slice(payload_bytes);
    key::encode(payload_bytes, &key.sign(&message).to_bytes())
}

pub fn sign(payload: &Payload) -> String {
    sign_bytes(
        &signing_key(),
        &payload.canonical_bytes().expect("canonical bytes"),
    )
}

pub fn trial(device: &DeviceId, issued_at: i64) -> String {
    sign(&payload(LicenseKind::Trial, device, issued_at))
}

pub fn lifetime(device: &DeviceId, issued_at: i64) -> String {
    sign(&payload(LicenseKind::Lifetime, device, issued_at))
}

/// A store that refuses everything, like a Credential Manager Windows will not open.
#[derive(Debug, Default)]
pub struct FailingStore;

impl pos_core::cloud::secrets::SecretStore for FailingStore {
    fn get(&self, _: &str) -> Result<Option<String>, pos_core::error::AppError> {
        Err(pos_core::error::AppError::sync("cloud_secret_store_failed"))
    }
    fn set(&self, _: &str, _: &str) -> Result<(), pos_core::error::AppError> {
        Err(pos_core::error::AppError::sync("cloud_secret_store_failed"))
    }
    fn delete(&self, _: &str) -> Result<(), pos_core::error::AppError> {
        Ok(())
    }
}

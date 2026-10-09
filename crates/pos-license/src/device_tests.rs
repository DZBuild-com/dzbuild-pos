//! The device code tells apart the tills installed from one disk
//! image (the motherboard's SMBIOS system UUID and the system disk's serial number join the four
//! Windows identities), and its check symbol is a weighted sum over GF(32) that catches every single
//! wrong symbol and every two neighbouring symbols swapped.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::device::{
    ALPHABET, Components, DeviceId, PREFIX, descriptor_serial, parse_code, smbios_system_uuid,
};
use crate::testkit;
use sha2::{Digest, Sha256};

fn code_of(components: &Components) -> String {
    DeviceId::from_components(components)
        .unwrap()
        .code()
        .to_owned()
}

/// Another till installed from the same disk image: Windows' identities (MachineGuid, ProductId,
/// the volume serial copied with the image) and the processor model are the same; the motherboard
/// and the disk are not.
fn clone_with(smbios_uuid: &str, disk_serial: &str) -> Components {
    Components {
        smbios_uuid: smbios_uuid.into(),
        disk_serial: disk_serial.into(),
        ..testkit::components()
    }
}

#[test]
fn clones_of_one_disk_image_get_different_codes() {
    let first = clone_with("4c4c4544-0042-3510-8051-b4c04f4d4a32", "S3Z1NB0K123456A");
    let second = clone_with("4c4c4544-0042-3510-8051-b4c04f4d4a33", "S3Z1NB0K123456A");
    assert_eq!(
        (
            &first.machine_guid,
            &first.product_id,
            first.volume_serial,
            &first.cpu
        ),
        (
            &second.machine_guid,
            &second.product_id,
            second.volume_serial,
            &second.cpu
        ),
        "the clones share every Windows identity"
    );
    let (a, b) = (
        DeviceId::from_components(&first).unwrap(),
        DeviceId::from_components(&second).unwrap(),
    );
    assert_ne!(a.code(), b.code(), "another motherboard, another code");
    assert_ne!(a.secret(), b.secret(), "and another sealing secret");
    // Cheap motherboards ship one placeholder UUID for every board: the disk tells them apart.
    let placeholder = "03000200-0400-0500-0006-000700080009";
    assert_ne!(
        code_of(&clone_with(placeholder, "S3Z1NB0K123456A")),
        code_of(&clone_with(placeholder, "S3Z1NB0K654321A")),
        "another disk, another code"
    );
}

#[test]
fn the_hardware_identities_are_read_in_one_canonical_form() {
    let base = testkit::components();
    let loose = Components {
        smbios_uuid: format!("  {}  ", base.smbios_uuid.to_uppercase()),
        disk_serial: format!("    {} ", base.disk_serial.to_lowercase()),
        ..base.clone()
    };
    assert_eq!(code_of(&loose), code_of(&base));
}

/// The codes of 1000 random PCs (a fixed seed: the same sample at every run).
fn sample_codes() -> Vec<String> {
    (0..1000_u32)
        .map(|i| {
            let random = |what: &str| hex::encode(Sha256::digest(format!("{what}-{i}").as_bytes()));
            code_of(&Components {
                machine_guid: random("guid"),
                volume_serial: i.wrapping_mul(2_654_435_761),
                cpu: random("cpu"),
                product_id: random("product"),
                smbios_uuid: random("smbios"),
                disk_serial: random("disk"),
            })
        })
        .collect()
}

/// The 16 symbols of a code (15 data symbols and the check symbol).
fn symbols(code: &str) -> Vec<char> {
    code[PREFIX.len()..].chars().filter(|c| *c != '-').collect()
}

#[test]
fn the_check_symbol_catches_every_single_substitution_and_adjacent_transposition() {
    let codes = sample_codes();
    let mut distinct = codes.clone();
    distinct.sort();
    distinct.dedup();
    assert_eq!(distinct.len(), codes.len(), "1000 PCs, 1000 codes");
    let (mut substitutions, mut transpositions) = (0, 0);
    for code in &codes {
        let good = symbols(code);
        assert_eq!(good.len(), 16, "{code}");
        for position in 0..good.len() {
            for replacement in ALPHABET.iter().copied().map(char::from) {
                if replacement == good[position] {
                    continue;
                }
                let mut typed = good.clone();
                typed[position] = replacement;
                let typed: String = typed.into_iter().collect();
                assert_eq!(
                    parse_code(&typed),
                    None,
                    "{code}: symbol {position} typed as {replacement} is accepted"
                );
                substitutions += 1;
            }
        }
        // Neighbours swapped (the typo the first, unweighted check let through once in 32), and any two
        // symbols swapped: every weight differs from every other.
        for first in 0..good.len() {
            for second in first + 1..good.len() {
                if good[first] == good[second] {
                    continue;
                }
                let mut typed = good.clone();
                typed.swap(first, second);
                let typed: String = typed.into_iter().collect();
                assert_eq!(
                    parse_code(&typed),
                    None,
                    "{code}: symbols {first} and {second} swapped are accepted"
                );
                if second == first + 1 {
                    transpositions += 1;
                }
            }
        }
    }
    assert_eq!(substitutions, 1000 * 16 * 31);
    assert!(transpositions > 1000 * 14, "{transpositions}");
}

#[test]
fn every_code_parses_back_however_it_is_typed_or_pasted() {
    for code in sample_codes() {
        let body = &code[PREFIX.len() + 1..];
        for typed in [
            code.clone(),
            code.to_lowercase().replace('-', " "),
            body.to_owned(),
            body.replace('0', "O").replace('1', "l"),
            code.replacen("DZPOS", "DZP0S", 1),
            format!(
                "\u{200f}رمز الجهاز: \u{2066}{}\u{2069}",
                code.replace('-', "\u{2013}")
            ),
            format!("Bonjour, mon code est {code}. Merci !"),
        ] {
            assert_eq!(
                parse_code(&typed).as_deref(),
                Some(code.as_str()),
                "{typed:?}"
            );
        }
    }
}

/// `GetSystemFirmwareTable('RSMB')` as a PC returns it: the 8-byte `RawSMBIOSData` header, a BIOS
/// structure (type 0) and its strings, the system structure (type 1, the UUID at offset 8), the end
/// of the table (type 127).
fn rsmb(uuid: [u8; 16]) -> Vec<u8> {
    let mut table = vec![0_u8, 0x18, 0x00, 0x00];
    table.extend([1, 2, 0, 0, 3, 0x7f, 0x80, 0x98, 0x0b, 0, 0, 0]);
    table.extend([0; 8]);
    table.extend(b"American Megatrends Inc.\0P1.20\x0003/12/2019\0\0");
    table.extend([1_u8, 0x1b, 0x01, 0x00, 1, 2, 3, 4]);
    table.extend(uuid);
    table.extend([6, 5, 6]);
    table.extend(b"To Be Filled By O.E.M.\0B450M Pro4\0\0");
    table.extend([127_u8, 4, 0x02, 0x00, 0, 0]);
    let mut raw = vec![0_u8, 3, 3, 0];
    raw.extend(u32::try_from(table.len()).unwrap().to_le_bytes());
    raw.extend(table);
    raw
}

/// The bytes a Dell PC stores for `4C4C4544-0042-3510-8051-B4C04F4D4A32` (SMBIOS 2.6: the first
/// three fields little-endian).
const DELL: [u8; 16] = [
    0x44, 0x45, 0x4c, 0x4c, 0x42, 0x00, 0x10, 0x35, 0x80, 0x51, 0xb4, 0xc0, 0x4f, 0x4d, 0x4a, 0x32,
];

#[test]
fn the_smbios_system_uuid_is_read_from_the_type_1_structure() {
    assert_eq!(
        smbios_system_uuid(&rsmb(DELL)).as_deref(),
        Some("4c4c4544-0042-3510-8051-b4c04f4d4a32")
    );
    // "not present" and "not set" (SMBIOS 7.2.1) are no identity
    assert_eq!(smbios_system_uuid(&rsmb([0; 16])), None);
    assert_eq!(smbios_system_uuid(&rsmb([0xff; 16])), None);
    // SMBIOS 2.0: a type 1 without a UUID
    let mut short = rsmb(DELL);
    let at = short
        .windows(4)
        .position(|w| w == [1, 0x1b, 0x01, 0x00])
        .unwrap();
    short[at + 1] = 0x08;
    assert_eq!(smbios_system_uuid(&short), None);
    // a table without a type 1 structure, a structure shorter than its header, nothing at all
    let mut none = rsmb(DELL);
    none[at] = 2;
    assert_eq!(smbios_system_uuid(&none), None);
    let mut broken = rsmb(DELL);
    broken[9] = 0x02;
    assert_eq!(smbios_system_uuid(&broken), None);
    assert_eq!(smbios_system_uuid(&[]), None);
    // every truncation of the table: no panic, never a UUID read past the end
    let whole = rsmb(DELL);
    for len in 0..whole.len() {
        let cut = smbios_system_uuid(&whole[..len]);
        assert!(cut.is_none() || len >= at + 24, "{len}: {cut:?}");
    }
    // the table length in the header bounds the walk
    let mut lying = rsmb(DELL);
    lying[4..8].copy_from_slice(&u32::try_from(at - 8).unwrap().to_le_bytes());
    assert_eq!(smbios_system_uuid(&lying), None);
}

/// A `STORAGE_DEVICE_DESCRIPTOR` as `IOCTL_STORAGE_QUERY_PROPERTY` returns it: 40 bytes of header
/// (the serial number's offset at byte 24), the product id, then the serial number.
fn descriptor(serial: &[u8]) -> Vec<u8> {
    let product = b"Samsung SSD 860 EVO 500GB\0";
    let mut d = vec![0_u8; 40];
    let serial_at = 40 + product.len();
    let size = serial_at + serial.len();
    d[0..4].copy_from_slice(&40_u32.to_le_bytes());
    d[4..8].copy_from_slice(&u32::try_from(size).unwrap().to_le_bytes());
    d[16..20].copy_from_slice(&40_u32.to_le_bytes());
    d[24..28].copy_from_slice(&u32::try_from(serial_at).unwrap().to_le_bytes());
    d[28..32].copy_from_slice(&11_u32.to_le_bytes());
    d.extend(product);
    d.extend(serial);
    d
}

#[test]
fn the_disk_serial_is_read_from_the_storage_descriptor() {
    assert_eq!(
        descriptor_serial(&descriptor(b"S3Z1NB0K123456A\0")).as_deref(),
        Some("S3Z1NB0K123456A")
    );
    // ATA serials come padded with spaces
    assert_eq!(
        descriptor_serial(&descriptor(b"     WD-WCC4N0KXR1A7\0")).as_deref(),
        Some("WD-WCC4N0KXR1A7")
    );
    // no serial: blank, absent (offset 0), past the end, nothing at all
    assert_eq!(descriptor_serial(&descriptor(b"        \0")), None);
    let mut absent = descriptor(b"S3Z1NB0K123456A\0");
    absent[24..28].copy_from_slice(&0_u32.to_le_bytes());
    assert_eq!(descriptor_serial(&absent), None);
    let mut past = descriptor(b"S3Z1NB0K123456A\0");
    past[24..28].copy_from_slice(&4096_u32.to_le_bytes());
    assert_eq!(descriptor_serial(&past), None);
    assert_eq!(descriptor_serial(&[]), None);
    let whole = descriptor(b"S3Z1NB0K123456A\0");
    for len in 0..whole.len() {
        let _ = descriptor_serial(&whole[..len]);
    }
}

/// The scheme is frozen: the test PC's code and sealing secret, for good. When this
/// fails, every licence already issued would name "another PC": undo the change (device.rs).
#[test]
fn the_scheme_is_frozen() {
    let device = testkit::device();
    assert_eq!(device.code(), "DZPOS-43CC-M9MK-7YBP-BPZH");
    assert_eq!(
        hex::encode(device.secret()),
        "dff55e6ced1a02ffa4f925b01c90cf0c251659016377e1e14cd0c3de77abe72e"
    );
}

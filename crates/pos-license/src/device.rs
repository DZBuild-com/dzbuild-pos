//! This PC's device code (`DZPOS-XXXX-XXXX-XXXX-XXXX`): 75 bits of a
//! SHA-256 over six identities — four from Windows (the machine GUID, the system volume's serial
//! number, the processor's vendor, brand and family/model/stepping, the Windows product id) and two
//! from the hardware (the motherboard's SMBIOS system UUID and the system disk's serial number, so
//! that tills installed from one disk image, which share the four Windows identities, get different
//! codes) — in Crockford base32, plus one check symbol: a weighted sum over GF(32) that refuses a
//! code typed from a WhatsApp message with one wrong symbol or two symbols swapped. Any change of a
//! component gives another code: the licence is then issued again (docs/release/licensing.md
//! § Re-issue). A second digest, the device secret, seals the stored records (`record.rs`) and
//! never leaves this PC.
//!
//! **Frozen since 2026-10-04 (before the first licence was issued): never change it
//! again.** Every licence names its PC by this code, so the identities read, their canonical line,
//! the two digest domains, the base32 body and the check weights stay as they are for good: any
//! change would turn every licence already issued into "another PC's".
//! `device_tests::the_scheme_is_frozen` pins a code and a secret; a new identity needs a new
//! licence format, not an edit here.

use crate::codes;
use pos_core::error::AppError;
use sha2::{Digest, Sha256};

pub const PREFIX: &str = "DZPOS";
/// Crockford base32: no I, L, O or U, so a code read aloud is not misread.
pub const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const CODE_CONTEXT: &[u8] = b"DZPOS-DEVICE-v2\x1f";
const SECRET_CONTEXT: &[u8] = b"DZPOS-DEVICE-SECRET-v2\x1f";
/// 15 data symbols (75 bits) and the check symbol.
const BODY_LEN: usize = 15;
/// The weights of the 15 data symbols in the check sum: α¹⁵ … α¹ in GF(32) = GF(2)[x]/(x⁵ + x² + 1)
/// with α = x (each weight is α times the next); the check symbol's own weight is α⁰ = 1. The 16
/// weights are distinct and none is 0, and GF(32) has no zero divisors, so one wrong symbol
/// (w·e ≠ 0) and any two different symbols swapped ((wᵢ + wⱼ)·(a + b) ≠ 0) always change the sum.
/// Integers mod 32 could not: 16·w = 0 for every even w, and two odd weights differ by an even one.
const CHECK_WEIGHTS: [u8; BODY_LEN] = [31, 29, 28, 14, 7, 17, 26, 13, 20, 10, 5, 16, 8, 4, 2];

/// What identifies this PC, as Windows and the firmware report it. `Debug` prints none of it.
#[derive(Clone, PartialEq, Eq)]
pub struct Components {
    /// `HKLM\SOFTWARE\Microsoft\Cryptography\MachineGuid` (a reinstall changes it).
    pub machine_guid: String,
    /// The serial number of the volume Windows runs from (a reformat changes it).
    pub volume_serial: u32,
    /// `VendorIdentifier|ProcessorNameString|Identifier` of
    /// `HKLM\HARDWARE\DESCRIPTION\System\CentralProcessor\0`.
    pub cpu: String,
    /// `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProductId`.
    pub product_id: String,
    /// The motherboard's SMBIOS system UUID (`GetSystemFirmwareTable('RSMB')`, the type-1
    /// structure), as Windows shows it; empty when the firmware has none. A clone of the disk image
    /// does not copy it; another motherboard changes it.
    pub smbios_uuid: String,
    /// The hardware serial number of the disk that holds the system volume
    /// (`IOCTL_STORAGE_QUERY_PROPERTY`); empty when the disk reports none. Another disk changes it,
    /// even one the image was copied to.
    pub disk_serial: String,
}

impl std::fmt::Debug for Components {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Components(..)")
    }
}

impl Components {
    /// One line in a fixed order and case: what both digests hash.
    fn canonical(&self) -> String {
        format!(
            "machine={}\x1fvolume={:08X}\x1fcpu={}\x1fproduct={}\x1fsmbios={}\x1fdisk={}",
            self.machine_guid.trim().to_ascii_lowercase(),
            self.volume_serial,
            self.cpu.split_whitespace().collect::<Vec<_>>().join(" "),
            self.product_id.trim().to_ascii_uppercase(),
            self.smbios_uuid.trim().to_ascii_lowercase(),
            // ATA serials come padded with spaces: only the visible characters count.
            self.disk_serial
                .chars()
                .filter(char::is_ascii_graphic)
                .collect::<String>()
                .to_ascii_uppercase(),
        )
    }
}

/// This PC as the licence sees it: the code shown to the customer and the secret that seals the
/// records. `Debug` prints neither (the code reaches the screen and the CLI through
/// `code()`, never a log line).
#[derive(Clone, PartialEq, Eq)]
pub struct DeviceId {
    code: String,
    secret: [u8; 32],
}

impl std::fmt::Debug for DeviceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DeviceId(..)")
    }
}

impl DeviceId {
    pub fn from_components(components: &Components) -> Result<DeviceId, AppError> {
        if components.machine_guid.trim().is_empty() {
            return Err(AppError::conflict(codes::DEVICE_UNREADABLE));
        }
        let line = components.canonical();
        let digest = sha256(&[CODE_CONTEXT, line.as_bytes()]);
        Ok(DeviceId {
            code: format_code(&base32_body(&digest)),
            secret: sha256(&[SECRET_CONTEXT, line.as_bytes()]),
        })
    }

    /// This PC (Windows only; elsewhere `license_device_unreadable`).
    pub fn this_pc() -> Result<DeviceId, AppError> {
        DeviceId::from_components(&system::components()?)
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn secret(&self) -> &[u8; 32] {
        &self.secret
    }
}

fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    let digest = hasher.finalize();
    let mut out = [0_u8; 32];
    out.copy_from_slice(digest.as_slice());
    out
}

/// The first 75 bits of `digest`, five at a time, as 15 Crockford symbols.
fn base32_body(digest: &[u8; 32]) -> String {
    let mut out = String::with_capacity(BODY_LEN);
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    let mut bytes = digest.iter();
    while out.len() < BODY_LEN {
        if bits < 5 {
            // 15 symbols read 10 bytes; the digest has 32.
            acc = (acc << 8) | u32::from(bytes.next().copied().unwrap_or(0));
            bits += 8;
        }
        bits -= 5;
        let index = (acc >> bits) & 0x1f;
        acc &= (1 << bits) - 1;
        out.push(char::from(ALPHABET[index as usize]));
    }
    out
}

/// Multiplication in GF(32) = GF(2)[x]/(x⁵ + x² + 1): the symbols' 5 bits are a polynomial's
/// coefficients.
const fn gf32_mul(a: u8, b: u8) -> u8 {
    let (mut a, mut b, mut product) = (a & 0x1f, b & 0x1f, 0_u8);
    while b != 0 {
        if b & 1 != 0 {
            product ^= a;
        }
        b >>= 1;
        a <<= 1;
        if a & 0x20 != 0 {
            a ^= 0b10_0101; // x⁵ = x² + 1
        }
    }
    product
}

/// The check symbol of 15 data symbols: Σ wᵢ·sᵢ in GF(32) (addition is XOR), so that the 16
/// symbols of a code, the check symbol weighing 1, sum to 0.
fn check_symbol(body: &str) -> char {
    let value = |symbol: u8| {
        ALPHABET
            .iter()
            .position(|s| *s == symbol)
            .and_then(|i| u8::try_from(i).ok())
            .unwrap_or(0)
    };
    let sum = body
        .bytes()
        .zip(CHECK_WEIGHTS)
        .fold(0, |sum, (symbol, weight)| {
            sum ^ gf32_mul(weight, value(symbol))
        });
    char::from(ALPHABET[usize::from(sum & 0x1f)])
}

/// `DZPOS-` and the 15 symbols with their check symbol, in four groups of four.
fn format_code(body: &str) -> String {
    let full = format!("{body}{}", check_symbol(body));
    let groups: Vec<String> = full
        .as_bytes()
        .chunks(4)
        .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
        .collect();
    format!("{PREFIX}-{}", groups.join("-"))
}

/// What a chat, a phone keyboard or a word processor does to a code, undone (final review): the
/// invisible format characters (zero-width spaces and joiners, direction marks and isolates, the
/// Arabic letter mark, the byte-order mark, the soft hyphen) dropped; typographic dashes and
/// minus signs read as `-`; Arabic-Indic and Persian digits and fullwidth ASCII read as ASCII.
pub(crate) fn fold(c: char) -> Option<char> {
    let shifted = |base: u32| char::from_digit(u32::from(c) - base, 10);
    match c {
        '\u{200b}'..='\u{200f}'
        | '\u{202a}'..='\u{202e}'
        | '\u{2060}'
        | '\u{2066}'..='\u{2069}'
        | '\u{061c}'
        | '\u{feff}'
        | '\u{00ad}' => None,
        '\u{2010}'..='\u{2015}' | '\u{2212}' | '\u{fe58}' | '\u{fe63}' | '\u{ff0d}' => Some('-'),
        '\u{0660}'..='\u{0669}' => shifted(0x0660),
        '\u{06f0}'..='\u{06f9}' => shifted(0x06f0),
        '\u{ff01}'..='\u{ff5e}' => char::from_u32(u32::from(c) - 0xfee0),
        other => Some(other),
    }
}

/// The symbols of the code that follows the last `DZPOS` (or `DZP0S`) in `text`: the 16 letters
/// and digits after it, dashes and spaces skipped; `None` when fewer follow, when something else
/// breaks them, or when the code goes on (a fifth group). Without the prefix, the whole text.
fn code_symbols(text: &str) -> Option<String> {
    let kept: Vec<(usize, char)> = text
        .char_indices()
        .filter(|(_, c)| !c.is_whitespace() && *c != '-')
        .collect();
    // `DZPOS` first (a body has no `O`, so it is always the prefix), else `DZP0S` typed for it.
    let last = |prefix: &str| {
        kept.windows(PREFIX.len())
            .rposition(|w| w.iter().map(|(_, c)| *c).eq(prefix.chars()))
    };
    let token = last(PREFIX).or_else(|| last("DZP0S"));
    let Some(at) = token else {
        return Some(kept.iter().map(|(_, c)| *c).collect());
    };
    let (last, c) = kept[at + PREFIX.len() - 1];
    let mut rest = text[last + c.len_utf8()..].chars().peekable();
    let mut symbols = String::with_capacity(BODY_LEN + 1);
    while let Some(c) = rest.next() {
        if symbols.len() == BODY_LEN + 1 {
            let goes_on = c.is_ascii_alphanumeric()
                || (c == '-' && rest.peek().is_some_and(char::is_ascii_alphanumeric));
            if goes_on {
                return None;
            }
            break;
        }
        if c.is_whitespace() || c == '-' {
            continue;
        }
        if !c.is_ascii_alphanumeric() {
            return None;
        }
        symbols.push(c);
    }
    (symbols.len() == BODY_LEN + 1).then_some(symbols)
}

/// A code as a person typed or pasted it — lower case, spaces, with or without `DZPOS-` (or
/// `DZP0S-`), `O` for `0`, `I` or `L` for `1`, copied from an Arabic chat with its invisible marks,
/// typographic dashes or Arabic-Indic digits, inside a sentence — in its canonical form; `None`
/// when it is not a device code or its check symbol is wrong.
pub fn parse_code(text: &str) -> Option<String> {
    let folded: String = text
        .chars()
        .filter_map(fold)
        .collect::<String>()
        .to_ascii_uppercase();
    // The prefix holds an `O`: it goes before the Crockford mapping (a body never contains "DZPOS":
    // the alphabet has no `O`).
    let body = code_symbols(&folded)?;
    let mapped: String = body
        .chars()
        .map(|c| match c {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        })
        .collect();
    if mapped.len() != BODY_LEN + 1 || !mapped.bytes().all(|b| ALPHABET.contains(&b)) {
        return None;
    }
    let (data, check) = mapped.split_at(BODY_LEN);
    check
        .starts_with(check_symbol(data))
        .then(|| format_code(data))
}

/// The system UUID of the first SMBIOS type-1 structure in `raw`, what
/// `GetSystemFirmwareTable('RSMB')` returns (`RawSMBIOSData`: 8 bytes of header, the table length at
/// byte 4, then the structures), in lower case as Windows shows it (SMBIOS 2.6: the first three
/// fields little-endian); `None` when the table has no UUID, is cut short or malformed, or holds
/// "not present" (all 0x00) or "not set" (all 0xFF).
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn smbios_system_uuid(raw: &[u8]) -> Option<String> {
    let declared = usize::try_from(u32::from_le_bytes(raw.get(4..8)?.try_into().ok()?)).ok()?;
    let rest = raw.get(8..)?;
    let table = rest.get(..declared.min(rest.len()))?;
    let mut at = 0;
    // Each structure: type, length of its formatted part (header included), handle; then its
    // strings, ended by two NULs.
    while let Some(&[kind, len, _, _]) = table.get(at..at + 4) {
        let len = usize::from(len);
        if len < 4 {
            return None;
        }
        match kind {
            1 => return uuid_text(table.get(at..at + len)?.get(8..24)?),
            127 => return None,
            _ => {}
        }
        let strings = table.get(at + len..)?;
        at += len + strings.windows(2).position(|w| w == [0, 0])? + 2;
    }
    None
}

/// 16 SMBIOS UUID bytes as `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`.
fn uuid_text(bytes: &[u8]) -> Option<String> {
    if bytes.iter().all(|b| *b == 0) || bytes.iter().all(|b| *b == 0xff) {
        return None;
    }
    const ORDER: [usize; 16] = [3, 2, 1, 0, 5, 4, 7, 6, 8, 9, 10, 11, 12, 13, 14, 15];
    let ordered = ORDER
        .iter()
        .map(|i| bytes.get(*i).copied())
        .collect::<Option<Vec<u8>>>()?;
    let hex = hex::encode(ordered);
    Some(format!(
        "{}-{}-{}-{}-{}",
        hex.get(0..8)?,
        hex.get(8..12)?,
        hex.get(12..16)?,
        hex.get(16..20)?,
        hex.get(20..32)?
    ))
}

/// The serial number of a `STORAGE_DEVICE_DESCRIPTOR` (`SerialNumberOffset` at byte 24, pointing
/// past the fixed fields to a NUL-terminated ASCII string), trimmed; `None` when the disk reports
/// none.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn descriptor_serial(descriptor: &[u8]) -> Option<String> {
    /// `RawDeviceProperties`: the first byte after the fixed fields.
    const FIXED_FIELDS: usize = 36;
    let offset =
        usize::try_from(u32::from_le_bytes(descriptor.get(24..28)?.try_into().ok()?)).ok()?;
    if offset < FIXED_FIELDS {
        return None; // 0: no serial number
    }
    let text = descriptor.get(offset..)?.split(|b| *b == 0).next()?;
    let serial = String::from_utf8_lossy(text).trim().to_owned();
    (!serial.is_empty()).then_some(serial)
}

#[cfg(windows)]
mod system {
    // Four FFI calls (GetVolumeInformationW, GetSystemFirmwareTable, CreateFileW, DeviceIoControl);
    // each states its contract at the call. None needs administrator rights.
    #![allow(unsafe_code)]

    use super::{Components, descriptor_serial, smbios_system_uuid};
    use crate::codes;
    use pos_core::error::AppError;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
        GetVolumeInformationW, OPEN_EXISTING,
    };
    use windows::Win32::System::IO::DeviceIoControl;
    use windows::Win32::System::Ioctl::{
        IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery, STORAGE_PROPERTY_QUERY,
        StorageDeviceProperty,
    };
    use windows::Win32::System::SystemInformation::{GetSystemFirmwareTable, RSMB};
    use windows::core::{Owned, PCWSTR};
    use windows_registry::LOCAL_MACHINE;

    /// A firmware table or a disk descriptor larger than this is not one.
    const MAX_READ: u32 = 1 << 20;

    fn unreadable() -> AppError {
        AppError::conflict(codes::DEVICE_UNREADABLE)
    }

    fn read(path: &str, name: &str) -> Option<String> {
        LOCAL_MACHINE.open(path).ok()?.get_string(name).ok()
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn components() -> Result<Components, AppError> {
        let machine_guid =
            read(r"SOFTWARE\Microsoft\Cryptography", "MachineGuid").ok_or_else(unreadable)?;
        let processor = r"HARDWARE\DESCRIPTION\System\CentralProcessor\0";
        let cpu = ["VendorIdentifier", "ProcessorNameString", "Identifier"]
            .into_iter()
            .map(|name| read(processor, name).unwrap_or_default())
            .collect::<Vec<_>>()
            .join("|");
        let product_id =
            read(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", "ProductId").unwrap_or_default();
        let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_owned());
        Ok(Components {
            machine_guid,
            volume_serial: volume_serial(&drive)?,
            cpu,
            product_id,
            // Absent the same way at every read (a firmware or a disk that reports none): empty.
            smbios_uuid: smbios_uuid().unwrap_or_default(),
            disk_serial: disk_serial(&drive).unwrap_or_default(),
        })
    }

    /// The motherboard's SMBIOS system UUID, from the firmware's SMBIOS table.
    fn smbios_uuid() -> Option<String> {
        // SAFETY: no buffer is passed; the call only returns the size the table needs.
        let size = unsafe { GetSystemFirmwareTable(RSMB, 0, None) };
        if size == 0 || size > MAX_READ {
            return None;
        }
        let mut raw = vec![0_u8; usize::try_from(size).ok()?];
        // SAFETY: `raw` is a live, exclusively borrowed buffer; the wrapper passes its pointer and
        // its length, and the call writes at most that many bytes.
        let written = unsafe { GetSystemFirmwareTable(RSMB, 0, Some(&mut raw)) };
        if written == 0 || written > size {
            return None;
        }
        raw.truncate(usize::try_from(written).ok()?);
        smbios_system_uuid(&raw)
    }

    /// The hardware serial number of the disk that holds `drive`: the volume opened without any
    /// access right (enough to query it, no administrator rights), then the storage device
    /// descriptor, its size asked first.
    fn disk_serial(drive: &str) -> Option<String> {
        let path = wide(&format!(r"\\.\{drive}"));
        // SAFETY: `path` is a NUL-terminated UTF-16 path that lives until the call returns; no
        // security attributes, no template file.
        let handle = unsafe {
            CreateFileW(
                PCWSTR(path.as_ptr()),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
        }
        .ok()?;
        // SAFETY: this function opened the handle and nothing else closes it: `Owned` closes it
        // once, when it is dropped.
        let handle = unsafe { Owned::new(handle) };
        let mut header = [0_u8; 8]; // STORAGE_DESCRIPTOR_HEADER { Version, Size }
        query_device(*handle, &mut header)?;
        let size = u32::from_le_bytes(header.get(4..8)?.try_into().ok()?);
        if !(8..=MAX_READ).contains(&size) {
            return None;
        }
        let mut descriptor = vec![0_u8; usize::try_from(size).ok()?];
        let written = query_device(*handle, &mut descriptor)?;
        descriptor.truncate(written);
        descriptor_serial(&descriptor)
    }

    /// `IOCTL_STORAGE_QUERY_PROPERTY` (the device descriptor) into `out`: the bytes written.
    fn query_device(handle: HANDLE, out: &mut [u8]) -> Option<usize> {
        let query = STORAGE_PROPERTY_QUERY {
            PropertyId: StorageDeviceProperty,
            QueryType: PropertyStandardQuery,
            ..Default::default()
        };
        let query_len = u32::try_from(size_of::<STORAGE_PROPERTY_QUERY>()).ok()?;
        let out_len = u32::try_from(out.len()).ok()?;
        let mut written: u32 = 0;
        // SAFETY: `handle` is open (the caller owns it) and was opened without
        // FILE_FLAG_OVERLAPPED, so no OVERLAPPED is needed; `query` is a live local of `query_len`
        // bytes the driver only reads; `out` is a live, exclusively borrowed buffer of `out_len`
        // bytes (a buffered IOCTL: the system copies at most that many bytes into it); `written` is
        // a live local.
        unsafe {
            DeviceIoControl(
                handle,
                IOCTL_STORAGE_QUERY_PROPERTY,
                Some(std::ptr::from_ref(&query).cast()),
                query_len,
                Some(out.as_mut_ptr().cast()),
                out_len,
                Some(&raw mut written),
                None,
            )
        }
        .ok()?;
        usize::try_from(written).ok().filter(|n| *n <= out.len())
    }

    fn volume_serial(drive: &str) -> Result<u32, AppError> {
        let root = wide(&format!("{drive}\\"));
        let mut serial: u32 = 0;
        // SAFETY: `root` is a NUL-terminated UTF-16 path that lives until the call returns; the only
        // out pointer is `serial`, a live local; every buffer argument is `None`.
        unsafe {
            GetVolumeInformationW(
                PCWSTR(root.as_ptr()),
                None,
                Some(&raw mut serial),
                None,
                None,
                None,
            )
        }
        .map_err(|_| unreadable())?;
        Ok(serial)
    }
}

#[cfg(not(windows))]
mod system {
    use super::Components;
    use pos_core::error::AppError;

    /// DZBuild POS runs on Windows only (spec §2).
    pub fn components() -> Result<Components, AppError> {
        Err(AppError::conflict(crate::codes::DEVICE_UNREADABLE))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Components {
        Components {
            machine_guid: "6F1C3A52-9D3E-4B7A-8C21-0E5D7F9A1B24".into(),
            volume_serial: 0x1A2B_3C4D,
            cpu: "GenuineIntel|Intel(R) Celeron(R) CPU  J1900  @ 1.99GHz|Intel64 Family 6 Model 55 Stepping 8"
                .into(),
            product_id: "00330-80000-00000-AA123".into(),
            smbios_uuid: "4c4c4544-0042-3510-8051-b4c04f4d4a32".into(),
            disk_serial: "S3Z1NB0K123456A".into(),
        }
    }

    #[test]
    fn the_code_is_stable_and_shaped_for_a_customer() {
        let first = DeviceId::from_components(&sample()).unwrap();
        let again = DeviceId::from_components(&sample()).unwrap();
        assert_eq!(first, again, "the same PC gives the same code");
        let code = first.code();
        assert_eq!(code.len(), "DZPOS-XXXX-XXXX-XXXX-XXXX".len(), "{code}");
        let groups: Vec<&str> = code.split('-').collect();
        assert_eq!(groups[0], PREFIX);
        assert_eq!(groups.len(), 5, "{code}");
        assert!(
            groups[1..]
                .iter()
                .all(|g| g.len() == 4 && g.bytes().all(|b| ALPHABET.contains(&b))),
            "{code}"
        );
        assert_eq!(parse_code(code).as_deref(), Some(code));
    }

    #[test]
    fn case_and_spacing_of_the_identities_do_not_change_the_code() {
        let mut loose = sample();
        loose.machine_guid = format!("  {}  ", loose.machine_guid.to_lowercase());
        loose.cpu = loose.cpu.replace("  ", " ");
        loose.product_id = format!(" {} ", loose.product_id.to_lowercase());
        assert_eq!(
            DeviceId::from_components(&loose).unwrap().code(),
            DeviceId::from_components(&sample()).unwrap().code()
        );
    }

    #[test]
    fn every_component_change_gives_another_code() {
        let base = DeviceId::from_components(&sample()).unwrap();
        let changes: [fn(&mut Components); 7] = [
            |c| c.machine_guid = "0f1c3a52-9d3e-4b7a-8c21-0e5d7f9a1b24".into(), // Windows reinstalled
            |c| c.volume_serial = 0x1A2B_3C4E,                                  // disk reformatted
            |c| c.cpu = "GenuineIntel|Intel(R) Celeron(R) N4020|Intel64 Family 6 Model 122".into(),
            |c| c.product_id = "00330-80000-00000-AA124".into(), // another Windows licence
            |c| c.smbios_uuid = "4c4c4544-0042-3510-8051-b4c04f4d4a33".into(), // another motherboard
            |c| c.smbios_uuid = String::new(), // a firmware without a UUID
            |c| c.disk_serial = "S3Z1NB0K123456B".into(), // the image copied to another disk
        ];
        for change in changes {
            let mut other = sample();
            change(&mut other);
            let id = DeviceId::from_components(&other).unwrap();
            assert_ne!(id.code(), base.code());
            assert_ne!(id.secret(), base.secret());
        }
    }

    #[test]
    fn the_secret_is_not_the_code_and_debug_prints_no_identity() {
        let id = DeviceId::from_components(&sample()).unwrap();
        let printed = format!("{id:?} {:?}", sample());
        assert!(
            !printed.contains(id.code()),
            "the device code is in {printed}"
        );
        assert!(!printed.contains(&hex::encode(id.secret())), "{printed}");
        for identity in [
            "6F1C3A52", "6f1c3a52", "1A2B3C4D", "J1900", "AA123", "4c4c4544", "S3Z1NB0K",
        ] {
            assert!(!printed.contains(identity), "{identity} in {printed}");
        }
        assert!(
            !id.code()
                .contains(&hex::encode(&id.secret()[..4]).to_uppercase())
        );
    }

    #[test]
    fn a_missing_machine_guid_is_unreadable() {
        let mut blank = sample();
        blank.machine_guid = "   ".into();
        assert_eq!(
            DeviceId::from_components(&blank).unwrap_err().code(),
            crate::codes::DEVICE_UNREADABLE
        );
    }

    #[test]
    fn a_typed_code_is_normalised_and_a_wrong_character_is_caught() {
        let code = DeviceId::from_components(&sample())
            .unwrap()
            .code()
            .to_owned();
        // read over the phone: lower case, spaces instead of dashes
        let typed = code.to_lowercase().replace('-', " ");
        assert_eq!(parse_code(&typed).as_deref(), Some(code.as_str()));
        // without the prefix, and with O for 0 / I or L for 1 (Crockford)
        let body = &code["DZPOS-".len()..];
        assert_eq!(parse_code(body).as_deref(), Some(code.as_str()));
        let confusable = body.replace('0', "O").replace('1', "l");
        assert_eq!(parse_code(&confusable).as_deref(), Some(code.as_str()));
        // One symbol wrong anywhere: the check symbol catches it every time (1000 random
        // codes and swapped symbols in `device_tests`). 16 positions × 31 replacements.
        let symbols: Vec<char> = body.chars().filter(|c| *c != '-').collect();
        assert_eq!(symbols.len(), 16);
        let (mut tried, mut caught) = (0, 0);
        for (position, &current) in symbols.iter().enumerate() {
            for &b in ALPHABET.iter() {
                let replacement = char::from(b);
                if replacement == current {
                    continue;
                }
                let mut wrong = symbols.clone();
                wrong[position] = replacement;
                tried += 1;
                if parse_code(&wrong.iter().collect::<String>()).is_none() {
                    caught += 1;
                }
            }
        }
        assert_eq!(tried, 16 * 31);
        assert_eq!(caught, tried);
        // final review: copied from an Arabic WhatsApp chat (direction marks, a zero-width space,
        // the Arabic letter mark, a soft hyphen), typed with typographic dashes, Arabic-Indic or
        // Persian digits, fullwidth letters, `DZP0S`, inside a sentence
        let arabic_digits: String = code
            .chars()
            .map(|c| match c.to_digit(10) {
                Some(d) => char::from_u32(0x0660 + d).unwrap(),
                None => c,
            })
            .collect();
        let persian_digits: String = code
            .chars()
            .map(|c| match c.to_digit(10) {
                Some(d) => char::from_u32(0x06f0 + d).unwrap(),
                None => c,
            })
            .collect();
        let fullwidth: String = code
            .chars()
            .map(|c| char::from_u32(u32::from(c) + 0xfee0).unwrap())
            .collect();
        for pasted in [
            format!("\u{200f}\u{061c}رمز الجهاز: \u{2066}{code}\u{2069}\u{200f} شكرا"),
            code.replace('-', "\u{2010}"),
            code.replacen('-', "\u{2013}", 2).replace('-', "\u{2014}"),
            code.replace('-', "\u{2212}"),
            format!("\u{200b}{}\u{00ad}", code.replace('-', " \u{200b}")),
            arabic_digits,
            persian_digits,
            fullwidth,
            code.replacen("DZPOS", "DZP0S", 1),
            format!("Bonjour, mon code est {code}. Merci !"),
            format!("code DZPOS (ancien) puis le bon : {}", code.to_lowercase()),
        ] {
            assert_eq!(
                parse_code(&pasted).as_deref(),
                Some(code.as_str()),
                "{pasted:?}"
            );
        }
        for junk in [
            "",
            "DZPOS",
            "DZPOS-XXXX",
            "hello world",
            &format!("{code}-0000"),
            "DZPOS-UUUU-UUUU-UUUU-UUUU",
        ] {
            assert_eq!(parse_code(junk), None, "{junk}");
        }
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "reads this PC's identity: cargo test -p pos-license this_pc_has_a_device_code -- --ignored"]
    fn this_pc_has_a_device_code() {
        let id = DeviceId::this_pc().unwrap();
        // `assert!`, not `assert_eq!`: a failure must not print this PC's code.
        assert!(
            parse_code(id.code()).as_deref() == Some(id.code()),
            "this PC's code does not parse back"
        );
        assert_eq!(
            DeviceId::this_pc().unwrap(),
            id,
            "stable from one read to the next"
        );
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "reads this PC's hardware (a physical PC): cargo test -p pos-license this_pc_has_hardware_identities -- --ignored"]
    fn this_pc_has_hardware_identities() {
        // Read as a standard user, twice; nothing of them is printed, even on a failure.
        let first = system::components().unwrap();
        let again = system::components().unwrap();
        assert!(
            first == again,
            "the identities change from one read to the next"
        );
        let uuid = &first.smbios_uuid;
        assert!(
            uuid.len() == 36 && uuid.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-'),
            "no SMBIOS system UUID (a length of {})",
            uuid.len()
        );
        // Optional: the UUID Windows shows (`(Get-CimInstance Win32_ComputerSystemProduct).UUID`)
        // in DZPOS_EXPECT_SMBIOS_UUID.
        if let Ok(expected) = std::env::var("DZPOS_EXPECT_SMBIOS_UUID") {
            assert!(
                expected.trim().to_ascii_lowercase() == *uuid,
                "not the UUID Windows shows"
            );
        }
        assert!(
            first.disk_serial.chars().any(|c| c.is_ascii_alphanumeric()),
            "no system disk serial number"
        );
    }

    #[test]
    fn the_check_weights_are_distinct_powers_of_alpha() {
        // α = x = 2; each weight is α times the next, the last is α¹, the check symbol's is α⁰.
        let mut power = 1_u8;
        for weight in CHECK_WEIGHTS.iter().rev() {
            power = gf32_mul(power, 2);
            assert_eq!(*weight, power);
        }
        let mut all: Vec<u8> = CHECK_WEIGHTS.to_vec();
        all.push(1);
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), BODY_LEN + 1, "16 distinct weights");
        assert!(all.iter().all(|w| (1..32).contains(w)));
        // GF(32): α has order 31, and every non-zero symbol has an inverse
        let order = (1..=31)
            .scan(1_u8, |p, _| {
                *p = gf32_mul(*p, 2);
                Some(*p)
            })
            .position(|p| p == 1);
        assert_eq!(order, Some(30));
        for a in 1..32 {
            assert!((1..32).any(|b| gf32_mul(a, b) == 1), "{a} has no inverse");
        }
    }
}

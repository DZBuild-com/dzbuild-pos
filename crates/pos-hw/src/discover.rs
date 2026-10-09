//! What is plugged in (ONB-009 wizard step, printer settings): Windows printers and COM ports.

use serde::Serialize;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum Discovered {
    WindowsPrinter {
        name: String,
        port: String,
        driver: String,
        is_default: bool,
        offline: bool,
        /// Name or driver looks like a receipt printer: listed first in the wizard.
        likely_receipt: bool,
        /// Name or driver looks like a label printer (TSPL/ZPL): listed next.
        likely_label: bool,
    },
    SerialPort {
        port: String,
        description: Option<String>,
    },
}

const RECEIPT_HINTS: [&str; 16] = [
    "pos",
    "receipt",
    "thermal",
    "text only",
    "xprinter",
    "xp-",
    "tm-",
    "epson tm",
    "rongta",
    "rp80",
    "rp58",
    "hprt",
    "sunmi",
    "58",
    "80mm",
    "smart pos",
];

const LABEL_HINTS: [&str; 13] = [
    "label",
    "etiquet",
    "xp-3",
    "xp-4",
    "xp-2",
    "xp-dt",
    "xp-tt",
    "tsc",
    "ttp-",
    "zdesigner",
    "zebra",
    "godex",
    "barcode",
];

pub fn likely_label_printer(name: &str, driver: &str) -> bool {
    let text = format!("{name} {driver}").to_lowercase();
    !text.contains("pdf") && LABEL_HINTS.iter().any(|hint| text.contains(hint))
}

/// A label printer (an XP-365B) is no receipt printer, whatever its name shares with one.
pub fn likely_receipt_printer(name: &str, driver: &str) -> bool {
    if likely_label_printer(name, driver) {
        return false;
    }
    let text = format!("{name} {driver}").to_lowercase();
    if text.contains("pdf")
        || text.contains("onenote")
        || text.contains("xps")
        || text.contains("fax")
    {
        return false;
    }
    RECEIPT_HINTS.iter().any(|hint| text.contains(hint))
}

/// Every printer and port found; a source that fails is logged and skipped.
pub fn discover() -> Vec<Discovered> {
    let mut found = Vec::new();
    #[cfg(windows)]
    match windows_printers::list() {
        Ok(printers) => found.extend(printers),
        Err(error) => tracing::warn!(%error, "could not list the Windows printers"),
    }
    match serialport::available_ports() {
        Ok(ports) => found.extend(ports.into_iter().map(|p| Discovered::SerialPort {
            description: match p.port_type {
                serialport::SerialPortType::UsbPort(usb) => usb.product.or(usb.manufacturer),
                serialport::SerialPortType::BluetoothPort => Some("Bluetooth".to_owned()),
                _ => None,
            },
            port: p.port_name,
        })),
        Err(error) => tracing::warn!(%error, "could not list the serial ports"),
    }
    found.sort_by_key(|d| match d {
        Discovered::WindowsPrinter {
            likely_receipt: true,
            ..
        } => 0,
        Discovered::WindowsPrinter {
            likely_label: true, ..
        } => 1,
        Discovered::WindowsPrinter { .. } => 2,
        Discovered::SerialPort { .. } => 3,
    });
    found
}

/// Printer name → driver name of the Windows printers (for `driver_names`); empty elsewhere.
pub fn windows_drivers() -> Vec<(String, String)> {
    #[cfg(windows)]
    match windows_printers::list() {
        Ok(printers) => printers
            .into_iter()
            .filter_map(|d| match d {
                Discovered::WindowsPrinter { name, driver, .. } => Some((name, driver)),
                Discovered::SerialPort { .. } => None,
            })
            .collect(),
        Err(error) => {
            tracing::warn!(%error, "could not list the Windows printers");
            Vec::new()
        }
    }
    #[cfg(not(windows))]
    Vec::new()
}

#[cfg(windows)]
mod windows_printers {
    #![allow(unsafe_code)] // FFI to winspool.drv; each unsafe block states its contract.

    use super::{Discovered, likely_label_printer, likely_receipt_printer};
    use crate::error::HwError;
    use windows::Win32::Graphics::Printing::{
        EnumPrintersW, GetDefaultPrinterW, PRINTER_ATTRIBUTE_WORK_OFFLINE,
        PRINTER_ENUM_CONNECTIONS, PRINTER_ENUM_LOCAL, PRINTER_INFO_2W,
    };
    use windows::core::{PCWSTR, PWSTR};

    fn text(p: PWSTR) -> String {
        if p.is_null() {
            return String::new();
        }
        // SAFETY: a non-null string pointer the spooler wrote into our live buffer.
        unsafe { p.to_string() }.unwrap_or_default()
    }

    fn default_printer() -> Option<String> {
        let mut len = 0_u32;
        // SAFETY: a size query without a buffer; `len` is a valid out pointer.
        let _ = unsafe { GetDefaultPrinterW(None, &mut len) };
        if len == 0 {
            return None;
        }
        let mut buffer = vec![0_u16; len as usize];
        // SAFETY: `buffer` holds `len` UTF-16 units.
        let ok =
            unsafe { GetDefaultPrinterW(Some(PWSTR(buffer.as_mut_ptr())), &mut len) }.as_bool();
        ok.then(|| String::from_utf16_lossy(&buffer[..(len as usize).saturating_sub(1)]))
    }

    pub(super) fn list() -> Result<Vec<Discovered>, HwError> {
        let flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;
        let (mut needed, mut returned) = (0_u32, 0_u32);
        // SAFETY: a size query without a buffer; out pointers are valid.
        let _ =
            unsafe { EnumPrintersW(flags, PCWSTR::null(), 2, None, &mut needed, &mut returned) };
        if needed == 0 {
            return Ok(Vec::new());
        }
        // u64 words keep the PRINTER_INFO_2W records at the start of the buffer aligned.
        let mut words = vec![0_u64; (needed as usize).div_ceil(8)];
        let bytes = words.len() * 8;
        {
            // SAFETY: `words` is a live allocation of `bytes` bytes borrowed only in this block.
            let buffer =
                unsafe { std::slice::from_raw_parts_mut(words.as_mut_ptr().cast::<u8>(), bytes) };
            // SAFETY: `buffer` is writable for its length; out pointers are valid.
            unsafe {
                EnumPrintersW(
                    flags,
                    PCWSTR::null(),
                    2,
                    Some(buffer),
                    &mut needed,
                    &mut returned,
                )
            }
            .map_err(|e| HwError::Io(e.to_string()))?;
        }
        let default = default_printer();
        // SAFETY: the call wrote `returned` PRINTER_INFO_2W records at the start of `words`,
        // which outlives `infos`.
        let infos = unsafe {
            std::slice::from_raw_parts(words.as_ptr().cast::<PRINTER_INFO_2W>(), returned as usize)
        };
        Ok(infos
            .iter()
            .map(|info| {
                let name = text(info.pPrinterName);
                let driver = text(info.pDriverName);
                Discovered::WindowsPrinter {
                    is_default: default.as_deref() == Some(name.as_str()),
                    offline: info.Attributes & PRINTER_ATTRIBUTE_WORK_OFFLINE != 0,
                    likely_receipt: likely_receipt_printer(&name, &driver),
                    likely_label: likely_label_printer(&name, &driver),
                    port: text(info.pPortName),
                    driver,
                    name,
                }
            })
            .filter(|d| matches!(d, Discovered::WindowsPrinter { name, .. } if !name.is_empty()))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipt_printers_are_recognised_by_name_or_driver() {
        for (name, driver) in [
            ("XP-80C", "XP-80"),
            ("POS-58", "POS58 Printer Driver"),
            ("Caisse", "Generic / Text Only"),
            ("EPSON TM-T20III Receipt", "EPSON TM-T20III Receipt5"),
            ("RONGTA RP80", "RP80"),
            ("Thermal Printer", "HPRT TP806"),
        ] {
            assert!(likely_receipt_printer(name, driver), "{name} / {driver}");
        }
        for (name, driver) in [
            ("Microsoft Print to PDF", "Microsoft Print To PDF"),
            ("HP LaserJet M404", "HP LaserJet M404 PCL-6"),
            ("OneNote (Desktop)", "Send to Microsoft OneNote 16 Driver"),
        ] {
            assert!(!likely_receipt_printer(name, driver), "{name} / {driver}");
        }
    }

    #[test]
    fn label_printers_are_recognised_by_name_or_driver() {
        for (name, driver) in [
            ("Xprinter XP-365B", "XP-365B"),
            ("Etiquettes", "TSC TTP-244 Pro"),
            ("ZDesigner GK420d", "ZDesigner GK420d"),
            ("Godex G500", "Godex G500"),
            ("Label", "Printer 4BARCODE"),
        ] {
            assert!(likely_label_printer(name, driver), "{name} / {driver}");
        }
        for (name, driver) in [
            ("XP-80C", "XP-80"),
            ("Microsoft Print to PDF", "Microsoft Print To PDF"),
        ] {
            assert!(!likely_label_printer(name, driver), "{name} / {driver}");
        }
    }

    #[test]
    fn a_label_printer_is_not_taken_for_a_receipt_printer() {
        assert!(!likely_receipt_printer("Xprinter XP-365B", "XP-365B"));
        assert!(!likely_receipt_printer("Etiquettes", "TSC TTP-244 Pro"));
    }

    #[test]
    fn discovery_never_fails_and_names_every_device() {
        for device in discover() {
            match device {
                Discovered::WindowsPrinter { name, .. } => assert!(!name.is_empty()),
                Discovered::SerialPort { port, .. } => assert!(!port.is_empty()),
            }
        }
        for (name, _driver) in windows_drivers() {
            assert!(!name.is_empty());
        }
    }

    #[test]
    fn devices_cross_the_ipc_tagged_by_kind() {
        let json = serde_json::to_value(Discovered::SerialPort {
            port: "COM3".into(),
            description: None,
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "kind": "serial_port", "port": "COM3", "description": null })
        );
    }
}

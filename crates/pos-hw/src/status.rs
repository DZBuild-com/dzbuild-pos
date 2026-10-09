//! Printer status (HW-008) from the Windows spooler or from the printer itself (`DLE EOT` over
//! TCP or COM), and the "printer doctor" (HW-009): a saved printer missing from Windows, a
//! driver Windows Update swapped, a queue set offline.

use crate::config::HardwareConfig;
use crate::discover::Discovered;
use crate::driver::Transport;
use serde::Serialize;
use specta::Type;

pub const PRINTER_STATUS_ERROR: u32 = 0x0000_0002;
pub const PRINTER_STATUS_PAPER_JAM: u32 = 0x0000_0008;
pub const PRINTER_STATUS_PAPER_OUT: u32 = 0x0000_0010;
pub const PRINTER_STATUS_PAPER_PROBLEM: u32 = 0x0000_0040;
pub const PRINTER_STATUS_OFFLINE: u32 = 0x0000_0080;
pub const PRINTER_STATUS_NOT_AVAILABLE: u32 = 0x0000_1000;
pub const PRINTER_STATUS_USER_INTERVENTION: u32 = 0x0010_0000;
pub const PRINTER_STATUS_DOOR_OPEN: u32 = 0x0040_0000;
pub const PRINTER_ATTRIBUTE_WORK_OFFLINE: u32 = 0x0000_0400;

/// `DLE EOT 1`, `2`, `3`, `4` in one request; four status bytes come back.
pub const DLE_EOT_ALL: [u8; 12] = [0x10, 0x04, 1, 0x10, 0x04, 2, 0x10, 0x04, 3, 0x10, 0x04, 4];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum StatusSource {
    Spooler,
    Device,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PrinterStatus {
    pub reachable: bool,
    pub paper_out: bool,
    pub paper_low: bool,
    pub cover_open: bool,
    pub error: bool,
    pub source: StatusSource,
}

impl PrinterStatus {
    /// A transport that cannot tell (a file printer): nothing known to be wrong.
    pub fn unknown() -> PrinterStatus {
        PrinterStatus {
            reachable: true,
            paper_out: false,
            paper_low: false,
            cover_open: false,
            error: false,
            source: StatusSource::Unknown,
        }
    }
}

/// `PRINTER_INFO_2W.Status` and `.Attributes` of a Windows printer.
pub fn from_spooler(status: u32, attributes: u32) -> PrinterStatus {
    let has = |bit: u32| status & bit != 0;
    PrinterStatus {
        reachable: !has(PRINTER_STATUS_OFFLINE)
            && !has(PRINTER_STATUS_NOT_AVAILABLE)
            && attributes & PRINTER_ATTRIBUTE_WORK_OFFLINE == 0,
        paper_out: has(PRINTER_STATUS_PAPER_OUT),
        paper_low: false,
        cover_open: has(PRINTER_STATUS_DOOR_OPEN),
        error: has(PRINTER_STATUS_ERROR)
            || has(PRINTER_STATUS_PAPER_JAM)
            || has(PRINTER_STATUS_PAPER_PROBLEM)
            || has(PRINTER_STATUS_USER_INTERVENTION),
        source: StatusSource::Spooler,
    }
}

/// Epson real-time status: n=1 bit 3 offline; n=2 bit 2 cover open, bit 5 paper end stop, bit 6
/// error; n=3 bits 2, 3, 5, 6 errors; n=4 bits 2–3 paper near end, bits 5–6 paper end.
pub fn from_dle_eot(bytes: &[u8; 4]) -> PrinterStatus {
    let [printer, offline, error, paper] = *bytes;
    PrinterStatus {
        reachable: printer & 0x08 == 0,
        paper_out: paper & 0x60 != 0 || offline & 0x20 != 0,
        paper_low: paper & 0x0C != 0,
        cover_open: offline & 0x04 != 0,
        error: offline & 0x40 != 0 || error & 0x6C != 0,
        source: StatusSource::Device,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum PrinterIssueKind {
    Missing,
    DriverChanged { saved: String, now: String },
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PrinterIssue {
    pub printer_id: String,
    pub printer_name: String,
    pub issue: PrinterIssueKind,
}

/// The saved Windows printers (receipt and label) against what Windows lists now.
pub fn doctor(config: &HardwareConfig, found: &[Discovered]) -> Vec<PrinterIssue> {
    let saved = config
        .printers
        .iter()
        .map(|p| (&p.id, &p.name, &p.transport))
        .chain(
            config
                .label_printers
                .iter()
                .map(|p| (&p.id, &p.name, &p.transport)),
        );
    let mut issues = Vec::new();
    for (id, name, transport) in saved {
        let Transport::Windows { printer_name } = transport else {
            continue;
        };
        let now = found.iter().find_map(|d| match d {
            Discovered::WindowsPrinter {
                name,
                driver,
                offline,
                ..
            } if name == printer_name.trim() => Some((driver, *offline)),
            _ => None,
        });
        let issue = match (now, config.driver_names.get(id)) {
            (None, _) => Some(PrinterIssueKind::Missing),
            (Some((driver, _)), Some(saved)) if saved != driver => {
                Some(PrinterIssueKind::DriverChanged {
                    saved: saved.clone(),
                    now: driver.clone(),
                })
            }
            (Some((_, true)), _) => Some(PrinterIssueKind::Offline),
            (Some(_), _) => None,
        };
        if let Some(issue) = issue {
            issues.push(PrinterIssue {
                printer_id: id.clone(),
                printer_name: name.clone(),
                issue,
            });
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{HardwareConfig, PrinterConfig};
    use crate::discover::Discovered;
    use crate::driver::Transport;
    use crate::profile::{PaperWidth, Preset, Profile};

    #[test]
    fn spooler_bits_and_dle_eot_bytes_become_a_status() {
        let fine = from_spooler(0, 0);
        assert!(fine.reachable && !fine.paper_out && fine.source == StatusSource::Spooler);
        assert!(from_spooler(PRINTER_STATUS_PAPER_OUT, 0).paper_out);
        assert!(!from_spooler(PRINTER_STATUS_OFFLINE, 0).reachable);
        assert!(!from_spooler(0, PRINTER_ATTRIBUTE_WORK_OFFLINE).reachable);
        assert!(from_spooler(PRINTER_STATUS_DOOR_OPEN, 0).cover_open);
        let ok = from_dle_eot(&[0x12, 0x12, 0x12, 0x12]);
        assert!(
            ok.reachable && !ok.paper_out && !ok.cover_open && ok.source == StatusSource::Device
        );
        let out = from_dle_eot(&[0x1A, 0x36, 0x12, 0x72]);
        assert!(!out.reachable && out.cover_open && out.paper_out);
        assert!(from_dle_eot(&[0x12, 0x12, 0x12, 0x1E]).paper_low);
    }

    #[test]
    fn the_doctor_finds_missing_printers_changed_drivers_and_offline_queues() {
        let windows = |id: &str, name: &str| PrinterConfig {
            id: id.into(),
            name: name.into(),
            transport: Transport::Windows {
                printer_name: name.into(),
            },
            profile: Profile::preset(Preset::Xprinter, PaperWidth::Mm80),
            mode: crate::config::PrintMode::Raw,
            paper_width_mm: 80,
        };
        let mut config = HardwareConfig {
            printers: vec![
                windows("p1", "XP-80C"),
                windows("p2", "POS-58"),
                windows("p3", "Caisse 2"),
            ],
            ..HardwareConfig::default()
        };
        config.driver_names.insert("p1".into(), "XP-80".into());
        config.driver_names.insert("p3".into(), "XP-58".into());
        let found = vec![
            Discovered::WindowsPrinter {
                name: "XP-80C".into(),
                port: "USB001".into(),
                driver: "Generic / Text Only".into(),
                is_default: false,
                offline: false,
                likely_receipt: true,
                likely_label: false,
            },
            Discovered::WindowsPrinter {
                name: "Caisse 2".into(),
                port: "USB002".into(),
                driver: "XP-58".into(),
                is_default: false,
                offline: true,
                likely_receipt: true,
                likely_label: false,
            },
        ];
        let issues = doctor(&config, &found);
        let kinds: Vec<(&str, &PrinterIssueKind)> = issues
            .iter()
            .map(|i| (i.printer_id.as_str(), &i.issue))
            .collect();
        assert_eq!(
            kinds,
            [
                (
                    "p1",
                    &PrinterIssueKind::DriverChanged {
                        saved: "XP-80".into(),
                        now: "Generic / Text Only".into()
                    }
                ),
                ("p2", &PrinterIssueKind::Missing),
                ("p3", &PrinterIssueKind::Offline),
            ]
        );
    }

    #[test]
    fn the_doctor_checks_label_printers_and_leaves_other_transports_alone() {
        let mut config = HardwareConfig::default();
        config.printers.push(PrinterConfig {
            id: "p1".into(),
            name: "Caisse réseau".into(),
            transport: Transport::Tcp {
                host: "192.168.1.50".into(),
                port: 9100,
            },
            profile: Profile::preset(Preset::Xprinter, PaperWidth::Mm80),
            mode: crate::config::PrintMode::Raw,
            paper_width_mm: 80,
        });
        config
            .label_printers
            .push(crate::config::LabelPrinterConfig {
                id: "l1".into(),
                name: "Étiquettes".into(),
                transport: Transport::Windows {
                    printer_name: "XP-365B".into(),
                },
                profile: crate::labels::LabelProfile::preset(crate::labels::LabelPreset::Xprinter),
            });
        let issues = doctor(&config, &[]);
        assert_eq!(
            issues,
            [PrinterIssue {
                printer_id: "l1".into(),
                printer_name: "Étiquettes".into(),
                issue: PrinterIssueKind::Missing
            }]
        );
        let json = serde_json::to_value(&issues[0]).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "printerId": "l1", "printerName": "Étiquettes", "issue": { "kind": "missing" } })
        );
        let changed = serde_json::to_value(PrinterIssueKind::DriverChanged {
            saved: "a".into(),
            now: "b".into(),
        })
        .unwrap();
        assert_eq!(
            changed,
            serde_json::json!({ "kind": "driver_changed", "saved": "a", "now": "b" })
        );
    }
}

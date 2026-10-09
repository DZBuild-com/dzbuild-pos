//! Per-PC hardware settings in `%APPDATA%\dev.dzbuild.pos\hardware.json` (printers belong to the
//! PC, not to the shop database): receipt printers, receipt options, scanners; since version 2
//! (M4) label printers, the scale, the customer displays and the drawer adapter; the A4/A5
//! document printers per kind (M3, `documents`); since version 3 (M8) the kitchen printers per
//! station and the printers whose Windows driver only takes pages. An older file loads with every
//! new section off.

use crate::display::VfdConfig;
use crate::driver::Transport;
use crate::error::HwError;
use crate::labels::LabelProfile;
use crate::profile::Profile;
use crate::scale::ScaleConfig;
use crate::scanner::SerialScannerConfig;
use crate::serial::BAUDS;
use pos_core::billing::settings::Paper;
use pos_core::documents::DocType;
use pos_core::lang::Lang;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, HashSet};
use std::io::Write;
use std::path::Path;

pub const CONFIG_VERSION: u32 = 3;
const NAME_MAX: usize = 60;
const ID_MAX: usize = 64;
/// The most copies of a kitchen ticket a station prints.
const KITCHEN_COPIES: std::ops::RangeInclusive<u8> = 1..=3;
/// The roll widths a page-only printer prints the receipt on.
const ROLL_WIDTHS_MM: [u16; 2] = [58, 80];

/// Raw ESC/POS through the transport, or a Windows driver that only prints pages (HW-002).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PrintMode {
    #[default]
    Raw,
    Driver,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PrinterConfig {
    pub id: String,
    pub name: String,
    pub transport: Transport,
    pub profile: Profile,
    /// HW-002: `Driver` for a Windows printer that refuses raw data; the app then prints the
    /// receipt as a page of the roll through the driver.
    #[serde(default)]
    pub mode: PrintMode,
    /// The roll a `Driver` printer prints the page on: 58 or 80 mm.
    #[serde(default = "default_width")]
    pub paper_width_mm: u16,
}

fn default_width() -> u16 {
    80
}

/// HW-004: the printer of a preparation station on this PC (a network kitchen printer on TCP
/// 9100 is shared by every till; each till maps the shop's stations to the printers it reaches).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StationPrinter {
    pub station_id: String,
    pub printer_id: String,
    /// 1 to 3 copies of each kitchen ticket.
    pub copies: u8,
    /// The printer's buzzer after each copy (kitchen models).
    pub buzzer: bool,
}

/// RST-006: where the kitchen tickets of each station print; an unmapped station prints on the
/// fallback, else on the receipt printer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct KitchenRouting {
    pub stations: Vec<StationPrinter>,
    pub fallback_printer_id: Option<String>,
}

impl KitchenRouting {
    /// Every route names a configured receipt printer (`printers`), once per station, with 1 to
    /// 3 copies.
    fn validate(&self, printers: &[PrinterConfig]) -> Result<(), HwError> {
        let known = |id: &str| printers.iter().any(|p| p.id == id);
        let mut stations = HashSet::new();
        for (i, route) in self.stations.iter().enumerate() {
            let at = |field: &str| HwError::invalid(&format!("kitchen.stations.{i}.{field}"));
            let station = route.station_id.trim();
            if station.is_empty() || station.len() > ID_MAX || !stations.insert(station) {
                return Err(at("stationId"));
            }
            if !known(&route.printer_id) {
                return Err(at("printerId"));
            }
            if !KITCHEN_COPIES.contains(&route.copies) {
                return Err(at("copies"));
            }
        }
        if let Some(id) = &self.fallback_printer_id
            && !known(id)
        {
            return Err(HwError::invalid("kitchen.fallbackPrinterId"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LabelPrinterConfig {
    pub id: String,
    pub name: String,
    pub transport: Transport,
    pub profile: LabelProfile,
}

/// What the scanner sends: most scanners emulate a US keyboard; some are set to AZERTY.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum ScannerLayout {
    #[default]
    Us,
    Azerty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct ReceiptOptions {
    /// The payment dialog's "print the receipt" box starts ticked (SAL-052).
    pub print_by_default: bool,
    /// Kick the drawer when cash moves (SAL-053).
    pub open_drawer_on_cash: bool,
    /// Kick before printing instead of after the cut (spec §9.1).
    pub drawer_before_print: bool,
    /// Print the shop logo at the top (DOC-014).
    pub logo: bool,
    /// Bilingual receipts: the second language's words after each label (UX-042).
    pub second_lang: Option<Lang>,
}

impl Default for ReceiptOptions {
    fn default() -> Self {
        ReceiptOptions {
            print_by_default: true,
            open_drawer_on_cash: true,
            drawer_before_print: false,
            logo: true,
            second_lang: None,
        }
    }
}

/// How a keyboard-wedge scanner ends a code: Enter or Tab (most), or nothing (the scan ends after
/// a short silence).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum HidSuffix {
    #[default]
    EnterOrTab,
    Silence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ScannerOptions {
    pub layout: ScannerLayout,
    /// Characters a wedge scanner types before each code (removed from the scan).
    pub prefix: String,
    pub suffix: HidSuffix,
    /// A scanner in USB-COM mode.
    pub serial: Option<SerialScannerConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct CustomerScreenConfig {
    pub enabled: bool,
    /// The monitor's name as Windows reports it (`\\.\DISPLAY2`); `None`: the first one that is
    /// not the main window's.
    pub monitor: Option<String>,
    pub show_logo: bool,
    /// How long "Merci / شكراً / Thank you" stays after a sale.
    pub thanks_seconds: u8,
}

impl Default for CustomerScreenConfig {
    fn default() -> Self {
        CustomerScreenConfig {
            enabled: false,
            monitor: None,
            show_logo: true,
            thanks_seconds: 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct DisplayConfig {
    pub vfd: Option<VfdConfig>,
    pub screen: CustomerScreenConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SerialKick {
    /// `ESC p 0 25 250` on the port (adapters that emulate a printer, printers on COM).
    EscPos,
    /// DTR raised for 200 ms (trigger-only adapters).
    Dtr,
}

/// HW-040/041: through the receipt printer (M1b) or a USB/COM drawer adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum DrawerConfig {
    #[default]
    Printer,
    Serial {
        port: String,
        baud: u32,
        kick: SerialKick,
    },
}

/// Where an A4/A5 document of one kind prints on this PC (spec §9.2: printer and paper per
/// document type).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KindPrinting {
    pub doc_type: DocType,
    #[serde(default)]
    pub printer: Option<String>,
    #[serde(default)]
    pub paper: Paper,
    #[serde(default = "one")]
    pub copies: u8,
}

fn one() -> u8 {
    1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct DocumentPrinting {
    /// A Windows printer name; the Windows default printer when `None`.
    pub default_printer: Option<String>,
    pub kinds: Vec<KindPrinting>,
    pub margins_mm: u8,
}

impl Default for DocumentPrinting {
    fn default() -> Self {
        DocumentPrinting {
            default_printer: None,
            kinds: Vec::new(),
            margins_mm: 12,
        }
    }
}

impl DocumentPrinting {
    /// Printer, paper and copies for `doc_type`; `default_paper` is the shop's layout setting.
    pub fn for_kind(&self, doc_type: DocType, default_paper: Paper) -> (Option<String>, Paper, u8) {
        match self.kinds.iter().find(|k| k.doc_type == doc_type) {
            Some(k) => (
                k.printer.clone().or_else(|| self.default_printer.clone()),
                k.paper,
                k.copies,
            ),
            None => (self.default_printer.clone(), default_paper, 1),
        }
    }

    fn validate(&self) -> Result<(), HwError> {
        let long = |p: &Option<String>| {
            p.as_deref()
                .is_some_and(|n| n.trim().is_empty() || n.chars().count() > 200)
        };
        if long(&self.default_printer) {
            return Err(HwError::invalid("documents.defaultPrinter"));
        }
        if self.margins_mm > 25 {
            return Err(HwError::invalid("documents.marginsMm"));
        }
        let mut seen = HashSet::new();
        for (i, k) in self.kinds.iter().enumerate() {
            if !seen.insert(k.doc_type) || long(&k.printer) || !(1..=5).contains(&k.copies) {
                return Err(HwError::invalid(&format!("documents.kinds.{i}")));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct HardwareConfig {
    pub version: u32,
    pub printers: Vec<PrinterConfig>,
    pub receipt_printer_id: Option<String>,
    pub receipt: ReceiptOptions,
    pub scanner: ScannerOptions,
    pub label_printers: Vec<LabelPrinterConfig>,
    pub label_printer_id: Option<String>,
    pub scale: Option<ScaleConfig>,
    pub display: DisplayConfig,
    pub drawer: DrawerConfig,
    /// Printer id → Windows driver name when last saved (HW-009 "printer doctor").
    pub driver_names: BTreeMap<String, String>,
    /// A4/A5 documents (M3): the Windows printer, paper and copies per kind.
    pub documents: DocumentPrinting,
    /// RST-006, HW-004 (M8): the kitchen printers of this PC per preparation station.
    pub kitchen: KitchenRouting,
}

impl Default for HardwareConfig {
    fn default() -> Self {
        HardwareConfig {
            version: CONFIG_VERSION,
            printers: Vec::new(),
            receipt_printer_id: None,
            receipt: ReceiptOptions::default(),
            scanner: ScannerOptions::default(),
            label_printers: Vec::new(),
            label_printer_id: None,
            scale: None,
            display: DisplayConfig::default(),
            drawer: DrawerConfig::Printer,
            driver_names: BTreeMap::new(),
            documents: DocumentPrinting::default(),
            kitchen: KitchenRouting::default(),
        }
    }
}

impl HardwareConfig {
    /// What `hardware_save` accepts: every section, and one device per serial port.
    pub fn validate(&self) -> Result<(), HwError> {
        self.validate_sections()?;
        self.check_ports()
    }

    /// Every section on its own (a file saved before the one-owner-per-port rule loads with it:
    /// it is refused only when saved again, never set aside with the shop's printers).
    fn validate_sections(&self) -> Result<(), HwError> {
        let mut ids = HashSet::new();
        for (i, printer) in self.printers.iter().enumerate() {
            let id = printer.id.trim();
            if id.is_empty() || id.len() > ID_MAX || !ids.insert(id) {
                return Err(HwError::invalid(&format!("printers.{i}.id")));
            }
            let name = printer.name.trim();
            if name.is_empty() || name.chars().count() > NAME_MAX {
                return Err(HwError::invalid(&format!("printers.{i}.name")));
            }
            printer
                .transport
                .validate()
                .map_err(|e| e.within(&format!("printers.{i}")))?;
            printer
                .profile
                .validate()
                .map_err(|e| e.within(&format!("printers.{i}")))?;
            if !ROLL_WIDTHS_MM.contains(&printer.paper_width_mm) {
                return Err(HwError::invalid(&format!("printers.{i}.paperWidthMm")));
            }
            // HW-002: only a printer installed in Windows has a driver to print pages through
            if printer.mode == PrintMode::Driver
                && !matches!(printer.transport, Transport::Windows { .. })
            {
                return Err(HwError::invalid(&format!("printers.{i}.mode")));
            }
        }
        // Checked before the label printers join `ids`: a label printer prints no receipt.
        if let Some(id) = &self.receipt_printer_id
            && !ids.contains(id.as_str())
        {
            return Err(HwError::invalid("receiptPrinterId"));
        }
        // Kitchen tickets are receipts: a label printer prints none either.
        self.kitchen.validate(&self.printers)?;
        for (i, printer) in self.label_printers.iter().enumerate() {
            let at = |field: &str| HwError::invalid(&format!("labelPrinters.{i}.{field}"));
            let id = printer.id.trim();
            if id.is_empty() || id.len() > ID_MAX || !ids.insert(id) {
                return Err(at("id"));
            }
            let name = printer.name.trim();
            if name.is_empty() || name.chars().count() > NAME_MAX {
                return Err(at("name"));
            }
            printer
                .transport
                .validate()
                .map_err(|e| e.within(&format!("labelPrinters.{i}")))?;
            printer
                .profile
                .validate()
                .map_err(|e| e.within(&format!("labelPrinters.{i}")))?;
        }
        if let Some(id) = &self.label_printer_id
            && !self.label_printers.iter().any(|p| &p.id == id)
        {
            return Err(HwError::invalid("labelPrinterId"));
        }
        if let Some(scale) = &self.scale {
            scale.validate().map_err(|e| e.within("scale"))?;
        }
        if let Some(vfd) = &self.display.vfd {
            vfd.validate().map_err(|e| e.within("display.vfd"))?;
        }
        let screen = &self.display.screen;
        if !(2..=30).contains(&screen.thanks_seconds) {
            return Err(HwError::setting("display.screen.thanksSeconds"));
        }
        if screen
            .monitor
            .as_ref()
            .is_some_and(|m| m.chars().count() > 128)
        {
            return Err(HwError::setting("display.screen.monitor"));
        }
        if self.scanner.prefix.len() > 8
            || !self
                .scanner
                .prefix
                .bytes()
                .all(|b| (0x20..0x7F).contains(&b))
        {
            return Err(HwError::setting("scanner.prefix"));
        }
        if let Some(serial) = &self.scanner.serial {
            serial.validate().map_err(|e| e.within("scanner.serial"))?;
        }
        if let DrawerConfig::Serial { port, baud, .. } = &self.drawer {
            if port.trim().is_empty() || port.len() > 64 {
                return Err(HwError::setting("drawer.port"));
            }
            if !BAUDS.contains(baud) {
                return Err(HwError::setting("drawer.baud"));
            }
        }
        self.documents.validate()
    }

    /// One owner per serial port (M4): two devices never share a COM port, whatever its case or
    /// spaces. The second one that names it is refused, by its field (`drawer.port`…).
    fn check_ports(&self) -> Result<(), HwError> {
        let mut claims: Vec<(Option<&str>, String)> = Vec::new();
        for (i, p) in self.printers.iter().enumerate() {
            if let Transport::Serial { port, .. } = &p.transport {
                claims.push((Some(port), format!("printers.{i}.transport.port")));
            }
        }
        for (i, p) in self.label_printers.iter().enumerate() {
            if let Transport::Serial { port, .. } = &p.transport {
                claims.push((Some(port), format!("labelPrinters.{i}.transport.port")));
            }
        }
        claims.push((
            self.scale.as_ref().map(|s| s.serial.port.as_str()),
            "scale.serial.port".into(),
        ));
        claims.push((
            self.display.vfd.as_ref().map(|v| v.serial.port.as_str()),
            "display.vfd.serial.port".into(),
        ));
        claims.push((
            self.scanner.serial.as_ref().map(|s| s.serial.port.as_str()),
            "scanner.serial.serial.port".into(),
        ));
        if let DrawerConfig::Serial { port, .. } = &self.drawer {
            claims.push((Some(port), "drawer.port".into()));
        }
        let mut taken: HashSet<String> = HashSet::new();
        for (port, field) in claims {
            if let Some(port) = port
                && !taken.insert(crate::serial::port_key(port))
            {
                return Err(HwError::setting(&field));
            }
        }
        Ok(())
    }

    pub fn receipt_printer(&self) -> Option<&PrinterConfig> {
        let id = self.receipt_printer_id.as_deref()?;
        self.printers.iter().find(|p| p.id == id)
    }

    /// The saved settings, or the defaults when the file does not exist yet.
    pub fn load(path: &Path) -> Result<HardwareConfig, HwError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(HardwareConfig::default());
            }
            Err(e) => return Err(HwError::Config(e.to_string())),
        };
        let mut config: HardwareConfig =
            serde_json::from_str(&text).map_err(|e| HwError::Config(e.to_string()))?;
        config
            .validate_sections()
            .map_err(|e| HwError::Config(e.to_string()))?;
        // An older file, its new sections defaulted, is a current one now: it saves as such.
        config.version = CONFIG_VERSION;
        Ok(config)
    }

    /// Validated, written to `<path>.tmp`, flushed to disk, then renamed over `path`: a power
    /// cut leaves either the old or the new file.
    pub fn save(&self, path: &Path) -> Result<(), HwError> {
        self.validate()?;
        let text =
            serde_json::to_string_pretty(self).map_err(|e| HwError::Config(e.to_string()))?;
        let temp = path.with_extension("json.tmp");
        let io = |e: std::io::Error| HwError::Io(e.to_string());
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        let mut file = std::fs::File::create(&temp).map_err(io)?;
        file.write_all(text.as_bytes()).map_err(io)?;
        file.sync_all().map_err(io)?;
        drop(file);
        std::fs::rename(&temp, path).map_err(io)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::Transport;
    use crate::profile::{PaperWidth, Preset, Profile};

    fn printer(id: &str, path: &str) -> PrinterConfig {
        PrinterConfig {
            id: id.into(),
            name: "Caisse 1".into(),
            transport: Transport::File { path: path.into() },
            profile: Profile::preset(Preset::Generic, PaperWidth::Mm80),
            mode: PrintMode::Raw,
            paper_width_mm: 80,
        }
    }

    #[test]
    fn defaults_are_valid_and_print_by_default() {
        let config = HardwareConfig::default();
        config.validate().unwrap();
        assert!(
            config.receipt.print_by_default
                && config.receipt.open_drawer_on_cash
                && config.receipt.logo
        );
        assert_eq!(config.scanner.layout, ScannerLayout::Us);
        assert!(config.receipt_printer().is_none());
    }

    #[test]
    fn a_missing_file_gives_the_defaults_and_a_saved_file_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hardware.json");
        assert_eq!(
            HardwareConfig::load(&path).unwrap(),
            HardwareConfig::default()
        );
        let config = HardwareConfig {
            printers: vec![printer("p1", "out.bin")],
            receipt_printer_id: Some("p1".into()),
            ..HardwareConfig::default()
        };
        config.save(&path).unwrap();
        assert!(
            !path.with_extension("json.tmp").exists(),
            "written through a temp file then renamed"
        );
        assert_eq!(HardwareConfig::load(&path).unwrap(), config);
        assert_eq!(
            HardwareConfig::load(&path)
                .unwrap()
                .receipt_printer()
                .map(|p| p.id.as_str()),
            Some("p1")
        );
    }

    #[test]
    fn unknown_fields_are_ignored_and_missing_ones_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hardware.json");
        std::fs::write(&path, r#"{"version":1,"printers":[],"futureThing":true}"#).unwrap();
        let config = HardwareConfig::load(&path).unwrap();
        assert!(config.receipt.print_by_default);
    }

    #[test]
    fn broken_or_inconsistent_files_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hardware.json");
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(
            HardwareConfig::load(&path).unwrap_err().code(),
            "hardware_config_invalid"
        );
        let field = |c: HardwareConfig| c.validate().unwrap_err().field().map(str::to_owned);
        let two = HardwareConfig {
            printers: vec![printer("p1", "a"), printer("p1", "b")],
            ..HardwareConfig::default()
        };
        assert_eq!(field(two).as_deref(), Some("printers.1.id"));
        let dangling = HardwareConfig {
            receipt_printer_id: Some("nope".into()),
            ..HardwareConfig::default()
        };
        assert_eq!(field(dangling).as_deref(), Some("receiptPrinterId"));
        let mut unnamed = printer("p1", "a");
        unnamed.name = "  ".into();
        assert_eq!(
            field(HardwareConfig {
                printers: vec![unnamed],
                ..HardwareConfig::default()
            })
            .as_deref(),
            Some("printers.0.name")
        );
    }

    #[test]
    fn the_config_crosses_the_ipc_in_camel_case() {
        let json = serde_json::to_value(HardwareConfig::default()).unwrap();
        assert_eq!(json["receipt"]["printByDefault"], true);
        assert_eq!(json["scanner"]["layout"], "us");
        assert_eq!(json["receiptPrinterId"], serde_json::Value::Null);
    }

    #[test]
    fn a_version_1_file_loads_with_the_new_sections_off() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hardware.json");
        std::fs::write(
            &path,
            r#"{"version":1,"printers":[],"receiptPrinterId":null,"receipt":{"printByDefault":true},"scanner":{"layout":"azerty"}}"#,
        )
        .unwrap();
        let config = HardwareConfig::load(&path).unwrap();
        assert_eq!(config.scanner.layout, ScannerLayout::Azerty);
        assert_eq!(
            (config.scanner.prefix.as_str(), config.scanner.suffix),
            ("", HidSuffix::EnterOrTab)
        );
        assert!(
            config.label_printers.is_empty()
                && config.scale.is_none()
                && config.display.vfd.is_none()
        );
        assert!(!config.display.screen.enabled);
        assert_eq!(config.drawer, DrawerConfig::Printer);
        config.save(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"version\": 3"), "saved as version 3");
    }

    /// M4: one owner per serial port. The pole display and a drawer adapter (or a scale, a
    /// serial scanner, a COM printer) on one port: the second is refused by its own field. A file
    /// saved before the rule still loads, its printers kept.
    #[test]
    fn two_devices_never_share_a_serial_port() {
        let display = DisplayConfig {
            vfd: Some(crate::display::VfdConfig::new("COM3")),
            screen: CustomerScreenConfig::default(),
        };
        let mut config = HardwareConfig {
            display,
            drawer: DrawerConfig::Serial {
                port: " com3".into(),
                baud: 9_600,
                kick: SerialKick::EscPos,
            },
            ..HardwareConfig::default()
        };
        let field = |c: &HardwareConfig| c.validate().unwrap_err().field().map(str::to_owned);
        let refused = config.validate().unwrap_err();
        assert_eq!(refused.code(), "hardware_setting_invalid");
        assert_eq!(field(&config).as_deref(), Some("drawer.port"));
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("hardware.json");
        std::fs::write(&file, serde_json::to_string(&config).unwrap()).unwrap();
        assert_eq!(HardwareConfig::load(&file).unwrap().drawer, config.drawer);
        config.drawer = DrawerConfig::Serial {
            port: "COM4".into(),
            baud: 9_600,
            kick: SerialKick::EscPos,
        };
        config.validate().unwrap();
        config.scale = Some(crate::scale::ScaleConfig::new(
            crate::scale::ScaleProtocol::CasEcr,
            "Com4",
        ));
        assert_eq!(
            field(&config).as_deref(),
            Some("drawer.port"),
            "the scale took COM4 first"
        );
        config.scale = None;
        config.scanner.serial = Some(crate::scanner::SerialScannerConfig::new("COM3"));
        assert_eq!(
            field(&config).as_deref(),
            Some("scanner.serial.serial.port")
        );
        config.scanner.serial = None;
        config.printers.push(PrinterConfig {
            id: "p1".into(),
            name: "Caisse".into(),
            transport: Transport::Serial {
                port: "COM4".into(),
                baud: 9_600,
            },
            profile: Profile::preset(Preset::Generic, PaperWidth::Mm80),
            mode: PrintMode::Raw,
            paper_width_mm: 80,
        });
        assert_eq!(field(&config).as_deref(), Some("drawer.port"));
    }

    #[test]
    fn label_printer_ids_are_unique_across_every_printer() {
        let mut config = HardwareConfig {
            printers: vec![printer("p1", "a.bin")],
            ..HardwareConfig::default()
        };
        config.label_printers.push(LabelPrinterConfig {
            id: "p1".into(),
            name: "Étiquettes".into(),
            transport: Transport::File {
                path: "b.bin".into(),
            },
            profile: crate::labels::LabelProfile::preset(crate::labels::LabelPreset::Xprinter),
        });
        assert_eq!(
            config.validate().unwrap_err().field(),
            Some("labelPrinters.0.id")
        );
        config.label_printers[0].id = "l1".into();
        config.label_printer_id = Some("l1".into());
        config.validate().unwrap();
        config.label_printer_id = Some("p1".into());
        assert_eq!(
            config.validate().unwrap_err().field(),
            Some("labelPrinterId"),
            "a receipt printer is no label printer"
        );
    }

    #[test]
    fn a_label_printer_is_no_receipt_printer() {
        let mut config = HardwareConfig::default();
        config.label_printers.push(LabelPrinterConfig {
            id: "l1".into(),
            name: "Étiquettes".into(),
            transport: Transport::File {
                path: "b.bin".into(),
            },
            profile: crate::labels::LabelProfile::preset(crate::labels::LabelPreset::Xprinter),
        });
        config.receipt_printer_id = Some("l1".into());
        assert_eq!(
            config.validate().unwrap_err().field(),
            Some("receiptPrinterId")
        );
    }

    #[test]
    fn every_new_section_names_its_field() {
        let field = |c: HardwareConfig| c.validate().unwrap_err().field().map(str::to_owned);
        let base = HardwareConfig::default();
        let mut scale =
            crate::scale::ScaleConfig::new(crate::scale::ScaleProtocol::Continuous, "COM3");
        scale.poll_ms = 1;
        assert_eq!(
            field(HardwareConfig {
                scale: Some(scale),
                ..base.clone()
            })
            .as_deref(),
            Some("scale.pollMs")
        );
        let mut vfd = crate::display::VfdConfig::new("COM2");
        vfd.columns = 5;
        let display = DisplayConfig {
            vfd: Some(vfd),
            screen: CustomerScreenConfig::default(),
        };
        assert_eq!(
            field(HardwareConfig {
                display,
                ..base.clone()
            })
            .as_deref(),
            Some("display.vfd.columns")
        );
        let screen = CustomerScreenConfig {
            thanks_seconds: 99,
            ..CustomerScreenConfig::default()
        };
        assert_eq!(
            field(HardwareConfig {
                display: DisplayConfig { vfd: None, screen },
                ..base.clone()
            })
            .as_deref(),
            Some("display.screen.thanksSeconds")
        );
        let scanner = ScannerOptions {
            prefix: "123456789".into(),
            ..ScannerOptions::default()
        };
        assert_eq!(
            field(HardwareConfig {
                scanner,
                ..base.clone()
            })
            .as_deref(),
            Some("scanner.prefix")
        );
        let serial = ScannerOptions {
            serial: Some(crate::scanner::SerialScannerConfig::new("")),
            ..ScannerOptions::default()
        };
        assert_eq!(
            field(HardwareConfig {
                scanner: serial,
                ..base.clone()
            })
            .as_deref(),
            Some("scanner.serial.serial.port"),
            "the JSON path of the port: scanner.serial (the scanner) .serial (its settings) .port"
        );
        let drawer = DrawerConfig::Serial {
            port: "COM7".into(),
            baud: 3,
            kick: SerialKick::EscPos,
        };
        assert_eq!(
            field(HardwareConfig { drawer, ..base }).as_deref(),
            Some("drawer.baud")
        );
    }

    #[test]
    fn the_version_3_config_crosses_the_ipc_in_camel_case() {
        let json = serde_json::to_value(HardwareConfig::default()).unwrap();
        assert_eq!(json["version"], 3);
        assert_eq!(json["labelPrinters"], serde_json::json!([]));
        assert_eq!(json["drawer"], serde_json::json!({ "kind": "printer" }));
        assert_eq!(json["display"]["screen"]["thanksSeconds"], 5);
        assert_eq!(json["scanner"]["suffix"], "enter_or_tab");
        assert_eq!(
            json["kitchen"],
            serde_json::json!({ "stations": [], "fallbackPrinterId": null })
        );
        let route = StationPrinter {
            station_id: "st-grill".into(),
            printer_id: "p1".into(),
            copies: 2,
            buzzer: true,
        };
        assert_eq!(
            serde_json::to_value(&route).unwrap(),
            serde_json::json!({ "stationId": "st-grill", "printerId": "p1", "copies": 2, "buzzer": true })
        );
        let printer = serde_json::to_value(printer("p1", "a.bin")).unwrap();
        assert_eq!(
            (printer["mode"].as_str(), printer["paperWidthMm"].as_u64()),
            (Some("raw"), Some(80))
        );
        assert_eq!(serde_json::to_value(PrintMode::Driver).unwrap(), "driver");
    }

    #[test]
    fn a_version_2_file_loads_as_version_3_without_kitchen_routes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hardware.json");
        std::fs::write(&path, include_str!("../testdata/hardware-v2.json")).unwrap();
        let config = HardwareConfig::load(&path).unwrap();
        assert_eq!((config.version, config.kitchen.stations.len()), (3, 0));
        assert!(
            config
                .printers
                .iter()
                .all(|p| p.mode == PrintMode::Raw && p.paper_width_mm == 80)
        );
        // the rest of the PC's devices come through unchanged
        assert_eq!(config.receipt_printer_id.as_deref(), Some("p1"));
        assert_eq!(config.label_printer_id.as_deref(), Some("l1"));
        assert!(config.scale.is_some());
        assert_eq!(config.kitchen.fallback_printer_id, None);
    }

    /// HW-004: each station's printer, its copies and the kitchen fallback name configured
    /// printers; HW-002: a page-only printer is a Windows printer on 58 or 80 mm paper.
    #[test]
    fn kitchen_routes_and_driver_printers_are_validated_by_field() {
        let field = |c: &HardwareConfig| c.validate().unwrap_err().field().map(str::to_owned);
        let route = |station: &str, printer: &str, copies: u8| StationPrinter {
            station_id: station.into(),
            printer_id: printer.into(),
            copies,
            buzzer: false,
        };
        let mut config = HardwareConfig {
            printers: vec![printer("p1", "a.bin"), printer("p2", "b.bin")],
            ..HardwareConfig::default()
        };
        config.kitchen.stations = vec![route("st-grill", "p1", 1), route("st-bar", "p2", 3)];
        config.kitchen.fallback_printer_id = Some("p2".into());
        config.validate().unwrap();
        config.kitchen.stations[1].copies = 0;
        assert_eq!(field(&config).as_deref(), Some("kitchen.stations.1.copies"));
        config.kitchen.stations[1].copies = 4;
        assert_eq!(field(&config).as_deref(), Some("kitchen.stations.1.copies"));
        config.kitchen.stations[1] = route("st-bar", "gone", 1);
        assert_eq!(
            field(&config).as_deref(),
            Some("kitchen.stations.1.printerId")
        );
        config.kitchen.stations[1] = route("st-grill", "p2", 1);
        assert_eq!(
            field(&config).as_deref(),
            Some("kitchen.stations.1.stationId"),
            "one route per station"
        );
        config.kitchen.stations.pop();
        config.kitchen.fallback_printer_id = Some("gone".into());
        assert_eq!(field(&config).as_deref(), Some("kitchen.fallbackPrinterId"));
        config.kitchen.fallback_printer_id = None;
        config.validate().unwrap();

        config.printers[0].paper_width_mm = 70;
        assert_eq!(field(&config).as_deref(), Some("printers.0.paperWidthMm"));
        config.printers[0].paper_width_mm = 58;
        config.validate().unwrap();
        config.printers[0].mode = PrintMode::Driver;
        assert_eq!(
            field(&config).as_deref(),
            Some("printers.0.mode"),
            "only a Windows printer prints pages"
        );
        config.printers[0].transport = Transport::Windows {
            printer_name: "POS-80 (GDI)".into(),
        };
        config.validate().unwrap();
    }

    #[test]
    fn an_old_hardware_file_loads_with_default_document_printing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hardware.json");
        std::fs::write(
            &path,
            r#"{ "version": 1, "printers": [], "receiptPrinterId": null }"#,
        )
        .unwrap();
        let config = HardwareConfig::load(&path).unwrap();
        assert_eq!(config.documents, DocumentPrinting::default());
        assert_eq!(config.documents.margins_mm, 12);
        assert_eq!(
            config.documents.for_kind(DocType::Invoice, Paper::A4),
            (None, Paper::A4, 1)
        );
    }

    #[test]
    fn document_printing_is_validated_and_chosen_per_kind() {
        let mut config = HardwareConfig::default();
        config.documents.default_printer = Some("HP LaserJet M110".into());
        config.documents.kinds = vec![KindPrinting {
            doc_type: DocType::DeliveryNote,
            printer: Some("EPSON LQ-350".into()),
            paper: Paper::A5,
            copies: 2,
        }];
        config.validate().unwrap();
        assert_eq!(
            config.documents.for_kind(DocType::DeliveryNote, Paper::A4),
            (Some("EPSON LQ-350".into()), Paper::A5, 2)
        );
        assert_eq!(
            config.documents.for_kind(DocType::Invoice, Paper::A4),
            (Some("HP LaserJet M110".into()), Paper::A4, 1)
        );
        config.documents.kinds[0].copies = 9;
        assert!(config.validate().is_err());
    }
}

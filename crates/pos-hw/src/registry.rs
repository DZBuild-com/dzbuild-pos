//! The PC's hardware (spec §4.2 `AppState::hw`): settings, a cached typesetter and logo, one
//! job at a time per printer line (a TCP or serial printer takes one connection), label printing
//! (laid out apart from the receipts), kitchen tickets on their station's printer (M8), printer
//! status and the printer doctor, the drawer, and the serial ports the device services open.

use crate::bitmap::Bitmap;
use crate::config::{
    DrawerConfig, HardwareConfig, LabelPrinterConfig, PrintMode, PrinterConfig, SerialKick,
};
use crate::driver::{self, Transport};
use crate::error::HwError;
use crate::escpos::Builder;
use crate::labels::LabelDpi;
use crate::labels::model::{LabelData, LabelGroup, LabelTemplate};
use crate::layout::Layout;
use crate::raster::{Typesetter, logo_bitmap};
use crate::receipt::receipt_layout;
use crate::render::{DrawerKick, Job, Rendered, render};
use crate::report::{test_page, x_layout, z_layout};
use crate::serial::{LinkFactory, SerialSettings, SystemLinks};
use crate::status::{PrinterIssue, PrinterStatus};
use pos_core::cash::closing::{SessionSummary, ZReport};
use pos_core::labels::LabelWarning;
use pos_core::lang::Lang;
use pos_docs::receipt::Receipt;
use serde::Serialize;
use specta::Type;
use std::collections::{BTreeMap, HashMap};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PrintOutcome {
    pub printer: String,
    pub bytes: u32,
    pub raster: bool,
}

/// What a file printer may write: print files, never the database, a backup, a log or these
/// settings, which share the data folder.
const PRINT_FILE_EXTENSIONS: [&str; 2] = ["bin", "prn"];

/// `ESC p 0 25 250`: the drawer pulse on pin 2 (HW-041 adapters that emulate a printer).
const DRAWER_PULSE: [u8; 5] = [0x1B, b'p', 0, 25, 250];

/// How long DTR stays up to fire a trigger-only drawer adapter (HW-041).
const DTR_PULSE: Duration = Duration::from_millis(200);

/// The drawer adapter pulse that goes with a receipt (HW-041), copied out of the settings so
/// the registry lock is released before the port opens.
struct AdapterPulse {
    port: String,
    baud: u32,
    kick: SerialKick,
    before_print: bool,
}

/// What a receipt sends, read under one lock: its printer, its bytes, and the drawer adapter
/// pulse that goes with them. The bytes of a page-only printer are its `DriverOnly` error (HW-002):
/// its drawer adapter still fires for the cash, and a drawer on the printer itself opens through
/// the driver's own setting, with the page.
struct ReceiptJob {
    printer: PrinterConfig,
    bytes: Result<Rendered, HwError>,
    adapter: Option<AdapterPulse>,
}

/// The logo at the printer's width, kept while the logo and the paper stay the same.
struct LogoCache {
    key: (u64, u16),
    bitmap: Option<Bitmap>,
}

struct Inner {
    config: HardwareConfig,
    typesetter: Option<Typesetter>,
    logo: Option<LogoCache>,
}

pub struct Registry {
    path: Option<PathBuf>,
    inner: Mutex<Inner>,
    /// One lock per printer line (its transport), held while bytes go out: two jobs never
    /// interleave on one printer, and a long label batch never holds up the receipt printer.
    lines: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    /// The label jobs' own typesetter: a batch (up to 5 000 labels) is laid out without holding
    /// the settings, the receipts or the reports.
    label_ts: Mutex<Option<Typesetter>>,
    /// The serial ports (the PC's, or a test double) of the drawer adapter and the device
    /// services.
    links: Arc<dyn LinkFactory>,
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registry")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl Default for Registry {
    /// In memory only (tests, and the start before the data folder is known); with no data
    /// folder, it refuses every file printer.
    fn default() -> Self {
        Registry::with(None, HardwareConfig::default(), Arc::new(SystemLinks))
    }
}

fn poisoned() -> HwError {
    HwError::Io("hardware lock poisoned".into())
}

/// HW-002: a printer whose Windows driver only takes pages never gets ESC/POS. Its prints come
/// back as [`HwError::DriverOnly`], and the app prints the page through the driver.
fn takes_raw(printer: &PrinterConfig) -> Result<(), HwError> {
    match (printer.mode, &printer.transport) {
        (PrintMode::Driver, Transport::Windows { printer_name }) => Err(HwError::DriverOnly {
            printer_name: printer_name.clone(),
            paper_width_mm: printer.paper_width_mm,
        }),
        _ => Ok(()),
    }
}

/// The kitchen printers' buzzer after a ticket: three beeps of 200 ms.
const KITCHEN_BUZZER: [u8; 4] = crate::escpos::buzzer(3, 2);

impl Registry {
    fn with(
        path: Option<PathBuf>,
        config: HardwareConfig,
        links: Arc<dyn LinkFactory>,
    ) -> Registry {
        Registry {
            path,
            inner: Mutex::new(Inner {
                config,
                typesetter: None,
                logo: None,
            }),
            lines: Mutex::new(HashMap::new()),
            label_ts: Mutex::new(None),
            links,
        }
    }

    /// Reads `path`; an unreadable file is renamed `hardware.json.bad` and the defaults are used,
    /// so a damaged file never stops the till.
    pub fn open(path: impl Into<PathBuf>) -> Registry {
        Registry::open_with(path, Arc::new(SystemLinks))
    }

    /// [`Registry::open`] with other serial ports than the PC's (tests, demos).
    pub fn open_with(path: impl Into<PathBuf>, links: Arc<dyn LinkFactory>) -> Registry {
        let path = path.into();
        let config = match HardwareConfig::load(&path) {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(
                    %error,
                    file = %path.display(),
                    "hardware settings unreadable; using the defaults"
                );
                if let Err(rename) = std::fs::rename(&path, path.with_extension("json.bad")) {
                    tracing::warn!(
                        error = %rename,
                        "could not set the broken hardware settings aside"
                    );
                }
                HardwareConfig::default()
            }
        };
        Registry::with(Some(path), config, links)
    }

    /// The serial ports this registry opens; the scale, display and scanner services share them.
    pub fn links(&self) -> Arc<dyn LinkFactory> {
        Arc::clone(&self.links)
    }

    fn lock(&self) -> Result<MutexGuard<'_, Inner>, HwError> {
        self.inner.lock().map_err(|_| poisoned())
    }

    /// The lock of the printer line `transport` (the same printer whatever its case or spaces).
    fn line(&self, transport: &Transport) -> Result<Arc<Mutex<()>>, HwError> {
        let key = match transport {
            Transport::Windows { printer_name } => {
                format!("windows:{}", printer_name.trim().to_lowercase())
            }
            Transport::Tcp { host, port } => format!("tcp:{}:{port}", host.trim().to_lowercase()),
            Transport::Serial { port, .. } => format!("serial:{}", crate::serial::port_key(port)),
            Transport::File { path } => format!("file:{}", path.trim().to_lowercase()),
        };
        let mut lines = self.lines.lock().map_err(|_| poisoned())?;
        Ok(Arc::clone(lines.entry(key).or_default()))
    }

    /// The label jobs' typesetter (never the one `inner` keeps for receipts and reports).
    fn label_typesetter(&self) -> Result<MutexGuard<'_, Option<Typesetter>>, HwError> {
        self.label_ts.lock().map_err(|_| poisoned())
    }

    pub fn config(&self) -> Result<HardwareConfig, HwError> {
        Ok(self.lock()?.config.clone())
    }

    /// Validates and stores `config`; `driver_names` is the registry's own record (HW-009): each
    /// Windows printer's driver as Windows lists it now, or as last recorded when Windows does
    /// not list it; whatever the caller sent there is ignored.
    pub fn save(&self, mut config: HardwareConfig) -> Result<HardwareConfig, HwError> {
        config.validate()?;
        for (i, printer) in config.printers.iter().enumerate() {
            self.check_transport(&printer.transport)
                .map_err(|_| HwError::invalid(&format!("printers.{i}.transport.path")))?;
        }
        for (i, printer) in config.label_printers.iter().enumerate() {
            self.check_transport(&printer.transport)
                .map_err(|_| HwError::invalid(&format!("labelPrinters.{i}.transport.path")))?;
        }
        let windows = windows_printers(&config);
        // Listed before the lock: the spooler may take a while to answer.
        let drivers = if windows.is_empty() {
            Vec::new()
        } else {
            crate::discover::windows_drivers()
        };
        let mut inner = self.lock()?;
        config.driver_names = driver_names(&windows, &drivers, &inner.config.driver_names);
        if let Some(path) = &self.path {
            config.save(path)?;
        }
        inner.config = config.clone();
        inner.logo = None;
        Ok(config)
    }

    /// A [`Transport::File`] writes only a `.bin` or `.prn` file inside the data folder (the
    /// folder of `hardware.json`), by a path that neither climbs out of it nor names an
    /// alternate stream of another file: whoever may change the hardware settings must not
    /// append print bytes to the database or to any other file.
    fn check_transport(&self, transport: &Transport) -> Result<(), HwError> {
        let Transport::File { path } = transport else {
            return Ok(());
        };
        let path = Path::new(path);
        let folder = self
            .path
            .as_deref()
            .and_then(Path::parent)
            .filter(|folder| folder.is_absolute());
        let inside = folder
            .and_then(|folder| path.strip_prefix(folder).ok())
            .is_some_and(|rest| {
                rest.components().next().is_some()
                    && rest.components().all(|c| match c {
                        Component::Normal(name) => !name.as_encoded_bytes().contains(&b':'),
                        _ => false,
                    })
            });
        let print_file = path.extension().is_some_and(|extension| {
            PRINT_FILE_EXTENSIONS
                .iter()
                .any(|allowed| extension.eq_ignore_ascii_case(allowed))
        });
        if inside && print_file {
            Ok(())
        } else {
            Err(HwError::invalid("transport.path"))
        }
    }

    fn send(&self, printer: &PrinterConfig, rendered: &Rendered) -> Result<PrintOutcome, HwError> {
        self.check_transport(&printer.transport)?;
        let line = self.line(&printer.transport)?;
        let _one_job = line.lock().map_err(|_| poisoned())?;
        let mut driver = driver::open(&printer.transport)?;
        driver.send(&rendered.bytes)?;
        Ok(PrintOutcome {
            printer: printer.name.clone(),
            bytes: u32::try_from(rendered.bytes.len()).unwrap_or(u32::MAX),
            raster: rendered.raster,
        })
    }

    fn receipt_printer(inner: &Inner) -> Result<PrinterConfig, HwError> {
        inner
            .config
            .receipt_printer()
            .cloned()
            .ok_or(HwError::NotConfigured)
    }

    /// The receipt's bytes for the receipt printer, with the logo and the drawer kick the
    /// options ask for; `cash_moved` is true when cash went in or out of the drawer. The kick
    /// is in the bytes only when the drawer hangs on the receipt printer; a USB/COM adapter is
    /// pulsed by [`Registry::print_receipt`]. A page-only printer gets none (`DriverOnly`).
    pub fn receipt_bytes(
        &self,
        receipt: &Receipt,
        logo: Option<&[u8]>,
        cash_moved: bool,
    ) -> Result<(PrinterConfig, Rendered), HwError> {
        let job = self.receipt_job(receipt, logo, cash_moved)?;
        Ok((job.printer, job.bytes?))
    }

    /// The receipt's printer and bytes, and the drawer adapter pulse that goes with them, all
    /// read under one lock.
    fn receipt_job(
        &self,
        receipt: &Receipt,
        logo: Option<&[u8]>,
        cash_moved: bool,
    ) -> Result<ReceiptJob, HwError> {
        let mut inner = self.lock()?;
        let printer = Registry::receipt_printer(&inner)?;
        let options = inner.config.receipt.clone();
        let wanted = cash_moved && options.open_drawer_on_cash;
        let adapter = match &inner.config.drawer {
            DrawerConfig::Serial { port, baud, kick } if wanted => Some(AdapterPulse {
                port: port.clone(),
                baud: *baud,
                kick: *kick,
                before_print: options.drawer_before_print,
            }),
            _ => None,
        };
        if let Err(page) = takes_raw(&printer) {
            return Ok(ReceiptJob {
                printer,
                bytes: Err(page),
                adapter,
            });
        }
        let layout = receipt_layout(receipt, options.second_lang);
        let logo = match (options.logo, logo) {
            (true, Some(bytes)) => inner.logo_for(bytes, printer.profile.dots),
            _ => None,
        };
        // the adapter is pulsed apart from the bytes (see `print_receipt`)
        let drawer = match &inner.config.drawer {
            _ if !wanted => DrawerKick::None,
            DrawerConfig::Printer if options.drawer_before_print => DrawerKick::BeforePrint,
            DrawerConfig::Printer => DrawerKick::AfterPrint,
            DrawerConfig::Serial { .. } => DrawerKick::None,
        };
        let typesetter = inner.typesetter.get_or_insert_with(Typesetter::new);
        let rendered = render(
            &Job {
                layout: &layout,
                logo: logo.as_ref(),
                drawer,
            },
            &printer.profile,
            typesetter,
        )?;
        Ok(ReceiptJob {
            printer,
            bytes: Ok(rendered),
            adapter,
        })
    }

    /// Prints the receipt; when cash moved and the drawer hangs on a USB/COM adapter (HW-041),
    /// pulses the adapter after the receipt, or before it when the options say so. The adapter
    /// is its own device: the drawer opens even when the printer fails or only takes pages
    /// (`DriverOnly`, HW-002), and an unplugged adapter never stops the receipt (it is logged;
    /// the drawer button then reports it).
    pub fn print_receipt(
        &self,
        receipt: &Receipt,
        logo: Option<&[u8]>,
        cash_moved: bool,
    ) -> Result<PrintOutcome, HwError> {
        let ReceiptJob {
            printer,
            bytes,
            adapter,
        } = self.receipt_job(receipt, logo, cash_moved)?;
        let send = || match &bytes {
            Ok(rendered) => self.send(&printer, rendered),
            Err(page) => Err(page.clone()),
        };
        let Some(adapter) = adapter else {
            return send();
        };
        let pulse = || {
            if let Err(error) = self.pulse_adapter(&adapter.port, adapter.baud, adapter.kick) {
                tracing::warn!(%error, port = %adapter.port, "the drawer adapter did not fire");
            }
        };
        if adapter.before_print {
            pulse();
        }
        let sent = send();
        if !adapter.before_print {
            pulse();
        }
        sent
    }

    fn report(&self, layout: &Layout) -> Result<PrintOutcome, HwError> {
        let (printer, rendered) = {
            let mut inner = self.lock()?;
            let printer = Registry::receipt_printer(&inner)?;
            takes_raw(&printer)?;
            let typesetter = inner.typesetter.get_or_insert_with(Typesetter::new);
            let rendered = render(
                &Job {
                    layout,
                    logo: None,
                    drawer: DrawerKick::None,
                },
                &printer.profile,
                typesetter,
            )?;
            (printer, rendered)
        };
        self.send(&printer, &rendered)
    }

    /// Any other slip on the receipt printer, laid out by the caller (the app's versement
    /// receipts and account statements, M2; the addition, the intake slip, the deposit receipt
    /// and the vouchers, M8), without logo or drawer kick.
    pub fn print_layout(&self, layout: &Layout) -> Result<PrintOutcome, HwError> {
        self.report(layout)
    }

    /// The printer `printer_id` and `layout` rendered for it, without logo or drawer kick; a
    /// page-only printer gets nothing (`DriverOnly`).
    fn slip_job(
        &self,
        printer_id: &str,
        layout: &Layout,
    ) -> Result<(PrinterConfig, Rendered), HwError> {
        let mut inner = self.lock()?;
        let printer = inner
            .config
            .printers
            .iter()
            .find(|p| p.id == printer_id)
            .cloned()
            .ok_or_else(|| HwError::PrinterNotFound(printer_id.to_owned()))?;
        takes_raw(&printer)?;
        let typesetter = inner.typesetter.get_or_insert_with(Typesetter::new);
        let rendered = render(
            &Job {
                layout,
                logo: None,
                drawer: DrawerKick::None,
            },
            &printer.profile,
            typesetter,
        )?;
        Ok((printer, rendered))
    }

    /// A slip on one configured printer (kitchen, bar, a second receipt printer).
    pub fn print_to(&self, printer_id: &str, layout: &Layout) -> Result<PrintOutcome, HwError> {
        let (printer, rendered) = self.slip_job(printer_id, layout)?;
        self.send(&printer, &rendered)
    }

    /// RST-006, HW-004: a kitchen or cancellation ticket on its station's printer (else the
    /// kitchen fallback, else the receipt printer), `copies` times, each copy followed by the
    /// buzzer after its cut when the station asks for it. Laid out once; the copies are separate
    /// jobs on the printer's line, so a receipt for the same printer waits one copy at most.
    pub fn print_kitchen(
        &self,
        station_id: Option<&str>,
        layout: &Layout,
    ) -> Result<Vec<PrintOutcome>, HwError> {
        let (printer_id, copies, buzzer) = {
            let inner = self.lock()?;
            let routing = &inner.config.kitchen;
            match station_id.and_then(|s| routing.stations.iter().find(|r| r.station_id == s)) {
                Some(route) => (route.printer_id.clone(), route.copies.max(1), route.buzzer),
                None => match routing
                    .fallback_printer_id
                    .as_ref()
                    .or(inner.config.receipt_printer_id.as_ref())
                {
                    Some(id) => (id.clone(), 1, false),
                    None => return Err(HwError::NotConfigured),
                },
            }
        };
        let (printer, mut rendered) = self.slip_job(&printer_id, layout)?;
        if buzzer {
            rendered.bytes.extend_from_slice(&KITCHEN_BUZZER);
        }
        (0..copies)
            .map(|_| self.send(&printer, &rendered))
            .collect()
    }

    /// The Z of a closed session; `approver` is the display name of the manager who accepted
    /// the variance (see [`z_layout`]).
    pub fn print_z(
        &self,
        z: &ZReport,
        shop: &str,
        lang: Lang,
        approver: Option<&str>,
    ) -> Result<PrintOutcome, HwError> {
        self.report(&z_layout(z, shop, lang, approver))
    }

    pub fn print_x(
        &self,
        summary: &SessionSummary,
        shop: &str,
        lang: Lang,
        printed_at: i64,
    ) -> Result<PrintOutcome, HwError> {
        self.report(&x_layout(summary, shop, lang, printed_at))
    }

    /// A test page on `printer`, saved or not yet (the wizard tests before saving).
    pub fn test_page(&self, printer: &PrinterConfig, lang: Lang) -> Result<PrintOutcome, HwError> {
        printer.transport.validate()?;
        takes_raw(printer)?;
        let rendered = {
            let mut inner = self.lock()?;
            let typesetter = inner.typesetter.get_or_insert_with(Typesetter::new);
            test_page(&printer.profile, &printer.name, lang, typesetter)?
        };
        self.send(printer, &rendered)
    }

    /// Opens the drawer (HW-040/041): through the receipt printer, or a USB/COM adapter. The
    /// registry lock is not held while the port is open.
    pub fn open_drawer(&self) -> Result<PrintOutcome, HwError> {
        let drawer = self.lock()?.config.drawer.clone();
        match drawer {
            DrawerConfig::Printer => self.kick_through_printer(),
            DrawerConfig::Serial { port, baud, kick } => {
                self.pulse_adapter(&port, baud, kick)?;
                Ok(PrintOutcome {
                    printer: port,
                    bytes: 0,
                    raster: false,
                })
            }
        }
    }

    /// Fires a USB/COM drawer adapter (HW-041): `ESC p 0 25 250`, or DTR up for 200 ms. The
    /// callers hold no registry lock.
    fn pulse_adapter(&self, port: &str, baud: u32, kick: SerialKick) -> Result<(), HwError> {
        let mut link = self.links.open(&SerialSettings::eight_n1(port, baud))?;
        match kick {
            SerialKick::EscPos => link.write_all(&DRAWER_PULSE),
            SerialKick::Dtr => {
                link.set_dtr(true)?;
                std::thread::sleep(DTR_PULSE);
                link.set_dtr(false)
            }
        }
    }

    /// The label printer `id`, else the default one.
    pub fn label_printer(&self, id: Option<&str>) -> Result<LabelPrinterConfig, HwError> {
        let inner = self.lock()?;
        let wanted = id.or(inner.config.label_printer_id.as_deref());
        wanted
            .and_then(|id| inner.config.label_printers.iter().find(|p| p.id == id))
            .cloned()
            .ok_or(HwError::LabelPrinterNotConfigured)
    }

    fn send_to(
        &self,
        name: &str,
        transport: &Transport,
        bytes: Vec<u8>,
    ) -> Result<PrintOutcome, HwError> {
        self.check_transport(transport)?;
        let line = self.line(transport)?;
        let _one_job = line.lock().map_err(|_| poisoned())?;
        driver::open(transport)?.send(&bytes)?;
        Ok(PrintOutcome {
            printer: name.to_owned(),
            bytes: u32::try_from(bytes.len()).unwrap_or(u32::MAX),
            raster: true,
        })
    }

    /// Every group in one job on the label printer (`printer_id`, else the default one), with
    /// the labels whose layout leaves out the lot or the expiry date they carry (décret 13-378:
    /// they print all the same, and the batch says so). Each label is laid out once.
    pub fn print_labels(
        &self,
        groups: &[LabelGroup],
        printer_id: Option<&str>,
    ) -> Result<(PrintOutcome, Vec<LabelWarning>), HwError> {
        let printer = self.label_printer(printer_id)?;
        let mut warnings = Vec::new();
        let bytes = {
            let mut ts = self.label_typesetter()?;
            let ts = ts.get_or_insert_with(Typesetter::new);
            let mut all = Vec::new();
            for group in groups {
                all.extend(crate::labels::job_noting(
                    group,
                    &printer.profile,
                    ts,
                    &mut warnings,
                )?);
            }
            all
        };
        let outcome = self.send_to(&printer.name, &printer.transport, bytes)?;
        Ok((outcome, warnings))
    }

    /// The label as a PNG at `dpi`, drawn from the boxes the printer gets.
    pub fn label_preview(
        &self,
        template: &LabelTemplate,
        label: &LabelData,
        dpi: LabelDpi,
    ) -> Result<Vec<u8>, HwError> {
        let mut ts = self.label_typesetter()?;
        let ts = ts.get_or_insert_with(Typesetter::new);
        crate::labels::to_png(&crate::labels::preview(
            template,
            label,
            dpi.dots_per_mm(),
            ts,
        )?)
    }

    /// Décret 13-378: the labels of `groups` whose layout at `dpi` leaves out the lot or the
    /// expiry date they carry (the batch prints all the same, and says so).
    pub fn labels_dropped(
        &self,
        groups: &[LabelGroup],
        dpi: LabelDpi,
    ) -> Result<Vec<LabelWarning>, HwError> {
        let mut ts = self.label_typesetter()?;
        let ts = ts.get_or_insert_with(Typesetter::new);
        let mut warnings = Vec::new();
        for group in groups {
            for label in &group.labels {
                warnings.extend(crate::labels::dropped(
                    &group.template,
                    label,
                    dpi.dots_per_mm(),
                    ts,
                )?);
            }
        }
        Ok(warnings)
    }

    /// A test label on `printer`, saved or not (the settings test before saving).
    pub fn label_test(
        &self,
        printer: &LabelPrinterConfig,
        lang: Lang,
    ) -> Result<PrintOutcome, HwError> {
        printer.transport.validate()?;
        printer.profile.validate()?;
        let group = crate::report::label_test_group(&printer.name, lang);
        let bytes = {
            let mut ts = self.label_typesetter()?;
            let ts = ts.get_or_insert_with(Typesetter::new);
            crate::labels::job(&group, &printer.profile, ts)?
        };
        self.send_to(&printer.name, &printer.transport, bytes)
    }

    /// HW-008: the spooler's view for Windows printers, the printer's own bytes over TCP/COM.
    pub fn printer_status(&self, printer_id: &str) -> Result<PrinterStatus, HwError> {
        let transport = {
            let inner = self.lock()?;
            inner
                .config
                .printers
                .iter()
                .find(|p| p.id == printer_id)
                .map(|p| p.transport.clone())
                .or_else(|| {
                    inner
                        .config
                        .label_printers
                        .iter()
                        .find(|p| p.id == printer_id)
                        .map(|p| p.transport.clone())
                })
                .ok_or_else(|| HwError::PrinterNotFound(printer_id.to_owned()))?
        };
        match &transport {
            Transport::Windows { printer_name } => {
                let (status, attributes) = driver::spooler_status(printer_name)?;
                Ok(crate::status::from_spooler(status, attributes))
            }
            Transport::Tcp { .. } | Transport::Serial { .. } => {
                let line = self.line(&transport)?;
                let _one_job = line.lock().map_err(|_| poisoned())?;
                let reply = driver::open(&transport)?.query(&crate::status::DLE_EOT_ALL, 4)?;
                let bytes: [u8; 4] = reply
                    .try_into()
                    .map_err(|_| HwError::Io("short status reply".into()))?;
                Ok(crate::status::from_dle_eot(&bytes))
            }
            Transport::File { .. } => Ok(PrinterStatus::unknown()),
        }
    }

    /// HW-009: the saved Windows printers Windows no longer lists, lists with another driver, or
    /// holds offline.
    pub fn doctor(&self) -> Result<Vec<PrinterIssue>, HwError> {
        let config = self.config()?;
        Ok(crate::status::doctor(&config, &crate::discover::discover()))
    }

    /// The kick through the receipt printer (HW-040).
    fn kick_through_printer(&self) -> Result<PrintOutcome, HwError> {
        let printer = Registry::receipt_printer(&*self.lock()?)?;
        let mut b = Builder::new();
        b.init(printer.profile.exit_chinese_mode)
            .drawer_kick(printer.profile.drawer_pin);
        self.send(
            &printer,
            &Rendered {
                bytes: b.into_bytes(),
                raster: false,
            },
        )
    }
}

/// `(id, Windows printer name)` of every saved printer on the spooler, receipt and label.
fn windows_printers(config: &HardwareConfig) -> Vec<(String, String)> {
    config
        .printers
        .iter()
        .map(|p| (&p.id, &p.transport))
        .chain(config.label_printers.iter().map(|p| (&p.id, &p.transport)))
        .filter_map(|(id, transport)| match transport {
            Transport::Windows { printer_name } => {
                Some((id.clone(), printer_name.trim().to_owned()))
            }
            _ => None,
        })
        .collect()
}

/// Printer id → driver: as Windows lists it now (`drivers`), else as `before` recorded it; the
/// ids of printers gone or off the spooler are dropped.
fn driver_names(
    windows: &[(String, String)],
    drivers: &[(String, String)],
    before: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    windows
        .iter()
        .filter_map(|(id, printer)| {
            drivers
                .iter()
                .find(|(name, _)| name == printer)
                .map(|(_, driver)| driver)
                .or_else(|| before.get(id))
                .map(|driver| (id.clone(), driver.clone()))
        })
        .collect()
}

impl Inner {
    /// The logo scaled to 60 % of the paper and 160 dots high; `None` (and a warning) when the
    /// stored image cannot be read: the receipt prints without it.
    fn logo_for(&mut self, bytes: &[u8], dots: u16) -> Option<Bitmap> {
        let mut hasher = DefaultHasher::new();
        bytes.hash(&mut hasher);
        let key = (hasher.finish(), dots);
        if let Some(cache) = &self.logo
            && cache.key == key
        {
            return cache.bitmap.clone();
        }
        let bitmap = match logo_bitmap(bytes, u32::from(dots) * 6 / 10, 160) {
            Ok(bitmap) => Some(bitmap),
            Err(error) => {
                tracing::warn!(%error, "the shop logo cannot be printed");
                None
            }
        };
        self.logo = Some(LogoCache {
            key,
            bitmap: bitmap.clone(),
        });
        bitmap
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{HardwareConfig, PrintMode, PrinterConfig};
    use crate::driver::Transport;
    use crate::profile::{PaperWidth, Preset, Profile};
    use crate::testdata::{logo_png, ticket, z_report};
    use pos_core::lang::Lang;

    fn with_file_printer(dir: &std::path::Path, preset: Preset) -> (Registry, std::path::PathBuf) {
        let out = dir.join("printer.bin");
        let registry = Registry::open(dir.join("hardware.json"));
        let mut config = HardwareConfig::default();
        config.printers.push(PrinterConfig {
            id: "p1".into(),
            name: "Caisse 1".into(),
            transport: Transport::File {
                path: out.to_string_lossy().into_owned(),
            },
            profile: Profile::preset(preset, PaperWidth::Mm80),
            mode: PrintMode::Raw,
            paper_width_mm: 80,
        });
        config.receipt_printer_id = Some("p1".into());
        registry.save(config).unwrap();
        (registry, out)
    }

    #[test]
    fn registry_is_shared_across_threads() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Registry>();
    }

    #[test]
    fn nothing_prints_without_a_receipt_printer() {
        let registry = Registry::default();
        assert_eq!(
            registry
                .print_receipt(&ticket(Lang::Fr), None, false)
                .unwrap_err()
                .code(),
            "printer_not_configured"
        );
        assert_eq!(
            registry.open_drawer().unwrap_err().code(),
            "printer_not_configured"
        );
    }

    #[test]
    fn a_receipt_reaches_the_printer_with_the_drawer_after_the_cut() {
        let dir = tempfile::tempdir().unwrap();
        let (registry, out) = with_file_printer(dir.path(), Preset::Generic);
        let outcome = registry
            .print_receipt(&ticket(Lang::Fr), None, true)
            .unwrap();
        assert_eq!(outcome.printer, "Caisse 1");
        assert!(!outcome.raster);
        let bytes = std::fs::read(&out).unwrap();
        assert!(bytes.starts_with(&[0x1B, b'@']));
        assert_eq!(u32::try_from(bytes.len()).unwrap(), outcome.bytes);
        assert!(
            bytes.ends_with(&[0x1D, b'V', 66, 0, 0x1B, b'p', 0, 25, 250]),
            "cut, then open the drawer"
        );
    }

    #[test]
    fn the_drawer_can_open_first_or_not_at_all() {
        let dir = tempfile::tempdir().unwrap();
        let (registry, out) = with_file_printer(dir.path(), Preset::Generic);
        let mut config = registry.config().unwrap();
        config.receipt.drawer_before_print = true;
        registry.save(config).unwrap();
        registry
            .print_receipt(&ticket(Lang::Fr), None, true)
            .unwrap();
        let bytes = std::fs::read(&out).unwrap();
        assert!(bytes.starts_with(&[0x1B, b'@', 0x1B, b'p', 0, 25, 250]));
        std::fs::remove_file(&out).unwrap();
        registry
            .print_receipt(&ticket(Lang::Fr), None, false)
            .unwrap();
        assert!(
            !std::fs::read(&out)
                .unwrap()
                .windows(2)
                .any(|w| w == [0x1B, b'p']),
            "no cash, no kick"
        );
    }

    #[test]
    fn the_logo_is_printed_when_asked_and_a_broken_one_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let (registry, out) = with_file_printer(dir.path(), Preset::Generic);
        registry
            .print_receipt(&ticket(Lang::Fr), Some(&logo_png()), false)
            .unwrap();
        assert!(
            std::fs::read(&out)
                .unwrap()
                .windows(3)
                .any(|w| w == [0x1D, b'v', b'0'])
        );
        std::fs::remove_file(&out).unwrap();
        registry
            .print_receipt(&ticket(Lang::Fr), Some(b"broken"), false)
            .unwrap();
        assert!(
            !std::fs::read(&out)
                .unwrap()
                .windows(3)
                .any(|w| w == [0x1D, b'v', b'0']),
            "printed without the logo"
        );
    }

    #[test]
    fn a_bilingual_option_rasterises_the_receipt() {
        let dir = tempfile::tempdir().unwrap();
        let (registry, _) = with_file_printer(dir.path(), Preset::Generic);
        let mut config = registry.config().unwrap();
        config.receipt.second_lang = Some(Lang::Ar);
        registry.save(config).unwrap();
        assert!(
            registry
                .print_receipt(&ticket(Lang::Fr), None, false)
                .unwrap()
                .raster
        );
    }

    #[test]
    fn drawer_z_and_test_page_jobs() {
        let dir = tempfile::tempdir().unwrap();
        let (registry, out) = with_file_printer(dir.path(), Preset::Xprinter);
        registry.open_drawer().unwrap();
        assert_eq!(
            std::fs::read(&out).unwrap(),
            [0x1B, b'@', 0x1C, b'.', 0x1B, b'p', 0, 25, 250]
        );
        std::fs::remove_file(&out).unwrap();
        registry
            .print_z(&z_report(), "Supérette El Amel", Lang::Fr, None)
            .unwrap();
        assert!(
            std::fs::read(&out)
                .unwrap()
                .windows(9)
                .any(|w| w == b"Rapport Z")
        );
        let printer = registry.config().unwrap().printers[0].clone();
        assert!(registry.test_page(&printer, Lang::Ar).unwrap().raster);
    }

    #[test]
    fn a_broken_settings_file_is_set_aside() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hardware.json");
        std::fs::write(&path, "{ broken").unwrap();
        let registry = Registry::open(&path);
        assert_eq!(registry.config().unwrap(), HardwareConfig::default());
        assert!(dir.path().join("hardware.json.bad").exists());
    }

    #[test]
    fn saved_settings_survive_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let (registry, _) = with_file_printer(dir.path(), Preset::Epson);
        let saved = registry.config().unwrap();
        drop(registry);
        assert_eq!(
            Registry::open(dir.path().join("hardware.json"))
                .config()
                .unwrap(),
            saved
        );
    }

    #[test]
    fn an_invalid_config_is_not_saved() {
        let registry = Registry::default();
        let bad = HardwareConfig {
            receipt_printer_id: Some("ghost".into()),
            ..HardwareConfig::default()
        };
        assert_eq!(
            registry.save(bad).unwrap_err().code(),
            "printer_setting_invalid"
        );
        assert_eq!(registry.config().unwrap(), HardwareConfig::default());
    }

    fn file(path: &std::path::Path) -> Transport {
        Transport::File {
            path: path.to_string_lossy().into_owned(),
        }
    }

    /// Whoever may change the hardware settings must not append print bytes to any file: a
    /// file printer writes a `.bin` or `.prn` file in the data folder, nothing else.
    #[test]
    fn file_printers_write_only_print_files_in_the_data_folder() {
        let dir = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let (registry, _) = with_file_printer(dir.path(), Preset::Generic);
        let database = dir.path().join("pos.db");
        std::fs::write(&database, b"SQLite format 3\0").unwrap();
        let refused = [
            elsewhere.path().join("printer.bin"),
            dir.path().join("..").join("printer.bin"),
            database.clone(),
            dir.path().join("hardware.json"),
            dir.path().join("pos.db:stream.bin"),
            std::path::PathBuf::from("printer.bin"),
        ];
        for path in &refused {
            let mut config = registry.config().unwrap();
            config.printers[0].transport = file(path);
            let saved = registry.save(config.clone()).unwrap_err();
            assert_eq!(
                saved.field(),
                Some("printers.0.transport.path"),
                "{}",
                path.display()
            );
            let tested = registry.test_page(&config.printers[0], Lang::Fr);
            assert_eq!(
                tested.unwrap_err().field(),
                Some("transport.path"),
                "{}",
                path.display()
            );
        }
        assert!(!elsewhere.path().join("printer.bin").exists());
        assert_eq!(std::fs::read(&database).unwrap(), b"SQLite format 3\0");
        let mut config = registry.config().unwrap();
        let prn = dir.path().join("Caisse.PRN");
        config.printers[0].transport = file(&prn);
        registry.save(config.clone()).unwrap();
        registry.test_page(&config.printers[0], Lang::Fr).unwrap();
        assert!(std::fs::read(&prn).unwrap().starts_with(&[0x1B, b'@']));
    }

    #[test]
    fn the_in_memory_registry_refuses_file_printers() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("printer.bin");
        let registry = Registry::default();
        let mut config = HardwareConfig::default();
        config.printers.push(PrinterConfig {
            id: "p1".into(),
            name: "Caisse 1".into(),
            transport: file(&out),
            profile: Profile::preset(Preset::Generic, PaperWidth::Mm80),
            mode: PrintMode::Raw,
            paper_width_mm: 80,
        });
        assert_eq!(
            registry.save(config.clone()).unwrap_err().field(),
            Some("printers.0.transport.path")
        );
        assert_eq!(
            registry
                .test_page(&config.printers[0], Lang::Fr)
                .unwrap_err()
                .field(),
            Some("transport.path")
        );
        assert!(!out.exists());
    }

    #[test]
    fn a_hand_edited_file_printer_outside_the_data_folder_prints_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let out = elsewhere.path().join("printer.bin");
        let path = dir.path().join("hardware.json");
        let mut config = HardwareConfig::default();
        config.printers.push(PrinterConfig {
            id: "p1".into(),
            name: "Caisse 1".into(),
            transport: file(&out),
            profile: Profile::preset(Preset::Generic, PaperWidth::Mm80),
            mode: PrintMode::Raw,
            paper_width_mm: 80,
        });
        config.receipt_printer_id = Some("p1".into());
        config.save(&path).unwrap();
        let registry = Registry::open(&path);
        assert_eq!(
            registry.config().unwrap(),
            config,
            "the settings still load"
        );
        let field = |r: Result<PrintOutcome, HwError>| r.unwrap_err().field().map(str::to_owned);
        assert_eq!(
            field(registry.print_receipt(&ticket(Lang::Fr), None, true)).as_deref(),
            Some("transport.path")
        );
        assert_eq!(
            field(registry.open_drawer()).as_deref(),
            Some("transport.path")
        );
        assert_eq!(
            field(registry.print_z(&z_report(), "Supérette El Amel", Lang::Fr, None)).as_deref(),
            Some("transport.path")
        );
        assert!(!out.exists());
    }

    use crate::config::{DrawerConfig, SerialKick};
    use crate::labels::model::{LabelGroup, LabelLang};
    use crate::labels::{LabelPreset, LabelProfile};
    use crate::serial::MemoryLinks;
    use crate::testdata::{milk_label, oil_shelf_label};

    fn with_label_printer(
        dir: &std::path::Path,
        preset: LabelPreset,
    ) -> (Registry, std::path::PathBuf) {
        let out = dir.join("labels.prn");
        let registry = Registry::open(dir.join("hardware.json"));
        let mut config = HardwareConfig::default();
        config
            .label_printers
            .push(crate::config::LabelPrinterConfig {
                id: "l1".into(),
                name: "Étiquettes".into(),
                transport: Transport::File {
                    path: out.to_string_lossy().into_owned(),
                },
                profile: LabelProfile::preset(preset),
            });
        config.label_printer_id = Some("l1".into());
        registry.save(config).unwrap();
        (registry, out)
    }

    /// A large label batch is laid out with the labels' own typesetter and sent on the label
    /// printer's line only: the settings, the receipts and the other printers stay free.
    #[test]
    fn a_label_batch_never_holds_up_the_receipt_printer() {
        let dir = tempfile::tempdir().unwrap();
        let (registry, _) = with_label_printer(dir.path(), LabelPreset::Xprinter);
        let labels = registry.label_printer(None).unwrap().transport;
        let laying_out = registry.label_typesetter().unwrap();
        assert!(registry.config().is_ok(), "the settings are not held");
        let line = registry.line(&labels).unwrap();
        let _sending = line.lock().unwrap();
        let receipt = Transport::File {
            path: dir
                .path()
                .join("receipt.bin")
                .to_string_lossy()
                .into_owned(),
        };
        assert!(
            registry.line(&receipt).unwrap().try_lock().is_ok(),
            "the receipt printer has a line of its own"
        );
        assert!(
            registry.line(&labels).unwrap().try_lock().is_err(),
            "one job at a time on one printer"
        );
        drop(laying_out);
        let com = |port: &str| Transport::Serial {
            port: port.into(),
            baud: 9_600,
        };
        let held = registry.line(&com("COM5")).unwrap();
        let _held = held.lock().unwrap();
        assert!(
            registry.line(&com(" com5")).unwrap().try_lock().is_err(),
            "one port, however it is typed"
        );
    }

    #[test]
    fn labels_print_to_the_label_printer_in_its_language() {
        let dir = tempfile::tempdir().unwrap();
        let (registry, out) = with_label_printer(dir.path(), LabelPreset::Xprinter);
        let (template, label) = milk_label(LabelLang::Fr);
        let (outcome, warnings) = registry
            .print_labels(
                &[LabelGroup {
                    template,
                    labels: vec![label],
                }],
                None,
            )
            .unwrap();
        assert_eq!(outcome.printer, "Étiquettes");
        assert!(warnings.is_empty());
        let bytes = std::fs::read(&out).unwrap();
        assert!(bytes.starts_with(b"SIZE 40 mm,25 mm\r\n"));
        std::fs::remove_file(&out).unwrap();
        let mut config = registry.config().unwrap();
        config.label_printers[0].profile = LabelProfile::preset(LabelPreset::Zebra);
        registry.save(config).unwrap();
        let (template, label) = oil_shelf_label();
        registry
            .print_labels(
                &[LabelGroup {
                    template,
                    labels: vec![label],
                }],
                Some("l1"),
            )
            .unwrap();
        assert!(std::fs::read(&out).unwrap().starts_with(b"~SD16\r\n^XA"));
    }

    #[test]
    fn nothing_prints_without_a_label_printer() {
        let registry = Registry::default();
        let (template, label) = milk_label(LabelLang::Fr);
        let error = registry
            .print_labels(
                &[LabelGroup {
                    template,
                    labels: vec![label],
                }],
                None,
            )
            .unwrap_err();
        assert_eq!(error.code(), "label_printer_not_configured");
    }

    #[test]
    fn label_file_printers_follow_the_data_folder_rule() {
        let dir = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let (registry, _) = with_label_printer(dir.path(), LabelPreset::Xprinter);
        let mut config = registry.config().unwrap();
        config.label_printers[0].transport = Transport::File {
            path: elsewhere
                .path()
                .join("x.prn")
                .to_string_lossy()
                .into_owned(),
        };
        assert_eq!(
            registry.save(config.clone()).unwrap_err().field(),
            Some("labelPrinters.0.transport.path")
        );
        assert_eq!(
            registry
                .label_test(&config.label_printers[0], Lang::Fr)
                .unwrap_err()
                .field(),
            Some("transport.path")
        );
        assert!(!elsewhere.path().join("x.prn").exists());
    }

    #[test]
    fn the_label_test_page_prints_on_an_unsaved_printer() {
        let dir = tempfile::tempdir().unwrap();
        let (registry, out) = with_label_printer(dir.path(), LabelPreset::Tsc);
        let printer = registry.config().unwrap().label_printers[0].clone();
        registry.label_test(&printer, Lang::Ar).unwrap();
        let text = crate::labels::dump(&std::fs::read(&out).unwrap());
        assert!(
            text.contains("\"EAN13\"")
                && text.contains("\"200000000000\"")
                && text.contains("PRINT 1,1"),
            "{text}"
        );
    }

    #[test]
    fn the_preview_is_a_png_at_the_printer_resolution() {
        let registry = Registry::default();
        let (template, label) = oil_shelf_label();
        let png = registry
            .label_preview(&template, &label, crate::labels::LabelDpi::D300)
            .unwrap();
        let image = image::load_from_memory(&png).unwrap();
        assert_eq!((image.width(), image.height()), (696, 480));
    }

    #[test]
    fn a_usb_drawer_adapter_opens_by_bytes_or_dtr() {
        let dir = tempfile::tempdir().unwrap();
        let links = MemoryLinks::new();
        links.wire("COM7");
        let registry = Registry::open_with(dir.path().join("hardware.json"), links.clone());
        let mut config = HardwareConfig {
            drawer: DrawerConfig::Serial {
                port: "COM7".into(),
                baud: 9_600,
                kick: SerialKick::EscPos,
            },
            ..HardwareConfig::default()
        };
        registry.save(config.clone()).unwrap();
        registry.open_drawer().unwrap();
        assert_eq!(links.written("COM7"), [0x1B, b'p', 0, 25, 250]);
        config.drawer = DrawerConfig::Serial {
            port: "COM7".into(),
            baud: 9_600,
            kick: SerialKick::Dtr,
        };
        registry.save(config).unwrap();
        registry.open_drawer().unwrap();
        let dtr = links.wire("COM7").lock().unwrap().dtr.clone();
        assert_eq!(dtr, [true, false]);
        config = registry.config().unwrap();
        config.drawer = DrawerConfig::Serial {
            port: "COM8".into(),
            baud: 9_600,
            kick: SerialKick::Dtr,
        };
        registry.save(config).unwrap();
        assert_eq!(
            registry.open_drawer().unwrap_err().code(),
            "port_unavailable"
        );
    }

    #[test]
    fn the_registry_hands_out_its_serial_links() {
        let links = MemoryLinks::new();
        links.wire("COM3");
        let dir = tempfile::tempdir().unwrap();
        let registry = Registry::open_with(dir.path().join("hardware.json"), links);
        assert!(
            registry
                .links()
                .open(&crate::serial::SerialSettings::eight_n1("COM3", 9_600))
                .is_ok()
        );
    }

    /// No printer by that name exists on any PC.
    const GHOST: &str = "DZBuild POS test: no such printer";

    fn windows_printer(id: &str) -> PrinterConfig {
        PrinterConfig {
            id: id.into(),
            name: "Caisse 1".into(),
            transport: Transport::Windows {
                printer_name: GHOST.into(),
            },
            profile: Profile::preset(Preset::Xprinter, PaperWidth::Mm80),
            mode: PrintMode::Raw,
            paper_width_mm: 80,
        }
    }

    #[test]
    fn a_driver_name_outlives_an_unplugged_printer_but_not_a_deleted_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hardware.json");
        let mut config = HardwareConfig::default();
        config.printers.push(windows_printer("p1"));
        config.driver_names.insert("p1".into(), "XP-80".into());
        config.driver_names.insert("gone".into(), "XP-58".into());
        config.save(&path).unwrap();
        let registry = Registry::open(&path);
        let mut sent = registry.config().unwrap();
        sent.driver_names.clear();
        let saved = registry.save(sent).unwrap();
        assert_eq!(
            saved.driver_names.into_iter().collect::<Vec<_>>(),
            [("p1".to_owned(), "XP-80".to_owned())],
            "the settings screen cannot erase it; a printer that left keeps its driver"
        );
        let mut tcp = registry.config().unwrap();
        tcp.printers[0].transport = Transport::Tcp {
            host: "192.168.1.50".into(),
            port: 9100,
        };
        assert!(registry.save(tcp).unwrap().driver_names.is_empty());
    }

    #[test]
    fn the_doctor_reports_a_printer_windows_no_longer_lists() {
        let registry = Registry::default();
        let mut config = HardwareConfig::default();
        config.printers.push(windows_printer("p1"));
        registry.save(config).unwrap();
        let issues = registry.doctor().unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(
            (issues[0].printer_id.as_str(), &issues[0].issue),
            ("p1", &crate::status::PrinterIssueKind::Missing)
        );
    }

    #[test]
    fn a_network_printer_tells_its_own_status() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let printer = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0_u8; 12];
            socket.read_exact(&mut request).unwrap();
            socket.write_all(&[0x12, 0x12, 0x12, 0x72]).unwrap();
            request
        });
        let registry = Registry::default();
        let mut config = HardwareConfig::default();
        config.printers.push(PrinterConfig {
            id: "p1".into(),
            name: "Caisse réseau".into(),
            transport: Transport::Tcp {
                host: "127.0.0.1".into(),
                port,
            },
            profile: Profile::preset(Preset::Xprinter, PaperWidth::Mm80),
            mode: PrintMode::Raw,
            paper_width_mm: 80,
        });
        registry.save(config).unwrap();
        let status = registry.printer_status("p1").unwrap();
        assert!(status.reachable && status.paper_out);
        assert_eq!(status.source, crate::status::StatusSource::Device);
        assert_eq!(printer.join().unwrap(), crate::status::DLE_EOT_ALL);
        assert_eq!(
            registry.printer_status("nope").unwrap_err().code(),
            "printer_not_found"
        );
    }

    #[test]
    fn a_file_printer_has_no_status_to_tell() {
        let dir = tempfile::tempdir().unwrap();
        let (registry, _) = with_label_printer(dir.path(), LabelPreset::Xprinter);
        assert_eq!(
            registry.printer_status("l1").unwrap(),
            crate::status::PrinterStatus::unknown()
        );
    }

    /// A receipt printer on file and the drawer on a USB/COM adapter on COM7 (HW-041).
    fn with_drawer_adapter(
        dir: &Path,
        links: Arc<dyn LinkFactory>,
        kick: SerialKick,
    ) -> (Registry, PathBuf) {
        let out = dir.join("printer.bin");
        let registry = Registry::open_with(dir.join("hardware.json"), links);
        let config = HardwareConfig {
            printers: vec![PrinterConfig {
                id: "p1".into(),
                name: "Caisse 1".into(),
                transport: file(&out),
                profile: Profile::preset(Preset::Generic, PaperWidth::Mm80),
                mode: PrintMode::Raw,
                paper_width_mm: 80,
            }],
            receipt_printer_id: Some("p1".into()),
            drawer: DrawerConfig::Serial {
                port: "COM7".into(),
                baud: 9_600,
                kick,
            },
            ..HardwareConfig::default()
        };
        registry.save(config).unwrap();
        (registry, out)
    }

    fn kicks(bytes: &[u8]) -> bool {
        bytes.windows(2).any(|w| w == [0x1B, b'p'])
    }

    #[test]
    fn a_cash_sale_opens_the_drawer_adapter_and_not_the_receipt_printer() {
        let dir = tempfile::tempdir().unwrap();
        let links = MemoryLinks::new();
        links.wire("COM7");
        let (registry, out) = with_drawer_adapter(dir.path(), links.clone(), SerialKick::EscPos);
        registry
            .print_receipt(&ticket(Lang::Fr), None, true)
            .unwrap();
        assert!(
            !kicks(&std::fs::read(&out).unwrap()),
            "the receipt printer has no drawer"
        );
        assert_eq!(links.written("COM7"), DRAWER_PULSE);
        let (_, rendered) = registry
            .receipt_bytes(&ticket(Lang::Fr), None, true)
            .unwrap();
        assert!(!kicks(&rendered.bytes));
        links.wire("COM7").lock().unwrap().written.clear();
        std::fs::remove_file(&out).unwrap();
        registry
            .print_receipt(&ticket(Lang::Fr), None, false)
            .unwrap();
        assert!(!kicks(&std::fs::read(&out).unwrap()));
        assert!(links.written("COM7").is_empty(), "no cash, no pulse");
        let mut config = registry.config().unwrap();
        config.receipt.open_drawer_on_cash = false;
        registry.save(config).unwrap();
        registry
            .print_receipt(&ticket(Lang::Fr), None, true)
            .unwrap();
        assert!(links.written("COM7").is_empty(), "the option is off");
    }

    /// Notes, each time a port opens, whether the receipt is already on file.
    struct PrintedYet {
        links: Arc<MemoryLinks>,
        receipt: PathBuf,
        seen: Mutex<Vec<bool>>,
    }

    impl LinkFactory for PrintedYet {
        fn open(&self, settings: &SerialSettings) -> Result<Box<dyn crate::serial::Link>, HwError> {
            self.seen.lock().unwrap().push(self.receipt.exists());
            self.links.open(settings)
        }
    }

    #[test]
    fn the_adapter_fires_after_the_receipt_or_before_it() {
        let dir = tempfile::tempdir().unwrap();
        let links = MemoryLinks::new();
        links.wire("COM7");
        let probe = Arc::new(PrintedYet {
            links: links.clone(),
            receipt: dir.path().join("printer.bin"),
            seen: Mutex::default(),
        });
        let (registry, out) = with_drawer_adapter(dir.path(), probe.clone(), SerialKick::Dtr);
        registry
            .print_receipt(&ticket(Lang::Fr), None, true)
            .unwrap();
        std::fs::remove_file(&out).unwrap();
        let mut config = registry.config().unwrap();
        config.receipt.drawer_before_print = true;
        registry.save(config).unwrap();
        registry
            .print_receipt(&ticket(Lang::Fr), None, true)
            .unwrap();
        assert_eq!(
            *probe.seen.lock().unwrap(),
            [true, false],
            "after the cut by default, before the receipt when asked"
        );
        assert_eq!(
            links.wire("COM7").lock().unwrap().dtr,
            [true, false, true, false]
        );
        assert!(!kicks(&std::fs::read(&out).unwrap()));
    }

    /// HW-002: a receipt printer that only takes pages gets no ESC/POS (the app prints the
    /// receipt as a page through its driver), and the cash still opens the drawer adapter.
    #[test]
    fn a_page_only_receipt_printer_still_opens_the_drawer_adapter() {
        let dir = tempfile::tempdir().unwrap();
        let links = MemoryLinks::new();
        links.wire("COM7");
        let (registry, out) = with_drawer_adapter(dir.path(), links.clone(), SerialKick::EscPos);
        let mut config = registry.config().unwrap();
        config.printers[0].transport = Transport::Windows {
            printer_name: "POS-58 (GDI)".into(),
        };
        config.printers[0].mode = PrintMode::Driver;
        config.printers[0].paper_width_mm = 58;
        registry.save(config).unwrap();
        assert_eq!(
            registry
                .print_receipt(&ticket(Lang::Fr), None, true)
                .unwrap_err(),
            HwError::DriverOnly {
                printer_name: "POS-58 (GDI)".into(),
                paper_width_mm: 58,
            }
        );
        assert_eq!(
            links.written("COM7"),
            DRAWER_PULSE,
            "the cash opens the drawer"
        );
        assert!(!out.exists());
        links.wire("COM7").lock().unwrap().written.clear();
        assert!(
            registry
                .print_receipt(&ticket(Lang::Fr), None, false)
                .is_err()
        );
        assert!(links.written("COM7").is_empty(), "no cash, no pulse");
    }

    #[test]
    fn the_drawer_adapter_and_the_receipt_printer_never_hold_each_other_up() {
        let dir = tempfile::tempdir().unwrap();
        let links = MemoryLinks::new();
        links.wire("COM7");
        let (registry, out) = with_drawer_adapter(dir.path(), links.clone(), SerialKick::EscPos);
        links.unplug("COM7", true);
        registry
            .print_receipt(&ticket(Lang::Fr), None, true)
            .unwrap();
        assert!(
            std::fs::read(&out).unwrap().starts_with(&[0x1B, b'@']),
            "an unplugged adapter never stops the receipt"
        );
        assert_eq!(
            registry.open_drawer().unwrap_err().code(),
            "port_unavailable",
            "the drawer button says why"
        );
        links.unplug("COM7", false);
        let broken = dir.path().join("broken.bin");
        std::fs::create_dir(&broken).unwrap();
        let mut config = registry.config().unwrap();
        config.printers[0].transport = file(&broken);
        registry.save(config).unwrap();
        assert!(
            registry
                .print_receipt(&ticket(Lang::Fr), None, true)
                .is_err()
        );
        assert_eq!(
            links.written("COM7"),
            DRAWER_PULSE,
            "cash moved: the drawer opens even when the printer fails"
        );
    }
}

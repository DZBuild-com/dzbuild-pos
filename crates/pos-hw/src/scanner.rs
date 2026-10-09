//! Serial (USB-COM) barcode scanners (HW-020): scanners set to their "USB virtual COM" mode type
//! nothing, so keyboard layouts never matter; the codes arrive on a port, end with a suffix, and
//! reach the till as `serial-scanned` events. HID (keyboard-wedge) scanners stay in the webview
//! (`src/lib/scanner.ts`).

use crate::device::{Backoff, DeviceKind, DeviceState, StatusSink, sleep_unless};
use crate::error::HwError;
use crate::serial::{Link, LinkFactory, SerialSettings};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub const CODE_MAX: usize = 64;
const SILENCE: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SerialSuffix {
    Cr,
    Lf,
    CrLf,
    Tab,
    /// No suffix: a code ends after 100 ms without bytes.
    Silence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SerialScannerConfig {
    pub serial: SerialSettings,
    /// Characters the scanner sends before each code (an AIM identifier such as `]E0`).
    pub prefix: String,
    pub suffix: SerialSuffix,
}

impl SerialScannerConfig {
    pub fn new(port: &str) -> SerialScannerConfig {
        SerialScannerConfig {
            serial: SerialSettings::eight_n1(port, 9_600),
            prefix: String::new(),
            suffix: SerialSuffix::Cr,
        }
    }

    pub fn validate(&self) -> Result<(), HwError> {
        self.serial.validate()?;
        if self.prefix.len() > 8 || !self.prefix.bytes().all(|b| (0x20..0x7F).contains(&b)) {
            return Err(HwError::setting("prefix"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ScanParser {
    prefix: Vec<u8>,
    suffix: SerialSuffix,
    buf: Vec<u8>,
    /// The current line outgrew any code: it is dropped whole at its end.
    runaway: bool,
}

impl ScanParser {
    pub fn new(cfg: &SerialScannerConfig) -> ScanParser {
        ScanParser {
            prefix: cfg.prefix.as_bytes().to_vec(),
            suffix: cfg.suffix,
            buf: Vec::new(),
            runaway: false,
        }
    }

    fn ends(&self, b: u8) -> bool {
        match self.suffix {
            SerialSuffix::Cr | SerialSuffix::Lf | SerialSuffix::CrLf => b == b'\r' || b == b'\n',
            SerialSuffix::Tab => b == b'\t',
            SerialSuffix::Silence => false,
        }
    }

    fn finish(&self, raw: &[u8]) -> Option<String> {
        let raw = raw.strip_prefix(self.prefix.as_slice()).unwrap_or(raw);
        let code: String = raw
            .iter()
            .filter(|b| (0x20..0x7F).contains(*b))
            .map(|b| char::from(*b))
            .collect();
        let code = code.trim();
        (!code.is_empty() && code.len() <= CODE_MAX).then(|| code.to_owned())
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Vec<String> {
        let mut out = Vec::new();
        for &b in bytes {
            if self.ends(b) {
                out.extend(self.flush());
            } else if self.buf.len() <= CODE_MAX + 16 {
                self.buf.push(b);
            } else {
                // a runaway line (no suffix, wrong settings): drop all of it, up to the next suffix
                self.buf.clear();
                self.runaway = true;
            }
        }
        out
    }

    /// Ends the current line (a suffix came, or the scanner fell silent): its code, or nothing
    /// for a runaway line.
    pub fn flush(&mut self) -> Option<String> {
        let raw = std::mem::take(&mut self.buf);
        if std::mem::take(&mut self.runaway) {
            return None;
        }
        self.finish(&raw)
    }
}

pub type ScanSink = Arc<dyn Fn(&str) + Send + Sync>;

pub struct SerialScanner {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl SerialScanner {
    pub fn start(
        cfg: SerialScannerConfig,
        links: Arc<dyn LinkFactory>,
        sink: ScanSink,
        status: StatusSink,
    ) -> Result<SerialScanner, HwError> {
        cfg.validate()?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("dzpos-scanner".into())
            .spawn(move || run(cfg, links, sink, status, flag))
            .map_err(|e| HwError::Io(e.to_string()))?;
        Ok(SerialScanner {
            stop,
            thread: Some(thread),
        })
    }

    pub fn stop(mut self) {
        self.halt();
    }

    fn halt(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for SerialScanner {
    fn drop(&mut self) {
        self.halt();
    }
}

fn run(
    cfg: SerialScannerConfig,
    links: Arc<dyn LinkFactory>,
    sink: ScanSink,
    status: StatusSink,
    stop: Arc<AtomicBool>,
) {
    let port = cfg.serial.port.clone();
    let mut parser = ScanParser::new(&cfg);
    let mut link: Option<Box<dyn Link>> = None;
    let mut backoff = Backoff::new();
    let mut last_byte = Instant::now();
    let mut buf = [0_u8; 128];
    let mut reported: Option<DeviceState> = None;
    let mut report = |state: DeviceState| {
        if reported.as_ref() != Some(&state) {
            status(DeviceKind::SerialScanner, &state);
            reported = Some(state);
        }
    };
    while !stop.load(Ordering::SeqCst) {
        if link.is_none() {
            match links.open(&cfg.serial) {
                Ok(opened) => {
                    link = Some(opened);
                    backoff.reset();
                    // half a code read before the cable came out is never glued to the next one
                    parser = ScanParser::new(&cfg);
                    report(DeviceState::Connected { port: port.clone() });
                }
                Err(error) => {
                    report(DeviceState::Disconnected {
                        port: port.clone(),
                        code: error.code().to_owned(),
                    });
                    sleep_unless(&stop, backoff.fail());
                }
            }
            continue;
        }
        let Some(l) = link.as_mut() else {
            continue;
        };
        match l.read(&mut buf, Duration::from_millis(50)) {
            Ok(0) => {
                if cfg.suffix == SerialSuffix::Silence
                    && last_byte.elapsed() >= SILENCE
                    && let Some(code) = parser.flush()
                {
                    sink(&code);
                }
            }
            Ok(n) => {
                last_byte = Instant::now();
                for code in parser.feed(&buf[..n]) {
                    sink(&code);
                }
            }
            Err(error) => {
                link = None;
                report(DeviceState::Disconnected {
                    port: port.clone(),
                    code: error.code().to_owned(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{DeviceKind, DeviceState, StatusSink};
    use crate::serial::MemoryLinks;
    use std::sync::mpsc;
    use std::time::Duration;

    fn parser(prefix: &str, suffix: SerialSuffix) -> ScanParser {
        ScanParser::new(&SerialScannerConfig {
            prefix: prefix.into(),
            suffix,
            ..SerialScannerConfig::new("COM4")
        })
    }

    #[test]
    fn line_suffixes_end_codes_and_prefixes_are_removed() {
        let mut p = parser("", SerialSuffix::Cr);
        assert_eq!(p.feed(b"6130123456782\r\n613"), ["6130123456782"]);
        assert_eq!(
            p.feed(b"0123456782\n"),
            ["6130123456782"],
            "a code split over two reads"
        );
        let mut prefixed = parser("]E0", SerialSuffix::CrLf);
        assert_eq!(prefixed.feed(b"]E06130123456782\r\n"), ["6130123456782"]);
        let mut tab = parser("", SerialSuffix::Tab);
        assert_eq!(tab.feed(b"P000123\tP000124\t"), ["P000123", "P000124"]);
        assert!(
            parser("", SerialSuffix::Cr).feed(b"\x01\x02\r").is_empty(),
            "control bytes only: no code"
        );
        let long = [b'1'; 80];
        let mut p = parser("", SerialSuffix::Cr);
        assert!(
            p.feed(&[&long[..], b"\r"].concat()).is_empty(),
            "longer than 64: dropped"
        );
    }

    #[test]
    fn a_runaway_line_is_dropped_whole_and_the_next_code_reads() {
        let runaway = [b'1'; 100];
        let mut p = parser("", SerialSuffix::Cr);
        assert_eq!(
            p.feed(&[&runaway[..], b"\r6130123456782\r"].concat()),
            ["6130123456782"],
            "no tail of the runaway line is taken for a code"
        );
        let mut silent = parser("", SerialSuffix::Silence);
        assert!(silent.feed(&runaway).is_empty());
        assert_eq!(silent.flush(), None, "a runaway burst is no code either");
        assert!(silent.feed(b"6130123456782").is_empty());
        assert_eq!(silent.flush().as_deref(), Some("6130123456782"));
    }

    #[test]
    fn a_silent_suffix_ends_the_code_after_a_pause() {
        let mut p = parser("", SerialSuffix::Silence);
        assert!(p.feed(b"6130123456782").is_empty());
        assert_eq!(p.flush().as_deref(), Some("6130123456782"));
        assert_eq!(p.flush(), None);
    }

    fn collect() -> (ScanSink, mpsc::Receiver<String>) {
        let (tx, rx) = mpsc::channel::<String>();
        let tx = std::sync::Mutex::new(tx);
        let sink: ScanSink = std::sync::Arc::new(move |code: &str| {
            if let Ok(tx) = tx.lock() {
                let _ = tx.send(code.to_owned());
            }
        });
        (sink, rx)
    }

    #[test]
    fn the_service_reads_the_port_and_calls_the_sink_once_per_code() {
        let links = MemoryLinks::new();
        links.feed("COM4", b"6130123456782\r\n");
        let (sink, rx) = collect();
        let scanner = SerialScanner::start(
            SerialScannerConfig::new("COM4"),
            links.clone(),
            sink,
            crate::device::silent_status(),
        )
        .unwrap();
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            "6130123456782"
        );
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err(), "once");
        scanner.stop();
    }

    #[test]
    fn a_scanner_without_a_suffix_sends_its_code_after_a_pause() {
        let links = MemoryLinks::new();
        links.feed("COM4", b"6130123456782");
        let (sink, rx) = collect();
        let _scanner = SerialScanner::start(
            SerialScannerConfig {
                suffix: SerialSuffix::Silence,
                ..SerialScannerConfig::new("COM4")
            },
            links.clone(),
            sink,
            crate::device::silent_status(),
        )
        .unwrap();
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            "6130123456782"
        );
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err(), "once");
    }

    #[test]
    fn an_unplugged_scanner_is_disconnected_and_reads_whole_codes_once_back() {
        const WAIT: Duration = Duration::from_secs(2);
        let links = MemoryLinks::new();
        links.feed("COM4", b"6130"); // the cable is pulled in the middle of a code
        let (states_tx, states) = mpsc::channel();
        let states_tx = std::sync::Mutex::new(states_tx);
        let status: StatusSink = std::sync::Arc::new(move |kind, state| {
            if let Ok(tx) = states_tx.lock() {
                let _ = tx.send((kind, state.clone()));
            }
        });
        let (sink, rx) = collect();
        let _scanner = SerialScanner::start(
            SerialScannerConfig::new("COM4"),
            links.clone(),
            sink,
            status,
        )
        .unwrap();
        let connected = (
            DeviceKind::SerialScanner,
            DeviceState::Connected {
                port: "COM4".into(),
            },
        );
        assert_eq!(states.recv_timeout(WAIT).unwrap(), connected);
        let wire = links.wire("COM4");
        let started = std::time::Instant::now();
        while !wire.lock().unwrap().incoming.is_empty() {
            assert!(started.elapsed() < WAIT, "the scanner reads its port");
            std::thread::sleep(Duration::from_millis(5));
        }
        links.unplug("COM4", true);
        let (kind, state) = states.recv_timeout(WAIT).unwrap();
        assert_eq!(kind, DeviceKind::SerialScanner);
        assert!(
            matches!(state, DeviceState::Disconnected { .. }),
            "{state:?}"
        );
        links.feed("COM4", b"6130123456782\r\n");
        links.unplug("COM4", false);
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(3)).unwrap(),
            "6130123456782",
            "the half code read before the unplug is not glued to the next one"
        );
        assert!(states.try_iter().any(|s| s == connected), "connected again");
    }

    #[test]
    fn a_missing_port_is_disconnected_without_panicking() {
        let (states_tx, states) = mpsc::channel();
        let states_tx = std::sync::Mutex::new(states_tx);
        let status: StatusSink = std::sync::Arc::new(move |kind, state| {
            if let Ok(tx) = states_tx.lock() {
                let _ = tx.send((kind, state.clone()));
            }
        });
        let (sink, _rx) = collect();
        let scanner = SerialScanner::start(
            SerialScannerConfig::new("COM42"),
            MemoryLinks::new(),
            sink,
            status,
        )
        .unwrap();
        assert_eq!(
            states.recv_timeout(Duration::from_secs(2)).unwrap(),
            (
                DeviceKind::SerialScanner,
                DeviceState::Disconnected {
                    port: "COM42".into(),
                    code: "port_unavailable".into(),
                }
            )
        );
        let started = std::time::Instant::now();
        scanner.stop();
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "stop returns promptly, backoff or not"
        );
    }

    #[test]
    fn configs_are_validated() {
        let field = |c: SerialScannerConfig| c.validate().unwrap_err().field().map(str::to_owned);
        assert_eq!(
            field(SerialScannerConfig {
                prefix: "123456789".into(),
                ..SerialScannerConfig::new("COM4")
            })
            .as_deref(),
            Some("prefix")
        );
        assert_eq!(
            field(SerialScannerConfig::new("")).as_deref(),
            Some("serial.port")
        );
    }
}

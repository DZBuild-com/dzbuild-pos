//! Serial ports for scales, pole displays, serial scanners and drawer adapters (HW-020, HW-030,
//! HW-041, HW-043): settings, a `Link` that reads with a timeout, the real ports (`serialport`)
//! and an in-memory double every test uses.

use crate::error::HwError;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, VecDeque};
use std::io::{ErrorKind, Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const BAUDS: [u32; 8] = [1_200, 2_400, 4_800, 9_600, 19_200, 38_400, 57_600, 115_200];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DataBits {
    Seven,
    Eight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Parity {
    None,
    Even,
    Odd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum StopBits {
    One,
    Two,
}

/// A port as Windows names it, whatever was typed: `com3 ` and `COM3` are one port.
pub fn port_key(port: &str) -> String {
    port.trim().to_ascii_uppercase()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SerialSettings {
    pub port: String,
    pub baud: u32,
    pub data_bits: DataBits,
    pub parity: Parity,
    pub stop_bits: StopBits,
}

impl SerialSettings {
    pub fn eight_n1(port: &str, baud: u32) -> SerialSettings {
        SerialSettings {
            port: port.to_owned(),
            baud,
            data_bits: DataBits::Eight,
            parity: Parity::None,
            stop_bits: StopBits::One,
        }
    }

    pub fn validate(&self) -> Result<(), HwError> {
        let port = self.port.trim();
        if port.is_empty() || port.len() > 64 || port.chars().any(char::is_control) {
            return Err(HwError::setting("serial.port"));
        }
        if !BAUDS.contains(&self.baud) {
            return Err(HwError::setting("serial.baud"));
        }
        Ok(())
    }
}

pub trait Link: Send {
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), HwError>;
    /// Waits at most `timeout`; 0 bytes means nothing arrived.
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, HwError>;
    fn clear_input(&mut self) -> Result<(), HwError>;
    fn set_dtr(&mut self, level: bool) -> Result<(), HwError>;
    fn describe(&self) -> String;
}

impl std::fmt::Debug for dyn Link {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.describe())
    }
}

pub trait LinkFactory: Send + Sync {
    fn open(&self, settings: &SerialSettings) -> Result<Box<dyn Link>, HwError>;
}

fn io(error: &std::io::Error) -> HwError {
    match error.kind() {
        ErrorKind::TimedOut | ErrorKind::WouldBlock => HwError::Timeout,
        ErrorKind::NotFound => HwError::PortUnavailable(error.to_string()),
        _ => HwError::Io(error.to_string()),
    }
}

/// The PC's real serial and USB-serial ports.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemLinks;

struct SystemLink {
    port: Box<dyn serialport::SerialPort>,
    name: String,
}

impl LinkFactory for SystemLinks {
    fn open(&self, settings: &SerialSettings) -> Result<Box<dyn Link>, HwError> {
        settings.validate()?;
        let port = serialport::new(settings.port.trim(), settings.baud)
            .data_bits(match settings.data_bits {
                DataBits::Seven => serialport::DataBits::Seven,
                DataBits::Eight => serialport::DataBits::Eight,
            })
            .parity(match settings.parity {
                Parity::None => serialport::Parity::None,
                Parity::Even => serialport::Parity::Even,
                Parity::Odd => serialport::Parity::Odd,
            })
            .stop_bits(match settings.stop_bits {
                StopBits::One => serialport::StopBits::One,
                StopBits::Two => serialport::StopBits::Two,
            })
            .flow_control(serialport::FlowControl::None)
            .timeout(Duration::from_millis(50))
            .open()
            .map_err(|e| match e.kind() {
                serialport::ErrorKind::NoDevice
                | serialport::ErrorKind::Io(ErrorKind::NotFound) => {
                    HwError::PortUnavailable(settings.port.clone())
                }
                _ => HwError::Io(e.to_string()),
            })?;
        Ok(Box::new(SystemLink {
            port,
            name: settings.port.trim().to_owned(),
        }))
    }
}

impl Link for SystemLink {
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), HwError> {
        self.port.write_all(bytes).map_err(|e| io(&e))?;
        self.port.flush().map_err(|e| io(&e))
    }

    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, HwError> {
        self.port
            .set_timeout(timeout)
            .map_err(|e| HwError::Io(e.to_string()))?;
        match self.port.read(buf) {
            Ok(n) => Ok(n),
            Err(e) if matches!(e.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) => Ok(0),
            Err(e) => Err(io(&e)),
        }
    }

    fn clear_input(&mut self) -> Result<(), HwError> {
        self.port
            .clear(serialport::ClearBuffer::Input)
            .map_err(|e| HwError::Io(e.to_string()))
    }

    fn set_dtr(&mut self, level: bool) -> Result<(), HwError> {
        self.port
            .write_data_terminal_ready(level)
            .map_err(|e| HwError::Io(e.to_string()))
    }

    fn describe(&self) -> String {
        self.name.clone()
    }
}

/// One simulated device on one port.
#[derive(Debug, Default)]
pub struct MemoryWire {
    /// What the device sent and the link has not read yet.
    pub incoming: VecDeque<u8>,
    /// Everything the link wrote.
    pub written: Vec<u8>,
    /// When the link writes exactly `.0`, `.1` is queued for reading.
    pub replies: Vec<(Vec<u8>, Vec<u8>)>,
    /// A streaming device: queued again whenever `incoming` runs dry.
    pub stream: Option<Vec<u8>>,
    /// The device answers only under these settings (baud, bits, parity); reads give nothing
    /// under any other.
    pub answers_to: Option<(u32, DataBits, Parity)>,
    pub unplugged: bool,
    pub opened_with: Option<SerialSettings>,
    pub opens: u32,
    pub dtr: Vec<bool>,
}

type Wire = Arc<Mutex<MemoryWire>>;

/// Simulated ports: tests and demos never need a device.
#[derive(Debug, Default)]
pub struct MemoryLinks {
    wires: Mutex<BTreeMap<String, Wire>>,
}

fn poisoned() -> HwError {
    HwError::Io("serial double lock poisoned".into())
}

impl MemoryLinks {
    pub fn new() -> Arc<MemoryLinks> {
        Arc::new(MemoryLinks::default())
    }

    /// The wire of `port`, created (plugged, silent) when missing.
    pub fn wire(&self, port: &str) -> Wire {
        match self.wires.lock() {
            Ok(mut wires) => wires.entry(port.to_owned()).or_default().clone(),
            Err(poisoned) => poisoned
                .into_inner()
                .entry(port.to_owned())
                .or_default()
                .clone(),
        }
    }

    fn with<T>(&self, port: &str, f: impl FnOnce(&mut MemoryWire) -> T) -> Option<T> {
        let wire = self.wire(port);
        let mut guard = wire.lock().ok()?;
        Some(f(&mut guard))
    }

    pub fn feed(&self, port: &str, bytes: &[u8]) {
        self.with(port, |w| w.incoming.extend(bytes.iter().copied()));
    }

    pub fn reply(&self, port: &str, request: &[u8], response: &[u8]) {
        self.with(port, |w| {
            w.replies.retain(|(r, _)| r != request);
            w.replies.push((request.to_vec(), response.to_vec()));
        });
    }

    pub fn stream(&self, port: &str, bytes: &[u8]) {
        self.with(port, |w| w.stream = Some(bytes.to_vec()));
    }

    pub fn unplug(&self, port: &str, unplugged: bool) {
        self.with(port, |w| w.unplugged = unplugged);
    }

    pub fn answer_only(&self, port: &str, baud: u32, bits: DataBits, parity: Parity) {
        self.with(port, |w| w.answers_to = Some((baud, bits, parity)));
    }

    pub fn written(&self, port: &str) -> Vec<u8> {
        self.with(port, |w| w.written.clone()).unwrap_or_default()
    }

    pub fn opened_with(&self, port: &str) -> Option<SerialSettings> {
        self.with(port, |w| w.opened_with.clone()).flatten()
    }
}

impl LinkFactory for MemoryLinks {
    fn open(&self, settings: &SerialSettings) -> Result<Box<dyn Link>, HwError> {
        settings.validate()?;
        let wire = self
            .wires
            .lock()
            .map_err(|_| poisoned())?
            .get(settings.port.trim())
            .cloned()
            .ok_or_else(|| HwError::PortUnavailable(settings.port.clone()))?;
        {
            let mut w = wire.lock().map_err(|_| poisoned())?;
            if w.unplugged {
                return Err(HwError::PortUnavailable(settings.port.clone()));
            }
            w.opened_with = Some(settings.clone());
            w.opens += 1;
        }
        Ok(Box::new(MemoryLink {
            wire,
            settings: settings.clone(),
        }))
    }
}

struct MemoryLink {
    wire: Wire,
    settings: SerialSettings,
}

impl MemoryLink {
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, MemoryWire>, HwError> {
        let guard = self.wire.lock().map_err(|_| poisoned())?;
        if guard.unplugged {
            return Err(HwError::Io(format!("{} was unplugged", self.settings.port)));
        }
        Ok(guard)
    }
}

impl Link for MemoryLink {
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), HwError> {
        let mut w = self.lock()?;
        w.written.extend_from_slice(bytes);
        let answer = w
            .replies
            .iter()
            .find(|(r, _)| r == bytes)
            .map(|(_, a)| a.clone());
        if let Some(answer) = answer {
            w.incoming.extend(answer);
        }
        Ok(())
    }

    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, HwError> {
        let n = {
            let mut w = self.lock()?;
            let heard = w.answers_to.is_none_or(|(baud, bits, parity)| {
                (baud, bits, parity)
                    == (
                        self.settings.baud,
                        self.settings.data_bits,
                        self.settings.parity,
                    )
            });
            if heard
                && w.incoming.is_empty()
                && let Some(stream) = w.stream.clone()
            {
                w.incoming.extend(stream);
            }
            let n = if heard {
                buf.len().min(w.incoming.len())
            } else {
                0
            };
            for slot in buf.iter_mut().take(n) {
                *slot = w.incoming.pop_front().unwrap_or(0);
            }
            n
        };
        if n == 0 {
            std::thread::sleep(timeout.min(Duration::from_millis(5)));
        }
        Ok(n)
    }

    fn clear_input(&mut self) -> Result<(), HwError> {
        self.lock()?.incoming.clear();
        Ok(())
    }

    fn set_dtr(&mut self, level: bool) -> Result<(), HwError> {
        self.lock()?.dtr.push(level);
        Ok(())
    }

    fn describe(&self) -> String {
        format!("memory:{}", self.settings.port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const T: Duration = Duration::from_millis(20);

    #[test]
    fn memory_links_answer_requests_and_record_what_was_written() {
        let links = MemoryLinks::new();
        links.reply("COM5", b"W", b"\x0201.250\r");
        let mut link = links
            .open(&SerialSettings::eight_n1("COM5", 9_600))
            .unwrap();
        link.write_all(b"W").unwrap();
        let mut buf = [0_u8; 32];
        let n = link.read(&mut buf, T).unwrap();
        assert_eq!(&buf[..n], b"\x0201.250\r");
        assert_eq!(
            link.read(&mut buf, T).unwrap(),
            0,
            "nothing more: a timeout reads 0"
        );
        assert_eq!(links.written("COM5"), b"W");
        links.stream("COM6", b"ST,GS,+0001.234kg\r\n");
        let mut stream = links
            .open(&SerialSettings::eight_n1("COM6", 9_600))
            .unwrap();
        let mut total = Vec::new();
        for _ in 0..3 {
            let n = stream.read(&mut buf, T).unwrap();
            total.extend_from_slice(&buf[..n]);
        }
        assert!(total.len() >= 19, "a streaming device never runs dry");
    }

    #[test]
    fn a_missing_or_unplugged_port_fails_without_panicking() {
        let links = MemoryLinks::new();
        let settings = SerialSettings::eight_n1("COM9", 9_600);
        assert_eq!(
            links.open(&settings).unwrap_err().code(),
            "port_unavailable"
        );
        links.wire("COM9");
        let mut link = links.open(&settings).unwrap();
        links.unplug("COM9", true);
        assert_eq!(link.write_all(b"x").unwrap_err().code(), "printer_io_error");
        assert_eq!(
            links.open(&settings).unwrap_err().code(),
            "port_unavailable"
        );
    }

    #[test]
    fn a_device_answers_only_under_its_own_settings() {
        let links = MemoryLinks::new();
        links.answer_only("COM3", 9_600, DataBits::Seven, Parity::Even);
        links.feed("COM3", b"\x0201.000\r");
        let mut buf = [0_u8; 16];
        let mut wrong = links
            .open(&SerialSettings::eight_n1("COM3", 9_600))
            .unwrap();
        assert_eq!(wrong.read(&mut buf, T).unwrap(), 0);
        let right = SerialSettings {
            data_bits: DataBits::Seven,
            parity: Parity::Even,
            ..SerialSettings::eight_n1("COM3", 9_600)
        };
        let mut link = links.open(&right).unwrap();
        assert!(link.read(&mut buf, T).unwrap() > 0);
    }

    #[test]
    fn settings_are_validated_and_cross_the_ipc_in_camel_case() {
        let field = |s: SerialSettings| s.validate().unwrap_err().field().map(str::to_owned);
        assert_eq!(
            field(SerialSettings::eight_n1(" ", 9_600)).as_deref(),
            Some("serial.port")
        );
        assert_eq!(
            field(SerialSettings::eight_n1("COM3", 1_234)).as_deref(),
            Some("serial.baud")
        );
        SerialSettings::eight_n1("COM3", 2_400).validate().unwrap();
        let json = serde_json::to_value(SerialSettings::eight_n1("COM3", 9_600)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "port": "COM3", "baud": 9600, "dataBits": "eight", "parity": "none", "stopBits": "one" })
        );
    }

    #[test]
    fn a_real_missing_port_is_unavailable() {
        let port = if cfg!(windows) {
            "COM250"
        } else {
            "/dev/ttyDZPOS-none"
        };
        assert_eq!(
            SystemLinks
                .open(&SerialSettings::eight_n1(port, 9_600))
                .unwrap_err()
                .code(),
            "port_unavailable"
        );
    }
}

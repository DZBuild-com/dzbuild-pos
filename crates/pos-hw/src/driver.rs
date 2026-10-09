//! Where a job's bytes go (spec §9.1, HW-001): the Windows spooler in RAW mode (default: works
//! with the USB driver the shop installed), a network printer's raw port, a serial or virtual
//! COM port, or a file. Every transport gives up after [`IO_TIMEOUT`].

use crate::error::HwError;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

#[cfg(windows)]
mod winspool;

pub const IO_TIMEOUT: Duration = Duration::from_secs(5);

/// How long a printer has to answer a status query.
pub const REPLY_TIMEOUT: Duration = Duration::from_secs(1);

/// Speeds receipt printers and USB-serial adapters use.
pub const SERIAL_BAUDS: [u32; 6] = [9_600, 19_200, 38_400, 57_600, 115_200, 4_800];

pub trait Driver: Send {
    /// Sends one whole job; returns once the transport took every byte.
    fn send(&mut self, bytes: &[u8]) -> Result<(), HwError>;
    /// "XP-80C", "192.168.1.50:9100", "COM3"… for logs and outcomes.
    fn describe(&self) -> String;
    /// Sends `request` and reads `reply_len` bytes back (printer status). Only transports that
    /// can read answer; the spooler cannot.
    fn query(&mut self, _request: &[u8], _reply_len: usize) -> Result<Vec<u8>, HwError> {
        Err(HwError::Unsupported)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum Transport {
    /// A printer installed in Windows, fed RAW through its spooler queue.
    Windows { printer_name: String },
    /// A network printer on its raw port (9100 by default).
    Tcp { host: String, port: u16 },
    /// A serial or virtual COM port (USB-serial adapters).
    Serial { port: String, baud: u32 },
    /// Appends the bytes to a file: support ("print to file") and tests.
    File { path: String },
}

impl Transport {
    pub fn validate(&self) -> Result<(), HwError> {
        match self {
            Transport::Windows { printer_name } if printer_name.trim().is_empty() => {
                Err(HwError::invalid("transport.printerName"))
            }
            Transport::Tcp { host, .. } if host.trim().is_empty() || host.len() > 253 => {
                Err(HwError::invalid("transport.host"))
            }
            Transport::Tcp { port: 0, .. } => Err(HwError::invalid("transport.port")),
            Transport::Serial { port, .. } if port.trim().is_empty() => {
                Err(HwError::invalid("transport.port"))
            }
            Transport::Serial { baud, .. } if !SERIAL_BAUDS.contains(baud) => {
                Err(HwError::invalid("transport.baud"))
            }
            Transport::File { path } if path.trim().is_empty() => {
                Err(HwError::invalid("transport.path"))
            }
            _ => Ok(()),
        }
    }

    pub fn label(&self) -> String {
        match self {
            Transport::Windows { printer_name } => printer_name.clone(),
            Transport::Tcp { host, port } => format!("{host}:{port}"),
            Transport::Serial { port, baud } => format!("{port} ({baud})"),
            Transport::File { path } => path.clone(),
        }
    }
}

pub fn open(transport: &Transport) -> Result<Box<dyn Driver>, HwError> {
    transport.validate()?;
    Ok(match transport {
        #[cfg(windows)]
        Transport::Windows { printer_name } => {
            Box::new(winspool::WinspoolDriver::new(printer_name))
        }
        #[cfg(not(windows))]
        Transport::Windows { .. } => return Err(HwError::Unsupported),
        Transport::Tcp { host, port } => Box::new(TcpDriver {
            host: host.trim().to_owned(),
            port: *port,
        }),
        Transport::Serial { port, baud } => Box::new(SerialDriver {
            port: port.trim().to_owned(),
            baud: *baud,
        }),
        Transport::File { path } => Box::new(FileDriver { path: path.clone() }),
    })
}

/// `(PRINTER_INFO_2W.Status, .Attributes)` of a Windows printer.
pub fn spooler_status(printer_name: &str) -> Result<(u32, u32), HwError> {
    #[cfg(windows)]
    {
        winspool::status(printer_name)
    }
    #[cfg(not(windows))]
    {
        let _ = printer_name;
        Err(HwError::Unsupported)
    }
}

fn io_error(error: &std::io::Error) -> HwError {
    match error.kind() {
        ErrorKind::TimedOut | ErrorKind::WouldBlock => HwError::Timeout,
        ErrorKind::ConnectionRefused
        | ErrorKind::ConnectionReset
        | ErrorKind::ConnectionAborted
        | ErrorKind::HostUnreachable
        | ErrorKind::NetworkUnreachable
        | ErrorKind::NotConnected => HwError::Offline(error.to_string()),
        _ => HwError::Io(error.to_string()),
    }
}

struct TcpDriver {
    host: String,
    port: u16,
}

impl TcpDriver {
    fn connect(&self) -> Result<TcpStream, HwError> {
        let address = (self.host.as_str(), self.port)
            .to_socket_addrs()
            .map_err(|e| HwError::Offline(e.to_string()))?
            .next()
            .ok_or_else(|| HwError::Offline(format!("{} has no address", self.host)))?;
        let stream = TcpStream::connect_timeout(&address, IO_TIMEOUT).map_err(|e| io_error(&e))?;
        stream
            .set_write_timeout(Some(IO_TIMEOUT))
            .map_err(|e| io_error(&e))?;
        Ok(stream)
    }
}

impl Driver for TcpDriver {
    fn send(&mut self, bytes: &[u8]) -> Result<(), HwError> {
        let mut stream = self.connect()?;
        stream.write_all(bytes).map_err(|e| io_error(&e))?;
        stream.flush().map_err(|e| io_error(&e))?;
        // Closing the write side tells printers that wait for the end of the job.
        let _ = stream.shutdown(std::net::Shutdown::Write);
        Ok(())
    }

    fn describe(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    fn query(&mut self, request: &[u8], reply_len: usize) -> Result<Vec<u8>, HwError> {
        let mut stream = self.connect()?;
        stream
            .set_read_timeout(Some(REPLY_TIMEOUT))
            .map_err(|e| io_error(&e))?;
        stream.write_all(request).map_err(|e| io_error(&e))?;
        let mut reply = vec![0_u8; reply_len];
        stream.read_exact(&mut reply).map_err(|e| io_error(&e))?;
        Ok(reply)
    }
}

struct SerialDriver {
    port: String,
    baud: u32,
}

impl SerialDriver {
    fn connect(&self, timeout: Duration) -> Result<Box<dyn serialport::SerialPort>, HwError> {
        serialport::new(self.port.as_str(), self.baud)
            .timeout(timeout)
            .open()
            .map_err(|e| match e.kind() {
                serialport::ErrorKind::NoDevice
                | serialport::ErrorKind::Io(ErrorKind::NotFound) => {
                    HwError::PortUnavailable(self.port.clone())
                }
                serialport::ErrorKind::Io(ErrorKind::TimedOut) => HwError::Timeout,
                _ => HwError::Io(e.to_string()),
            })
    }
}

impl Driver for SerialDriver {
    fn send(&mut self, bytes: &[u8]) -> Result<(), HwError> {
        let mut port = self.connect(IO_TIMEOUT)?;
        port.write_all(bytes).map_err(|e| io_error(&e))?;
        port.flush().map_err(|e| io_error(&e))
    }

    fn describe(&self) -> String {
        self.port.clone()
    }

    fn query(&mut self, request: &[u8], reply_len: usize) -> Result<Vec<u8>, HwError> {
        let mut port = self.connect(REPLY_TIMEOUT)?;
        port.write_all(request).map_err(|e| io_error(&e))?;
        port.flush().map_err(|e| io_error(&e))?;
        let mut reply = vec![0_u8; reply_len];
        port.read_exact(&mut reply).map_err(|e| io_error(&e))?;
        Ok(reply)
    }
}

struct FileDriver {
    path: String,
}

impl Driver for FileDriver {
    fn send(&mut self, bytes: &[u8]) -> Result<(), HwError> {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| HwError::Io(e.to_string()))?;
        file.write_all(bytes)
            .map_err(|e| HwError::Io(e.to_string()))?;
        file.sync_all().map_err(|e| HwError::Io(e.to_string()))
    }

    fn describe(&self) -> String {
        self.path.clone()
    }
}

/// Keeps every job in memory: tests and previews.
#[derive(Debug, Default)]
pub struct MemoryDriver {
    pub jobs: Vec<Vec<u8>>,
}

impl Driver for MemoryDriver {
    fn send(&mut self, bytes: &[u8]) -> Result<(), HwError> {
        self.jobs.push(bytes.to_vec());
        Ok(())
    }

    fn describe(&self) -> String {
        "memory".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::net::TcpListener;

    #[test]
    fn tcp_driver_sends_the_whole_job() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let reader = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut received = Vec::new();
            socket.read_to_end(&mut received).unwrap();
            received
        });
        let job: Vec<u8> = (0..=255).cycle().take(10_000).collect();
        let mut driver = open(&Transport::Tcp {
            host: "127.0.0.1".into(),
            port,
        })
        .unwrap();
        driver.send(&job).unwrap();
        drop(driver);
        assert_eq!(reader.join().unwrap(), job);
    }

    #[test]
    fn a_tcp_printer_answers_a_status_query() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let printer = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0_u8; 3];
            socket.read_exact(&mut request).unwrap();
            socket.write_all(&[0x16, 0x12]).unwrap();
            request
        });
        let mut driver = open(&Transport::Tcp {
            host: "127.0.0.1".into(),
            port,
        })
        .unwrap();
        assert_eq!(driver.query(&[0x10, 0x04, 1], 2).unwrap(), [0x16, 0x12]);
        assert_eq!(printer.join().unwrap(), [0x10, 0x04, 1]);
    }

    #[test]
    fn a_silent_tcp_printer_times_out_within_a_second_or_two() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let printer = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0_u8; 3];
            socket.read_exact(&mut request).unwrap();
            // holds the connection open without answering
            std::thread::sleep(Duration::from_millis(1_500));
        });
        let mut driver = open(&Transport::Tcp {
            host: "127.0.0.1".into(),
            port,
        })
        .unwrap();
        let started = std::time::Instant::now();
        assert_eq!(
            driver.query(&[0x10, 0x04, 1], 1).unwrap_err().code(),
            "printer_timeout"
        );
        assert!(started.elapsed() < Duration::from_secs(2));
        printer.join().unwrap();
    }

    #[test]
    fn transports_that_cannot_read_have_no_status() {
        let dir = tempfile::tempdir().unwrap();
        let mut file = open(&Transport::File {
            path: dir.path().join("x.bin").to_string_lossy().into_owned(),
        })
        .unwrap();
        assert_eq!(
            file.query(b"x", 1).unwrap_err().code(),
            "hardware_unsupported"
        );
        assert_eq!(
            MemoryDriver::default().query(b"x", 1).unwrap_err().code(),
            "hardware_unsupported"
        );
    }

    #[test]
    fn an_unknown_printer_has_no_spooler_status() {
        let code = spooler_status("DZBuild POS test: no such printer")
            .unwrap_err()
            .code();
        if cfg!(windows) {
            assert_eq!(code, "printer_not_found");
        } else {
            assert_eq!(code, "hardware_unsupported");
        }
    }

    #[test]
    fn a_closed_tcp_port_is_an_offline_printer() {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        // the listener is dropped: nothing listens on `port` any more
        let mut driver = open(&Transport::Tcp {
            host: "127.0.0.1".into(),
            port,
        })
        .unwrap();
        assert_eq!(driver.send(b"x").unwrap_err().code(), "printer_offline");
    }

    #[test]
    fn file_driver_appends_each_job() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("receipts.bin");
        let transport = Transport::File {
            path: path.to_string_lossy().into_owned(),
        };
        open(&transport).unwrap().send(b"one").unwrap();
        open(&transport).unwrap().send(b"two").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"onetwo");
    }

    #[test]
    fn memory_driver_keeps_the_jobs() {
        let mut memory = MemoryDriver::default();
        memory.send(b"a").unwrap();
        memory.send(b"bc").unwrap();
        assert_eq!(memory.jobs, [b"a".to_vec(), b"bc".to_vec()]);
    }

    #[test]
    fn transports_are_validated_before_opening() {
        let field = |t: Transport| t.validate().unwrap_err().field().map(str::to_owned);
        assert_eq!(
            field(Transport::Windows {
                printer_name: " ".into()
            })
            .as_deref(),
            Some("transport.printerName")
        );
        assert_eq!(
            field(Transport::Tcp {
                host: "".into(),
                port: 9100
            })
            .as_deref(),
            Some("transport.host")
        );
        assert_eq!(
            field(Transport::Tcp {
                host: "192.168.1.50".into(),
                port: 0
            })
            .as_deref(),
            Some("transport.port")
        );
        assert_eq!(
            field(Transport::Serial {
                port: "COM3".into(),
                baud: 1234
            })
            .as_deref(),
            Some("transport.baud")
        );
        assert_eq!(
            field(Transport::Serial {
                port: "".into(),
                baud: 9600
            })
            .as_deref(),
            Some("transport.port")
        );
        assert_eq!(
            field(Transport::File { path: "".into() }).as_deref(),
            Some("transport.path")
        );
        Transport::Tcp {
            host: "192.168.1.50".into(),
            port: 9100,
        }
        .validate()
        .unwrap();
        assert!(
            open(&Transport::Tcp {
                host: "".into(),
                port: 9100
            })
            .is_err()
        );
    }

    #[test]
    fn a_missing_serial_port_is_unavailable() {
        let port = if cfg!(windows) {
            "COM250"
        } else {
            "/dev/ttyDZPOS-none"
        };
        let mut driver = open(&Transport::Serial {
            port: port.into(),
            baud: 9600,
        })
        .unwrap();
        assert_eq!(driver.send(b"x").unwrap_err().code(), "port_unavailable");
    }

    #[test]
    fn transports_cross_the_ipc_tagged_by_kind() {
        let json = serde_json::to_value(Transport::Windows {
            printer_name: "XP-80C".into(),
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "kind": "windows", "printerName": "XP-80C" })
        );
        let back: Transport = serde_json::from_value(
            serde_json::json!({ "kind": "tcp", "host": "10.0.0.9", "port": 9100 }),
        )
        .unwrap();
        assert_eq!(
            back,
            Transport::Tcp {
                host: "10.0.0.9".into(),
                port: 9100
            }
        );
    }

    #[cfg(windows)]
    #[test]
    fn an_unknown_windows_printer_is_reported() {
        let mut driver = open(&Transport::Windows {
            printer_name: "DZBuild POS test: no such printer".into(),
        })
        .unwrap();
        assert_eq!(driver.send(b"x").unwrap_err().code(), "printer_not_found");
    }
}

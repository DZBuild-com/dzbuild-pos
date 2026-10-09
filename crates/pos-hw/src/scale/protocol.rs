//! Scale protocols as pure frame parsers: a continuous ASCII stream (the cheap indicators, A&D
//! style `ST,GS,+0001.234kg` or plain `1.234 kg`), Mettler Toledo 8217 on demand (`W`), and CAS
//! ECR with its ENQ/ACK/DC1 handshake. Weights are grams (milli-units of kg).

use crate::serial::{DataBits, Parity, SerialSettings, StopBits};
use pos_core::money::Qty;
use serde::{Deserialize, Serialize};
use specta::Type;

const STX: u8 = 0x02;
const ETX: u8 = 0x03;
const EOT: u8 = 0x04;
const ENQ: u8 = 0x05;
const ACK: u8 = 0x06;
const SOH: u8 = 0x01;
const DC1: u8 = 0x11;
const CR: u8 = 0x0D;
const BUFFER_MAX: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ScaleProtocol {
    Continuous,
    Toledo8217,
    CasEcr,
}

impl ScaleProtocol {
    pub const ALL: [ScaleProtocol; 3] = [
        ScaleProtocol::Continuous,
        ScaleProtocol::Toledo8217,
        ScaleProtocol::CasEcr,
    ];

    /// Each protocol's usual settings: Toledo 8217 at 9600 7E1, the others 9600 8N1.
    pub fn serial(self, port: &str) -> SerialSettings {
        match self {
            ScaleProtocol::Toledo8217 => SerialSettings {
                port: port.to_owned(),
                baud: 9_600,
                data_bits: DataBits::Seven,
                parity: Parity::Even,
                stop_bits: StopBits::One,
            },
            ScaleProtocol::Continuous | ScaleProtocol::CasEcr => {
                SerialSettings::eight_n1(port, 9_600)
            }
        }
    }

    pub const fn supports_zero(self) -> bool {
        matches!(self, ScaleProtocol::Toledo8217)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ReadingStatus {
    Ok,
    Motion,
    Overload,
    Underload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reading {
    pub weight: Option<Qty>,
    /// `None` when the protocol does not say (the service then waits for equal readings).
    pub stable: Option<bool>,
    pub status: ReadingStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    Reading(Reading),
    /// Bytes to send back now (a handshake step).
    Send(Vec<u8>),
}

pub trait Protocol: Send {
    /// Bytes to send at each poll; `None` for a streaming scale.
    fn poll(&mut self) -> Option<Vec<u8>>;
    fn feed(&mut self, bytes: &[u8]) -> Vec<Parsed>;
    fn zero(&self) -> Option<Vec<u8>>;
    /// Forget a half-read frame (after a timeout or a reconnection).
    fn reset(&mut self);
}

pub fn protocol(kind: ScaleProtocol) -> Box<dyn Protocol> {
    match kind {
        ScaleProtocol::Continuous => Box::new(Continuous::default()),
        ScaleProtocol::Toledo8217 => Box::new(Toledo8217::default()),
        ScaleProtocol::CasEcr => Box::new(CasEcr::default()),
    }
}

fn keep_tail(buf: &mut Vec<u8>) {
    if buf.len() > BUFFER_MAX {
        let extra = buf.len() - BUFFER_MAX;
        buf.drain(..extra);
    }
}

/// `number` in `unit` (kg when `None`) as grams, the fraction rounded half up to the gram.
pub fn parse_weight(number: &str, unit: Option<&str>) -> Option<Qty> {
    let n = number.trim();
    let (negative, digits) = match n.strip_prefix('-') {
        Some(rest) => (true, rest.trim()),
        None => (false, n.strip_prefix('+').unwrap_or(n).trim()),
    };
    let (int, frac) = match digits.find(['.', ',']) {
        Some(i) => (digits.get(..i)?, digits.get(i + 1..)?),
        None => (digits, ""),
    };
    let all_digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if (int.is_empty() && frac.is_empty())
        || !all_digits(int)
        || !all_digits(frac)
        || int.len() > 7
        || frac.len() > 6
    {
        return None;
    }
    let per_unit: i64 = match unit.map(str::to_ascii_lowercase).as_deref() {
        None | Some("kg") => 1_000,
        Some("g") => 1,
        _ => return None,
    };
    let whole: i64 = if int.is_empty() { 0 } else { int.parse().ok()? };
    let mut grams = whole.checked_mul(per_unit)?;
    if !frac.is_empty() {
        let kept_digits = if per_unit == 1_000 { 3 } else { 0 };
        let padded: String = frac
            .chars()
            .chain(std::iter::repeat('0'))
            .take(kept_digits + 1)
            .collect();
        let (kept, next) = padded.split_at(kept_digits);
        let kept: i64 = if kept.is_empty() {
            0
        } else {
            kept.parse().ok()?
        };
        let up = next.bytes().next().is_some_and(|b| b >= b'5');
        grams = grams.checked_add(kept + i64::from(up))?;
    }
    Some(Qty(if negative { -grams } else { grams }))
}

fn status_of(weight: Qty, stable: Option<bool>) -> ReadingStatus {
    if weight.0 < 0 {
        ReadingStatus::Underload
    } else if stable == Some(false) {
        ReadingStatus::Motion
    } else {
        ReadingStatus::Ok
    }
}

#[derive(Debug, Default)]
struct Continuous {
    buf: Vec<u8>,
}

/// One line of a streaming indicator.
fn continuous_line(text: &str) -> Option<Reading> {
    let mut stable = None;
    let mut overload = false;
    let mut sign = "";
    let mut number: Option<(String, Option<String>)> = None;
    for token in text.split([',', ' ', '\t']).filter(|t| !t.is_empty()) {
        match token {
            "ST" => stable = Some(true),
            "US" => stable = Some(false),
            "OL" => overload = true,
            "+" | "-" => sign = token,
            _ if number.is_none()
                && token
                    .starts_with(|c: char| c.is_ascii_digit() || matches!(c, '+' | '-' | '.')) =>
            {
                let split = token
                    .find(|c: char| c.is_ascii_alphabetic())
                    .unwrap_or(token.len());
                let (value, unit) = token.split_at(split);
                number = Some((
                    format!("{sign}{value}"),
                    (!unit.is_empty()).then(|| unit.to_owned()),
                ));
            }
            "KG" | "G" | "LB" => {
                if let Some((_, unit @ None)) = &mut number {
                    *unit = Some(token.to_owned());
                }
            }
            _ => {}
        }
    }
    if overload {
        return Some(Reading {
            weight: None,
            stable,
            status: ReadingStatus::Overload,
        });
    }
    let (value, unit) = number?;
    let weight = parse_weight(&value, unit.as_deref())?;
    Some(Reading {
        weight: Some(weight),
        stable,
        status: status_of(weight, stable),
    })
}

impl Protocol for Continuous {
    fn poll(&mut self) -> Option<Vec<u8>> {
        None
    }

    fn feed(&mut self, bytes: &[u8]) -> Vec<Parsed> {
        self.buf.extend_from_slice(bytes);
        keep_tail(&mut self.buf);
        let mut out = Vec::new();
        while let Some(end) = self.buf.iter().position(|b| *b == b'\r' || *b == b'\n') {
            let line: Vec<u8> = self.buf.drain(..=end).collect();
            let text = String::from_utf8_lossy(&line[..line.len() - 1])
                .trim()
                .to_ascii_uppercase();
            if let Some(reading) = continuous_line(&text) {
                out.push(Parsed::Reading(reading));
            }
        }
        out
    }

    fn zero(&self) -> Option<Vec<u8>> {
        None
    }

    fn reset(&mut self) {
        self.buf.clear();
    }
}

#[derive(Debug, Default)]
struct Toledo8217 {
    buf: Vec<u8>,
}

impl Protocol for Toledo8217 {
    fn poll(&mut self) -> Option<Vec<u8>> {
        Some(b"W".to_vec())
    }

    fn feed(&mut self, bytes: &[u8]) -> Vec<Parsed> {
        self.buf.extend_from_slice(bytes);
        keep_tail(&mut self.buf);
        let mut out = Vec::new();
        loop {
            let Some(start) = self.buf.iter().position(|b| *b == STX) else {
                self.buf.clear();
                break;
            };
            self.buf.drain(..start);
            let Some(end) = self.buf.iter().position(|b| *b == CR) else {
                break;
            };
            let frame: Vec<u8> = self.buf.drain(..=end).collect();
            let body = &frame[1..frame.len() - 1];
            let reading = match body {
                [b'?', status, ..] => {
                    let status = if status & 0x02 != 0 {
                        ReadingStatus::Overload
                    } else if status & 0x04 != 0 {
                        ReadingStatus::Underload
                    } else {
                        ReadingStatus::Motion
                    };
                    Some(Reading {
                        weight: None,
                        stable: Some(false),
                        status,
                    })
                }
                _ => {
                    let text = String::from_utf8_lossy(body);
                    parse_weight(
                        text.trim()
                            .trim_end_matches(|c: char| c.is_ascii_alphabetic()),
                        None,
                    )
                    .map(|w| Reading {
                        weight: Some(w),
                        stable: Some(true),
                        status: status_of(w, Some(true)),
                    })
                }
            };
            out.extend(reading.map(Parsed::Reading));
        }
        out
    }

    fn zero(&self) -> Option<Vec<u8>> {
        Some(b"Z".to_vec())
    }

    fn reset(&mut self) {
        self.buf.clear();
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
enum CasState {
    #[default]
    Idle,
    AwaitAck,
    AwaitFrame,
}

#[derive(Debug, Default)]
struct CasEcr {
    state: CasState,
    buf: Vec<u8>,
}

const CAS_FRAME: usize = 15;

impl Protocol for CasEcr {
    fn poll(&mut self) -> Option<Vec<u8>> {
        self.state = CasState::AwaitAck;
        self.buf.clear();
        Some(vec![ENQ])
    }

    fn feed(&mut self, bytes: &[u8]) -> Vec<Parsed> {
        let mut out = Vec::new();
        if self.state == CasState::Idle {
            return out;
        }
        self.buf.extend_from_slice(bytes);
        keep_tail(&mut self.buf);
        if self.state == CasState::AwaitAck {
            let Some(ack) = self.buf.iter().position(|b| *b == ACK) else {
                return out;
            };
            self.buf.drain(..=ack);
            self.state = CasState::AwaitFrame;
            out.push(Parsed::Send(vec![DC1]));
        }
        let Some(start) = self.buf.iter().position(|b| *b == SOH) else {
            return out;
        };
        self.buf.drain(..start);
        if self.buf.len() < CAS_FRAME {
            return out;
        }
        let frame: Vec<u8> = self.buf.drain(..CAS_FRAME).collect();
        self.state = CasState::Idle;
        if frame[1] != STX || frame[13] != ETX || frame[14] != EOT {
            return out;
        }
        let stable = match frame[2] {
            b'S' => Some(true),
            b'U' => Some(false),
            _ => None,
        };
        let sign = if frame[3] == b'-' { "-" } else { "" };
        let value = String::from_utf8_lossy(&frame[4..10]);
        let unit = String::from_utf8_lossy(&frame[10..12]).trim().to_owned();
        if let Some(weight) = parse_weight(&format!("{sign}{}", value.trim()), Some(&unit)) {
            out.push(Parsed::Reading(Reading {
                weight: Some(weight),
                stable,
                status: status_of(weight, stable),
            }));
        }
        out
    }

    fn zero(&self) -> Option<Vec<u8>> {
        None
    }

    fn reset(&mut self) {
        self.state = CasState::Idle;
        self.buf.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn readings(parsed: Vec<Parsed>) -> Vec<Reading> {
        parsed
            .into_iter()
            .filter_map(|p| match p {
                Parsed::Reading(r) => Some(r),
                Parsed::Send(_) => None,
            })
            .collect()
    }

    fn weight(g: i64, stable: Option<bool>, status: ReadingStatus) -> Reading {
        Reading {
            weight: Some(Qty(g)),
            stable,
            status,
        }
    }

    #[test]
    fn weights_read_kilos_and_grams() {
        for (number, unit, grams) in [
            ("1.234", None, 1_234),
            ("+0001.234", Some("kg"), 1_234),
            ("-0.012", Some("KG"), -12),
            ("1,25", None, 1_250),
            ("001234", Some("g"), 1_234),
            ("0.0005", None, 1),
            ("2", None, 2_000),
        ] {
            assert_eq!(
                parse_weight(number, unit),
                Some(Qty(grams)),
                "{number} {unit:?}"
            );
        }
        for bad in ["", "1.2.3", "abc", "12345678"] {
            assert_eq!(parse_weight(bad, None), None, "{bad}");
        }
        assert_eq!(parse_weight("1.0", Some("lb")), None, "pounds are not read");
    }

    #[test]
    fn continuous_lines_carry_stability_sign_and_overload() {
        let mut p = protocol(ScaleProtocol::Continuous);
        assert_eq!(p.poll(), None, "a streaming scale is never asked");
        let got = readings(p.feed(
            b"ST,GS,+0001.234kg\r\nUS,NT,-0000.012kg\r\nOL,GS,+9999.999kg\r\n   1.234 kg\r\n001234 g\r\n",
        ));
        assert_eq!(
            got,
            [
                weight(1_234, Some(true), ReadingStatus::Ok),
                weight(-12, Some(false), ReadingStatus::Underload),
                Reading {
                    weight: None,
                    stable: None,
                    status: ReadingStatus::Overload
                },
                weight(1_234, None, ReadingStatus::Ok),
                weight(1_234, None, ReadingStatus::Ok),
            ]
        );
        assert!(
            readings(p.feed(b"ST,GS,+0001")).is_empty(),
            "half a line waits"
        );
        assert_eq!(
            readings(p.feed(b".500kg\r\n")),
            [weight(1_500, Some(true), ReadingStatus::Ok)]
        );
        assert!(readings(p.feed(b"\xff\xfe garbage\r\n")).is_empty());
    }

    #[test]
    fn toledo_status_frames_are_never_weights() {
        let mut p = protocol(ScaleProtocol::Toledo8217);
        assert_eq!(p.poll().as_deref(), Some(&b"W"[..]));
        assert_eq!(p.zero().as_deref(), Some(&b"Z"[..]));
        assert_eq!(
            readings(p.feed(b"\x0201.250\r")),
            [weight(1_250, Some(true), ReadingStatus::Ok)]
        );
        assert_eq!(
            readings(p.feed(b"\x02?\x01\r\x02?\x02\r\x02?\x04\r")),
            [
                Reading {
                    weight: None,
                    stable: Some(false),
                    status: ReadingStatus::Motion
                },
                Reading {
                    weight: None,
                    stable: Some(false),
                    status: ReadingStatus::Overload
                },
                Reading {
                    weight: None,
                    stable: Some(false),
                    status: ReadingStatus::Underload
                },
            ]
        );
        assert!(
            readings(p.feed(b"noise\x0200.5")).is_empty(),
            "noise before STX is dropped; the frame waits for CR"
        );
        assert_eq!(
            readings(p.feed(b"00\r")),
            [weight(500, Some(true), ReadingStatus::Ok)]
        );
    }

    #[test]
    fn cas_frames_need_the_handshake_and_a_whole_frame() {
        let mut p = protocol(ScaleProtocol::CasEcr);
        assert_eq!(p.zero(), None);
        assert!(
            p.feed(b"\x01\x02S 01.234kg\x00\x03\x04").is_empty(),
            "no frame before the handshake"
        );
        assert_eq!(p.poll().as_deref(), Some(&[0x05][..]), "ENQ");
        assert_eq!(p.feed(&[0x06]), [Parsed::Send(vec![0x11])], "ACK → DC1");
        assert!(p.feed(b"\x01\x02S 01.2").is_empty(), "half a frame");
        assert_eq!(
            readings(p.feed(b"34kg\x00\x03\x04")),
            [weight(1_234, Some(true), ReadingStatus::Ok)]
        );
        p.poll();
        p.feed(&[0x06]);
        assert_eq!(
            readings(p.feed(b"\x01\x02U-00.010kg\x00\x03\x04")),
            [weight(-10, Some(false), ReadingStatus::Underload)]
        );
        p.poll();
        p.feed(&[0x06]);
        assert!(
            readings(p.feed(b"\x01\x02S 01.234kg\x00\x03\x05")).is_empty(),
            "no EOT: not a frame"
        );
    }

    #[test]
    fn each_protocol_has_its_usual_serial_settings() {
        let toledo = ScaleProtocol::Toledo8217.serial("COM3");
        assert_eq!(
            (toledo.baud, toledo.data_bits, toledo.parity),
            (9_600, DataBits::Seven, Parity::Even)
        );
        let cas = ScaleProtocol::CasEcr.serial("COM3");
        assert_eq!(
            (cas.baud, cas.data_bits, cas.parity),
            (9_600, DataBits::Eight, Parity::None)
        );
        assert!(
            ScaleProtocol::Toledo8217.supports_zero() && !ScaleProtocol::Continuous.supports_zero()
        );
    }
}

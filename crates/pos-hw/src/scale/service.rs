//! The scale's reader thread (HW-030, VRT-010): polls or listens, turns readings into
//! `WeightEvent`s (gross, tare, net, stable), publishes each change once, reports and survives
//! unplugged cables, and answers the till's "a stable weight, please" within a bound.

use super::protocol::{Parsed, Protocol, Reading, ReadingStatus, ScaleProtocol, protocol};
use crate::device::{Backoff, DeviceKind, DeviceState, StatusSink, sleep_unless};
use crate::error::HwError;
use crate::serial::{DataBits, Link, LinkFactory, Parity, SerialSettings, StopBits};
use pos_core::money::Qty;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ScaleConfig {
    pub serial: SerialSettings,
    pub protocol: ScaleProtocol,
    pub poll_ms: u16,
    /// Equal readings that make a weight stable when the scale does not say.
    pub stable_readings: u8,
    pub capacity_g: u32,
}

impl ScaleConfig {
    pub fn new(protocol: ScaleProtocol, port: &str) -> ScaleConfig {
        ScaleConfig {
            serial: protocol.serial(port),
            protocol,
            poll_ms: 250,
            stable_readings: 3,
            capacity_g: 30_000,
        }
    }

    pub fn validate(&self) -> Result<(), HwError> {
        self.serial.validate()?;
        if !(50..=2_000).contains(&self.poll_ms) {
            return Err(HwError::setting("pollMs"));
        }
        if !(1..=10).contains(&self.stable_readings) {
            return Err(HwError::setting("stableReadings"));
        }
        if !(1_000..=300_000).contains(&self.capacity_g) {
            return Err(HwError::setting("capacityG"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ScaleState {
    Ok,
    Motion,
    Overload,
    Underload,
    Disconnected,
    NoResponse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WeightEvent {
    pub gross: Qty,
    pub net: Qty,
    pub tare: Qty,
    pub stable: bool,
    pub state: ScaleState,
}

impl WeightEvent {
    fn silent(state: ScaleState, tare: Qty) -> WeightEvent {
        WeightEvent {
            gross: Qty::ZERO,
            net: Qty::ZERO,
            tare,
            stable: false,
            state,
        }
    }
}

/// Receives each new `WeightEvent`, in publishing order, on the thread that publishes it (the
/// reader, or the one taring) while the service's weights are locked: it must return quickly and
/// never call back into the `ScaleService`.
pub type WeightSink = Arc<dyn Fn(&WeightEvent) + Send + Sync>;

/// Stable when the scale says so, or after `needed` equal readings when it does not.
#[derive(Debug, Clone)]
pub struct Stability {
    needed: usize,
    recent: VecDeque<Qty>,
}

impl Stability {
    pub fn new(needed: u8) -> Stability {
        Stability {
            needed: usize::from(needed.max(1)),
            recent: VecDeque::new(),
        }
    }

    pub fn push(&mut self, reading: &Reading) -> bool {
        let Some(weight) = reading.weight else {
            self.recent.clear();
            return false;
        };
        if self.recent.back().is_some_and(|last| *last != weight) {
            self.recent.clear();
        }
        self.recent.push_back(weight);
        while self.recent.len() > self.needed {
            self.recent.pop_front();
        }
        match reading.stable {
            Some(said) => said,
            None => self.recent.len() >= self.needed,
        }
    }
}

enum Command {
    Zero(Sender<Result<(), HwError>>),
}

/// The last published event and the tare, under one lock: an event is always built with the
/// tare in force when it is stored.
#[derive(Default)]
struct Weights {
    latest: Option<WeightEvent>,
    tare: Qty,
}

struct Shared {
    stop: AtomicBool,
    weights: Mutex<Weights>,
    changed: Condvar,
    sink: WeightSink,
}

impl Shared {
    fn weights(&self) -> Result<MutexGuard<'_, Weights>, HwError> {
        self.weights.lock().map_err(|_| HwError::ScaleNotResponding)
    }

    /// Stores `event` and hands it to the sink when it differs from the last one (called with the
    /// lock held, so the sink sees the events in the order they are stored).
    fn store(&self, weights: &mut Weights, event: WeightEvent) {
        if weights.latest.as_ref() == Some(&event) {
            return;
        }
        (self.sink)(&event);
        weights.latest = Some(event);
        self.changed.notify_all();
    }

    /// Publishes the event `build` makes with the current tare.
    fn publish(&self, build: impl FnOnce(Qty) -> WeightEvent) {
        if let Ok(mut weights) = self.weights() {
            let event = build(weights.tare);
            self.store(&mut weights, event);
        }
    }

    /// Sets the tare and publishes the last event again with it.
    fn retare(&self, weights: &mut Weights, tare: Qty) -> Option<WeightEvent> {
        weights.tare = tare;
        let current = weights.latest.clone()?;
        let event = WeightEvent {
            net: Qty(current.gross.0 - tare.0),
            tare,
            ..current
        };
        self.store(weights, event.clone());
        Some(event)
    }
}

fn event_of(reading: &Reading, stable: bool, tare: Qty, capacity_g: u32) -> WeightEvent {
    let gross = reading.weight.unwrap_or(Qty::ZERO);
    let over = gross.0 > i64::from(capacity_g);
    let state = match reading.status {
        _ if over => ScaleState::Overload,
        ReadingStatus::Ok => ScaleState::Ok,
        ReadingStatus::Motion => ScaleState::Motion,
        ReadingStatus::Overload => ScaleState::Overload,
        ReadingStatus::Underload => ScaleState::Underload,
    };
    let stable = stable && state == ScaleState::Ok && reading.weight.is_some();
    WeightEvent {
        gross,
        net: Qty(gross.0 - tare.0),
        tare,
        stable,
        state,
    }
}

pub struct ScaleService {
    shared: Arc<Shared>,
    commands: Sender<Command>,
    thread: Option<JoinHandle<()>>,
    supports_zero: bool,
}

impl ScaleService {
    pub fn start(
        config: ScaleConfig,
        links: Arc<dyn LinkFactory>,
        sink: WeightSink,
        status: StatusSink,
    ) -> Result<ScaleService, HwError> {
        config.validate()?;
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            weights: Mutex::new(Weights::default()),
            changed: Condvar::new(),
            sink,
        });
        let (commands, receiver) = mpsc::channel();
        let supports_zero = config.protocol.supports_zero();
        let worker = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("dzpos-scale".into())
            .spawn(move || run(config, links, status, worker, receiver))
            .map_err(|e| HwError::Io(e.to_string()))?;
        Ok(ScaleService {
            shared,
            commands,
            thread: Some(thread),
            supports_zero,
        })
    }

    pub fn latest(&self) -> Option<WeightEvent> {
        self.shared.weights().ok()?.latest.clone()
    }

    /// Software tare: the current stable gross becomes the tare.
    pub fn tare(&self) -> Result<WeightEvent, HwError> {
        let mut weights = self.shared.weights()?;
        let latest = weights.latest.clone().ok_or(HwError::ScaleNotResponding)?;
        match latest.state {
            ScaleState::Disconnected | ScaleState::NoResponse => {
                return Err(HwError::ScaleNotResponding);
            }
            ScaleState::Overload => return Err(HwError::ScaleOverload),
            _ => {}
        }
        if !latest.stable || latest.gross.0 <= 0 {
            return Err(HwError::ScaleUnstable);
        }
        self.shared
            .retare(&mut weights, latest.gross)
            .ok_or(HwError::ScaleNotResponding)
    }

    pub fn clear_tare(&self) -> Option<WeightEvent> {
        let mut weights = self.shared.weights().ok()?;
        self.shared.retare(&mut weights, Qty::ZERO)
    }

    pub fn zero(&self) -> Result<(), HwError> {
        if !self.supports_zero {
            return Err(HwError::ScaleCommandUnsupported);
        }
        let (reply, answer) = mpsc::channel();
        self.commands
            .send(Command::Zero(reply))
            .map_err(|_| HwError::ScaleNotResponding)?;
        answer
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| HwError::ScaleNotResponding)?
    }

    /// The next stable event with a positive net, or the reason there is none within `timeout`.
    pub fn read_stable(&self, timeout: Duration) -> Result<WeightEvent, HwError> {
        let deadline = Instant::now() + timeout;
        let mut weights = self.shared.weights()?;
        loop {
            match weights.latest.as_ref() {
                Some(e) if e.stable && e.net.0 > 0 => return Ok(e.clone()),
                Some(e) if e.state == ScaleState::Overload => return Err(HwError::ScaleOverload),
                _ => {}
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(match weights.latest.as_ref().map(|e| e.state) {
                    None | Some(ScaleState::Disconnected | ScaleState::NoResponse) => {
                        HwError::ScaleNotResponding
                    }
                    Some(_) => HwError::ScaleUnstable,
                });
            }
            weights = self
                .shared
                .changed
                .wait_timeout(weights, left)
                .map_err(|_| HwError::ScaleNotResponding)?
                .0;
        }
    }

    pub fn stop(mut self) {
        self.halt();
    }

    fn halt(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for ScaleService {
    fn drop(&mut self) {
        self.halt();
    }
}

fn run(
    config: ScaleConfig,
    links: Arc<dyn LinkFactory>,
    status: StatusSink,
    shared: Arc<Shared>,
    commands: Receiver<Command>,
) {
    let port = config.serial.port.clone();
    let mut protocol: Box<dyn Protocol> = protocol(config.protocol);
    let mut link: Option<Box<dyn Link>> = None;
    let mut backoff = Backoff::new();
    let mut stability = Stability::new(config.stable_readings);
    let poll = Duration::from_millis(u64::from(config.poll_ms));
    let silence = poll.saturating_mul(4).max(Duration::from_secs(2));
    let mut last_reading = Instant::now();
    let mut reported: Option<DeviceState> = None;
    let mut report = |state: DeviceState| {
        if reported.as_ref() != Some(&state) {
            status(DeviceKind::Scale, &state);
            reported = Some(state);
        }
    };
    let mut buf = [0_u8; 256];
    while !shared.stop.load(Ordering::SeqCst) {
        while let Ok(Command::Zero(reply)) = commands.try_recv() {
            let answer = match (link.as_mut(), protocol.zero()) {
                (Some(l), Some(bytes)) => l.write_all(&bytes),
                (None, _) => Err(HwError::ScaleNotResponding),
                (_, None) => Err(HwError::ScaleCommandUnsupported),
            };
            let _ = reply.send(answer);
        }
        if link.is_none() {
            match links.open(&config.serial) {
                Ok(opened) => {
                    link = Some(opened);
                    backoff.reset();
                    protocol.reset();
                    last_reading = Instant::now();
                    report(DeviceState::Connected { port: port.clone() });
                }
                Err(error) => {
                    shared.publish(|tare| WeightEvent::silent(ScaleState::Disconnected, tare));
                    report(DeviceState::Disconnected {
                        port: port.clone(),
                        code: error.code().to_owned(),
                    });
                    sleep_unless(&shared.stop, backoff.fail());
                }
            }
            continue;
        }
        let Some(l) = link.as_mut() else {
            continue;
        };
        let mut lost = false;
        if let Some(request) = protocol.poll() {
            lost = l.write_all(&request).is_err();
        }
        let deadline = Instant::now() + poll;
        while !lost && !shared.stop.load(Ordering::SeqCst) {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            match l.read(&mut buf, left.min(Duration::from_millis(50))) {
                Ok(0) => {}
                Ok(n) => {
                    for parsed in protocol.feed(&buf[..n]) {
                        match parsed {
                            Parsed::Send(bytes) => lost |= l.write_all(&bytes).is_err(),
                            Parsed::Reading(reading) => {
                                if last_reading.elapsed() > silence {
                                    // The scale answers again after a silence.
                                    report(DeviceState::Connected { port: port.clone() });
                                }
                                last_reading = Instant::now();
                                let stable = stability.push(&reading);
                                shared.publish(|tare| {
                                    event_of(&reading, stable, tare, config.capacity_g)
                                });
                            }
                        }
                    }
                }
                Err(_) => lost = true,
            }
        }
        if lost {
            link = None;
            shared.publish(|tare| WeightEvent::silent(ScaleState::Disconnected, tare));
            report(DeviceState::Disconnected {
                port: port.clone(),
                code: "scale_not_responding".into(),
            });
            continue;
        }
        if last_reading.elapsed() > silence {
            // A scale switched off or unplugged behind a port that still opens (a motherboard
            // COM port, a USB-serial adapter left in the PC) is a disconnected device.
            protocol.reset();
            shared.publish(|tare| WeightEvent::silent(ScaleState::NoResponse, tare));
            report(DeviceState::Disconnected {
                port: port.clone(),
                code: "scale_not_responding".into(),
            });
        }
    }
}

/// How often `probe` repeats a candidate's request: room for a whole answer, handshake included
/// (CAS: ENQ → ACK → DC1 → frame), between two requests.
const PROBE_REQUEST_EVERY: Duration = Duration::from_millis(200);

/// Tries the usual protocol and speed pairs on `port` for `per_try` each and returns the first
/// one a reading comes back from: the settings screen's "Detect" (ONB-009).
pub fn probe(
    links: &dyn LinkFactory,
    port: &str,
    per_try: Duration,
) -> Option<(ScaleProtocol, SerialSettings)> {
    let settings = |baud, data_bits, parity| SerialSettings {
        port: port.to_owned(),
        baud,
        data_bits,
        parity,
        stop_bits: StopBits::One,
    };
    let candidates = [
        (
            ScaleProtocol::Toledo8217,
            settings(9_600, DataBits::Seven, Parity::Even),
        ),
        (
            ScaleProtocol::CasEcr,
            settings(9_600, DataBits::Eight, Parity::None),
        ),
        (
            ScaleProtocol::Continuous,
            settings(9_600, DataBits::Eight, Parity::None),
        ),
        (
            ScaleProtocol::Continuous,
            settings(4_800, DataBits::Eight, Parity::None),
        ),
        (
            ScaleProtocol::Continuous,
            settings(2_400, DataBits::Eight, Parity::None),
        ),
        (
            ScaleProtocol::Toledo8217,
            settings(4_800, DataBits::Seven, Parity::Even),
        ),
    ];
    let mut buf = [0_u8; 256];
    for (kind, serial) in candidates {
        let Ok(mut link) = links.open(&serial) else {
            return None;
        };
        let _ = link.clear_input();
        let mut parser = protocol(kind);
        let deadline = Instant::now() + per_try;
        let mut next_request = Instant::now();
        while Instant::now() < deadline {
            // One request per window, not one per read: a new CAS ENQ would restart the
            // handshake and throw away the frame that answers the previous one.
            let now = Instant::now();
            if now >= next_request {
                next_request = now + PROBE_REQUEST_EVERY;
                if let Some(request) = parser.poll()
                    && link.write_all(&request).is_err()
                {
                    break;
                }
            }
            let Ok(n) = link.read(&mut buf, Duration::from_millis(40)) else {
                break;
            };
            for parsed in parser.feed(&buf[..n]) {
                match parsed {
                    Parsed::Send(bytes) => {
                        let _ = link.write_all(&bytes);
                    }
                    Parsed::Reading(reading)
                        if reading.weight.is_some() || reading.status != ReadingStatus::Ok =>
                    {
                        return Some((kind, serial));
                    }
                    Parsed::Reading(_) => {}
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{DeviceKind, DeviceState};
    use crate::serial::{DataBits, MemoryLinks, Parity};
    use std::sync::mpsc;
    use std::time::Duration;

    const WAIT: Duration = Duration::from_secs(2);

    fn fast(protocol: ScaleProtocol, port: &str) -> ScaleConfig {
        ScaleConfig {
            poll_ms: 50,
            ..ScaleConfig::new(protocol, port)
        }
    }

    fn collect() -> (WeightSink, mpsc::Receiver<WeightEvent>) {
        let (tx, rx) = mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        let sink: WeightSink = Arc::new(move |e: &WeightEvent| {
            if let Ok(tx) = tx.lock() {
                let _ = tx.send(e.clone());
            }
        });
        (sink, rx)
    }

    fn next_where(
        rx: &mpsc::Receiver<WeightEvent>,
        pred: impl Fn(&WeightEvent) -> bool,
    ) -> WeightEvent {
        loop {
            let e = rx.recv_timeout(WAIT).expect("an event in time");
            if pred(&e) {
                return e;
            }
        }
    }

    #[test]
    fn a_stable_weight_is_published_once_and_net_follows_the_tare() {
        let links = MemoryLinks::new();
        links.answer_only("COM3", 9_600, DataBits::Seven, Parity::Even);
        links.reply("COM3", b"W", b"\x0201.250\r");
        let (sink, rx) = collect();
        let scale = ScaleService::start(
            fast(ScaleProtocol::Toledo8217, "COM3"),
            links.clone(),
            sink,
            crate::device::silent_status(),
        )
        .unwrap();
        let first = next_where(&rx, |e| e.state == ScaleState::Ok);
        assert_eq!(
            (first.gross, first.net, first.stable),
            (Qty(1_250), Qty(1_250), true)
        );
        std::thread::sleep(Duration::from_millis(300));
        assert!(
            rx.try_iter().all(|e| e == first),
            "the same reading is not published again"
        );
        let tared = scale.tare().unwrap();
        assert_eq!((tared.net, tared.tare), (Qty(0), Qty(1_250)));
        links.reply("COM3", b"W", b"\x0201.750\r");
        assert_eq!(next_where(&rx, |e| e.gross == Qty(1_750)).net, Qty(500));
        assert_eq!(scale.clear_tare().map(|e| e.net), Some(Qty(1_750)));
        scale.stop();
    }

    #[test]
    fn an_unstable_reading_is_never_returned_as_stable() {
        let links = MemoryLinks::new();
        links.stream("COM4", b"US,GS,+0001.234kg\r\n");
        let (sink, rx) = collect();
        let scale = ScaleService::start(
            fast(ScaleProtocol::Continuous, "COM4"),
            links,
            sink,
            crate::device::silent_status(),
        )
        .unwrap();
        let e = next_where(&rx, |e| e.gross == Qty(1_234));
        assert!(!e.stable && e.state == ScaleState::Motion);
        assert_eq!(
            scale
                .read_stable(Duration::from_millis(300))
                .unwrap_err()
                .code(),
            "scale_unstable"
        );
        assert_eq!(scale.tare().unwrap_err().code(), "scale_unstable");
    }

    #[test]
    fn a_scale_without_stability_flags_settles_after_equal_readings() {
        let links = MemoryLinks::new();
        links.stream("COM4", b"   0.850 kg\r\n");
        let (sink, rx) = collect();
        let scale = ScaleService::start(
            fast(ScaleProtocol::Continuous, "COM4"),
            links,
            sink,
            crate::device::silent_status(),
        )
        .unwrap();
        let settled = next_where(&rx, |e| e.stable);
        assert_eq!(settled.net, Qty(850));
        assert_eq!(
            scale.read_stable(Duration::from_secs(1)).unwrap().net,
            Qty(850)
        );
    }

    #[test]
    fn unplugging_the_scale_reports_disconnected_then_reconnects() {
        let links = MemoryLinks::new();
        links.stream("COM4", b"ST,GS,+0000.500kg\r\n");
        let (states_tx, states) = mpsc::channel();
        let states_tx = std::sync::Mutex::new(states_tx);
        let status: StatusSink = Arc::new(move |kind, state| {
            if let Ok(tx) = states_tx.lock() {
                let _ = tx.send((kind, state.clone()));
            }
        });
        let (sink, rx) = collect();
        let _scale = ScaleService::start(
            fast(ScaleProtocol::Continuous, "COM4"),
            links.clone(),
            sink,
            status,
        )
        .unwrap();
        next_where(&rx, |e| e.stable);
        links.unplug("COM4", true);
        next_where(&rx, |e| e.state == ScaleState::Disconnected);
        let (kind, state) = states
            .recv_timeout(WAIT)
            .map(|_| ())
            .and_then(|()| states.recv_timeout(WAIT))
            .unwrap();
        assert_eq!(kind, DeviceKind::Scale);
        assert!(
            matches!(state, DeviceState::Disconnected { .. }),
            "{state:?}"
        );
        links.unplug("COM4", false);
        next_where(&rx, |e| e.state == ScaleState::Ok);
    }

    #[test]
    fn read_stable_times_out_instead_of_blocking() {
        let links = MemoryLinks::new();
        links.wire("COM5"); // plugged, silent
        let (sink, _rx) = collect();
        let scale = ScaleService::start(
            fast(ScaleProtocol::Toledo8217, "COM5"),
            links,
            sink,
            crate::device::silent_status(),
        )
        .unwrap();
        let started = std::time::Instant::now();
        assert!(scale.read_stable(Duration::from_millis(400)).is_err());
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn a_silent_scale_on_an_open_port_is_reported_disconnected_until_it_answers() {
        let links = MemoryLinks::new();
        links.wire("COM5"); // the port opens, the scale is switched off
        let (states_tx, states) = mpsc::channel();
        let states_tx = std::sync::Mutex::new(states_tx);
        let status: StatusSink = Arc::new(move |kind, state| {
            if let Ok(tx) = states_tx.lock() {
                let _ = tx.send((kind, state.clone()));
            }
        });
        let (sink, rx) = collect();
        let _scale = ScaleService::start(
            fast(ScaleProtocol::Toledo8217, "COM5"),
            links.clone(),
            sink,
            status,
        )
        .unwrap();
        let port = || "COM5".to_owned();
        assert_eq!(
            states.recv_timeout(WAIT).unwrap(),
            (DeviceKind::Scale, DeviceState::Connected { port: port() })
        );
        // No reading for max(2 s, 4 polls): the weight says no_response, the device says
        // disconnected, both within about 2.5 s.
        let bound = Duration::from_secs(3);
        let silent = rx.recv_timeout(bound).unwrap();
        assert_eq!(silent.state, ScaleState::NoResponse);
        assert_eq!(
            states.recv_timeout(bound).unwrap(),
            (
                DeviceKind::Scale,
                DeviceState::Disconnected {
                    port: port(),
                    code: "scale_not_responding".into(),
                }
            )
        );
        // The scale is switched on again: connected, once.
        links.reply("COM5", b"W", b"\x0201.000\r");
        next_where(&rx, |e| e.state == ScaleState::Ok);
        assert_eq!(
            states.recv_timeout(WAIT).unwrap(),
            (DeviceKind::Scale, DeviceState::Connected { port: port() })
        );
        std::thread::sleep(Duration::from_millis(200));
        assert!(states.try_recv().is_err(), "each state is reported once");
    }

    #[test]
    fn a_missing_port_is_disconnected_without_panicking() {
        let links = MemoryLinks::new();
        let (sink, rx) = collect();
        let scale = ScaleService::start(
            fast(ScaleProtocol::Continuous, "COM42"),
            links,
            sink,
            crate::device::silent_status(),
        )
        .unwrap();
        assert_eq!(next_where(&rx, |_| true).state, ScaleState::Disconnected);
        assert_eq!(
            scale
                .read_stable(Duration::from_millis(100))
                .unwrap_err()
                .code(),
            "scale_not_responding"
        );
    }

    #[test]
    fn zero_goes_to_toledo_and_is_refused_by_continuous_scales() {
        let links = MemoryLinks::new();
        links.reply("COM3", b"W", b"\x0200.000\r");
        let (sink, rx) = collect();
        let toledo = ScaleService::start(
            fast(ScaleProtocol::Toledo8217, "COM3"),
            links.clone(),
            sink,
            crate::device::silent_status(),
        )
        .unwrap();
        next_where(&rx, |_| true);
        toledo.zero().unwrap();
        assert!(
            links.written("COM3").contains(&b'Z'),
            "the zero command reached the scale"
        );
        links.stream("COM4", b"ST,GS,+0000.000kg\r\n");
        let (sink, _rx) = collect();
        let plain = ScaleService::start(
            fast(ScaleProtocol::Continuous, "COM4"),
            links,
            sink,
            crate::device::silent_status(),
        )
        .unwrap();
        assert_eq!(
            plain.zero().unwrap_err().code(),
            "scale_command_unsupported"
        );
    }

    #[test]
    fn stability_needs_equal_readings_unless_the_scale_says() {
        let mut s = Stability::new(3);
        let r = |g, stable| Reading {
            weight: Some(Qty(g)),
            stable,
            status: ReadingStatus::Ok,
        };
        assert!(!s.push(&r(500, None)) && !s.push(&r(500, None)) && s.push(&r(500, None)));
        assert!(!s.push(&r(510, None)), "a change starts over");
        assert!(s.push(&r(510, Some(true))), "the scale says stable");
        assert!(!s.push(&r(510, Some(false))));
    }

    #[test]
    fn probe_finds_the_protocol_and_the_speed() {
        let links = MemoryLinks::new();
        links.answer_only("COM6", 4_800, DataBits::Eight, Parity::None);
        links.stream("COM6", b"ST,GS,+0000.250kg\r\n");
        let found = probe(links.as_ref(), "COM6", Duration::from_millis(150)).unwrap();
        assert_eq!((found.0, found.1.baud), (ScaleProtocol::Continuous, 4_800));
        links.answer_only("COM7", 9_600, DataBits::Seven, Parity::Even);
        links.reply("COM7", b"W", b"\x0200.100\r");
        assert_eq!(
            probe(links.as_ref(), "COM7", Duration::from_millis(150))
                .unwrap()
                .0,
            ScaleProtocol::Toledo8217
        );
        links.answer_only("COM8", 9_600, DataBits::Eight, Parity::None);
        links.reply("COM8", &[0x05], &[0x06]); // ENQ → ACK
        links.reply("COM8", &[0x11], b"\x01\x02S 01.234kg\x00\x03\x04"); // DC1 → frame
        let cas = probe(links.as_ref(), "COM8", Duration::from_millis(150)).unwrap();
        assert_eq!(
            (cas.0, cas.1.baud, cas.1.data_bits, cas.1.parity),
            (ScaleProtocol::CasEcr, 9_600, DataBits::Eight, Parity::None)
        );
        let enquiries = links.written("COM8").iter().filter(|b| **b == 0x05).count();
        assert!(
            enquiries <= 2,
            "one ENQ per request window, not one per read: {enquiries}"
        );
        assert_eq!(
            probe(links.as_ref(), "COM99", Duration::from_millis(50)),
            None
        );
    }

    #[test]
    fn configs_are_validated() {
        let field = |c: ScaleConfig| c.validate().unwrap_err().field().map(str::to_owned);
        let base = ScaleConfig::new(ScaleProtocol::Continuous, "COM3");
        assert_eq!(
            field(ScaleConfig {
                poll_ms: 10,
                ..base.clone()
            })
            .as_deref(),
            Some("pollMs")
        );
        assert_eq!(
            field(ScaleConfig {
                stable_readings: 0,
                ..base.clone()
            })
            .as_deref(),
            Some("stableReadings")
        );
        assert_eq!(
            field(ScaleConfig {
                capacity_g: 10,
                ..base
            })
            .as_deref(),
            Some("capacityG")
        );
    }
}

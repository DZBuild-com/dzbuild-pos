//! Writes the frames to the pole display on its own thread: `show` returns at once, the newest
//! frame wins when several wait. The thread opens the port when it starts, so the display's state
//! is known before the first frame, but writes nothing at that open: `ESC @` blanks DM-D and
//! CD5220 displays, so it goes just before the first frame or test pattern written on each opened
//! link, and a restart never wipes what the port's previous owner left (the settings' test
//! pattern). A missing or unplugged display is reported and retried with the backoff (0.5 s
//! doubling to 5 s), and gets the newest frame once it answers again. Stopping the service
//! (session end) clears a sale off the display; the settings' test pattern stays. The current
//! frame is kept even with no VFD: the customer screen asks for it when it opens.

use super::frame::DisplayFrame;
use super::vfd::{CLEAR, INIT, VfdConfig, bytes, lines, test_lines};
use crate::device::{Backoff, DeviceKind, DeviceState, StatusSink};
use crate::error::HwError;
use crate::serial::{Link, LinkFactory};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

enum Message {
    /// Boxed: a frame (with its BaridiMob QR since M8) is far larger than the other messages.
    Show(Box<DisplayFrame>),
    Test(Sender<Result<(), HwError>>),
    Stop,
}

pub struct DisplayService {
    current: Arc<Mutex<Option<DisplayFrame>>>,
    shop: Arc<Mutex<String>>,
    sender: Option<Sender<Message>>,
    thread: Option<JoinHandle<()>>,
}

impl DisplayService {
    /// No pole display: only the current frame is kept (for the customer screen).
    pub fn none() -> DisplayService {
        DisplayService {
            current: Arc::new(Mutex::new(None)),
            shop: Arc::new(Mutex::new(String::new())),
            sender: None,
            thread: None,
        }
    }

    /// The thread opens the port at once: stop the service that held this port before (one
    /// owner per serial port; Windows opens a COM port for one owner only, and the display is
    /// reported disconnected until the backoff's next attempt finds the port free).
    pub fn start(
        vfd: Option<VfdConfig>,
        links: Arc<dyn LinkFactory>,
        status: StatusSink,
    ) -> Result<DisplayService, HwError> {
        let Some(cfg) = vfd else {
            return Ok(DisplayService::none());
        };
        let current = Arc::new(Mutex::new(None));
        let shop = Arc::new(Mutex::new(String::new()));
        cfg.validate()?;
        let (sender, receiver) = mpsc::channel();
        let worker_shop = Arc::clone(&shop);
        let thread = std::thread::Builder::new()
            .name("dzpos-vfd".into())
            .spawn(move || run(cfg, links, status, worker_shop, receiver))
            .map_err(|e| HwError::Io(e.to_string()))?;
        Ok(DisplayService {
            current,
            shop,
            sender: Some(sender),
            thread: Some(thread),
        })
    }

    pub fn has_vfd(&self) -> bool {
        self.sender.is_some()
    }

    pub fn show(&self, frame: DisplayFrame) {
        if let Ok(mut current) = self.current.lock() {
            *current = Some(frame.clone());
        }
        if let Some(sender) = &self.sender {
            let _ = sender.send(Message::Show(Box::new(frame)));
        }
    }

    pub fn current(&self) -> Option<DisplayFrame> {
        self.current.lock().ok()?.clone()
    }

    pub fn set_shop(&self, latin_name: String) {
        if let Ok(mut shop) = self.shop.lock() {
            *shop = latin_name;
        }
    }

    pub fn test(&self) -> Result<(), HwError> {
        let sender = self.sender.as_ref().ok_or(HwError::DisplayNotConfigured)?;
        let (reply, answer) = mpsc::channel();
        sender
            .send(Message::Test(reply))
            .map_err(|_| HwError::DisplayNotConfigured)?;
        answer
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| HwError::Timeout)?
    }

    pub fn stop(mut self) {
        self.halt();
    }

    fn halt(&mut self) {
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(Message::Stop);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for DisplayService {
    fn drop(&mut self) {
        self.halt();
    }
}

fn run(
    cfg: VfdConfig,
    links: Arc<dyn LinkFactory>,
    status: StatusSink,
    shop: Arc<Mutex<String>>,
    receiver: Receiver<Message>,
) {
    let port = cfg.serial.port.clone();
    let mut link: Option<Box<dyn Link>> = None;
    // The opened link has not had its `ESC @` yet.
    let mut needs_init = false;
    let mut backoff = Backoff::new();
    let mut next_try = Instant::now();
    let mut reported: Option<DeviceState> = None;
    let mut report = |state: DeviceState| {
        if reported.as_ref() != Some(&state) {
            status(DeviceKind::Vfd, &state);
            reported = Some(state);
        }
    };
    // The newest frame the display has not shown yet.
    let mut pending: Option<DisplayFrame> = None;
    // A till frame is on the display (not a blank screen or the test pattern).
    let mut showing_frame = false;
    loop {
        // Connected, the thread waits for the till; away, only until its next attempt.
        let first = if link.is_some() {
            match receiver.recv() {
                Ok(message) => Some(message),
                Err(_) => break,
            }
        } else {
            match receiver.recv_timeout(next_try.saturating_duration_since(Instant::now())) {
                Ok(message) => Some(message),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        };
        let (mut tests, mut stop) = (Vec::new(), false);
        for message in first.into_iter().chain(receiver.try_iter()) {
            match message {
                Message::Show(frame) => pending = Some(*frame),
                Message::Test(reply) => tests.push(reply),
                Message::Stop => stop = true,
            }
        }
        if stop {
            if showing_frame && let Some(l) = link.as_mut() {
                let _ = l.write_all(&[CLEAR]);
            }
            break;
        }
        if link.is_none() && (Instant::now() >= next_try || !tests.is_empty()) {
            match links.open(&cfg.serial) {
                Ok(opened) => {
                    link = Some(opened);
                    needs_init = true;
                    backoff.reset();
                    report(DeviceState::Connected { port: port.clone() });
                }
                Err(error) => {
                    next_try = Instant::now() + backoff.fail();
                    report(DeviceState::Disconnected {
                        port: port.clone(),
                        code: error.code().to_owned(),
                    });
                    for reply in tests.drain(..) {
                        let _ = reply.send(Err(error.clone()));
                    }
                    continue;
                }
            }
        }
        let Some(l) = link.as_mut() else {
            continue;
        };
        let shop_name = shop.lock().map(|s| s.clone()).unwrap_or_default();
        let mut failed = None;
        if let Some(frame) = pending.take() {
            let screen = bytes(&lines(&frame, &shop_name, &cfg), &cfg);
            match send(l.as_mut(), &mut needs_init, screen) {
                Ok(()) => showing_frame = true,
                Err(error) => {
                    failed = Some(error);
                    pending = Some(frame);
                }
            }
        }
        for reply in tests {
            let result = send(l.as_mut(), &mut needs_init, bytes(&test_lines(&cfg), &cfg));
            match &result {
                Ok(()) => showing_frame = false,
                Err(error) => failed = Some(error.clone()),
            }
            let _ = reply.send(result);
        }
        if let Some(error) = failed {
            link = None;
            next_try = Instant::now() + backoff.fail();
            report(DeviceState::Disconnected {
                port: port.clone(),
                code: error.code().to_owned(),
            });
        }
    }
}

/// Writes one screen, preceded by `ESC @` when it is the first on a freshly opened link.
fn send(link: &mut dyn Link, needs_init: &mut bool, screen: Vec<u8>) -> Result<(), HwError> {
    let out = if *needs_init {
        [INIT.as_slice(), &screen].concat()
    } else {
        screen
    };
    link.write_all(&out)?;
    *needs_init = false;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::frame::{DisplayFrame, DisplayMode};
    use crate::serial::MemoryLinks;
    use pos_core::lang::Lang;
    use pos_core::money::Money;
    use std::time::{Duration, Instant};

    fn wait_until(what: &str, f: impl Fn() -> bool) {
        let started = Instant::now();
        while !f() {
            assert!(started.elapsed() < Duration::from_secs(2), "{what}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn total(amount: i64) -> DisplayFrame {
        DisplayFrame {
            mode: DisplayMode::Sale,
            total: Money(amount),
            ..DisplayFrame::idle(Lang::Fr)
        }
    }

    #[test]
    fn show_coalesces_and_the_vfd_gets_the_last_frame() {
        let links = MemoryLinks::new();
        links.wire("COM2");
        let display = DisplayService::start(
            Some(VfdConfig::new("COM2")),
            links.clone(),
            crate::device::silent_status(),
        )
        .unwrap();
        for centimes in (1..=30).map(|n| n * 1_000) {
            display.show(total(centimes));
        }
        assert_eq!(display.current(), Some(total(30_000)));
        wait_until("the last total reached the display", || {
            String::from_utf8_lossy(&links.written("COM2")).contains("300,00")
        });
        assert!(links.written("COM2").starts_with(&INIT));
    }

    #[test]
    fn without_a_vfd_the_current_frame_is_still_kept() {
        let display =
            DisplayService::start(None, MemoryLinks::new(), crate::device::silent_status())
                .unwrap();
        assert!(!display.has_vfd());
        display.show(total(5_000));
        assert_eq!(display.current(), Some(total(5_000)));
        assert_eq!(display.test().unwrap_err().code(), "display_not_configured");
    }

    #[test]
    fn a_missing_vfd_never_blocks_show() {
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        let status: crate::device::StatusSink = std::sync::Arc::new(move |kind, state| {
            if let Ok(tx) = tx.lock() {
                let _ = tx.send((kind, state.clone()));
            }
        });
        let display =
            DisplayService::start(Some(VfdConfig::new("COM88")), MemoryLinks::new(), status)
                .unwrap();
        let started = Instant::now();
        display.show(total(1_000));
        assert!(started.elapsed() < Duration::from_millis(50));
        let (kind, state) = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(kind, crate::device::DeviceKind::Vfd);
        assert!(matches!(
            state,
            crate::device::DeviceState::Disconnected { .. }
        ));
    }

    #[test]
    fn the_test_pattern_reaches_the_display() {
        let links = MemoryLinks::new();
        links.wire("COM2");
        let display = DisplayService::start(
            Some(VfdConfig::new("COM2")),
            links.clone(),
            crate::device::silent_status(),
        )
        .unwrap();
        display.test().unwrap();
        assert!(String::from_utf8_lossy(&links.written("COM2")).contains("DZBuild POS"));
    }

    #[test]
    fn an_unplugged_display_gets_the_newest_frame_when_it_comes_back() {
        let links = MemoryLinks::new();
        links.wire("COM2");
        let states = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = std::sync::Arc::clone(&states);
        let status: crate::device::StatusSink = std::sync::Arc::new(move |_, state| {
            if let Ok(mut seen) = seen.lock() {
                seen.push(state.clone());
            }
        });
        let last = || states.lock().unwrap().last().cloned();
        let written = || String::from_utf8_lossy(&links.written("COM2")).into_owned();
        let display =
            DisplayService::start(Some(VfdConfig::new("COM2")), links.clone(), status).unwrap();
        display.show(total(1_000));
        wait_until("the first total reached the display", || {
            written().contains("10,00")
        });
        links.unplug("COM2", true);
        display.show(total(2_000));
        wait_until("the unplugged display is reported", || {
            matches!(last(), Some(DeviceState::Disconnected { .. }))
        });
        display.show(total(3_000));
        links.unplug("COM2", false);
        wait_until(
            "the newest total reached the display once it was back",
            || written().contains("30,00"),
        );
        wait_until("the display is reported back", || {
            matches!(last(), Some(DeviceState::Connected { .. }))
        });
        assert!(
            !written().contains("20,00"),
            "the older frame gave way to the newest"
        );
        let cfg = VfdConfig::new("COM2");
        let newest = bytes(&lines(&total(3_000), "", &cfg), &cfg);
        assert!(
            links
                .written("COM2")
                .ends_with(&[INIT.as_slice(), &newest].concat()),
            "the reopened link gets its ESC @ just before its first frame"
        );
    }

    #[test]
    fn stopping_clears_the_last_sale_but_not_the_test_pattern() {
        let links = MemoryLinks::new();
        links.wire("COM2");
        let display = DisplayService::start(
            Some(VfdConfig::new("COM2")),
            links.clone(),
            crate::device::silent_status(),
        )
        .unwrap();
        display.show(total(5_000));
        wait_until("the total reached the display", || {
            String::from_utf8_lossy(&links.written("COM2")).contains("50,00")
        });
        display.stop();
        assert_eq!(
            links.written("COM2").last(),
            Some(&0x0C),
            "the pole display does not keep a finished sale"
        );

        links.wire("COM3");
        let cfg = VfdConfig::new("COM3");
        let tested = DisplayService::start(
            Some(cfg.clone()),
            links.clone(),
            crate::device::silent_status(),
        )
        .unwrap();
        tested.test().unwrap();
        drop(tested);
        assert!(
            links
                .written("COM3")
                .ends_with(&bytes(&test_lines(&cfg), &cfg)),
            "the settings' test stays readable"
        );
    }

    #[test]
    fn a_restarted_display_keeps_the_test_pattern_until_its_next_frame() {
        let links = MemoryLinks::new();
        links.wire("COM2");
        let cfg = VfdConfig::new("COM2");
        let running = DisplayService::start(
            Some(cfg.clone()),
            links.clone(),
            crate::device::silent_status(),
        )
        .unwrap();
        running.show(total(5_000));
        wait_until("the total reached the display", || {
            String::from_utf8_lossy(&links.written("COM2")).contains("50,00")
        });
        // `Devices::with_port_free`: the running service gives the port up for the settings' test…
        running.stop();
        let tester = DisplayService::start(
            Some(cfg.clone()),
            links.clone(),
            crate::device::silent_status(),
        )
        .unwrap();
        tester.test().unwrap();
        tester.stop();
        // …then `apply` starts it again on the same port.
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let status: StatusSink = Arc::new(move |_, state| {
            if let Ok(tx) = tx.lock() {
                let _ = tx.send(state.clone());
            }
        });
        let restarted = DisplayService::start(Some(cfg.clone()), links.clone(), status).unwrap();
        assert!(
            matches!(
                rx.recv_timeout(Duration::from_secs(2)),
                Ok(DeviceState::Connected { .. })
            ),
            "the restarted service opened its port"
        );
        let pattern = bytes(&test_lines(&cfg), &cfg);
        assert!(
            links.written("COM2").ends_with(&pattern),
            "opening the port leaves the test pattern on the display"
        );
        restarted.show(total(7_000));
        wait_until("the next frame reached the display", || {
            String::from_utf8_lossy(&links.written("COM2")).contains("70,00")
        });
        let frame = bytes(&lines(&total(7_000), "", &cfg), &cfg);
        assert!(
            links
                .written("COM2")
                .ends_with(&[pattern.as_slice(), &INIT, &frame].concat()),
            "ESC @ goes just before the first frame written on the new link"
        );
    }
}

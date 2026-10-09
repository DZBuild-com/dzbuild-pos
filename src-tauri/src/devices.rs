//! The PC's device threads — scale, pole display, serial scanner — owned by the app. Each one
//! starts from `hardware.json`, restarts when its section changes, reports its state (kept here
//! for `devices_status`) and calls the sinks the window events are made from. One owner per
//! serial port: a service is stopped (its port closed) before the one replacing it starts, and a
//! settings test or a probe on a port in use pauses its service (`with_port_free`); no service
//! opens a lent port, and what runs once it is given back is the last saved configuration.

use pos_core::company::Company;
use pos_hw::HwError;
use pos_hw::config::HardwareConfig;
use pos_hw::device::{DeviceKind, DeviceState, StatusSink};
use pos_hw::display::{DisplayFrame, DisplayService, VfdConfig};
use pos_hw::scale::{ScaleConfig, ScaleService, WeightSink};
use pos_hw::scanner::{ScanSink, SerialScanner, SerialScannerConfig};
use pos_hw::serial::{LinkFactory, SerialSettings, port_key};
use serde::Serialize;
use specta::Type;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

/// Where the device threads send what they read: the window events in the app, nowhere in tests.
#[derive(Clone)]
pub struct Sinks {
    pub weight: WeightSink,
    pub scan: ScanSink,
    pub status: StatusSink,
}

impl Sinks {
    pub fn silent() -> Sinks {
        Sinks {
            weight: Arc::new(|_| {}),
            scan: Arc::new(|_| {}),
            status: pos_hw::device::silent_status(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStatus {
    pub device: DeviceKind,
    pub state: DeviceState,
}

/// The sections of `hardware.json` the running services were started from.
#[derive(Debug, Clone, Default, PartialEq)]
struct Running {
    scale: Option<ScaleConfig>,
    vfd: Option<VfdConfig>,
    scanner: Option<SerialScannerConfig>,
}

impl Running {
    fn of(config: &HardwareConfig) -> Running {
        Running {
            scale: config.scale.clone(),
            vfd: config.display.vfd.clone(),
            scanner: config.scanner.serial.clone(),
        }
    }

    /// Without the devices on one of the `lent` ports: they wait until it is given back.
    fn without(mut self, lent: &[String]) -> Running {
        let on = |serial: &SerialSettings| lent.iter().any(|p| *p == port_key(&serial.port));
        if self.scale.as_ref().is_some_and(|s| on(&s.serial)) {
            self.scale = None;
        }
        if self.vfd.as_ref().is_some_and(|v| on(&v.serial)) {
            self.vfd = None;
        }
        if self.scanner.as_ref().is_some_and(|s| on(&s.serial)) {
            self.scanner = None;
        }
        self
    }

    fn has(&self, kind: DeviceKind) -> bool {
        match kind {
            DeviceKind::Scale => self.scale.is_some(),
            DeviceKind::Vfd => self.vfd.is_some(),
            DeviceKind::SerialScanner => self.scanner.is_some(),
        }
    }
}

pub struct Devices {
    sinks: Mutex<Sinks>,
    states: Arc<Mutex<BTreeMap<DeviceKind, DeviceState>>>,
    /// The sections of the last `apply`: what runs once no port is lent.
    wanted: Mutex<Running>,
    /// Ports lent to a settings test or a probe (`with_port_free`): no service opens them.
    lent: Mutex<Vec<String>>,
    running: Mutex<Running>,
    scale: Mutex<Option<ScaleService>>,
    /// Always there (`DisplayService::none()` without a pole display).
    display: Mutex<DisplayService>,
    /// The till's newest frame: the customer screen reads it, a pole display started by a
    /// settings change shows it. One resumed after a settings test or a probe does not: the
    /// test pattern stays until the till's next frame (`Replay`).
    current: Mutex<Option<DisplayFrame>>,
    scanner: Mutex<Option<SerialScanner>>,
    /// The shop's Latin name for the pole display's idle screen.
    shop: Mutex<String>,
}

/// The frame a restarted pole display gets.
#[derive(Debug, Clone, Copy)]
enum Replay {
    /// A settings change: the till's current frame.
    Current,
    /// The end of a settings test or a probe: only a frame the till showed while the port was
    /// lent, so the test pattern stays readable until then.
    ShownMeanwhile,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    match m.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// The name a pole display can show (Latin letters only, HW-043): French, else English.
pub fn latin_name(company: &Company) -> String {
    [&company.name_fr, &company.name_en]
        .into_iter()
        .map(|name| name.trim())
        .find(|name| !name.is_empty())
        .unwrap_or_default()
        .to_owned()
}

impl Devices {
    /// No thread runs; every device is off until `apply`.
    pub fn idle() -> Devices {
        let states = [
            DeviceKind::Scale,
            DeviceKind::Vfd,
            DeviceKind::SerialScanner,
        ]
        .into_iter()
        .map(|kind| (kind, DeviceState::Off))
        .collect();
        Devices {
            sinks: Mutex::new(Sinks::silent()),
            states: Arc::new(Mutex::new(states)),
            wanted: Mutex::new(Running::default()),
            lent: Mutex::new(Vec::new()),
            running: Mutex::new(Running::default()),
            scale: Mutex::new(None),
            display: Mutex::new(DisplayService::none()),
            current: Mutex::new(None),
            scanner: Mutex::new(None),
            shop: Mutex::new(String::new()),
        }
    }

    /// The sinks of the services started from now on.
    pub fn set_sinks(&self, sinks: Sinks) {
        *lock(&self.sinks) = sinks;
    }

    /// The status sink handed to the threads: records the state, then calls the window's sink.
    fn status_sink(&self) -> StatusSink {
        let states = Arc::clone(&self.states);
        let forward = lock(&self.sinks).status.clone();
        Arc::new(move |kind, state| {
            lock(&states).insert(kind, state.clone());
            forward(kind, state);
        })
    }

    fn set_state(&self, kind: DeviceKind, state: DeviceState) {
        let forward = lock(&self.sinks).status.clone();
        lock(&self.states).insert(kind, state.clone());
        forward(kind, &state);
    }

    /// Starts, restarts or stops each device whose section of `config` changed; the others keep
    /// running untouched. A device on a port lent to a probe starts once the probe gives it back.
    pub fn apply(&self, config: &HardwareConfig, links: Arc<dyn LinkFactory>) {
        *lock(&self.wanted) = Running::of(config);
        self.run(links, Replay::Current);
    }

    /// Brings the running services in line with the last `apply`, the lent ports left alone.
    fn run(&self, links: Arc<dyn LinkFactory>, replay: Replay) {
        let mut running = lock(&self.running);
        let wanted = lock(&self.wanted).clone().without(&lock(&self.lent));
        let sinks = lock(&self.sinks).clone();
        if running.scale != wanted.scale {
            // Dropped first: its thread ends and closes the port before the next one opens it.
            lock(&self.scale).take();
            match &wanted.scale {
                None => self.set_state(DeviceKind::Scale, DeviceState::Off),
                Some(cfg) => match ScaleService::start(
                    cfg.clone(),
                    Arc::clone(&links),
                    sinks.weight.clone(),
                    self.status_sink(),
                ) {
                    Ok(service) => *lock(&self.scale) = Some(service),
                    Err(error) => {
                        self.set_state(DeviceKind::Scale, disconnected(&cfg.serial.port, &error))
                    }
                },
            }
        }
        if running.vfd != wanted.vfd {
            let mut display = lock(&self.display);
            let frame = match replay {
                Replay::Current => lock(&self.current).clone(),
                // The paused service holds only what the till showed after the pause.
                Replay::ShownMeanwhile => display.current(),
            };
            // The old service stops (and gives its port up) before the new one starts.
            *display = DisplayService::none();
            match DisplayService::start(wanted.vfd.clone(), Arc::clone(&links), self.status_sink())
            {
                Ok(service) => {
                    service.set_shop(lock(&self.shop).clone());
                    if let Some(frame) = frame {
                        service.show(frame);
                    }
                    *display = service;
                    drop(display);
                    if wanted.vfd.is_none() {
                        self.set_state(DeviceKind::Vfd, DeviceState::Off);
                    }
                }
                Err(error) => {
                    drop(display);
                    let port = wanted
                        .vfd
                        .as_ref()
                        .map(|v| v.serial.port.clone())
                        .unwrap_or_default();
                    self.set_state(DeviceKind::Vfd, disconnected(&port, &error));
                }
            }
        }
        if running.scanner != wanted.scanner {
            lock(&self.scanner).take();
            match &wanted.scanner {
                None => self.set_state(DeviceKind::SerialScanner, DeviceState::Off),
                Some(cfg) => match SerialScanner::start(
                    cfg.clone(),
                    Arc::clone(&links),
                    sinks.scan.clone(),
                    self.status_sink(),
                ) {
                    Ok(service) => *lock(&self.scanner) = Some(service),
                    Err(error) => self.set_state(
                        DeviceKind::SerialScanner,
                        disconnected(&cfg.serial.port, &error),
                    ),
                },
            }
        }
        *running = wanted;
    }

    pub fn states(&self) -> Vec<DeviceStatus> {
        lock(&self.states)
            .iter()
            .map(|(device, state)| DeviceStatus {
                device: *device,
                state: state.clone(),
            })
            .collect()
    }

    /// `f` on the running scale (`scale_not_configured` when there is none). The scale stays
    /// locked meanwhile, so a restart waits for a `read_stable` (at most 3 s) to finish.
    pub fn with_scale<T>(
        &self,
        f: impl FnOnce(&ScaleService) -> Result<T, HwError>,
    ) -> Result<T, HwError> {
        let scale = lock(&self.scale);
        let service = scale.as_ref().ok_or(HwError::ScaleNotConfigured)?;
        f(service)
    }

    /// The pole display gets `frame` on its own thread; this returns at once.
    pub fn show(&self, frame: DisplayFrame) {
        let display = lock(&self.display);
        *lock(&self.current) = Some(frame.clone());
        display.show(frame);
    }

    pub fn current_frame(&self) -> Option<DisplayFrame> {
        lock(&self.current).clone()
    }

    pub fn set_shop(&self, latin_name: String) {
        lock(&self.display).set_shop(latin_name.clone());
        *lock(&self.shop) = latin_name;
    }

    /// Stops what holds `port`, runs `f` (a probe, a test), then starts what the last `apply`
    /// asks for: a configuration saved while `f` ran is the one that runs afterwards, and no
    /// service opens `port` before `f` ends. A resumed pole display writes nothing until the
    /// till's next frame, so a test pattern stays readable.
    pub fn with_port_free<T>(
        &self,
        port: &str,
        links: Arc<dyn LinkFactory>,
        f: impl FnOnce() -> T,
    ) -> T {
        // `com3 ` and `COM3` are one port.
        let port = port_key(port);
        let paused = {
            let mut running = lock(&self.running);
            lock(&self.lent).push(port.clone());
            let on_port = |serial: &SerialSettings| port_key(&serial.port) == port;
            let mut paused = Vec::new();
            if running.scale.as_ref().is_some_and(|s| on_port(&s.serial)) {
                lock(&self.scale).take();
                running.scale = None;
                paused.push(DeviceKind::Scale);
            }
            if running.scanner.as_ref().is_some_and(|s| on_port(&s.serial)) {
                lock(&self.scanner).take();
                running.scanner = None;
                paused.push(DeviceKind::SerialScanner);
            }
            if running.vfd.as_ref().is_some_and(|v| on_port(&v.serial)) {
                // A blank service keeps only what the till shows meanwhile (`Replay`).
                *lock(&self.display) = DisplayService::none();
                running.vfd = None;
                paused.push(DeviceKind::Vfd);
            }
            paused
        };
        let out = f();
        {
            let mut lent = lock(&self.lent);
            if let Some(i) = lent.iter().position(|p| *p == port) {
                lent.remove(i);
            }
        }
        self.run(links, Replay::ShownMeanwhile);
        // A device removed from the settings while it was paused stays off.
        let wanted = lock(&self.wanted).clone();
        for kind in paused.into_iter().filter(|kind| !wanted.has(*kind)) {
            self.set_state(kind, DeviceState::Off);
        }
        out
    }

    /// The app is closing: every thread ends and its port is closed (the pole display is
    /// cleared of the last sale).
    pub fn stop_all(&self) {
        lock(&self.scale).take();
        lock(&self.scanner).take();
        *lock(&self.display) = DisplayService::none();
    }
}

fn disconnected(port: &str, error: &HwError) -> DeviceState {
    DeviceState::Disconnected {
        port: port.to_owned(),
        code: error.code().to_owned(),
    }
}

/// After the database opened: the window sinks, the devices of `hardware.json`, the shop's
/// Latin name on the pole display, and the customer screen. Blocking (the database, the serial
/// ports, the window): call it on a blocking thread.
pub fn start(handle: &tauri::AppHandle) {
    use tauri::Manager;
    let Some(st) = handle
        .try_state::<crate::state::SharedState>()
        .map(|s| s.inner().clone())
    else {
        return;
    };
    st.devices.set_sinks(crate::events::sinks(handle));
    let config = match st.hw.config() {
        Ok(config) => config,
        Err(error) => {
            tracing::warn!(%error, "no hardware settings: devices stay off");
            return;
        }
    };
    match st.db.read(pos_core::company::get) {
        Ok(Some(company)) => st.devices.set_shop(latin_name(&company)),
        Ok(None) => {}
        Err(error) => tracing::warn!(%error, "the pole display starts without the shop's name"),
    }
    st.devices.apply(&config, st.hw.links());
    if let Err(error) = crate::customer_display::sync(handle, &config.display.screen) {
        tracing::warn!(%error, "the customer screen could not open");
    }
    // A monitor unplugged or plugged in: the customer screen follows (never onto the till's).
    let watched = handle.clone();
    crate::customer_display::watch(handle.clone(), move || {
        let st = watched.try_state::<crate::state::SharedState>()?;
        st.hw.config().ok().map(|c| c.display.screen)
    });
}

//! Devices through the command functions (Review Focus 2 and 5), over in-memory serial ports.

use super::devices::{self, ScaleProbe};
use super::tests::first_run;
use super::testshop::TestShop;
use crate::devices::Sinks;
use crate::state::AppState;
use pos_core::lang::Lang;
use pos_core::money::{Money, Qty};
use pos_hw::device::{DeviceKind, DeviceState};
use pos_hw::display::vfd::{INIT, bytes, lines, test_lines};
use pos_hw::display::{DisplayFrame, DisplayMode, DisplayService, VfdConfig};
use pos_hw::scale::{ScaleConfig, ScaleProtocol};
use pos_hw::serial::MemoryLinks;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn shop_with_ports(dir: &std::path::Path, links: Arc<MemoryLinks>) -> TestShop {
    let registry = pos_hw::Registry::open_with(dir.join("hardware.json"), links);
    let st = AppState::for_tests_with(registry, crate::printing::silent());
    st.devices.set_sinks(Sinks::silent());
    let owner = first_run(&st);
    TestShop { st, owner }
}

fn wait_until(what: &str, f: impl Fn() -> bool) {
    let started = Instant::now();
    while !f() {
        assert!(started.elapsed() < Duration::from_secs(3), "{what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn the_till_reads_a_stable_weight_and_tares() {
    let dir = tempfile::tempdir().unwrap();
    let links = MemoryLinks::new();
    links.stream("COM3", b"ST,GS,+0001.250kg\r\n");
    let shop = shop_with_ports(dir.path(), links);
    shop.owner();
    let mut config = shop.st.hw.config().unwrap();
    config.scale = Some(ScaleConfig {
        poll_ms: 50,
        ..ScaleConfig::new(ScaleProtocol::Continuous, "COM3")
    });
    super::hardware::save(&shop.st, config).unwrap();
    shop.cashier();
    let reading = devices::read_stable(&shop.st, 2_000).unwrap();
    assert_eq!((reading.net, reading.stable), (Qty(1_250), true));
    assert_eq!(devices::tare(&shop.st).unwrap().net, Qty::ZERO);
    assert_eq!(
        devices::clear_tare(&shop.st).unwrap().map(|e| e.net),
        Some(Qty(1_250))
    );
    wait_until("the scale reported connected", || {
        devices::status(&shop.st).unwrap().iter().any(|s| {
            s.device == DeviceKind::Scale && matches!(s.state, DeviceState::Connected { .. })
        })
    });
}

/// VRT-010: a weighed label carries the scale's stable weight at the moment it prints, never the
/// weight the window sent; without a scale, the weight typed by hand.
#[test]
fn a_weighed_label_carries_the_weight_the_scale_reads_now() {
    let dir = tempfile::tempdir().unwrap();
    let links = MemoryLinks::new();
    let shop = shop_with_ports(dir.path(), Arc::clone(&links));
    shop.owner();
    assert_eq!(
        super::labels::weighed_net(&shop.st, Qty(900)).unwrap(),
        Qty(900),
        "no scale: the weight typed"
    );
    links.stream("COM3", b"ST,GS,+0001.250kg\r\n");
    let mut config = shop.st.hw.config().unwrap();
    config.scale = Some(ScaleConfig {
        poll_ms: 50,
        ..ScaleConfig::new(ScaleProtocol::Continuous, "COM3")
    });
    super::hardware::save(&shop.st, config).unwrap();
    shop.cashier();
    assert_eq!(
        super::labels::weighed_net(&shop.st, Qty(900)).unwrap(),
        Qty(1_250),
        "the old 0,900 kg the window sent is not what prints"
    );
}

#[test]
fn display_show_reaches_the_vfd_and_becomes_the_current_frame() {
    let dir = tempfile::tempdir().unwrap();
    let links = MemoryLinks::new();
    links.wire("COM2");
    let shop = shop_with_ports(dir.path(), links.clone());
    shop.owner();
    let mut config = shop.st.hw.config().unwrap();
    config.display.vfd = Some(VfdConfig::new("COM2"));
    super::hardware::save(&shop.st, config).unwrap();
    shop.cashier();
    let frame = DisplayFrame {
        mode: DisplayMode::Sale,
        total: Money(125_000),
        ..DisplayFrame::idle(Lang::Fr)
    };
    devices::show(&shop.st, frame.clone()).unwrap();
    assert_eq!(devices::current(&shop.st).unwrap(), Some(frame));
    wait_until("the VFD shows the total", || {
        String::from_utf8_lossy(&links.written("COM2")).contains("1 250,00")
    });
    let negative = DisplayFrame {
        total: Money(-1),
        ..DisplayFrame::idle(Lang::Fr)
    };
    assert_eq!(
        devices::show(&shop.st, negative).unwrap_err().code(),
        "hardware_setting_invalid"
    );
}

#[test]
fn the_customer_screen_reads_without_a_session_and_the_till_needs_one() {
    let shop = super::testshop::shop();
    shop.st.set_session(None).unwrap();
    assert_eq!(devices::current(&shop.st).unwrap(), None);
    let branding = devices::shop_branding(&shop.st).unwrap();
    assert!(!branding.name_fr.is_empty() || !branding.name_ar.is_empty());
    let frame = DisplayFrame::idle(Lang::Ar);
    assert_eq!(
        devices::show(&shop.st, frame).unwrap_err().code(),
        "session_required"
    );
}

#[test]
fn a_pc_with_nothing_attached_lists_no_device_and_never_hangs() {
    let dir = tempfile::tempdir().unwrap();
    let shop = shop_with_ports(dir.path(), MemoryLinks::new());
    shop.owner();
    let states = devices::status(&shop.st).unwrap();
    assert!(
        states.iter().all(|s| s.state == DeviceState::Off),
        "{states:?}"
    );
    assert_eq!(
        devices::tare(&shop.st).unwrap_err().code(),
        "scale_not_configured"
    );
    let started = Instant::now();
    assert_eq!(
        devices::test_vfd(&shop.st, VfdConfig::new("COM77"))
            .unwrap_err()
            .code(),
        "port_unavailable"
    );
    assert_eq!(devices::probe(&shop.st, "COM78".into()).unwrap(), None);
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn a_probe_frees_the_port_of_the_running_scale_and_finds_it() {
    let dir = tempfile::tempdir().unwrap();
    let links = MemoryLinks::new();
    links.stream("COM3", b"ST,GS,+0000.500kg\r\n");
    let shop = shop_with_ports(dir.path(), links.clone());
    shop.owner();
    let mut config = shop.st.hw.config().unwrap();
    config.scale = Some(ScaleConfig {
        poll_ms: 50,
        ..ScaleConfig::new(ScaleProtocol::Continuous, "COM3")
    });
    super::hardware::save(&shop.st, config).unwrap();
    let found: Option<ScaleProbe> = devices::probe(&shop.st, "COM3".into()).unwrap();
    assert_eq!(found.map(|p| p.protocol), Some(ScaleProtocol::Continuous));
    shop.cashier();
    assert_eq!(
        devices::read_stable(&shop.st, 2_000).unwrap().net,
        Qty(500),
        "the scale runs again"
    );
}

/// A pole display on COM2 of a shop whose owner is signed in.
fn shop_with_vfd(dir: &std::path::Path, links: &Arc<MemoryLinks>) -> TestShop {
    links.wire("COM2");
    let shop = shop_with_ports(dir, links.clone());
    shop.owner();
    let mut config = shop.st.hw.config().unwrap();
    config.display.vfd = Some(VfdConfig::new("COM2"));
    super::hardware::save(&shop.st, config).unwrap();
    shop
}

#[test]
fn the_pole_display_greets_with_the_name_company_update_gave_the_shop() {
    let dir = tempfile::tempdir().unwrap();
    let links = MemoryLinks::new();
    let shop = shop_with_vfd(dir.path(), &links);
    let company = shop.st.db.read(pos_core::company::get).unwrap().unwrap();
    let mut input = pos_core::company::CompanyInput::from(company);
    input.name_fr = "Superette Nour".into();
    super::company::update(&shop.st, input).unwrap();
    devices::show(&shop.st, DisplayFrame::idle(Lang::Ar)).unwrap();
    wait_until("the idle screen names the shop in Latin letters", || {
        String::from_utf8_lossy(&links.written("COM2")).contains("Superette Nour")
    });
}

#[test]
fn a_vfd_test_on_the_running_display_keeps_the_customer_screen_frame() {
    let dir = tempfile::tempdir().unwrap();
    let links = MemoryLinks::new();
    let shop = shop_with_vfd(dir.path(), &links);
    let frame = DisplayFrame {
        mode: DisplayMode::Sale,
        total: Money(125_000),
        ..DisplayFrame::idle(Lang::Fr)
    };
    devices::show(&shop.st, frame.clone()).unwrap();
    devices::test_vfd(&shop.st, VfdConfig::new("COM2")).unwrap();
    assert_eq!(
        devices::current(&shop.st).unwrap(),
        Some(frame),
        "the customer screen still has the sale"
    );
    let next = DisplayFrame {
        mode: DisplayMode::Sale,
        total: Money(200_000),
        ..DisplayFrame::idle(Lang::Fr)
    };
    devices::show(&shop.st, next).unwrap();
    wait_until("the display runs again after the test", || {
        String::from_utf8_lossy(&links.written("COM2")).contains("2 000,00")
    });
}

fn sale(centimes: i64) -> DisplayFrame {
    DisplayFrame {
        mode: DisplayMode::Sale,
        total: Money(centimes),
        ..DisplayFrame::idle(Lang::Fr)
    }
}

fn shows(links: &MemoryLinks, port: &str, text: &str) -> bool {
    String::from_utf8_lossy(&links.written(port)).contains(text)
}

fn opens(links: &MemoryLinks, port: &str) -> u32 {
    links.wire(port).lock().map(|w| w.opens).unwrap_or_default()
}

#[test]
fn a_vfd_test_on_the_running_display_stays_readable_until_the_till_s_next_frame() {
    let dir = tempfile::tempdir().unwrap();
    let links = MemoryLinks::new();
    let shop = shop_with_vfd(dir.path(), &links);
    let cfg = VfdConfig::new("COM2");
    devices::show(&shop.st, sale(125_000)).unwrap();
    wait_until("the VFD shows the sale", || {
        shows(&links, "COM2", "1 250,00")
    });
    let opened = opens(&links, "COM2");
    devices::test_vfd(&shop.st, cfg.clone()).unwrap();
    // The test's own service opens the port, then the till's service takes it back.
    wait_until("the till's pole display took its port back", || {
        opens(&links, "COM2") >= opened + 2
    });
    std::thread::sleep(Duration::from_millis(300));
    let pattern = bytes(&test_lines(&cfg), &cfg);
    assert!(
        links.written("COM2").ends_with(&pattern),
        "the test pattern stays on the pole display"
    );
    assert_eq!(
        devices::current(&shop.st).unwrap(),
        Some(sale(125_000)),
        "the customer screen keeps the sale"
    );
    devices::show(&shop.st, sale(200_000)).unwrap();
    wait_until("the till's next frame replaces the pattern", || {
        shows(&links, "COM2", "2 000,00")
    });
    let next = bytes(&lines(&sale(200_000), "", &cfg), &cfg);
    assert!(
        links
            .written("COM2")
            .ends_with(&[pattern.as_slice(), &INIT, &next].concat()),
        "the next frame follows the pattern on the reopened link"
    );
}

#[test]
fn a_frame_the_till_shows_during_a_vfd_test_reaches_the_pole_display_after_it() {
    let dir = tempfile::tempdir().unwrap();
    let links = MemoryLinks::new();
    let shop = shop_with_vfd(dir.path(), &links);
    let cfg = VfdConfig::new("COM2");
    devices::show(&shop.st, sale(125_000)).unwrap();
    wait_until("the VFD shows the sale", || {
        shows(&links, "COM2", "1 250,00")
    });
    shop.st.devices.with_port_free("COM2", links.clone(), || {
        let tester = DisplayService::start(
            Some(cfg.clone()),
            links.clone(),
            pos_hw::device::silent_status(),
        )
        .unwrap();
        tester.test().unwrap();
        tester.stop();
        devices::show(&shop.st, sale(300_000)).unwrap();
    });
    wait_until("the sale shown meanwhile reaches the display", || {
        shows(&links, "COM2", "3 000,00")
    });
    let pattern = bytes(&test_lines(&cfg), &cfg);
    let meanwhile = bytes(&lines(&sale(300_000), "", &cfg), &cfg);
    assert!(
        links
            .written("COM2")
            .ends_with(&[pattern.as_slice(), &INIT, &meanwhile].concat())
    );
    assert_eq!(devices::current(&shop.st).unwrap(), Some(sale(300_000)));
}

#[test]
fn after_a_vfd_test_a_moved_pole_display_shows_the_till_s_current_frame() {
    let dir = tempfile::tempdir().unwrap();
    let links = MemoryLinks::new();
    let shop = shop_with_vfd(dir.path(), &links);
    devices::show(&shop.st, sale(125_000)).unwrap();
    wait_until("the VFD shows the sale", || {
        shows(&links, "COM2", "1 250,00")
    });
    devices::test_vfd(&shop.st, VfdConfig::new("COM2")).unwrap();
    links.wire("COM4");
    let mut config = shop.st.hw.config().unwrap();
    config.display.vfd = Some(VfdConfig::new("COM4"));
    super::hardware::save(&shop.st, config).unwrap();
    wait_until("the settings change shows the sale on the new port", || {
        shows(&links, "COM4", "1 250,00")
    });
}

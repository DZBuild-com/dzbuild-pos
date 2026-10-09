//! M4 carry-forwards (Task 20): a weighed label and the designer's preview report a lot or an
//! expiry date the label had no room for, the pole display knows the shop from the first run on,
//! and a hardware save that lands while a probe holds a port is what runs once the probe ends.

use super::devices;
use super::labels::{self, LabelPreviewInput, WeighedLabelInput};
use super::tests::first_run;
use super::testshop::TestShop;
use crate::devices::Sinks;
use crate::state::AppState;
use pos_core::catalog::extras::{self, ProductExtrasInput};
use pos_core::catalog::products::{Barcode, Product, ProductInput};
use pos_core::catalog::scale_items::ScaleItemInput;
use pos_core::catalog::{UNIT_KG_ID, UNIT_PIECE_ID};
use pos_core::labels::{LabelTemplate, LabelWarningCode, WEIGHED_TEMPLATE, WeighedLabelRequest};
use pos_core::lang::Lang;
use pos_core::money::{Money, Qty};
use pos_hw::config::LabelPrinterConfig;
use pos_hw::device::{DeviceKind, DeviceState};
use pos_hw::display::{DisplayFrame, VfdConfig};
use pos_hw::driver::Transport;
use pos_hw::labels::{LabelPreset, LabelProfile};
use pos_hw::scale::{ScaleConfig, ScaleProtocol};
use pos_hw::serial::MemoryLinks;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn wait_until(what: &str, f: impl Fn() -> bool) {
    let started = Instant::now();
    while !f() {
        assert!(started.elapsed() < Duration::from_secs(3), "{what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// An Xprinter label printer writing to a file of `dir`, the default of this PC.
fn label_printer(shop: &TestShop, dir: &std::path::Path) {
    let mut config = shop.st.hw.config().unwrap();
    config.label_printers.push(LabelPrinterConfig {
        id: "l1".into(),
        name: "Étiquettes".into(),
        transport: Transport::File {
            path: dir.join("labels.prn").to_string_lossy().into_owned(),
        },
        profile: LabelProfile::preset(LabelPreset::Xprinter),
    });
    config.label_printer_id = Some("l1".into());
    super::hardware::save(&shop.st, config).unwrap();
}

/// `template` saved under `id` next to the shop's templates.
fn save_template(shop: &TestShop, id: &str, template: LabelTemplate) {
    let mut templates = labels::templates(&shop.st).unwrap().templates;
    templates.push(LabelTemplate {
        id: id.into(),
        ..template
    });
    labels::save_templates(&shop.st, templates, BTreeMap::new()).unwrap();
}

/// Décret 13-378: a weighed package's expiry date (its shelf life) printed on a label too small
/// for it is left out; the print says so, as `labels_print` does.
#[test]
fn a_weighed_label_too_small_for_its_expiry_date_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let (shop, _failures) = super::testshop::shop_in(dir.path());
    shop.owner();
    label_printer(&shop, dir.path());
    let mut input = ProductInput::simple("Tomates", UNIT_KG_ID, &shop.rate_id(900), Money(12_000));
    input.name_ar = "طماطم".into();
    input.is_weighed = true;
    input.plu = Some(10012);
    let tomato = shop.product_with(input);
    let fresh = ScaleItemInput {
        shelf_life_days: Some(3),
        ..ScaleItemInput::default()
    };
    labels::set_scale_item(&shop.st, tomato.id.clone(), fresh).unwrap();
    let templates = labels::templates(&shop.st).unwrap().templates;
    let weighed = templates
        .iter()
        .find(|t| t.id == WEIGHED_TEMPLATE)
        .unwrap()
        .clone();
    save_template(
        &shop,
        "shop.weighed_40x22",
        LabelTemplate {
            width_dmm: 400,
            height_dmm: 220,
            ..weighed
        },
    );
    let print = |template_id: Option<&str>| {
        labels::print_weighed(
            &shop.st,
            WeighedLabelInput {
                request: WeighedLabelRequest {
                    product_id: tomato.id.clone(),
                    net: Qty(1_250),
                    copies: 1,
                    template_id: template_id.map(Into::into),
                    lang: None,
                },
                printer_id: None,
                arabic_override: false,
            },
        )
        .unwrap()
    };
    let small = print(Some("shop.weighed_40x22"));
    assert_eq!(
        (small.outcome.printer.as_str(), small.labels),
        ("Étiquettes", 1),
        "printed all the same"
    );
    let dropped: Vec<(&str, LabelWarningCode)> = small
        .warnings
        .iter()
        .map(|w| (w.product_id.as_str(), w.code))
        .collect();
    assert_eq!(
        dropped,
        [(tomato.id.as_str(), LabelWarningCode::LotOrExpiryDropped)]
    );
    let built_in = print(None);
    assert!(
        built_in.warnings.is_empty(),
        "the 58 × 40 weighed label holds it: {:?}",
        built_in.warnings
    );
}

/// A lot-tracked food product with its Arabic name and a barcode.
fn yogurt(shop: &TestShop) -> Product {
    let mut input = ProductInput::simple(
        "Yaourt Soummam",
        UNIT_PIECE_ID,
        &shop.rate_id(900),
        Money(4_000),
    );
    input.name_ar = "ياغورت الصومام".into();
    input.barcodes = vec![Barcode {
        barcode: "6130123456782".into(),
        pack_qty: Qty(1_000),
        label: None,
    }];
    let product = shop.product_with(input);
    let (id, actor) = (product.id.clone(), shop.owner.actor());
    let input = ProductExtrasInput {
        track_lots: true,
        ..ProductExtrasInput::default()
    };
    shop.st
        .db
        .write(move |tx| extras::set(tx, &actor, &id, input))
        .unwrap();
    product
}

/// The designer previews a lot-tracked product with its lot, so a label too small for the lot
/// says so before anything is printed.
#[test]
fn the_preview_of_a_lot_tracked_product_says_when_its_lot_does_not_fit() {
    let shop = super::testshop::shop();
    shop.owner();
    let yogurt = yogurt(&shop);
    let (product_id, now) = (yogurt.id.clone(), shop.st.now());
    shop.st
        .db
        .write(move |tx| {
            pos_core::stock::lots::get_or_create(
                tx,
                &product_id,
                &pos_core::stock::lots::LotInput {
                    lot_number: "L2609-ALGER-BAB-EZZOUAR-000123-A".into(),
                    expiry_date: Some("2027-03-31".into()),
                },
                None,
                now,
            )
        })
        .unwrap();
    let mut template = labels::templates(&shop.st).unwrap().templates[0].clone();
    template.id = "shop.lot_40x25".into();
    (template.fields.lot, template.fields.expiry) = (true, true);
    let preview = |template: LabelTemplate, product_id: &str| {
        labels::preview(
            &shop.st,
            LabelPreviewInput {
                template,
                product_id: Some(product_id.into()),
                lang: None,
            },
        )
        .unwrap()
    };
    let tracked = preview(template.clone(), &yogurt.id);
    assert_eq!(
        tracked.label.lot_number.as_deref(),
        Some("L2609-ALGER-BAB-EZZOUAR-000123-A")
    );
    let codes: Vec<LabelWarningCode> = tracked.warnings.iter().map(|w| w.code).collect();
    assert_eq!(codes, [LabelWarningCode::LotOrExpiryDropped]);
    let plain = yogurt_without_lots(&shop);
    assert!(preview(template, &plain.id).warnings.is_empty());
}

fn yogurt_without_lots(shop: &TestShop) -> Product {
    let mut input = ProductInput::simple(
        "Yaourt nature",
        UNIT_PIECE_ID,
        &shop.rate_id(900),
        Money(3_500),
    );
    input.name_ar = "ياغورت طبيعي".into();
    shop.product_with(input)
}

/// A shop whose serial ports are `links`, before its first run.
fn state_with_ports(dir: &std::path::Path, links: Arc<MemoryLinks>) -> crate::state::SharedState {
    let registry = pos_hw::Registry::open_with(dir.join("hardware.json"), links);
    let st = AppState::for_tests_with(registry, crate::printing::silent());
    st.devices.set_sinks(Sinks::silent());
    st
}

/// The pole display shows the shop's name from the first session on, not from the next start.
#[test]
fn the_pole_display_greets_with_the_shop_named_in_the_first_run() {
    let dir = tempfile::tempdir().unwrap();
    let links = MemoryLinks::new();
    links.wire("COM2");
    let st = state_with_ports(dir.path(), links.clone());
    let mut config = st.hw.config().unwrap();
    config.display.vfd = Some(VfdConfig::new("COM2"));
    let saved = st.hw.save(config).unwrap();
    st.devices.apply(&saved, st.hw.links());
    first_run(&st);
    devices::show(&st, DisplayFrame::idle(Lang::Fr)).unwrap();
    wait_until("the idle screen names the new shop", || {
        String::from_utf8_lossy(&links.written("COM2")).contains("El Amel")
    });
}

fn opens(links: &MemoryLinks, port: &str) -> u32 {
    links.wire(port).lock().map(|w| w.opens).unwrap_or_default()
}

fn scale_on(port: &str, poll_ms: u16) -> ScaleConfig {
    ScaleConfig {
        poll_ms,
        ..ScaleConfig::new(ScaleProtocol::Continuous, port)
    }
}

/// A shop whose scale streams 0.500 kg on COM3 and is connected; the owner is signed in.
fn shop_with_scale(dir: &std::path::Path, links: &Arc<MemoryLinks>) -> TestShop {
    links.stream("COM3", b"ST,GS,+0000.500kg\r\n");
    let st = state_with_ports(dir, links.clone());
    let owner = first_run(&st);
    let shop = TestShop { st, owner };
    let mut config = shop.st.hw.config().unwrap();
    config.scale = Some(scale_on("COM3", 50));
    super::hardware::save(&shop.st, config).unwrap();
    wait_until("the scale reported connected", || connected(&shop));
    shop
}

fn connected(shop: &TestShop) -> bool {
    devices::status(&shop.st)
        .unwrap()
        .iter()
        .any(|s| s.device == DeviceKind::Scale && matches!(s.state, DeviceState::Connected { .. }))
}

fn save_scale(shop: &TestShop, scale: Option<ScaleConfig>) {
    let mut config = shop.st.hw.config().unwrap();
    config.scale = scale;
    super::hardware::save(&shop.st, config).unwrap();
}

#[test]
fn a_scale_moved_while_a_probe_holds_its_port_runs_on_its_new_port_after_it() {
    let dir = tempfile::tempdir().unwrap();
    let links = MemoryLinks::new();
    links.stream("COM5", b"ST,GS,+0000.750kg\r\n");
    let shop = shop_with_scale(dir.path(), &links);
    let opened = opens(&links, "COM3");
    shop.st.devices.with_port_free("COM3", links.clone(), || {
        save_scale(&shop, Some(scale_on("COM5", 50)));
    });
    shop.cashier();
    assert_eq!(
        devices::read_stable(&shop.st, 2_000).unwrap().net,
        Qty(750),
        "the scale saved during the probe"
    );
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(opens(&links, "COM3"), opened, "the old port stays closed");
}

#[test]
fn a_scale_removed_while_a_probe_holds_its_port_stays_off_after_it() {
    let dir = tempfile::tempdir().unwrap();
    let links = MemoryLinks::new();
    let shop = shop_with_scale(dir.path(), &links);
    let opened = opens(&links, "COM3");
    shop.st.devices.with_port_free("COM3", links.clone(), || {
        save_scale(&shop, None);
    });
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(opens(&links, "COM3"), opened, "no scale comes back");
    let scale = devices::status(&shop.st)
        .unwrap()
        .into_iter()
        .find(|s| s.device == DeviceKind::Scale)
        .unwrap();
    assert_eq!(scale.state, DeviceState::Off);
    shop.cashier();
    assert_eq!(
        devices::tare(&shop.st).unwrap_err().code(),
        "scale_not_configured"
    );
}

#[test]
fn a_scale_saved_on_the_port_a_probe_holds_waits_for_the_probe_then_runs() {
    let dir = tempfile::tempdir().unwrap();
    let links = MemoryLinks::new();
    let shop = shop_with_scale(dir.path(), &links);
    let opened = opens(&links, "COM3");
    shop.st.devices.with_port_free("COM3", links.clone(), || {
        save_scale(&shop, Some(scale_on("COM3", 60)));
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(
            opens(&links, "COM3"),
            opened,
            "no service opens the port the probe holds"
        );
    });
    wait_until("the saved scale opened its port after the probe", || {
        opens(&links, "COM3") > opened
    });
    shop.cashier();
    assert_eq!(devices::read_stable(&shop.st, 2_000).unwrap().net, Qty(500));
}

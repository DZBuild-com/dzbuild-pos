//! A butcher-greengrocer's morning over simulated devices: the scale weighs, the till sells,
//! the pole display follows, the label printer prints a weighed label that the till reads
//! back, the PLU file goes to the label scale, and the books balance to the gram and centime.

use super::devices;
use super::labels::{self, WeighedLabelInput};
use super::tests::first_run;
use super::testshop::TestShop;
use crate::devices::Sinks;
use crate::state::AppState;
use pos_core::auth::Permission;
use pos_core::catalog::products::ProductInput;
use pos_core::catalog::{UNIT_KG_ID, scale_codes};
use pos_core::labels::WeighedLabelRequest;
use pos_core::lang::Lang;
use pos_core::money::{Money, Qty};
use pos_core::payments::PaymentInput;
use pos_core::sales::{self, Cart, CartLine, SaleInput};
use pos_hw::config::LabelPrinterConfig;
use pos_hw::display::{DisplayFrame, DisplayMode, VfdConfig};
use pos_hw::driver::Transport;
use pos_hw::labels::{LabelPreset, LabelProfile};
use pos_hw::scale::{ScaleConfig, ScaleProtocol};
use pos_hw::serial::MemoryLinks;
use std::time::{Duration, Instant};

#[test]
fn a_weighed_morning_balances() {
    let dir = tempfile::tempdir().unwrap();
    let links = MemoryLinks::new();
    links.stream("COM3", b"ST,GS,+0001.250kg\r\n");
    links.wire("COM2");
    let registry = pos_hw::Registry::open_with(dir.path().join("hardware.json"), links.clone());
    let st = AppState::for_tests_with(registry, crate::printing::silent());
    st.devices.set_sinks(Sinks::silent());
    let owner = first_run(&st);
    let shop = TestShop { st, owner };
    shop.owner();

    // hardware: scale, pole display, label printer
    let labels_out = dir.path().join("labels.prn");
    let mut config = shop.st.hw.config().unwrap();
    config.scale = Some(ScaleConfig {
        poll_ms: 50,
        ..ScaleConfig::new(ScaleProtocol::Continuous, "COM3")
    });
    config.display.vfd = Some(VfdConfig::new("COM2"));
    config.label_printers.push(LabelPrinterConfig {
        id: "l1".into(),
        name: "Étiquettes".into(),
        transport: Transport::File {
            path: labels_out.to_string_lossy().into_owned(),
        },
        profile: LabelProfile::preset(LabelPreset::Xprinter),
    });
    config.label_printer_id = Some("l1".into());
    super::hardware::save(&shop.st, config).unwrap();

    // catalogue: tomatoes by the kilo, PLU 10012, 5 kg in stock
    let mut input = ProductInput::simple("Tomates", UNIT_KG_ID, &shop.rate_id(900), Money(12_000));
    input.name_ar = "طماطم".into();
    input.is_weighed = true;
    input.plu = Some(10012);
    let tomato = shop.product_with(input);
    shop.stock_in(&tomato.id, 5);

    // the cashier weighs 1.250 kg on the scale
    shop.cashier();
    shop.open_till(0);
    let reading = devices::read_stable(&shop.st, 2_000).unwrap();
    assert_eq!(reading.net, Qty(1_250));

    // a weighed label for a take-away package, read back at the till
    let printed = labels::print_weighed(
        &shop.st,
        WeighedLabelInput {
            request: WeighedLabelRequest {
                product_id: tomato.id.clone(),
                net: reading.net,
                copies: 1,
                template_id: None,
                lang: None,
            },
            printer_id: None,
            arabic_override: false,
        },
    )
    .unwrap();
    assert_eq!(printed.outcome.printer, "Étiquettes");
    let label_code = scale_codes::encode(&scale_codes::pack_layouts()[0], 10012, 1_250).unwrap();
    assert!(
        String::from_utf8_lossy(&std::fs::read(&labels_out).unwrap()).contains(&label_code[..12])
    );
    let scanned = super::products::find_by_code(&shop.st, label_code)
        .unwrap()
        .unwrap();
    assert_eq!(scanned.pack_qty, Qty(1_250));

    // one sale: the weighed line and the scanned package
    let ctx = shop.st.till(Permission::SalesCreate).unwrap();
    let product = tomato.id.clone();
    let sale = shop
        .st
        .db
        .write(move |tx| {
            sales::create_sale(
                tx,
                &ctx,
                SaleInput {
                    cart: Cart::new(
                        Lang::Fr,
                        vec![
                            CartLine::product(&product, Qty(1_250)),
                            CartLine::product(&product, Qty(1_250)),
                        ],
                    ),
                    payments: vec![PaymentInput::cash(Money(40_000))],
                },
            )
        })
        .unwrap();
    assert_eq!(
        sale.document.total_ttc,
        Money(30_000),
        "2 × 1.250 kg × 120 DA"
    );

    // the pole display follows the paid frame
    let paid = DisplayFrame {
        mode: DisplayMode::Paid,
        total: Money(30_000),
        paid: Some(Money(40_000)),
        change: Some(Money(10_000)),
        ..DisplayFrame::idle(Lang::Fr)
    };
    devices::show(&shop.st, paid).unwrap();
    let started = Instant::now();
    while !String::from_utf8_lossy(&links.written("COM2")).contains("Rendu") {
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "the VFD shows the change"
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    // the PLU file for the label scale
    shop.owner();
    let csv = String::from_utf8(
        labels::plu_bytes(&shop.st, pos_docs::sheet::SheetFormat::Csv, Lang::Fr).unwrap(),
    )
    .unwrap();
    assert!(csv.contains("10012") && csv.contains("Tomates"));

    // the books: 5 kg − 2.5 kg, the chains intact, the label journaled
    let id = tomato.id.clone();
    assert_eq!(
        shop.st
            .db
            .read(move |c| pos_core::stock::on_hand(c, &id))
            .unwrap(),
        Qty(2_500)
    );
    assert_eq!(
        shop.st
            .db
            .read(pos_core::audit::verify_chain)
            .unwrap()
            .first_broken_seq,
        None
    );
    let chains = shop.st.db.read(pos_core::documents::verify_all).unwrap();
    assert_eq!(chains.iter().map(|c| c.rows).sum::<i64>(), 1, "{chains:?}");
    assert!(
        chains.iter().all(|c| c.first_broken_seq.is_none()),
        "{chains:?}"
    );
    let journaled = shop
        .st
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM audit_log WHERE action = 'labels.printed'",
                [],
                |r| r.get::<_, i64>(0),
            )?)
        })
        .unwrap();
    assert_eq!(journaled, 1);
}

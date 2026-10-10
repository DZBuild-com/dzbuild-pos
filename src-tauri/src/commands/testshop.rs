//! A shop for command tests: the M0 first run (IFU, TTC; owner "Amine Benali", PIN 2580), a
//! cashier (10 % discount limit), a manager, products and opening stock, all through services.

use super::auth::{self, LoginInput};
use super::tests::first_run;
use crate::printing::{Notifier, PrintFailed};
use crate::state::{AppState, SharedState};
use pos_core::auth::{CASHIER_ROLE_ID, MANAGER_ROLE_ID, NewUser, Permission, Session, hash_pin};
use pos_core::cash::{self, CashSession};
use pos_core::catalog::products::{self, Barcode, Product, ProductInput};
use pos_core::catalog::{UNIT_PIECE_ID, tax_rates};
use pos_core::lang::Lang;
use pos_core::money::{Money, Qty, Rate};
use pos_core::payments::{PaymentInput, PaymentMethod};
use pos_core::sales::{self, Cart, CartLine, SaleInput, ValidatedSale};
use pos_core::stock::{self, AdjustInput, AdjustReason};
use pos_core::time::now_ms;
use pos_hw::config::{HardwareConfig, PrinterConfig};
use pos_hw::driver::Transport;
use pos_hw::profile::{PaperWidth, Preset, Profile};
use std::path::Path;
use std::sync::Arc;

pub(crate) const OWNER_PIN: &str = "2580";
pub(crate) const CASHIER_PIN: &str = "4826";
pub(crate) const MANAGER_PIN: &str = "7391";

pub(crate) struct TestShop {
    pub st: SharedState,
    pub owner: Session,
}

pub(crate) fn shop() -> TestShop {
    let st = AppState::for_tests();
    let owner = first_run(&st);
    TestShop { st, owner }
}

/// The M0 first run on a state built elsewhere (the DZBuild tests' runtime).
pub(crate) fn first_run_shop(st: SharedState) -> TestShop {
    let owner = first_run(&st);
    TestShop { st, owner }
}

/// A shop whose hardware settings live in `dir` (a file printer may only write a `.bin` file
/// there) and whose print failures land in a channel.
pub(crate) fn shop_in(dir: &Path) -> (TestShop, crossbeam_channel::Receiver<PrintFailed>) {
    let (tx, rx) = crossbeam_channel::unbounded();
    let notify: Notifier = Arc::new(move |event| {
        let _ = tx.send(event);
    });
    let st = AppState::for_tests_with(pos_hw::Registry::open(dir.join("hardware.json")), notify);
    let owner = first_run(&st);
    (TestShop { st, owner }, rx)
}

impl TestShop {
    /// A user created by the owner (the owner's session is used for the audit row).
    pub(crate) fn user(
        &self,
        name: &str,
        role_id: &str,
        pin: &str,
        discount_limit_bp: i32,
    ) -> String {
        let actor = self.owner.actor();
        let hash = hash_pin(pin).unwrap();
        let input = NewUser {
            username: None,
            display_name: name.into(),
            pin: String::new(),
            role_id: role_id.into(),
            lang: Lang::Fr,
            discount_limit_bp,
        };
        self.st
            .db
            .write(move |tx| pos_core::auth::users::create(tx, &actor, &input, &hash))
            .unwrap()
            .id
    }

    pub(crate) fn sign_in(&self, user_id: &str, pin: &str) -> Session {
        auth::login(
            &self.st,
            &LoginInput {
                user: user_id.into(),
                pin: pin.into(),
            },
        )
        .unwrap()
    }

    fn find_or_create(&self, name: &str, role_id: &str, pin: &str, limit: i32) -> String {
        let tiles = self.st.db.read(pos_core::auth::users::list_tiles).unwrap();
        tiles
            .into_iter()
            .find(|t| t.display_name == name)
            .map(|t| t.id)
            .unwrap_or_else(|| self.user(name, role_id, pin, limit))
    }

    /// Signs "Yacine" in (Caissier, 10 % discount limit, PIN 4826).
    pub(crate) fn cashier(&self) -> Session {
        let id = self.find_or_create("Yacine", CASHIER_ROLE_ID, CASHIER_PIN, 1_000);
        self.sign_in(&id, CASHIER_PIN)
    }

    /// Signs "Samira" in (Gérant, PIN 7391).
    pub(crate) fn manager(&self) -> Session {
        let id = self.find_or_create("Samira", MANAGER_ROLE_ID, MANAGER_PIN, 2_000);
        self.sign_in(&id, MANAGER_PIN)
    }

    /// Signs the owner back in.
    pub(crate) fn owner(&self) -> Session {
        self.sign_in(&self.owner.user_id, OWNER_PIN)
    }

    pub(crate) fn rate_id(&self, bp: i32) -> String {
        self.st
            .db
            .read(tax_rates::list)
            .unwrap()
            .into_iter()
            .find(|r| r.rate_bp == Rate(bp) && r.active)
            .unwrap()
            .id
    }

    pub(crate) fn product_with(&self, input: ProductInput) -> Product {
        let actor = self.owner.actor();
        self.st
            .db
            .write(move |tx| products::create(tx, &actor, input))
            .unwrap()
    }

    /// A piece product at `price` centimes TTC, VAT `rate_bp`, with barcode `barcode` if any.
    pub(crate) fn product(
        &self,
        name_fr: &str,
        price: i64,
        rate_bp: i32,
        barcode: Option<&str>,
    ) -> Product {
        let mut input =
            ProductInput::simple(name_fr, UNIT_PIECE_ID, &self.rate_id(rate_bp), Money(price));
        input.cost_price = Money(price * 6 / 10);
        if let Some(code) = barcode {
            input.barcodes = vec![Barcode {
                barcode: code.into(),
                pack_qty: Qty(1_000),
                label: None,
            }];
        }
        self.product_with(input)
    }

    /// Opening stock of `pieces` at the default location.
    pub(crate) fn stock_in(&self, product_id: &str, pieces: i64) {
        let actor = self.owner.actor();
        let input = AdjustInput {
            product_id: product_id.into(),
            location_id: None,
            qty_delta: Qty(pieces * 1_000),
            reason: AdjustReason::Opening,
            unit_cost: None,
            note: None,
        };
        self.st
            .db
            .write(move |tx| stock::adjust(tx, &actor, input, now_ms()))
            .unwrap();
    }

    /// Opens the till for whoever is signed in, with `float` centimes.
    pub(crate) fn open_till(&self, float: i64) -> CashSession {
        let ctx = self.st.till(Permission::CashOpen).unwrap();
        self.st
            .db
            .write(move |tx| cash::open_session(tx, &ctx, Money(float)))
            .unwrap()
    }

    /// Sells `pieces` of `product_id`, paid `cash` centimes, as whoever is signed in.
    pub(crate) fn sell(&self, product_id: &str, pieces: i64, cash: i64) -> ValidatedSale {
        let ctx = self.st.till(Permission::SalesCreate).unwrap();
        let input = SaleInput {
            cart: Cart::new(
                Lang::Fr,
                vec![CartLine::product(product_id, Qty(pieces * 1_000))],
            ),
            payments: vec![PaymentInput::cash(Money(cash))],
        };
        self.st
            .db
            .write(move |tx| sales::create_sale(tx, &ctx, input))
            .unwrap()
    }

    /// The company with every mention an invoice needs (décret 05-468 art. 3) and its activity.
    pub(crate) fn complete_company(&self) {
        let (actor, pack) = (self.owner.actor(), self.st.datapack.clone());
        self.st
            .db
            .write(move |tx| {
                let current = pos_core::company::get(tx)?
                    .ok_or_else(|| pos_core::error::AppError::conflict("setup_required"))?;
                let mut input = pos_core::company::CompanyInput::from(current);
                input.legal_form = Some("SARL".into());
                input.capital = Some(Money(100_000_000));
                input.address = Some("12 rue Larbi Ben M'hidi, Alger".into());
                input.wilaya = Some("16".into());
                input.phone = Some("023 45 67 89".into());
                input.rc = Some("16/00-1234567B26".into());
                input.nif = Some("000116001234567".into());
                input.nis = Some("000116007654321".into());
                input.ai = Some("16012345678".into());
                input.rib = Some("00799999001234567890".into());
                pos_core::company::save(tx, &actor, &pack, input)?;
                pos_core::settings::set_value(
                    tx,
                    &actor,
                    "company.activity",
                    serde_json::json!({
                        "ar": "تجارة التجزئة للمواد الغذائية العامة",
                        "fr": "Commerce de détail en alimentation générale",
                        "en": "General food retail"
                    }),
                )
                .map(drop)
            })
            .unwrap();
    }

    /// `complete_company`, then the réel regime (TVA shown, invoices HT + TVA).
    pub(crate) fn reel_company(&self) {
        self.complete_company();
        let (actor, pack) = (self.owner.actor(), self.st.datapack.clone());
        self.st
            .db
            .write(move |tx| {
                let current = pos_core::company::get(tx)?
                    .ok_or_else(|| pos_core::error::AppError::conflict("setup_required"))?;
                let mut input = pos_core::company::CompanyInput::from(current);
                input.tax_regime = pos_core::company::TaxRegime::Reel;
                pos_core::company::save(tx, &actor, &pack, input).map(drop)
            })
            .unwrap();
    }

    /// A reseller with every identifier: a B2B buyer.
    pub(crate) fn b2b_customer(&self, name: &str) -> pos_core::customers::Customer {
        let (actor, pack, name) = (
            self.owner.actor(),
            self.st.datapack.clone(),
            name.to_owned(),
        );
        self.st
            .db
            .write(move |tx| {
                let mut input = pos_core::customers::CustomerInput::named(&name);
                input.customer_type = pos_core::customers::CustomerType::Reseller;
                input.address = Some("5 rue Didouche Mourad, Alger".into());
                input.wilaya = Some("16".into());
                input.nif = Some("000016001234567".into());
                input.rc = Some("16/00-7654321B26".into());
                input.nis = Some("000016007654321".into());
                input.ai = Some("16098765432".into());
                input.legal_form = Some("SARL".into());
                input.activity = Some("Quincaillerie".into());
                pos_core::customers::create(tx, &actor, &pack, input)
            })
            .unwrap()
    }

    /// Signs in "Karim", the only user of a custom role holding exactly `permissions` (once per
    /// shop: role names are unique).
    pub(crate) fn with_permissions(&self, permissions: &[Permission]) -> Session {
        let actor = self.owner.actor();
        let input = pos_core::auth::roles::RoleInput {
            name_ar: "دور تجريبي".into(),
            name_fr: "Rôle d'essai".into(),
            name_en: "Test role".into(),
            permissions: permissions.to_vec(),
        };
        let role = self
            .st
            .db
            .write(move |tx| pos_core::auth::roles::create(tx, &actor, input))
            .unwrap();
        let id = self.user("Karim", &role.id, "5173", 0);
        self.sign_in(&id, "5173")
    }

    /// The owner approves `permission` for the next till write of whoever is signed in.
    pub(crate) fn approve(&self, permission: Permission) {
        auth::manager_override(
            &self.st,
            &auth::OverrideInput {
                approver_id: self.owner.user_id.clone(),
                pin: OWNER_PIN.into(),
                permission,
            },
        )
        .unwrap();
    }

    /// A validated invoice of 1 190.00 DA TTC to "SARL Amine", on account, made by the owner.
    pub(crate) fn validated_invoice(&self) -> String {
        self.complete_company();
        let customer = self.b2b_customer("SARL Amine");
        let cement = self.product("Ciment 25 kg", 119_000, 1_900, None);
        self.stock_in(&cement.id, 10);
        self.owner();
        let mut cart = Cart::new(Lang::Fr, vec![CartLine::product(&cement.id, Qty(1_000))]);
        cart.customer_id = Some(customer.id);
        let input = pos_core::billing::drafts::DraftInput {
            doc_type: pos_core::documents::DocType::Invoice,
            cart,
            valid_until: None,
            due_date: None,
            second_lang: None,
            draft_id: None,
        };
        let draft = crate::commands::billing::save_draft(&self.st, None, input).unwrap();
        let payments = vec![PaymentInput {
            method: pos_core::payments::PaymentMethod::Credit,
            ..PaymentInput::cash(Money(119_000))
        }];
        let validate = pos_core::billing::validate::ValidateInput {
            id: draft.id,
            payments,
        };
        crate::commands::billing::validate(&self.st, validate)
            .unwrap()
            .document
            .id
    }

    /// A customer of the shop by name (M2's quick creation, by the owner): his id.
    pub(crate) fn customer(&self, name: &str) -> String {
        let (actor, pack) = (self.owner.actor(), self.st.datapack.clone());
        let input = pos_core::customers::CustomerInput::named(name);
        self.st
            .db
            .write(move |tx| pos_core::customers::create(tx, &actor, &pack, input))
            .unwrap()
            .id
    }

    /// An M3 delivery note of `pieces` of `product_id` to `customer_id`, validated by whoever is
    /// signed in (a BL takes no payment: the customer owes its total on delivery).
    pub(crate) fn delivery_note_on_account(
        &self,
        customer_id: &str,
        product_id: &str,
        pieces: i64,
    ) -> pos_core::documents::Document {
        self.complete_company();
        let mut cart = Cart::new(
            Lang::Fr,
            vec![CartLine::product(product_id, Qty(pieces * 1_000))],
        );
        cart.customer_id = Some(customer_id.into());
        let input = pos_core::billing::drafts::DraftInput {
            doc_type: pos_core::documents::DocType::DeliveryNote,
            cart,
            valid_until: None,
            due_date: None,
            second_lang: None,
            draft_id: None,
        };
        let draft = crate::commands::billing::save_draft(&self.st, None, input).unwrap();
        let validate = pos_core::billing::validate::ValidateInput {
            id: draft.id,
            payments: Vec::new(),
        };
        crate::commands::billing::validate(&self.st, validate)
            .unwrap()
            .document
    }

    /// A cash ticket of one piece at 150.00 DA, sold by the owner at an open till.
    pub(crate) fn cash_ticket(&self) -> String {
        let milk = self.product("Lait 1 L", 15_000, 900, None);
        self.stock_in(&milk.id, 5);
        self.owner();
        self.open_till(1_000_000);
        self.sell(&milk.id, 1, 20_000).document.id
    }

    /// This PC prints `doc_type` on the Windows printer `printer`, A4, `copies` at a time.
    pub(crate) fn set_document_printer(
        &self,
        doc_type: pos_core::documents::DocType,
        printer: &str,
        copies: u8,
    ) {
        let mut config = self.st.hw.config().unwrap();
        config.documents.kinds.retain(|k| k.doc_type != doc_type);
        config.documents.kinds.push(pos_hw::config::KindPrinting {
            doc_type,
            printer: Some(printer.into()),
            paper: pos_core::billing::settings::Paper::A4,
            copies,
        });
        self.st.hw.save(config).unwrap();
    }

    /// A piece product at `price` centimes TTC (19 %) in the category `category_id`; its id.
    pub(crate) fn product_in_category(
        &self,
        name_fr: &str,
        price: i64,
        category_id: &str,
    ) -> String {
        let mut input =
            ProductInput::simple(name_fr, UNIT_PIECE_ID, &self.rate_id(1_900), Money(price));
        input.category_id = Some(category_id.into());
        self.product_with(input).id
    }

    /// A VAT-exempt medicine at `price` centimes whose pharmacy row says controlled (the
    /// pharmacy module must be on); its id.
    pub(crate) fn controlled_medicine(&self, name_fr: &str, price: i64) -> String {
        let id = self.product(name_fr, price, 0, None).id;
        let (actor, product_id) = (self.owner.actor(), id.clone());
        let input = pos_core::pharmacy::PharmacyProductInput {
            dci: Some(name_fr.into()),
            controlled: true,
            schedule: Some("Psychotrope".into()),
            prescription_required: true,
        };
        self.st
            .db
            .write(move |tx| pos_core::pharmacy::set(tx, &actor, &product_id, input))
            .unwrap();
        id
    }

    /// `SELECT COUNT(*) …` on the shop's database.
    pub(crate) fn count(&self, sql: &'static str) -> i64 {
        self.st
            .db
            .read(move |c| Ok(c.query_row(sql, [], |r| r.get(0))?))
            .unwrap()
    }
}

/// An auto soldes promotion over one product and an open full count, created through the
/// services by the owner: every report kind can then run, the two that need one included.
pub(crate) fn promotion_and_count(shop: &TestShop) -> (String, String) {
    use pos_core::promotions::{
        self, Campaign, PromoKind, PromoScope, PromotionInput, TriggerMode,
    };
    use pos_core::stock::counts::{self, CountInput, CountScope};
    let jacket = shop.product("Veste hiver", 890_000, 1_900, None);
    shop.stock_in(&jacket.id, 12);
    let soldes = PromotionInput {
        name_ar: String::new(),
        name_fr: "Soldes d'automne".into(),
        name_en: String::new(),
        kind: PromoKind::Percent,
        campaign: Campaign::Solde,
        trigger_mode: TriggerMode::Auto,
        scope: PromoScope::Products {
            ids: vec![jacket.id],
        },
        value: 3_000,
        buy_qty: None,
        get_qty: None,
        min_qty: Qty::ZERO,
        priority: 0,
        starts_on: Some("2026-09-01".into()),
        ends_on: Some("2026-09-30".into()),
        active: true,
    };
    let actor = shop.owner.actor();
    let promotion = shop
        .st
        .db
        .write(move |tx| promotions::create(tx, &actor, soldes))
        .unwrap();
    let ctx =
        pos_core::till::TillContext::new(shop.owner.clone(), shop.st.datapack.clone(), now_ms());
    let full = CountInput {
        location_id: None,
        scope: CountScope::Full,
        category_id: None,
        product_ids: Vec::new(),
        blind: false,
        note: None,
    };
    let count = shop
        .st
        .db
        .write(move |tx| counts::start(tx, &ctx, full))
        .unwrap();
    (promotion.id, count.id)
}

/// A receipt printer that writes to `path` (80 mm, generic). The registry accepts it only when
/// `path` is a `.bin`/`.prn` file inside the folder of its `hardware.json`.
pub(crate) fn file_printer(path: &std::path::Path) -> HardwareConfig {
    let mut config = HardwareConfig::default();
    config.printers.push(PrinterConfig {
        id: "p1".into(),
        name: "Caisse 1".into(),
        transport: Transport::File {
            path: path.to_string_lossy().into_owned(),
        },
        profile: Profile::preset(Preset::Generic, PaperWidth::Mm80),
        mode: pos_hw::config::PrintMode::Raw,
        paper_width_mm: 80,
    });
    config.receipt_printer_id = Some("p1".into());
    config
}

/// This PC prints on one `File` printer per name in `dir` (80 mm, generic), the first one as the
/// receipt printer.
pub(crate) fn use_file_printers(st: &AppState, dir: &Path, names: &[&str]) {
    let mut config = HardwareConfig::default();
    for (i, name) in names.iter().enumerate() {
        config.printers.push(PrinterConfig {
            id: format!("p{}", i + 1),
            name: (*name).to_owned(),
            transport: Transport::File {
                path: dir.join(name).to_string_lossy().into_owned(),
            },
            profile: Profile::preset(Preset::Generic, PaperWidth::Mm80),
            mode: pos_hw::config::PrintMode::Raw,
            paper_width_mm: 80,
        });
    }
    config.receipt_printer_id = config.printers.first().map(|p| p.id.clone());
    st.hw.save(config).unwrap();
}

/// Waits (≤ 10 s) until what was printed to `path` holds `needle`: the print queue works on its
/// own thread, one job after the other.
pub(crate) fn wait_for_text(path: &std::path::Path, needle: &str) -> String {
    for _ in 0..200 {
        if let Ok(bytes) = std::fs::read(path) {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            if text.contains(needle) {
                return text;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("{needle} never printed to {}", path.display());
}

/// Waits (≤ 10 s) until `path` holds at least `min` bytes: the print queue works on its own thread.
pub(crate) fn wait_for_bytes(path: &std::path::Path, min: usize) -> Vec<u8> {
    for _ in 0..200 {
        if let Ok(bytes) = std::fs::read(path)
            && bytes.len() >= min
        {
            return bytes;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("nothing printed to {}", path.display());
}

// ---------------------------------------------------------------------------------------------
// M8 acceptance (Task 31): the shop of every activity template, the shop of every module, and a
// day in every pack through the commands' inner functions.
// ---------------------------------------------------------------------------------------------

/// Two IMEIs that pass the Luhn check (Task 8's).
const IMEIS: [&str; 2] = ["356938035643809", "490154203237518"];

/// The M0 first run of a shop of `activity` (the setup wizard applies its template with its
/// seeds), its owner signed in, this PC licensed with the test key.
pub(crate) fn first_run_as(activity: pos_core::company::ShopActivity) -> TestShop {
    let st = AppState::for_tests();
    let owner = super::setup::complete_first_run(
        &st,
        pos_core::setup::FirstRunInput {
            lang: Lang::Fr,
            shop_name: "Supérette El Amel".into(),
            activity,
            tax_regime: pos_core::company::TaxRegime::Ifu,
            price_mode: pos_core::company::PriceMode::Ttc,
            wilaya: Some("16".into()),
            phone: None,
            owner_name: "Amine Benali".into(),
            owner_pin: OWNER_PIN.into(),
        },
    )
    .unwrap();
    let shop = TestShop { st, owner };
    shop.licensed();
    shop
}

/// The shop of a day in every pack: the general template, licensed, the receipt printer and the
/// kitchen printer as files in `dir` (the kitchen station on its own printer), then
/// [`TestShop::stock_every_pack`].
pub(crate) fn every_module_shop(dir: &Path) -> TestShop {
    let (shop, _failures) = shop_in(dir);
    shop.licensed();
    use_file_printers(&shop.st, dir, &["caisse.bin", "cuisine.bin"]);
    shop.stock_every_pack();
    let kitchen = super::restaurant::stations(&shop.st)
        .unwrap()
        .into_iter()
        .find(|s| s.code == "CUISINE")
        .unwrap();
    let mut config = shop.st.hw.config().unwrap();
    config.kitchen = pos_hw::config::KitchenRouting {
        stations: vec![pos_hw::config::StationPrinter {
            station_id: kitchen.id,
            printer_id: "p2".into(),
            copies: 1,
            buzzer: false,
        }],
        fallback_printer_id: None,
    };
    shop.st.hw.save(config).unwrap();
    shop
}

/// The count sheet of a drawer holding `amount`: its 2 000 DA notes on one line, the rest in M1b's
/// notes and coins, then 2 DA and 1 DA coins.
pub(crate) fn counted(amount: Money) -> pos_core::cash::CloseInput {
    use pos_core::cash::closing::{CountLine, DENOMINATIONS};
    let mut left = amount.0;
    let mut counts = Vec::new();
    for denomination in DENOMINATIONS.iter().map(|d| d.0).chain([200, 100]) {
        let count = left / denomination;
        if count > 0 {
            counts.push(CountLine {
                denomination: Money(denomination),
                count,
            });
            left -= count * denomination;
        }
    }
    assert_eq!(left, 0, "{amount:?} is not a sum of notes and coins");
    pos_core::cash::CloseInput {
        counts,
        notes: None,
    }
}

/// A sale request of `cart` paid by `payments`, its receipt printed, with the packs' `extras`.
fn sale_request(
    cart: Cart,
    payments: Vec<PaymentInput>,
    extras: pos_core::sales::SaleExtras,
) -> super::sales::SaleRequest {
    super::sales::SaleRequest {
        sale: SaleInput { cart, payments },
        print_receipt: true,
        extras,
    }
}

/// A cart of `qty` of `product_id`, for `customer_id` when there is one.
fn cart_for(customer_id: Option<&str>, product_id: &str, qty: Qty) -> Cart {
    let mut cart = Cart::new(Lang::Fr, vec![CartLine::product(product_id, qty)]);
    cart.customer_id = customer_id.map(str::to_owned);
    cart
}

/// The column `key` of `table` added up; a report without that column fails the test.
pub(crate) fn column_total(table: &pos_core::table::Table, key: &str) -> i64 {
    let i = table
        .column(key)
        .unwrap_or_else(|| panic!("no column {key}"));
    table
        .rows
        .iter()
        .filter_map(|r| r.get(i).and_then(pos_core::table::Cell::as_i64))
        .sum()
}

/// The sequence numbers a list of printed numbers carries for one counter key: `TK2026-C1-000123`,
/// `RV2026-C2-000004`, `AC2026-000012` (prefix, year, optional series, number), kept when they
/// fall in `series` and, unless the counter runs across years (`year` 0), in `year`.
fn sequence_of(numbers: &[String], series: &str, year: i64) -> Vec<i64> {
    let mut out = Vec::new();
    for number in numbers {
        let rest = number.trim_start_matches(|c: char| c.is_ascii_alphabetic());
        let parts: Vec<&str> = rest.split('-').collect();
        let (found_year, found_series, seq) = match parts.as_slice() {
            [y, n] => (*y, "", *n),
            [y, s, n] => (*y, *s, *n),
            _ => panic!("unexpected number {number}"),
        };
        let found_year: i64 = found_year.parse().unwrap();
        if found_series == series && (year == 0 || found_year == year) {
            out.push(seq.parse().unwrap());
        }
    }
    out.sort_unstable();
    out
}

impl TestShop {
    /// This PC's licence: a lifetime licence of M9's test device, signed with the test key
    /// (`pos_license::testkit`, never the production key).
    pub(crate) fn licensed(&self) {
        let key = pos_license::testkit::lifetime(&pos_license::testkit::device(), now_ms());
        let status = super::license::activate(&self.st, key).unwrap();
        assert!(status.can_write, "{status:?}");
    }

    /// The live categories, as the catalogue screen lists them.
    pub(crate) fn categories(&self) -> Vec<pos_core::catalog::categories::Category> {
        super::catalog::categories_list(&self.st, false).unwrap()
    }

    /// The product whose reference is `reference` (a template's `SVC-…` service).
    pub(crate) fn product_by_reference(&self, reference: &str) -> Product {
        let owned = reference.to_owned();
        let id: String = self
            .st
            .db
            .read(move |c| {
                Ok(c.query_row(
                    "SELECT id FROM products WHERE reference = ?1",
                    [&owned],
                    |r| r.get(0),
                )?)
            })
            .unwrap_or_else(|e| panic!("no product {reference}: {e:?}"));
        super::products::get(&self.st, id).unwrap()
    }

    /// The id of the product named `name_fr` (one of [`Self::stock_every_pack`]'s goods).
    pub(crate) fn product_named(&self, name_fr: &str) -> String {
        let owned = name_fr.to_owned();
        self.st
            .db
            .read(move |c| {
                Ok(c.query_row(
                    "SELECT id FROM products WHERE name_fr = ?1",
                    [&owned],
                    |r| r.get(0),
                )?)
            })
            .unwrap_or_else(|e| panic!("no product {name_fr}: {e:?}"))
    }

    /// The id of the customer named `name`.
    pub(crate) fn customer_named(&self, name: &str) -> String {
        let owned = name.to_owned();
        self.st
            .db
            .read(move |c| {
                Ok(
                    c.query_row("SELECT id FROM customers WHERE name = ?1", [&owned], |r| {
                        r.get(0)
                    })?,
                )
            })
            .unwrap_or_else(|e| panic!("no customer {name}: {e:?}"))
    }

    /// The id of the user named `name` (made on the main PC, copied to its tills).
    pub(crate) fn user_named(&self, name: &str) -> String {
        self.st
            .db
            .read(pos_core::auth::users::list_tiles)
            .unwrap()
            .into_iter()
            .find(|t| t.display_name == name)
            .map(|t| t.id)
            .unwrap_or_else(|| panic!("no user {name}"))
    }

    /// What is on hand of the product named `name_fr`, in milli-units.
    pub(crate) fn on_hand_of(&self, name_fr: &str) -> Qty {
        let id = self.product_named(name_fr);
        self.st
            .db
            .read(move |c| pos_core::stock::on_hand(c, &id))
            .unwrap()
    }

    /// A product sold by the kilo at `price` centimes TTC (19 %), costing `cost`; its id.
    fn by_kg(&self, name_fr: &str, price: i64, cost: i64) -> String {
        let mut input = ProductInput::simple(
            name_fr,
            pos_core::catalog::UNIT_KG_ID,
            &self.rate_id(1_900),
            Money(price),
        );
        input.is_weighed = true;
        input.cost_price = Money(cost);
        self.product_with(input).id
    }

    /// A category of the catalogue, named in the three languages; its id.
    fn category(&self, fr: &str, ar: &str, en: &str) -> String {
        let input = pos_core::catalog::categories::CategoryInput {
            parent_id: None,
            name_ar: ar.into(),
            name_fr: fr.into(),
            name_en: en.into(),
            color: None,
            sort_order: 50,
            image: None,
        };
        super::catalog::categories_create(&self.st, input)
            .unwrap()
            .id
    }

    /// `product_id` tracked by IMEI, with `imeis` in stock at the shop.
    fn serials_in(&self, product_id: &str, imeis: &[&str]) {
        let (pid, numbers) = (
            product_id.to_owned(),
            imeis.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>(),
        );
        self.st
            .db
            .write(move |tx| {
                pos_core::catalog::extras::set_quiet(
                    tx,
                    &pid,
                    pos_core::catalog::extras::ProductExtrasInput {
                        track_serials: true,
                        ..Default::default()
                    },
                )?;
                for imei in &numbers {
                    pos_core::stock::serials::register(
                        tx,
                        &pid,
                        imei,
                        Some(pos_core::stock::SHOP_LOCATION_ID),
                        None,
                        now_ms(),
                    )?;
                }
                Ok(())
            })
            .unwrap();
        self.stock_in(product_id, i64::try_from(imeis.len()).unwrap());
    }

    /// The report `kind` of today's business day, as the owner reads it.
    pub(crate) fn report(&self, kind: pos_core::reports::ReportKind) -> pos_core::table::Table {
        let day = super::services::business_day(&self.st);
        super::reports::run(
            &self.st,
            pos_core::reports::ReportRequest::new(kind, &day, &day),
        )
        .unwrap_or_else(|e| panic!("{kind:?}: {e:?}"))
        .report
        .table
    }

    /// The masters of a day in every pack, on the PC that keeps them (a single PC or the main
    /// PC): every module turned on by hand (their own seeds come with them: the dining room and
    /// its tables, the kitchen and the bar, the flexy operators, the instalment price level), the
    /// mixed till, the company's mentions, the gold board, the regulated staples, a van, a
    /// Mobilis float of 20 000 DA bought outside the till, the goods of every pack in stock, their
    /// customers, an employee, and Épicerie Amine's delivery note waiting for its round.
    pub(crate) fn stock_every_pack(&self) {
        let st = &self.st;
        self.owner();
        // the categories the seeded kitchen and bar take
        let dishes = self.category("Plats", "أطباق", "Dishes");
        let tea = self.category("Boissons chaudes", "مشروبات ساخنة", "Hot drinks");
        let on = super::packs::set_modules(st, pos_core::packs::Module::ALL.to_vec()).unwrap();
        assert_eq!(on.modules, pos_core::packs::Module::ALL.to_vec());
        super::packs::set_till_mode(st, pos_core::packs::TillMode::Mixed).unwrap();
        let tables = super::restaurant::tables(st, None).unwrap();
        assert!(
            tables.iter().any(|t| t.code == "T1"),
            "the restaurant module came with its dining room"
        );
        let stations = super::restaurant::stations(st).unwrap();
        for (code, category) in [("CUISINE", &dishes), ("BAR", &tea)] {
            assert!(
                stations
                    .iter()
                    .any(|s| s.code == code && s.category_ids.contains(category)),
                "the {code} station prepares its category: {stations:?}"
            );
        }
        self.complete_company();
        super::jewellery::set_metal_price(
            st,
            pos_core::jewellery::Metal::Gold,
            750,
            Money(1_450_000),
        )
        .unwrap();
        // the nisab of the year, entered by the owner on the main PC (the software has none)
        super::settings::set(st, "zakat.nisab".into(), serde_json::json!(229_000_000)).unwrap();
        super::settings::set(
            st,
            "zakat.nisab_source".into(),
            serde_json::json!("MARW, 1447"),
        )
        .unwrap();
        super::packs::seed_regulated(st).unwrap();
        super::wholesale::van_create(
            st,
            "VAN1".into(),
            super::wholesale::VanNames {
                name_ar: "الشاحنة 1".into(),
                name_fr: "Camion 1".into(),
                name_en: "Van 1".into(),
            },
        )
        .unwrap();
        super::phones::purchase(
            st,
            pos_core::flexy::FloatPurchase {
                operator: "mobilis".into(),
                amount: Money(2_000_000),
                cost: Money(1_920_000),
                from_till: false,
                note: None,
            },
        )
        .unwrap();
        // the goods
        for (name, price, category) in [
            ("Brochettes", 90_000, &dishes),
            ("Chorba", 35_000, &dishes),
            ("Thé à la menthe", 10_000, &tea),
        ] {
            let id = self.product_in_category(name, price, category);
            self.stock_in(&id, 50);
        }
        let redmi = self.product("Redmi Note 14", 3_200_000, 1_900, None).id;
        self.serials_in(&redmi, &IMEIS);
        for (name, price, pieces) in [
            ("Écran A54", 900_000, 2),
            ("Réfrigérateur", 12_000_000, 2),
            ("Parfum", 700_000, 3),
            ("Canapé", 6_000_000, 2),
            ("Téléviseur", 6_000_000, 2),
        ] {
            let id = self.product(name, price, 1_900, None).id;
            self.stock_in(&id, pieces);
        }
        let mut labour = ProductInput::simple(
            "Main d'œuvre réparation",
            UNIT_PIECE_ID,
            &self.rate_id(1_900),
            Money(300_000),
        );
        labour.kind = products::ProductKind::Service;
        labour.track_stock = false;
        self.product_with(labour);
        let flour = self.product("Farine 25 kg", 250_000, 900, None).id;
        self.stock_in(&flour, 50);
        let ring = self.product("Bague or 18 carats", 0, 1_900, None).id;
        super::jewellery::piece_set(
            st,
            ring.clone(),
            pos_core::jewellery::PieceInput {
                metal: pos_core::jewellery::Metal::Gold,
                titre_permille: 750,
                weight_mg: 5_200,
                facon: Money(300_000),
                stones: None,
                hallmark: Some("Tête de Massinissa".into()),
                priced_by_weight: true,
            },
        )
        .unwrap();
        self.stock_in(&ring, 2);
        let tramadol = self.controlled_medicine("Tramadol 50 mg", 45_000);
        self.stock_in(&tramadol, 10);
        let carcass = self.by_kg("Carcasse d'agneau", 0, 150_000);
        self.stock_in(&carcass, 20);
        self.by_kg("Gigot", 280_000, 0);
        self.by_kg("Côtelettes", 260_000, 0);
        self.by_kg("Épaule", 220_000, 0);
        // the people
        for name in ["Walid", "Farid", "Nadia"] {
            self.customer(name);
        }
        let mut karima = pos_core::customers::CustomerInput::named("Karima");
        karima.phone = Some("0550123456".into());
        super::customers::create(st, karima).unwrap();
        let amine = self.b2b_customer("Épicerie Amine").id;
        self.find_or_create("Samira", MANAGER_ROLE_ID, MANAGER_PIN, 2_000);
        // the delivery note of the day's round, issued in the back office
        let draft = super::billing::save_draft(
            st,
            None,
            pos_core::billing::drafts::DraftInput {
                doc_type: pos_core::documents::DocType::DeliveryNote,
                cart: cart_for(Some(&amine), &flour, Qty(4_000)),
                valid_until: None,
                due_date: None,
                second_lang: None,
                draft_id: None,
            },
        )
        .unwrap();
        super::billing::validate(
            st,
            pos_core::billing::validate::ValidateInput {
                id: draft.id,
                payments: Vec::new(),
            },
        )
        .unwrap();
    }

    /// RST: a table of two at T1 (brochettes for seat 1, chorba and two teas for seat 2) sent to
    /// the kitchen and the bar, the addition in two even shares, then seat 2 split off and paid by
    /// card while seat 1 pays its 1 800 DA addition in cash. The cash seat 1 handed over: its
    /// TTC and the duty the payment charged.
    pub(crate) fn day_restaurant(&self) -> Money {
        use pos_core::restaurant::bill::{PayInput, SplitBy};
        use pos_core::restaurant::tickets::{AddLineInput, DiningMode, OpenInput};
        let st = &self.st;
        let table = super::restaurant::tables(st, None)
            .unwrap()
            .into_iter()
            .find(|t| t.code == "T1")
            .unwrap();
        let ticket = super::restaurant::ticket_open(
            st,
            OpenInput {
                mode: DiningMode::DineIn,
                table_id: Some(table.id),
                label: None,
                covers: 2,
                customer_id: None,
                delivery_address: None,
                lang: Lang::Fr,
                price_level_id: None,
            },
        )
        .unwrap();
        for (name, pieces, seat) in [
            ("Brochettes", 2, 1),
            ("Chorba", 1, 2),
            ("Thé à la menthe", 2, 2),
        ] {
            let line = AddLineInput {
                cart_line: CartLine::product(&self.product_named(name), Qty(pieces * 1_000)),
                seat: Some(seat),
                course: None,
                modifiers: Vec::new(),
            };
            super::restaurant::ticket_add_line(st, ticket.id.clone(), line).unwrap();
        }
        let sent = super::restaurant::ticket_send(st, ticket.id.clone()).unwrap();
        assert_eq!(
            sent.len(),
            2,
            "a kitchen ticket for the kitchen and one for the bar"
        );
        let bill = super::restaurant::ticket_prebill(st, ticket.id.clone(), Some(2)).unwrap();
        assert_eq!(bill.priced.totals.total_ttc, Money(235_000));
        assert_eq!(
            bill.shares,
            vec![Money(117_500); 2],
            "two even shares of the addition"
        );
        let split =
            super::restaurant::ticket_split(st, ticket.id.clone(), SplitBy::Seat { seat: 2 })
                .unwrap();
        assert_eq!(
            (split.from.total_ttc, split.to.total_ttc),
            (Money(180_000), Money(55_000))
        );
        let by_card = super::restaurant::ticket_pay_with(
            st,
            PayInput {
                ticket_id: split.to.id.clone(),
                payments: vec![PaymentInput::electronic(
                    PaymentMethod::CardCib,
                    Money(55_000),
                    Some("T1-S2"),
                )],
            },
            pos_core::sales::SaleExtras::default(),
            false,
        )
        .unwrap();
        assert_eq!(by_card.paid.sale.settlement.stamp_duty, Money::ZERO);
        let seat_one = super::restaurant::ticket_prebill(st, split.from.id.clone(), None).unwrap();
        let paid = super::restaurant::ticket_pay_with(
            st,
            PayInput {
                ticket_id: split.from.id.clone(),
                payments: vec![PaymentInput::cash(seat_one.cash_due)],
            },
            pos_core::sales::SaleExtras::default(),
            true,
        )
        .unwrap();
        let settled = &paid.paid.sale.settlement;
        let cash = Money(180_000).checked_add(settled.stamp_charged).unwrap();
        assert_eq!(
            (seat_one.cash_due, settled.change),
            (cash, Money::ZERO),
            "the addition's cash due is what the payment took"
        );
        cash
    }

    /// VRT-002: a 1 000 DA Mobilis top-up paid in cash, drawn on the operator's float. The cash
    /// it took: the top-up and its duty.
    pub(crate) fn day_flexy(&self) -> Money {
        let st = &self.st;
        let float = || {
            super::phones::balances(st)
                .unwrap()
                .into_iter()
                .find(|(operator, _)| operator == "mobilis")
                .map_or(Money::ZERO, |(_, balance)| balance)
        };
        let before = float();
        let line =
            super::phones::line(st, "mobilis".into(), "0661234567".into(), Money(100_000)).unwrap();
        let done = super::sales::create(
            st,
            sale_request(
                Cart::new(Lang::Fr, vec![line]),
                vec![PaymentInput::cash(Money(200_000))],
                pos_core::sales::SaleExtras::default(),
            ),
        )
        .unwrap();
        assert_eq!(
            float(),
            Money(before.0 - 100_000),
            "the top-up's face value left the float"
        );
        Money(100_000)
            .checked_add(done.sale.settlement.stamp_charged)
            .unwrap()
    }

    /// PHN: a phone sold with the IMEI the till offers, paid by Edahabia. No cash.
    pub(crate) fn day_serials(&self) -> Money {
        let st = &self.st;
        let redmi = self.product_named("Redmi Note 14");
        let offered = super::phones::serials_available_in(st, redmi.clone()).unwrap();
        let imei = offered[0].serial_no.clone();
        let extras = pos_core::sales::SaleExtras {
            serials: vec![pos_core::sales::extensions::SaleSerial {
                line: 0,
                serial_no: imei.clone(),
            }],
            ..Default::default()
        };
        let done = super::sales::create(
            st,
            sale_request(
                cart_for(None, &redmi, Qty(1_000)),
                vec![PaymentInput::electronic(
                    PaymentMethod::Edahabia,
                    Money(3_200_000),
                    Some("ED4471"),
                )],
                extras,
            ),
        )
        .unwrap();
        assert_eq!(done.sale.settlement.cash_tendered, Money::ZERO);
        let left = super::phones::serials_available_in(st, redmi).unwrap();
        assert!(
            left.iter().all(|s| s.serial_no != imei) && left.len() + 1 == offered.len(),
            "the IMEI sold leaves the stock"
        );
        Money::ZERO
    }

    /// RPR: Walid's phone taken in for a new screen with a 2 000 DA deposit in cash (its duty
    /// charged), the screen and the labour set, ready, then picked up: the deposit pays first and
    /// the 10 000 DA balance is paid in cash. The cash of the deposit and of the balance, each
    /// with its duty.
    pub(crate) fn day_repair(&self) -> Money {
        use pos_core::deposits::{DepositInput, OwnerType};
        use pos_core::service_tickets::{
            IntakeInput, PickupInput, ServiceItemInput, ServiceKind, ServiceStatus,
        };
        let st = &self.st;
        let ticket = super::services::intake(
            st,
            IntakeInput {
                kind: ServiceKind::Repair,
                customer_id: self.customer_named("Walid"),
                item: "Samsung A54".into(),
                brand: Some("Samsung".into()),
                model: Some("A54".into()),
                serial_no: None,
                accessories: Some("Chargeur".into()),
                condition: Some("Écran fissuré".into()),
                fault: "Remplacement écran".into(),
                estimate: Some(Money(1_200_000)),
                promised_on: None,
                technician_id: None,
                lang: Lang::Fr,
                items: Vec::new(),
            },
        )
        .unwrap();
        let deposit = super::services::deposit_take(
            st,
            DepositInput {
                owner_type: OwnerType::ServiceTicket,
                owner_id: ticket.id.clone(),
                amount: Money(200_000),
                method: PaymentMethod::Cash,
                reference: None,
                cheque_bank: None,
                cheque_date: None,
                note: None,
            },
        )
        .unwrap();
        assert!(deposit.stamp_charged.is_positive(), "the deposit's duty");
        let items = vec![
            ServiceItemInput {
                product_id: self.product_named("Écran A54"),
                qty: Qty(1_000),
                unit_price: None,
            },
            ServiceItemInput {
                product_id: self.product_named("Main d'œuvre réparation"),
                qty: Qty(1_000),
                unit_price: None,
            },
        ];
        super::services::set_items(st, ticket.id.clone(), items).unwrap();
        for status in [ServiceStatus::InProgress, ServiceStatus::Ready] {
            super::services::set_status(st, ticket.id.clone(), status, None).unwrap();
        }
        let picked = super::services::pickup(
            st,
            PickupInput {
                id: ticket.id.clone(),
                payments: vec![PaymentInput::cash(Money(1_100_000))],
                extras: pos_core::sales::SaleExtras::default(),
            },
            true,
        )
        .unwrap();
        let sale = &picked.picked.sale;
        assert_eq!(sale.document.total_ttc, Money(1_200_000));
        assert_eq!(picked.picked.ticket.status, ServiceStatus::Delivered);
        Money(200_000 + 1_000_000)
            .checked_add(deposit.stamp_charged)
            .and_then(|m| m.checked_add(sale.settlement.stamp_charged))
            .unwrap()
    }

    /// TMP: Farid's fridge (120 000 DA) on six instalments with 30 000 DA down in cash (its duty
    /// charged), then 15 000 DA of the first instalment collected in cash. The cash of the down
    /// payment and of the versement, each with its duty.
    pub(crate) fn day_instalment(&self) -> Money {
        use pos_core::instalments::{
            GuaranteeInput, GuaranteeKind, PlanPayment, QuoteInput, SignInput,
        };
        let st = &self.st;
        let farid = self.customer_named("Farid");
        let fridge = self.product_named("Réfrigérateur");
        let first_due_on = pos_core::time::business_date(st.now() + 30 * 86_400_000)
            .format("%Y-%m-%d")
            .to_string();
        let quote_input = QuoteInput {
            cart: cart_for(Some(&farid), &fridge, Qty(1_000)),
            down: Money(3_000_000),
            down_method: PaymentMethod::Cash,
            count: 6,
            first_due_on,
        };
        let quote = super::instalments::quote(st, quote_input.clone()).unwrap();
        assert!(quote.stamp_duty.is_positive(), "the down payment's duty");
        let signed = super::instalments::sign(
            st,
            SignInput {
                quote: quote_input,
                down_reference: None,
                cheque_bank: None,
                cheque_date: None,
                guarantees: vec![GuaranteeInput {
                    kind: GuaranteeKind::PaySlip,
                    holder_name: None,
                    reference: Some("Fiche de paie septembre 2026".into()),
                    phone: None,
                    note: None,
                }],
                extras: pos_core::sales::SaleExtras::default(),
                note: None,
            },
            true,
        )
        .unwrap();
        let settled = &signed.signed.sale.settlement;
        assert_eq!(
            Money(settled.cash_tendered.0 + settled.change.0),
            quote.down_to_receive,
            "the contract's ticket took the down payment and its duty"
        );
        let collected = super::instalments::collect(
            st,
            signed.signed.plan.id.clone(),
            PlanPayment {
                amount: Money(1_500_000),
                method: PaymentMethod::Cash,
                reference: None,
                cheque_bank: None,
                cheque_date: None,
                note: None,
            },
            true,
        )
        .unwrap();
        let versement = &collected.collected.entry;
        assert!(versement.receipt_number.is_some(), "its RV receipt");
        quote
            .down_to_receive
            .checked_add(Money(1_500_000))
            .and_then(|m| m.checked_add(versement.stamp_charged))
            .unwrap()
    }

    /// GFT + LOY: a 5 000 DA gift voucher sold in cash (paid in with its duty), then Karima,
    /// enrolled in the loyalty programme, buys a 7 000 DA perfume with the voucher and 2 000 DA in
    /// cash. The cash of the voucher and of the ticket, each with its duty.
    pub(crate) fn day_vouchers(&self) -> Money {
        let st = &self.st;
        let karima = self.customer_named("Karima");
        super::loyalty::enrol(st, karima.clone(), None).unwrap();
        let sold = super::loyalty::voucher_sell(
            st,
            super::loyalty::voucher_sale(500_000, PaymentMethod::Cash),
        )
        .unwrap();
        assert!(
            sold.voucher.stamp_charged.is_positive(),
            "the voucher's duty"
        );
        let voucher = PaymentInput {
            method: PaymentMethod::Voucher,
            amount: Money(500_000),
            reference: Some(sold.voucher.code.clone()),
            cheque_bank: None,
            cheque_date: None,
        };
        let done = super::sales::create(
            st,
            sale_request(
                cart_for(Some(&karima), &self.product_named("Parfum"), Qty(1_000)),
                vec![voucher, PaymentInput::cash(Money(300_000))],
                pos_core::sales::SaleExtras::default(),
            ),
        )
        .unwrap();
        let spent = super::loyalty::voucher_get(st, sold.voucher.code.clone()).unwrap();
        assert_eq!(spent.balance, Money::ZERO, "the voucher is spent");
        assert!(
            super::loyalty::account(st, karima).unwrap().balance > 0,
            "the ticket earned points"
        );
        Money(500_000 + 200_000)
            .checked_add(sold.voucher.stamp_charged)
            .and_then(|m| m.checked_add(done.sale.settlement.stamp_charged))
            .unwrap()
    }

    /// ORD-005: Nadia's 60 000 DA sofa on layaway: 15 000 DA then 45 000 DA in cash (each with
    /// its duty), then the sale paid by the order's deposits alone. The cash of the two deposits.
    pub(crate) fn day_layaway(&self) -> Money {
        use pos_core::deposits::{DepositInput, OwnerType};
        use pos_core::orders::{CustomerOrderInput, FulfilInput, OrderDeposit, OrderKind};
        let st = &self.st;
        let nadia = self.customer_named("Nadia");
        let sofa = self.product_named("Canapé");
        let cash = |amount: i64| OrderDeposit {
            amount: Money(amount),
            method: PaymentMethod::Cash,
            reference: None,
            cheque_bank: None,
            cheque_date: None,
        };
        // the deposit dialog's quote: what the first deposit charges on top
        let first_quote = super::services::deposit_quote(
            st,
            DepositInput {
                owner_type: OwnerType::CustomerOrder,
                owner_id: String::new(),
                amount: Money(1_500_000),
                method: PaymentMethod::Cash,
                reference: None,
                cheque_bank: None,
                cheque_date: None,
                note: None,
            },
        )
        .unwrap();
        let order = super::orders::create(
            st,
            CustomerOrderInput {
                draft_id: None,
                cart: cart_for(Some(&nadia), &sofa, Qty(1_000)),
                kind: OrderKind::Layaway,
                due_on: None,
                reserve: true,
                deposit: Some(cash(1_500_000)),
                note: None,
            },
        )
        .unwrap();
        assert_eq!(order.held, Money(1_500_000));
        let second = super::orders::pay_in(st, order.id.clone(), cash(4_500_000)).unwrap();
        let fulfilled = super::orders::fulfil(
            st,
            FulfilInput {
                id: order.id.clone(),
                payments: vec![PaymentInput {
                    method: PaymentMethod::Voucher,
                    amount: Money(6_000_000),
                    reference: Some(order.number.clone()),
                    cheque_bank: None,
                    cheque_date: None,
                }],
                extras: pos_core::sales::SaleExtras::default(),
            },
            true,
        )
        .unwrap()
        .fulfilled;
        assert_eq!(
            (fulfilled.order.held, fulfilled.sale.document.stamp_duty),
            (Money::ZERO, Money::ZERO),
            "the deposits pay the sale; their duty was paid with them"
        );
        first_quote
            .to_receive
            .checked_add(Money(4_500_000))
            .and_then(|m| m.checked_add(second.stamp_charged))
            .unwrap()
    }

    /// VRT-101/WHS: the day's round in the van: two sacks loaded, Épicerie Amine's delivery note
    /// (four sacks on account) delivered and its 10 000 DA collected in cash, the round closed
    /// (its versement and duty in this drawer), the van unloaded back to the shop. The cash of
    /// the collection with its duty.
    pub(crate) fn day_round(&self) -> Money {
        use pos_core::wholesale::rounds::{RoundInput, StopReport, StopStatus};
        let st = &self.st;
        let amine = self.customer_named("Épicerie Amine");
        let flour = self.product_named("Farine 25 kg");
        let bl: String = self
            .st
            .db
            .read(move |c| {
                Ok(c.query_row(
                    "SELECT id FROM documents WHERE doc_type = 'delivery_note' AND customer_id = ?1
                       AND status = 'validated'",
                    [&amine],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        let van: String = self
            .st
            .db
            .read(|c| {
                Ok(c.query_row(
                    "SELECT id FROM stock_locations WHERE code = 'VAN1'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        let round = super::wholesale::round_create(
            st,
            RoundInput {
                driver_name: "Rachid".into(),
                vehicle_plate: Some("01234-116-16".into()),
                van_location_id: Some(van),
                ..RoundInput::default()
            },
        )
        .unwrap();
        super::wholesale::round_set_stops(st, round.id.clone(), vec![bl]).unwrap();
        super::wholesale::round_load(
            st,
            round.id.clone(),
            vec![pos_core::stock::transfers::TransferLine {
                product_id: flour,
                qty: Qty(2_000),
                lot_number: None,
                serials: Vec::new(),
            }],
        )
        .unwrap();
        super::wholesale::round_start(st, round.id.clone()).unwrap();
        super::wholesale::round_report_stop(
            st,
            round.id.clone(),
            StopReport {
                seq: 1,
                status: StopStatus::Delivered,
                collected: Money(1_000_000),
                method: Some(PaymentMethod::Cash),
                reference: None,
                cheque_bank: None,
                cheque_date: None,
                note: None,
            },
        )
        .unwrap();
        let closed = super::wholesale::round_close(st, round.id.clone()).unwrap();
        super::wholesale::round_unload(st, round.id.clone()).unwrap();
        let versements = &closed.round.versements;
        assert_eq!(versements.len(), 1, "one versement for the collection");
        Money(1_000_000)
            .checked_add(versements[0].entry.stamp_charged)
            .unwrap()
    }

    /// EMP: a 3 000 DA salary advance to Samira, paid from the drawer. The cash it took out.
    pub(crate) fn day_staff_advance(&self) -> Money {
        let entry = super::staff::advance(
            &self.st,
            pos_core::staff::AdvanceInput {
                user_id: self.user_named("Samira"),
                amount: Money(300_000),
                method: PaymentMethod::Cash,
                from_till: true,
                note: Some("Aïd".into()),
            },
        )
        .unwrap();
        assert_eq!(entry.amount, Money(300_000));
        Money(300_000)
    }

    /// FIN-007: the zakat statement (against the nisab `stock_every_pack` entered), numbered
    /// `ZK…`. No money moves.
    pub(crate) fn day_zakat(&self) -> Money {
        let saved =
            super::staff::save_zakat(&self.st, super::staff::zakat_input(Money(300_000_000)))
                .unwrap();
        assert!(saved.number.starts_with("ZK"), "{}", saved.number);
        Money::ZERO
    }

    /// JWL: the 18-carat ring (5.2 g at 14 500 DA/g and 3 000 DA of making: 78 400 DA) to its
    /// identified buyer, 20 000 DA in cash and the rest by card. The cash part with its duty.
    pub(crate) fn day_jewellery(&self) -> Money {
        let st = &self.st;
        let extras = pos_core::sales::SaleExtras {
            buyer: Some(pos_core::jewellery::BuyerIdentity {
                name: "Samira Belkacem".into(),
                id_kind: pos_core::jewellery::BuyerIdKind::Cni,
                id_number: "109876543210987654".into(),
                address: "12 rue Didouche Mourad, Alger".into(),
            }),
            ..Default::default()
        };
        let done = super::sales::create(
            st,
            sale_request(
                cart_for(None, &self.product_named("Bague or 18 carats"), Qty(1_000)),
                vec![
                    PaymentInput::cash(Money(2_500_000)),
                    PaymentInput::electronic(PaymentMethod::CardCib, Money(5_840_000), Some("A9")),
                ],
                extras,
            ),
        )
        .unwrap();
        assert_eq!(done.sale.document.total_ttc, Money(7_840_000));
        Money(2_000_000)
            .checked_add(done.sale.settlement.stamp_charged)
            .unwrap()
    }

    /// PHA: a controlled medicine sold on its prescription (the register's entry). The cash with
    /// its duty.
    pub(crate) fn day_pharmacy(&self) -> Money {
        let extras = pos_core::sales::SaleExtras {
            prescriptions: vec![pos_core::pharmacy::register::PrescriptionInput {
                line: 0,
                prescriber_name: "Dr Benali".into(),
                prescriber_ref: Some("CNOM 1234".into()),
                prescription_no: Some("0457".into()),
                prescription_date: super::services::business_day(&self.st),
                patient_name: "M. Haddad".into(),
                patient_ref: None,
            }],
            ..Default::default()
        };
        let done = super::sales::create(
            &self.st,
            sale_request(
                cart_for(None, &self.product_named("Tramadol 50 mg"), Qty(1_000)),
                vec![PaymentInput::cash(Money(50_000))],
                extras,
            ),
        )
        .unwrap();
        Money(45_000)
            .checked_add(done.sale.settlement.stamp_charged)
            .unwrap()
    }

    /// BCH: the 20 kg lamb carcass cut into 6 kg of gigot, 5 kg of chops and 7 kg of shoulder
    /// (2 kg of loss), half a kilo of chops thrown away, then 1.250 kg of gigot weighed and sold
    /// in cash (3 500 DA). The cash with its duty.
    pub(crate) fn day_butcher(&self) -> Money {
        use pos_core::butcher::waste::{WasteInput, WasteLine, WasteReason};
        use pos_core::butcher::yields::{CostMethod, RunCut, RunInput};
        let st = &self.st;
        let gigot = self.product_named("Gigot");
        let chops = self.product_named("Côtelettes");
        let cuts = [(&gigot, 6_000), (&chops, 5_000)]
            .into_iter()
            .map(|(id, qty)| (id.clone(), qty))
            .chain([(self.product_named("Épaule"), 7_000)])
            .map(|(product_id, qty)| RunCut {
                product_id,
                qty: Qty(qty),
            })
            .collect();
        let run = super::butcher::run(
            st,
            RunInput {
                template_id: None,
                source_product_id: self.product_named("Carcasse d'agneau"),
                location_id: None,
                input_qty: Qty(20_000),
                input_cost: None,
                method: CostMethod::Value,
                cuts,
                note: None,
            },
        )
        .unwrap();
        assert_eq!((run.output_qty, run.loss_qty), (Qty(18_000), Qty(2_000)));
        assert_eq!(
            run.cuts.iter().map(|c| c.cost.0).sum::<i64>(),
            run.input_cost.0,
            "the carcass cost goes to the cuts to the centime"
        );
        super::butcher::record_waste(
            st,
            WasteInput {
                location_id: None,
                lines: vec![WasteLine {
                    product_id: chops,
                    qty: Qty(500),
                    reason: WasteReason::Damage,
                }],
                note: None,
            },
        )
        .unwrap();
        let done = super::sales::create(
            st,
            sale_request(
                cart_for(None, &gigot, Qty(1_250)),
                vec![PaymentInput::cash(Money(400_000))],
                pos_core::sales::SaleExtras::default(),
            ),
        )
        .unwrap();
        assert_eq!(done.sale.document.total_ttc, Money(350_000));
        Money(350_000)
            .checked_add(done.sale.settlement.stamp_charged)
            .unwrap()
    }

    /// PAY: the TV paid by a CCP debit, the seeded custom payment type riding a transfer. No cash.
    pub(crate) fn day_payment_type(&self) -> Money {
        let st = &self.st;
        let ccp = super::completion::list_types(st, false)
            .unwrap()
            .into_iter()
            .find(|p| p.code == "ccp_debit")
            .unwrap();
        let extras = pos_core::sales::SaleExtras {
            payment_types: vec![pos_core::payment_types::TenderType {
                payment: 0,
                payment_type_id: ccp.id,
            }],
            ..Default::default()
        };
        let done = super::sales::create(
            st,
            sale_request(
                cart_for(None, &self.product_named("Téléviseur"), Qty(1_000)),
                vec![PaymentInput::electronic(
                    ccp.base_method,
                    Money(6_000_000),
                    Some("CCP-0042"),
                )],
                extras,
            ),
        )
        .unwrap();
        assert_eq!(done.sale.settlement.stamp_duty, Money::ZERO);
        Money::ZERO
    }

    /// After a day of every `day_*` (on a single PC, or on a main PC once its till's day is in):
    /// every pack's ledger, the stock of every good and the packs' reports say what the day did.
    pub(crate) fn packs_reconcile(&self) {
        use pos_core::reports::ReportKind as K;
        let st = &self.st;
        self.owner();
        let sum = |sql: &'static str| self.count(sql);
        // ledgers: nothing held on a deposit, nothing left on the voucher, the float, the advance,
        // the contract and the round's customer
        assert_eq!(sum("SELECT COALESCE(SUM(amount), 0) FROM deposits"), 0);
        assert_eq!(
            sum("SELECT COALESCE(SUM(amount), 0) FROM gift_voucher_ledger"),
            0
        );
        assert!(
            super::phones::balances(st)
                .unwrap()
                .contains(&("mobilis".to_owned(), Money(1_900_000))),
            "the float: 20 000 DA bought, 1 000 DA topped up"
        );
        let samira = self.user_named("Samira");
        let owed = super::staff::list(st)
            .unwrap()
            .into_iter()
            .find(|p| p.user_id == samira)
            .map(|p| p.owed);
        assert_eq!(owed, Some(Money(300_000)));
        let plans = super::instalments::list(st, Default::default()).unwrap();
        assert_eq!(plans.len(), 1);
        let plan = &plans[0];
        assert_eq!(plan.remaining, Money(plan.financed.0 - 1_500_000));
        let farid = super::customers::get(st, self.customer_named("Farid")).unwrap();
        assert_eq!(
            farid.balance, plan.remaining,
            "Farid owes what the contract has left"
        );
        let amine = super::customers::get(st, self.customer_named("Épicerie Amine")).unwrap();
        assert_eq!(
            amine.balance,
            Money(-1_000_000),
            "a delivery note carries no debt until invoiced: the collection waits as an advance"
        );
        let karima = super::loyalty::account(st, self.customer_named("Karima")).unwrap();
        assert!(karima.balance > 0);
        // stock
        for (name, left) in [
            ("Brochettes", 48_000),
            ("Chorba", 49_000),
            ("Thé à la menthe", 48_000),
            ("Redmi Note 14", 1_000),
            ("Écran A54", 1_000),
            ("Réfrigérateur", 1_000),
            ("Parfum", 2_000),
            ("Canapé", 1_000),
            ("Téléviseur", 1_000),
            ("Farine 25 kg", 46_000),
            ("Bague or 18 carats", 1_000),
            ("Tramadol 50 mg", 9_000),
            ("Carcasse d'agneau", 0),
            ("Gigot", 4_750),
            ("Côtelettes", 4_500),
            ("Épaule", 7_000),
        ] {
            assert_eq!(self.on_hand_of(name), Qty(left), "{name} on hand");
        }
        // the legal registers and the reports of the packs
        assert_eq!(sum("SELECT COUNT(*) FROM controlled_register"), 1);
        assert_eq!(sum("SELECT COUNT(*) FROM jeweller_register"), 1);
        assert_eq!(self.report(K::ControlledRegister).rows.len(), 1);
        assert_eq!(self.report(K::JewellerRegister).rows.len(), 1);
        let tables = self.report(K::SalesByTable);
        assert_eq!(
            (
                column_total(&tables, "tickets"),
                column_total(&tables, "totalTtc")
            ),
            (2, 235_000)
        );
        assert_eq!(column_total(&self.report(K::FlexySales), "amount"), 100_000);
        assert_eq!(
            column_total(&self.report(K::ServiceTickets), "totalTtc"),
            1_200_000
        );
        assert_eq!(
            column_total(&self.report(K::DeliveryRounds), "cashCollected"),
            1_000_000
        );
        assert_eq!(column_total(&self.report(K::GiftVouchers), "balance"), 0);
        assert_eq!(
            column_total(&self.report(K::LoyaltyPoints), "balance"),
            karima.balance
        );
        assert_eq!(
            column_total(&self.report(K::StaffAdvances), "owed"),
            300_000
        );
        assert_eq!(column_total(&self.report(K::YieldRuns), "lossQty"), 2_000);
        assert_eq!(column_total(&self.report(K::Waste), "qty"), 500);
        let types = self.report(K::PaymentTypes);
        assert!(
            types
                .rows
                .iter()
                .any(|r| r.iter().any(|c| c.as_i64() == Some(6_000_000))),
            "the CCP debit is counted apart: {types:?}"
        );
    }

    /// The fiscal chains of this PC verify: every document series, the versement receipts and
    /// the Z closures.
    pub(crate) fn fiscal_chains_hold(&self) {
        self.owner();
        for chain in super::documents::verify_chain(&self.st).unwrap() {
            assert!(chain.first_broken_seq.is_none(), "{chain:?}");
        }
        let receipts = super::credit::verify_receipts(&self.st).unwrap();
        assert!(receipts.first_broken_number.is_none(), "{receipts:?}");
        let closures = super::cash::verify_closures(&self.st).unwrap();
        assert!(closures.first_broken_z.is_none(), "{closures:?}");
    }

    /// For every counter of this PC, the documents or rows carrying its series hold the numbers
    /// 1..=last without a hole; the jeweller register (numbered per year by its rows) too. The
    /// codes that no printed document carries (customer, supplier and product codes, internal
    /// EANs) are left out; any other counter without a check here fails the test.
    pub(crate) fn numbering_is_gap_free(&self) -> bool {
        let counters: Vec<(String, String, i64, i64)> = self
            .st
            .db
            .read(|c| {
                let mut stmt = c.prepare(
                    "SELECT doc_type, series, fiscal_year, last_number FROM counters
                     ORDER BY doc_type, series, fiscal_year",
                )?;
                let rows = stmt
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .unwrap();
        let texts = |sql: &'static str| -> Vec<String> {
            self.st
                .db
                .read(move |c| {
                    let mut stmt = c.prepare(sql)?;
                    let rows = stmt
                        .query_map([], |r| r.get(0))?
                        .collect::<Result<Vec<String>, _>>()?;
                    Ok(rows)
                })
                .unwrap()
        };
        let mut gap_free = true;
        for (key, series, year, last) in counters {
            let seqs: Vec<i64> = match key.as_str() {
                pos_core::numbering::Z_COUNTER => {
                    let code = series.clone();
                    self.st
                        .db
                        .read(move |c| {
                            let mut stmt = c.prepare(
                                "SELECT z.z_seq FROM cash_closures z JOIN terminals t ON t.id = z.terminal_id
                                 WHERE t.code = ?1 ORDER BY z.z_seq",
                            )?;
                            let rows = stmt
                                .query_map([&code], |r| r.get(0))?
                                .collect::<Result<Vec<i64>, _>>()?;
                            Ok(rows)
                        })
                        .unwrap()
                }
                "controlled_register" => self
                    .st
                    .db
                    .read(move |c| {
                        let mut stmt = c.prepare(
                            "SELECT seq FROM controlled_register WHERE ?1 = 0 OR year = ?1 ORDER BY seq",
                        )?;
                        let rows = stmt
                            .query_map([year], |r| r.get(0))?
                            .collect::<Result<Vec<i64>, _>>()?;
                        Ok(rows)
                    })
                    .unwrap(),
                "credit_receipt" => sequence_of(
                    &texts("SELECT receipt_number FROM customer_ledger WHERE receipt_number IS NOT NULL"),
                    &series,
                    year,
                ),
                "deposit" => sequence_of(
                    &texts("SELECT receipt_number FROM deposits WHERE receipt_number IS NOT NULL"),
                    &series,
                    year,
                ),
                "yield_run" => sequence_of(&texts("SELECT number FROM yield_runs"), &series, year),
                "service_ticket" => {
                    sequence_of(&texts("SELECT number FROM service_tickets"), &series, year)
                }
                "instalment_plan" => {
                    sequence_of(&texts("SELECT number FROM instalment_plans"), &series, year)
                }
                "gift_voucher" => sequence_of(&texts("SELECT code FROM gift_vouchers"), &series, year),
                "delivery_round" => {
                    sequence_of(&texts("SELECT number FROM delivery_rounds"), &series, year)
                }
                "zakat_statement" => {
                    sequence_of(&texts("SELECT number FROM zakat_statements"), &series, year)
                }
                "customer_code" | "supplier_code" | "product_reference" | "internal_ean" => {
                    continue;
                }
                doc if pos_core::documents::DocType::parse(doc).is_some() => {
                    let (doc, wanted) = (doc.to_owned(), series.clone());
                    let numbers: Vec<String> = self
                        .st
                        .db
                        .read(move |c| {
                            let mut stmt = c.prepare(
                                "SELECT number FROM documents
                                 WHERE doc_type = ?1 AND series = ?2 AND number IS NOT NULL",
                            )?;
                            let rows = stmt
                                .query_map([&doc, &wanted], |r| r.get(0))?
                                .collect::<Result<Vec<String>, _>>()?;
                            Ok(rows)
                        })
                        .unwrap();
                    sequence_of(&numbers, &series, year)
                }
                other => panic!("the counter {other} has no gap check in numbering_is_gap_free"),
            };
            if seqs != (1..=last).collect::<Vec<_>>() {
                eprintln!("{key} {series} {year}: {seqs:?} for 1..={last}");
                gap_free = false;
            }
        }
        let jewels: Vec<(i64, i64)> = self
            .st
            .db
            .read(|c| {
                let mut stmt =
                    c.prepare("SELECT year, seq FROM jeweller_register ORDER BY year, seq")?;
                let rows = stmt
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .unwrap();
        let mut expected: std::collections::BTreeMap<i64, i64> = std::collections::BTreeMap::new();
        for (year, seq) in jewels {
            let next = expected.entry(year).or_insert(0);
            *next += 1;
            if seq != *next {
                eprintln!("jeweller register {year}: {seq} where {next} was due");
                gap_free = false;
            }
        }
        gap_free
    }
}

//! M2 Review Focus 5: a whole month of a shop, September 2026, driven day by day through the same
//! functions the `#[tauri::command]` wrappers call, with the clock pinned to each minute of each
//! day (`AppState::clock`). At the end every ledger balances to the centime against its cached
//! balance, its report rows and an independent tally kept by the test itself: the kridi and the
//! supplier accounts, the stock and its lots, the drawer of every Z, the sales reports, the P&L
//! and the soldes declaration; the audit journal and the document chains verify, and every report
//! exports and reads back in Arabic and English.

use super::testshop::{CASHIER_PIN, MANAGER_PIN, TestShop, shop};
use super::{audit, auth, cash, company, credit, customers, documents, exports, imports};
use super::{expenses as expense_cmds, product_extras, promotions as promo_cmds, purchases};
use super::{reports, returns, sales, settings, stock, stock_m2, suppliers};
use crate::state::AppState;
use pos_core::audit::KNOWN_ACTIONS;
use pos_core::auth::Permission;
use pos_core::cash::CloseInput;
use pos_core::cash::closing::{CountLine, DENOMINATIONS};
use pos_core::company::{CompanyInput, PriceMode, TaxRegime};
use pos_core::customers::credit::CollectInput;
use pos_core::error::AppError;
use pos_core::expenses::NewExpense;
use pos_core::imports::engine::{ImportJob, ImportOptions};
use pos_core::imports::spec::{ImportEntity, ImportPolicy};
use pos_core::lang::Lang;
use pos_core::money::{Money, Qty};
use pos_core::payments::{PaymentInput, PaymentMethod, Settlement};
use pos_core::promotions::{Campaign, PromoKind, PromoScope, PromotionInput, TriggerMode};
use pos_core::purchases::PurchaseLineInput;
use pos_core::purchases::invoices::InvoiceInput;
use pos_core::purchases::orders::OrderInput;
use pos_core::purchases::receipts::ReceiptInput;
use pos_core::purchases::returns::{SupplierReturnInput, SupplierReturnLine};
use pos_core::reports::{ReportKind, ReportRequest};
use pos_core::returns::{ReturnInput, ReturnLineInput, ReturnReason};
use pos_core::sales::{Cart, CartLine, SaleInput, ValidatedSale};
use pos_core::stock::counts::{CountInput, CountMode, CountScope, Uncounted};
use pos_core::stock::transfers::{TransferInput, TransferLine};
use pos_core::suppliers::SupplierPaymentInput;
use pos_core::table::{Cell, Table};
use pos_core::tax::tva::Discount;
use pos_docs::sheet::SheetFormat;
use serde_json::json;
use std::collections::BTreeMap;
use std::time::Instant;

/// 2026-09-01 00:00 in Algiers (UTC+1), unix milliseconds.
const SEPT_1: i64 = 1_788_217_200_000;
const DAY_MS: i64 = 86_400_000;
/// The till opens every morning with 10 000 DA.
const FLOAT: i64 = 1_000_000;

/// The catalogue: reference, name, category, unit, TVA %, price TTC and cost HT (centimes).
/// The olives are sold by weight, their cost has odd centimes: a line's cost and a stock value
/// then round (half away from zero, per line).
#[rustfmt::skip]
const CATALOGUE: [(&str, &str, &str, &str, &str, i64, i64); 40] = [
    ("P01", "Sucre blanc 1 kg", "Épicerie", "pc", "9", 13_000, 9_500),
    ("P02", "Huile de table 1 L", "Épicerie", "pc", "9", 25_000, 19_000),
    ("P03", "Pâtes spaghetti 500 g", "Épicerie", "pc", "9", 9_000, 6_000),
    ("P04", "Riz 1 kg", "Épicerie", "pc", "9", 18_000, 13_000),
    ("P05", "Lentilles 1 kg", "Épicerie", "pc", "9", 26_000, 19_000),
    ("P06", "Café moulu 250 g", "Épicerie", "pc", "9", 32_000, 24_000),
    ("P07", "Thé vert 250 g", "Épicerie", "pc", "9", 15_000, 10_500),
    ("P08", "Farine 1 kg", "Épicerie", "pc", "9", 8_000, 5_500),
    ("P09", "Semoule fine 1 kg", "Épicerie", "pc", "9", 12_000, 8_500),
    ("P10", "Concentré de tomate 400 g", "Épicerie", "pc", "9", 14_000, 10_000),
    ("P11", "Sel fin 1 kg", "Épicerie", "pc", "9", 3_000, 1_800),
    ("P12", "Olives vertes", "Épicerie", "kg", "9", 69_000, 48_235),
    ("P13", "Confiture d'abricot", "Épicerie", "pc", "9", 21_000, 15_000),
    ("P14", "Biscuits secs", "Épicerie", "pc", "9", 6_000, 4_000),
    ("P15", "Chocolat au lait 100 g", "Épicerie", "pc", "9", 17_000, 12_000),
    ("P16", "Eau minérale 1,5 L", "Boissons", "pc", "19", 4_000, 2_500),
    ("P17", "Soda orange 1 L", "Boissons", "pc", "19", 11_000, 7_500),
    ("P18", "Jus d'orange 1 L", "Boissons", "pc", "19", 19_000, 13_000),
    ("P19", "Soda cola 2 L", "Boissons", "pc", "19", 16_000, 11_000),
    ("P20", "Eau gazeuse 1 L", "Boissons", "pc", "19", 6_000, 4_000),
    ("P21", "Boisson énergisante", "Boissons", "pc", "19", 23_000, 16_000),
    ("P22", "Sirop de grenadine", "Boissons", "pc", "19", 20_000, 14_000),
    ("P23", "Café soluble 100 g", "Boissons", "pc", "19", 35_000, 25_000),
    ("P24", "Lait de soja 1 L", "Boissons", "pc", "19", 28_000, 20_000),
    ("P25", "Thé glacé 1 L", "Boissons", "pc", "19", 12_000, 8_000),
    ("P26", "Lait UHT 1 L", "Crèmerie", "pc", "9", 15_000, 11_000),
    ("P27", "Yaourt nature x4", "Crèmerie", "pc", "9", 10_000, 7_000),
    ("P28", "Fromage fondu x8", "Crèmerie", "pc", "9", 17_000, 12_000),
    ("P29", "Beurre 250 g", "Crèmerie", "pc", "9", 36_000, 27_000),
    ("P30", "Crème fraîche 200 ml", "Crèmerie", "pc", "9", 14_000, 10_000),
    ("P31", "Lben 1 L", "Crèmerie", "pc", "9", 9_000, 6_500),
    ("P32", "Camembert", "Crèmerie", "pc", "9", 33_000, 24_000),
    ("P33", "Savon de Marseille", "Hygiène", "pc", "19", 7_000, 4_500),
    ("P34", "Shampooing 400 ml", "Hygiène", "pc", "19", 29_000, 20_000),
    ("P35", "Dentifrice 75 ml", "Hygiène", "pc", "19", 18_000, 12_500),
    ("P36", "Lessive 1 kg", "Hygiène", "pc", "19", 35_000, 25_000),
    ("P37", "Eau de Javel 1 L", "Hygiène", "pc", "19", 8_000, 5_000),
    ("P38", "Papier toilette x4", "Hygiène", "pc", "19", 22_000, 15_500),
    ("P39", "Liquide vaisselle 1 L", "Hygiène", "pc", "19", 15_000, 10_000),
    ("P40", "Brosse à dents", "Hygiène", "pc", "19", 11_000, 7_000),
];

/// The 30 products counted in at opening (the milk, P26, only ever comes in by lot).
const OPENING: [&str; 30] = [
    "P01", "P02", "P03", "P04", "P05", "P06", "P07", "P08", "P09", "P10", "P11", "P12", "P13",
    "P14", "P15", "P16", "P17", "P18", "P19", "P20", "P21", "P22", "P23", "P24", "P25", "P27",
    "P28", "P29", "P30", "P31",
];
/// On soldes from the 20th to the 26th (30 %).
const SOLDES: [&str; 5] = ["P01", "P02", "P03", "P04", "P05"];
/// Sold one at a time, in cash at the exact amount (never above 300 DA: no duty).
const CHEAP: [&str; 11] = [
    "P08", "P09", "P10", "P11", "P14", "P16", "P17", "P20", "P25", "P27", "P31",
];
/// Sold four at a time: above 300 DA, the droit de timbre is due.
const DUTIABLE: [&str; 10] = [
    "P13", "P15", "P18", "P19", "P21", "P22", "P24", "P28", "P29", "P06",
];
/// Karim's Tuesday coffee, Amine's Sunday tea.
const KARIM_BUYS: &str = "P23";
const AMINE_BUYS: &str = "P07";
const MILK: &str = "P26";
const OLIVES: &str = "P12";

/// Every call goes through here: a refusal fails the test with its error, so no call of the
/// month returned `AppError::Internal` (nor anything else).
#[track_caller]
fn must<T>(result: Result<T, AppError>, what: &str) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("{what}: {error:?}"),
    }
}

/// A refusal the scenario expects, with its code.
#[track_caller]
fn refused<T>(result: Result<T, AppError>, code: &str, what: &str) {
    match result {
        Ok(_) => panic!("{what}: accepted"),
        Err(error) => assert_eq!(error.code(), code, "{what}: {error:?}"),
    }
}

/// A milli-quantity times a unit price in centimes, rounded half away from zero: the rounding
/// of `LINE_COST` and `value_sql`, written again here so the reports are checked against it.
fn value_of(qty: i64, price: i64) -> i64 {
    let product = i128::from(qty) * i128::from(price);
    let half = if product < 0 { -500 } else { 500 };
    i64::try_from((product + half) / 1_000).unwrap()
}

/// Centimes as a French spreadsheet writes them (`482,35`).
fn decimal(centimes: i64) -> String {
    format!("{},{:02}", centimes / 100, centimes % 100)
}

fn da(amount: i64) -> Money {
    Money(amount * 100)
}

fn pieces(n: i64) -> Qty {
    Qty(n * 1_000)
}

/// The count sheet of `amount`: notes and coins largest first, then 1 DA coins and centimes.
fn count_sheet(amount: i64) -> Vec<CountLine> {
    let mut left = amount;
    let mut lines = Vec::new();
    for denomination in DENOMINATIONS.iter().map(|d| d.0).chain([100, 1]) {
        let count = left / denomination;
        if count > 0 {
            lines.push(CountLine {
                denomination: Money(denomination),
                count,
            });
            left -= count * denomination;
        }
    }
    lines
}

fn cart(lines: &[(&str, Qty)], customer: Option<&str>) -> Cart {
    let mut cart = Cart::new(
        Lang::Fr,
        lines
            .iter()
            .map(|(id, qty)| CartLine::product(id, *qty))
            .collect(),
    );
    cart.customer_id = customer.map(str::to_owned);
    cart
}

fn tender(method: PaymentMethod, amount: Money) -> PaymentInput {
    PaymentInput {
        method,
        ..PaymentInput::cash(amount)
    }
}

fn line(product_id: &str, qty: Qty, unit_cost: Money) -> PurchaseLineInput {
    PurchaseLineInput {
        product_id: product_id.into(),
        qty,
        unit_cost,
        pack_qty: None,
        tax_rate_id: None,
        discount: Discount::None,
        origin_line_id: None,
        lot_number: None,
        expiry_date: None,
        serials: Vec::new(),
        note: None,
    }
}

fn versement(customer_id: &str, amount: Money, method: PaymentMethod) -> CollectInput {
    CollectInput {
        customer_id: customer_id.into(),
        amount,
        method,
        reference: None,
        cheque_bank: None,
        cheque_date: None,
        allocate_to: Vec::new(),
        note: None,
    }
}

fn int(table: &Table, row: &[Cell], key: &str) -> i64 {
    let at = table
        .column(key)
        .unwrap_or_else(|| panic!("no column {key}"));
    row[at].as_i64().unwrap_or(0)
}

fn text<'a>(table: &Table, row: &'a [Cell], key: &str) -> &'a str {
    let at = table
        .column(key)
        .unwrap_or_else(|| panic!("no column {key}"));
    row[at].as_str().unwrap_or_default()
}

fn total(table: &Table, key: &str) -> i64 {
    int(table, table.totals.as_ref().unwrap(), key)
}

/// The row whose `key` column reads `value`.
fn row_of<'a>(table: &'a Table, key: &str, value: &str) -> Option<&'a Vec<Cell>> {
    table.rows.iter().find(|r| text(table, r, key) == value)
}

/// The shop, the people working in it and what the test itself counted as the month went.
struct Month {
    shop: TestShop,
    yacine: String,
    samira: String,
    day: i64,
    minute: i64,
    /// Reference → product id.
    products: BTreeMap<String, String>,
    customers: BTreeMap<&'static str, String>,
    suppliers: BTreeMap<&'static str, String>,
    /// Every ticket and return note of the month.
    sales_docs: Vec<String>,
    /// Cash expected in the open drawer.
    drawer: i64,
    /// Session id → the cash the test expects in it at the close.
    expected_by_session: BTreeMap<String, i64>,
    /// Customer id → the kridi the test expects on the account.
    kridi: BTreeMap<String, i64>,
    /// Supplier id → what the test expects the shop to owe it.
    payables: BTreeMap<String, i64>,
    /// Soldes product id → quantity sold during the soldes.
    soldes_sold: BTreeMap<String, i64>,
    /// Versements paid by cheque (not in any drawer).
    cheques: i64,
    /// Expenses of the month (drawer and transfer).
    expenses: i64,
}

impl Month {
    fn st(&self) -> &AppState {
        &self.shop.st
    }

    fn id(&self, reference: &str) -> String {
        self.products[reference].clone()
    }

    /// The next minute of the day (the shop opens at 08:00): every operation has its instant.
    fn tick(&mut self) {
        self.minute += 1;
        self.shop
            .st
            .clock
            .pin(SEPT_1 + (self.day - 1) * DAY_MS + 8 * 3_600_000 + self.minute * 60_000);
    }

    fn start_day(&mut self, day: i64) {
        self.day = day;
        self.minute = 0;
        self.tick();
        let today = pos_core::time::business_date(self.st().now());
        assert_eq!(
            today.format("%Y-%m-%d").to_string(),
            format!("2026-09-{day:02}")
        );
    }

    fn weekday(&self) -> String {
        pos_core::time::business_date(self.st().now())
            .format("%A")
            .to_string()
    }

    fn as_cashier(&self) {
        self.shop.sign_in(&self.yacine, CASHIER_PIN);
    }

    fn as_manager(&self) {
        self.shop.sign_in(&self.samira, MANAGER_PIN);
    }

    /// Samira types her PIN on the cashier's till for one operation.
    fn approve(&mut self, permission: Permission) {
        self.tick();
        let input = auth::OverrideInput {
            approver_id: self.samira.clone(),
            pin: MANAGER_PIN.into(),
            permission,
        };
        must(auth::manager_override(self.st(), &input), "approval");
    }

    fn import(
        &mut self,
        entity: ImportEntity,
        file: &str,
        csv: &str,
        mapping: &[&str],
    ) -> ImportJob {
        self.tick();
        let staged = must(imports::stage_bytes(self.st(), csv.as_bytes(), file), file);
        let preview = must(
            imports::preview(self.st(), staged.token.clone(), 0, entity, None),
            file,
        );
        let options = ImportOptions {
            entity,
            policy: ImportPolicy::CreateOnly,
            file_name: file.into(),
            header_row: preview.header_row,
            mapping: mapping.iter().map(|k| Some((*k).to_owned())).collect(),
            location_id: None,
            count_id: None,
        };
        let dry = must(
            imports::dry_run(self.st(), staged.token.clone(), 0, options.clone()),
            file,
        );
        assert_eq!(dry.fail, 0, "{file}: {:?}", dry.problems);
        let job = must(
            imports::apply(self.st(), staged.token, 0, options, &mut |_| {}),
            file,
        );
        assert_eq!(job.failed, 0, "{file}: {:?}", job.problems);
        job
    }

    fn open_till(&mut self) -> String {
        self.tick();
        let session = must(cash::open_session(self.st(), Money(FLOAT)), "open the till");
        self.drawer = FLOAT;
        session.id
    }

    /// What the cart costs in cash with `tenders` beside it (the backend's figures).
    fn quote(&self, cart: &Cart, tenders: Vec<PaymentInput>) -> Settlement {
        let input = sales::PreviewInput {
            cart: cart.clone(),
            tenders,
            store_credit: None,
        };
        must(sales::preview(self.st(), input), "preview").settlement
    }

    fn sell(&mut self, cart: Cart, payments: Vec<PaymentInput>) -> ValidatedSale {
        self.tick();
        let customer = cart.customer_id.clone();
        let request = sales::SaleRequest {
            sale: SaleInput { cart, payments },
            print_receipt: false,
            extras: Default::default(),
        };
        let sale = must(sales::create(self.st(), request), "sale").sale;
        let s = &sale.settlement;
        self.drawer += s.cash_tendered.0 - s.change.0;
        let on_credit: i64 = s
            .tenders
            .iter()
            .filter(|t| t.method == PaymentMethod::Credit)
            .map(|t| t.amount.0)
            .sum();
        if let Some(customer) = customer {
            *self.kridi.entry(customer).or_default() += on_credit;
        }
        if (20..=26).contains(&self.day) {
            for l in &sale.document.lines {
                let product = l.product_id.clone().unwrap_or_default();
                if self.soldes_ids().contains(&product) {
                    *self.soldes_sold.entry(product).or_default() += l.qty.0;
                }
            }
        }
        self.sales_docs.push(sale.document.id.clone());
        sale
    }

    fn soldes_ids(&self) -> Vec<String> {
        SOLDES.iter().map(|r| self.id(r)).collect()
    }

    /// `qty` of `reference` paid in cash at the exact amount due.
    fn sell_exact(&mut self, reference: &str, qty: Qty) -> ValidatedSale {
        let cart = cart(&[(&self.id(reference), qty)], None);
        let due = self.quote(&cart, Vec::new()).total_due;
        let sale = self.sell(cart, vec![PaymentInput::cash(due)]);
        assert_eq!(sale.settlement.change, Money::ZERO);
        sale
    }

    /// Returns one piece of `line_id` of ticket `original_id`, with Samira's approval.
    fn return_one(&mut self, original_id: &str, line_id: &str, refund: PaymentMethod) {
        self.approve(Permission::SalesReturn);
        self.tick();
        let input = ReturnInput {
            original_id: Some(original_id.into()),
            lines: vec![ReturnLineInput {
                origin_line_id: Some(line_id.into()),
                product_id: None,
                qty: pieces(1),
                unit_price: None,
                restock: true,
                reason: ReturnReason::ChangedMind,
                note: None,
            }],
            refund_method: refund,
            refund_reference: None,
            lang: Lang::Fr,
            note: None,
        };
        let done = must(
            returns::create(
                self.st(),
                returns::ReturnRequest {
                    input,
                    print_receipt: false,
                    extras: Default::default(),
                },
            ),
            "return",
        );
        let refunded = done.returned.refund.0;
        assert!(refunded > 0);
        match refund {
            PaymentMethod::Cash => self.drawer -= refunded,
            PaymentMethod::Credit => {
                let customer = done.returned.document.customer_id.clone().unwrap();
                *self.kridi.entry(customer).or_default() -= refunded;
            }
            _ => {}
        }
        self.sales_docs.push(done.returned.document.id.clone());
    }

    fn collect(&mut self, customer: &str, input: CollectInput) {
        self.tick();
        let cash_paid = input.method == PaymentMethod::Cash;
        let amount = input.amount;
        let done = must(
            credit::collect(
                self.st(),
                credit::CollectRequest {
                    input,
                    print_receipt: false,
                },
            ),
            "versement",
        );
        let quote = &done.collected.quote;
        assert_eq!(
            quote.to_receive,
            Money(amount.0 + quote.stamp_charged.0),
            "what the customer hands over"
        );
        if cash_paid {
            assert!(
                quote.stamp_charged.is_positive(),
                "cash above 300 DA pays the duty"
            );
            self.drawer += quote.to_receive.0;
        } else {
            assert_eq!(
                quote.stamp_duty,
                Money::ZERO,
                "a cheque with its details is exempt"
            );
            self.cheques += amount.0;
        }
        *self
            .kridi
            .entry(self.customers[customer].clone())
            .or_default() -= amount.0;
    }

    fn till_expense(&mut self, category_id: &str, amount: Money) {
        self.tick();
        let expense = NewExpense {
            category_id: category_id.into(),
            amount,
            method: PaymentMethod::Cash,
            from_till: true,
            reference: None,
            supplier_id: None,
            business_date: None,
            note: Some("Ticket de caisse".into()),
        };
        must(expense_cmds::record(self.st(), expense), "till expense");
        self.drawer -= amount.0;
        self.expenses += amount.0;
    }

    /// The cashier counts the drawer and closes: `short` centimes missing from the count.
    fn close_till(&mut self, session_id: &str, short: i64) {
        self.tick();
        let closed = must(
            cash::close_session(
                self.st(),
                CloseInput {
                    counts: count_sheet(self.drawer - short),
                    notes: None,
                },
            ),
            "close the till",
        );
        assert_eq!(closed.z.variance, Money(-short), "day {}", self.day);
        self.expected_by_session
            .insert(session_id.to_owned(), self.drawer);
    }

    fn report(&self, kind: ReportKind, edit: impl FnOnce(&mut ReportRequest)) -> Table {
        let mut request = ReportRequest::new(kind, "2026-09-01", "2026-09-30");
        edit(&mut request);
        must(reports::run(self.st(), request), kind.as_str())
            .report
            .table
    }
}

fn catalogue_csv() -> String {
    let mut csv =
        String::from("Référence;Désignation;Famille;Unité;TVA;Prix de vente;Prix d'achat\r\n");
    for (reference, name, category, unit, rate, price, cost) in CATALOGUE {
        let (price, cost) = (decimal(price), decimal(cost));
        csv.push_str(&format!(
            "{reference};{name};{category};{unit};{rate};{price};{cost}\r\n"
        ));
    }
    csv
}

fn opening_stock_csv() -> String {
    let mut csv = String::from("Référence;Quantité;Coût unitaire\r\n");
    for reference in OPENING {
        let cost = decimal(CATALOGUE.iter().find(|p| p.0 == reference).unwrap().6);
        csv.push_str(&format!("{reference};60;{cost}\r\n"));
    }
    csv
}

fn ids_by(st: &AppState, sql: &'static str) -> BTreeMap<String, String> {
    st.db
        .read(move |c| {
            let mut stmt = c.prepare(sql)?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<BTreeMap<String, String>, _>>()?;
            Ok(rows)
        })
        .unwrap()
}

/// Day 1: the owner sets the shop up (réel, TTC, the duty charged to the customer, the wali's
/// soldes week) and imports the catalogue, the customers and their notebook, the suppliers and
/// their balances, and the opening stock.
fn set_up() -> Month {
    let shop = shop();
    let yacine = shop.cashier().user_id;
    let samira = shop.manager().user_id;
    shop.owner();
    let mut month = Month {
        shop,
        yacine,
        samira,
        day: 1,
        minute: 0,
        products: BTreeMap::new(),
        customers: BTreeMap::new(),
        suppliers: BTreeMap::new(),
        sales_docs: Vec::new(),
        drawer: 0,
        expected_by_session: BTreeMap::new(),
        kridi: BTreeMap::new(),
        payables: BTreeMap::new(),
        soldes_sold: BTreeMap::new(),
        cheques: 0,
        expenses: 0,
    };
    let m = &mut month;
    m.start_day(1);
    let mut input = CompanyInput::named(Lang::Fr, "Supérette El Amel");
    input.name_ar = "مغازة الأمل".into();
    input.tax_regime = TaxRegime::Reel;
    input.price_mode = PriceMode::Ttc;
    input.nif = Some("000116001234567".into());
    input.wilaya = Some("16".into());
    must(company::update(m.st(), input), "company");
    must(
        settings::set(m.st(), "stamp_duty.mode".into(), json!("charge_customer")),
        "stamp duty",
    );
    must(
        settings::set(
            m.st(),
            "promotions.soldes_windows".into(),
            json!([{ "from": "2026-09-20", "to": "2026-09-26" }]),
        ),
        "soldes window",
    );

    let job = m.import(
        ImportEntity::Products,
        "articles.csv",
        &catalogue_csv(),
        &[
            "reference",
            "nameFr",
            "category",
            "unit",
            "taxRate",
            "sellPrice",
            "costPrice",
        ],
    );
    assert_eq!(job.created, 40);
    m.products = ids_by(
        m.st(),
        // Not the regulated staples the General template seeded at the first run (M8).
        "SELECT reference, id FROM products WHERE reference IS NOT NULL AND reference NOT LIKE 'REG-%'",
    );
    assert_eq!(m.products.len(), 40);
    let lots = pos_core::catalog::extras::ProductExtrasInput {
        track_lots: true,
        ..Default::default()
    };
    m.tick();
    must(
        product_extras::set_extras(m.st(), m.id(MILK), lots),
        "milk by lot",
    );

    let job = m.import(
        ImportEntity::Customers,
        "clients.csv",
        // a reseller is identified (NIF, RC, AI and address, CIDTA 183 ter)
        "Code;Nom;Type;Téléphone;Plafond;NIF;RC;AI;Adresse\r\n\
         C001;Karim Benali;Particulier;0661234567;;;;;\r\n\
         C002;Nadia Hamdi;Particulier;0770112233;;;;;\r\n\
         C003;Quincaillerie Amine;Revendeur;0555443322;10000,00;001216098765432;16/00-1234567B21;16012345678;\
         12 rue Hassiba Ben Bouali, Alger\r\n",
        &[
            "code",
            "name",
            "customerType",
            "phone",
            "creditLimit",
            "nif",
            "rc",
            "ai",
            "address",
        ],
    );
    assert_eq!(job.created, 3);
    let customers = ids_by(m.st(), "SELECT code, id FROM customers");
    for (name, code) in [("karim", "C001"), ("nadia", "C002"), ("amine", "C003")] {
        m.customers.insert(name, customers[code].clone());
    }
    m.import(
        ImportEntity::CustomerBalances,
        "carnet.csv",
        "Code;Montant;Note\r\nC001;1500,00;Carnet\r\nC003;8000,00;Carnet\r\n",
        &["code", "amount", "note"],
    );
    m.kridi.insert(m.customers["karim"].clone(), 150_000);
    m.kridi.insert(m.customers["nadia"].clone(), 0);
    m.kridi.insert(m.customers["amine"].clone(), 800_000);

    m.import(
        ImportEntity::Suppliers,
        "fournisseurs.csv",
        "Code;Raison sociale;Délai\r\nF001;Distribution El Baraka;30\r\nF002;Grossiste Tassili;15\r\n",
        &["code", "name", "paymentTermsDays"],
    );
    let suppliers = ids_by(m.st(), "SELECT code, id FROM suppliers");
    m.suppliers.insert("baraka", suppliers["F001"].clone());
    m.suppliers.insert("tassili", suppliers["F002"].clone());
    m.payables.insert(suppliers["F001"].clone(), 2_500_000);
    m.payables.insert(suppliers["F002"].clone(), 1_200_000);
    m.import(
        ImportEntity::SupplierBalances,
        "dettes.csv",
        "Code;Montant\r\nF001;25000,00\r\nF002;12000,00\r\n",
        &["code", "amount"],
    );
    let job = m.import(
        ImportEntity::OpeningStock,
        "stock.csv",
        &opening_stock_csv(),
        &["reference", "qty", "unitCost"],
    );
    assert_eq!(job.total, 30);
    month
}

const TRANSPORT: &str = "00000000-0000-7000-8000-000000000505";
const SUPPLIES: &str = "00000000-0000-7000-8000-000000000506";
const ELECTRICITY: &str = "00000000-0000-7000-8000-000000000502";

/// What the month produced that the checks need.
struct Documents {
    order_id: String,
    receipts: Vec<String>,
    invoice_entry: Option<String>,
    invoice_ttc: i64,
    /// The value of the goods sent back to the supplier, credited on the account.
    supplier_return: i64,
    promotion_id: String,
    count_id: String,
    count_losses: i64,
    /// Karim's first Tuesday: the ticket and its line, returned to his account on Thursday.
    karim_ticket: Option<(String, String)>,
}

/// The back office of the morning, as Samira.
fn back_office(m: &mut Month, docs: &mut Documents) {
    let baraka = m.suppliers["baraka"].clone();
    match m.day {
        3 => {
            m.as_manager();
            m.tick();
            let order = OrderInput {
                supplier_id: baraka.clone(),
                lines: vec![
                    line(&m.id(MILK), pieces(96), Money(11_140)),
                    line(&m.id("P01"), pieces(24), Money(9_735)),
                    line(&m.id("P16"), pieces(48), Money(2_615)),
                ],
                supplier_ref: None,
                note: None,
            };
            let draft = must(purchases::save_order(m.st(), None, order), "order");
            m.tick();
            let order = must(purchases::validate_order(m.st(), draft.id), "order sent");
            docs.order_id = order.id;
        }
        6 | 10 => {
            m.as_manager();
            let remaining = must(
                purchases::remaining(m.st(), docs.order_id.clone()),
                "remaining",
            );
            let origin = |product: &str| {
                remaining
                    .iter()
                    .find(|l| l.line.product_id.as_deref() == Some(product))
                    .unwrap()
                    .line
                    .id
                    .clone()
            };
            let (lot, expiry, other, cost) = if m.day == 6 {
                ("L2609A", "2026-10-20", "P01", Money(9_735))
            } else {
                ("L2609B", "2026-10-27", "P16", Money(2_615))
            };
            let mut milk = line(&m.id(MILK), pieces(48), Money(11_140));
            milk.origin_line_id = Some(origin(&m.id(MILK)));
            milk.lot_number = Some(lot.into());
            milk.expiry_date = Some(expiry.into());
            let n = if other == "P01" { 24 } else { 48 };
            let mut rest = line(&m.id(other), pieces(n), cost);
            rest.origin_line_id = Some(origin(&m.id(other)));
            m.tick();
            let done = must(
                purchases::receive(
                    m.st(),
                    ReceiptInput {
                        supplier_id: baraka,
                        order_id: Some(docs.order_id.clone()),
                        location_id: None,
                        lines: vec![milk, rest],
                        supplier_ref: Some(format!("BL-{lot}")),
                        note: None,
                        invoice: None,
                    },
                ),
                "goods received",
            );
            docs.receipts.push(done.receipt.id);
        }
        11 => {
            m.as_manager();
            m.tick();
            let done = must(
                purchases::record_invoice(
                    m.st(),
                    InvoiceInput {
                        supplier_id: baraka,
                        supplier_number: "FA-2026-0911".into(),
                        receipt_ids: docs.receipts.clone(),
                        lines: Vec::new(),
                        note: None,
                        payment: None,
                    },
                ),
                "supplier invoice",
            );
            docs.invoice_ttc = done.invoice.total_ttc.0;
            *m.payables.get_mut(&m.suppliers["baraka"]).unwrap() += done.entry.amount.0;
            assert_eq!(done.entry.amount, done.invoice.total_ttc);
            docs.invoice_entry = Some(done.entry.id);
        }
        12 => {
            m.as_manager();
            m.tick();
            let done = must(
                purchases::return_to_supplier(
                    m.st(),
                    SupplierReturnInput {
                        supplier_id: baraka,
                        location_id: None,
                        lines: vec![SupplierReturnLine {
                            product_id: m.id("P16"),
                            qty: pieces(6),
                            unit_cost: None,
                            lot_number: None,
                            serials: Vec::new(),
                            note: Some("Bouteilles fendues".into()),
                        }],
                        credit_supplier: true,
                        note: None,
                    },
                ),
                "supplier return",
            );
            let credited = done.entry.unwrap().amount.0;
            assert!(credited < 0, "the supplier credits the goods sent back");
            docs.supplier_return = -credited;
            *m.payables.get_mut(&m.suppliers["baraka"]).unwrap() += credited;
        }
        13 => {
            m.as_manager();
            let places = must(stock::locations(m.st()), "locations");
            let shelf = places.iter().find(|l| l.is_default).unwrap().id.clone();
            let damaged = places
                .iter()
                .find(|l| l.kind == pos_core::stock::LocationKind::Damaged)
                .unwrap()
                .id
                .clone();
            m.tick();
            must(
                stock_m2::transfer(
                    m.st(),
                    TransferInput {
                        from_location_id: shelf,
                        to_location_id: damaged,
                        lines: vec![TransferLine {
                            product_id: m.id("P19"),
                            qty: pieces(1),
                            lot_number: None,
                            serials: Vec::new(),
                        }],
                        driver_name: None,
                        vehicle: None,
                        note: Some("Bouteille cabossée".into()),
                    },
                ),
                "to the damaged goods",
            );
        }
        15 => {
            m.as_manager();
            m.tick();
            let bill = NewExpense {
                category_id: ELECTRICITY.into(),
                amount: da(4_500),
                method: PaymentMethod::Transfer,
                from_till: false,
                reference: Some("VIR-SONELGAZ-0915".into()),
                supplier_id: None,
                business_date: None,
                note: None,
            };
            must(expense_cmds::record(m.st(), bill), "electricity");
            m.expenses += da(4_500).0;
        }
        19 => {
            m.as_manager();
            m.tick();
            let soldes = PromotionInput {
                name_ar: "تخفيضات نهاية الصيف".into(),
                name_fr: "Soldes de fin d'été".into(),
                name_en: "End of summer sale".into(),
                kind: PromoKind::Percent,
                campaign: Campaign::Solde,
                trigger_mode: TriggerMode::Auto,
                scope: PromoScope::Products {
                    ids: m.soldes_ids(),
                },
                value: 3_000,
                buy_qty: None,
                get_qty: None,
                min_qty: Qty::ZERO,
                priority: 0,
                starts_on: Some("2026-09-20".into()),
                ends_on: Some("2026-09-26".into()),
                active: true,
            };
            let promotion = must(promo_cmds::create(m.st(), soldes), "soldes");
            assert!(!promotion.outside_soldes_window, "inside the wali's week");
            docs.promotion_id = promotion.id;
        }
        28 => {
            m.as_manager();
            let boissons = ids_by(m.st(), "SELECT name_fr, id FROM categories")["Boissons"].clone();
            m.tick();
            let count = must(
                stock_m2::start_count(
                    m.st(),
                    CountInput {
                        location_id: None,
                        scope: CountScope::Category,
                        category_id: Some(boissons),
                        product_ids: Vec::new(),
                        blind: false,
                        note: None,
                    },
                ),
                "count",
            );
            let lines = must(
                stock_m2::count_lines(m.st(), count.id.clone(), false),
                "count lines",
            );
            assert_eq!(lines.len(), 10, "the ten drinks");
            let (soda, sparkling) = (m.id("P17"), m.id("P20"));
            for l in lines {
                let expected = l.expected_qty.unwrap().0;
                let counted = if l.product_id == soda {
                    expected - 2_000
                } else if l.product_id == sparkling {
                    expected + 1_000
                } else {
                    expected
                };
                docs.count_losses += value_of(counted - expected, l.unit_cost.unwrap().0);
                m.tick();
                must(
                    stock_m2::set_counted(
                        m.st(),
                        count.id.clone(),
                        l.product_id,
                        Qty(counted),
                        CountMode::Set,
                    ),
                    "counted",
                );
            }
            m.tick();
            let closed = must(
                stock_m2::close_count(m.st(), count.id.clone(), Uncounted::Ignore),
                "count closed",
            );
            assert_eq!(closed.adjusted, 2, "two differences");
            assert_eq!(closed.variance_value, Money(docs.count_losses));
            docs.count_id = count.id;
        }
        _ => {}
    }
}

/// Days 2–29: Yacine opens, sells, takes the kridi versements and closes; Samira approves.
fn trading_day(m: &mut Month, docs: &mut Documents) {
    let day = m.day;
    let weekday = m.weekday();
    m.as_cashier();
    let session_id = m.open_till();
    let at = |list: &[&'static str], k: i64| list[usize::try_from(k).unwrap() % list.len()];

    m.sell_exact(at(&CHEAP, day), pieces(1));
    // with change: a 500 DA note
    let with_change = cart(&[(&m.id(at(&CHEAP, day + 3)), pieces(1))], None);
    let due = m.quote(&with_change, Vec::new()).total_due;
    let sale = m.sell(with_change, vec![PaymentInput::cash(da(500))]);
    assert_eq!(sale.settlement.change, Money(da(500).0 - due.0));
    // above 300 DA in cash: the droit de timbre is added to the amount due
    let dutiable = m.sell_exact(at(&DUTIABLE, day), pieces(4));
    assert!(dutiable.document.stamp_duty.is_positive(), "day {day}");
    m.sell_exact(at(&SOLDES, day), pieces(1));
    // olives by weight: 401 g to 752 g
    m.sell_exact(OLIVES, Qty(375 + 13 * day));
    if day >= 7 {
        m.sell_exact(MILK, pieces(1));
    } else {
        m.sell_exact(at(&CHEAP, day + 9), pieces(1));
    }
    // by card: no duty
    let by_card = cart(&[(&m.id(at(&CHEAP, day + 5)), pieces(2))], None);
    let due = m.quote(&by_card, Vec::new()).total_ttc;
    let card = m.sell(
        by_card,
        vec![PaymentInput::electronic(
            PaymentMethod::CardCib,
            due,
            Some(&format!("AUT{day:02}")),
        )],
    );
    assert_eq!(card.settlement.stamp_duty, Money::ZERO);

    if day == 5 {
        m.till_expense(TRANSPORT, da(800));
    }
    if day == 20 {
        m.till_expense(SUPPLIES, da(1_200));
    }
    match weekday.as_str() {
        "Tuesday" => {
            // Karim: half on his kridi, the rest in cash with the duty on the cash part
            let karim = m.customers["karim"].clone();
            let basket = cart(&[(&m.id(KARIM_BUYS), pieces(4))], Some(&karim));
            let half = Money(m.quote(&basket, Vec::new()).total_ttc.0 / 2);
            let credit = tender(PaymentMethod::Credit, half);
            let cash_due = m.quote(&basket, vec![credit.clone()]).cash_due;
            let sale = m.sell(basket, vec![credit, PaymentInput::cash(cash_due)]);
            assert!(sale.settlement.stamp_duty.is_positive());
            if day == 8 {
                docs.karim_ticket =
                    Some((sale.document.id.clone(), sale.document.lines[0].id.clone()));
            }
        }
        "Thursday" => {
            if day == 10 {
                let (ticket, line) = docs.karim_ticket.clone().unwrap();
                m.return_one(&ticket, &line, PaymentMethod::Credit);
            } else {
                m.return_one(
                    &dutiable.document.id,
                    &dutiable.document.lines[0].id,
                    PaymentMethod::Cash,
                );
            }
        }
        "Saturday" => {
            let (who, amount, method) = match day {
                5 => ("karim", da(1_000), PaymentMethod::Cash),
                12 => ("karim", da(700), PaymentMethod::Cash),
                19 => ("amine", da(3_000), PaymentMethod::Cheque),
                _ => ("amine", da(2_000), PaymentMethod::Cash),
            };
            let mut input = versement(&m.customers[who], amount, method);
            if method == PaymentMethod::Cheque {
                input.reference = Some("0012345".into());
                input.cheque_bank = Some("CPA".into());
                input.cheque_date = Some("2026-09-19".into());
            }
            m.collect(who, input);
        }
        "Sunday" => {
            // Amine's hardware shop buys on its account; above the limit a manager approves
            let amine = m.customers["amine"].clone();
            let order = cart(&[(&m.id(AMINE_BUYS), pieces(10))], Some(&amine));
            let on_account = vec![tender(PaymentMethod::Credit, da(1_500))];
            if day == 13 {
                m.tick();
                refused(
                    sales::create(
                        m.st(),
                        sales::SaleRequest {
                            sale: SaleInput {
                                cart: order.clone(),
                                payments: on_account.clone(),
                            },
                            print_receipt: false,
                            extras: Default::default(),
                        },
                    ),
                    "credit_limit_exceeded",
                    "11 000 DA against a 10 000 DA limit",
                );
                m.approve(Permission::CreditLimitOverride);
            }
            m.sell(order, on_account);
        }
        _ => {}
    }
    if day == 25 {
        // the supplier invoice, half from the drawer, the rest by transfer
        m.as_manager();
        let entry = docs.invoice_entry.clone().unwrap();
        let from_drawer = docs.invoice_ttc / 200 * 100;
        for (amount, method, from_till) in [
            (from_drawer, PaymentMethod::Cash, true),
            (
                docs.invoice_ttc - from_drawer,
                PaymentMethod::Transfer,
                false,
            ),
        ] {
            m.tick();
            let payment = SupplierPaymentInput {
                supplier_id: m.suppliers["baraka"].clone(),
                amount: Money(amount),
                method,
                reference: (!from_till).then(|| "VIR-0925".to_owned()),
                cheque_bank: None,
                cheque_date: None,
                from_till,
                allocate_to: vec![entry.clone()],
                note: None,
            };
            must(suppliers::pay(m.st(), payment), "supplier paid");
        }
        *m.payables.get_mut(&m.suppliers["baraka"]).unwrap() -= docs.invoice_ttc;
        m.drawer -= from_drawer;
        m.as_cashier();
    }
    // one evening the drawer is 5 DA short
    let short = if day == 16 { 500 } else { 0 };
    m.close_till(&session_id, short);
}

impl Documents {
    fn new() -> Documents {
        Documents {
            order_id: String::new(),
            receipts: Vec::new(),
            invoice_entry: None,
            invoice_ttc: 0,
            supplier_return: 0,
            promotion_id: String::new(),
            count_id: String::new(),
            count_losses: 0,
            karim_ticket: None,
        }
    }
}

/// The non-zero rows `sql` returns as `(key, key) → qty`.
fn grouped(st: &AppState, sql: &'static str) -> BTreeMap<(String, String), i64> {
    st.db
        .read(move |c| {
            let mut stmt = c.prepare(sql)?;
            let rows = stmt
                .query_map([], |r| Ok(((r.get(0)?, r.get(1)?), r.get(2)?)))?
                .collect::<Result<Vec<((String, String), i64)>, _>>()?;
            Ok(rows.into_iter().filter(|(_, qty)| *qty != 0).collect())
        })
        .unwrap()
}

fn ledger_sum(st: &AppState, sql: &'static str, party: &str) -> i64 {
    let party = party.to_owned();
    st.db
        .read(move |c| Ok(c.query_row(sql, [party], |r| r.get(0))?))
        .unwrap()
}

/// Every Z of the month against the drawer the test counted; their totals against the reports.
fn check_the_drawers(m: &Month) {
    let sessions: Vec<_> = must(cash::sessions(m.st(), 200), "sessions")
        .into_iter()
        .filter(|s| s.business_date.starts_with("2026-09"))
        .collect();
    assert_eq!(sessions.len(), 28, "one Z a day, from the 2nd to the 29th");
    let (mut net_ttc, mut stamp_charged, mut credit_payments) = (0_i64, 0_i64, 0_i64);
    let (mut cash_in, mut variance) = (0_i64, 0_i64);
    let mut methods: BTreeMap<String, i64> = BTreeMap::new();
    for session in sessions {
        let day = session.business_date.clone();
        let z = must(cash::z_report(m.st(), session.id.clone()), "Z");
        let expected = z.summary.expected_cash.expect("the stored Z keeps it").0;
        assert_eq!(
            z.counted_cash.0 - z.variance.0,
            expected,
            "drawer {day}: counted − variance = expected"
        );
        assert_eq!(
            expected, m.expected_by_session[&session.id],
            "drawer {day}: float + cash sales − change − refunds + versements and their duty − \
             till expenses − the supplier paid from the drawer"
        );
        assert_eq!(session.expected_cash, Some(Money(expected)), "drawer {day}");
        net_ttc += z.summary.net_ttc.0;
        stamp_charged += z.summary.stamp_charged.0;
        credit_payments += z.summary.credit_payments.0;
        cash_in += z.summary.cash_sales.0 + z.summary.cash_refunds.0 + z.summary.credit_payments.0;
        variance += z.variance.0;
        for method in &z.summary.by_method {
            let tender = match method.method {
                PaymentMethod::Change => PaymentMethod::Cash,
                other => other,
            };
            *methods.entry(tender.as_str().to_owned()).or_default() += method.net.0;
        }
    }
    assert_eq!(
        must(cash::verify_closures(m.st()), "closures").first_broken_z,
        None,
        "the chain of the Z closures"
    );

    let sessions = m.report(ReportKind::CashSessions, |_| {});
    assert_eq!(
        (total(&sessions, "variance"), variance),
        (-500, -500),
        "the evening the drawer was 5 DA short"
    );
    let daily = m.report(ReportKind::DailyByMethod, |_| {});
    let received = |method: &str| -> i64 {
        daily
            .rows
            .iter()
            .filter(|r| text(&daily, r, "method") == method)
            .map(|r| int(&daily, r, "total"))
            .sum()
    };
    assert_eq!(
        received("cash"),
        cash_in,
        "cash received per day (sales net of change and refunds, versements and their duty) = the Z"
    );
    let summary = m.report(ReportKind::SalesSummary, |_| {});
    assert_eq!(
        total(&summary, "netTtc"),
        net_ttc,
        "Σ Z net_ttc = sales_summary netTtc"
    );
    assert_eq!(
        total(&summary, "stampDuty"),
        stamp_charged,
        "Σ Z duty charged = sales_summary stampDuty (the mixed tenders' cash part included)"
    );
    let by_method = m.report(ReportKind::SalesByMethod, |_| {});
    let reported: BTreeMap<String, i64> = by_method
        .rows
        .iter()
        .map(|r| {
            (
                text(&by_method, r, "method").to_owned(),
                int(&by_method, r, "net"),
            )
        })
        .collect();
    assert_eq!(reported, methods, "Z by_method nets = sales_by_method nets");
    assert_eq!(
        received("card_cib"),
        methods["card_cib"],
        "card per day = card on the Z"
    );
    assert_eq!(
        total(&summary, "onCredit"),
        methods["credit"],
        "sales_summary onCredit = the kridi tendered, net of returns to the account"
    );
    let activity = m.report(ReportKind::CreditActivity, |_| {});
    assert_eq!(
        credit_payments + m.cheques,
        total(&activity, "collected") + total(&activity, "stampDuty"),
        "Σ Z credit_payments (cash versements and their duty) + cheques = collected + duty"
    );
}

/// The kridi and the payables: the cached balance, the ledger, the report rows and the ageing
/// all say what the test counted.
fn check_the_accounts(m: &Month, docs: &Documents) {
    let balances = m.report(ReportKind::CustomerBalances, |_| {});
    let aging = m.report(ReportKind::CustomerAging, |_| {});
    let aged = |table: &Table, code: &str| -> [i64; 2] {
        row_of(table, "code", code).map_or([0, 0], |r| {
            let buckets: i64 = ["days0To30", "days31To60", "days61To90", "over90"]
                .iter()
                .map(|k| int(table, r, k))
                .sum();
            [int(table, r, "balance"), buckets - int(table, r, "advance")]
        })
    };
    for (name, id) in &m.customers {
        let customer = must(customers::get(m.st(), id.clone()), name);
        let ledger = ledger_sum(
            m.st(),
            "SELECT COALESCE(SUM(amount), 0) FROM customer_ledger WHERE customer_id = ?1",
            id,
        );
        let listed =
            row_of(&balances, "code", &customer.code).map_or(0, |r| int(&balances, r, "balance"));
        let [aged_balance, aged_buckets] = aged(&aging, &customer.code);
        let expected = m.kridi[id];
        assert_eq!(
            [
                customer.balance.0,
                ledger,
                listed,
                aged_balance,
                aged_buckets
            ],
            [expected; 5],
            "{name}: cached balance = ledger = customer_balances = ageing = the notebook"
        );
    }
    assert_eq!(
        must(credit::rebuild_balances(m.st()), "rebuild the kridi").corrected,
        0,
        "credit_rebuild_balances changes nothing"
    );

    let payables = m.report(ReportKind::SupplierBalances, |_| {});
    for (name, id) in &m.suppliers {
        let supplier = must(suppliers::get(m.st(), id.clone()), name);
        let ledger = ledger_sum(
            m.st(),
            "SELECT COALESCE(SUM(amount), 0) FROM supplier_ledger WHERE supplier_id = ?1",
            id,
        );
        let [listed, buckets] = aged(&payables, &supplier.code);
        let expected = m.payables[id];
        assert_eq!(
            [supplier.balance.0, ledger, listed, buckets],
            [expected; 4],
            "{name}: cached balance = ledger = supplier_balances = ageing = the invoices"
        );
    }
    let actor = m.shop.owner.actor();
    let rebuilt = m
        .st()
        .db
        .write(move |tx| {
            pos_core::ledger::rebuild_balances(tx, pos_core::ledger::Side::Supplier, &actor)
        })
        .unwrap();
    assert_eq!(rebuilt.corrected, 0, "the payables rebuild changes nothing");
    let purchases = m.report(ReportKind::PurchasesBySupplier, |_| {});
    let baraka = row_of(&purchases, "code", "F001").unwrap();
    assert_eq!(
        ["receipts", "purchases", "returns", "payments", "balance"]
            .map(|k| int(&purchases, baraka, k)),
        [
            2,
            docs.invoice_ttc,
            docs.supplier_return,
            docs.invoice_ttc,
            m.payables[&m.suppliers["baraka"]]
        ],
        "the month with the distributor: two receipts, one invoice paid, one return"
    );
    let expenses = m.report(ReportKind::Expenses, |_| {});
    assert_eq!(
        [total(&expenses, "amount"), total(&expenses, "fromTill")],
        [m.expenses, da(800 + 1_200).0],
        "the expenses report: the two till expenses and the bill paid by transfer"
    );
}

/// Levels, lots and the valuation against the stock ledger.
fn check_the_stock(m: &Month) {
    let levels = grouped(
        m.st(),
        "SELECT product_id, location_id, qty FROM stock_levels",
    );
    let moved = grouped(
        m.st(),
        "SELECT product_id, location_id, SUM(qty_delta) FROM stock_movements GROUP BY 1, 2",
    );
    assert_eq!(
        levels, moved,
        "stock_levels = the sum of each product's movements"
    );
    let lots = grouped(m.st(), "SELECT lot_id, location_id, qty FROM lot_levels");
    let lotted = grouped(
        m.st(),
        "SELECT lot_id, location_id, SUM(qty_delta) FROM stock_movements
         WHERE lot_id IS NOT NULL GROUP BY 1, 2",
    );
    assert_eq!(lots, lotted, "lot levels = the lotted movements");
    let milk = m.id(MILK);
    let milk_level: i64 = levels
        .iter()
        .filter(|((p, _), _)| *p == milk)
        .map(|(_, q)| q)
        .sum();
    let milk_lots: i64 = must(stock_m2::lots_of(m.st(), milk.clone()), "milk lots")
        .iter()
        .map(|l| l.qty.0)
        .sum();
    assert_eq!(milk_lots, milk_level, "all the milk came in by lot");
    assert_eq!(
        milk_level,
        96_000 - 23_000,
        "96 received, one sold a day from the 7th"
    );
    assert_eq!(
        must(stock::rebuild_levels(m.st()), "rebuild the levels").corrected,
        0,
        "stock::rebuild_levels changes nothing"
    );

    let held: Vec<(i64, i64)> = m
        .st()
        .db
        .read(|c| {
            let mut stmt = c.prepare(
                "SELECT sl.qty, p.cost_price FROM stock_levels sl
                 JOIN products p ON p.id = sl.product_id
                 JOIN stock_locations loc ON loc.id = sl.location_id
                 WHERE loc.kind = 'storage' AND p.archived_at IS NULL AND p.track_stock = 1
                   AND sl.qty <> 0",
            )?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .unwrap();
    let at_cost: i64 = held.iter().map(|(qty, cost)| value_of(*qty, *cost)).sum();
    let valuation = m.report(ReportKind::StockValuation, |_| {});
    assert_eq!(
        total(&valuation, "costValue"),
        at_cost,
        "stock_valuation = Σ qty × CMUP, each rounded like value_sql"
    );
}

/// The P&L against the documents of the month, the count and the expenses.
fn check_the_profit(m: &Month, docs: &Documents) {
    let ids = m.sales_docs.clone();
    let (revenue, cost) = m
        .st()
        .db
        .read(move |c| {
            let (mut revenue, mut cost) = (0_i64, 0_i64);
            for id in &ids {
                let doc = pos_core::documents::get(c, id)?;
                for l in doc.lines.iter().filter(|l| l.parent_line_id.is_none()) {
                    revenue += l.total_ht.0;
                    cost += value_of(l.qty.0, l.cost_price.0);
                }
            }
            Ok((revenue, cost))
        })
        .unwrap();
    let pnl = m.report(ReportKind::ProfitAndLoss, |_| {});
    let line = |code: &str| -> i64 {
        pnl.rows
            .iter()
            .filter(|r| text(&pnl, r, "line") == code)
            .map(|r| int(&pnl, r, "amount"))
            .sum()
    };
    assert_eq!(
        line("revenue"),
        revenue,
        "revenue = Σ line HT of tickets and return notes"
    );
    assert_eq!(line("cost_of_goods"), -cost, "cost of goods = Σ line cost");
    assert_eq!(line("gross_margin"), revenue - cost);
    assert_eq!(
        line("stamp_absorbed"),
        0,
        "the duty is charged, never absorbed"
    );
    assert_eq!(
        line("stock_losses"),
        docs.count_losses,
        "the count's two differences at cost; the damaged goods moved are no loss"
    );
    assert_eq!(line("expenses_total"), -m.expenses, "the three expenses");
    assert_eq!(line("expense"), -m.expenses);
    assert_eq!(
        line("net_profit"),
        revenue - cost + docs.count_losses - m.expenses,
        "net = gross margin + stock losses − expenses"
    );
}

fn check_the_soldes(m: &Month, docs: &Documents) {
    let declaration = m.report(ReportKind::PromotionDeclaration, |r| {
        r.promotion_id = Some(docs.promotion_id.clone());
    });
    assert_eq!(declaration.rows.len(), 5, "the five products on soldes");
    for reference in SOLDES {
        let price = CATALOGUE.iter().find(|p| p.0 == reference).unwrap().5;
        let row = row_of(&declaration, "reference", reference).unwrap();
        assert_eq!(
            [
                int(&declaration, row, "referencePrice"),
                int(&declaration, row, "qtySold")
            ],
            [price, m.soldes_sold[&m.id(reference)]],
            "{reference}: the price before and the quantity sold during the week"
        );
    }
}

fn check_the_chains(m: &Month) {
    assert_eq!(
        must(audit::verify(m.st()), "audit chain").first_broken_seq,
        None,
        "audit::verify_chain"
    );
    for chain in must(documents::verify_chain(m.st()), "document chains") {
        assert_eq!(
            chain.first_broken_seq, None,
            "documents::verify_all: {:?} {}",
            chain.doc_type, chain.series
        );
    }
    let actions: Vec<String> = m
        .st()
        .db
        .read(|c| {
            let mut stmt = c.prepare("SELECT DISTINCT action FROM audit_log")?;
            let rows = stmt
                .query_map([], |r| r.get(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .unwrap();
    for action in actions {
        assert!(
            KNOWN_ACTIONS.contains(&action.as_str()),
            "{action} is in KNOWN_ACTIONS"
        );
    }
}

/// Every report kind, in xlsx and csv, in Arabic and English, reads back with its rows; every
/// closed-list word it holds has a label in the three languages.
fn check_the_exports(m: &Month, docs: &Documents) {
    let dir = tempfile::tempdir().unwrap();
    // A supérette's month: the reports of the vertical packs stay off with their modules.
    for kind in ReportKind::ALL.into_iter().filter(|k| k.module().is_none()) {
        let request = || {
            let mut request = ReportRequest::new(kind, "2026-09-01", "2026-09-30");
            match kind {
                ReportKind::PromotionDeclaration => {
                    request.promotion_id = Some(docs.promotion_id.clone());
                }
                ReportKind::CountVariance => request.count_id = Some(docs.count_id.clone()),
                _ => {}
            }
            request
        };
        for lang in [Lang::Ar, Lang::Fr, Lang::En] {
            let mut asked = request();
            asked.lang = Some(lang);
            let table = must(reports::run(m.st(), asked), kind.as_str())
                .report
                .table;
            for (i, column) in table.columns.iter().enumerate() {
                let Some(domain) = column.tag else { continue };
                for row in table.rows.iter().chain(table.totals.iter()) {
                    if let Some(value) = row[i].as_str() {
                        assert!(
                            pos_docs::sheet::tag_label(lang, domain, value).is_some(),
                            "{}: tags.{}.{value} in {}",
                            kind.as_str(),
                            domain.as_str(),
                            lang.as_str()
                        );
                    }
                }
            }
            if lang == Lang::Fr {
                continue;
            }
            for (format, extension) in [(SheetFormat::Xlsx, "xlsx"), (SheetFormat::Csv, "csv")] {
                let path =
                    dir.path()
                        .join(format!("{}-{}.{extension}", kind.as_str(), lang.as_str()));
                let done = must(
                    exports::export_to(
                        m.st(),
                        exports::ExportRequest {
                            target: exports::ExportTarget::Report { request: request() },
                            format,
                            lang,
                        },
                        &path,
                    ),
                    kind.as_str(),
                );
                let file = must(pos_docs::import::read_path(&path), "read back");
                let sheet = &file.sheets[0];
                let header = pos_docs::import::header_row(sheet);
                let data_rows = sheet.rows.len() - header - 1 - usize::from(table.totals.is_some());
                assert_eq!(
                    (data_rows, usize::try_from(done.rows).unwrap()),
                    (table.rows.len(), table.rows.len()),
                    "{} in {extension}, {}",
                    kind.as_str(),
                    lang.as_str()
                );
            }
        }
    }
}

/// Review Focus 5: September 2026 in a supérette, day by day, balances to the centime.
#[test]
fn a_whole_month_balances_to_the_centime() {
    let started = Instant::now();
    let mut m = set_up();
    let mut docs = Documents::new();
    for day in 2..=29 {
        m.start_day(day);
        back_office(&mut m, &mut docs);
        trading_day(&mut m, &mut docs);
    }
    m.start_day(30);
    m.shop.owner();
    check_the_drawers(&m);
    check_the_accounts(&m, &docs);
    check_the_stock(&m);
    check_the_profit(&m, &docs);
    check_the_soldes(&m, &docs);
    check_the_exports(&m, &docs);
    check_the_chains(&m);
    eprintln!("the month ran in {:?}", started.elapsed());
}

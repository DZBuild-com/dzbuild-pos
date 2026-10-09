//! Reports (spec §5.1 M2, mvp-backlog §37): each kind runs a few integer SQL aggregates over the
//! ledgers and returns the generic `Table` of Task 2 plus an optional chart hint; the screens and
//! the xlsx/csv writer (`pos-docs::sheet`) show the same table. Conventions:
//! - sales figures come from validated tickets, return notes, invoices and credit notes
//!   (`SALES_DOCS`; return and credit lines are negative, so every sum nets them); an invoice
//!   made from a ticket is left out, its ticket already counts (M3);
//! - top lines only (`SALES_LINES`): a kit's component lines carry stock, not money;
//! - a line's cost is its recorded `cost_price × qty`, rounded half away from zero in integer
//!   SQL (`LINE_COST`); the revenue is HT under the régime réel and TTC under the IFU, per
//!   document (`REVENUE`);
//! - days are business days; an instant becomes the Algiers date of its milliseconds;
//! - the permission of a kind is its group's; costs and margins are removed by `hide_costs` for
//!   viewers without `reports.profit`.

#[cfg(test)]
mod books_tests;
mod cash;
mod dashboard;
mod packs;
mod parties;
mod profit;
mod sales;
mod stock;
mod tax;
#[cfg(test)]
mod tests;

pub use dashboard::{Dashboard, DashboardView, DayTotal, MethodNet, TopLine, dashboard};

use crate::auth::Permission;
use crate::error::AppError;
use crate::lang::Lang;
use crate::ledger::Side;
use crate::party::clean_day;
use crate::table::{Cell, ColumnKind, Table};
use chrono::NaiveDate;
use rusqlite::{Connection, Row, ToSql};
use serde::{Deserialize, Serialize};
use specta::Type;

/// The longest range a dated report accepts: five years of days.
pub const MAX_RANGE_DAYS: i64 = 1_830;
/// Rows of a line-level report (journal, movements) before it says "truncated".
pub const DEFAULT_ROW_LIMIT: u32 = 10_000;
pub const MAX_ROW_LIMIT: u32 = 50_000;
/// Rows of a report written to a file (every row of the period, below a sheet's 1 048 576):
/// a report longer than that is refused (`export_too_large`), never written cut.
pub const EXPORT_ROW_LIMIT: u32 = 1_000_000;
/// Columns a viewer without `reports.profit` never receives.
pub const COST_COLUMNS: [&str; 6] = [
    "cost",
    "unitCost",
    "costPrice",
    "costValue",
    "margin",
    "marginRate",
];

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum ReportKind {
    SalesSummary,
    SalesByProduct,
    SalesByCategory,
    SalesByCustomer,
    SalesByCashier,
    SalesByMethod,
    PeriodComparison,
    TopProducts,
    Discounts,
    Returns,
    SalesLines,
    ProfitByPeriod,
    ProfitByProduct,
    ProfitByCategory,
    ProfitAndLoss,
    SlowMovers,
    LossPrevention,
    StockLevels,
    StockValuation,
    LowStock,
    Expiry,
    StockMovements,
    CountVariance,
    PurchasesBySupplier,
    PurchasesByProduct,
    SupplierBalances,
    CustomerBalances,
    CustomerAging,
    CreditActivity,
    CashSessions,
    CashMovements,
    DailyByMethod,
    Expenses,
    PromotionDeclaration,
    VatByRate,
    VatDeductible,
    StampDutyStatement,
    G50,
    IfuTurnover,
    G9Suppliers,
    CustomersAnnual,
    SalesRegister,
    PurchasesRegister,
    /// RPT-065 (restaurant): per dining mode, table or label and waiter.
    SalesByTable,
    /// The controlled-medicine register of the period (pharmacy, `pharmacy.register`).
    ControlledRegister,
    /// STK-026: the waste log per day, product and reason, at cost.
    Waste,
    /// VRT-011: the carcass cutting runs of the period.
    YieldRuns,
    /// RPT-074: top-ups per day and operator, the commission on them, the float bought.
    FlexySales,
    /// RPT-068: service tickets of the period with their technician, days, estimate and margin.
    ServiceTickets,
    /// RPT-069: ready items waiting longer than `repairs.uncollected_days`, with their deposits.
    UncollectedTickets,
    /// TMP-005: every instalment due unpaid past its date, as of `to`, with its days late.
    InstalmentsOverdue,
    /// RPT-073: delivery rounds of the period with their stops and collections.
    DeliveryRounds,
    /// LOY-002: per enrolled customer, the points earned, redeemed, expired, taken back and
    /// adjusted over the period, and the balance at `to`.
    LoyaltyPoints,
    /// LOY-001, PAY-010: the gift, exchange and loyalty vouchers alive over the period, what was
    /// spent of them and what the shop still owes their holders.
    GiftVouchers,
    /// VRT-141: the jeweller's special client register (CII art. 359, LF 2026 art. 62) of the
    /// year of `from`, every entry in order, with the buyers' identities.
    JewellerRegister,
    /// RPT-071: precious-metal pieces sold per metal and fineness, grams, turnover, making
    /// charges and the average price per gram.
    JewellerySales,
    /// RPT-062 lite: per cashier, tickets, returns, net sales HT and TTC, their commission rate
    /// and the commission on the net HT.
    StaffCommissions,
    /// EMP-003 lite: per employee, the advances given, repaid and kept off salaries over the
    /// period, and what is still owed at `to`.
    StaffAdvances,
    /// PAY-006: tenders and amount per base method, each custom payment type (prélèvement CCP,
    /// TPE BNA…) on its own row under the method it rides.
    PaymentTypes,
    /// RPT-020 (PDP-003): the journal's personal-data rows of the period — customer files
    /// opened, exported or anonymised, consents recorded — with the user and the customer's
    /// code, never a name.
    PersonalDataAccess,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ReportGroup {
    Sales,
    Profit,
    Stock,
    Purchases,
    Customers,
    Cash,
    Declarations,
    Tax,
    /// Reports of the vertical packs (M8), shown while their module is on.
    Packs,
}

impl ReportGroup {
    pub const ALL: [ReportGroup; 9] = [
        ReportGroup::Sales,
        ReportGroup::Profit,
        ReportGroup::Stock,
        ReportGroup::Purchases,
        ReportGroup::Customers,
        ReportGroup::Cash,
        ReportGroup::Declarations,
        ReportGroup::Tax,
        ReportGroup::Packs,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            ReportGroup::Sales => "sales",
            ReportGroup::Profit => "profit",
            ReportGroup::Stock => "stock",
            ReportGroup::Purchases => "purchases",
            ReportGroup::Customers => "customers",
            ReportGroup::Cash => "cash",
            ReportGroup::Declarations => "declarations",
            ReportGroup::Tax => "tax",
            ReportGroup::Packs => "packs",
        }
    }

    pub const fn permission(self) -> Permission {
        match self {
            ReportGroup::Sales | ReportGroup::Declarations => Permission::ReportsSales,
            ReportGroup::Profit => Permission::ReportsProfit,
            ReportGroup::Stock => Permission::ReportsStock,
            ReportGroup::Purchases => Permission::PurchasesView,
            ReportGroup::Customers => Permission::CreditView,
            ReportGroup::Cash => Permission::ReportsCash,
            ReportGroup::Tax => Permission::ReportsTax,
            // The group's default; a pack kind that needs another right says so in
            // `ReportKind::permission`.
            ReportGroup::Packs => Permission::ReportsSales,
        }
    }
}

/// What the report screen offers for a kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ReportFilter {
    Grouping,
    Terminal,
    User,
    Category,
    Product,
    Customer,
    Location,
    Supplier,
    Promotion,
    Count,
}

use ReportFilter as F;

const SALES_FILTERS: &[ReportFilter] = &[F::Terminal, F::User, F::Customer];
const LINE_FILTERS: &[ReportFilter] = &[F::Terminal, F::User, F::Customer, F::Category, F::Product];
const PERIOD_FILTERS: &[ReportFilter] = &[F::Grouping, F::Terminal, F::User, F::Customer];

impl ReportKind {
    pub const ALL: [ReportKind; 60] = [
        ReportKind::SalesSummary,
        ReportKind::SalesByProduct,
        ReportKind::SalesByCategory,
        ReportKind::SalesByCustomer,
        ReportKind::SalesByCashier,
        ReportKind::SalesByMethod,
        ReportKind::PeriodComparison,
        ReportKind::TopProducts,
        ReportKind::Discounts,
        ReportKind::Returns,
        ReportKind::SalesLines,
        ReportKind::ProfitByPeriod,
        ReportKind::ProfitByProduct,
        ReportKind::ProfitByCategory,
        ReportKind::ProfitAndLoss,
        ReportKind::SlowMovers,
        ReportKind::LossPrevention,
        ReportKind::StockLevels,
        ReportKind::StockValuation,
        ReportKind::LowStock,
        ReportKind::Expiry,
        ReportKind::StockMovements,
        ReportKind::CountVariance,
        ReportKind::PurchasesBySupplier,
        ReportKind::PurchasesByProduct,
        ReportKind::SupplierBalances,
        ReportKind::CustomerBalances,
        ReportKind::CustomerAging,
        ReportKind::CreditActivity,
        ReportKind::CashSessions,
        ReportKind::CashMovements,
        ReportKind::DailyByMethod,
        ReportKind::Expenses,
        ReportKind::PromotionDeclaration,
        ReportKind::VatByRate,
        ReportKind::VatDeductible,
        ReportKind::StampDutyStatement,
        ReportKind::G50,
        ReportKind::IfuTurnover,
        ReportKind::G9Suppliers,
        ReportKind::CustomersAnnual,
        ReportKind::SalesRegister,
        ReportKind::PurchasesRegister,
        ReportKind::SalesByTable,
        ReportKind::ControlledRegister,
        ReportKind::Waste,
        ReportKind::YieldRuns,
        ReportKind::FlexySales,
        ReportKind::ServiceTickets,
        ReportKind::UncollectedTickets,
        ReportKind::InstalmentsOverdue,
        ReportKind::DeliveryRounds,
        ReportKind::LoyaltyPoints,
        ReportKind::GiftVouchers,
        ReportKind::JewellerRegister,
        ReportKind::JewellerySales,
        ReportKind::StaffCommissions,
        ReportKind::StaffAdvances,
        ReportKind::PaymentTypes,
        ReportKind::PersonalDataAccess,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            ReportKind::SalesSummary => "sales_summary",
            ReportKind::SalesByProduct => "sales_by_product",
            ReportKind::SalesByCategory => "sales_by_category",
            ReportKind::SalesByCustomer => "sales_by_customer",
            ReportKind::SalesByCashier => "sales_by_cashier",
            ReportKind::SalesByMethod => "sales_by_method",
            ReportKind::PeriodComparison => "period_comparison",
            ReportKind::TopProducts => "top_products",
            ReportKind::Discounts => "discounts",
            ReportKind::Returns => "returns",
            ReportKind::SalesLines => "sales_lines",
            ReportKind::ProfitByPeriod => "profit_by_period",
            ReportKind::ProfitByProduct => "profit_by_product",
            ReportKind::ProfitByCategory => "profit_by_category",
            ReportKind::ProfitAndLoss => "profit_and_loss",
            ReportKind::SlowMovers => "slow_movers",
            ReportKind::LossPrevention => "loss_prevention",
            ReportKind::StockLevels => "stock_levels",
            ReportKind::StockValuation => "stock_valuation",
            ReportKind::LowStock => "low_stock",
            ReportKind::Expiry => "expiry",
            ReportKind::StockMovements => "stock_movements",
            ReportKind::CountVariance => "count_variance",
            ReportKind::PurchasesBySupplier => "purchases_by_supplier",
            ReportKind::PurchasesByProduct => "purchases_by_product",
            ReportKind::SupplierBalances => "supplier_balances",
            ReportKind::CustomerBalances => "customer_balances",
            ReportKind::CustomerAging => "customer_aging",
            ReportKind::CreditActivity => "credit_activity",
            ReportKind::CashSessions => "cash_sessions",
            ReportKind::CashMovements => "cash_movements",
            ReportKind::DailyByMethod => "daily_by_method",
            ReportKind::Expenses => "expenses",
            ReportKind::PromotionDeclaration => "promotion_declaration",
            ReportKind::VatByRate => "vat_by_rate",
            ReportKind::VatDeductible => "vat_deductible",
            ReportKind::StampDutyStatement => "stamp_duty_statement",
            ReportKind::G50 => "g50",
            ReportKind::IfuTurnover => "ifu_turnover",
            ReportKind::G9Suppliers => "g9_suppliers",
            ReportKind::CustomersAnnual => "customers_annual",
            ReportKind::SalesRegister => "sales_register",
            ReportKind::PurchasesRegister => "purchases_register",
            ReportKind::SalesByTable => "sales_by_table",
            ReportKind::ControlledRegister => "controlled_register",
            ReportKind::Waste => "waste",
            ReportKind::YieldRuns => "yield_runs",
            ReportKind::FlexySales => "flexy_sales",
            ReportKind::ServiceTickets => "service_tickets",
            ReportKind::UncollectedTickets => "uncollected_tickets",
            ReportKind::InstalmentsOverdue => "instalments_overdue",
            ReportKind::DeliveryRounds => "delivery_rounds",
            ReportKind::LoyaltyPoints => "loyalty_points",
            ReportKind::GiftVouchers => "gift_vouchers",
            ReportKind::JewellerRegister => "jeweller_register",
            ReportKind::JewellerySales => "jewellery_sales",
            ReportKind::StaffCommissions => "staff_commissions",
            ReportKind::StaffAdvances => "staff_advances",
            ReportKind::PaymentTypes => "payment_types",
            ReportKind::PersonalDataAccess => "personal_data_access",
        }
    }

    pub const fn group(self) -> ReportGroup {
        match self {
            ReportKind::ProfitByPeriod
            | ReportKind::ProfitByProduct
            | ReportKind::ProfitByCategory
            | ReportKind::ProfitAndLoss => ReportGroup::Profit,
            ReportKind::SlowMovers
            | ReportKind::StockLevels
            | ReportKind::StockValuation
            | ReportKind::LowStock
            | ReportKind::Expiry
            | ReportKind::StockMovements
            | ReportKind::CountVariance => ReportGroup::Stock,
            ReportKind::PurchasesBySupplier
            | ReportKind::PurchasesByProduct
            | ReportKind::SupplierBalances => ReportGroup::Purchases,
            ReportKind::CustomerBalances
            | ReportKind::CustomerAging
            | ReportKind::CreditActivity
            | ReportKind::PersonalDataAccess => ReportGroup::Customers,
            ReportKind::LossPrevention
            | ReportKind::CashSessions
            | ReportKind::CashMovements
            | ReportKind::DailyByMethod
            | ReportKind::Expenses => ReportGroup::Cash,
            ReportKind::PromotionDeclaration => ReportGroup::Declarations,
            ReportKind::VatByRate
            | ReportKind::VatDeductible
            | ReportKind::StampDutyStatement
            | ReportKind::G50
            | ReportKind::IfuTurnover
            | ReportKind::G9Suppliers
            | ReportKind::CustomersAnnual
            | ReportKind::SalesRegister
            | ReportKind::PurchasesRegister => ReportGroup::Tax,
            ReportKind::SalesByTable
            | ReportKind::ControlledRegister
            | ReportKind::Waste
            | ReportKind::YieldRuns
            | ReportKind::FlexySales
            | ReportKind::ServiceTickets
            | ReportKind::UncollectedTickets
            | ReportKind::InstalmentsOverdue
            | ReportKind::DeliveryRounds
            | ReportKind::LoyaltyPoints
            | ReportKind::GiftVouchers
            | ReportKind::JewellerRegister
            | ReportKind::JewellerySales
            | ReportKind::StaffCommissions
            | ReportKind::StaffAdvances => ReportGroup::Packs,
            _ => ReportGroup::Sales,
        }
    }

    pub const fn permission(self) -> Permission {
        match self {
            // Health data (loi 18-07 art. 18): read with the right that writes it.
            ReportKind::ControlledRegister => Permission::PharmacyRegister,
            ReportKind::Waste | ReportKind::YieldRuns => Permission::ReportsStock,
            // The float's cost left the drawer: a cash figure, not a margin.
            ReportKind::FlexySales => Permission::ReportsCash,
            // Customers' debts: read with the kridi notebook's right.
            ReportKind::InstalmentsOverdue => Permission::CreditView,
            // Points are a sales figure; vouchers are money the shop holds for their bearers.
            ReportKind::LoyaltyPoints => Permission::ReportsSales,
            ReportKind::GiftVouchers => Permission::ReportsCash,
            // The register holds the buyers' identity documents and addresses: a manager's read,
            // never a cashier's (who holds `customers.view`).
            ReportKind::JewellerRegister => Permission::CustomersManage,
            ReportKind::JewellerySales => Permission::ReportsSales,
            // Commissions are a sales figure; advances left the drawer as wages.
            ReportKind::StaffCommissions => Permission::ReportsSales,
            ReportKind::StaffAdvances => Permission::ReportsCash,
            // Who opened which customer file is read with the journal's right (PDP-003).
            ReportKind::PersonalDataAccess => Permission::AuditView,
            _ => self.group().permission(),
        }
    }

    /// `false` for "as of" reports: they ignore `from` and skip the range check. The stock
    /// levels, valuation, low stock, expiry, balances and ageing read their ledgers as they stood
    /// at the end of the `to` day (valued and compared at today's prices, minimums and credit
    /// limits); `count_variance` and `promotion_declaration` show their own document.
    pub const fn dated(self) -> bool {
        !matches!(
            self,
            ReportKind::StockLevels
                | ReportKind::StockValuation
                | ReportKind::LowStock
                | ReportKind::Expiry
                | ReportKind::CountVariance
                | ReportKind::SupplierBalances
                | ReportKind::CustomerBalances
                | ReportKind::CustomerAging
                | ReportKind::PromotionDeclaration
                | ReportKind::UncollectedTickets
                | ReportKind::InstalmentsOverdue
        )
    }

    pub const fn filters(self) -> &'static [ReportFilter] {
        match self {
            ReportKind::SalesSummary | ReportKind::ProfitByPeriod => PERIOD_FILTERS,
            ReportKind::SalesByCustomer
            | ReportKind::SalesByCashier
            | ReportKind::SalesByMethod => SALES_FILTERS,
            ReportKind::PeriodComparison => SALES_FILTERS,
            ReportKind::SalesByProduct
            | ReportKind::SalesByCategory
            | ReportKind::TopProducts
            | ReportKind::Discounts
            | ReportKind::Returns
            | ReportKind::SalesLines
            | ReportKind::ProfitByProduct
            | ReportKind::ProfitByCategory => LINE_FILTERS,
            ReportKind::ProfitAndLoss | ReportKind::Expenses => &[],
            ReportKind::SlowMovers | ReportKind::StockLevels => &[F::Location, F::Category],
            ReportKind::StockValuation | ReportKind::LowStock | ReportKind::Expiry => {
                &[F::Location]
            }
            ReportKind::StockMovements => &[F::Location, F::Category, F::Product],
            ReportKind::CountVariance => &[F::Count],
            ReportKind::PurchasesBySupplier | ReportKind::SupplierBalances => &[F::Supplier],
            ReportKind::PurchasesByProduct => &[F::Supplier, F::Category, F::Product],
            ReportKind::CustomerBalances
            | ReportKind::CustomerAging
            | ReportKind::CreditActivity => &[F::Customer],
            ReportKind::LossPrevention => &[F::User],
            ReportKind::CashSessions | ReportKind::CashMovements => &[F::Terminal, F::User],
            ReportKind::DailyByMethod => &[F::Terminal],
            ReportKind::PromotionDeclaration => &[F::Promotion],
            ReportKind::VatByRate | ReportKind::IfuTurnover => &[F::Grouping],
            ReportKind::StampDutyStatement | ReportKind::SalesRegister => &[F::Terminal],
            ReportKind::VatDeductible | ReportKind::G9Suppliers | ReportKind::PurchasesRegister => {
                &[F::Supplier]
            }
            ReportKind::CustomersAnnual => &[F::Customer],
            ReportKind::G50 => &[],
            ReportKind::SalesByTable => &[F::Terminal, F::User],
            ReportKind::ControlledRegister => &[],
            ReportKind::Waste => &[F::Location, F::Category],
            ReportKind::YieldRuns => &[],
            ReportKind::FlexySales => &[F::Terminal],
            ReportKind::ServiceTickets => &[F::User],
            ReportKind::UncollectedTickets => &[],
            ReportKind::InstalmentsOverdue => &[F::Customer],
            ReportKind::DeliveryRounds => &[F::User],
            ReportKind::LoyaltyPoints => &[F::Customer],
            ReportKind::GiftVouchers => &[],
            ReportKind::JewellerRegister | ReportKind::JewellerySales => &[],
            ReportKind::StaffCommissions => &[F::Terminal, F::User],
            ReportKind::StaffAdvances => &[F::User],
            ReportKind::PaymentTypes => SALES_FILTERS,
            ReportKind::PersonalDataAccess => &[F::User, F::Customer],
        }
    }

    /// The kind reads tables kept on the main PC only (`HostOnly`: the legal registers, the
    /// delivery rounds): a till runs it on the main PC (final review).
    pub const fn reads_host_only(self) -> bool {
        matches!(
            self,
            ReportKind::ControlledRegister
                | ReportKind::JewellerRegister
                | ReportKind::JewellerySales
                | ReportKind::DeliveryRounds
        )
    }

    /// The pack module a kind belongs to: the report is refused (and hidden from the catalogue)
    /// while that module is off.
    pub const fn module(self) -> Option<crate::packs::Module> {
        match self {
            ReportKind::SalesByTable => Some(crate::packs::Module::Restaurant),
            ReportKind::ControlledRegister => Some(crate::packs::Module::Pharmacy),
            ReportKind::Waste => Some(crate::packs::Module::Waste),
            ReportKind::YieldRuns => Some(crate::packs::Module::Yield),
            ReportKind::FlexySales => Some(crate::packs::Module::Flexy),
            ReportKind::ServiceTickets | ReportKind::UncollectedTickets => {
                Some(crate::packs::Module::Repairs)
            }
            ReportKind::InstalmentsOverdue => Some(crate::packs::Module::Instalments),
            ReportKind::DeliveryRounds => Some(crate::packs::Module::DeliveryRounds),
            ReportKind::LoyaltyPoints => Some(crate::packs::Module::Loyalty),
            ReportKind::GiftVouchers => Some(crate::packs::Module::GiftVouchers),
            ReportKind::JewellerRegister | ReportKind::JewellerySales => {
                Some(crate::packs::Module::Jewellery)
            }
            ReportKind::StaffCommissions | ReportKind::StaffAdvances => {
                Some(crate::packs::Module::Staff)
            }
            _ => None,
        }
    }

    /// The filter a kind cannot run without.
    pub const fn required_filter(self) -> Option<ReportFilter> {
        match self {
            ReportKind::CountVariance => Some(ReportFilter::Count),
            ReportKind::PromotionDeclaration => Some(ReportFilter::Promotion),
            _ => None,
        }
    }

    pub const fn default_limit(self) -> u32 {
        match self {
            ReportKind::TopProducts => 20,
            _ => DEFAULT_ROW_LIMIT,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Grouping {
    #[default]
    Day,
    Week,
    Month,
    Year,
}

impl Grouping {
    /// The period key of the `YYYY-MM-DD` expression `day`: the day, the Sunday that starts its
    /// week (the Algerian working week), `YYYY-MM` or `YYYY`.
    pub(crate) fn sql(self, day: &str) -> String {
        match self {
            Grouping::Day => day.to_owned(),
            Grouping::Week => format!("date({day}, '-' || strftime('%w', {day}) || ' days')"),
            Grouping::Month => format!("substr({day}, 1, 7)"),
            Grouping::Year => format!("substr({day}, 1, 4)"),
        }
    }

    pub(crate) const fn column_kind(self) -> ColumnKind {
        match self {
            Grouping::Day | Grouping::Week => ColumnKind::Date,
            Grouping::Month | Grouping::Year => ColumnKind::Text,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReportRequest {
    pub kind: ReportKind,
    /// First and last business day, `YYYY-MM-DD`, both included. "As of" reports read `to`.
    pub from: String,
    pub to: String,
    #[serde(default)]
    #[specta(optional)]
    pub group: Option<Grouping>,
    #[serde(default)]
    #[specta(optional)]
    pub terminal_id: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    pub user_id: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    pub category_id: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    pub product_id: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    pub customer_id: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    pub location_id: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    pub supplier_id: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    pub promotion_id: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    pub count_id: Option<String>,
    #[serde(default)]
    #[specta(optional)]
    pub limit: Option<u32>,
    /// Language of the names in the rows (products, categories, locations, expense categories).
    #[serde(default)]
    #[specta(optional)]
    pub lang: Option<Lang>,
}

impl ReportRequest {
    pub fn new(kind: ReportKind, from: &str, to: &str) -> ReportRequest {
        ReportRequest {
            kind,
            from: from.to_owned(),
            to: to.to_owned(),
            group: None,
            terminal_id: None,
            user_id: None,
            category_id: None,
            product_id: None,
            customer_id: None,
            location_id: None,
            supplier_id: None,
            promotion_id: None,
            count_id: None,
            limit: None,
            lang: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ChartKind {
    /// Vertical bars along time.
    Bar,
    /// Horizontal bars for ranked names.
    HBar,
}

/// A single-series chart drawn from two columns of the table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Chart {
    pub kind: ChartKind,
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub kind: ReportKind,
    pub from: String,
    pub to: String,
    pub table: Table,
    pub chart: Option<Chart>,
    /// More rows existed than the limit.
    pub truncated: bool,
}

impl Report {
    /// The same report for a viewer without `reports.profit` (like M1b `commands::redact`): the
    /// cost and margin columns are removed from the rows and the totals, and so is a chart drawn
    /// from one of them.
    pub fn hide_costs(mut self) -> Report {
        let keep: Vec<bool> = self
            .table
            .columns
            .iter()
            .map(|c| !COST_COLUMNS.contains(&c.key.as_str()))
            .collect();
        let strip = |row: Vec<Cell>| -> Vec<Cell> {
            row.into_iter()
                .zip(keep.iter())
                .filter_map(|(cell, kept)| kept.then_some(cell))
                .collect()
        };
        self.table.rows = std::mem::take(&mut self.table.rows)
            .into_iter()
            .map(strip)
            .collect();
        self.table.totals = self.table.totals.take().map(strip);
        self.table
            .columns
            .retain(|c| !COST_COLUMNS.contains(&c.key.as_str()));
        if self
            .chart
            .as_ref()
            .is_some_and(|c| COST_COLUMNS.contains(&c.value.as_str()))
        {
            self.chart = None;
        }
        self
    }
}

/// What a report builder returns.
pub(crate) struct Built {
    pub table: Table,
    pub chart: Option<Chart>,
    pub truncated: bool,
}

impl Built {
    pub(crate) fn plain(table: Table) -> Built {
        Built {
            table,
            chart: None,
            truncated: false,
        }
    }

    pub(crate) fn charted(table: Table, kind: ChartKind, label: &str, value: &str) -> Built {
        Built {
            table,
            chart: Some(Chart {
                kind,
                label: label.to_owned(),
                value: value.to_owned(),
            }),
            truncated: false,
        }
    }

    /// Keeps `limit` rows of `rows` fetched with `LIMIT :row_limit + 1`.
    pub(crate) fn cut<T>(mut rows: Vec<T>, limit: i64) -> (Vec<T>, bool) {
        let keep = usize::try_from(limit).unwrap_or(usize::MAX);
        let truncated = rows.len() > keep;
        rows.truncate(keep);
        (rows, truncated)
    }
}

/// The checked request a builder reads: cleaned days, limit, language.
pub(crate) struct Scope<'a> {
    pub req: &'a ReportRequest,
    pub from: String,
    pub to: String,
    pub limit: i64,
    pub lang: Lang,
}

impl Scope<'_> {
    /// The same filters over another range (period comparison).
    pub(crate) fn over(&self, from: String, to: String) -> Scope<'_> {
        Scope {
            req: self.req,
            from,
            to,
            limit: self.limit,
            lang: self.lang,
        }
    }

    /// The named parameters `sql` uses. Every builder writes its filters as
    /// `(:x IS NULL OR col = :x)`, so an absent filter binds NULL; a parameter the statement does
    /// not name is not bound (rusqlite refuses unknown names).
    fn params<'s>(&'s self, sql: &str) -> Vec<(&'static str, &'s dyn ToSql)> {
        let r = self.req;
        let all: [(&'static str, &'s dyn ToSql); 12] = [
            (":day_from", &self.from),
            (":day_to", &self.to),
            (":terminal_id", &r.terminal_id),
            (":user_id", &r.user_id),
            (":category_id", &r.category_id),
            (":product_id", &r.product_id),
            (":customer_id", &r.customer_id),
            (":location_id", &r.location_id),
            (":supplier_id", &r.supplier_id),
            (":promotion_id", &r.promotion_id),
            (":count_id", &r.count_id),
            (":row_limit", &self.limit),
        ];
        all.into_iter()
            .filter(|(name, _)| sql.contains(name))
            .collect()
    }
}

/// Runs `sql` with the scope's parameters and maps every row.
pub(crate) fn rows<T>(
    conn: &Connection,
    sql: &str,
    scope: &Scope<'_>,
    map: impl FnMut(&Row<'_>) -> rusqlite::Result<T>,
) -> Result<Vec<T>, AppError> {
    let mut stmt = conn.prepare(sql)?;
    let params = scope.params(sql);
    let out = stmt
        .query_map(params.as_slice(), map)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(out)
}

/// `N` integer columns from `start`; SQL `NULL` (an empty `SUM`) reads as 0.
pub(crate) fn ints<const N: usize>(r: &Row<'_>, start: usize) -> rusqlite::Result<[i64; N]> {
    let mut out = [0_i64; N];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = r.get::<_, Option<i64>>(start + i)?.unwrap_or(0);
    }
    Ok(out)
}

/// Adds `v` into `acc` (report totals saturate, they never panic).
pub(crate) fn add<const N: usize>(acc: &mut [i64; N], v: &[i64; N]) {
    for (a, b) in acc.iter_mut().zip(v) {
        *a = a.saturating_add(*b);
    }
}

/// `part / whole` in basis points, rounded half away from zero; `NULL` against nothing.
pub(crate) fn rate_bp(part: i64, whole: i64) -> Cell {
    if whole == 0 {
        return Cell::NULL;
    }
    let num = i128::from(part) * 10_000;
    let den = i128::from(whole);
    let (q, r) = (num / den, num % den);
    let q = if 2 * r.abs() >= den.abs() {
        q + num.signum() * den.signum()
    } else {
        q
    };
    i64::try_from(q).map_or(Cell::NULL, Cell::int)
}

/// `total / count` rounded half up; 0 without any count.
pub(crate) fn average(total: i64, count: i64) -> i64 {
    if count <= 0 {
        return 0;
    }
    let (q, r) = (total / count, total % count);
    if 2 * r.abs() >= count {
        q + total.signum()
    } else {
        q
    }
}

/// The name of the row aliased `alias` (`name_ar`, `name_fr`, `name_en`) in `lang`, falling back
/// on the other languages when it is empty.
pub(crate) fn name_of(alias: &str, lang: Lang) -> String {
    let order = match lang {
        Lang::Ar => ["ar", "fr", "en"],
        Lang::Fr => ["fr", "ar", "en"],
        Lang::En => ["en", "fr", "ar"],
    };
    format!(
        "COALESCE(NULLIF({alias}.name_{}, ''), NULLIF({alias}.name_{}, ''), {alias}.name_{})",
        order[0], order[1], order[2]
    )
}

/// The Algiers business date of the millisecond instant `column`.
pub(crate) fn day_of(column: &str) -> String {
    format!(
        "date(({column} + {}) / 1000, 'unixepoch')",
        crate::time::ALGIERS_OFFSET_MS
    )
}

/// A milli-quantity times a unit price in centimes, rounded half away from zero (integer SQL).
pub(crate) fn value_sql(qty: &str, price: &str) -> String {
    format!("(({qty}) * ({price}) + CASE WHEN ({qty}) < 0 THEN -500 ELSE 500 END) / 1000")
}

/// The same as `value_sql`, in Rust (saturating).
pub(crate) fn value_of(qty: i64, price: i64) -> i64 {
    let product = i128::from(qty) * i128::from(price);
    let half = if product < 0 { -500 } else { 500 };
    i64::try_from((product + half) / 1000).unwrap_or(if product < 0 { i64::MIN } else { i64::MAX })
}

/// Validated sales documents of the range, till, cashier and customer (alias `d`): tickets,
/// return notes, invoices and credit notes — an invoice made from a ticket is left out, its
/// ticket already counts (M3).
pub(crate) const SALES_DOCS: &str = "d.status = 'validated'
     AND (d.doc_type IN ('ticket', 'return_note', 'credit_note')
          OR (d.doc_type = 'invoice' AND d.origin IS NOT 'ticket'))
     AND d.supplier_id IS NULL AND d.business_date BETWEEN :day_from AND :day_to
     AND (:terminal_id IS NULL OR d.terminal_id = :terminal_id)
     AND (:user_id IS NULL OR d.user_id = :user_id)
     AND (:customer_id IS NULL OR d.customer_id = :customer_id)";

/// A sale (a ticket or an invoice) and a return (a return note or a credit note).
pub(crate) const SALE_KINDS: &str = "d.doc_type IN ('ticket', 'invoice')";
pub(crate) const RETURN_KINDS: &str = "d.doc_type IN ('return_note', 'credit_note')";

/// Top lines of the requested product and category (alias `l`, products left-joined as `p`).
pub(crate) const SALES_LINES: &str = "l.parent_line_id IS NULL
     AND (:product_id IS NULL OR l.product_id = :product_id)
     AND (:category_id IS NULL OR p.category_id = :category_id)";

/// The recorded cost of line `l` (negative on a return), rounded half away from zero.
pub(crate) const LINE_COST: &str =
    "((l.cost_price * l.qty + CASE WHEN l.qty < 0 THEN -500 ELSE 500 END) / 1000)";

/// What the shop earns on line `l` of document `d`: HT under the réel, TTC under the IFU.
pub(crate) const REVENUE: &str =
    "(CASE d.tax_regime WHEN 'ifu' THEN l.total_ttc ELSE l.total_ht END)";

fn check(req: &ReportRequest) -> Result<(String, String), AppError> {
    let from = clean_day(Some(req.from.as_str()), "from")?
        .ok_or_else(|| AppError::validation("from", "required"))?;
    let to = clean_day(Some(req.to.as_str()), "to")?
        .ok_or_else(|| AppError::validation("to", "required"))?;
    if req.kind.dated() {
        let parse = |d: &str| {
            NaiveDate::parse_from_str(d, "%Y-%m-%d")
                .map_err(|_| AppError::validation("to", "date_invalid"))
        };
        let span = (parse(&to)? - parse(&from)?).num_days();
        if !(0..MAX_RANGE_DAYS).contains(&span) {
            return Err(AppError::validation("to", "report_range_invalid"));
        }
    }
    let missing = match req.kind.required_filter() {
        Some(ReportFilter::Count) if req.count_id.is_none() => Some("countId"),
        Some(ReportFilter::Promotion) if req.promotion_id.is_none() => Some("promotionId"),
        _ => None,
    };
    if let Some(field) = missing {
        return Err(AppError::validation(field, "report_filter_required"));
    }
    Ok((from, to))
}

/// Runs one report with the bundled regulatory data pack (M2's callers and tests); see
/// [`run_with`].
pub fn run(conn: &Connection, req: &ReportRequest) -> Result<Report, AppError> {
    run_with(conn, req, &crate::datapack::bundled()?)
}

/// Runs one report with the regulatory data pack the tax reports need (G50 boxes, the
/// deduction limit, the IFU threshold). The caller checks `req.kind.permission()` and calls
/// `hide_costs` for a viewer without `reports.profit`.
pub fn run_with(
    conn: &Connection,
    req: &ReportRequest,
    pack: &crate::datapack::Pack,
) -> Result<Report, AppError> {
    let limit = req
        .limit
        .unwrap_or(req.kind.default_limit())
        .clamp(1, MAX_ROW_LIMIT);
    run_limited(conn, req, pack, limit)
}

/// The report as a file holds it: every row of the period (the screen's cut does not apply; the
/// top-N report keeps its N), up to [`EXPORT_ROW_LIMIT`], beyond which it is refused
/// (`rows`/`export_too_large`) rather than written cut — a legal register is never partial.
pub fn run_for_export(
    conn: &Connection,
    req: &ReportRequest,
    pack: &crate::datapack::Pack,
) -> Result<Report, AppError> {
    let limit = match req.kind {
        ReportKind::TopProducts => req
            .limit
            .unwrap_or(req.kind.default_limit())
            .clamp(1, MAX_ROW_LIMIT),
        _ => EXPORT_ROW_LIMIT,
    };
    let report = run_limited(conn, req, pack, limit)?;
    if report.truncated {
        return Err(AppError::validation("rows", "export_too_large"));
    }
    Ok(report)
}

/// The controlled register report built from its entries as the main PC answers them
/// (a till's copy holds no register: `HostOnly`); the same report [`run_with`] gives there.
pub fn controlled_register_from(
    req: &ReportRequest,
    entries: Vec<crate::pharmacy::register::RegisterEntry>,
) -> Result<Report, AppError> {
    if req.kind != ReportKind::ControlledRegister {
        return Err(AppError::validation("kind", "value_invalid"));
    }
    let (from, to) = check(req)?;
    Ok(Report {
        kind: req.kind,
        from,
        to,
        table: crate::pharmacy::register::table_of(entries)?,
        chart: None,
        truncated: false,
    })
}

fn run_limited(
    conn: &Connection,
    req: &ReportRequest,
    pack: &crate::datapack::Pack,
    limit: u32,
) -> Result<Report, AppError> {
    let (from, to) = check(req)?;
    // A pack's report is refused while its module is off (on screen and in a file alike).
    if let Some(m) = req.kind.module() {
        crate::packs::require(conn, m)?;
    }
    let limit = i64::from(limit);
    let scope = Scope {
        req,
        from,
        to,
        limit,
        lang: req.lang.unwrap_or_default(),
    };
    let built = match req.kind {
        ReportKind::SalesSummary => sales::summary(conn, &scope)?,
        ReportKind::SalesByProduct => sales::by_product(conn, &scope)?,
        ReportKind::SalesByCategory => sales::by_category(conn, &scope)?,
        ReportKind::SalesByCustomer => sales::by_customer(conn, &scope)?,
        ReportKind::SalesByCashier => sales::by_cashier(conn, &scope)?,
        ReportKind::SalesByMethod => sales::by_method(conn, &scope)?,
        ReportKind::PeriodComparison => sales::period_comparison(conn, &scope)?,
        ReportKind::TopProducts => sales::top_products(conn, &scope)?,
        ReportKind::Discounts => sales::discounts(conn, &scope)?,
        ReportKind::Returns => sales::returns(conn, &scope)?,
        ReportKind::SalesLines => sales::lines(conn, &scope)?,
        ReportKind::SlowMovers => sales::slow_movers(conn, &scope)?,
        ReportKind::LossPrevention => sales::loss_prevention(conn, &scope)?,
        ReportKind::ProfitByPeriod => profit::by_period(conn, &scope)?,
        ReportKind::ProfitByProduct => profit::by_product(conn, &scope)?,
        ReportKind::ProfitByCategory => profit::by_category(conn, &scope)?,
        ReportKind::ProfitAndLoss => profit::profit_and_loss(conn, &scope)?,
        ReportKind::StockLevels => stock::levels(conn, &scope)?,
        ReportKind::StockValuation => stock::valuation(conn, &scope)?,
        ReportKind::LowStock => stock::low_stock(conn, &scope)?,
        ReportKind::Expiry => stock::expiry(conn, &scope)?,
        ReportKind::StockMovements => stock::movements(conn, &scope)?,
        ReportKind::CountVariance => stock::count_variance(conn, &scope)?,
        ReportKind::PurchasesBySupplier => parties::purchases_by_supplier(conn, &scope)?,
        ReportKind::PurchasesByProduct => parties::purchases_by_product(conn, &scope)?,
        ReportKind::SupplierBalances => parties::aging(conn, &scope, Side::Supplier)?,
        ReportKind::CustomerBalances => parties::customer_balances(conn, &scope)?,
        ReportKind::CustomerAging => parties::aging(conn, &scope, Side::Customer)?,
        ReportKind::CreditActivity => parties::credit_activity(conn, &scope)?,
        ReportKind::CashSessions => cash::sessions(conn, &scope)?,
        ReportKind::CashMovements => cash::movements(conn, &scope)?,
        ReportKind::DailyByMethod => cash::daily_by_method(conn, &scope)?,
        ReportKind::Expenses => cash::expenses(conn, &scope)?,
        ReportKind::PromotionDeclaration => {
            let id = req.promotion_id.as_deref().unwrap_or_default();
            Built::plain(crate::promotions::declaration(conn, id, scope.lang)?)
        }
        ReportKind::VatByRate => tax::vat_by_rate(conn, &scope)?,
        ReportKind::VatDeductible => tax::vat_deductible(conn, &scope, pack)?,
        ReportKind::StampDutyStatement => tax::stamp_duty_statement(conn, &scope)?,
        ReportKind::G50 => tax::g50_table(conn, &scope, pack)?,
        ReportKind::IfuTurnover => tax::ifu_turnover(conn, &scope, pack)?,
        ReportKind::G9Suppliers => tax::g9(conn, &scope, pack)?,
        ReportKind::CustomersAnnual => tax::customers_annual(conn, &scope)?,
        ReportKind::SalesRegister => tax::sales_register(conn, &scope)?,
        ReportKind::PurchasesRegister => tax::purchases_register(conn, &scope)?,
        ReportKind::SalesByTable => packs::sales_by_table(conn, &scope)?,
        ReportKind::ControlledRegister => packs::controlled_register(conn, &scope)?,
        ReportKind::Waste => packs::waste(conn, &scope)?,
        ReportKind::YieldRuns => packs::yield_runs(conn, &scope)?,
        ReportKind::FlexySales => packs::flexy_sales(conn, &scope)?,
        ReportKind::ServiceTickets => packs::service_tickets(conn, &scope)?,
        ReportKind::UncollectedTickets => packs::uncollected_tickets(conn, &scope)?,
        ReportKind::InstalmentsOverdue => packs::instalments_overdue(conn, &scope)?,
        ReportKind::DeliveryRounds => packs::delivery_rounds(conn, &scope)?,
        ReportKind::LoyaltyPoints => packs::loyalty_points(conn, &scope)?,
        ReportKind::GiftVouchers => packs::gift_vouchers(conn, &scope)?,
        ReportKind::JewellerRegister => packs::jeweller_register(conn, &scope)?,
        ReportKind::JewellerySales => packs::jewellery_sales(conn, &scope)?,
        ReportKind::StaffCommissions => packs::staff_commissions(conn, &scope)?,
        ReportKind::StaffAdvances => packs::staff_advances(conn, &scope)?,
        ReportKind::PaymentTypes => packs::payment_types(conn, &scope)?,
        ReportKind::PersonalDataAccess => packs::personal_data_access(conn, &scope)?,
    };
    Ok(Report {
        kind: req.kind,
        from: scope.from,
        to: scope.to,
        table: built.table,
        chart: built.chart,
        truncated: built.truncated,
    })
}

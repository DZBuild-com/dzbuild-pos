//! The permission catalogue of spec §4.5. Strings are stable: they are stored in `roles.permissions`.

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "lowercase")]
pub enum PermissionGroup {
    Sales,
    Cash,
    Products,
    Stock,
    Purchases,
    Customers,
    Documents,
    Reports,
    Settings,
    Audit,
}

impl PermissionGroup {
    pub const ALL: [PermissionGroup; 10] = [
        PermissionGroup::Sales,
        PermissionGroup::Cash,
        PermissionGroup::Products,
        PermissionGroup::Stock,
        PermissionGroup::Purchases,
        PermissionGroup::Customers,
        PermissionGroup::Documents,
        PermissionGroup::Reports,
        PermissionGroup::Settings,
        PermissionGroup::Audit,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            PermissionGroup::Sales => "sales",
            PermissionGroup::Cash => "cash",
            PermissionGroup::Products => "products",
            PermissionGroup::Stock => "stock",
            PermissionGroup::Purchases => "purchases",
            PermissionGroup::Customers => "customers",
            PermissionGroup::Documents => "documents",
            PermissionGroup::Reports => "reports",
            PermissionGroup::Settings => "settings",
            PermissionGroup::Audit => "audit",
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type,
)]
pub enum Permission {
    #[serde(rename = "sales.create")]
    SalesCreate,
    #[serde(rename = "sales.discount")]
    SalesDiscount,
    #[serde(rename = "sales.price_override")]
    SalesPriceOverride,
    #[serde(rename = "sales.void_line")]
    SalesVoidLine,
    #[serde(rename = "sales.hold")]
    SalesHold,
    #[serde(rename = "sales.return")]
    SalesReturn,
    #[serde(rename = "sales.reprint")]
    SalesReprint,
    #[serde(rename = "sales.view_all")]
    SalesViewAll,
    #[serde(rename = "cash.open")]
    CashOpen,
    #[serde(rename = "cash.close")]
    CashClose,
    #[serde(rename = "cash.paid_in_out")]
    CashPaidInOut,
    #[serde(rename = "cash.view_expected")]
    CashViewExpected,
    #[serde(rename = "cash.view_history")]
    CashViewHistory,
    #[serde(rename = "products.view")]
    ProductsView,
    #[serde(rename = "products.edit")]
    ProductsEdit,
    #[serde(rename = "products.price")]
    ProductsPrice,
    #[serde(rename = "products.import")]
    ProductsImport,
    #[serde(rename = "products.labels")]
    ProductsLabels,
    #[serde(rename = "stock.view")]
    StockView,
    #[serde(rename = "stock.adjust")]
    StockAdjust,
    #[serde(rename = "stock.count")]
    StockCount,
    #[serde(rename = "stock.transfer")]
    StockTransfer,
    #[serde(rename = "purchases.view")]
    PurchasesView,
    #[serde(rename = "purchases.manage")]
    PurchasesManage,
    #[serde(rename = "suppliers.manage")]
    SuppliersManage,
    #[serde(rename = "customers.view")]
    CustomersView,
    #[serde(rename = "customers.manage")]
    CustomersManage,
    #[serde(rename = "credit.view")]
    CreditView,
    #[serde(rename = "credit.collect")]
    CreditCollect,
    #[serde(rename = "credit.limit_override")]
    CreditLimitOverride,
    /// Read and write the controlled-medicine register (health data, pharmacy pack only).
    #[serde(rename = "pharmacy.register")]
    PharmacyRegister,
    #[serde(rename = "documents.invoice")]
    DocumentsInvoice,
    #[serde(rename = "documents.quote")]
    DocumentsQuote,
    #[serde(rename = "documents.delivery")]
    DocumentsDelivery,
    #[serde(rename = "documents.credit_note")]
    DocumentsCreditNote,
    #[serde(rename = "reports.sales")]
    ReportsSales,
    #[serde(rename = "reports.stock")]
    ReportsStock,
    #[serde(rename = "reports.profit")]
    ReportsProfit,
    #[serde(rename = "reports.cash")]
    ReportsCash,
    #[serde(rename = "reports.tax")]
    ReportsTax,
    #[serde(rename = "settings.company")]
    SettingsCompany,
    #[serde(rename = "settings.users")]
    SettingsUsers,
    #[serde(rename = "settings.hardware")]
    SettingsHardware,
    #[serde(rename = "settings.data")]
    SettingsData,
    #[serde(rename = "settings.dzbuild")]
    SettingsDzbuild,
    #[serde(rename = "audit.view")]
    AuditView,
}

impl Permission {
    /// Spec order; the role editor shows them in this order.
    pub const ALL: [Permission; 46] = [
        Permission::SalesCreate,
        Permission::SalesDiscount,
        Permission::SalesPriceOverride,
        Permission::SalesVoidLine,
        Permission::SalesHold,
        Permission::SalesReturn,
        Permission::SalesReprint,
        Permission::SalesViewAll,
        Permission::CashOpen,
        Permission::CashClose,
        Permission::CashPaidInOut,
        Permission::CashViewExpected,
        Permission::CashViewHistory,
        Permission::ProductsView,
        Permission::ProductsEdit,
        Permission::ProductsPrice,
        Permission::ProductsImport,
        Permission::ProductsLabels,
        Permission::StockView,
        Permission::StockAdjust,
        Permission::StockCount,
        Permission::StockTransfer,
        Permission::PurchasesView,
        Permission::PurchasesManage,
        Permission::SuppliersManage,
        Permission::CustomersView,
        Permission::CustomersManage,
        Permission::CreditView,
        Permission::CreditCollect,
        Permission::CreditLimitOverride,
        Permission::PharmacyRegister,
        Permission::DocumentsInvoice,
        Permission::DocumentsQuote,
        Permission::DocumentsDelivery,
        Permission::DocumentsCreditNote,
        Permission::ReportsSales,
        Permission::ReportsStock,
        Permission::ReportsProfit,
        Permission::ReportsCash,
        Permission::ReportsTax,
        Permission::SettingsCompany,
        Permission::SettingsUsers,
        Permission::SettingsHardware,
        Permission::SettingsData,
        Permission::SettingsDzbuild,
        Permission::AuditView,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Permission::SalesCreate => "sales.create",
            Permission::SalesDiscount => "sales.discount",
            Permission::SalesPriceOverride => "sales.price_override",
            Permission::SalesVoidLine => "sales.void_line",
            Permission::SalesHold => "sales.hold",
            Permission::SalesReturn => "sales.return",
            Permission::SalesReprint => "sales.reprint",
            Permission::SalesViewAll => "sales.view_all",
            Permission::CashOpen => "cash.open",
            Permission::CashClose => "cash.close",
            Permission::CashPaidInOut => "cash.paid_in_out",
            Permission::CashViewExpected => "cash.view_expected",
            Permission::CashViewHistory => "cash.view_history",
            Permission::ProductsView => "products.view",
            Permission::ProductsEdit => "products.edit",
            Permission::ProductsPrice => "products.price",
            Permission::ProductsImport => "products.import",
            Permission::ProductsLabels => "products.labels",
            Permission::StockView => "stock.view",
            Permission::StockAdjust => "stock.adjust",
            Permission::StockCount => "stock.count",
            Permission::StockTransfer => "stock.transfer",
            Permission::PurchasesView => "purchases.view",
            Permission::PurchasesManage => "purchases.manage",
            Permission::SuppliersManage => "suppliers.manage",
            Permission::CustomersView => "customers.view",
            Permission::CustomersManage => "customers.manage",
            Permission::CreditView => "credit.view",
            Permission::CreditCollect => "credit.collect",
            Permission::CreditLimitOverride => "credit.limit_override",
            Permission::PharmacyRegister => "pharmacy.register",
            Permission::DocumentsInvoice => "documents.invoice",
            Permission::DocumentsQuote => "documents.quote",
            Permission::DocumentsDelivery => "documents.delivery",
            Permission::DocumentsCreditNote => "documents.credit_note",
            Permission::ReportsSales => "reports.sales",
            Permission::ReportsStock => "reports.stock",
            Permission::ReportsProfit => "reports.profit",
            Permission::ReportsCash => "reports.cash",
            Permission::ReportsTax => "reports.tax",
            Permission::SettingsCompany => "settings.company",
            Permission::SettingsUsers => "settings.users",
            Permission::SettingsHardware => "settings.hardware",
            Permission::SettingsData => "settings.data",
            Permission::SettingsDzbuild => "settings.dzbuild",
            Permission::AuditView => "audit.view",
        }
    }

    pub fn parse(s: &str) -> Option<Permission> {
        Permission::ALL.into_iter().find(|p| p.as_str() == s)
    }

    /// Position in `ALL`, used to keep stored lists in spec order.
    pub fn index(self) -> usize {
        Permission::ALL
            .iter()
            .position(|p| *p == self)
            .unwrap_or(usize::MAX)
    }

    /// Group shown in the role editor (suppliers → purchases, credit → customers). The match is
    /// exhaustive so that a new permission cannot compile without a group.
    pub const fn group(self) -> PermissionGroup {
        match self {
            Permission::SalesCreate
            | Permission::SalesDiscount
            | Permission::SalesPriceOverride
            | Permission::SalesVoidLine
            | Permission::SalesHold
            | Permission::SalesReturn
            | Permission::SalesReprint
            | Permission::SalesViewAll => PermissionGroup::Sales,
            Permission::CashOpen
            | Permission::CashClose
            | Permission::CashPaidInOut
            | Permission::CashViewExpected
            | Permission::CashViewHistory => PermissionGroup::Cash,
            Permission::ProductsView
            | Permission::ProductsEdit
            | Permission::ProductsPrice
            | Permission::ProductsImport
            | Permission::ProductsLabels => PermissionGroup::Products,
            Permission::StockView
            | Permission::StockAdjust
            | Permission::StockCount
            | Permission::StockTransfer => PermissionGroup::Stock,
            Permission::PurchasesView
            | Permission::PurchasesManage
            | Permission::SuppliersManage => PermissionGroup::Purchases,
            Permission::CustomersView
            | Permission::CustomersManage
            | Permission::CreditView
            | Permission::CreditCollect
            | Permission::CreditLimitOverride
            | Permission::PharmacyRegister => PermissionGroup::Customers,
            Permission::DocumentsInvoice
            | Permission::DocumentsQuote
            | Permission::DocumentsDelivery
            | Permission::DocumentsCreditNote => PermissionGroup::Documents,
            Permission::ReportsSales
            | Permission::ReportsStock
            | Permission::ReportsProfit
            | Permission::ReportsCash
            | Permission::ReportsTax => PermissionGroup::Reports,
            Permission::SettingsCompany
            | Permission::SettingsUsers
            | Permission::SettingsHardware
            | Permission::SettingsData
            | Permission::SettingsDzbuild => PermissionGroup::Settings,
            Permission::AuditView => PermissionGroup::Audit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn forty_six_unique_permissions_serialised_like_the_spec() {
        assert_eq!(Permission::ALL.len(), 46);
        let names: BTreeSet<&str> = Permission::ALL.iter().map(|p| p.as_str()).collect();
        assert_eq!(names.len(), 46);
        for p in Permission::ALL {
            assert_eq!(
                serde_json::to_string(&p).unwrap(),
                format!("\"{}\"", p.as_str())
            );
            assert_eq!(Permission::parse(p.as_str()), Some(p));
        }
        assert_eq!(Permission::parse("sales.fly"), None);
    }

    #[test]
    fn groups_match_the_spec_table() {
        let count = |g| Permission::ALL.iter().filter(|p| p.group() == g).count();
        assert_eq!(count(PermissionGroup::Sales), 8);
        assert_eq!(count(PermissionGroup::Cash), 5);
        assert_eq!(count(PermissionGroup::Products), 5);
        assert_eq!(count(PermissionGroup::Stock), 4);
        assert_eq!(count(PermissionGroup::Purchases), 3);
        assert_eq!(count(PermissionGroup::Customers), 6);
        assert_eq!(count(PermissionGroup::Documents), 4);
        assert_eq!(count(PermissionGroup::Reports), 5);
        assert_eq!(count(PermissionGroup::Settings), 5);
        assert_eq!(count(PermissionGroup::Audit), 1);
        assert_eq!(
            Permission::SuppliersManage.group(),
            PermissionGroup::Purchases
        );
        assert_eq!(
            Permission::CreditLimitOverride.group(),
            PermissionGroup::Customers
        );
        assert_eq!(
            Permission::PharmacyRegister.group(),
            PermissionGroup::Customers
        );
        assert_eq!(Permission::PharmacyRegister.index(), 30);
    }

    #[test]
    fn group_names_serialise_like_as_str_and_index_is_spec_order() {
        let names: BTreeSet<&str> = PermissionGroup::ALL.iter().map(|g| g.as_str()).collect();
        assert_eq!(names.len(), 10);
        for g in PermissionGroup::ALL {
            assert_eq!(
                serde_json::to_string(&g).unwrap(),
                format!("\"{}\"", g.as_str())
            );
        }
        for (position, p) in Permission::ALL.into_iter().enumerate() {
            assert_eq!(p.index(), position);
        }
    }
}

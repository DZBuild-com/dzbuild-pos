//! What each import accepts (IMX §38 matrix): the fields, which are required, which find the
//! existing record, and the closed lists. Labels are pos-docs' (`columns.<key>`).

use crate::auth::Permission;
use crate::catalog::price_levels;
use crate::error::AppError;
use crate::sqlenum::sql_text_enum;
use crate::table::TagDomain;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum ImportEntity {
    Products,
    Categories,
    Customers,
    Suppliers,
    OpeningStock,
    CustomerBalances,
    SupplierBalances,
    CountLines,
}

impl ImportEntity {
    pub const ALL: [ImportEntity; 8] = [
        ImportEntity::Products,
        ImportEntity::Categories,
        ImportEntity::Customers,
        ImportEntity::Suppliers,
        ImportEntity::OpeningStock,
        ImportEntity::CustomerBalances,
        ImportEntity::SupplierBalances,
        ImportEntity::CountLines,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            ImportEntity::Products => "products",
            ImportEntity::Categories => "categories",
            ImportEntity::Customers => "customers",
            ImportEntity::Suppliers => "suppliers",
            ImportEntity::OpeningStock => "opening_stock",
            ImportEntity::CustomerBalances => "customer_balances",
            ImportEntity::SupplierBalances => "supplier_balances",
            ImportEntity::CountLines => "count_lines",
        }
    }

    pub fn parse(s: &str) -> Option<ImportEntity> {
        ImportEntity::ALL.into_iter().find(|e| e.as_str() == s)
    }

    pub const fn permission(self) -> Permission {
        match self {
            ImportEntity::Products | ImportEntity::Categories | ImportEntity::OpeningStock => {
                Permission::ProductsImport
            }
            ImportEntity::Customers | ImportEntity::CustomerBalances => Permission::CustomersManage,
            ImportEntity::Suppliers | ImportEntity::SupplierBalances => Permission::SuppliersManage,
            ImportEntity::CountLines => Permission::StockCount,
        }
    }

    /// Imports that post money or stock (opening balances, opening stock, count lines) create
    /// ledger rows only: no update policy applies to them.
    pub const fn posts(self) -> bool {
        matches!(
            self,
            ImportEntity::OpeningStock
                | ImportEntity::CustomerBalances
                | ImportEntity::SupplierBalances
                | ImportEntity::CountLines
        )
    }

    /// The field within whose value a row may repeat an identifying value: opening stock loads
    /// one product into several locations, one row per location.
    pub const fn duplicate_scope(self) -> Option<&'static str> {
        match self {
            ImportEntity::OpeningStock => Some("location"),
            _ => None,
        }
    }
}

sql_text_enum!(ImportEntity);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ImportPolicy {
    /// New records only; a row matching an existing one is skipped and reported.
    #[default]
    CreateOnly,
    /// New records are created, matching ones updated (their previous state kept for undo).
    Upsert,
    /// Upsert, then archive the products absent from the file (products only).
    Sync,
}

impl ImportPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            ImportPolicy::CreateOnly => "create_only",
            ImportPolicy::Upsert => "upsert",
            ImportPolicy::Sync => "sync",
        }
    }

    pub fn parse(s: &str) -> Option<ImportPolicy> {
        [
            ImportPolicy::CreateOnly,
            ImportPolicy::Upsert,
            ImportPolicy::Sync,
        ]
        .into_iter()
        .find(|p| p.as_str() == s)
    }
}

sql_text_enum!(ImportPolicy);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    Text,
    Code,
    Money,
    Qty,
    Percent,
    Int,
    Bool,
    Date,
    List,
    Choice,
}

impl FieldKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            FieldKind::Text => "text",
            FieldKind::Code => "code",
            FieldKind::Money => "money",
            FieldKind::Qty => "qty",
            FieldKind::Percent => "percent",
            FieldKind::Int => "int",
            FieldKind::Bool => "bool",
            FieldKind::Date => "date",
            FieldKind::List => "list",
            FieldKind::Choice => "choice",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FieldSpec {
    pub key: String,
    pub kind: FieldKind,
    /// Every row must fill it.
    pub required: bool,
    /// Finds the existing record (reference, barcode, code, phone, a category name).
    pub identifies: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub choice: Option<TagDomain>,
}

impl FieldSpec {
    fn new(key: &str, kind: FieldKind) -> FieldSpec {
        FieldSpec {
            key: key.to_owned(),
            kind,
            required: false,
            identifies: false,
            choice: None,
        }
    }

    fn required(mut self) -> FieldSpec {
        self.required = true;
        self
    }

    fn identifies(mut self) -> FieldSpec {
        self.identifies = true;
        self
    }

    fn choice(mut self, domain: TagDomain) -> FieldSpec {
        self.choice = Some(domain);
        self
    }
}

/// One refusal of a dry run or an apply: the file row (1-based, as the spreadsheet numbers it),
/// the field, the value as read, and a code of `PROBLEM_CODES`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub row: u32,
    pub field: Option<String>,
    pub value: Option<String>,
    pub code: String,
}

pub const PROBLEM_CODES: [&str; 24] = [
    "required",
    "too_long",
    "number_invalid",
    "too_many_decimals",
    "ambiguous_separator",
    "negative",
    "date_invalid",
    "choice_invalid",
    "bool_invalid",
    "phone_invalid",
    "nif_invalid",
    "email_invalid",
    "not_found",
    "duplicate_in_file",
    "already_exists",
    "unit_not_found",
    "tax_rate_not_found",
    "location_not_found",
    "barcode_taken",
    "reference_taken",
    "phone_taken",
    "opening_already_set",
    "value_invalid",
    "rejected",
];

use FieldKind as K;

/// The fields of `entity` that need no database (every entity; products also get one price per
/// extra price level through `fields`).
pub fn base_fields(entity: ImportEntity) -> Vec<FieldSpec> {
    let f = FieldSpec::new;
    match entity {
        ImportEntity::Products => vec![
            f("reference", K::Code).identifies(),
            f("barcode", K::List).identifies(),
            f("nameFr", K::Text),
            f("nameAr", K::Text),
            f("nameEn", K::Text),
            f("category", K::Text),
            f("unit", K::Code),
            f("taxRate", K::Percent),
            f("sellPrice", K::Money),
            f("costPrice", K::Money),
            f("minStock", K::Qty),
            f("trackStock", K::Bool),
            f("brand", K::Text),
            f("supplierRef", K::Code),
        ],
        ImportEntity::Categories => vec![
            f("nameFr", K::Text).identifies(),
            f("nameAr", K::Text).identifies(),
            f("nameEn", K::Text).identifies(),
            f("parent", K::Text),
        ],
        ImportEntity::Customers => vec![
            f("code", K::Code).identifies(),
            f("name", K::Text).required(),
            f("customerType", K::Choice).choice(TagDomain::CustomerType),
            f("phone", K::Code).identifies(),
            f("phone2", K::Code),
            f("email", K::Text),
            f("address", K::Text),
            f("wilaya", K::Code),
            f("nif", K::Code),
            f("nis", K::Code),
            f("rc", K::Code),
            f("ai", K::Code),
            f("creditLimit", K::Money),
            f("discount", K::Percent),
            f("notes", K::Text),
        ],
        ImportEntity::Suppliers => vec![
            f("code", K::Code).identifies(),
            f("name", K::Text).required(),
            f("phone", K::Code),
            f("email", K::Text),
            f("address", K::Text),
            f("wilaya", K::Code),
            f("nif", K::Code),
            f("nis", K::Code),
            f("rc", K::Code),
            f("ai", K::Code),
            f("rib", K::Code),
            f("paymentTermsDays", K::Int),
            f("notes", K::Text),
        ],
        ImportEntity::OpeningStock => vec![
            f("reference", K::Code).identifies(),
            f("barcode", K::Code).identifies(),
            f("qty", K::Qty).required(),
            f("unitCost", K::Money),
            f("location", K::Code),
        ],
        ImportEntity::CustomerBalances => vec![
            f("code", K::Code).identifies(),
            f("phone", K::Code).identifies(),
            f("name", K::Text),
            f("amount", K::Money).required(),
            f("note", K::Text),
        ],
        ImportEntity::SupplierBalances => vec![
            f("code", K::Code).identifies(),
            f("name", K::Text),
            f("amount", K::Money).required(),
            f("note", K::Text),
        ],
        ImportEntity::CountLines => vec![
            f("reference", K::Code).identifies(),
            f("barcode", K::Code).identifies(),
            f("counted", K::Qty).required(),
        ],
    }
}

/// `base_fields` plus, for products, one `price.<CODE>` column per active price level other than
/// the default one (whose price is `sellPrice`).
pub fn fields(conn: &Connection, entity: ImportEntity) -> Result<Vec<FieldSpec>, AppError> {
    let mut out = base_fields(entity);
    if entity == ImportEntity::Products {
        for level in price_levels::list(conn)? {
            if !level.is_default && !level.archived {
                out.push(FieldSpec::new(&format!("price.{}", level.code), K::Money));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::shop;

    #[test]
    fn product_fields_add_one_price_per_extra_level() {
        let shop = shop();
        let fields = shop.db.read(|c| fields(c, ImportEntity::Products)).unwrap();
        let keys: Vec<&str> = fields.iter().map(|f| f.key.as_str()).collect();
        assert!(keys.starts_with(&["reference", "barcode", "nameFr", "nameAr", "nameEn"]));
        // The codes seeded for `WHOLESALE_LEVEL_ID` and the default `RETAIL_LEVEL_ID`, verbatim.
        assert!(keys.contains(&"price.wholesale"), "{keys:?}");
        assert!(
            !keys.contains(&"price.retail"),
            "the default level is the selling price"
        );
        assert!(
            fields
                .iter()
                .filter(|f| f.identifies)
                .all(|f| ["reference", "barcode"].contains(&f.key.as_str()))
        );
    }

    #[test]
    fn every_entity_has_an_identifying_or_required_field_and_a_permission() {
        for entity in ImportEntity::ALL {
            let fields = base_fields(entity);
            assert!(
                fields.iter().any(|f| f.identifies || f.required),
                "{}",
                entity.as_str()
            );
            assert!(!entity.permission().as_str().is_empty());
        }
        assert_eq!(
            ImportEntity::CountLines.permission(),
            crate::auth::Permission::StockCount
        );
    }
}

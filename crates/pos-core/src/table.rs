//! Generic tables: the one shape every report and every exportable list returns (spec §12
//! "every table view exports its current rows"). Column keys are stable camelCase identifiers;
//! their labels live in `crates/pos-docs/locales` (`columns.<key>`), so the xlsx/csv writer and
//! the screens show the same words. Money cells are centimes, quantities milli-units, percents
//! basis points, dates `YYYY-MM-DD`, instants unix milliseconds.

use crate::error::AppError;
use crate::money::{Money, Qty, Rate};
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(untagged)]
pub enum Cell {
    // `()` exports as `null` (a unit variant would export as the string "Null").
    Null(()),
    Int(i64),
    Text(String),
}

impl Cell {
    pub const NULL: Cell = Cell::Null(());

    pub fn text(value: impl Into<String>) -> Cell {
        Cell::Text(value.into())
    }

    pub fn opt_text(value: Option<String>) -> Cell {
        value.map_or(Cell::NULL, Cell::Text)
    }

    pub fn int(value: i64) -> Cell {
        Cell::Int(value)
    }

    pub fn money(value: Money) -> Cell {
        Cell::Int(value.0)
    }

    pub fn qty(value: Qty) -> Cell {
        Cell::Int(value.0)
    }

    pub fn rate(value: Rate) -> Cell {
        Cell::Int(i64::from(value.0))
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Cell::Int(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Cell::Text(v) => Some(v),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ColumnKind {
    Text,
    /// Numbers that are identifiers (barcodes, NIF, phone): text, left-to-right, zeros kept.
    Code,
    Money,
    Qty,
    Int,
    /// Basis points.
    Percent,
    Date,
    DateTime,
    /// A code of a closed list, translated through `tags.<domain>.<value>`.
    Tag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TagDomain {
    PaymentMethod,
    DocType,
    EntryKind,
    MovementReason,
    CashMovementKind,
    ReturnReason,
    ExpiryStatus,
    OrderStatus,
    CountStatus,
    CustomerType,
    Campaign,
    PnlLine,
    Metric,
    ExpenseGroup,
    /// `yes`/`no`, for boolean columns of exports.
    YesNo,
    /// A line of the G50 helper (`caHt9` … `stampDuty`, `tvaDeductible`, `tvaDue`, `tvaCredit`).
    G50Line,
    /// Why a supplier invoice's TVA is not deductible (CTCA art. 30).
    DeductionReason,
    /// Where a document stands in the archive (`billing::state::DocState::TAGS`).
    DocState,
    /// How a restaurant ticket was served: `dine_in`, `takeaway`, `delivery`.
    DiningMode,
    /// Why stock was adjusted by hand (`stock::AdjustReason`): the waste log's reasons.
    AdjustReason,
    /// Where a service ticket stands (`service_tickets::ServiceStatus`).
    ServiceStatus,
    /// What a service ticket is for: `repair`, `pressing`, `tailoring`, `other`.
    ServiceKind,
    /// Where a delivery round stands: `planned`, `out`, `closed`.
    RoundStatus,
    /// How a round's stop went: `pending`, `delivered`, `failed`.
    StopStatus,
    /// What a voucher is: `gift`, `exchange`, `loyalty`.
    VoucherKind,
    /// Where a voucher stands: `active`, `spent`, `expired`.
    VoucherStatus,
    /// A precious metal: `gold`, `silver`, `platinum`.
    Metal,
    /// A buyer's identity document: `cni`, `passport`, `driving_licence`, `other`.
    IdKind,
    /// A row of an employee's advances ledger: `advance`, `repayment`, `salary_deduction`,
    /// `adjustment`.
    StaffEntryKind,
    /// An action of the audit journal (`privacy.accessed`…), labelled as the journal screen
    /// labels it.
    AuditAction,
    /// Why a customer file was opened (`view`, `export`, `print`) or the channel of a consent
    /// (`sms`, `whatsapp`, `loyalty`).
    PrivacyPurpose,
}

impl TagDomain {
    pub const fn as_str(self) -> &'static str {
        match self {
            TagDomain::PaymentMethod => "payment_method",
            TagDomain::DocType => "doc_type",
            TagDomain::EntryKind => "entry_kind",
            TagDomain::MovementReason => "movement_reason",
            TagDomain::CashMovementKind => "cash_movement_kind",
            TagDomain::ReturnReason => "return_reason",
            TagDomain::ExpiryStatus => "expiry_status",
            TagDomain::OrderStatus => "order_status",
            TagDomain::CountStatus => "count_status",
            TagDomain::CustomerType => "customer_type",
            TagDomain::Campaign => "campaign",
            TagDomain::PnlLine => "pnl_line",
            TagDomain::Metric => "metric",
            TagDomain::ExpenseGroup => "expense_group",
            TagDomain::YesNo => "yes_no",
            TagDomain::G50Line => "g50_line",
            TagDomain::DeductionReason => "deduction_reason",
            TagDomain::DocState => "doc_state",
            TagDomain::DiningMode => "dining_mode",
            TagDomain::AdjustReason => "adjust_reason",
            TagDomain::ServiceStatus => "service_status",
            TagDomain::ServiceKind => "service_kind",
            TagDomain::RoundStatus => "round_status",
            TagDomain::StopStatus => "stop_status",
            TagDomain::VoucherKind => "voucher_kind",
            TagDomain::VoucherStatus => "voucher_status",
            TagDomain::Metal => "metal",
            TagDomain::IdKind => "id_kind",
            TagDomain::StaffEntryKind => "staff_entry_kind",
            TagDomain::AuditAction => "audit_action",
            TagDomain::PrivacyPurpose => "privacy_purpose",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TableColumn {
    pub key: String,
    pub kind: ColumnKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub tag: Option<TagDomain>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Table {
    pub columns: Vec<TableColumn>,
    pub rows: Vec<Vec<Cell>>,
    pub totals: Option<Vec<Cell>>,
}

impl Table {
    pub fn new(columns: &[(&str, ColumnKind)]) -> Table {
        Table {
            columns: columns
                .iter()
                .map(|(key, kind)| TableColumn {
                    key: (*key).to_owned(),
                    kind: *kind,
                    tag: None,
                })
                .collect(),
            rows: Vec::new(),
            totals: None,
        }
    }

    /// Marks the `Tag` column `key` as a code of `domain`.
    pub fn tag(mut self, key: &str, domain: TagDomain) -> Table {
        if let Some(column) = self.columns.iter_mut().find(|c| c.key == key) {
            column.tag = Some(domain);
        }
        self
    }

    pub fn push(&mut self, row: Vec<Cell>) -> Result<(), AppError> {
        if row.len() != self.columns.len() {
            return Err(AppError::Internal(format!(
                "a row of {} cells in a table of {} columns",
                row.len(),
                self.columns.len()
            )));
        }
        self.rows.push(row);
        Ok(())
    }

    pub fn set_totals(&mut self, row: Vec<Cell>) -> Result<(), AppError> {
        if row.len() != self.columns.len() {
            return Err(AppError::Internal("totals do not fit the columns".into()));
        }
        self.totals = Some(row);
        Ok(())
    }

    pub fn column(&self, key: &str) -> Option<usize> {
        self.columns.iter().position(|c| c.key == key)
    }

    /// Sum of an integer column (saturating: report totals never panic).
    pub fn money_total(&self, key: &str) -> Money {
        let Some(i) = self.column(key) else {
            return Money::ZERO;
        };
        Money(
            self.rows
                .iter()
                .filter_map(|r| r.get(i).and_then(Cell::as_i64))
                .fold(0_i64, i64::saturating_add),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::{Money, Qty, Rate};
    use serde_json::json;

    #[test]
    fn cells_serialise_as_plain_json_values() {
        let row = vec![
            Cell::text("Lait"),
            Cell::money(Money(15_000)),
            Cell::qty(Qty(2_500)),
            Cell::rate(Rate(1_900)),
            Cell::NULL,
            Cell::opt_text(None),
        ];
        assert_eq!(
            serde_json::to_value(&row).unwrap(),
            json!(["Lait", 15000, 2500, 1900, null, null])
        );
    }

    #[test]
    fn rows_must_fit_the_columns_and_totals_add_up() {
        let mut t = Table::new(&[("product", ColumnKind::Text), ("netTtc", ColumnKind::Money)]);
        t.push(vec![Cell::text("Lait"), Cell::money(Money(30_000))])
            .unwrap();
        t.push(vec![Cell::text("Huile"), Cell::money(Money(65_000))])
            .unwrap();
        assert_eq!(
            t.push(vec![Cell::text("court")]).unwrap_err().code(),
            "internal_error"
        );
        assert_eq!(t.money_total("netTtc"), Money(95_000));
        assert_eq!(t.column("netTtc"), Some(1));
        let tagged =
            Table::new(&[("method", ColumnKind::Tag)]).tag("method", TagDomain::PaymentMethod);
        assert_eq!(
            serde_json::to_value(&tagged.columns[0]).unwrap(),
            json!({ "key": "method", "kind": "tag", "tag": "payment_method" })
        );
    }
}

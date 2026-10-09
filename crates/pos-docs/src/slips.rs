//! Language-independent models of the pack slips printed on the thermal printers (the layouts are
//! `pos-hw::slips`, Task 18): kitchen and cancellation tickets, the addition, the intake slip of a
//! service ticket, the deposit receipt and the voucher. Every word comes from the document catalogues
//! (`slip.*`), numbers stay Western digits.

use crate::receipt::{label, method_label};
use pos_core::deposits::Deposit;
use pos_core::gift_vouchers::{Voucher, VoucherKind};
use pos_core::lang::Lang;
use pos_core::money::Money;
use pos_core::restaurant::bill::PreBill;
use pos_core::restaurant::kitchen::{KitchenKind, KitchenTicket};
use pos_core::restaurant::tickets::DiningMode;
use pos_core::service_tickets::{ServiceKind, ServiceTicket};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KitchenSlipLine {
    /// "2" or "1.5" (kg): the quantity as the cook reads it.
    pub qty: String,
    pub name: String,
    pub note: Option<String>,
    pub seat: Option<String>,
    pub course: Option<String>,
    pub options: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KitchenSlip {
    pub lang: Lang,
    pub rtl: bool,
    pub title: String,
    pub cancel: bool,
    pub station: Option<String>,
    /// "Table T4", "À emporter n° 12", "Comptoir".
    pub place: String,
    pub order_no: Option<i64>,
    pub covers: String,
    pub waiter: String,
    /// `HH:MM`, Algiers.
    pub time: String,
    pub seq: String,
    pub lines: Vec<KitchenSlipLine>,
}

fn qty_text(milli: i64) -> String {
    if milli % 1_000 == 0 {
        (milli / 1_000).to_string()
    } else {
        let s = format!("{}.{:03}", milli / 1_000, (milli % 1_000).abs());
        s.trim_end_matches('0').to_owned()
    }
}

fn place(
    lang: Lang,
    mode: DiningMode,
    table: Option<&str>,
    label_: Option<&str>,
    order_no: Option<i64>,
) -> String {
    match (mode, table, order_no) {
        (_, Some(t), _) => label(lang, "slip.table").replace("{table}", t),
        (DiningMode::Takeaway, _, Some(n)) => {
            label(lang, "slip.takeaway").replace("{n}", &n.to_string())
        }
        (DiningMode::Delivery, _, Some(n)) => {
            label(lang, "slip.delivery").replace("{n}", &n.to_string())
        }
        _ => label_.unwrap_or_default().to_owned(),
    }
}

pub fn kitchen_slip(k: &KitchenTicket, lang: Lang) -> KitchenSlip {
    let cancel = k.kind == KitchenKind::Cancel;
    let time = pos_core::time::algiers_datetime(k.created_at)
        .format("%H:%M")
        .to_string();
    KitchenSlip {
        lang,
        rtl: lang.is_rtl(),
        title: label(
            lang,
            if cancel {
                "slip.kitchen.cancel"
            } else {
                "slip.kitchen.title"
            },
        ),
        cancel,
        station: k.station_name.clone(),
        place: place(
            lang,
            k.mode,
            k.table_code.as_deref(),
            k.label.as_deref(),
            k.order_no,
        ),
        order_no: k.order_no,
        covers: label(lang, "slip.covers").replace("{n}", &k.covers.to_string()),
        waiter: k.waiter_name.clone(),
        time,
        seq: label(lang, "slip.kitchen.seq").replace("{n}", &k.seq.to_string()),
        lines: k
            .lines
            .iter()
            .map(|l| KitchenSlipLine {
                qty: qty_text(l.qty.0),
                name: l.name.clone(),
                note: l.note.clone(),
                seat: l
                    .seat
                    .map(|s| label(lang, "slip.seat").replace("{n}", &s.to_string())),
                course: l
                    .course
                    .map(|c| label(lang, "slip.course").replace("{n}", &c.to_string())),
                options: l.options.clone(),
            })
            .collect(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreBillLine {
    pub qty: String,
    pub name: String,
    pub total: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreBillSlip {
    pub lang: Lang,
    pub rtl: bool,
    pub shop: String,
    pub title: String,
    pub place: String,
    pub covers: String,
    pub waiter: String,
    pub lines: Vec<PreBillLine>,
    pub total_label: String,
    pub total: Money,
    pub cash_label: String,
    pub cash_due: Money,
    pub shares_label: Option<String>,
    pub shares: Vec<Money>,
    /// "Ce document n'est pas un ticket de caisse."
    pub notice: String,
}

pub fn prebill_slip(bill: &PreBill, shop: &str, lang: Lang) -> PreBillSlip {
    let t = &bill.ticket;
    PreBillSlip {
        lang,
        rtl: lang.is_rtl(),
        shop: shop.to_owned(),
        title: label(lang, "slip.prebill.title"),
        place: place(
            lang,
            t.mode,
            t.table_code.as_deref(),
            t.label.as_deref(),
            t.order_no,
        ),
        covers: label(lang, "slip.covers").replace("{n}", &t.covers.to_string()),
        waiter: t.waiter_name.clone(),
        lines: bill
            .priced
            .lines
            .iter()
            .map(|l| PreBillLine {
                qty: qty_text(l.qty.0),
                name: l.description.clone(),
                total: l.amounts.total_ttc,
            })
            .collect(),
        total_label: label(lang, "slip.prebill.total"),
        total: bill.priced.totals.total_ttc,
        cash_label: label(lang, "slip.prebill.cash"),
        cash_due: bill.cash_due,
        shares_label: (!bill.shares.is_empty()).then(|| {
            label(lang, "slip.prebill.shares").replace("{n}", &bill.shares.len().to_string())
        }),
        shares: bill.shares.clone(),
        notice: label(lang, "slip.prebill.notice"),
    }
}

/// The intake slip of a service ticket ("bon de réparation", وصل التصليح): the customer keeps it
/// to collect the item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntakeSlip {
    pub lang: Lang,
    pub rtl: bool,
    pub shop: String,
    pub title: String,
    pub number: String,
    /// `DD/MM/YYYY HH:MM`, Algiers.
    pub date: String,
    pub customer: String,
    /// (label, value): the item, brand and model, IMEI or serial number, accessories, condition,
    /// fault — the ones filled in.
    pub rows: Vec<(String, String)>,
    pub estimate_label: String,
    pub estimate: Option<Money>,
    pub promised_label: String,
    /// `DD/MM/YYYY`.
    pub promised: Option<String>,
    /// The shop's conditions (`repairs.conditions` in the slip's language).
    pub conditions: Option<String>,
    pub signature: String,
}

fn day_text(day: &str) -> String {
    chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .map_or_else(|_| day.to_owned(), |d| d.format("%d/%m/%Y").to_string())
}

pub fn intake_slip(
    ticket: &ServiceTicket,
    shop: &str,
    conditions: Option<&str>,
    lang: Lang,
    now: i64,
) -> IntakeSlip {
    let l = |key: &str| label(lang, key);
    let model = [ticket.brand.as_deref(), ticket.model.as_deref()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    let serial_label = |s: &str| {
        if s.len() == 15 && s.bytes().all(|b| b.is_ascii_digit()) {
            "IMEI".to_owned()
        } else {
            l("slip.intake.serial")
        }
    };
    let mut rows = vec![(l("slip.intake.item"), ticket.item.clone())];
    if !model.is_empty() {
        rows.push((l("slip.intake.model"), model));
    }
    if let Some(s) = &ticket.serial_no {
        rows.push((serial_label(s), s.clone()));
    }
    for (key, value) in [
        ("slip.intake.accessories", &ticket.accessories),
        ("slip.intake.condition", &ticket.condition),
    ] {
        if let Some(v) = value {
            rows.push((l(key), v.clone()));
        }
    }
    rows.push((l("slip.intake.fault"), ticket.fault.clone()));
    IntakeSlip {
        lang,
        rtl: lang.is_rtl(),
        shop: shop.to_owned(),
        title: l(if ticket.kind == ServiceKind::Repair {
            "slip.intake.title"
        } else {
            "slip.intake.service"
        }),
        number: ticket.number.clone(),
        date: pos_core::time::algiers_datetime(now)
            .format("%d/%m/%Y %H:%M")
            .to_string(),
        customer: ticket.customer_name.clone(),
        rows,
        estimate_label: l("slip.intake.estimate"),
        estimate: ticket.estimate,
        promised_label: l("slip.intake.promised"),
        promised: ticket.promised_on.as_deref().map(day_text),
        conditions: conditions
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .map(str::to_owned),
        signature: l("slip.intake.signature"),
    }
}

/// The receipt of a deposit (arrhes, `AC…`) with its droit de timbre.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DepositSlip {
    pub lang: Lang,
    pub rtl: bool,
    pub title: String,
    pub receipt_number: String,
    /// "Pour RP2026-000001": the order or the service ticket it is held for.
    pub owner_number: String,
    pub customer: Option<String>,
    pub method: Option<String>,
    pub amount: Money,
    pub stamp_label: String,
    pub stamp: Money,
    pub total_label: String,
    /// What was received: the amount and the duty charged.
    pub total: Money,
    pub held_label: String,
    /// Every deposit held for the owner so far, this one included.
    pub held: Money,
}

pub fn deposit_slip(
    deposit: &Deposit,
    owner_number: &str,
    customer: Option<&str>,
    held: Money,
    lang: Lang,
) -> DepositSlip {
    let l = |key: &str| label(lang, key);
    DepositSlip {
        lang,
        rtl: lang.is_rtl(),
        title: l("slip.deposit.title"),
        receipt_number: deposit.receipt_number.clone().unwrap_or_default(),
        owner_number: l("slip.deposit.for").replace("{number}", owner_number),
        customer: customer.map(str::to_owned),
        method: deposit.method.map(|m| method_label(lang, m)),
        amount: deposit.amount,
        stamp_label: l("slip.deposit.stamp"),
        stamp: deposit.stamp_duty,
        total_label: l("slip.deposit.total"),
        total: deposit
            .amount
            .checked_add(deposit.stamp_charged)
            .unwrap_or(deposit.amount),
        held_label: l("slip.deposit.held"),
        held,
    }
}

/// The shop printed on a slip that leaves the counter (a voucher is spent another day).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlipShop {
    pub name: String,
    pub address: Option<String>,
    pub phone: Option<String>,
}

impl SlipShop {
    /// A fixed shop for the slip tests of `pos-docs` and `pos-hw`.
    #[doc(hidden)]
    pub fn sample() -> SlipShop {
        SlipShop {
            name: "Supérette El Amel".into(),
            address: Some("12 rue Larbi Ben M'hidi, Alger".into()),
            phone: Some("023 45 67 89".into()),
        }
    }
}

/// A gift, exchange or loyalty voucher (`BA…`) as printed: its kind, amount, barcode and the
/// conditions with its expiry date.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoucherSlip {
    pub lang: Lang,
    pub rtl: bool,
    /// "Bon cadeau", "Bon d'échange" or "Chèque fidélité" in the slip's language.
    pub title: String,
    pub code: String,
    /// The EAN-13 printed as a barcode (the till scans it as the tender's reference).
    pub barcode: String,
    pub amount: Money,
    /// `YYYY-MM-DD`.
    pub expires_on: String,
    pub conditions: String,
    pub shop: SlipShop,
    /// A gift voucher sold for cash: the droit de timbre recorded on its `issue` row and what was
    /// received. `None` when no duty was due (paid by card, exchange and loyalty vouchers).
    pub duty: Option<VoucherSlipDuty>,
}

/// The duty lines of a gift voucher sold for cash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoucherSlipDuty {
    pub stamp_label: String,
    pub stamp_duty: Money,
    pub received_label: String,
    /// What the buyer handed over: the amount and the duty charged.
    pub received: Money,
}

pub fn voucher_slip(v: &Voucher, shop: &SlipShop, lang: Lang) -> VoucherSlip {
    let l = |key: &str| label(lang, key);
    let duty =
        (v.kind == VoucherKind::Gift && v.stamp_duty.is_positive()).then(|| VoucherSlipDuty {
            stamp_label: l("slip.voucher.duty"),
            stamp_duty: v.stamp_duty,
            received_label: l("slip.voucher.received"),
            received: v
                .initial_amount
                .checked_add(v.stamp_charged)
                .unwrap_or(v.initial_amount),
        });
    VoucherSlip {
        lang,
        rtl: lang.is_rtl(),
        title: l(match v.kind {
            VoucherKind::Gift => "slip.voucher.gift",
            VoucherKind::Exchange => "slip.voucher.exchange",
            VoucherKind::Loyalty => "slip.voucher.loyalty",
        }),
        code: v.code.clone(),
        barcode: v.barcode.clone(),
        amount: v.initial_amount,
        expires_on: v.expires_on.clone(),
        conditions: l("slip.voucher.conditions").replace("{date}", &v.expires_on),
        shop: shop.clone(),
        duty,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_core::money::Qty;
    use pos_core::restaurant::kitchen::{KitchenKind, KitchenLine, KitchenTicket};
    use pos_core::restaurant::tickets::DiningMode;

    fn ticket(kind: KitchenKind) -> KitchenTicket {
        KitchenTicket {
            id: "k1".into(),
            ticket_id: "t1".into(),
            station_id: Some("s1".into()),
            station_name: Some("Grill".into()),
            seq: 2,
            kind,
            lines: vec![KitchenLine {
                name: "Brochettes".into(),
                qty: Qty(2_000),
                unit_code: "pc".into(),
                note: Some("Bien cuit".into()),
                seat: Some(1),
                course: None,
                options: vec!["Frites".into()],
            }],
            mode: DiningMode::DineIn,
            order_no: None,
            table_code: Some("T4".into()),
            label: None,
            covers: 3,
            waiter_name: "Sofiane".into(),
            created_at: 1_790_593_200_000,
            printed_at: None,
        }
    }

    #[test]
    fn a_kitchen_slip_names_the_table_the_station_and_the_cancellation_in_each_language() {
        let fr = kitchen_slip(&ticket(KitchenKind::Order), Lang::Fr);
        assert_eq!(
            (
                fr.title.as_str(),
                fr.place.as_str(),
                fr.lines[0].qty.as_str()
            ),
            ("Bon de cuisine", "Table T4", "2")
        );
        let ar = kitchen_slip(&ticket(KitchenKind::Cancel), Lang::Ar);
        assert!(ar.rtl && ar.cancel);
        assert_eq!(ar.title, "إلغاء");
        let en = kitchen_slip(&ticket(KitchenKind::Order), Lang::En);
        assert_eq!(en.lines[0].options, ["Frites"]);
        assert_eq!(en.time, "12:00");
    }

    #[test]
    fn the_receipt_names_the_service_and_the_place_in_its_language() {
        use pos_core::restaurant::bill::DiningInfo;
        let table = DiningInfo {
            mode: DiningMode::DineIn,
            order_no: None,
            table_code: Some("T4".into()),
            label: None,
            covers: 3,
        };
        let fr = crate::receipt::dining(&table, Lang::Fr);
        assert_eq!(
            (fr.mode.as_str(), fr.place.as_deref(), fr.covers),
            ("Sur place", Some("Table T4"), 3)
        );
        let takeaway = DiningInfo {
            mode: DiningMode::Takeaway,
            order_no: Some(12),
            table_code: None,
            label: None,
            covers: 0,
        };
        let ar = crate::receipt::dining(&takeaway, Lang::Ar);
        assert_eq!(
            (ar.mode.as_str(), ar.order_no, ar.place),
            ("للأخذ", Some(12), None)
        );
        let slip = kitchen_slip(
            &KitchenTicket {
                mode: DiningMode::Takeaway,
                order_no: Some(12),
                table_code: None,
                ..ticket(KitchenKind::Order)
            },
            Lang::En,
        );
        assert_eq!(slip.place, "Takeaway no. 12");
    }

    #[test]
    fn the_intake_slip_prints_the_device_its_state_and_the_conditions_in_arabic() {
        let ticket = pos_core::service_tickets::tests_fixture();
        let slip = intake_slip(
            &ticket,
            "الأمل للهواتف",
            Some("كل جهاز لا يُستلم بعد 90 يوماً…"),
            Lang::Ar,
            1_790_593_200_000,
        );
        assert!(slip.rtl);
        assert_eq!(slip.title, "وصل التصليح");
        assert_eq!(slip.number, "RP2026-000001");
        assert!(
            slip.rows
                .iter()
                .any(|(k, v)| k == "IMEI" && v == "356938035643809")
        );
    }

    #[test]
    fn a_voucher_slip_names_its_kind_amount_and_expiry_in_each_language() {
        let v = pos_core::gift_vouchers::Voucher::sample();
        let fr = voucher_slip(&v, &SlipShop::sample(), Lang::Fr);
        assert_eq!(
            (fr.title.as_str(), fr.code.as_str(), fr.amount),
            ("Bon cadeau", "BA2026-000001", Money(500_000))
        );
        assert!(fr.conditions.contains("2027-09-28"));
        let ar = voucher_slip(&v, &SlipShop::sample(), Lang::Ar);
        assert_eq!((ar.title.as_str(), ar.rtl), ("قسيمة هدية", true));
    }

    #[test]
    fn a_gift_voucher_sold_for_cash_prints_its_duty_and_what_was_received() {
        use pos_core::gift_vouchers::{Voucher, VoucherKind};
        let v = Voucher::sample();
        assert_eq!(
            (v.stamp_duty, v.stamp_charged),
            (Money(5_000), Money(5_000))
        );
        let fr = voucher_slip(&v, &SlipShop::sample(), Lang::Fr);
        assert_eq!(
            fr.duty,
            Some(VoucherSlipDuty {
                stamp_label: "Droit de timbre".into(),
                stamp_duty: Money(5_000),
                received_label: "Total reçu".into(),
                received: Money(505_000),
            })
        );
        let ar = voucher_slip(&v, &SlipShop::sample(), Lang::Ar)
            .duty
            .unwrap();
        assert_eq!(
            (ar.stamp_label.as_str(), ar.received_label.as_str()),
            ("حق الطابع", "المبلغ المقبوض")
        );
        let en = voucher_slip(&v, &SlipShop::sample(), Lang::En)
            .duty
            .unwrap();
        assert_eq!(
            (en.stamp_label.as_str(), en.received_label.as_str()),
            ("Stamp duty", "Total received")
        );
        let by_card = Voucher {
            stamp_duty: Money::ZERO,
            stamp_charged: Money::ZERO,
            ..Voucher::sample()
        };
        assert_eq!(
            voucher_slip(&by_card, &SlipShop::sample(), Lang::Fr).duty,
            None,
            "no duty on a voucher paid by card"
        );
        let exchange = Voucher {
            kind: VoucherKind::Exchange,
            stamp_duty: Money::ZERO,
            stamp_charged: Money::ZERO,
            ..Voucher::sample()
        };
        assert_eq!(
            voucher_slip(&exchange, &SlipShop::sample(), Lang::Fr).duty,
            None,
            "no money received for an exchange voucher"
        );
    }

    #[test]
    fn an_exchange_voucher_and_the_points_line_read_in_english() {
        use pos_core::gift_vouchers::{Voucher, VoucherKind};
        let v = Voucher {
            kind: VoucherKind::Exchange,
            ..Voucher::sample()
        };
        let en = voucher_slip(&v, &SlipShop::sample(), Lang::En);
        assert_eq!(
            (en.title.as_str(), en.barcode.as_str()),
            ("Exchange voucher", "2982600000018")
        );
        let points = crate::receipt::ReceiptLoyalty {
            earned: 23,
            balance: 36,
            card: "2970000000015".into(),
        };
        assert_eq!(points.line(Lang::Fr), "Points fidélité : +23 — solde 36");
        assert_eq!(points.line(Lang::En), "Loyalty points: +23 — balance 36");
    }

    #[test]
    fn the_deposit_receipt_shows_the_duty_the_total_received_and_what_is_held() {
        use pos_core::deposits::{Deposit, DepositKind, OwnerType};
        use pos_core::payments::PaymentMethod;
        let deposit = Deposit {
            id: "d1".into(),
            owner_type: OwnerType::ServiceTicket,
            owner_id: "t1".into(),
            customer_id: None,
            kind: DepositKind::Deposit,
            amount: Money(500_000),
            method: Some(PaymentMethod::Cash),
            reference: None,
            stamp_duty: Money(5_000),
            stamp_charged: Money(5_000),
            receipt_number: Some("AC2026-000001".into()),
            document_id: None,
            business_date: "2026-09-28".into(),
            created_at: 1_790_593_200_000,
        };
        let fr = deposit_slip(
            &deposit,
            "RP2026-000001",
            Some("Nassim Ould Ali"),
            Money(700_000),
            Lang::Fr,
        );
        assert_eq!(
            (
                fr.title.as_str(),
                fr.owner_number.as_str(),
                fr.receipt_number.as_str()
            ),
            ("Reçu d'arrhes", "Pour RP2026-000001", "AC2026-000001")
        );
        assert_eq!(
            (fr.stamp, fr.total, fr.held),
            (Money(5_000), Money(505_000), Money(700_000))
        );
        assert_eq!(
            deposit_slip(&deposit, "RP2026-000001", None, Money(500_000), Lang::En).stamp_label,
            "Stamp duty"
        );
    }
}

//! Thermal layouts of the pack slips. Their words come from the language-independent models of
//! `pos_docs::slips`, already in the slip's language; Arabic is rastered by `render` like any RTL
//! layout. Kitchen and cancellation tickets (in large type, for the cook), the addition, the
//! repair intake, the deposit receipt and the vouchers; [`with_demo`] marks the demo shop's.

use crate::fmt::{amount, currency};
use crate::layout::{Align, Layout, Row};
use pos_core::lang::Lang;
use pos_core::money::Money;
use pos_docs::slips::{DepositSlip, IntakeSlip, KitchenSlip, PreBillSlip, SlipShop, VoucherSlip};

fn text(text: impl Into<String>, align: Align, large: bool, bold: bool) -> Row {
    Row::Text {
        text: text.into(),
        align,
        large,
        bold,
    }
}

fn money(value: Money, lang: Lang) -> String {
    format!("{} {}", amount(value, lang), currency(lang))
}

fn pair(start: impl Into<String>, end: impl Into<String>, bold: bool) -> Row {
    Row::Pair {
        start: start.into(),
        end: end.into(),
        large: false,
        bold,
    }
}

/// "0 couverts", "0 أشخاص", "0 covers": a takeaway, a delivery or the counter seats nobody.
fn seats_nobody(covers: &str) -> bool {
    covers.split_whitespace().next() == Some("0")
}

/// The non-empty parts joined by " — " ("Table T4 — 3 couverts").
fn joined<'a>(parts: impl IntoIterator<Item = &'a str>) -> String {
    parts
        .into_iter()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(" — ")
}

/// A slip printed from the demo shop's database says "DÉMO – sans valeur
/// fiscale" first, in large type and in the slip's language, like its receipts. `demo` is the
/// company's demo flag; the print path calls this on every pack slip it prints.
pub fn with_demo(mut layout: Layout, lang: Lang, demo: bool) -> Layout {
    if demo {
        layout.rows.insert(
            0,
            text(
                pos_docs::receipt::label(lang, "receipt.demo"),
                Align::Center,
                true,
                true,
            ),
        );
    }
    layout
}

/// RST-006: a kitchen or cancellation ticket for the cook, in large type: the title (a
/// cancellation stands out between asterisks), the station, the table or the takeaway number,
/// who serves how many covers, the time and the send, then each line "2 × Brochettes" with its
/// options, its note, its seat and its course indented under it.
pub fn kitchen_layout(s: &KitchenSlip) -> Layout {
    let title = if s.cancel {
        format!("*** {} ***", s.title)
    } else {
        s.title.clone()
    };
    let mut rows = vec![text(title, Align::Center, true, true)];
    if let Some(station) = &s.station {
        rows.push(text(station.clone(), Align::Center, true, false));
    }
    rows.push(text(s.place.clone(), Align::Center, true, true));
    let covers = (s.order_no.is_none() && !seats_nobody(&s.covers)).then_some(s.covers.as_str());
    rows.push(pair(
        joined([Some(s.waiter.as_str()), covers].into_iter().flatten()),
        format!("{} · {}", s.time, s.seq),
        false,
    ));
    rows.push(Row::Rule);
    for line in &s.lines {
        rows.push(text(
            format!("{} × {}", line.qty, line.name),
            Align::Start,
            true,
            true,
        ));
        for option in &line.options {
            rows.push(text(format!("   + {option}"), Align::Start, false, false));
        }
        if let Some(note) = &line.note {
            rows.push(text(format!("   ! {note}"), Align::Start, false, true));
        }
        let extra: Vec<&str> = [line.seat.as_deref(), line.course.as_deref()]
            .into_iter()
            .flatten()
            .collect();
        if !extra.is_empty() {
            rows.push(text(
                format!("   {}", extra.join(" · ")),
                Align::Start,
                false,
                false,
            ));
        }
    }
    rows.push(Row::Rule);
    rows.push(Row::Feed { lines: 3 });
    Layout { rtl: s.rtl, rows }
}

/// RST-007: the addition of a table: the shop, the place and its covers, the lines and their
/// totals, the total, what to pay in cash with the duty when it differs, the even shares when
/// the bill is split, and "Ce document n'est pas un ticket de caisse" to close it.
pub fn prebill_layout(s: &PreBillSlip) -> Layout {
    let lang = s.lang;
    let covers = (!seats_nobody(&s.covers)).then_some(s.covers.as_str());
    let mut rows = vec![
        text(s.shop.clone(), Align::Center, true, true),
        Row::Rule,
        text(s.title.clone(), Align::Center, true, true),
        text(
            joined([Some(s.place.as_str()), covers].into_iter().flatten()),
            Align::Center,
            false,
            false,
        ),
    ];
    if !s.waiter.trim().is_empty() {
        rows.push(text(s.waiter.clone(), Align::Center, false, false));
    }
    rows.push(Row::Rule);
    for line in &s.lines {
        rows.push(pair(
            format!("{} × {}", line.qty, line.name),
            amount(line.total, lang),
            false,
        ));
    }
    rows.push(Row::Rule);
    rows.push(Row::Pair {
        start: s.total_label.clone(),
        end: money(s.total, lang),
        large: true,
        bold: true,
    });
    if s.cash_due != s.total {
        rows.push(pair(s.cash_label.clone(), money(s.cash_due, lang), true));
    }
    if let Some(label) = &s.shares_label {
        rows.push(text(label.clone(), Align::Start, false, false));
        let n = s.shares.len();
        for (i, share) in s.shares.iter().enumerate() {
            rows.push(pair(
                format!("  {}/{n}", i + 1),
                amount(*share, lang),
                false,
            ));
        }
    }
    rows.push(Row::Rule);
    rows.push(text(s.notice.clone(), Align::Center, false, true));
    rows.push(Row::Feed { lines: 3 });
    Layout { rtl: s.rtl, rows }
}

/// ORD-005, ORD-006: the intake slip the customer keeps to collect the item: the ticket number in
/// large type, the date, the customer, the item, its make, serial or IMEI, accessories, condition
/// and fault, the estimate and the promised date, the shop's conditions, the signature, and the
/// ticket number as a Code 128 the counter scans at pickup.
pub fn intake_layout(s: &IntakeSlip) -> Layout {
    let lang = s.lang;
    let mut rows = vec![
        text(s.shop.clone(), Align::Center, true, true),
        Row::Rule,
        text(s.title.clone(), Align::Center, true, true),
        text(s.number.clone(), Align::Center, true, true),
        text(s.date.clone(), Align::Center, false, false),
        Row::Rule,
        text(s.customer.clone(), Align::Start, false, true),
    ];
    for (label, value) in &s.rows {
        rows.push(pair(label.clone(), value.clone(), false));
    }
    if let Some(estimate) = s.estimate {
        rows.push(pair(s.estimate_label.clone(), money(estimate, lang), true));
    }
    if let Some(day) = &s.promised {
        rows.push(pair(s.promised_label.clone(), day.clone(), true));
    }
    if let Some(conditions) = &s.conditions {
        rows.push(Row::Rule);
        rows.push(text(conditions.clone(), Align::Start, false, false));
    }
    rows.push(Row::Rule);
    rows.push(text(s.signature.clone(), Align::Start, false, false));
    rows.push(Row::Feed { lines: 3 });
    rows.push(Row::Barcode {
        data: s.number.clone(),
    });
    rows.push(Row::Feed { lines: 3 });
    Layout { rtl: s.rtl, rows }
}

/// The receipt of a deposit (`AC…`, CRD-007): the shop, its number in large type, the time it
/// was taken (`at`, `DD/MM/YYYY HH:MM`), what it is held for and the customer, the amount by its
/// payment method, the droit de timbre charged on its cash (never recomputed here), what was
/// received and every deposit held for that order or ticket so far.
pub fn deposit_layout(s: &DepositSlip, shop: &str, at: &str) -> Layout {
    let lang = s.lang;
    let label = |key: &str| pos_docs::receipt::label(lang, key);
    let mut rows = vec![
        text(shop, Align::Center, true, true),
        Row::Rule,
        text(s.title.clone(), Align::Center, true, true),
        text(s.receipt_number.clone(), Align::Center, true, true),
        pair(label("receipt.date"), at, false),
        text(s.owner_number.clone(), Align::Center, false, true),
    ];
    if let Some(customer) = &s.customer {
        rows.push(pair(label("receipt.customer"), customer.clone(), false));
    }
    rows.push(Row::Rule);
    rows.push(pair(
        s.method.clone().unwrap_or_default(),
        money(s.amount, lang),
        false,
    ));
    if s.stamp != Money::ZERO {
        rows.push(pair(s.stamp_label.clone(), money(s.stamp, lang), false));
    }
    rows.push(Row::Pair {
        start: s.total_label.clone(),
        end: money(s.total, lang),
        large: true,
        bold: true,
    });
    rows.push(Row::Rule);
    rows.push(pair(s.held_label.clone(), money(s.held, lang), true));
    rows.push(text(label("receipt.thanks"), Align::Center, false, true));
    rows.push(Row::Feed { lines: 3 });
    Layout { rtl: s.rtl, rows }
}

/// The shop a slip that leaves the counter names: a voucher is spent another day.
fn shop_rows(shop: &SlipShop) -> Vec<Row> {
    let mut rows = vec![text(shop.name.clone(), Align::Center, true, true)];
    for line in [&shop.address, &shop.phone].into_iter().flatten() {
        rows.push(text(line.clone(), Align::Center, false, false));
    }
    rows.push(Row::Rule);
    rows
}

/// A gift, exchange or loyalty voucher (`BA…`): its kind and amount in large type, its code, the
/// duty of a gift voucher sold for cash, the conditions with the expiry date, and the EAN-13 the
/// till scans as the tender's reference.
pub fn voucher_layout(s: &VoucherSlip) -> Layout {
    let mut rows = shop_rows(&s.shop);
    rows.push(text(s.title.clone(), Align::Center, true, true));
    rows.push(text(money(s.amount, s.lang), Align::Center, true, true));
    rows.push(text(s.code.clone(), Align::Center, false, true));
    if let Some(duty) = &s.duty {
        rows.push(Row::Rule);
        rows.push(Row::Pair {
            start: duty.stamp_label.clone(),
            end: money(duty.stamp_duty, s.lang),
            large: false,
            bold: false,
        });
        rows.push(Row::Pair {
            start: duty.received_label.clone(),
            end: money(duty.received, s.lang),
            large: false,
            bold: true,
        });
    }
    rows.push(Row::Rule);
    rows.push(text(s.conditions.clone(), Align::Start, false, false));
    rows.push(Row::Barcode {
        data: s.barcode.clone(),
    });
    rows.push(Row::Feed { lines: 3 });
    Layout { rtl: s.rtl, rows }
}

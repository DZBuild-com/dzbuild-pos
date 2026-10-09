//! The close of a delivery round previewed before it is booked (Task 28, VRT-100): what the
//! driver collected by method, the cash he hands in with the droit de timbre his customers pay on
//! it, the stops not delivered. Nothing is written.

use super::rounds::{self, RoundInput, RoundStatus, StopReport, StopStatus};
use super::tests::{bl, invoice_on_account, wholesaler_for_reports as wholesaler};
use crate::money::Money;
use crate::payments::PaymentMethod;
use crate::testkit::Shop;

fn report(
    seq: u32,
    status: StopStatus,
    collected: i64,
    method: Option<PaymentMethod>,
) -> StopReport {
    let transfer = method == Some(PaymentMethod::Transfer);
    StopReport {
        seq,
        status,
        collected: Money(collected),
        method,
        reference: transfer.then(|| "VIR-117".into()),
        cheque_bank: None,
        cheque_date: None,
        note: None,
    }
}

/// Rachid's round out with three stops: an invoice of 5 000 DA paid in cash, a delivery note of
/// 10 000 DA paid 3 000 DA by transfer, and a delivery note not delivered.
fn round_out(shop: &Shop) -> String {
    let flour = shop.product("Farine 25 kg", Money(250_000), &shop.tva9).id;
    shop.stock_in(&flour, 100_000);
    shop.complete_company();
    let amine = shop.b2b_customer("Épicerie Amine").id;
    let nour = shop.b2b_customer("Supérette Nour").id;
    let karim = shop.b2b_customer("Alimentation Karim").id;
    let docs = vec![
        invoice_on_account(shop, &amine, &flour, 2_000).id,
        bl(shop, &nour, &flour, 4_000).id,
        bl(shop, &karim, &flour, 1_000).id,
    ];
    let ctx = shop.owner_till();
    shop.db
        .write(move |tx| {
            let round = rounds::create(
                tx,
                &ctx,
                RoundInput {
                    driver_name: "Rachid".into(),
                    ..RoundInput::default()
                },
            )?;
            rounds::set_stops(tx, &ctx, &round.id, docs)?;
            rounds::start(tx, &ctx, &round.id)?;
            rounds::report_stop(
                tx,
                &ctx,
                &round.id,
                report(1, StopStatus::Delivered, 500_000, Some(PaymentMethod::Cash)),
            )?;
            rounds::report_stop(
                tx,
                &ctx,
                &round.id,
                report(
                    2,
                    StopStatus::Delivered,
                    300_000,
                    Some(PaymentMethod::Transfer),
                ),
            )?;
            rounds::report_stop(tx, &ctx, &round.id, report(3, StopStatus::Failed, 0, None))?;
            Ok(round.id)
        })
        .unwrap()
}

#[test]
fn a_rounds_close_is_previewed_with_the_cash_to_hand_in_and_nothing_written() {
    let shop = wholesaler();
    let id = round_out(&shop);
    let (ctx, round_id) = (shop.owner_till(), id.clone());
    let preview = shop
        .db
        .read(move |c| rounds::close_preview(c, &ctx, &round_id))
        .unwrap();
    assert_eq!(
        preview.collected,
        vec![
            (PaymentMethod::Cash, Money(500_000)),
            (PaymentMethod::Transfer, Money(300_000)),
        ],
        "by method, in the order of the route"
    );
    assert_eq!(
        preview.cash_to_hand_in,
        Money(505_000),
        "5 000 DA in cash and the 50 DA of droit de timbre the customer pays on it"
    );
    assert_eq!(preview.failed, 1);

    let round_id = id.clone();
    let round = shop.db.read(move |c| rounds::get(c, &round_id)).unwrap();
    assert_eq!(round.status, RoundStatus::Out, "a preview writes nothing");
    let (ctx, round_id) = (shop.owner_till(), id);
    let closed = shop
        .db
        .write(move |tx| rounds::close(tx, &ctx, &round_id))
        .unwrap();
    let booked = closed
        .versements
        .iter()
        .filter(|v| v.entry.method == Some(PaymentMethod::Cash))
        .map(|v| v.quote.to_receive.0)
        .sum::<i64>();
    assert_eq!(
        booked, preview.cash_to_hand_in.0,
        "the close books the cash the preview announced"
    );
}

#[test]
fn a_round_with_nothing_collected_hands_in_no_cash() {
    let shop = wholesaler();
    let ctx = shop.owner_till();
    let id = shop
        .db
        .write(move |tx| {
            let round = rounds::create(
                tx,
                &ctx,
                RoundInput {
                    driver_name: "Rachid".into(),
                    ..RoundInput::default()
                },
            )?;
            Ok(round.id)
        })
        .unwrap();
    let ctx = shop.owner_till();
    let preview = shop
        .db
        .read(move |c| rounds::close_preview(c, &ctx, &id))
        .unwrap();
    assert_eq!(
        (preview.collected, preview.cash_to_hand_in, preview.failed),
        (Vec::new(), Money::ZERO, 0)
    );
    let ctx = shop.owner_till();
    assert_eq!(
        shop.db
            .read(move |c| rounds::close_preview(c, &ctx, "nope"))
            .unwrap_err()
            .code(),
        "round_not_found"
    );
}

//! A replayed item carries the clock offset its till measured: the main PC keeps it next to the
//! item (`lan_inbox`) and on the till's row, in the transaction that raises the `clock_skew`
//! notice, so the tills list and the notice say the same at once (Task 25's two-hour clock).

use super::replay::Received;
use super::testkit::lan_pair;
use crate::money::Money;
use crate::testkit::NOON;

#[test]
fn a_replayed_item_records_the_tills_clock_offset_on_its_row() {
    let lan = lan_pair();
    let milk = lan.host.product("Lait", Money(10_000), &lan.host.tva19);
    lan.host.stock_in(&milk.id, 10_000);
    lan.pull();
    lan.open_till_session(0);
    lan.sell_on_till(&milk.id, 1_000, 10_500);
    let two_hours = 7_200_000;
    lan.push_with(Received {
        at: NOON - two_hours,
        skew_ms: Some(-two_hours),
    });
    let till = lan.till_terminal.id.clone();
    let tills = lan
        .host
        .db
        .read(move |c| super::tills::list(c, NOON - two_hours))
        .unwrap();
    let row = tills.iter().find(|t| t.terminal_id == till).unwrap();
    assert_eq!(row.skew_ms, Some(-two_hours));
    assert_eq!(
        lan.count_host("SELECT COUNT(*) FROM lan_notices WHERE kind = 'clock_skew'"),
        1
    );
}

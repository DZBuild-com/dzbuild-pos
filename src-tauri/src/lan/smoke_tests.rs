//! The LAN end to end on two PCs (Review Focus 1–3).

use super::two_pcs::{
    TILL_CODE, TwoPcs, cash_sale, close_approved, eventually, on_hand, tickets_of,
};
use crate::commands::{cash as cash_cmd, sales as sales_cmd};
use pos_core::money::{Money, Qty};
use pos_core::sync::notices::{self, NoticeKind};
use pos_core::sync::outbox::{self, ItemStatus};
use pos_core::sync::replay::{self, Received, Sender};
use pos_core::sync::status::LinkState;
use pos_core::sync::{SKEW_BLOCK_MS, journal, tills};
use pos_core::time::now_ms;

fn journal_intact(pcs: &TwoPcs) -> bool {
    let till = pcs.till().st.terminal.id.clone();
    pcs.host
        .st
        .db
        .read(move |c| journal::verify(c, &till))
        .is_ok_and(|r| r.broken == 0)
}

fn chains_intact(st: &crate::state::AppState) -> bool {
    st.db
        .read(pos_core::documents::verify_all)
        .is_ok_and(|all| all.iter().all(|c| c.first_broken_seq.is_none()))
}

#[test]
fn selling_continues_when_the_main_pc_is_down() {
    let pcs = TwoPcs::new();
    let till_id = pcs.till().st.terminal.id.clone();
    pcs.till().cashier();
    cash_cmd::open_session(&pcs.till().st, Money(1_000_000)).unwrap();

    // online: the ticket reaches the main PC within the cycle
    sales_cmd::create(&pcs.till().st, cash_sale(&pcs.milk.id, 2, 50_000)).unwrap();
    pcs.wait_sent();
    assert_eq!(tickets_of(&pcs.host.st, &till_id).len(), 1);

    // the main PC goes down; the till keeps selling on its own copy and numbering
    pcs.host_down();
    let offline = sales_cmd::create(&pcs.till().st, cash_sale(&pcs.milk.id, 3, 50_000)).unwrap();
    let number = offline.sale.document.number.clone().unwrap();
    assert!(number.contains(&format!("-{TILL_CODE}-")), "{number}");
    let status = pcs
        .till()
        .st
        .lan
        .status(&pcs.till().st)
        .unwrap()
        .client
        .unwrap();
    assert_eq!(status.link, LinkState::Offline);
    assert!(status.pending >= 1);

    // meanwhile the main PC's own cashier sells too
    pcs.host.cashier();
    cash_cmd::open_session(&pcs.host.st, Money(500_000)).unwrap();
    sales_cmd::create(&pcs.host.st, cash_sale(&pcs.milk.id, 1, 20_000)).unwrap();

    // back up: the till sends what it owes, then takes what the main PC did
    pcs.host_up();
    pcs.wait_sent();
    pcs.wait_caught_up();
    assert_eq!(
        tickets_of(&pcs.host.st, &till_id),
        tickets_of(&pcs.till().st, &till_id),
        "same numbers, same times, same hashes on both PCs"
    );
    let host_id = pcs.host.st.terminal.id.clone();
    assert_eq!(
        tickets_of(&pcs.till().st, &host_id),
        tickets_of(&pcs.host.st, &host_id)
    );
    assert_eq!(
        on_hand(&pcs.host.st, &pcs.milk.id),
        Qty(18_000),
        "24 − 2 − 3 − 1"
    );
    assert_eq!(on_hand(&pcs.till().st, &pcs.milk.id), Qty(18_000));

    // a Z on each PC; the main PC holds the till's, anchored in the till's journal
    let till_z = close_approved(pcs.till());
    let host_z = close_approved(&pcs.host);
    pcs.wait_sent();
    let closures = pcs
        .host
        .st
        .db
        .read({
            let till_id = till_id.clone();
            move |c| tills::closures(c, &till_id, 10)
        })
        .unwrap();
    assert_eq!(closures.len(), 1);
    assert_eq!(closures[0].z_number, till_z.z.z_number);
    assert_eq!(closures[0].offline_pending, None, "closed online");
    assert_eq!((till_z.z.summary.tickets, host_z.z.summary.tickets), (2, 1));
    assert!(journal_intact(&pcs));
    assert!(chains_intact(&pcs.host.st) && chains_intact(&pcs.till().st));
    let merged = pcs.host.st.db.read(journal::verify_merge).unwrap();
    assert_eq!(merged, None, "the host's merge chain is intact");
}

#[test]
fn a_till_killed_mid_replay_resends_and_the_host_keeps_one_copy() {
    let mut pcs = TwoPcs::new();
    let till_id = pcs.till().st.terminal.id.clone();
    pcs.till().cashier();
    cash_cmd::open_session(&pcs.till().st, Money(1_000_000)).unwrap();
    pcs.wait_sent();
    // the engine stops: nothing leaves the till for now
    pcs.till().st.lan.stop();
    for _ in 0..3 {
        sales_cmd::create(&pcs.till().st, cash_sale(&pcs.milk.id, 1, 20_000)).unwrap();
    }
    let items = pcs.till().st.db.read(|c| outbox::pending(c, 100)).unwrap();
    assert_eq!(items.len(), 3, "one item per sale");
    let seqs: Vec<i64> = items.iter().map(|i| i.local_seq).collect();

    // the main PC applies them … and the power goes on the till before it hears the answers
    let sender = Sender {
        terminal_id: till_id.clone(),
        code: TILL_CODE.into(),
    };
    let answers = replay::apply_batch(
        &pcs.host.st.db,
        &sender,
        items,
        &Received {
            at: now_ms(),
            skew_ms: Some(0),
        },
    )
    .unwrap();
    assert!(answers.iter().all(|a| a.status == ItemStatus::Applied));
    pcs.kill_till();

    // the till starts again, sends the same three items, and is told they are known
    pcs.start_till();
    pcs.wait_sent();
    assert_eq!(
        tickets_of(&pcs.host.st, &till_id).len(),
        3,
        "one copy of each ticket"
    );
    assert_eq!(
        on_hand(&pcs.host.st, &pcs.milk.id),
        Qty(21_000),
        "stock taken once per sale"
    );
    for seq in seqs {
        let item = pcs.till().st.db.read(move |c| outbox::get(c, seq)).unwrap();
        assert_eq!(
            item.status,
            outbox::OutboxStatus::Sent,
            "answered as already known"
        );
    }
    assert!(journal_intact(&pcs));
    assert!(chains_intact(&pcs.host.st));
}

#[test]
fn a_two_hour_skew_keeps_the_receipt_times_and_is_recorded_on_both_sides() {
    let pcs = TwoPcs::new();
    let till_id = pcs.till().st.terminal.id.clone();
    pcs.till().cashier();
    // the drawer opened while the clocks agreed
    cash_cmd::open_session(&pcs.till().st, Money(1_000_000)).unwrap();
    // then the main PC's clock is set two hours back: the till is two hours ahead of it
    pcs.host.st.shift_clock(-2 * 3_600_000);
    pcs.till().st.lan.sync_now();
    eventually("the till measured the difference", || {
        pcs.till()
            .st
            .lan
            .skew_ms()
            .is_some_and(|s| s < -SKEW_BLOCK_MS)
    });

    let before = now_ms();
    let sold = sales_cmd::create(&pcs.till().st, cash_sale(&pcs.milk.id, 1, 20_000)).unwrap();
    let at = sold.sale.document.validated_at.unwrap();
    assert!(at >= before, "the ticket keeps the till's own time");
    pcs.wait_sent();
    let id = sold.sale.document.id.clone();
    let copy = pcs
        .host
        .st
        .db
        .read(move |c| pos_core::documents::get(c, &id))
        .unwrap();
    assert_eq!(
        (copy.validated_at, copy.hash.clone()),
        (Some(at), sold.sale.document.hash.clone()),
        "the main PC never restamps a printed receipt"
    );

    // recorded on the main PC (the till's row, a notice) and on the till (its journal)
    let now = pcs.host.st.now();
    let till = pcs
        .host
        .st
        .db
        .read(move |c| tills::list(c, now))
        .unwrap()
        .into_iter()
        .find(|t| t.terminal_id == till_id)
        .unwrap();
    assert!(till.skew_ms.is_some_and(|s| s < -SKEW_BLOCK_MS));
    let open = pcs
        .host
        .st
        .db
        .read(|c| notices::list(c, false, 100))
        .unwrap();
    assert!(open.iter().any(|n| n.kind == NoticeKind::ClockSkew));
    let journaled: i64 = pcs
        .till()
        .st
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM audit_log WHERE action = 'lan.clock_skew'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(journaled, 1, "once per episode");

    // decision: a drawer does not open while the clock is this far off and the main PC answers
    close_approved(pcs.till());
    assert_eq!(
        cash_cmd::open_session(&pcs.till().st, Money(1_000_000))
            .unwrap_err()
            .code(),
        "lan_clock_skew"
    );
}

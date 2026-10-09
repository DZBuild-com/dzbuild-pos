//! What the main PC answers an item it can never record, and a till's journal that comes back
//! over rows it already sent (M5, Task 20). An item that
//! fails the same way at every attempt is refused for good — kept as `rejected` on the till,
//! with a notice on the main PC — instead of an error the till retries forever. A journal row
//! whose `seq` the main PC already holds for the till (the till went back to an older copy and
//! wrote again) cannot be stored next to the first one: the item is still recorded, the
//! journal notice is raised, and what the row was stays in the item's `lan.replayed` row.

use super::notices::{self, NoticeKind};
use super::outbox::{self, ItemResult, ItemStatus, WireItem};
use super::replay::{Received, apply_batch};
use super::testkit::{LanPair, lan_pair};
use super::tills;
use crate::cash::ManualKind;
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::payments::{PaymentInput, PaymentMethod};
use crate::sales::{Cart, CartLine, SaleInput, create_sale};
use crate::testkit::NOON;
use serde_json::{Value, json};

fn send(lan: &LanPair, items: Vec<WireItem>) -> Result<Vec<ItemResult>, crate::error::AppError> {
    apply_batch(
        &lan.host.db,
        &lan.sender(),
        items,
        &Received {
            at: NOON + 2_000,
            skew_ms: Some(0),
        },
    )
}

fn pending(lan: &LanPair) -> Vec<WireItem> {
    lan.till.read(|c| outbox::pending(c, 10)).unwrap()
}

fn open_notices(lan: &LanPair, kind: NoticeKind) -> Vec<notices::Notice> {
    lan.host
        .db
        .read(|c| notices::list(c, false, 50))
        .unwrap()
        .into_iter()
        .filter(|n| n.kind == kind)
        .collect()
}

/// A kridi sale on the till, recorded as its own item (the session is already on the main PC).
fn a_kridi_sale_to_send(lan: &LanPair) -> WireItem {
    let milk = lan.host.product("Lait", Money(10_000), &lan.host.tva19).id;
    lan.host.stock_in(&milk, 10_000);
    let karim = lan.host.customer("Karim").id;
    lan.pull();
    lan.open_till_session(0);
    assert!(lan.push().iter().all(|r| r.status == ItemStatus::Applied));
    let mut cart = Cart::new(Lang::Fr, vec![CartLine::product(&milk, Qty(1_000))]);
    cart.customer_id = Some(karim);
    let input = SaleInput {
        cart,
        payments: vec![PaymentInput {
            method: PaymentMethod::Credit,
            ..PaymentInput::cash(Money(10_000))
        }],
    };
    let ctx = crate::till::TillContext::new(lan.till_owner.clone(), lan.host.pack.clone(), NOON);
    lan.till
        .write(move |tx| create_sale(tx, &ctx, input))
        .unwrap();
    let mut items = pending(lan);
    assert_eq!(items.len(), 1, "the sale");
    items.remove(0)
}

/// The main PC's database was restored from before the customer existed: the till's kridi
/// entry names a customer it does not have. The same answer comes at every attempt, so the
/// item is refused for good and the owner is told, instead of the till retrying it forever.
#[test]
fn a_kridi_entry_for_a_customer_the_main_pc_lacks_is_refused_not_retried_forever() {
    let lan = lan_pair();
    let mut item = a_kridi_sale_to_send(&lan);
    let unknown = crate::ids::new_id();
    let mut named = 0;
    for row in &mut item.bundle.rows {
        if row.table == "customer_ledger" {
            row.image.insert("customer_id".into(), json!(unknown));
            named += 1;
        }
    }
    assert!(named > 0, "the sale wrote its kridi entry");
    let inbox_before = lan.count_host("SELECT COUNT(*) FROM lan_inbox");
    let results = send(&lan, vec![item])
        .expect("refused for good: an error here is retried by the till forever");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].status, ItemStatus::Rejected);
    assert_eq!(results[0].code.as_deref(), Some("replay_invalid"));
    let rejected = open_notices(&lan, NoticeKind::ReplayRejected);
    assert_eq!(rejected.len(), 1);
    assert_eq!(rejected[0].details["code"], "replay_invalid");
    assert_eq!(
        rejected[0].details["cause"], "record_not_found",
        "the main PC's notice says why"
    );
    assert_eq!(
        lan.count_host("SELECT COUNT(*) FROM lan_inbox"),
        inbox_before,
        "nothing of the item was recorded"
    );
    let journaled: Vec<String> = lan
        .host
        .db
        .read(|c| {
            let mut stmt = c.prepare(
                "SELECT payload FROM audit_log WHERE action = 'lan.replay_rejected' ORDER BY seq",
            )?;
            let rows = stmt
                .query_map([], |r| r.get(0))?
                .collect::<Result<Vec<String>, _>>()?;
            Ok(rows)
        })
        .unwrap();
    assert_eq!(
        journaled.len(),
        1,
        "the refusal is in the main PC's chained journal"
    );
    assert!(
        journaled[0].contains("record_not_found") && journaled[0].contains("\"terminal\":\"C2\""),
        "{}",
        journaled[0]
    );
}

/// A till's ticket whose content (here a line's description) no longer hashes to the hash the
/// till computed when it validated it: the main PC never gives it a fresh hash. The item is
/// refused with the cause named, journaled on both PCs, and no chain is "recomputed".
#[test]
fn a_ticket_that_no_longer_matches_its_own_hash_is_refused_not_rebased() {
    let lan = lan_pair();
    let milk = lan.host.product("Lait", Money(10_000), &lan.host.tva19).id;
    lan.host.stock_in(&milk, 10_000);
    lan.pull();
    lan.open_till_session(0);
    assert!(lan.push().iter().all(|r| r.status == ItemStatus::Applied));
    lan.sell_on_till(&milk, 1_000, 10_000);
    let mut item = pending(&lan).remove(0);
    let mut edited = 0;
    for row in &mut item.bundle.rows {
        if row.table == "document_lines" {
            row.image
                .insert("description".into(), json!("Lait (remplacé)"));
            edited += 1;
        }
    }
    assert!(edited > 0, "the sale has lines");
    let results = send(&lan, vec![item]).unwrap();
    assert_eq!(results[0].status, ItemStatus::Rejected);
    assert_eq!(results[0].code.as_deref(), Some("replay_invalid"));
    let rejected = open_notices(&lan, NoticeKind::ReplayRejected);
    assert_eq!(rejected.len(), 1);
    assert_eq!(rejected[0].details["cause"], "replay_document_mismatch");
    assert_eq!(rejected[0].details["documentNumber"], "TK2026-C2-000001");
    assert!(
        open_notices(&lan, NoticeKind::ChainRebased).is_empty(),
        "never recomputed"
    );
    assert_eq!(
        lan.count_host("SELECT COUNT(*) FROM documents WHERE series = 'C2'"),
        0,
        "nothing of the ticket was recorded"
    );
    assert_eq!(
        lan.count_host(
            "SELECT COUNT(*) FROM audit_log WHERE action = 'lan.replay_rejected'
             AND payload LIKE '%replay_document_mismatch%'"
        ),
        1
    );
}

/// The till went back to an older copy of its database and wrote again: its next journal row
/// carries a `seq` the main PC already holds for it, with another row. The main PC keeps the row
/// it has (`UNIQUE (terminal_id, seq)`), records the item, raises the journal notice, and keeps
/// what the other row was in the item's `lan.replayed` row.
#[test]
fn a_journal_row_over_a_seq_the_main_pc_holds_is_a_journal_notice_not_a_refusal() {
    let lan = lan_pair();
    lan.open_till_session(0);
    assert!(lan.push().iter().all(|r| r.status == ItemStatus::Applied));
    let till = lan.till_terminal.id.clone();
    let (held_seq, held_id): (i64, String) = lan
        .host
        .db
        .read(move |c| {
            Ok(c.query_row(
                "SELECT seq, id FROM lan_journal WHERE terminal_id = ?1 ORDER BY seq DESC LIMIT 1",
                [&till],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?)
        })
        .unwrap();
    let ctx = lan.till_ctx();
    lan.till
        .write(move |tx| {
            crate::cash::move_cash(tx, &ctx, ManualKind::PaidIn, Money(5_000), Some("Monnaie"))
                .map(drop)
        })
        .unwrap();
    let mut items = pending(&lan);
    assert_eq!(items.len(), 1, "the paid-in");
    let row = &mut items[0].bundle.audit[0];
    row.seq = held_seq;
    let (again_id, again_hash) = (row.id.clone(), row.hash.clone());
    assert_ne!(again_id, held_id);
    let journal_before = lan.count_host("SELECT COUNT(*) FROM lan_journal");

    let results = send(&lan, items).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(
        (results[0].status, results[0].code.as_deref()),
        (ItemStatus::Applied, None),
        "the paid-in is recorded"
    );
    assert_eq!(
        lan.count_host("SELECT COUNT(*) FROM cash_movements WHERE kind = 'paid_in'"),
        1
    );
    assert_eq!(
        lan.count_host("SELECT COUNT(*) FROM lan_journal"),
        journal_before,
        "the row the main PC holds at that seq stays the only one"
    );
    let broken = open_notices(&lan, NoticeKind::JournalBroken);
    assert_eq!(broken.len(), 1, "the journal notice");
    let replayed: Value = lan
        .host
        .db
        .read(|c| {
            let text: String = c.query_row(
                "SELECT payload FROM audit_log WHERE action = 'lan.replayed' ORDER BY seq DESC LIMIT 1",
                [],
                |r| r.get(0),
            )?;
            Ok(serde_json::from_str(&text)?)
        })
        .unwrap();
    assert_eq!(
        replayed["journalCollisions"],
        json!([{ "seq": held_seq, "id": again_id, "hash": again_hash }]),
        "what the row that could not be stored was"
    );
    let dashboard = lan.host.db.read(|c| tills::list(c, NOON)).unwrap();
    assert!(
        !dashboard[0].journal_intact,
        "the till's journal shows broken"
    );
}

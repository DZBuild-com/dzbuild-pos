//! LAN-010 end to end, three PCs: C2 and C3 sell on the main PC C1, C3 pulls, C1 is gone, C3
//! is promoted and C2 joins it again with what it kept. The new main PC already holds C2's rows
//! (through C1's feed), some in a later state: every kept item is answered as already applied,
//! C2's journal on C3 starts where C2's chain was, C2's new copy numbers its outbox from 1, and
//! no number C1 may have printed after C3's last pull is issued again.

use super::devices::{self, PairRequest};
use super::journal;
use super::outbox::{self, ItemStatus};
use super::replay::{Received, Sender, apply_batch};
use super::replica::{self, PendingReplica};
use super::role;
use super::snapshot;
use super::testkit::{HOST_ID, TillPc, add_till, lan_host, pull_into, push_items};
use crate::audit::Actor;
use crate::cash::{self, CloseInput};
use crate::db::{Db, DbOptions};
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::payments::PaymentInput;
use crate::sales::{Cart, CartLine, SaleInput, create_sale};
use crate::testkit::NOON;
use crate::till::TillContext;
use std::sync::Arc;

fn received() -> Received {
    Received {
        at: NOON + 1_000,
        skew_ms: Some(0),
    }
}

fn sender(till: &TillPc) -> Sender {
    Sender {
        terminal_id: till.terminal.id.clone(),
        code: till.terminal.code.clone(),
    }
}

fn ctx(till: &TillPc, owner: bool) -> TillContext {
    let session = if owner {
        till.owner.clone()
    } else {
        till.cashier.clone()
    };
    TillContext::new(session, Arc::new(crate::datapack::bundled().unwrap()), NOON)
}

fn open(till: &TillPc) {
    let ctx = ctx(till, false);
    till.db
        .write(move |tx| cash::open_session(tx, &ctx, Money(0)))
        .unwrap();
}

fn sell(db: &Db, ctx: &TillContext, product: &str) {
    let (ctx, product) = (ctx.clone(), product.to_owned());
    db.write(move |tx| {
        create_sale(
            tx,
            &ctx,
            SaleInput {
                cart: Cart::new(Lang::Fr, vec![CartLine::product(&product, Qty(1_000))]),
                payments: vec![PaymentInput::cash(Money(10_000))],
            },
        )
    })
    .unwrap();
}

fn close(till: &TillPc) {
    let ctx = ctx(till, true);
    till.db
        .write(move |tx| cash::close_session(tx, &ctx, CloseInput::default()))
        .unwrap();
}

fn count(db: &Db, sql: &'static str) -> i64 {
    db.read(move |c| Ok(c.query_row(sql, [], |r| r.get(0))?))
        .unwrap()
}

fn all_applied(results: &[outbox::ItemResult]) -> bool {
    results
        .iter()
        .all(|r| matches!(r.status, ItemStatus::Applied | ItemStatus::Duplicate))
}

#[test]
fn a_till_joins_a_promoted_till_again_with_what_it_kept() {
    let dir = tempfile::tempdir().unwrap();
    let c1 = lan_host(dir.path());
    let milk = c1.product("Lait", Money(10_000), &c1.tva19).id;
    c1.stock_in(&milk, 100_000);
    // The main PC sells too: its own series (C1) is one C3 could never continue safely.
    c1.open_till(0);
    sell(&c1.db, &c1.till(), &milk);
    let c2 = add_till(&c1, HOST_ID, dir.path(), "C2");
    let c3 = add_till(&c1, HOST_ID, dir.path(), "C3");
    for till in [&c2, &c3] {
        pull_into(&c1.db, &till.db, &till.terminal.id);
    }
    // C2: three tickets and a Z, then a new session.
    open(&c2);
    for _ in 0..3 {
        sell(&c2.db, &ctx(&c2, false), &milk);
    }
    close(&c2);
    open(&c2);
    assert!(all_applied(&push_items(
        &c1.db,
        HOST_ID,
        &c2.db,
        &sender(&c2),
        received()
    )));
    // C3 sells more than C2, so its outbox numbers run ahead of C2's.
    open(&c3);
    for _ in 0..8 {
        sell(&c3.db, &ctx(&c3, false), &milk);
    }
    assert!(all_applied(&push_items(
        &c1.db,
        HOST_ID,
        &c3.db,
        &sender(&c3),
        received()
    )));
    pull_into(&c1.db, &c3.db, &c3.terminal.id);
    // C2 sells once more; C1 records it, C3 never hears of it.
    sell(&c2.db, &ctx(&c2, false), &milk);
    assert!(all_applied(&push_items(
        &c1.db,
        HOST_ID,
        &c2.db,
        &sender(&c2),
        received()
    )));
    assert_eq!(
        count(
            &c3.db,
            "SELECT last_number FROM counters WHERE doc_type = 'ticket' AND series = 'C1'"
        ),
        1
    );

    // C1 is gone: C3 becomes the main PC.
    c3.db.set_write_hook(None);
    let actor = c3.owner.actor();
    let new_host = c3
        .db
        .write(move |tx| role::promote(tx, &actor, NOON))
        .unwrap();
    c3.db
        .set_write_hook(Some(Arc::new(super::feed::HostHook::new(Arc::new(|_| {})))));
    assert_eq!(
        count(
            &c3.db,
            "SELECT last_number FROM counters WHERE doc_type = 'ticket' AND series = 'C1'"
        ),
        1 + role::PROMOTION_MARGIN,
        "the old main PC's tickets jump ahead"
    );
    assert_eq!(
        count(
            &c3.db,
            "SELECT last_number FROM counters WHERE doc_type = 'ticket' AND series = 'C3'"
        ),
        8,
        "this PC's own series goes on"
    );
    let ticket_skips = c3
        .db
        .read(|c| crate::numbering::journaled_skips(c, "ticket", "C1"))
        .unwrap();
    assert_eq!(ticket_skips.len(), 1, "journaled with the promotion");

    // C2 joins C3: only C2 itself may take over the C2 series.
    let (c2_id, c2_code) = (c2.terminal.id.clone(), c2.terminal.code.clone());
    let request = move |terminal_id: Option<String>| PairRequest {
        code: "000000".into(),
        terminal_code: c2_code.clone(),
        terminal_name: "Caisse 2".into(),
        app_version: "0.7.0".into(),
        schema_version: crate::db::migrations::LATEST_VERSION,
        install_id: crate::ids::new_id(),
        terminal_id,
    };
    let stranger = request(None);
    assert_eq!(
        c3.db
            .write(move |tx| devices::register(
                tx,
                &Actor::default(),
                &stranger,
                crate::db::migrations::LATEST_VERSION,
                NOON
            ))
            .unwrap_err()
            .code(),
        "terminal_code_taken"
    );
    let proven = request(Some(c2_id.clone()));
    let paired = c3
        .db
        .write(move |tx| {
            devices::register(
                tx,
                &Actor::default(),
                &proven,
                crate::db::migrations::LATEST_VERSION,
                NOON,
            )
        })
        .unwrap();
    assert_eq!(paired.terminal.id, c2_id);
    let kept = c2.db.read(|c| outbox::kept_since(c, 0)).unwrap();
    let kept_count = kept.len();
    let results = apply_batch(&c3.db, &sender(&c2), kept, &received()).unwrap();
    assert_eq!(results.len(), kept_count, "every kept item answered");
    assert!(all_applied(&results), "{results:?}");
    assert_eq!(
        count(
            &c3.db,
            "SELECT COUNT(*) FROM documents WHERE series = 'C2' AND status = 'validated'"
        ),
        4,
        "the ticket C3 never heard of is recorded"
    );
    assert_eq!(
        count(
            &c3.db,
            "SELECT last_number FROM counters WHERE doc_type = 'ticket' AND series = 'C2'"
        ),
        4
    );
    assert!(
        count(
            &c3.db,
            "SELECT COUNT(*) FROM audit_log WHERE action = 'lan.replayed' AND payload LIKE '%\"resend\":true%'"
        ) >= 1,
        "the items C3 already held are journaled as resends"
    );
    let id = c2_id.clone();
    let report = c3.db.read(move |c| journal::verify(c, &id)).unwrap();
    assert_eq!(
        (report.broken, report.first_broken_seq),
        (0, None),
        "C2's journal starts where its chain was"
    );
    assert_eq!(
        count(
            &c3.db,
            "SELECT COUNT(*) FROM lan_notices WHERE kind = 'journal_broken'"
        ),
        0
    );

    // C2 gets C3's copy: a new outbox line, numbered from 1 and accepted in order.
    let file = snapshot::export(&c3.db, &dir.path().join("snapshots"), &c2_id, NOON).unwrap();
    {
        let copy = rusqlite::Connection::open(&file.path).unwrap();
        let seq: i64 = copy
            .query_row(
                "SELECT COUNT(*) FROM sqlite_sequence WHERE name = 'lan_outbox'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(seq, 0, "the copy carries no outbox sequence");
    }
    let id = c2_id.clone();
    assert_eq!(
        c3.db
            .read(move |c| Ok(c.query_row(
                "SELECT last_replayed_seq FROM lan_devices WHERE terminal_id = ?1",
                [&id],
                |r| r.get::<_, Option<i64>>(0)
            )?))
            .unwrap(),
        None,
        "a new copy starts a new outbox line"
    );
    let TillPc {
        db: old_c2,
        terminal: c2_terminal,
        cashier: c2_cashier,
        ..
    } = c2;
    old_c2.close().unwrap();
    let c2_dir = dir.path().join("c2-data");
    std::fs::create_dir_all(&c2_dir).unwrap();
    let c2_path = dir.path().join("C2.db");
    let pending = PendingReplica {
        seq: file.seq,
        host_id: new_host.clone(),
        host_name: "Supérette El Amel".into(),
        host_address: "127.0.0.1:7345".into(),
        terminal_id: c2_id.clone(),
        terminal_code: "C2".into(),
        terminal_name: "Caisse 2".into(),
        paired_at: NOON,
    };
    replica::stage(&c2_dir, &file.path, &pending).unwrap();
    replica::install_pending(&c2_dir, &c2_path, &c2_dir.join("backups"), NOON).unwrap();
    let new_c2 = Db::open(
        &c2_path,
        DbOptions {
            readers: 1,
            backup_dir: Some(c2_dir.join("backups")),
        },
    )
    .unwrap();
    replica::adopt(&new_c2, &c2_dir, &pending, NOON).unwrap();
    new_c2.set_write_hook(Some(Arc::new(outbox::OutboxHook::new(
        c2_id.clone(),
        Arc::new(|| {}),
    ))));
    let first: i64 = count(&new_c2, "SELECT MIN(local_seq) FROM lan_outbox");
    assert_eq!(first, 1, "numbered from 1");
    let c2_ctx = TillContext::new(
        c2_cashier,
        Arc::new(crate::datapack::bundled().unwrap()),
        NOON,
    );
    sell(&new_c2, &c2_ctx, &milk);
    let results = push_items(
        &c3.db,
        &new_host,
        &new_c2,
        &Sender {
            terminal_id: c2_terminal.id.clone(),
            code: c2_terminal.code.clone(),
        },
        received(),
    );
    assert!(!results.is_empty());
    assert!(all_applied(&results), "{results:?}");
    assert_eq!(
        count(
            &c3.db,
            "SELECT COUNT(*) FROM documents WHERE series = 'C2' AND status = 'validated'"
        ),
        5
    );
    let report = c3.db.read(move |c| journal::verify(c, &c2_id)).unwrap();
    assert_eq!((report.broken, report.first_broken_seq), (0, None));
}

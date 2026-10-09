//! A PC that already was a till joins a main PC again (`join(Some(db), ..)`): its copy is staged
//! only once the main PC has answered every kept outbox item (M5 design: the till's copy is
//! replaced only after its outbox has been replayed), so no sale is left behind and no ticket
//! number is issued twice.

use super::join::{JoinRequest, Joined, join};
use crate::testing::{self, Host, Till};
use pos_core::db::Db;
use pos_core::error::AppError;
use pos_core::sync::{outbox, replica};
use pos_core::time::now_ms;
use std::path::Path;
use std::sync::Arc;

fn sell(till: &Till, ctx: &pos_core::till::TillContext, product: &str) {
    let (ctx, product) = (ctx.clone(), product.to_owned());
    till.db
        .write(move |tx| {
            pos_core::sales::create_sale(
                tx,
                &ctx,
                pos_core::sales::SaleInput {
                    cart: pos_core::sales::Cart::new(
                        pos_core::lang::Lang::Fr,
                        vec![pos_core::sales::CartLine::product(
                            &product,
                            pos_core::money::Qty(1_000),
                        )],
                    ),
                    payments: vec![pos_core::payments::PaymentInput::cash(
                        pos_core::money::Money(10_500),
                    )],
                },
            )
        })
        .unwrap();
}

/// The till's outbox, in order.
fn outbox_seqs(db: &Db) -> Vec<i64> {
    db.read(|c| {
        let mut stmt = c.prepare("SELECT local_seq FROM lan_outbox ORDER BY local_seq")?;
        let rows = stmt
            .query_map([], |r| r.get(0))?
            .collect::<Result<Vec<i64>, _>>()?;
        Ok(rows)
    })
    .unwrap()
}

/// What the main PC recorded from `terminal_id`, in order.
fn inbox_seqs(host: &Host, terminal_id: &str) -> Vec<i64> {
    let terminal_id = terminal_id.to_owned();
    host.db
        .read(move |c| {
            let mut stmt = c.prepare(
                "SELECT local_seq FROM lan_inbox WHERE terminal_id = ?1 ORDER BY local_seq",
            )?;
            let rows = stmt
                .query_map([&terminal_id], |r| r.get(0))?
                .collect::<Result<Vec<i64>, _>>()?;
            Ok(rows)
        })
        .unwrap()
}

fn host_tickets(host: &Host) -> i64 {
    host.db
        .read(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM documents WHERE series = 'C2'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap()
}

/// The owner of the main PC lets C2 go and shows a new code: the same PC joins again as C2
/// with its own database (`existing`) and folder, like a till that re-joins after its token
/// was revoked.
fn rejoin(host: &Host, till: &Till, till_dir: &Path) -> Result<Joined, AppError> {
    let id = till.terminal.id.clone();
    host.db
        .write(move |tx| {
            pos_core::sync::devices::revoke(tx, &pos_core::audit::Actor::default(), &id, 0)
        })
        .unwrap();
    let code = host.desk.lock().unwrap().issue(now_ms()).unwrap().code;
    join(
        Some(till.db.clone()),
        till_dir,
        JoinRequest {
            address: host.base(),
            code,
            terminal_code: "C2".into(),
            terminal_name: "Caisse 2".into(),
            app_version: "0.5.0".into(),
            schema_version: pos_core::db::migrations::LATEST_VERSION,
            install_id: pos_core::ids::new_id(),
            former_terminal: None,
        },
        Arc::new(|_, _| {}),
    )
}

#[test]
fn a_former_till_sends_every_kept_item_before_its_copy_is_staged() {
    let dir = tempfile::tempdir().unwrap();
    let host = testing::host(dir.path());
    let milk = host.product("Lait", 10_000);
    let till = testing::till(dir.path(), &host);
    let till_dir = dir.path().join("till");
    let ctx = till.open_session();
    for _ in 0..49 {
        sell(&till, &ctx, &milk);
    }
    let kept = outbox_seqs(&till.db);
    assert!(
        kept.len() > 50,
        "more than one replay batch: {}",
        kept.len()
    );
    assert!(!replica::has_pending(&till_dir));

    let joined = rejoin(&host, &till, &till_dir).unwrap();

    assert_eq!(
        inbox_seqs(&host, &till.terminal.id),
        kept,
        "every kept item was answered by the main PC, in order"
    );
    assert_eq!(host_tickets(&host), 49, "every ticket reached the main PC");
    assert_eq!(joined.pending.terminal_id, till.terminal.id);
    assert!(replica::has_pending(&till_dir));
}

#[test]
fn a_former_till_whose_line_stopped_at_a_refused_item_does_not_join() {
    let dir = tempfile::tempdir().unwrap();
    let host = testing::host(dir.path());
    let milk = host.product("Lait", 10_000);
    let till = testing::till(dir.path(), &host);
    let till_dir = dir.path().join("till");
    let ctx = till.open_session();
    for _ in 0..3 {
        sell(&till, &ctx, &milk);
    }
    let kept = outbox_seqs(&till.db);
    assert!(kept.len() < 50, "one replay batch: {}", kept.len());
    // The previous main PC refused the first ticket: the items after it wait behind it.
    let refused = kept[2];
    till.db
        .write(move |tx| outbox::mark_rejected(tx, refused, "replay_invalid"))
        .unwrap();

    let error = rejoin(&host, &till, &till_dir).unwrap_err();

    assert_eq!(error.code(), "lan_outbox_not_empty");
    assert!(
        !replica::has_pending(&till_dir),
        "the till keeps its own copy"
    );
    assert!(
        inbox_seqs(&host, &till.terminal.id).is_empty(),
        "nothing was sent while the line is stopped"
    );
    assert_eq!(till.db.read(outbox::depth).unwrap().rejected, 1);
}

#[test]
fn a_gap_inside_a_replay_batch_fails_the_join_before_anything_is_staged() {
    let dir = tempfile::tempdir().unwrap();
    let host = testing::host(dir.path());
    let milk = host.product("Lait", 10_000);
    let till = testing::till(dir.path(), &host);
    let till_dir = dir.path().join("till");
    let ctx = till.open_session();
    for _ in 0..3 {
        sell(&till, &ctx, &milk);
    }
    let kept = outbox_seqs(&till.db);
    assert!(kept.len() < 50, "one replay batch: {}", kept.len());
    // An item sent more than 30 days ago by the till's clock of the time (it was wrong) is no
    // longer kept, so the kept items have a hole the main PC cannot step over.
    let (old, host_id) = (kept[2], host.config.host_id.clone());
    till.db
        .write(move |tx| {
            outbox::mark_sent(tx, old, &host_id, now_ms() - outbox::KEEP_SENT_MS - 60_000)
        })
        .unwrap();

    let error = rejoin(&host, &till, &till_dir).unwrap_err();

    assert_eq!(error.code(), "replay_out_of_order");
    assert!(
        !replica::has_pending(&till_dir),
        "nothing staged: the unsent tickets stay in the till's copy"
    );
    assert_eq!(
        inbox_seqs(&host, &till.terminal.id),
        kept[..2],
        "the main PC took the items before the hole and nothing after it"
    );
}

/// M6 review (finding 7): a till whose damaged copy was set aside joins from a new, empty
/// database; the terminal it was (kept in `local.json`) lets it take its own terminal and series
/// back, while a new PC asking for that code without it is refused.
#[test]
fn a_till_set_aside_takes_its_own_terminal_back_from_an_empty_database() {
    let dir = tempfile::tempdir().unwrap();
    let host = testing::host(dir.path());
    let till = testing::till(dir.path(), &host);
    let id = till.terminal.id.clone();
    host.db
        .write(move |tx| {
            pos_core::sync::devices::revoke(tx, &pos_core::audit::Actor::default(), &id, 0)
        })
        .unwrap();
    let request = |former_terminal: Option<String>| JoinRequest {
        address: host.base(),
        code: host.desk.lock().unwrap().issue(now_ms()).unwrap().code,
        terminal_code: "C2".into(),
        terminal_name: "Caisse 2".into(),
        app_version: "0.5.0".into(),
        schema_version: pos_core::db::migrations::LATEST_VERSION,
        install_id: pos_core::ids::new_id(),
        former_terminal,
    };
    let stranger = join(
        None,
        &dir.path().join("stranger"),
        request(None),
        Arc::new(|_, _| {}),
    );
    assert_eq!(
        stranger.map(|_| ()).unwrap_err().code(),
        "terminal_code_taken"
    );
    let joined = join(
        None,
        &dir.path().join("set-aside"),
        request(Some(till.terminal.id.clone())),
        Arc::new(|_, _| {}),
    )
    .unwrap();
    assert_eq!(joined.pending.terminal_id, till.terminal.id);
    assert_eq!(joined.pending.terminal_code, "C2");
}

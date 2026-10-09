//! A main PC (C1, a file database with its capture triggers and write hook) and one till (C2,
//! a copy of the main PC taken with `Db::snapshot_to`, then set up as M5's `replica::adopt`
//! does) for the core LAN tests. Transport is a direct call: `pull` hands feed pages to the
//! till; later tasks add `push` (Task 9). Test-only.

use super::apply::{ApplyReport, apply_page};
use super::capture::{CaptureSet, install};
use super::changelog::{self, LAN_OUTBOX_CONSUMER};
use super::feed::{self, HostHook, PAGE_ROWS};
use crate::auth::session::{Session, build_session};
use crate::company::{PriceMode, TaxRegime};
use crate::db::{Db, DbOptions, meta_set};
use crate::ids::new_id;
use crate::lang::Lang;
use crate::money::{Money, Qty};
use crate::payments::PaymentInput;
use crate::sales::{Cart, CartLine, SaleInput, ValidatedSale, create_sale};
use crate::terminal::{META_TERMINAL_ID, Terminal, TerminalRole};
use crate::testkit::{NOON, Shop, shop_in};
use crate::till::TillContext;
use rusqlite::params;
use std::sync::Arc;

pub(crate) const HOST_ID: &str = "00000000-0000-7000-8000-00000000a001";

pub(crate) struct LanPair {
    /// Holds the folder of both databases until the pair drops.
    #[allow(dead_code)]
    pub dir: tempfile::TempDir,
    pub host: Shop,
    pub till: Db,
    pub till_terminal: Terminal,
    pub till_cashier: Session,
    /// The owner signed in on the till (read by the tests of the later M5 tasks).
    #[allow(dead_code)]
    pub till_owner: Session,
}

fn options(dir: &std::path::Path, name: &str) -> DbOptions {
    DbOptions {
        readers: 2,
        backup_dir: Some(dir.join(name)),
    }
}

/// A till of a test main PC: its database (a copy of the main PC set up as `replica::adopt`
/// does) and its terminal, with the cashier and the owner signed in on it.
pub(crate) struct TillPc {
    pub db: Db,
    pub terminal: Terminal,
    pub cashier: Session,
    pub owner: Session,
}

/// Registers till `code` on `host` (journal anchored at the host's journal tail, as a snapshot
/// export does) and makes its database from a copy of the host's, at `<dir>/<code>.db`.
pub(crate) fn add_till(host: &Shop, host_id: &str, dir: &std::path::Path, code: &str) -> TillPc {
    let terminal = Terminal {
        id: new_id(),
        code: code.into(),
        name: format!("Caisse {code}"),
        role: TerminalRole::Client,
    };
    let t = terminal.clone();
    host.db
        .write(move |tx| {
            tx.execute(
                "INSERT INTO terminals (id, code, name, role, created_at) VALUES (?1, ?2, ?3, 'client', 0)",
                params![t.id, t.code, t.name],
            )?;
            feed::register_till(tx, &t.id, 0)?;
            tx.execute(
                "INSERT INTO lan_devices (terminal_id, name, token_hash, paired_at, journal_anchor_seq, journal_anchor_hash)
                 SELECT ?1, ?2, ?3, 0, COALESCE(MAX(seq), 0),
                        COALESCE((SELECT hash FROM audit_log ORDER BY seq DESC LIMIT 1), ?4)
                 FROM audit_log",
                params![
                    t.id,
                    t.name,
                    super::devices::hash_token(&format!("test-token-{}", t.code)),
                    crate::audit::GENESIS_HASH
                ],
            )?;
            Ok(())
        })
        .unwrap();
    let seq = host.db.read(feed::head).unwrap();
    let till_path = dir.join(format!("{code}.db"));
    host.db.snapshot_to(&till_path).unwrap();
    let till = Db::open(&till_path, options(dir, &format!("{code}-backups"))).unwrap();
    let (t, host_id) = (terminal.clone(), host_id.to_owned());
    till.write_replica(move |tx| {
        meta_set(tx, META_TERMINAL_ID, &t.id)?;
        // The main PC's own tables a real snapshot scrubs (`snapshot::SCRUB`): a till's copy
        // holds no tills' registry, so its own Z anchors into its own `audit_log`.
        tx.execute_batch(
            "DELETE FROM sync_cursors; DELETE FROM changelog; DELETE FROM lan_commits; DELETE FROM lan_devices;
             DELETE FROM lan_outbox; DELETE FROM sqlite_sequence WHERE name = 'lan_outbox';",
        )?;
        tx.execute(
            "UPDATE lan_local SET role = 'client', host_id = ?1, host_address = '127.0.0.1:7345',
                    host_name = 'Supérette El Amel', applied_seq = ?2, paired_at = 0, updated_at = 0",
            params![host_id, seq],
        )?;
        changelog::register(tx, LAN_OUTBOX_CONSUMER, 0)?;
        Ok(())
    })
    .unwrap();
    let id = terminal.id.clone();
    till.write(move |tx| install(tx, &CaptureSet::Client { terminal_id: id }))
        .unwrap();
    till.set_write_hook(Some(Arc::new(super::outbox::OutboxHook::new(
        terminal.id.clone(),
        Arc::new(|| {}),
    ))));
    let (cashier_id, owner_id, t) = (
        host.cashier.user_id.clone(),
        host.owner.user_id.clone(),
        terminal.clone(),
    );
    let (cashier, owner) = till
        .read(move |c| {
            Ok((
                build_session(c, &cashier_id, &t, NOON)?,
                build_session(c, &owner_id, &t, NOON)?,
            ))
        })
        .unwrap();
    TillPc {
        db: till,
        terminal,
        cashier,
        owner,
    }
}

/// Sends the pending items of `till` to the main PC `host` like the engine does, and records
/// the answers on the till.
pub(crate) fn push_items(
    host: &Db,
    host_id: &str,
    till: &Db,
    sender: &super::replay::Sender,
    received: super::replay::Received,
) -> Vec<super::outbox::ItemResult> {
    use super::outbox::{self, ItemStatus};
    let items = till.read(|c| outbox::pending(c, 100)).unwrap();
    if items.is_empty() {
        return Vec::new();
    }
    let results = super::replay::apply_batch(host, sender, items, &received).unwrap();
    for result in &results {
        let (seq, me, host_id) = (
            result.local_seq,
            sender.terminal_id.clone(),
            host_id.to_owned(),
        );
        match result.status {
            ItemStatus::Applied | ItemStatus::Duplicate => {
                let adjustments = result.adjustments.clone();
                if !adjustments.is_empty() {
                    till.write_replica(move |tx| outbox::reconcile(tx, &me, &adjustments, NOON))
                        .unwrap();
                }
                till.write(move |tx| outbox::mark_sent(tx, seq, &host_id, NOON))
                    .unwrap();
            }
            ItemStatus::Rejected => {
                let code = result.code.clone().unwrap_or_default();
                till.write(move |tx| outbox::mark_rejected(tx, seq, &code))
                    .unwrap();
            }
        }
    }
    results
}

/// Every page the main PC `host` has for `terminal_id`, applied to `till` like the engine does.
pub(crate) fn pull_into(host: &Db, till: &Db, terminal_id: &str) -> ApplyReport {
    let mut total = ApplyReport::default();
    loop {
        let since: i64 = till
            .read(|c| Ok(c.query_row("SELECT applied_seq FROM lan_local", [], |r| r.get(0))?))
            .unwrap();
        let me = terminal_id.to_owned();
        let page = host
            .read(move |c| feed::page(c, since, &me, PAGE_ROWS))
            .unwrap();
        let me = terminal_id.to_owned();
        let more = page.more;
        let report = till
            .write_replica(move |tx| apply_page(tx, &me, &page, NOON))
            .unwrap();
        total.rows += report.rows;
        total.skipped_own += report.skipped_own;
        total.deleted += report.deleted;
        total.derived += report.derived;
        total.applied_seq = report.applied_seq;
        if !more {
            return total;
        }
    }
}

/// A main PC made from a fresh test shop at `<dir>/host.db`, with its capture triggers and its
/// write hook.
pub(crate) fn lan_host(dir: &std::path::Path) -> Shop {
    let host_db = Db::open(&dir.join("host.db"), options(dir, "host-backups")).unwrap();
    let host = shop_in(host_db, TaxRegime::Reel, PriceMode::Ttc);
    let terminal_id = host.terminal.id.clone();
    host.db
        .write(move |tx| {
            install(tx, &CaptureSet::Host)?;
            tx.execute(
                "UPDATE lan_local SET role = 'host', host_id = ?1, updated_at = 0",
                [HOST_ID],
            )?;
            tx.execute(
                "UPDATE terminals SET role = 'host' WHERE id = ?1",
                [&terminal_id],
            )?;
            Ok(())
        })
        .unwrap();
    host.db
        .set_write_hook(Some(Arc::new(HostHook::new(Arc::new(|_| {})))));
    host
}

pub(crate) fn lan_pair() -> LanPair {
    let dir = tempfile::tempdir().unwrap();
    let host = lan_host(dir.path());
    let TillPc {
        db: till,
        terminal: till_terminal,
        cashier: till_cashier,
        owner: till_owner,
    } = add_till(&host, HOST_ID, dir.path(), "C2");
    LanPair {
        dir,
        host,
        till,
        till_terminal,
        till_cashier,
        till_owner,
    }
}

impl LanPair {
    pub fn sender(&self) -> super::replay::Sender {
        super::replay::Sender {
            terminal_id: self.till_terminal.id.clone(),
            code: self.till_terminal.code.clone(),
        }
    }

    /// Sends the till's pending items like the engine does, and records the answers.
    pub fn push(&self) -> Vec<super::outbox::ItemResult> {
        self.push_with(super::replay::Received {
            at: NOON + 1_000,
            skew_ms: Some(0),
        })
    }

    pub fn push_with(&self, received: super::replay::Received) -> Vec<super::outbox::ItemResult> {
        push_items(&self.host.db, HOST_ID, &self.till, &self.sender(), received)
    }

    pub fn till_ctx(&self) -> TillContext {
        TillContext::new(self.till_cashier.clone(), Arc::clone(&self.host.pack), NOON)
    }

    /// Every page the main PC has for this till, applied like the engine does.
    pub fn pull(&self) -> ApplyReport {
        pull_into(&self.host.db, &self.till, &self.till_terminal.id)
    }

    pub fn open_till_session(&self, float: i64) {
        let ctx = self.till_ctx();
        self.till
            .write(move |tx| crate::cash::open_session(tx, &ctx, Money(float)))
            .unwrap();
    }

    pub fn sell_on_till(&self, product_id: &str, qty: i64, cash: i64) -> ValidatedSale {
        let ctx = self.till_ctx();
        let input = SaleInput {
            cart: Cart::new(Lang::Fr, vec![CartLine::product(product_id, Qty(qty))]),
            payments: vec![PaymentInput::cash(Money(cash))],
        };
        self.till
            .write(move |tx| create_sale(tx, &ctx, input))
            .unwrap()
    }

    pub fn count_till(&self, sql: &'static str) -> i64 {
        self.till
            .read(move |c| Ok(c.query_row(sql, [], |r| r.get(0))?))
            .unwrap()
    }

    pub fn count_host(&self, sql: &'static str) -> i64 {
        self.host.count(sql)
    }
}

//! Two DZBuild POS on one machine, as two shop PCs on a switch: a main PC and a till, each with
//! its own data folder, database, LAN runtime, port and token, talking HTTP and WebSocket over
//! loopback. The main PC keeps a fixed port so it can go down and come back where the till
//! looks for it.

use super::LanOptions;
use super::vault::{MemoryVault, TokenVault};
use crate::commands::cash::close_session;
use crate::commands::testshop::{OWNER_PIN, TestShop, file_printer};
use crate::commands::{auth, lan as lan_cmd, sales as sales_cmd};
use crate::state::{AppPaths, AppState, SharedState};
use pos_core::auth::Permission;
use pos_core::cash::CloseInput;
use pos_core::catalog::products::Product;
use pos_core::db::migrations::LATEST_VERSION;
use pos_core::lang::Lang;
use pos_core::money::{Money, Qty};
use pos_core::payments::PaymentInput;
use pos_core::sales::{Cart, CartLine, SaleInput};
use pos_core::sync::{feed, outbox, role};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(crate) const TILL_CODE: &str = "C2";

pub(crate) fn eventually(what: &str, check: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if check() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("never happened: {what}");
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map(|addr| addr.port())
        .expect("a free loopback port")
}

fn open(dir: &Path, vault: Arc<dyn TokenVault>, bind: SocketAddr) -> SharedState {
    Arc::new(
        AppState::open_with(
            &AppPaths::new(dir),
            crate::printing::silent(),
            vault,
            LanOptions {
                bind,
                advertise: false,
            },
        )
        .expect("the PC opens"),
    )
}

pub(crate) struct TwoPcs {
    dir: tempfile::TempDir,
    pub host: TestShop,
    till: Option<TestShop>,
    till_vault: Arc<MemoryVault>,
    pub milk: Product,
    pub oil: Product,
}

impl TwoPcs {
    pub(crate) fn new() -> TwoPcs {
        let dir = tempfile::tempdir().expect("temp dir");
        let port = free_port();
        let host_st = open(
            &dir.path().join("host"),
            Arc::new(MemoryVault::default()),
            SocketAddr::from(([127, 0, 0, 1], port)),
        );
        let owner = crate::commands::tests::first_run(&host_st);
        let host = TestShop { st: host_st, owner };
        let milk = host.product("Lait Candia 1 L", 15_000, 900, Some("6130000000017"));
        let oil = host.product("Huile Elio 5 L", 65_000, 1_900, None);
        host.stock_in(&milk.id, 24);
        host.stock_in(&oil.id, 10);
        // the users exist before the copy is taken; the owner signs back in last
        host.cashier();
        host.manager();
        host.owner();
        lan_cmd::become_host(&host.st, &lan_cmd::quiet()).expect("main PC");
        eventually("the main PC listens", || {
            host.st
                .lan
                .status(&host.st)
                .is_ok_and(|s| s.host.is_some_and(|h| h.listening))
        });
        let code = lan_cmd::pairing_code(&host.st).expect("a code").code;
        let till_vault = Arc::new(MemoryVault::default());
        let joined = pos_server::client::join::join(
            None,
            &dir.path().join("till"),
            pos_server::client::join::JoinRequest {
                address: format!("127.0.0.1:{port}"),
                code,
                terminal_code: TILL_CODE.into(),
                terminal_name: "Caisse 2".into(),
                app_version: env!("CARGO_PKG_VERSION").into(),
                schema_version: LATEST_VERSION,
                install_id: pos_core::ids::new_id(),
                former_terminal: None,
            },
            Arc::new(|_, _| {}),
        )
        .expect("the till joins");
        till_vault.set(&joined.token).expect("vault");
        let mut pcs = TwoPcs {
            dir,
            host,
            till: None,
            till_vault,
            milk,
            oil,
        };
        pcs.start_till();
        pcs
    }

    /// Opens (or reopens after a kill) the till from its folder and starts its engine.
    pub(crate) fn start_till(&mut self) {
        let st = open(
            &self.dir.path().join("till"),
            self.till_vault.clone(),
            SocketAddr::from(([127, 0, 0, 1], 0)),
        );
        st.lan
            .start(&st, lan_cmd::quiet())
            .expect("the till's engine");
        let owner = self.host.owner.clone();
        self.till = Some(TestShop { st, owner });
        eventually("the till is online", || self.till().st.lan.is_online());
    }

    pub(crate) fn till(&self) -> &TestShop {
        self.till.as_ref().expect("the till runs")
    }

    /// A power cut on the till: its engine stops where it is and its state goes away without
    /// the clean-shutdown mark.
    pub(crate) fn kill_till(&mut self) {
        if let Some(till) = self.till.take() {
            till.st.lan.stop();
            drop(till);
        }
    }

    /// A receipt printer writing to a file in `shop`'s folder (M1b's `file_printer`).
    pub(crate) fn printer(&self, shop: &TestShop) -> PathBuf {
        let folder = if std::ptr::eq(shop, &self.host) {
            "host"
        } else {
            "till"
        };
        let out = self.dir.path().join(folder).join("till.bin");
        shop.st.hw.save(file_printer(&out)).expect("printer");
        out
    }

    pub(crate) fn host_down(&self) {
        self.host.st.lan.stop();
        eventually("the till sees the main PC gone", || {
            !self.till().st.lan.is_online()
        });
    }

    pub(crate) fn host_up(&self) {
        self.host.st.lan.restart(&self.host.st, lan_cmd::quiet());
        eventually("the main PC listens again", || {
            self.host
                .st
                .lan
                .status(&self.host.st)
                .is_ok_and(|s| s.host.is_some_and(|h| h.listening))
        });
        self.till().st.lan.sync_now();
        eventually("the till is back online", || self.till().st.lan.is_online());
    }

    pub(crate) fn wait_sent(&self) {
        let till = self.till();
        till.st.lan.sync_now();
        eventually("the till sent everything", || {
            till.st
                .db
                .read(outbox::depth)
                .is_ok_and(|d| d.pending == 0 && d.rejected == 0)
        });
    }

    pub(crate) fn wait_caught_up(&self) {
        let till = self.till();
        till.st.lan.sync_now();
        eventually("the till applied the main PC's changes", || {
            let head = self.host.st.db.read(feed::head).unwrap_or(i64::MAX);
            till.st
                .db
                .read(role::current)
                .is_ok_and(|r| r.applied_seq >= head)
        });
    }
}

impl Drop for TwoPcs {
    fn drop(&mut self) {
        if let Some(till) = &self.till {
            till.st.lan.stop();
        }
        self.host.st.lan.stop();
    }
}

/// Number, validation time and hash of every validated document of `terminal_id`, in chain order.
pub(crate) fn tickets_of(
    st: &AppState,
    terminal_id: &str,
) -> Vec<(Option<String>, Option<i64>, Option<String>)> {
    let terminal_id = terminal_id.to_owned();
    st.db
        .read(move |c| {
            let mut stmt = c.prepare(
                "SELECT number, validated_at, hash FROM documents
                 WHERE terminal_id = ?1 AND hash IS NOT NULL ORDER BY doc_type, chain_seq",
            )?;
            let rows = stmt
                .query_map([&terminal_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .expect("documents")
}

pub(crate) fn on_hand(st: &AppState, product_id: &str) -> Qty {
    let product_id = product_id.to_owned();
    st.db
        .read(move |c| pos_core::stock::on_hand(c, &product_id))
        .expect("stock")
}

pub(crate) fn cash_sale(product_id: &str, pieces: i64, cash: i64) -> sales_cmd::SaleRequest {
    sales_cmd::SaleRequest {
        sale: SaleInput {
            cart: Cart::new(
                Lang::Fr,
                vec![CartLine::product(product_id, Qty(pieces * 1_000))],
            ),
            payments: vec![PaymentInput::cash(Money(cash))],
        },
        print_receipt: false,
        extras: Default::default(),
    }
}

/// The owner accepts whatever the count says, then the drawer closes with an empty count.
pub(crate) fn close_approved(shop: &TestShop) -> pos_core::cash::closing::Closed {
    auth::manager_override(
        &shop.st,
        &auth::OverrideInput {
            approver_id: shop.owner.user_id.clone(),
            pin: OWNER_PIN.into(),
            permission: Permission::CashViewExpected,
        },
    )
    .expect("approval");
    close_session(
        &shop.st,
        CloseInput {
            counts: vec![],
            notes: None,
        },
    )
    .expect("the Z")
}

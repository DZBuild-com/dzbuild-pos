//! A main PC for the server tests: a file database set up like the first-run wizard, made a
//! main PC, served on 127.0.0.1 with an ephemeral port. Test-only.

use crate::host::{HostConfig, HostServer, HostServices, RpcCaller, RpcHandler};
use pos_core::db::{Db, DbOptions, migrations::LATEST_VERSION};
use pos_core::error::AppError;
use pos_core::sync::pairing::PairingDesk;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub(crate) const OWNER_PIN: &str = "2580";

pub(crate) struct NoRpc;

impl RpcHandler for NoRpc {
    fn call(
        &self,
        _: RpcCaller,
        service: &str,
        _: serde_json::Value,
    ) -> Result<serde_json::Value, AppError> {
        Err(AppError::not_found(&format!("rpc_{service}")))
    }
}

pub(crate) struct Host {
    pub db: Db,
    pub desk: Arc<Mutex<PairingDesk>>,
    pub server: Option<HostServer>,
    pub config: HostConfig,
    pub terminal_id: String,
}

pub(crate) fn host_db(dir: &Path) -> (Db, String) {
    let db = Db::open(
        &dir.join("pos.db"),
        DbOptions {
            readers: 2,
            backup_dir: Some(dir.join("backups")),
        },
    )
    .unwrap();
    let terminal = pos_core::setup::bootstrap(&db).unwrap();
    let pack = Arc::new(pos_core::datapack::bundled().unwrap());
    let seed = pack.clone();
    db.write(move |tx| pos_core::catalog::ensure_defaults(tx, &seed))
        .unwrap();
    pos_core::setup::complete_first_run(
        &db,
        pack,
        &terminal,
        pos_core::setup::FirstRunInput {
            lang: pos_core::lang::Lang::Fr,
            shop_name: "Supérette El Amel".into(),
            activity: pos_core::company::ShopActivity::General,
            tax_regime: pos_core::company::TaxRegime::Reel,
            price_mode: pos_core::company::PriceMode::Ttc,
            wilaya: Some("16".into()),
            phone: None,
            owner_name: "Amine Benali".into(),
            owner_pin: OWNER_PIN.into(),
        },
    )
    .unwrap();
    db.write(|tx| {
        pos_core::sync::role::become_host(tx, &pos_core::audit::Actor::default(), 0).map(drop)
    })
    .unwrap();
    (db, terminal.id)
}

pub(crate) fn host(dir: &Path) -> Host {
    host_with_clock(dir, Arc::new(pos_core::time::now_ms))
}

/// [`host`] whose clock is `clock` (a main PC whose clock is set back).
pub(crate) fn host_with_clock(dir: &Path, clock: Arc<dyn Fn() -> i64 + Send + Sync>) -> Host {
    let (db, terminal_id) = host_db(dir);
    let host_id = db
        .read(pos_core::sync::role::current)
        .unwrap()
        .host_id
        .unwrap();
    let desk = Arc::new(Mutex::new(PairingDesk::default()));
    let config = HostConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        host_id,
        shop_name: "Supérette El Amel".into(),
        app_version: "0.5.0".into(),
        schema_version: LATEST_VERSION,
        snapshot_dir: dir.join("lan").join("snapshots"),
        update_dir: dir.join("updates").join("mirror"),
    };
    let mut services = services(&db, &desk, &terminal_id);
    services.clock = clock;
    let server = HostServer::start(config.clone(), services).unwrap();
    let head = server.head_watch();
    db.set_write_hook(Some(Arc::new(pos_core::sync::feed::HostHook::new(
        Arc::new(move |h| {
            head.send_replace(h);
        }),
    ))));
    Host {
        db,
        desk,
        server: Some(server),
        config,
        terminal_id,
    }
}

pub(crate) fn services(db: &Db, desk: &Arc<Mutex<PairingDesk>>, terminal_id: &str) -> HostServices {
    HostServices {
        db: db.clone(),
        desk: desk.clone(),
        rpc: Arc::new(NoRpc),
        host_terminal_id: terminal_id.to_owned(),
        clock: Arc::new(pos_core::time::now_ms),
    }
}

impl Host {
    /// The server's address, or the configured one when this `Host` has no server of its own.
    pub fn base(&self) -> String {
        let addr = self
            .server
            .as_ref()
            .map_or(self.config.bind, HostServer::local_addr);
        format!("http://{addr}")
    }

    /// The same main PC's database and desk, without a server (tests point a till elsewhere).
    pub fn detached(other: &Host) -> Host {
        Host {
            db: other.db.clone(),
            desk: other.desk.clone(),
            server: None,
            config: other.config.clone(),
            terminal_id: other.terminal_id.clone(),
        }
    }

    /// Pairs `code` (C2…) directly through pos-core, journaled like `POST /pair` with the main
    /// PC's terminal as actor, and returns its token.
    pub fn pair(&self, code: &str) -> String {
        let issued = self
            .desk
            .lock()
            .unwrap()
            .issue(pos_core::time::now_ms())
            .unwrap();
        let request = pos_core::sync::devices::PairRequest {
            code: issued.code,
            terminal_code: code.into(),
            terminal_name: format!("Caisse {code}"),
            app_version: "0.5.0".into(),
            schema_version: LATEST_VERSION,
            install_id: pos_core::ids::new_id(),
            terminal_id: None,
        };
        pos_core::sync::devices::pair(
            &self.db,
            &self.desk,
            request,
            LATEST_VERSION,
            &pos_core::audit::Actor::system(Some(self.terminal_id.clone())),
            pos_core::time::now_ms(),
        )
        .unwrap()
        .token
    }

    /// A product created by the owner on the main PC.
    pub fn product(&self, name: &str, price: i64) -> String {
        create_product(&self.db, name, price).unwrap()
    }
}

/// A product created by the owner on the main PC; its id.
fn create_product(db: &Db, name: &str, price: i64) -> Result<String, AppError> {
    let name = name.to_owned();
    db.write(move |tx| {
        let rate = pos_core::catalog::tax_rates::by_code(tx, "TVA19")?
            .ok_or_else(|| AppError::not_found("tax_rate"))?;
        pos_core::catalog::products::create(
            tx,
            &pos_core::audit::Actor::default(),
            pos_core::catalog::products::ProductInput::simple(
                &name,
                pos_core::catalog::UNIT_PIECE_ID,
                &rate.id,
                pos_core::money::Money(price),
            ),
        )
    })
    .map(|product| product.id)
}

/// A host service that creates the product `input["name"]` on the main PC and answers
/// `{ "id", "name" }` (the till's engine tests call it through `/rpc`).
pub(crate) struct CreatesProduct {
    pub db: Db,
}

impl RpcHandler for CreatesProduct {
    fn call(
        &self,
        _: RpcCaller,
        _: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, AppError> {
        let name = input["name"]
            .as_str()
            .ok_or_else(|| AppError::validation("name", "required"))?;
        let id = create_product(&self.db, name, 1_000)?;
        Ok(serde_json::json!({ "id": id, "name": name }))
    }
}

pub(crate) struct Till {
    pub db: Db,
    pub terminal: pos_core::terminal::Terminal,
    pub token: String,
    pub user_id: String,
}

/// A second PC joins `host` as C2 through HTTP, like the wizard of Task 22 does.
pub(crate) fn till(dir: &Path, host: &Host) -> Till {
    let issued = host
        .desk
        .lock()
        .unwrap()
        .issue(pos_core::time::now_ms())
        .unwrap();
    let request = pos_core::sync::devices::PairRequest {
        code: issued.code,
        terminal_code: "C2".into(),
        terminal_name: "Caisse 2".into(),
        app_version: "0.5.0".into(),
        schema_version: LATEST_VERSION,
        install_id: pos_core::ids::new_id(),
        terminal_id: None,
    };
    let paired: crate::wire::PairResponse = http()
        .post(format!("{}/pair", host.base()))
        .json(&request)
        .send()
        .unwrap()
        .json()
        .unwrap();
    let response = http()
        .get(format!("{}/snapshot", host.base()))
        .bearer_auth(&paired.token)
        .send()
        .unwrap();
    let seq: i64 = response.headers()[crate::wire::H_SEQ]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    let sha = response.headers()[crate::wire::H_SHA256]
        .to_str()
        .unwrap()
        .to_owned();
    let till_dir = dir.join("till");
    std::fs::create_dir_all(&till_dir).unwrap();
    let download = till_dir.join("download.db");
    std::fs::write(&download, response.bytes().unwrap()).unwrap();
    pos_core::sync::snapshot::verify_download(&download, &sha, seq).unwrap();
    let pending = pos_core::sync::replica::PendingReplica {
        seq,
        host_id: paired.host_id.clone(),
        host_name: paired.shop_name.clone(),
        host_address: host.server.as_ref().unwrap().local_addr().to_string(),
        terminal_id: paired.terminal.id.clone(),
        terminal_code: paired.terminal.code.clone(),
        terminal_name: paired.terminal.name.clone(),
        paired_at: pos_core::time::now_ms(),
    };
    pos_core::sync::replica::stage(&till_dir, &download, &pending).unwrap();
    let db_path = till_dir.join("pos.db");
    pos_core::sync::replica::install_pending(
        &till_dir,
        &db_path,
        &till_dir.join("backups"),
        pos_core::time::now_ms(),
    )
    .unwrap();
    let db = Db::open(
        &db_path,
        DbOptions {
            readers: 2,
            backup_dir: Some(till_dir.join("backups")),
        },
    )
    .unwrap();
    let terminal =
        pos_core::sync::replica::adopt(&db, &till_dir, &pending, pos_core::time::now_ms()).unwrap();
    db.set_write_hook(Some(Arc::new(pos_core::sync::outbox::OutboxHook::new(
        terminal.id.clone(),
        Arc::new(|| {}),
    ))));
    let user_id: String = db
        .read(|c| {
            Ok(c.query_row(
                "SELECT id FROM users ORDER BY created_at LIMIT 1",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    Till {
        db,
        terminal,
        token: paired.token,
        user_id,
    }
}

impl Till {
    /// Signs the owner in on the till and opens the drawer (a local write: it lands in the outbox).
    pub fn open_session(&self) -> pos_core::till::TillContext {
        let (id, terminal) = (self.user_id.clone(), self.terminal.clone());
        let session = self
            .db
            .read(move |c| {
                pos_core::auth::build_session(c, &id, &terminal, pos_core::time::now_ms())
            })
            .unwrap();
        let ctx = pos_core::till::TillContext::new(
            session,
            Arc::new(pos_core::datapack::bundled().unwrap()),
            pos_core::time::now_ms(),
        );
        let open = ctx.clone();
        self.db
            .write(move |tx| pos_core::cash::open_session(tx, &open, pos_core::money::Money(0)))
            .unwrap();
        ctx
    }
}

struct Quiet;

impl crate::client::ClientEvents for Quiet {
    fn status(&self, _: &pos_core::sync::status::ClientStatus) {}
    fn applied(&self, _: &pos_core::sync::apply::ApplyReport) {}
}

/// The engine of `till` against `host`, telling nobody of its status.
pub(crate) fn client(host: &Host, till: &Till) -> crate::client::LanClient {
    crate::client::LanClient::start(
        till.db.clone(),
        crate::client::ClientConfig {
            base: host.base(),
            host_id: host.config.host_id.clone(),
            terminal_id: till.terminal.id.clone(),
            token: till.token.clone(),
            app_version: "0.5.0".into(),
            schema_version: LATEST_VERSION,
        },
        Arc::new(tokio::sync::Notify::new()),
        Arc::new(Quiet),
    )
    .unwrap()
}

/// A blocking HTTP client for tests (off any Tokio runtime).
pub(crate) fn http() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap()
}

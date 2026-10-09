//! Link and sync commands against the DZBuild stand-in.

use super::cloud::{self, CloudPage, PublishChange};
use super::cloud_mock::{Dz, cloud_shop, one_store, settled};
use super::testshop::{CASHIER_PIN, OWNER_PIN};
use pos_core::cloud::link::AttemptState;
use pos_core::cloud::publish::PublishQuery;
use pos_core::cloud::secrets::{SecretStore, refresh_account};
use pos_core::lang::Lang;
use pos_core::money::Money;
use serde_json::json;

#[test]
fn linking_is_for_the_owner_and_nothing_needs_it() {
    let dz = Dz::start();
    let (shop, _) = cloud_shop(&dz);
    shop.cashier();
    assert!(
        !cloud::status(&shop.st).unwrap().linked,
        "every user sees the status"
    );
    assert_eq!(
        cloud::link_start(&shop.st, Lang::Fr).unwrap_err().code(),
        "insufficient_permission"
    );
    assert_eq!(
        cloud::unlink(&shop.st, CASHIER_PIN.into())
            .unwrap_err()
            .code(),
        "insufficient_permission"
    );
    // the sale works while nothing is linked (the engine idles)
    let milk = shop.product("Lait", 11_000, 1_900, None);
    shop.stock_in(&milk.id, 5);
    shop.open_till(0);
    shop.sell(&milk.id, 1, 11_000);
    assert!(dz.requests().is_empty(), "no request without a link");
}

#[test]
fn the_browser_flow_links_the_store_and_keeps_the_token_in_the_secret_store() {
    let dz = Dz::start();
    dz.mount_link(one_store());
    let (shop, secrets) = cloud_shop(&dz);
    shop.owner();
    let start = cloud::link_start(&shop.st, Lang::Ar).unwrap();
    assert!(
        start
            .attempt
            .authorize_url
            .as_deref()
            .is_some_and(|u| u.contains("code_challenge_method=S256"))
    );
    let view = settled(&shop);
    let AttemptState::Linked { link } = view.state else {
        panic!("{:?}", view.state)
    };
    assert_eq!(
        (link.store_id.as_str(), link.store_name.as_str()),
        ("141", "Boutique Amel")
    );
    assert_eq!(
        secrets.get(&refresh_account(&link.id)).unwrap().as_deref(),
        Some("rt_1")
    );
    let status = cloud::status(&shop.st).unwrap();
    assert!(status.linked);
    let stored: i64 = shop
        .st
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM cloud_links
                 WHERE instr(account_sub || store_id || scopes || COALESCE(device_id, '') || COALESCE(location_id, ''), 'rt_1') > 0
                    OR instr(account_sub || store_id || scopes, 'at_1') > 0",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(stored, 0, "no token in the database");
    assert_eq!(
        cloud::link_start(&shop.st, Lang::Fr).unwrap_err().code(),
        "cloud_already_linked"
    );
}

#[test]
fn several_stores_ask_the_owner_to_choose() {
    let dz = Dz::start();
    dz.mount_link(json!([
        { "store_id": 141, "store_name": "Boutique Amel", "plan": "pro", "role": "owner" },
        { "store_id": 152, "store_name": "Amel Cosmétiques", "plan": "free", "role": "owner" }
    ]));
    let (shop, _) = cloud_shop(&dz);
    shop.owner();
    cloud::link_start(&shop.st, Lang::Fr).unwrap();
    let view = settled(&shop);
    let AttemptState::ChooseStore { stores } = view.state else {
        panic!("{:?}", view.state)
    };
    assert_eq!(stores.len(), 2);
    let link = cloud::link_choose(&shop.st, "152".into()).unwrap();
    assert_eq!(link.store_name, "Amel Cosmétiques");
}

#[test]
fn the_phone_flow_shows_a_code_and_a_qr() {
    let dz = Dz::start();
    dz.mount_link(one_store());
    let (shop, _) = cloud_shop(&dz);
    shop.owner();
    let start = cloud::device_flow_start(&shop.st).unwrap();
    let device = start.attempt.device.clone().unwrap();
    assert_eq!(device.user_code, "DZKM-4Q7P");
    let qr = start.qr.unwrap();
    assert!(qr.size >= 21 && qr.modules.len() == (qr.size * qr.size) as usize);
    cloud::link_cancel(&shop.st).unwrap();
    assert_eq!(cloud::link_poll(&shop.st).unwrap(), None);
}

#[test]
fn unlink_needs_the_pin_revokes_and_keeps_the_outbox() {
    let dz = Dz::start();
    dz.mount_link(one_store());
    let (shop, secrets) = cloud_shop(&dz);
    shop.owner();
    cloud::link_start(&shop.st, Lang::Fr).unwrap();
    let AttemptState::Linked { link } = settled(&shop).state else {
        panic!("not linked")
    };
    assert_eq!(
        cloud::unlink(&shop.st, "0000".into()).unwrap_err().code(),
        "invalid_pin"
    );
    let outcome = cloud::unlink(&shop.st, OWNER_PIN.into()).unwrap();
    assert!(outcome.revoked && outcome.device_removed);
    assert_eq!(secrets.get(&refresh_account(&link.id)).unwrap(), None);
    assert!(!cloud::status(&shop.st).unwrap().linked);
    assert_eq!(dz.calls("POST", "/oauth/revoke"), 1);
}

#[test]
fn publishing_failed_items_and_reconcile_work_offline() {
    let dz = Dz::start();
    dz.mount_link(one_store());
    let (shop, _) = cloud_shop(&dz);
    shop.owner();
    cloud::link_start(&shop.st, Lang::Fr).unwrap();
    settled(&shop);
    let milk = shop.product("Lait", 11_000, 1_900, None);
    let n = cloud::publish_set(
        &shop.st,
        PublishChange {
            product_ids: vec![milk.id.clone()],
            publish: true,
            draft: false,
        },
    )
    .unwrap();
    assert_eq!(n, 1);
    let rows = cloud::publish_list(
        &shop.st,
        PublishQuery {
            text: None,
            only_published: true,
            limit: None,
        },
    )
    .unwrap();
    assert_eq!((rows.len(), rows[0].price), (1, Money(11_000)));
    assert!(cloud::outbox_failed(&shop.st).unwrap().is_empty());
    assert_eq!(
        cloud::stock_reconcile(&shop.st, None).unwrap(),
        0,
        "nothing sent yet"
    );
    cloud::sync_now(&shop.st).unwrap();
}

/// The DZBuild link runs on the main PC only (spec §4.6, LAN-012): on a till, linking, importing,
/// publishing, the reconcile and a retry answer `host_required` before any request or write;
/// the status, the notices and the signup page still answer.
#[test]
fn a_till_leaves_the_link_and_publishing_to_the_main_pc() {
    let dz = Dz::start();
    dz.mount_link(one_store());
    let (shop, _) = cloud_shop(&dz);
    shop.owner();
    let milk = shop.product("Lait", 11_000, 1_900, None);
    shop.st
        .db
        .write(|tx| {
            tx.execute(
                "UPDATE lan_local SET role = 'client', host_id = '00000000-0000-7000-8000-00000000a001',
                     host_address = '127.0.0.1:7345', paired_at = 1 WHERE id = 1",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    let opened = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let log = opened.clone();
    shop.st
        .cloud()
        .unwrap()
        .set_opener(std::sync::Arc::new(move |url: &str| {
            log.lock().unwrap().push(url.to_owned());
            Ok(())
        }));
    let refused = |result: Result<(), pos_core::error::AppError>| result.unwrap_err().code();
    assert_eq!(
        refused(cloud::link_start(&shop.st, Lang::Fr).map(drop)),
        "host_required"
    );
    assert_eq!(
        refused(cloud::device_flow_start(&shop.st).map(drop)),
        "host_required"
    );
    assert_eq!(
        refused(cloud::link_choose(&shop.st, "141".into()).map(drop)),
        "host_required"
    );
    assert_eq!(
        refused(cloud::import_online(&shop.st).map(drop)),
        "host_required"
    );
    assert_eq!(
        refused(
            cloud::publish_set(
                &shop.st,
                PublishChange {
                    product_ids: vec![milk.id.clone()],
                    publish: true,
                    draft: false,
                },
            )
            .map(drop)
        ),
        "host_required"
    );
    assert_eq!(
        refused(cloud::stock_reconcile(&shop.st, None).map(drop)),
        "host_required"
    );
    assert_eq!(
        refused(cloud::outbox_retry(
            &shop.st,
            "00000000-0000-7000-8000-0000000000ff".into()
        )),
        "host_required"
    );
    assert!(dz.requests().is_empty(), "a till never talks to DZBuild");
    assert!(opened.lock().unwrap().is_empty(), "no browser opened");
    let untouched: i64 = shop
        .st
        .db
        .read(|c| {
            Ok(c.query_row(
                "SELECT (SELECT COUNT(*) FROM cloud_links) + (SELECT COUNT(*) FROM cloud_publish)",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(untouched, 0, "no link and no publish choice on a till");
    assert!(!cloud::status(&shop.st).unwrap().linked);
    cloud::notices(&shop.st).unwrap();
    cloud::open_page(&shop.st, CloudPage::Signup).unwrap();
    assert_eq!(
        opened.lock().unwrap().len(),
        1,
        "the signup page still opens"
    );
}

#[test]
fn on_a_till_the_status_names_the_main_pc_that_runs_the_link() {
    let dz = Dz::start();
    let (shop, _) = cloud_shop(&dz);
    shop.cashier();
    assert_eq!(
        cloud::status(&shop.st).unwrap().main_pc,
        None,
        "a single PC runs its own link"
    );
    shop.st
        .db
        .write(|tx| {
            tx.execute(
                "UPDATE lan_local SET role = 'client', host_id = '00000000-0000-7000-8000-00000000a001',
                     host_name = 'Supérette El Amel', host_address = '192.168.1.10:7345', paired_at = 1
                 WHERE id = 1",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    let status = cloud::status(&shop.st).unwrap();
    assert_eq!(
        (status.linked, status.main_pc.as_deref()),
        (false, Some("Supérette El Amel")),
        "Settings › DZBuild is read-only on a till and names the main PC"
    );
}

#[test]
fn pages_open_in_the_system_browser_with_the_signup_campaign() {
    let dz = Dz::start();
    let (shop, _) = cloud_shop(&dz);
    let opened = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let log = opened.clone();
    shop.st
        .cloud()
        .unwrap()
        .set_opener(std::sync::Arc::new(move |url: &str| {
            log.lock().unwrap().push(url.to_owned());
            Ok(())
        }));
    shop.cashier();
    cloud::open_page(&shop.st, CloudPage::Signup).unwrap();
    assert!(
        opened.lock().unwrap()[0]
            .ends_with("?utm_source=dzbuild-pos&utm_medium=app&utm_campaign=link")
    );
    assert_eq!(
        cloud::open_page(&shop.st, CloudPage::Store)
            .unwrap_err()
            .code(),
        "cloud_not_linked"
    );
}

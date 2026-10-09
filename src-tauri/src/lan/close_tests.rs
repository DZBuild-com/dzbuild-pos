//! The main PC's close button while tills are online (Task 24): held once and asked, never twice
//! within a minute, never after "Fermer".

#[test]
fn the_main_pc_holds_its_first_close_while_tills_are_online() {
    use crate::commands::testshop::shop;
    use pos_core::db::migrations::LATEST_VERSION;
    use pos_core::sync::devices::{self, PairRequest, Touch};
    let shop = shop();
    crate::commands::lan::become_host(&shop.st, &crate::commands::lan::quiet()).unwrap();
    let now = pos_core::time::now_ms();
    assert_eq!(
        shop.st.lan.should_hold_close(&shop.st, now).unwrap(),
        None,
        "no till: closes at once"
    );
    let code = crate::commands::lan::pairing_code(&shop.st).unwrap().code;
    let paired = devices::pair(
        &shop.st.db,
        &shop.st.lan.desk,
        PairRequest {
            code,
            terminal_code: "C2".into(),
            terminal_name: "Caisse 2".into(),
            app_version: env!("CARGO_PKG_VERSION").into(),
            schema_version: LATEST_VERSION,
            install_id: "install-c2".into(),
            terminal_id: None,
        },
        LATEST_VERSION,
        &shop.owner.actor(),
        now,
    )
    .unwrap();
    let till = paired.terminal.id.clone();
    shop.st
        .db
        .write(move |tx| devices::touch(tx, &till, &Touch::default(), LATEST_VERSION, now))
        .unwrap();
    assert_eq!(
        shop.st.lan.should_hold_close(&shop.st, now).unwrap(),
        Some(1)
    );
    assert_eq!(
        shop.st
            .lan
            .should_hold_close(&shop.st, now + 5_000)
            .unwrap(),
        None,
        "closed again: closes"
    );
    assert_eq!(
        shop.st
            .lan
            .should_hold_close(&shop.st, now + 90_000)
            .unwrap(),
        Some(1),
        "asked again later"
    );
    shop.st.lan.confirm_close();
    assert_eq!(
        shop.st
            .lan
            .should_hold_close(&shop.st, now + 200_000)
            .unwrap(),
        None
    );
    shop.st.lan.stop();
}

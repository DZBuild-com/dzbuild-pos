//! A Z closed on a till with operations still to send says so on paper (Task 24, LAN-003).

use super::*;
use crate::testdata::z_report;

#[test]
fn an_offline_z_says_so_on_paper() {
    let mut z = z_report();
    z.offline_pending = Some(3);
    let texts: Vec<String> = z_layout(&z, "Supérette El Amel", Lang::Fr, None)
        .texts()
        .map(str::to_owned)
        .collect();
    let at = texts
        .iter()
        .position(|t| t == &label(Lang::Fr, "z.offline"))
        .unwrap();
    assert_eq!(texts[at + 1], "3");
    let plain: Vec<String> = z_layout(&z_report(), "Shop", Lang::Fr, None)
        .texts()
        .map(str::to_owned)
        .collect();
    assert!(!plain.contains(&label(Lang::Fr, "z.offline")));
}

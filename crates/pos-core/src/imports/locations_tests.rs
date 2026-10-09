//! Opening stock into several locations: one row per (product, location) (M2 residual, M8 Task 11).

use super::engine::{ImportOptions, apply, dry_run, rows_of};
use super::spec::{ImportEntity, ImportPolicy};
use crate::catalog::UNIT_PIECE_ID;
use crate::catalog::products::ProductInput;
use crate::money::{Money, Qty};
use crate::stock::{self, LocationInput};
use crate::testkit::shop;
use std::collections::BTreeMap;

fn options() -> ImportOptions {
    ImportOptions {
        entity: ImportEntity::OpeningStock,
        policy: ImportPolicy::CreateOnly,
        file_name: "stock.xlsx".into(),
        header_row: 0,
        mapping: ["reference", "qty", "location"]
            .iter()
            .map(|k| Some((*k).to_owned()))
            .collect(),
        location_id: None,
        count_id: None,
    }
}

fn sheet(rows: &[&[&str]]) -> Vec<Vec<String>> {
    rows.iter()
        .map(|r| r.iter().map(|c| (*c).to_owned()).collect())
        .collect()
}

#[test]
fn one_product_loads_into_several_locations_one_row_each() {
    let shop = shop();
    let mut input = ProductInput::simple("Huile 5 L", UNIT_PIECE_ID, &shop.tva19, Money(90_000));
    input.reference = Some("H5".into());
    let oil = shop.product_with(input).id;
    let actor = shop.owner.actor();
    let depot = shop
        .db
        .write(move |tx| {
            stock::create_location(
                tx,
                &actor,
                LocationInput {
                    code: "DEP".into(),
                    name_fr: "Dépôt".into(),
                    ..LocationInput::default()
                },
            )
        })
        .unwrap()
        .id;
    let opts = options();
    let rows = rows_of(
        &sheet(&[
            &["Réf", "Qté", "Lieu"],
            &["H5", "12", ""],
            &["H5", "30", "DEP"],
            &["h5", "1", "dep"],
        ]),
        &opts,
    );
    let (ctx, o, r) = (shop.owner_till(), opts.clone(), rows.clone());
    let dry = dry_run(&shop.db, &ctx, &o, &r, &BTreeMap::new()).unwrap();
    let job = apply(
        &shop.db,
        &shop.owner_till(),
        &opts,
        &rows,
        &BTreeMap::new(),
        &dry.fingerprint,
        &mut |_| {},
    )
    .unwrap();
    assert_eq!((job.created, job.failed), (2, 1));
    assert_eq!(
        (job.problems[0].row, job.problems[0].code.as_str()),
        (4, "duplicate_in_file"),
        "the same product twice in the same location is still a duplicate"
    );
    let level = |loc: String| {
        let id = oil.clone();
        shop.db.read(move |c| stock::level(c, &id, &loc)).unwrap()
    };
    let main = shop.db.read(stock::default_location).unwrap().id;
    assert_eq!((level(main), level(depot)), (Qty(12_000), Qty(30_000)));
}

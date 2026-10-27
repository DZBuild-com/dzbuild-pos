//! Forward-only schema migrations (spec §4.3). Committed files are immutable: add a new one.

use rusqlite_migration::{M, Migrations};

pub(crate) const LIST: &[M<'static>] = &[
    M::up(include_str!("../../migrations/0001_foundation.sql")),
    M::up(include_str!("../../migrations/0002_catalogue_sales.sql")),
    M::up(include_str!("../../migrations/0003_back_office.sql")),
    M::up(include_str!("../../migrations/0004_documents_fiscal.sql")),
    M::up(include_str!("../../migrations/0005_hardware.sql")),
    M::up(include_str!("../../migrations/0006_dzbuild_link.sql")),
    M::up(include_str!("../../migrations/0007_lan.sql")),
    M::up(include_str!("../../migrations/0008_packs.sql")),
    M::up(include_str!(
        "../../migrations/0009_count_lots_order_payment.sql"
    )),
    M::up(include_str!("../../migrations/0010_dzbuild_variants.sql")),
];

pub const MIGRATIONS: Migrations<'static> = Migrations::from_slice(LIST);

/// `PRAGMA user_version` after every migration of this build.
pub const LATEST_VERSION: i64 = LIST.len() as i64;

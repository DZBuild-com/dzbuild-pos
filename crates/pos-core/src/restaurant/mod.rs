//! Restaurant and café (master spec §29): rooms and tables on a grid, open tickets per table,
//! takeaway and delivery tickets with a daily order number, modifiers, transfer and merge, the
//! daily menu (Task 4); kitchen tickets, the addition, splits, voids with a reason and the payment
//! (Task 5). A table ticket is not a fiscal document: it becomes one M1a ticket when it is paid.

pub mod bill;
#[cfg(test)]
mod bill_tests;
#[cfg(test)]
mod comp_cover_tests;
#[cfg(test)]
mod extras_tests;
pub mod floor;
#[cfg(test)]
mod floor_screen_tests;
pub mod kitchen;
pub mod menu;
pub mod modifiers;
#[cfg(test)]
mod preview_tests;
#[cfg(test)]
pub(crate) mod tests;
pub mod tickets;

use crate::audit::Actor;
use crate::error::AppError;
use rusqlite::Transaction;

/// The restaurant template's seeds: a room with eight tables, sauces, cooking and supplements,
/// the kitchen and the bar.
/// The counter's predefined tickets and the kitchen tickets' language in the shop's language
/// `lang` (final review: a template applied in Arabic does not leave French buttons and French
/// kitchen tickets), each only while the owner has not set it. Journaled by the template's row.
pub fn seed_settings(tx: &Transaction<'_>, lang: crate::lang::Lang) -> Result<(), AppError> {
    use crate::lang::Lang;
    use serde_json::json;
    if !crate::settings::is_stored(tx, "restaurant.predefined_tickets")? {
        let labels = match lang {
            Lang::Ar => json!(["الكونتوار", "التراس"]),
            Lang::Fr => json!(["Comptoir", "Terrasse"]),
            Lang::En => json!(["Counter", "Terrace"]),
        };
        crate::settings::set_quiet(tx, "restaurant.predefined_tickets", labels)?;
    }
    if !crate::settings::is_stored(tx, "restaurant.kitchen_lang")? {
        crate::settings::set_quiet(tx, "restaurant.kitchen_lang", json!(lang))?;
    }
    Ok(())
}

pub fn seed(tx: &Transaction<'_>, actor: &Actor) -> Result<u32, AppError> {
    Ok(floor::seed_room(tx, actor)?
        + modifiers::seed(tx, actor)?
        + kitchen::seed_stations(tx, actor)?)
}

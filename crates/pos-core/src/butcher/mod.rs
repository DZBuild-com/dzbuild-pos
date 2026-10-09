//! Butcher, charcuterie, fruit and vegetables (master spec §30 FRS): the weighed sale is M4's
//! (scales, weight barcodes, price per kg on the display and labels); this module adds the carcass
//! cutting yield (VRT-011) and the daily waste log (STK-026).

#[cfg(test)]
mod lang_tests;
#[cfg(test)]
mod lot_tests;
#[cfg(test)]
mod tests;
pub mod waste;
#[cfg(test)]
mod waste_day_tests;
pub mod yields;

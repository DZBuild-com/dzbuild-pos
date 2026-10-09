//! Taxes (spec §6): TVA per line and per rate, and the droit de timbre on cash. Pure integer
//! functions; rates come from the catalogue, brackets from the regulatory data pack. Nothing
//! else in the code base computes a tax or rounds one.

pub mod timbre;
pub mod tva;

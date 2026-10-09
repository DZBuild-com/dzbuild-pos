//! A4/A5 documents (spec §9.2): language-independent print models built in Rust from a
//! `PrintContext`, rendered by the React templates of `src/documents/` in the print host. Every
//! word is in the model (document catalogues, spec §11): the templates never translate.

pub mod document;
#[cfg(test)]
mod fixtures;
pub mod labels;
pub mod packs;
pub mod qr;
pub mod receipt;
pub mod statement;
pub mod table;
#[cfg(test)]
mod template_words;
#[cfg(test)]
pub(crate) mod testdata;
pub mod warranty;

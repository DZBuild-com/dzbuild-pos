//! The label model `pos_core::labels` owns (M4 [core] Task 3): languages, fields, templates and
//! the built-in ones, barcodes, the contents of one label and the labels of one template.
//! Sizes are tenths of a millimetre (`_dmm`).

pub use pos_core::labels::{
    LabelBarcode, LabelData, LabelFields, LabelGroup, LabelLang, LabelSymbology, LabelTemplate,
    SymbologyChoice, WEIGHED_TEMPLATE, builtin_templates,
};

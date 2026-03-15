//! RNC (RELAX NG Compact) data model and XSD-to-RNC conversion.

mod convert;
mod model;

pub use convert::xsd_to_rnc;
pub use model::{
    wrap_comment, RncAttribute, RncBodyItem, RncElement, RncEnumSummary, RncGlobalElement,
    RncLayer, RncNamespace, RncPattern, RncQuantifier, RncSchema,
};

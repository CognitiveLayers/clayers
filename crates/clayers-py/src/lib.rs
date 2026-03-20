mod errors;
mod knowledge_model;
mod query;
mod repo;
mod spec;
mod xml;

use pyo3::prelude::*;

#[pymodule]
fn _clayers(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Error hierarchy
    errors::register(m)?;

    // Shared query types
    query::register(m)?;

    // Spec result types
    spec::register(m)?;

    // KnowledgeModel
    knowledge_model::register(m)?;

    // XML submodule
    xml::register(m)?;

    // Repo submodule
    repo::register(m)?;

    Ok(())
}

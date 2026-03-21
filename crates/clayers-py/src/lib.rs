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

    // CLI entry point (optional feature)
    #[cfg(feature = "cli")]
    m.add_function(wrap_pyfunction!(cli_main, m)?)?;

    Ok(())
}

/// Run the clayers CLI, parsing arguments from sys.argv.
#[cfg(feature = "cli")]
#[pyfunction]
fn cli_main(py: Python<'_>) -> PyResult<()> {
    let sys = py.import("sys")?;
    let argv: Vec<String> = sys.getattr("argv")?.extract()?;
    clayers_cli::cli_main_from(argv);
    Ok(())
}

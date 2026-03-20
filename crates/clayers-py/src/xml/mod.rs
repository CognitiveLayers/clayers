pub mod hash;

use pyo3::prelude::*;

pub use hash::ContentHash;

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "xml")?;
    m.add_class::<ContentHash>()?;
    parent.add_submodule(&m)?;

    // Make it importable as clayers._clayers.xml
    parent
        .py()
        .import("sys")?
        .getattr("modules")?
        .set_item("clayers._clayers.xml", &m)?;

    Ok(())
}

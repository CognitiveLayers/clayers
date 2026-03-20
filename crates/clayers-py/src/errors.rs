use pyo3::exceptions::PyException;
use pyo3::prelude::*;

pyo3::create_exception!(_clayers, ClayersError, PyException);
pyo3::create_exception!(_clayers, XmlError, ClayersError);
pyo3::create_exception!(_clayers, SpecError, ClayersError);
pyo3::create_exception!(_clayers, RepoError, ClayersError);

pub fn spec_err(e: clayers_spec::Error) -> PyErr {
    SpecError::new_err(e.to_string())
}

pub fn repo_err(e: clayers_repo::Error) -> PyErr {
    RepoError::new_err(e.to_string())
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("ClayersError", m.py().get_type::<ClayersError>())?;
    m.add("XmlError", m.py().get_type::<XmlError>())?;
    m.add("SpecError", m.py().get_type::<SpecError>())?;
    m.add("RepoError", m.py().get_type::<RepoError>())?;
    Ok(())
}

use pyo3::prelude::*;

#[pyclass(frozen, from_py_object)]
#[derive(Clone)]
pub struct QueryResult {
    #[pyo3(get)]
    pub kind: String,
    #[pyo3(get)]
    pub count: Option<usize>,
    #[pyo3(get)]
    pub values: Option<Vec<String>>,
}

#[pymethods]
impl QueryResult {
    fn __repr__(&self) -> String {
        match self.kind.as_str() {
            "count" => format!("QueryResult(count={})", self.count.unwrap_or(0)),
            _ => format!(
                "QueryResult(kind='{}', values={})",
                self.kind,
                self.values.as_ref().map_or(0, Vec::len)
            ),
        }
    }
}

impl From<clayers_spec::query::QueryResult> for QueryResult {
    fn from(r: clayers_spec::query::QueryResult) -> Self {
        match r {
            clayers_spec::query::QueryResult::Count(n) => Self {
                kind: "count".into(),
                count: Some(n),
                values: None,
            },
            clayers_spec::query::QueryResult::Text(t) => Self {
                kind: "text".into(),
                count: None,
                values: Some(t),
            },
            clayers_spec::query::QueryResult::Xml(x) => Self {
                kind: "xml".into(),
                count: None,
                values: Some(x),
            },
        }
    }
}

impl From<clayers_repo::QueryResult> for QueryResult {
    fn from(r: clayers_repo::QueryResult) -> Self {
        match r {
            clayers_repo::QueryResult::Count(n) => Self {
                kind: "count".into(),
                count: Some(n),
                values: None,
            },
            clayers_repo::QueryResult::Text(t) => Self {
                kind: "text".into(),
                count: None,
                values: Some(t),
            },
            clayers_repo::QueryResult::Xml(x) => Self {
                kind: "xml".into(),
                count: None,
                values: Some(x),
            },
        }
    }
}

pub fn parse_query_mode_spec(mode: &str) -> PyResult<clayers_spec::query::QueryMode> {
    match mode {
        "count" => Ok(clayers_spec::query::QueryMode::Count),
        "text" => Ok(clayers_spec::query::QueryMode::Text),
        "xml" => Ok(clayers_spec::query::QueryMode::Xml),
        _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "invalid query mode: {mode} (expected 'count', 'text', or 'xml')"
        ))),
    }
}

pub fn parse_query_mode_repo(mode: &str) -> PyResult<clayers_repo::QueryMode> {
    match mode {
        "count" => Ok(clayers_repo::QueryMode::Count),
        "text" => Ok(clayers_repo::QueryMode::Text),
        "xml" => Ok(clayers_repo::QueryMode::Xml),
        _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "invalid query mode: {mode} (expected 'count', 'text', or 'xml')"
        ))),
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<QueryResult>()?;
    Ok(())
}

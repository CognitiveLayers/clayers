use pyo3::prelude::*;

#[pyclass(frozen, from_py_object)]
#[derive(Clone)]
pub struct ContentHash {
    inner: clayers_xml::ContentHash,
}

impl ContentHash {
    pub fn from_inner(inner: clayers_xml::ContentHash) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> clayers_xml::ContentHash {
        self.inner
    }
}

#[pymethods]
impl ContentHash {
    #[staticmethod]
    fn from_bytes(bytes: &[u8]) -> PyResult<Self> {
        if bytes.len() != 32 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "expected exactly 32 bytes",
            ));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(bytes);
        Ok(Self {
            inner: clayers_xml::ContentHash::from_bytes(arr),
        })
    }

    #[staticmethod]
    fn from_canonical(data: &[u8]) -> Self {
        Self {
            inner: clayers_xml::ContentHash::from_canonical(data),
        }
    }

    #[staticmethod]
    fn from_hex(s: &str) -> PyResult<Self> {
        let inner: clayers_xml::ContentHash = s
            .parse()
            .map_err(|e: clayers_xml::Error| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(Self { inner })
    }

    #[getter]
    fn hex(&self) -> String {
        self.inner.to_hex()
    }

    #[getter]
    fn prefixed(&self) -> String {
        self.inner.to_prefixed()
    }

    fn __str__(&self) -> String {
        self.inner.to_string()
    }

    fn __repr__(&self) -> String {
        format!("ContentHash('{}')", self.inner.to_prefixed())
    }

    fn __eq__(&self, other: &ContentHash) -> bool {
        self.inner == other.inner
    }

    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.inner.hash(&mut hasher);
        hasher.finish()
    }
}

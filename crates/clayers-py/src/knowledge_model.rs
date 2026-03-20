use std::path::PathBuf;

use pyo3::prelude::*;

use crate::errors::{SpecError, spec_err};
use crate::query::{QueryResult, parse_query_mode_spec};
use crate::spec::types;

#[pyclass]
pub struct KnowledgeModel {
    spec_dir: PathBuf,
    repo_root: Option<PathBuf>,
    name: String,
    files: Vec<String>,
    schema_dir: Option<String>,
}

#[pymethods]
impl KnowledgeModel {
    #[new]
    #[pyo3(signature = (path, repo_root=None))]
    fn new(path: &str, repo_root: Option<&str>) -> PyResult<Self> {
        let spec_dir = PathBuf::from(path)
            .canonicalize()
            .map_err(|e| SpecError::new_err(format!("invalid path: {e}")))?;
        let repo_root_path = repo_root.map(PathBuf::from);

        let index_files = clayers_spec::discovery::find_index_files(&spec_dir)
            .map_err(|e| SpecError::new_err(e.to_string()))?;

        if index_files.is_empty() {
            return Err(SpecError::new_err(format!(
                "no index files found in {}",
                spec_dir.display()
            )));
        }

        let name = spec_dir
            .file_name()
            .map_or_else(|| "unknown".into(), |n| n.to_string_lossy().into_owned());

        let mut all_files = Vec::new();
        for index_path in &index_files {
            let file_paths = clayers_spec::discovery::discover_spec_files(index_path)
                .map_err(spec_err)?;
            for fp in file_paths {
                all_files.push(fp.display().to_string());
            }
        }

        let schema_dir =
            clayers_spec::discovery::find_schema_dir(&spec_dir).map(|p| p.display().to_string());

        Ok(Self {
            spec_dir,
            repo_root: repo_root_path,
            name,
            files: all_files,
            schema_dir,
        })
    }

    #[getter]
    fn name(&self) -> &str {
        &self.name
    }

    #[getter]
    fn files(&self) -> Vec<String> {
        self.files.clone()
    }

    #[getter]
    fn schema_dir(&self) -> Option<&str> {
        self.schema_dir.as_deref()
    }

    #[getter]
    fn combined_xml(&self) -> PyResult<String> {
        let index_files = clayers_spec::discovery::find_index_files(&self.spec_dir)
            .map_err(|e| SpecError::new_err(e.to_string()))?;
        let mut all_file_paths: Vec<PathBuf> = Vec::new();
        for index_path in &index_files {
            let fps = clayers_spec::discovery::discover_spec_files(index_path)
                .map_err(spec_err)?;
            all_file_paths.extend(fps);
        }
        let combined = clayers_spec::assembly::assemble_combined_string(&all_file_paths)
            .map_err(spec_err)?;
        Ok(combined)
    }

    fn validate(&self) -> PyResult<types::ValidationResult> {
        let result = clayers_spec::validate::validate_spec(&self.spec_dir).map_err(spec_err)?;
        Ok(result.into())
    }

    fn check_drift(&self) -> PyResult<types::DriftReport> {
        let result = clayers_spec::drift::check_drift(
            &self.spec_dir,
            self.repo_root.as_deref(),
        )
        .map_err(spec_err)?;
        Ok(result.into())
    }

    #[pyo3(signature = (code_path=None))]
    fn coverage(&self, code_path: Option<&str>) -> PyResult<types::CoverageReport> {
        let result =
            clayers_spec::coverage::analyze_coverage(&self.spec_dir, code_path).map_err(spec_err)?;
        Ok(result.into())
    }

    fn connectivity(&self) -> PyResult<types::ConnectivityReport> {
        let result =
            clayers_spec::connectivity::analyze_connectivity(&self.spec_dir).map_err(spec_err)?;
        Ok(result.into())
    }

    fn fix_node_hashes(&self) -> PyResult<types::FixReport> {
        let result =
            clayers_spec::fix::fix_node_hashes(&self.spec_dir).map_err(spec_err)?;
        Ok(result.into())
    }

    fn fix_artifact_hashes(&self) -> PyResult<types::FixReport> {
        let result =
            clayers_spec::fix::fix_artifact_hashes(&self.spec_dir).map_err(spec_err)?;
        Ok(result.into())
    }

    #[pyo3(signature = (xpath, *, mode="xml"))]
    fn query(&self, xpath: &str, mode: &str) -> PyResult<QueryResult> {
        let qm = parse_query_mode_spec(mode)?;
        let result =
            clayers_spec::query::execute_query(&self.spec_dir, xpath, qm).map_err(spec_err)?;
        Ok(result.into())
    }

    fn __repr__(&self) -> String {
        format!("KnowledgeModel('{}', files={})", self.name, self.files.len())
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<KnowledgeModel>()?;
    Ok(())
}

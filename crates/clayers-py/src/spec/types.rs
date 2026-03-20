use pyo3::prelude::*;

use clayers_spec::connectivity;
use clayers_spec::coverage;
use clayers_spec::drift;
use clayers_spec::fix;
use clayers_spec::validate;

// -- Validation --

#[pyclass(frozen)]
pub struct ValidationResult {
    #[pyo3(get)]
    pub spec_name: String,
    #[pyo3(get)]
    pub file_count: usize,
    #[pyo3(get)]
    pub errors: Vec<ValidationError>,
    #[pyo3(get)]
    pub is_valid: bool,
}

#[pyclass(frozen, from_py_object)]
#[derive(Clone)]
pub struct ValidationError {
    #[pyo3(get)]
    pub message: String,
}

#[pymethods]
impl ValidationError {
    fn __repr__(&self) -> String {
        format!("ValidationError('{}')", self.message)
    }
}

impl From<validate::ValidationResult> for ValidationResult {
    fn from(r: validate::ValidationResult) -> Self {
        let is_valid = r.is_valid();
        Self {
            spec_name: r.spec_name,
            file_count: r.file_count,
            errors: r
                .errors
                .into_iter()
                .map(|e| ValidationError { message: e.message })
                .collect(),
            is_valid,
        }
    }
}

// -- Drift --

#[pyclass(frozen)]
pub struct DriftReport {
    #[pyo3(get)]
    pub spec_name: String,
    #[pyo3(get)]
    pub total_mappings: usize,
    #[pyo3(get)]
    pub drifted_count: usize,
    #[pyo3(get)]
    pub mapping_drifts: Vec<MappingDrift>,
}

#[pyclass(frozen, from_py_object)]
#[derive(Clone)]
pub struct MappingDrift {
    #[pyo3(get)]
    pub mapping_id: String,
    #[pyo3(get)]
    pub status: DriftStatus,
}

#[pyclass(frozen, from_py_object)]
#[derive(Clone)]
pub struct DriftStatus {
    #[pyo3(get)]
    pub kind: String,
    #[pyo3(get)]
    pub stored_hash: Option<String>,
    #[pyo3(get)]
    pub current_hash: Option<String>,
    #[pyo3(get)]
    pub artifact_path: Option<String>,
    #[pyo3(get)]
    pub reason: Option<String>,
}

impl From<drift::DriftReport> for DriftReport {
    fn from(r: drift::DriftReport) -> Self {
        Self {
            spec_name: r.spec_name,
            total_mappings: r.total_mappings,
            drifted_count: r.drifted_count,
            mapping_drifts: r.mapping_drifts.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<drift::MappingDrift> for MappingDrift {
    fn from(m: drift::MappingDrift) -> Self {
        Self {
            mapping_id: m.mapping_id,
            status: m.status.into(),
        }
    }
}

impl From<drift::DriftStatus> for DriftStatus {
    fn from(s: drift::DriftStatus) -> Self {
        match s {
            drift::DriftStatus::Clean => Self {
                kind: "clean".into(),
                stored_hash: None,
                current_hash: None,
                artifact_path: None,
                reason: None,
            },
            drift::DriftStatus::SpecDrifted {
                stored_hash,
                current_hash,
            } => Self {
                kind: "spec_drifted".into(),
                stored_hash: Some(stored_hash),
                current_hash: Some(current_hash),
                artifact_path: None,
                reason: None,
            },
            drift::DriftStatus::ArtifactDrifted {
                stored_hash,
                current_hash,
                artifact_path,
            } => Self {
                kind: "artifact_drifted".into(),
                stored_hash: Some(stored_hash),
                current_hash: Some(current_hash),
                artifact_path: Some(artifact_path),
                reason: None,
            },
            drift::DriftStatus::Unavailable { reason } => Self {
                kind: "unavailable".into(),
                stored_hash: None,
                current_hash: None,
                artifact_path: None,
                reason: Some(reason),
            },
        }
    }
}

// -- Coverage --

#[pyclass(frozen)]
pub struct CoverageReport {
    #[pyo3(get)]
    pub spec_name: String,
    #[pyo3(get)]
    pub total_nodes: usize,
    #[pyo3(get)]
    pub mapped_nodes: usize,
    #[pyo3(get)]
    pub exempt_nodes: usize,
    #[pyo3(get)]
    pub unmapped_nodes: Vec<String>,
    #[pyo3(get)]
    pub artifact_coverages: Vec<ArtifactCoverage>,
    #[pyo3(get)]
    pub file_coverages: Vec<FileCoverage>,
}

#[pyclass(frozen, from_py_object)]
#[derive(Clone)]
pub struct ArtifactCoverage {
    #[pyo3(get)]
    pub mapping_id: String,
    #[pyo3(get)]
    pub artifact_path: String,
    #[pyo3(get)]
    pub strength: String,
    #[pyo3(get)]
    pub line_count: usize,
}

#[pyclass(frozen, from_py_object)]
#[derive(Clone)]
pub struct FileCoverage {
    #[pyo3(get)]
    pub file_path: String,
    #[pyo3(get)]
    pub total_lines: usize,
    #[pyo3(get)]
    pub covered_lines: usize,
    #[pyo3(get)]
    pub coverage_percent: f64,
}

impl From<coverage::CoverageReport> for CoverageReport {
    fn from(r: coverage::CoverageReport) -> Self {
        Self {
            spec_name: r.spec_name,
            total_nodes: r.total_nodes,
            mapped_nodes: r.mapped_nodes,
            exempt_nodes: r.exempt_nodes,
            unmapped_nodes: r.unmapped_nodes,
            artifact_coverages: r.artifact_coverages.into_iter().map(Into::into).collect(),
            file_coverages: r.file_coverages.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<coverage::ArtifactCoverage> for ArtifactCoverage {
    fn from(a: coverage::ArtifactCoverage) -> Self {
        Self {
            mapping_id: a.mapping_id,
            artifact_path: a.artifact_path,
            strength: a.strength.to_string(),
            line_count: a.line_count,
        }
    }
}

impl From<coverage::FileCoverage> for FileCoverage {
    fn from(f: coverage::FileCoverage) -> Self {
        Self {
            file_path: f.file_path,
            total_lines: f.total_lines,
            covered_lines: f.covered_lines,
            coverage_percent: f.coverage_percent,
        }
    }
}

// -- Connectivity --

#[pyclass(frozen)]
pub struct ConnectivityReport {
    #[pyo3(get)]
    pub spec_name: String,
    #[pyo3(get)]
    pub node_count: usize,
    #[pyo3(get)]
    pub edge_count: usize,
    #[pyo3(get)]
    pub density: f64,
    #[pyo3(get)]
    pub components: Vec<Vec<String>>,
    #[pyo3(get)]
    pub isolated_nodes: Vec<String>,
    #[pyo3(get)]
    pub hub_nodes: Vec<HubNode>,
    #[pyo3(get)]
    pub bridge_nodes: Vec<BridgeNode>,
    #[pyo3(get)]
    pub cycles: Vec<Cycle>,
    #[pyo3(get)]
    pub acyclic_violations: usize,
}

#[pyclass(frozen, from_py_object)]
#[derive(Clone)]
pub struct HubNode {
    #[pyo3(get)]
    pub id: String,
    #[pyo3(get)]
    pub in_degree: usize,
    #[pyo3(get)]
    pub out_degree: usize,
    #[pyo3(get)]
    pub total_degree: usize,
}

#[pyclass(frozen, from_py_object)]
#[derive(Clone)]
pub struct BridgeNode {
    #[pyo3(get)]
    pub id: String,
    #[pyo3(get)]
    pub centrality: f64,
}

#[pyclass(frozen, from_py_object)]
#[derive(Clone)]
pub struct Cycle {
    #[pyo3(get)]
    pub nodes: Vec<String>,
    #[pyo3(get)]
    pub has_acyclic_violation: bool,
}

impl From<connectivity::ConnectivityReport> for ConnectivityReport {
    fn from(r: connectivity::ConnectivityReport) -> Self {
        Self {
            spec_name: r.spec_name,
            node_count: r.node_count,
            edge_count: r.edge_count,
            density: r.density,
            components: r.components,
            isolated_nodes: r.isolated_nodes,
            hub_nodes: r.hub_nodes.into_iter().map(Into::into).collect(),
            bridge_nodes: r.bridge_nodes.into_iter().map(Into::into).collect(),
            cycles: r.cycles.into_iter().map(Into::into).collect(),
            acyclic_violations: r.acyclic_violations,
        }
    }
}

impl From<connectivity::HubNode> for HubNode {
    fn from(h: connectivity::HubNode) -> Self {
        Self {
            id: h.id,
            in_degree: h.in_degree,
            out_degree: h.out_degree,
            total_degree: h.total_degree,
        }
    }
}

impl From<connectivity::BridgeNode> for BridgeNode {
    fn from(b: connectivity::BridgeNode) -> Self {
        Self {
            id: b.id,
            centrality: b.centrality,
        }
    }
}

impl From<connectivity::Cycle> for Cycle {
    fn from(c: connectivity::Cycle) -> Self {
        Self {
            nodes: c.nodes,
            has_acyclic_violation: c.has_acyclic_violation,
        }
    }
}

// -- Fix --

#[pyclass(frozen)]
pub struct FixReport {
    #[pyo3(get)]
    pub spec_name: String,
    #[pyo3(get)]
    pub total_mappings: usize,
    #[pyo3(get)]
    pub fixed_count: usize,
    #[pyo3(get)]
    pub results: Vec<FixResult>,
}

#[pyclass(frozen, from_py_object)]
#[derive(Clone)]
pub struct FixResult {
    #[pyo3(get)]
    pub mapping_id: String,
    #[pyo3(get)]
    pub old_hash: String,
    #[pyo3(get)]
    pub new_hash: String,
}

impl From<fix::FixReport> for FixReport {
    fn from(r: fix::FixReport) -> Self {
        Self {
            spec_name: r.spec_name,
            total_mappings: r.total_mappings,
            fixed_count: r.fixed_count,
            results: r.results.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<fix::FixResult> for FixResult {
    fn from(f: fix::FixResult) -> Self {
        Self {
            mapping_id: f.mapping_id,
            old_hash: f.old_hash,
            new_hash: f.new_hash,
        }
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<ValidationResult>()?;
    m.add_class::<ValidationError>()?;
    m.add_class::<DriftReport>()?;
    m.add_class::<MappingDrift>()?;
    m.add_class::<DriftStatus>()?;
    m.add_class::<CoverageReport>()?;
    m.add_class::<ArtifactCoverage>()?;
    m.add_class::<FileCoverage>()?;
    m.add_class::<ConnectivityReport>()?;
    m.add_class::<HubNode>()?;
    m.add_class::<BridgeNode>()?;
    m.add_class::<Cycle>()?;
    m.add_class::<FixReport>()?;
    m.add_class::<FixResult>()?;
    Ok(())
}

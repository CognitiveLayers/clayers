"""Tests for KnowledgeModel against the shipped self-referential spec."""

import pytest

from clayers import KnowledgeModel, SpecError, Queryable


class TestConstruction:
    def test_valid_path(self, shipped_spec):
        km = KnowledgeModel(shipped_spec)
        assert km.name

    def test_nonexistent_path(self):
        with pytest.raises(SpecError):
            KnowledgeModel("/nonexistent/path")

    def test_empty_dir(self, tmp_path):
        with pytest.raises(SpecError, match="no index files"):
            KnowledgeModel(str(tmp_path))


class TestProperties:
    def test_name(self, shipped_spec):
        km = KnowledgeModel(shipped_spec)
        assert km.name == "clayers"

    def test_files(self, shipped_spec):
        km = KnowledgeModel(shipped_spec)
        assert len(km.files) >= 5
        assert all(f.endswith(".xml") for f in km.files)

    def test_combined_xml(self, shipped_spec):
        km = KnowledgeModel(shipped_spec)
        xml = km.combined_xml
        assert "<" in xml
        assert "cmb:spec" in xml

    def test_schema_dir(self, shipped_spec):
        km = KnowledgeModel(shipped_spec)
        # The shipped spec has schemas/ next to clayers/
        sd = km.schema_dir
        assert sd is None or "schemas" in sd


class TestValidation:
    def test_shipped_spec_is_valid(self, shipped_spec):
        km = KnowledgeModel(shipped_spec)
        result = km.validate()
        assert result.is_valid
        assert result.file_count > 0
        assert result.spec_name == "clayers"
        assert len(result.errors) == 0

    def test_validation_result_attributes(self, shipped_spec):
        km = KnowledgeModel(shipped_spec)
        result = km.validate()
        assert isinstance(result.spec_name, str)
        assert isinstance(result.file_count, int)
        assert isinstance(result.is_valid, bool)
        assert isinstance(result.errors, list)


class TestDrift:
    def test_drift_report(self, shipped_spec):
        km = KnowledgeModel(shipped_spec)
        drift = km.check_drift()
        assert drift.total_mappings > 0
        assert isinstance(drift.drifted_count, int)
        assert isinstance(drift.spec_name, str)
        assert len(drift.mapping_drifts) == drift.total_mappings

    def test_drift_status_attributes(self, shipped_spec):
        km = KnowledgeModel(shipped_spec)
        drift = km.check_drift()
        for md in drift.mapping_drifts:
            assert isinstance(md.mapping_id, str)
            assert md.status.kind in ("clean", "spec_drifted", "artifact_drifted", "unavailable")


class TestCoverage:
    def test_coverage_report(self, shipped_spec):
        km = KnowledgeModel(shipped_spec)
        cov = km.coverage()
        assert cov.total_nodes > 0
        assert isinstance(cov.mapped_nodes, int)
        assert isinstance(cov.exempt_nodes, int)
        assert isinstance(cov.unmapped_nodes, list)

    def test_artifact_coverages(self, shipped_spec):
        km = KnowledgeModel(shipped_spec)
        cov = km.coverage()
        assert len(cov.artifact_coverages) > 0
        for ac in cov.artifact_coverages:
            assert isinstance(ac.mapping_id, str)
            assert isinstance(ac.artifact_path, str)
            assert ac.strength in ("precise", "moderate", "broad")
            assert isinstance(ac.line_count, int)

    def test_file_coverages(self, shipped_spec):
        km = KnowledgeModel(shipped_spec)
        cov = km.coverage()
        assert len(cov.file_coverages) > 0
        for fc in cov.file_coverages:
            assert isinstance(fc.file_path, str)
            assert fc.total_lines > 0
            assert 0.0 <= fc.coverage_percent <= 100.0

    def test_coverage_with_filter(self, shipped_spec):
        km = KnowledgeModel(shipped_spec)
        filtered = km.coverage(code_path="clayers-spec")
        # Should only include files matching the filter
        for fc in filtered.file_coverages:
            assert "clayers-spec" in fc.file_path


class TestConnectivity:
    def test_connectivity_report(self, shipped_spec):
        km = KnowledgeModel(shipped_spec)
        conn = km.connectivity()
        assert conn.node_count >= 50
        assert conn.edge_count > 0
        assert 0.0 <= conn.density <= 1.0
        assert len(conn.components) >= 1

    def test_isolated_nodes(self, shipped_spec):
        km = KnowledgeModel(shipped_spec)
        conn = km.connectivity()
        assert isinstance(conn.isolated_nodes, list)

    def test_hub_nodes(self, shipped_spec):
        km = KnowledgeModel(shipped_spec)
        conn = km.connectivity()
        assert len(conn.hub_nodes) > 0
        for hub in conn.hub_nodes:
            assert isinstance(hub.id, str)
            assert hub.total_degree == hub.in_degree + hub.out_degree

    def test_bridge_nodes(self, shipped_spec):
        km = KnowledgeModel(shipped_spec)
        conn = km.connectivity()
        for bridge in conn.bridge_nodes:
            assert isinstance(bridge.id, str)
            assert bridge.centrality >= 0.0

    def test_cycles(self, shipped_spec):
        km = KnowledgeModel(shipped_spec)
        conn = km.connectivity()
        assert isinstance(conn.cycles, list)
        assert isinstance(conn.acyclic_violations, int)


class TestQuery:
    def test_count_terms(self, shipped_spec):
        km = KnowledgeModel(shipped_spec)
        result = km.query("//trm:term", mode="count")
        assert result.kind == "count"
        assert result.count is not None
        assert result.count >= 15

    def test_text_mode(self, shipped_spec):
        km = KnowledgeModel(shipped_spec)
        result = km.query("//trm:term/trm:name", mode="text")
        assert result.kind == "text"
        assert result.values is not None
        assert len(result.values) >= 15
        assert all(isinstance(v, str) for v in result.values)

    def test_xml_mode(self, shipped_spec):
        km = KnowledgeModel(shipped_spec)
        result = km.query('//trm:term[@id="term-layer"]', mode="xml")
        assert result.kind == "xml"
        assert result.values is not None
        assert len(result.values) >= 1
        assert "<" in result.values[0]

    def test_default_mode_is_xml(self, shipped_spec):
        km = KnowledgeModel(shipped_spec)
        result = km.query('//trm:term[@id="term-layer"]')
        assert result.kind == "xml"

    def test_invalid_mode(self, shipped_spec):
        km = KnowledgeModel(shipped_spec)
        with pytest.raises(ValueError, match="invalid query mode"):
            km.query("//trm:term", mode="invalid")


class TestQueryable:
    def test_implements_queryable(self, shipped_spec):
        km = KnowledgeModel(shipped_spec)
        assert isinstance(km, Queryable)

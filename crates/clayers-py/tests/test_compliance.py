"""Integration test for the store compliance tester.

Runs the full compliance suite (88 tests: 20 deterministic, 21 property,
47 query) against ComplianceMemoryStore, which wraps the Rust MemoryStore
and exposes the store protocol to Python.
"""

from clayers._clayers.repo import ComplianceMemoryStore, run_store_compliance


def test_compliance_all_pass():
    """All compliance tests should pass against the built-in MemoryStore."""
    results = run_store_compliance(ComplianceMemoryStore)
    failures = [r for r in results if not r.passed]
    assert not failures, "\n".join(
        f"  FAIL [{r.category}] {r.name}: {r.error}" for r in failures
    )


def test_compliance_count():
    """Compliance suite should contain at least 80 tests."""
    results = run_store_compliance(ComplianceMemoryStore)
    assert len(results) >= 80, f"Expected at least 80 tests, got {len(results)}"


def test_compliance_categories():
    """Compliance suite should cover deterministic, property, and query tests."""
    results = run_store_compliance(ComplianceMemoryStore)
    categories = {r.category for r in results}
    assert "deterministic" in categories, "Missing deterministic tests"
    assert "query" in categories, "Missing query tests"
    # Property tests have sub-categories like "property:object_store"
    has_property = any("property" in c for c in categories)
    assert has_property, "Missing property tests"

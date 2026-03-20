import os
from pathlib import Path

import pytest

# Shipped spec lives alongside the crate
CRATE_DIR = Path(__file__).resolve().parent.parent
SHIPPED_SPEC = CRATE_DIR.parent.parent / "clayers" / "clayers"


@pytest.fixture
def shipped_spec():
    """Path to the clayers self-referential spec."""
    assert SHIPPED_SPEC.exists(), f"shipped spec not found: {SHIPPED_SPEC}"
    return str(SHIPPED_SPEC)


@pytest.fixture
def memory_repo():
    """Fresh sync Repo backed by MemoryStore."""
    from clayers.repo import Repo, MemoryStore

    return Repo(MemoryStore())


@pytest.fixture
def author():
    from clayers.repo import Author

    return Author("Test User", "test@example.com")

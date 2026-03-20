"""Tests for SqliteStore backend."""

import pytest

from clayers.repo import Repo, Author
from clayers.xml import ContentHash

try:
    from clayers.repo import SqliteStore

    HAS_SQLITE = True
except ImportError:
    HAS_SQLITE = False

pytestmark = pytest.mark.skipif(not HAS_SQLITE, reason="sqlite feature not enabled")


@pytest.fixture
def author():
    return Author("SQLite Tester", "sqlite@test.com")


class TestSqliteInMemory:
    def test_open_in_memory(self):
        store = SqliteStore.open_in_memory()
        r = Repo(store)
        h = r.import_xml("<root/>")
        assert isinstance(h, ContentHash)

    def test_full_workflow(self, author):
        store = SqliteStore.open_in_memory()
        r = Repo(store)
        h = r.import_xml("<root>sqlite data</root>")
        t = r.build_tree([("doc.xml", h)])
        c = r.commit("main", t, author, "sqlite commit")
        xml = r.export_xml(h)
        assert "sqlite data" in xml
        branches = r.list_branches()
        assert any(name == "main" for name, _ in branches)


class TestSqlitePersistent:
    def test_open_file(self, tmp_path, author):
        db_path = str(tmp_path / "test.db")
        store = SqliteStore.open(db_path)
        r = Repo(store)
        h = r.import_xml("<root>persistent</root>")
        t = r.build_tree([("doc.xml", h)])
        r.commit("main", t, author, "persistent commit")

        # Verify file was created
        assert (tmp_path / "test.db").exists()

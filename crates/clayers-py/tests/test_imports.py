"""Tests that the public API surface is importable and consistent."""


class TestTopLevelImports:
    def test_exceptions(self):
        from clayers import ClayersError, XmlError, SpecError, RepoError

    def test_knowledge_model(self):
        from clayers import KnowledgeModel

    def test_query_result(self):
        from clayers import QueryResult

    def test_queryable(self):
        from clayers import Queryable

    def test_content_hash(self):
        from clayers import ContentHash

    def test_repo_types(self):
        from clayers import Repo, MemoryStore, Author


class TestSubmoduleImports:
    def test_xml_submodule(self):
        from clayers.xml import ContentHash

    def test_repo_submodule(self):
        from clayers.repo import Repo, MemoryStore, Author

    def test_repo_aio_submodule(self):
        from clayers.repo.aio import Repo

    def test_native_module(self):
        import clayers._clayers

    def test_native_xml(self):
        from clayers._clayers.xml import ContentHash

    def test_native_repo(self):
        from clayers._clayers.repo import Repo

    def test_native_repo_aio(self):
        from clayers._clayers.repo.aio import Repo


class TestSqliteImport:
    def test_sqlite_store(self):
        from clayers.repo import SqliteStore
        from clayers import SqliteStore as SqliteStore2
        assert SqliteStore is SqliteStore2


class TestSyncAsyncRepoDistinct:
    def test_different_classes(self):
        from clayers.repo import Repo as SyncRepo
        from clayers.repo.aio import Repo as AsyncRepo
        assert SyncRepo is not AsyncRepo

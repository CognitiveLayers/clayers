"""Tests for the synchronous Repo API."""

import pytest

from clayers import Queryable, RepoError
from clayers.repo import Repo, MemoryStore, Author
from clayers.xml import ContentHash


class TestConstruction:
    def test_with_memory_store(self):
        r = Repo(MemoryStore())
        assert repr(r) == "Repo(...)"

    def test_with_invalid_store(self):
        with pytest.raises(RepoError, match="expected MemoryStore"):
            Repo("not a store")


class TestImportExport:
    def test_import_returns_hash(self, memory_repo):
        h = memory_repo.import_xml("<root>hello</root>")
        assert isinstance(h, ContentHash)
        assert len(h.hex) == 64

    def test_import_deterministic(self, memory_repo):
        h1 = memory_repo.import_xml("<root>same</root>")
        h2 = memory_repo.import_xml("<root>same</root>")
        assert h1 == h2

    def test_import_different_content_different_hash(self, memory_repo):
        h1 = memory_repo.import_xml("<a>one</a>")
        h2 = memory_repo.import_xml("<b>two</b>")
        assert h1 != h2

    def test_export_roundtrip(self, memory_repo):
        xml_in = "<root>hello world</root>"
        h = memory_repo.import_xml(xml_in)
        xml_out = memory_repo.export_xml(h)
        assert "hello world" in xml_out

    def test_export_preserves_structure(self, memory_repo):
        xml_in = "<parent><child attr='val'>text</child></parent>"
        h = memory_repo.import_xml(xml_in)
        xml_out = memory_repo.export_xml(h)
        assert "child" in xml_out
        assert "text" in xml_out


class TestTreeAndCommit:
    def test_build_tree(self, memory_repo):
        h = memory_repo.import_xml("<doc/>")
        tree = memory_repo.build_tree([("file.xml", h)])
        assert isinstance(tree, ContentHash)

    def test_build_tree_multiple_files(self, memory_repo):
        h1 = memory_repo.import_xml("<a/>")
        h2 = memory_repo.import_xml("<b/>")
        tree = memory_repo.build_tree([("a.xml", h1), ("b.xml", h2)])
        assert isinstance(tree, ContentHash)

    def test_build_tree_order_independent(self, memory_repo):
        h1 = memory_repo.import_xml("<a/>")
        h2 = memory_repo.import_xml("<b/>")
        t1 = memory_repo.build_tree([("z.xml", h1), ("a.xml", h2)])
        t2 = memory_repo.build_tree([("a.xml", h2), ("z.xml", h1)])
        assert t1 == t2

    def test_commit(self, memory_repo, author):
        h = memory_repo.import_xml("<root/>")
        tree = memory_repo.build_tree([("doc.xml", h)])
        commit = memory_repo.commit("main", tree, author, "initial commit")
        assert isinstance(commit, ContentHash)

    def test_full_workflow(self, memory_repo, author):
        h = memory_repo.import_xml("<root>data</root>")
        tree = memory_repo.build_tree([("doc.xml", h)])
        commit = memory_repo.commit("main", tree, author, "first")
        xml_out = memory_repo.export_xml(h)
        assert "data" in xml_out
        branches = memory_repo.list_branches()
        assert any(name == "main" for name, _ in branches)


class TestBranches:
    def test_list_branches_empty(self, memory_repo):
        branches = memory_repo.list_branches()
        assert branches == []

    def test_create_branch(self, memory_repo, author):
        h = memory_repo.import_xml("<r/>")
        tree = memory_repo.build_tree([("f.xml", h)])
        commit = memory_repo.commit("main", tree, author, "init")
        memory_repo.create_branch("feature", commit)
        branches = dict(memory_repo.list_branches())
        assert "feature" in branches
        assert branches["feature"] == commit

    def test_delete_branch(self, memory_repo, author):
        h = memory_repo.import_xml("<r/>")
        tree = memory_repo.build_tree([("f.xml", h)])
        commit = memory_repo.commit("main", tree, author, "init")
        memory_repo.create_branch("temp", commit)
        memory_repo.delete_branch("temp")
        branches = dict(memory_repo.list_branches())
        assert "temp" not in branches

    def test_branch_returns_content_hash(self, memory_repo, author):
        h = memory_repo.import_xml("<r/>")
        tree = memory_repo.build_tree([("f.xml", h)])
        commit = memory_repo.commit("main", tree, author, "init")
        branches = memory_repo.list_branches()
        for name, hash_val in branches:
            assert isinstance(name, str)
            assert isinstance(hash_val, ContentHash)


class TestTags:
    def test_list_tags_empty(self, memory_repo):
        tags = memory_repo.list_tags()
        assert tags == []

    def test_create_tag(self, memory_repo, author):
        h = memory_repo.import_xml("<r/>")
        tree = memory_repo.build_tree([("f.xml", h)])
        commit = memory_repo.commit("main", tree, author, "init")
        memory_repo.create_tag("v1.0", commit, author, "Release 1.0")
        tags = dict(memory_repo.list_tags())
        assert "v1.0" in tags


class TestLog:
    def test_log_single_commit(self, memory_repo, author):
        h = memory_repo.import_xml("<r/>")
        tree = memory_repo.build_tree([("f.xml", h)])
        commit = memory_repo.commit("main", tree, author, "first")
        history = memory_repo.log(commit)
        assert len(history) == 1
        assert history[0].message == "first"
        assert history[0].author.name == "Test User"
        assert history[0].author.email == "test@example.com"
        assert isinstance(history[0].tree, ContentHash)
        assert isinstance(history[0].timestamp, str)

    def test_log_multiple_commits(self, memory_repo, author):
        h1 = memory_repo.import_xml("<a/>")
        t1 = memory_repo.build_tree([("a.xml", h1)])
        memory_repo.commit("main", t1, author, "first")

        h2 = memory_repo.import_xml("<b/>")
        t2 = memory_repo.build_tree([("b.xml", h2)])
        c2 = memory_repo.commit("main", t2, author, "second")

        history = memory_repo.log(c2)
        assert len(history) == 2
        assert history[0].message == "second"
        assert history[1].message == "first"

    def test_log_with_limit(self, memory_repo, author):
        h = memory_repo.import_xml("<a/>")
        t = memory_repo.build_tree([("a.xml", h)])
        memory_repo.commit("main", t, author, "first")

        h2 = memory_repo.import_xml("<b/>")
        t2 = memory_repo.build_tree([("b.xml", h2)])
        c2 = memory_repo.commit("main", t2, author, "second")

        history = memory_repo.log(c2, limit=1)
        assert len(history) == 1


class TestDiffTrees:
    def test_diff_identical_trees(self, memory_repo, author):
        h = memory_repo.import_xml("<r/>")
        t = memory_repo.build_tree([("f.xml", h)])
        changes = memory_repo.diff_trees(t, t)
        assert changes == []

    def test_diff_added_file(self, memory_repo, author):
        h1 = memory_repo.import_xml("<a/>")
        t1 = memory_repo.build_tree([("a.xml", h1)])

        h2 = memory_repo.import_xml("<b/>")
        t2 = memory_repo.build_tree([("a.xml", h1), ("b.xml", h2)])

        changes = memory_repo.diff_trees(t1, t2)
        assert len(changes) == 1
        assert changes[0].kind == "added"
        assert changes[0].path == "b.xml"

    def test_diff_removed_file(self, memory_repo, author):
        h1 = memory_repo.import_xml("<a/>")
        h2 = memory_repo.import_xml("<b/>")
        t1 = memory_repo.build_tree([("a.xml", h1), ("b.xml", h2)])
        t2 = memory_repo.build_tree([("a.xml", h1)])

        changes = memory_repo.diff_trees(t1, t2)
        assert len(changes) == 1
        assert changes[0].kind == "removed"
        assert changes[0].path == "b.xml"

    def test_diff_modified_file(self, memory_repo, author):
        h1 = memory_repo.import_xml("<a>old</a>")
        h2 = memory_repo.import_xml("<a>new</a>")
        t1 = memory_repo.build_tree([("a.xml", h1)])
        t2 = memory_repo.build_tree([("a.xml", h2)])

        changes = memory_repo.diff_trees(t1, t2)
        assert len(changes) == 1
        assert changes[0].kind == "modified"
        assert changes[0].path == "a.xml"
        assert changes[0].old_hash is not None
        assert changes[0].new_hash is not None


class TestQueryable:
    def test_implements_protocol(self, memory_repo):
        assert isinstance(memory_repo, Queryable)


class TestAuthor:
    def test_construction(self):
        a = Author("Alice", "alice@example.com")
        assert a.name == "Alice"
        assert a.email == "alice@example.com"

    def test_repr(self):
        a = Author("Bob", "bob@test.com")
        r = repr(a)
        assert "Bob" in r
        assert "bob@test.com" in r

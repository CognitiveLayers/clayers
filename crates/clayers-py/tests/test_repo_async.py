"""Tests for the async Repo API."""

import asyncio

import pytest

from clayers.repo.aio import Repo as AsyncRepo
from clayers.repo import MemoryStore, Author
from clayers.xml import ContentHash


@pytest.fixture
def author():
    return Author("Async Tester", "async@test.com")


def run(coro):
    """Helper to run coroutines in tests without pytest-asyncio."""
    return asyncio.run(coro)


class TestImportExport:
    def test_import_returns_hash(self):
        async def go():
            r = AsyncRepo(MemoryStore())
            h = await r.import_xml("<root>hello</root>")
            assert isinstance(h, ContentHash)
            assert len(h.hex) == 64

        run(go())

    def test_export_roundtrip(self):
        async def go():
            r = AsyncRepo(MemoryStore())
            h = await r.import_xml("<root>async data</root>")
            xml = await r.export_xml(h)
            assert "async data" in xml

        run(go())


class TestFullWorkflow:
    def test_import_tree_commit_export(self, author):
        async def go():
            r = AsyncRepo(MemoryStore())
            h = await r.import_xml("<root>hello</root>")
            t = await r.build_tree([("doc.xml", h)])
            c = await r.commit("main", t, author, "initial")
            assert isinstance(c, ContentHash)

            xml = await r.export_xml(h)
            assert "hello" in xml

            branches = await r.list_branches()
            assert any(name == "main" for name, _ in branches)

        run(go())

    def test_multiple_commits(self, author):
        async def go():
            r = AsyncRepo(MemoryStore())
            h1 = await r.import_xml("<a/>")
            t1 = await r.build_tree([("a.xml", h1)])
            await r.commit("main", t1, author, "first")

            h2 = await r.import_xml("<b/>")
            t2 = await r.build_tree([("b.xml", h2)])
            c2 = await r.commit("main", t2, author, "second")

            history = await r.log(c2)
            assert len(history) == 2
            assert history[0].message == "second"

        run(go())


class TestBranches:
    def test_list_branches(self, author):
        async def go():
            r = AsyncRepo(MemoryStore())
            h = await r.import_xml("<r/>")
            t = await r.build_tree([("f.xml", h)])
            commit = await r.commit("main", t, author, "init")

            branches = await r.list_branches()
            assert any(name == "main" for name, _ in branches)
            for name, hash_val in branches:
                assert isinstance(hash_val, ContentHash)

        run(go())


class TestTags:
    def test_create_and_list_tags(self, author):
        async def go():
            r = AsyncRepo(MemoryStore())
            h = await r.import_xml("<r/>")
            t = await r.build_tree([("f.xml", h)])
            c = await r.commit("main", t, author, "init")

            await r.create_tag("v1.0", c, author, "Release 1.0")
            tags = await r.list_tags()
            assert any(name == "v1.0" for name, _ in tags)

        run(go())


class TestDiff:
    def test_diff_trees(self, author):
        async def go():
            r = AsyncRepo(MemoryStore())
            h1 = await r.import_xml("<a/>")
            h2 = await r.import_xml("<b/>")
            t1 = await r.build_tree([("a.xml", h1)])
            t2 = await r.build_tree([("a.xml", h1), ("b.xml", h2)])

            changes = await r.diff_trees(t1, t2)
            assert len(changes) == 1
            assert changes[0].kind == "added"

        run(go())


class TestQuery:
    def test_query_count(self, author):
        async def go():
            r = AsyncRepo(MemoryStore())
            xml = '<root xmlns:trm="urn:clayers:terminology"><trm:term id="t1"><trm:name>Test</trm:name></trm:term></root>'
            h = await r.import_xml(xml)
            t = await r.build_tree([("doc.xml", h)])
            await r.commit("main", t, author, "init")

            result = await r.query("//trm:term", mode="count")
            assert result.kind == "count"

        run(go())

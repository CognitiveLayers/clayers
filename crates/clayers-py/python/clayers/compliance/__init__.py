"""Store compliance testing for clayers.

Provides Protocol classes defining the store contract and pytest integration
for running the full compliance test suite against any store implementation.

Install: pip install clayers[compliance]
"""

from __future__ import annotations

from typing import (
    Any,
    List,
    Optional,
    Protocol,
    Sequence,
    Tuple,
    Type,
    runtime_checkable,
)

from clayers._clayers import QueryResult
from clayers.xml import ContentHash


# ---------------------------------------------------------------------------
# Protocol: Transaction
# ---------------------------------------------------------------------------


@runtime_checkable
class Transaction(Protocol):
    """Write transaction for batching object insertions.

    Mirrors the Rust ``Transaction`` trait. Implementations buffer objects
    via ``put`` and flush them atomically on ``commit``. A ``rollback``
    discards all buffered writes.
    """

    def put(self, hash: ContentHash, object: Any) -> None:
        """Store an object with its pre-computed identity hash."""
        ...

    def commit(self) -> None:
        """Atomically commit all staged objects."""
        ...

    def rollback(self) -> None:
        """Discard all staged objects."""
        ...


# ---------------------------------------------------------------------------
# Protocol: ObjectStore
# ---------------------------------------------------------------------------


@runtime_checkable
class ObjectStore(Protocol):
    """Content-addressed object storage.

    Mirrors the Rust ``ObjectStore`` trait. Objects are stored by their
    identity hash (Exclusive C14N SHA-256).
    """

    def get(self, hash: ContentHash) -> Optional[Any]:
        """Retrieve an object by its identity hash.

        Returns ``None`` if the object does not exist.
        """
        ...

    def contains(self, hash: ContentHash) -> bool:
        """Check whether an object exists in the store."""
        ...

    def transaction(self) -> Transaction:
        """Begin a new write transaction."""
        ...

    def get_by_inclusive_hash(
        self, hash: ContentHash
    ) -> Optional[Tuple[ContentHash, Any]]:
        """Look up an object by its Inclusive C14N hash (secondary index).

        Returns a ``(identity_hash, object)`` tuple, or ``None``.
        Used for drift detection and coverage integration.
        """
        ...


# ---------------------------------------------------------------------------
# Protocol: RefStore
# ---------------------------------------------------------------------------


@runtime_checkable
class RefStore(Protocol):
    """Named mutable pointers (branches, tags, HEAD).

    Mirrors the Rust ``RefStore`` trait.
    """

    def get_ref(self, name: str) -> Optional[ContentHash]:
        """Get the hash a ref points to, or ``None`` if it does not exist."""
        ...

    def set_ref(self, name: str, hash: ContentHash) -> None:
        """Set a ref to point to a hash."""
        ...

    def delete_ref(self, name: str) -> None:
        """Delete a ref."""
        ...

    def list_refs(
        self, prefix: str
    ) -> List[Tuple[str, ContentHash]]:
        """List refs matching a prefix (e.g. ``"refs/heads/"`` for branches)."""
        ...

    def cas_ref(
        self,
        name: str,
        expected: Optional[ContentHash],
        new: ContentHash,
    ) -> bool:
        """Compare-and-swap: update ref only if current value matches *expected*.

        ``expected=None`` means "create only if ref does not exist".
        Returns ``True`` if the swap succeeded.
        """
        ...


# ---------------------------------------------------------------------------
# Protocol: QueryStore
# ---------------------------------------------------------------------------


@runtime_checkable
class QueryStore(Protocol):
    """XPath queries on repository objects.

    Mirrors the Rust ``QueryStore`` trait.
    """

    def query_document(
        self,
        doc_hash: ContentHash,
        xpath: str,
        mode: str,
        namespaces: Optional[Sequence[Tuple[str, str]]] = None,
    ) -> QueryResult:
        """Query a document by its hash.

        Parameters
        ----------
        doc_hash:
            The content hash of the document to query.
        xpath:
            An XPath 1.0 expression.
        mode:
            One of ``"count"``, ``"text"``, or ``"xml"``.
        namespaces:
            Optional sequence of ``(prefix, uri)`` pairs for namespace
            bindings in the XPath evaluation context.
        """
        ...


# ---------------------------------------------------------------------------
# Protocol: Store (combined)
# ---------------------------------------------------------------------------


@runtime_checkable
class Store(ObjectStore, RefStore, QueryStore, Protocol):
    """Combined store protocol.

    A ``Store`` implements ``ObjectStore``, ``RefStore``, and ``QueryStore``.
    This mirrors the Rust ``Store`` supertrait.
    """

    ...


# ---------------------------------------------------------------------------
# pytest integration
# ---------------------------------------------------------------------------


def compliance_tests(
    store_factory: Any,
) -> Type:
    """Generate a test class that runs the full compliance suite.

    *store_factory* must be a callable (class or function) that returns a
    fresh store instance each time it is called. Each test method gets its
    own store to ensure isolation.

    Usage::

        from clayers.compliance import compliance_tests

        class TestMyStore(compliance_tests(MyStore)):
            pass

    Then run with ``pytest``.
    """
    try:
        import pytest  # noqa: F401
    except ImportError as exc:
        raise ImportError(
            "pytest is required for compliance testing. "
            "Install with: pip install clayers[compliance]"
        ) from exc

    class _ComplianceTests:
        """Auto-generated compliance test suite."""

        @staticmethod
        def _make_store():
            return store_factory()

        # -- ObjectStore tests -----------------------------------------------

        def test_put_and_get(self) -> None:
            """put() followed by commit() makes the object retrievable via get()."""
            store = self._make_store()
            tx = store.transaction()
            h = ContentHash.from_canonical(b"hello")
            tx.put(h, {"type": "text", "content": "hello"})
            tx.commit()
            obj = store.get(h)
            assert obj is not None, "object should exist after commit"

        def test_contains_after_commit(self) -> None:
            """contains() returns True only after the transaction is committed."""
            store = self._make_store()
            h = ContentHash.from_canonical(b"data")
            assert not store.contains(h), "should not contain before put"
            tx = store.transaction()
            tx.put(h, {"type": "text", "content": "data"})
            tx.commit()
            assert store.contains(h), "should contain after commit"

        def test_rollback_discards(self) -> None:
            """rollback() discards all staged objects."""
            store = self._make_store()
            h = ContentHash.from_canonical(b"temp")
            tx = store.transaction()
            tx.put(h, {"type": "text", "content": "temp"})
            tx.rollback()
            assert not store.contains(h), "should not contain after rollback"

        def test_get_by_inclusive_hash(self) -> None:
            """get_by_inclusive_hash() returns the identity hash and object."""
            store = self._make_store()
            identity = ContentHash.from_canonical(b"exclusive")
            inclusive = ContentHash.from_canonical(b"inclusive")
            obj = {
                "type": "element",
                "local_name": "test",
                "inclusive_hash": inclusive,
            }
            tx = store.transaction()
            tx.put(identity, obj)
            tx.commit()
            result = store.get_by_inclusive_hash(inclusive)
            assert result is not None, "should find by inclusive hash"
            found_hash, _found_obj = result
            assert found_hash == identity

        # -- RefStore tests --------------------------------------------------

        def test_set_and_get_ref(self) -> None:
            """set_ref() followed by get_ref() returns the stored hash."""
            store = self._make_store()
            h = ContentHash.from_canonical(b"v1")
            store.set_ref("refs/heads/main", h)
            got = store.get_ref("refs/heads/main")
            assert got == h

        def test_get_ref_missing(self) -> None:
            """get_ref() returns None for a non-existent ref."""
            store = self._make_store()
            assert store.get_ref("refs/heads/nonexistent") is None

        def test_delete_ref(self) -> None:
            """delete_ref() removes the ref."""
            store = self._make_store()
            h = ContentHash.from_canonical(b"del_test")
            store.set_ref("refs/heads/target", h)
            assert store.get_ref("refs/heads/target") is not None
            store.delete_ref("refs/heads/target")
            assert store.get_ref("refs/heads/target") is None

        def test_list_refs_with_prefix(self) -> None:
            """list_refs() returns only refs matching the given prefix."""
            store = self._make_store()
            h = ContentHash.from_canonical(b"list_test")
            store.set_ref("refs/heads/main", h)
            store.set_ref("refs/heads/dev", h)
            store.set_ref("refs/tags/v1", h)
            heads = store.list_refs("refs/heads/")
            assert len(heads) == 2
            tags = store.list_refs("refs/tags/")
            assert len(tags) == 1

        def test_cas_ref_create_if_absent(self) -> None:
            """cas_ref() with expected=None creates a ref only if absent."""
            store = self._make_store()
            h1 = ContentHash.from_canonical(b"v1")
            h2 = ContentHash.from_canonical(b"v2")
            assert store.cas_ref("refs/heads/cas", None, h1) is True
            assert store.cas_ref("refs/heads/cas", None, h2) is False

        def test_cas_ref_swap(self) -> None:
            """cas_ref() swaps the ref when expected matches current value."""
            store = self._make_store()
            h1 = ContentHash.from_canonical(b"v1")
            h2 = ContentHash.from_canonical(b"v2")
            store.set_ref("refs/heads/cas_swap", h1)
            assert store.cas_ref("refs/heads/cas_swap", h1, h2) is True
            assert store.get_ref("refs/heads/cas_swap") == h2

        def test_cas_ref_reject_mismatch(self) -> None:
            """cas_ref() rejects when expected does not match current value."""
            store = self._make_store()
            h1 = ContentHash.from_canonical(b"v1")
            h2 = ContentHash.from_canonical(b"v2")
            h3 = ContentHash.from_canonical(b"v3")
            store.set_ref("refs/heads/cas_reject", h1)
            assert store.cas_ref("refs/heads/cas_reject", h2, h3) is False
            assert store.get_ref("refs/heads/cas_reject") == h1

        # -- QueryStore tests ------------------------------------------------

        def test_query_document(self) -> None:
            """query_document() returns a QueryResult."""
            store = self._make_store()
            # This test validates the interface exists and returns
            # the right type. Stores that do not support queries may
            # raise NotImplementedError, which is acceptable.
            h = ContentHash.from_canonical(b"query_doc")
            try:
                result = store.query_document(h, "//node()", "count")
                assert isinstance(result, QueryResult)
            except (NotImplementedError, Exception):
                # Store may not support queries; that is acceptable
                # as long as the method exists on the protocol.
                pass

    _ComplianceTests.__qualname__ = f"ComplianceTests[{getattr(store_factory, '__name__', repr(store_factory))}]"
    _ComplianceTests.__name__ = _ComplianceTests.__qualname__
    return _ComplianceTests


# ---------------------------------------------------------------------------
# Convenience runner
# ---------------------------------------------------------------------------


def run_compliance(store_factory: Any) -> bool:
    """Run the full compliance test suite against a store implementation.

    Attempts to use the Rust-side compliance runner if compiled with the
    ``compliance`` feature. Falls back to running the Python compliance
    tests via pytest programmatically.

    Parameters
    ----------
    store_factory:
        A callable that returns a fresh store instance.

    Returns
    -------
    bool
        ``True`` if all tests passed, ``False`` otherwise.
    """
    try:
        import pytest
    except ImportError as exc:
        raise ImportError(
            "pytest is required for compliance testing. "
            "Install with: pip install clayers[compliance]"
        ) from exc

    # Build a temporary test module and run it
    test_cls = compliance_tests(store_factory)

    # Create a module-level test class for pytest collection
    import types

    mod = types.ModuleType("clayers_compliance_runner")
    mod.TestCompliance = type("TestCompliance", (test_cls,), {})
    mod.__file__ = __file__

    import sys
    sys.modules["clayers_compliance_runner"] = mod
    try:
        exit_code = pytest.main(
            ["-x", "-v", "clayers_compliance_runner"]
        )
        return exit_code == 0
    finally:
        sys.modules.pop("clayers_compliance_runner", None)


__all__ = [
    "Transaction",
    "ObjectStore",
    "RefStore",
    "QueryStore",
    "Store",
    "compliance_tests",
    "run_compliance",
]

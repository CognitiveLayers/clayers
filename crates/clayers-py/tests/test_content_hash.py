"""Tests for ContentHash wrapper."""

import pytest

from clayers.xml import ContentHash


class TestConstruction:
    def test_from_canonical(self):
        h = ContentHash.from_canonical(b"hello world")
        assert h.hex
        assert len(h.hex) == 64

    def test_from_canonical_deterministic(self):
        h1 = ContentHash.from_canonical(b"test data")
        h2 = ContentHash.from_canonical(b"test data")
        assert h1 == h2
        assert h1.hex == h2.hex

    def test_from_canonical_different_input_different_hash(self):
        h1 = ContentHash.from_canonical(b"aaa")
        h2 = ContentHash.from_canonical(b"bbb")
        assert h1 != h2
        assert h1.hex != h2.hex

    def test_from_bytes(self):
        data = bytes(range(32))
        h = ContentHash.from_bytes(data)
        assert len(h.hex) == 64

    def test_from_bytes_wrong_length(self):
        with pytest.raises(ValueError):
            ContentHash.from_bytes(b"too short")

    def test_from_hex_roundtrip(self):
        h = ContentHash.from_canonical(b"roundtrip test")
        h2 = ContentHash.from_hex(h.prefixed)
        assert h == h2

    def test_from_hex_invalid(self):
        with pytest.raises(ValueError):
            ContentHash.from_hex("not-a-hash")

    def test_from_hex_no_prefix(self):
        with pytest.raises(ValueError):
            ContentHash.from_hex("abcd" * 16)


class TestProperties:
    def test_hex_is_64_chars(self):
        h = ContentHash.from_canonical(b"x")
        assert len(h.hex) == 64
        assert all(c in "0123456789abcdef" for c in h.hex)

    def test_prefixed_starts_with_sha256(self):
        h = ContentHash.from_canonical(b"x")
        assert h.prefixed.startswith("sha256:")
        assert len(h.prefixed) == 7 + 64


class TestDunderMethods:
    def test_str(self):
        h = ContentHash.from_canonical(b"str test")
        s = str(h)
        assert s.startswith("sha256:")

    def test_repr(self):
        h = ContentHash.from_canonical(b"repr test")
        r = repr(h)
        assert "ContentHash" in r
        assert "sha256:" in r

    def test_eq(self):
        h1 = ContentHash.from_canonical(b"eq test")
        h2 = ContentHash.from_canonical(b"eq test")
        assert h1 == h2

    def test_ne(self):
        h1 = ContentHash.from_canonical(b"a")
        h2 = ContentHash.from_canonical(b"b")
        assert h1 != h2

    def test_hash_consistent(self):
        h1 = ContentHash.from_canonical(b"hash test")
        h2 = ContentHash.from_canonical(b"hash test")
        assert hash(h1) == hash(h2)

    def test_usable_as_dict_key(self):
        h = ContentHash.from_canonical(b"dict key")
        d = {h: "value"}
        h2 = ContentHash.from_canonical(b"dict key")
        assert d[h2] == "value"

    def test_usable_in_set(self):
        h1 = ContentHash.from_canonical(b"set test")
        h2 = ContentHash.from_canonical(b"set test")
        h3 = ContentHash.from_canonical(b"different")
        s = {h1, h2, h3}
        assert len(s) == 2

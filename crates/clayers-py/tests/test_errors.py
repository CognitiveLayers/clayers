"""Tests for the exception hierarchy."""

from clayers import ClayersError, XmlError, SpecError, RepoError


def test_base_is_exception():
    assert issubclass(ClayersError, Exception)


def test_xml_error_inherits():
    assert issubclass(XmlError, ClayersError)
    assert issubclass(XmlError, Exception)


def test_spec_error_inherits():
    assert issubclass(SpecError, ClayersError)


def test_repo_error_inherits():
    assert issubclass(RepoError, ClayersError)


def test_xml_error_is_catchable_as_base():
    try:
        raise XmlError("test xml error")
    except ClayersError as e:
        assert "test xml error" in str(e)


def test_spec_error_is_catchable_as_base():
    try:
        raise SpecError("test spec error")
    except ClayersError as e:
        assert "test spec error" in str(e)


def test_repo_error_is_catchable_as_base():
    try:
        raise RepoError("test repo error")
    except ClayersError as e:
        assert "test repo error" in str(e)


def test_errors_are_distinct():
    assert XmlError is not SpecError
    assert SpecError is not RepoError
    assert XmlError is not RepoError

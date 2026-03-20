from typing import Protocol, runtime_checkable


@runtime_checkable
class Queryable(Protocol):
    def query(self, xpath: str, *, mode: str = "xml") -> "QueryResult": ...

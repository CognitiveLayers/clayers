# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

This project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
While the major version remains 0, minor versions (0.x.0) may contain
breaking changes, and patch versions (0.0.x) are used for backwards-compatible
fixes and additions.

## [Unreleased]

### Added

- **Merge framework** (`clayers-repo`): three-way merge with pluggable strategies
  - `MergeStrategy` trait with built-in implementations: `Ours`, `Theirs`,
    `AutoMerge`, `Manual`
  - File-level three-way merge (`merge_trees`) classifying each path as
    unchanged, one-side-changed, convergent, or conflicting
  - Element-level three-way merge (`merge_elements`) with recursive
    identity-based child matching via `ChildKey` (by `@id` or positional)
  - Attribute-level three-way merge with deterministic ordering
  - `MergePolicy` for per-file strategy overrides
  - Merge commits with two parents
  - Sidecar divergence documents at `.clayers/divergence/{path}.{hash}`,
    keeping original documents valid during conflict resolution
  - `tree_has_divergences()` and `list_divergence_entries()` for tree-level
    conflict detection
  - `Repo::merge()` porcelain method with LCA finding, fast-forward
    detection, and merge commit creation
- **CLI `merge` command**: `clayers merge <branch> [--strategy ours|theirs|auto|manual]`
  - Reports auto-merged files, conflicts, and merge commit hash
  - Exports merged working copy to disk
  - Exits non-zero when unresolved conflicts remain

### Fixed

- `clayers log` now shows real content-addressed commit hashes instead
  of fake hashes derived from timestamp/index/string lengths
- `clayers merge` commit hash display no longer truncates into the
  `sha256:` prefix (was showing `sha256:9` instead of hex digits)

## [0.1.3] - 2025-03-19

### Fixed

- aarch64 cross-compilation: switched to native-tls with vendored OpenSSL
  headers in manylinux_2_28 container
- aarch64 manylinux container rebased from Debian to RHEL

## [0.1.2] - 2025-03-19

### Fixed

- aarch64 cross-compilation: replaced ring with a backend that
  cross-compiles in manylinux containers

## [0.1.1] - 2025-03-19

### Fixed

- aarch64 cross-compilation: replaced aws-lc-sys (requires C compiler
  targeting aarch64) with ring

## [0.1.0] - 2025-03-19

Initial public release.

### Added

- **Layered XML specification format** with orthogonal layers: prose,
  terminology, organization, relation, decision, source, plan, artifact,
  LLM description, revision
- **XSD 1.1 schemas** for all layers with OASIS XML Catalog
- **Content-addressed Merkle DAG repository** (`clayers-repo`) for XML
  documents with git-like branching, commits, and tags
- **Structural diff** exploiting Merkle hashes for short-circuit comparison
- **Conflict representation** via `<repo:divergence>` elements
- **Import/export pipeline** with namespace-preserving round-trip fidelity
- **Storage backends**: in-memory and SQLite
- **WebSocket remote transport** for push/pull between repositories
- **Commit graph** with LCA finding and history traversal
- **CLI** (`clayers`): validate, artifact (drift/coverage/fix-hash),
  connectivity, schema (RNC export), query (XPath), doc (HTML generation),
  adopt (project bootstrapping with skills)
- **Repository CLI**: init, add, rm, status, commit, log, branch, checkout,
  clone, push, pull, remote, revert, diff, serve
- **HTML documentation generator** with offline support, navigation,
  code fragments, and artifact visualization
- **Python bindings** (`clayers-py`) via PyO3
- **CI/CD pipeline** for crates.io and PyPI publishing (x86_64 + aarch64)
- Apache-2.0 license

[Unreleased]: https://github.com/CognitiveLayers/clayers/compare/v0.1.3...HEAD
[0.1.3]: https://github.com/CognitiveLayers/clayers/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/CognitiveLayers/clayers/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/CognitiveLayers/clayers/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/CognitiveLayers/clayers/releases/tag/v0.1.0

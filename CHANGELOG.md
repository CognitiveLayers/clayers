# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

This project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
While the major version remains 0, minor versions (0.x.0) may contain
breaking changes, and patch versions (0.0.x) are used for backwards-compatible
fixes and additions.

## [Unreleased]

### Added

- **`clayers search`: semantic search over a spec.** New subcommand
  that ranks nodes by meaning, complementing the XPath-based
  `clayers query`. Combines HuggingFace text embeddings
  (`fastembed`, default `bge-small-en-v1.5`) with a 256-bit
  structural fingerprint per node in a single `usearch` index using
  a weighted custom metric. Supports `--xpath`/`--layer` post-filters
  and `--alpha`/`--beta` weight tuning. Index lives in a gitignored
  `.clayers/search/` sidecar and rebuilds incrementally keyed on the
  existing C14N content hash. Bundled on by default; opt out with
  `--no-default-features` for a minimal binary.

### Fixed

- **Repository leaf hashing no longer aliases text with comments/PIs.**
  `clayers-repo` now hashes comment and processing-instruction leaves
  in explicit non-text domains, so XML like `<root>x<!--x--></root>`
  round-trips without the text node being replaced by a duplicate
  comment. Text leaves keep the legacy raw-payload hash domain, so
  existing text-only repository objects do not require migration.
- **Checkout and pull now protect local working-copy edits.**
  CLI branch/pull updates fail before rewriting files when tracked
  files have staged or unstaged local changes, and the export path
  refuses to overwrite untracked files at target paths. Pull preflights
  those target paths before syncing refs, so rejected pulls do not
  advance the current branch.

## [0.2.1] - 2026-04-18

### Added

- **Schema-driven validation** in `clayers validate`. The shipped layer
  XSDs are now enforced end-to-end on every spec file: required
  attributes, pattern facets (e.g. hash format), enumeration
  restrictions (e.g. `coverage` values), content-model conformance,
  and strict `xs:any namespace="##other"` wildcard resolution. The
  existing structural checks (well-formedness, ID uniqueness,
  relation/artifact ref resolution) still run; schema findings are
  layered on top and fail the command the same way.

### Changed

- **Schema refinements** to make the self-spec pass under the new
  validation:
  - `trm:definition` is now mixed content with inline `trm:ref` and a
    lax `xs:any ##other` (previously `xs:string` — rejected the
    inline `pr:code` / `trm:ref` usages already in the spec).
  - `org:part` accepts an optional `required` boolean (mirrors the
    same attribute on `org:topicref`; lets a whole reading-map part
    be marked supplementary).
  - `pr:section` body allows foreign-layer block elements via
    `xs:any namespace="##other" processContents="strict"`, matching
    the established pattern in `layer`/`plan`/`testing` schemas.
- **New `xmi-permissive.xsd`**: declares the `xmi:XMI` root with any
  attribute / any content so UML architecture models pass strict
  wildcard resolution without pulling in the full XMI metamodel.
  Mirrors `uml-permissive.xsd`.

## [0.2.0] - 2026-04-18

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

- **Documented `clayers query` argument order matches the CLI.**
  README and AGENTS.md examples and the XPath recipe table previously
  showed `query PATH XPATH`, but the CLI is `query XPATH [PATH]`
  (XPath required, path optional in repo mode). Following the docs
  produced "unable to open database file" errors because the XPath
  string was being interpreted as the path. All examples are now
  corrected.
- **`clayers artifact --drift` now detects spec-side drift.** Previously
  `check_single_mapping` in `clayers-spec` had a placeholder for the
  spec-side hash check that never ran, so any edit to a mapped spec
  node was silently reported as `Clean`. The combined-document
  assembly + C14N hashing pipeline already used by
  `--fix-node-hash` is now also used by `--drift`, producing
  `SpecDrifted` results when a node's current C14N hash differs from
  its stored `node-hash`. Includes a regression test
  (`spec_node_edit_is_reported_as_spec_drifted`) that fixes a hash,
  edits the node, and asserts `SpecDrifted` is returned.
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

[Unreleased]: https://github.com/CognitiveLayers/clayers/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/CognitiveLayers/clayers/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/CognitiveLayers/clayers/compare/v0.1.3...v0.2.0
[0.1.3]: https://github.com/CognitiveLayers/clayers/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/CognitiveLayers/clayers/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/CognitiveLayers/clayers/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/CognitiveLayers/clayers/releases/tag/v0.1.0

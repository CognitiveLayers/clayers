# Cognitive Layersifications

## The Problem

Software code is a degraded version of intent. Specifications are either
implicit (in developers' heads), scattered across disconnected documentation,
or maintained in heavyweight tools that drift from reality. The result:
code becomes the de facto spec, but code only captures *how*, not *why*.

We keep specifications implicit or outdated, then wonder why systems diverge
from their original intent.

## Core Thesis

Instead of prompting LLMs (or developers) to iterate directly on code, iterate
on the *specification*. The spec is the living source of truth for software
construction. Code is a produced artifact that implements the spec, and tools
can verify the mapping between them.

The specification matures over time: it starts as prose and gains precision
through additional semantic layers. Each layer adds meaning without replacing
the previous one.

## Approach: Layered XML Documents

The format is a set of XML documents, each representing a distinct layer of
meaning. Every layer has its own XML Schema 1.1 namespace and schema. Layers
are separate files for:

- **File organization**: each layer can be versioned, reviewed, and edited
  independently
- **Context size control**: an LLM updating artifact mappings doesn't need
  to load the prose layer
- **Independent evolution**: prose can go through ten revisions while the
  artifact mapping stays pinned

All layers share a common ID space (enforced by the index) and reference each
other by node ID. Every non-index layer file declares which index it belongs
to via a back-reference attribute, making each file self-describing.

## Layers

| Layer | Purpose | Rate of change |
|-------|---------|----------------|
| **Index** | File manifest, defines spec boundary and ID uniqueness domain | When spec structure changes |
| **Revision** | Named snapshots for temporal anchoring of artifact mappings | Per-release/milestone |
| **Prose** | DITA-style technical writing elements (the content itself) | When humans refine wording |
| **Terminology** | Controlled vocabulary with canonical definitions | When domain understanding evolves |
| **Organization** | Topic typing: concept, task, reference (inspired by DITA) | When spec structure changes |
| **Relation** | Typed semantic links within and across specs | When architecture evolves |
| **Artifact Mapping** | Code traceability with drift detection | When code or spec changes |
| **LLM** | Machine-readable descriptions for language model consumption | When schemas or guidance evolve |

### Layer Interactions

The prose layer borrows DITA's *internal writing elements*: paragraphs,
ordered steps, definition lists, code examples, notes, cross-references. It
defines how to write clear technical content.

The organization layer borrows DITA's *topic specialization*: it classifies
what kind of thing each node is (concept, task, reference). This is separate
from prose because *what you're writing about* is orthogonal to *how you write
it*.

The terminology layer provides a controlled vocabulary. Other layers reference
term IDs for precision: when prose says "settlement", it links to the
canonical definition.

The relation layer expresses typed, directional links: `precedes`,
`depends-on`, `refines`, `conflicts-with`, `implements`, `constrains`,
`reverses`. Relations can cross spec boundaries using a three-coordinate
address (spec + revision + node ID).

### The LLM Layer

The LLM layer embeds descriptions optimized for language model consumption.
It operates in two modes:

- **Out-of-band** (`llm.xml`): a layer file containing `<node ref="...">` elements
  that describe spec nodes (keyref-validated) and `<schema namespace="...">` elements
  that describe schemas or specific schema elements by namespace URI. This follows
  the standard layer-file pattern with independent evolution.

- **In-band** (`<llm:describe>` inside `<xs:appinfo>`): descriptions placed directly
  inside XSD schema files, co-located with the elements they describe. Any XSD can
  import the LLM namespace and add descriptions to its annotations.

The layer is self-describing: llm.xsd contains in-band descriptions of its own
elements, and the clayers's llm.xml includes out-of-band descriptions of all
schemas including itself.

### The Artifact Mapping Layer

This is the most critical layer. It maps produced artifacts (files, file
segments) to spec nodes at specific revisions. Each mapping pins:

- **Spec side**: node ID + revision name + hash of the C14N form of the node
- **Artifact side**: repo + repo revision + file path + zero or more child
  `<range>` elements, each carrying a content hash and optional addressing
  (line range, byte offsets, or line+column). Multiple ranges per artifact
  allow mapping to several non-contiguous regions of the same file.

The hashes enable **drift detection**. When a spec node changes, its hash
changes, and tooling can flag artifact mappings that reference the old hash.
When code changes, its content hash changes, and tooling can flag spec nodes
whose implementation has diverged.

Coverage is explicit: `full`, `partial`, or `none`. This makes
underimplementation directly queryable. Overimplementation (code with no
spec mapping) is detected by tooling that diffs the artifact set against
the spec node set.

Artifact addressing uses file paths at a named repository revision with
content hashes. No AST addressing in the schema itself. This is VCS-agnostic:
the format doesn't assume git, even if git is the first backend.

Source fragment canonicalization is intentionally deferred. For now, just
hashing the selected range at the current revision. False positives (cosmetic
code changes flagged as drift) are a review burden, not a correctness problem.

## Cross-Document References

XSD `xs:IDREF` is strictly single-document scoped in both XSD 1.0 and 1.1.
Since each layer is a separate document, cross-layer references use
`xs:string` with tooling-level validation.

The validator collects all IDs across all layer files (discovered via the
index manifest) and verifies every reference attribute resolves. This is
equivalent to IDREF enforcement but operates across documents.

Schematron could provide declarative cross-document rules (via XSLT's
`document()` function), but current Python Schematron libraries
(`pyschematron`) don't support `document()`. Schematron is parked for
future use with an XSLT-based processor.

## Workflow Vision

The primary workflow is co-evolution:

1. **LLM proposes spec changes**, human approves
2. **Code follows** from approved spec
3. **Tooling detects drift** between spec and code
4. **Cycle repeats**

Other workflows are possible:

- **Forward engineering**: human writes spec, LLM generates code
- **Bootstrapping**: LLM reads existing code and synthesizes a draft spec.
  The draft may be 60% accurate, but correcting is dramatically easier than
  creating from scratch. Each correction makes the spec more authoritative.

The bootstrapping story is the adoption killer feature. Nobody will rewrite
their existing system's spec from scratch. But if an LLM can produce a draft
that humans correct, adoption becomes incremental.

## Validation Tooling

The CLI (`clayers-cli`) is a PEP 723 script using `xmlschema`, `rich`,
and `click`. Single-pass validation:

The validator assembles a combined document per spec (wrapping all layer
root elements in a `<cmb:spec>` element) and validates against a
dynamically generated combined schema. Layer schemas declare their content
elements via `spec:content-element` appinfo annotations and their keyrefs
via `spec:keyref` annotations. The validator scans all `.xsd` files,
collects these annotations, and generates the combined schema at validation
time. This handles both structural validation and cross-layer referential
integrity (via `xs:unique`/`xs:keyref`) in one pass.

Spec discovery is driven by the `idx:index` attribute on each layer file,
not by filename conventions. The validator resolves the index from any file
it's given. Namespace prefixes are discovered from all `.xsd` files at
startup, not hardcoded.

```bash
# Validate a spec directory
uv run --script docs/ideas/clayers/clayers-cli validate examples/payment-processing/

# Validate a single file (resolves spec from idx:index attribute)
uv run --script docs/ideas/clayers/clayers-cli validate examples/payment-processing/overview.xml

# Fix node-hash attributes in artifact mappings (C14N + SHA-256)
uv run --script docs/ideas/clayers/clayers-cli artifact --fix-node-hash clayers/clayers/

# Fix artifact content hashes in range elements
uv run --script docs/ideas/clayers/clayers-cli artifact --fix-artifact-hash clayers/clayers/

# Check for spec-side and artifact-side drift (read-only, exit 0=clean, 1=drift)
uv run --script docs/ideas/clayers/clayers-cli artifact --drift clayers/clayers/
```

## File Structure

```
clayers/
  clayers-cli                             # PEP 723 CLI (validate, ...)
  README.md                                  # This file
  schemas/                                   # Format definition
    spec.xsd                                 # Universal root element + annotation markers
    index.xsd                                # File manifest
    revision.xsd                             # Named snapshots
    terminology.xsd                          # Controlled vocabulary
    prose.xsd                                # DITA-style writing elements
    organization.xsd                         # Topic typing
    relation.xsd                             # Semantic links
    artifact.xsd                             # Code traceability
    llm.xsd                                  # LLM descriptions (in-band + out-of-band)
  clayers/                                    # Specification instances
    clayers/                                 # Self-referential spec
      index.xml                              # Manifest
      revision.xml                           # "draft-1" snapshot
      overview.xml                           # Format intro, layers, core vocabulary
      validation.xml                         # Combined documents, cross-layer integrity
      traceability.xml                       # Artifact mapping, drift, hash tooling
      schema.xml                             # XSD design, extensibility, index, revisions
      descriptions.xml                       # LLM layer: in-band/out-of-band descriptions
  examples/                                  # Example specifications
    payment-processing/                      # Fintech domain example
      index.xml                              # Manifest
      revision.xml                           # "draft-1" snapshot
      overview.xml                           # Payment processing intro + core terms
      authorization.xml                      # Auth flow, merchants, acquirers, issuers
      settlement.xml                         # Capture, clearing, settlement
      disputes.xml                           # Chargebacks and reversals
```

## What's Next

This is a prototype. Areas to explore:

- Additional layers for more semantic precision (constraints, invariants,
  state machines, data models)
- Tooling for spec synthesis from existing codebases
- LLM integration for spec/code co-evolution workflows
- Schematron rules for declarative cross-layer constraints (when a capable
  processor is available)
- Spec diffing and merge tooling
- Visualization of spec coverage and drift

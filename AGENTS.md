# Clayers Agent Guide

Instructions for AI agents working in this repository.

## Development Workflow (MANDATORY)

**Spec first, code second.** Every change to clayers follows this order:

1. **Update the spec.** Before writing any code, update the relevant
   spec files in `clayers/clayers/` to describe what you are building.
   Add or modify prose, terminology, organization, relations, and
   LLM descriptions. If the feature is new, add a plan element.

2. **Validate the spec.** Run `cargo run -p clayers -- validate clayers/clayers/`
   to catch structural errors. Fix hashes, check drift.

3. **Implement the code.** Write the Rust (or other) code that realizes
   what the spec describes.

4. **Map spec to code.** Add or update artifact mappings linking spec
   nodes to the implementing code. Fix hashes on both sides.

5. **Iterate on quality.** After each change, check and improve:

   | Metric | Command | What to look for |
   |--------|---------|------------------|
   | **Coverage** | `cargo run -p clayers -- artifact --coverage clayers/clayers/` | Unmapped nodes, partial coverage, uncovered code ranges |
   | **Connectivity** | `cargo run -p clayers -- connectivity clayers/clayers/` | Isolated nodes, missing relations, low density |
   | **Drift** | `cargo run -p clayers -- artifact --drift clayers/clayers/` | Spec or artifact hashes out of sync |
   | **Comprehension** | `cargo run -p clayers -- schema schemas/` | Namespace declarations, layer completeness |
   | **Validation** | `cargo run -p clayers -- validate clayers/clayers/` | Structural errors, broken references |

   Iterate until coverage is full, connectivity has no isolated nodes,
   drift is clean, and validation passes.

6. **Commit.** Spec changes and code changes go in the same commit
   when they are part of the same logical unit.

**Why spec-first?** The spec is the source of truth. Writing the spec
first forces you to think through the design before coding. Artifact
mappings then provide machine-verifiable traceability between what was
specified and what was built. Drift detection catches divergence
automatically.

## Quick Reference

```bash
# Validate a spec
cargo run -p clayers -- validate clayers/clayers/

# Check for drift (read-only, exit 0=clean, 1=drift)
cargo run -p clayers -- artifact --drift clayers/clayers/

# Coverage analysis
cargo run -p clayers -- artifact --coverage clayers/clayers/

# Coverage for a specific code path
cargo run -p clayers -- artifact --coverage clayers/clayers/ --code-path src/

# Connectivity analysis
cargo run -p clayers -- connectivity clayers/clayers/

# Export schemas as RNC
cargo run -p clayers -- schema schemas/

# Export a single layer
cargo run -p clayers -- schema schemas/ --layer pr

# Fix spec-side hashes (after editing prose/terminology/etc.)
cargo run -p clayers -- artifact --fix-node-hash clayers/clayers/

# Fix artifact-side hashes (after editing code)
cargo run -p clayers -- artifact --fix-artifact-hash clayers/clayers/

# XPath query (XPath first, then path)
cargo run -p clayers -- query '//trm:term/trm:name' clayers/clayers/ --text
```

All commands accept a directory (searches recursively) or a single XML file.

## Directory Layout

```
clayers/
  AGENTS.md              # This file
  crates/                # Rust workspace
    clayers/             # CLI binary
    clayers-spec/        # Spec-aware logic (validate, drift, coverage, etc.)
    clayers-xml/         # Domain-agnostic XML utilities (C14N, RNC, catalog)
  schemas/               # XSD 1.1 schemas (one per layer)
    catalog.xml          # OASIS XML Catalog (namespace-to-file mapping)
    spec.xsd             # Root element, annotation markers
    index.xsd            # File manifest
    revision.xsd         # Named snapshots
    prose.xsd            # DITA-style writing elements
    terminology.xsd      # Controlled vocabulary
    organization.xsd     # Topic typing (concept, task, reference)
    relation.xsd         # Typed semantic links
    decision.xsd         # Decision records
    source.xsd           # External references and citations
    plan.xsd             # Implementation plans and acceptance criteria
    artifact.xsd         # Code traceability + drift detection
    llm.xsd              # Machine-readable descriptions
  clayers/              # Specification instances (knowledge base)
    clayers/             # Self-referential spec (describes the format)
      index.xml          # Manifest: lists all files in the spec
      revision.xml       # Revision layer ("draft-1")
      overview.xml       # Layers, vocabulary, file structure
      validation.xml     # Combined documents, cross-layer integrity
      traceability.xml   # Artifact mapping, drift, hash tooling
      schema.xml         # XSD design, extensibility
      descriptions.xml   # LLM layer descriptions
  examples/              # Example specifications
    payment-processing/
  clayers-harness.py     # Empirical harness for spec quality measurement
  experiments/           # Harness experiment configurations
```

## Concepts

**Spec**: a set of XML files sharing an index. Each file is a
`<spec:clayers>` document mixing content from multiple layers.

**Layer**: a semantic namespace (prose, terminology, organization, relation,
artifact, llm, revision). Each layer has its own XSD schema. Layers are
orthogonal: editing prose does not require touching artifact mappings.

**Node**: any XML element with an `id` attribute. Nodes are the unit of
cross-referencing. All IDs must be unique within a spec (enforced by the
index).

**Revision**: a named snapshot (e.g., `draft-1`). Artifact mappings pin to
revisions so hashes are temporally anchored.

**Artifact mapping**: a bidirectional link between a spec node (at a revision)
and code (at a repo revision). Both sides carry content hashes for drift
detection.

**Drift**: divergence between stored hashes and current content. Spec drift
means the node changed; artifact drift means the code changed.

## Workflow: Editing Spec Content

When you edit prose, terminology, organization, or relation content:

1. Make your XML edits
2. Run `validate` to catch structural/referential errors
3. Run `artifact --fix-node-hash` to update C14N hashes in artifact mappings
4. Run `artifact --drift` to verify no unexpected drift remains
5. Run `validate` once more (hash fixes may have changed the file)

```bash
cargo run -p clayers -- validate clayers/clayers/
cargo run -p clayers -- artifact --fix-node-hash clayers/clayers/
cargo run -p clayers -- artifact --drift clayers/clayers/
cargo run -p clayers -- validate clayers/clayers/
```

## Workflow: Updating Artifact Mappings After Code Changes

When mapped code files change (line shifts, refactors, new functions):

1. Run `artifact --drift` to see what drifted
2. Analyze the output: distinguish line-range shifts from true semantic drift
3. Update `start-line`/`end-line` attributes if ranges shifted
4. Run `artifact --fix-artifact-hash` to recompute content hashes
5. Run `artifact --drift` to confirm clean
6. Run `validate` to confirm structural integrity

```bash
cargo run -p clayers -- artifact --drift clayers/clayers/
# ... manually update line ranges in the XML if needed ...
cargo run -p clayers -- artifact --fix-artifact-hash clayers/clayers/
cargo run -p clayers -- artifact --drift clayers/clayers/
cargo run -p clayers -- validate clayers/clayers/
```

## Workflow: Adding a New Spec Node

When adding a new concept, task, or reference section:

1. Add the prose content (`<pr:section id="my-new-thing">`)
2. Add topic typing (`<org:concept ref="my-new-thing">` or `org:task`/`org:reference`)
3. Add relations (`<rel:relation>`) to existing nodes if relevant
4. Add an LLM description (`<llm:node ref="my-new-thing">`)
5. Optionally add an artifact mapping if code implements the concept
6. Validate, fix hashes, check drift

All elements go in the same file (files are organized by topic, not by layer).

## Workflow: Creating a Plan

When creating a plan for implementation work:

1. Create the plan element with title, overview, and items:
   ```xml
   <pln:plan id="plan-my-feature">
     <pln:title>Feature implementation plan</pln:title>
     <pln:overview>Brief description.</pln:overview>
     <pln:status>proposed</pln:status>
     <pln:item id="item-step-1">
       <pln:title>First step</pln:title>
       <pln:description>What to do.</pln:description>
       <pln:acceptance>
         <pln:criterion>The expected outcome.
           <pln:witness type="command">the-verification-command</pln:witness>
         </pln:criterion>
       </pln:acceptance>
       <pln:item-status>pending</pln:item-status>
     </pln:item>
   </pln:plan>
   ```
2. Use `rel:relation type="depends-on"` or `type="precedes"` for item ordering
3. For rich content, embed prose elements (`pr:p`, `pr:section`, `trm:ref`, `src:cite`) directly inside plans/items
4. Add witnesses to criteria: `command` (shell), `script` (executable, with `lang`), `manual` (human, with `role`)
5. Validate, fix hashes, check drift (standard workflow)

## Workflow: Adding a New Artifact Mapping

```xml
<art:mapping id="map-my-thing">
  <art:spec-ref node="my-thing"
            revision="draft-1"
            node-hash="sha256:placeholder"/>
  <art:artifact repo="clayers"
            repo-revision="COMMIT_HASH"
            path="crates/clayers-spec/src/thing.rs">
    <art:range hash="sha256:placeholder"
               start-line="10" end-line="50"/>
  </art:artifact>
  <art:coverage>full</art:coverage>
  <art:note>Brief description of what the code implements.</art:note>
</art:mapping>
```

Then:
```bash
cargo run -p clayers -- artifact --fix-node-hash clayers/clayers/
cargo run -p clayers -- artifact --fix-artifact-hash clayers/clayers/
cargo run -p clayers -- validate clayers/clayers/
```

Use `repo-revision` from `git rev-parse --short HEAD`. Multiple
`<art:range>` elements per artifact are allowed for non-contiguous
code regions.

## Drift Detection Output

```
drift: clayers (23 mappings, 2 drifted)

  map-overview-readme: OK
  map-term-node: OK

  map-artifact-drift: ARTIFACT DRIFTED
    file: crates/clayers-spec/src/drift.rs:425-491
    stored:  sha256:d4ca047eaa97...
    current: sha256:abc123def456...

  map-term-drift: SPEC DRIFTED
    node: term-drift (revision: draft-1)
    stored:  sha256:19d35c7c6a82...
    current: sha256:def456abc123...
```

- **OK**: hashes match on both sides
- **ARTIFACT DRIFTED**: code changed
- **SPEC DRIFTED**: spec node changed, shows stored vs current node-hash

## XML Conventions

**Namespace prefixes** (consistent across all files):

| Prefix | URI | Purpose |
|--------|-----|---------|
| `spec` | `urn:clayers:spec` | Root element, index attribute |
| `pr` | `urn:clayers:prose` | Prose content |
| `trm` | `urn:clayers:terminology` | Terms and definitions |
| `org` | `urn:clayers:organization` | Topic typing |
| `rel` | `urn:clayers:relation` | Semantic links |
| `dec` | `urn:clayers:decision` | Decision metadata |
| `src` | `urn:clayers:source` | Source declarations, inline citations |
| `pln` | `urn:clayers:plan` | Implementation plans and acceptance criteria |
| `art` | `urn:clayers:artifact` | Code traceability |
| `llm` | `urn:clayers:llm` | Machine descriptions |
| `rev` | `urn:clayers:revision` | Named snapshots |
| `idx` | `urn:clayers:index` | File manifest |

**Every file** must have a `<spec:clayers>` root with `spec:index="index.xml"`.

**IDs must be unique** across the entire spec (all files combined). The
combined schema enforces this via `xs:unique`.

**Cross-references** use `xs:string` attributes (not `xs:IDREF`) because
layers are separate documents. The validator checks all references resolve
across the combined document.

## Common Errors

| Error | Cause | Fix |
|-------|-------|-----|
| `value doesn't match any pattern of ['+:+']` | Placeholder hash like `sha256:placeholder` | Run `--fix-node-hash` and `--fix-artifact-hash` |
| `key sequence ... not found` | Dangling reference to nonexistent ID | Check spelling of `ref`, `node`, `from`, `to` attributes |
| `duplicate key value` | Two elements share the same `id` | Rename one of them |
| `not accepted by the model group` | Wrong element in wrong context | Check the schema for allowed children |

## Rules for Agents

1. **Spec first, code second.** Always update the spec before implementing.
   See [Development Workflow](#development-workflow-mandatory).
2. **Always validate after changes.** No exceptions.
3. **Fix hashes before committing.** Run `--fix-node-hash` after prose edits,
   `--fix-artifact-hash` after code edits. Both if unsure.
4. **Check drift before and after.** Use `--drift` to understand the current
   state before making changes, and to verify clean state after.
5. **Iterate on quality metrics.** After implementing, check coverage,
   connectivity, and drift. Fix gaps before considering the work done.
6. **One file = one topic.** Files mix layers (prose + org + rel + art + llm
   for the same topic). Do not organize by layer type.
7. **Keep IDs stable.** Renaming an ID breaks all references to it across
   all files. If you must rename, search all XML files for the old ID.
8. **Use placeholders for new hashes.** Write `sha256:placeholder` and let
   the tooling compute the real values.
9. **Update line ranges when code moves.** After refactors, check
   `--drift` output and update `start-line`/`end-line` attributes manually.
   Then run `--fix-artifact-hash` to recompute content hashes.

## Using Specs as a Knowledge Base

The `query` command and the layered schema system turn every spec into a
structured knowledge base. Instead of grepping XML files, you can run typed
XPath 1.0 queries against the fully assembled combined document (including
XSLT-synthesized relations).

CLI: `clayers query <XPATH> [PATH] [OPTIONS]`. The XPath is the
required positional, the spec/repo path comes after it (optional in
repo mode). Examples below abbreviate the path as `PATH`.

Quick example:
```bash
cargo run -p clayers -- query '//trm:term[@id="term-drift"]/trm:definition' clayers/clayers/ --text
```

### XPath Recipe Table

| Task | Command |
|------|---------|
| Find a term's definition | `query '//trm:term[@id="ID"]/trm:definition' PATH --text` |
| List all terms | `query '//trm:term/trm:name' PATH --text` |
| Find code for a concept | `query '//art:mapping[art:spec-ref/@node="ID"]' PATH` |
| Get file paths for a node | `query '//art:mapping[art:spec-ref/@node="ID"]/art:artifact/@path' PATH` |
| Get line ranges | `query '//art:mapping[art:spec-ref/@node="ID"]//art:range/@start-line' PATH` |
| Get LLM description | `query '//llm:node[@ref="ID"]' PATH --text` |
| Find relations from a node | `query '//rel:relation[@from="ID"]' PATH` |
| Find relations to a node | `query '//rel:relation[@to="ID"]' PATH` |
| List all sections | `query '//pr:section/pr:title' PATH --text` |
| Count terms | `query '//trm:term' PATH --count` |
| Find depends-on relations | `query '//rel:relation[@type="depends-on"]' PATH` |

### Workflow: Understanding a Concept End-to-End

1. Find the term definition: `query '//trm:term[@id="ID"]/trm:definition' PATH --text`
2. Read the LLM description: `query '//llm:node[@ref="ID"]' PATH --text`
3. Find what it depends on: `query '//rel:relation[@from="ID"]' PATH`
4. Find what depends on it: `query '//rel:relation[@to="ID"]' PATH`
5. Locate implementing code: `query '//art:mapping[art:spec-ref/@node="ID"]/art:artifact/@path' PATH`

### Workflow: Finding Code That Implements a Spec Concept

1. Get artifact mappings: `query '//art:mapping[art:spec-ref/@node="ID"]' PATH`
2. Extract file paths: `query '//art:mapping[art:spec-ref/@node="ID"]/art:artifact/@path' PATH`
3. Extract line ranges: `query '//art:mapping[art:spec-ref/@node="ID"]//art:range/@start-line' PATH`

### Notes

- **XPath 1.0 only**: no regex, no `if/then/else`, no string functions beyond
  `contains()`, `starts-with()`, `substring()`, etc.
- **Namespace prefixes**: `spec`, `pr`, `trm`, `org`, `rel`, `art`, `llm`,
  `rev`, `idx`, `cmb` (all pre-registered).
- **Output is raw**: no Rich formatting on results, so output can be piped
  into other tools.

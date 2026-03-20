---
name: clayers-onboard
description: >
  Systematically onboard this project to clayers specifications. Analyzes the
  entire codebase, creates structured specs with prose/terminology/relations/LLM
  descriptions, maps all code with artifact traceability, and iterates until
  coverage, connectivity, and drift are all clean. Use when: "onboard to clayers",
  "create specs", "clayers-onboard", "drive coverage to 100%".
argument-hint: "[--resume | --phase N]"
---

# Clayers Onboard

Systematically create a complete clayers specification for **{{PROJECT_NAME}}**.

The goal: every meaningful code construct is described in the spec, mapped via
artifact traceability, and verified clean by the tooling. This is not a quick
sketch -- it is a thorough, exhaustive extraction that leaves no code unmapped.

## Prerequisites

Before starting, verify:

1. **clayers is installed**: run `clayers --version`
2. **Project is adopted**: `.clayers/schemas/` exists with XSD files, and
   `clayers/{{PROJECT_NAME}}/index.xml` exists. If not, run `clayers adopt .`
3. **You can read the entire codebase**: you need access to all source files

If resuming (`--resume`), skip to the phase where you left off. Run
`clayers artifact --coverage clayers/{{PROJECT_NAME}}/` and
`clayers connectivity clayers/{{PROJECT_NAME}}/` to assess current state.

---

## Phase 1: Codebase Discovery

**Goal**: Build a complete mental model of the project before writing any XML.

### 1.1 Inventory source directories

Scan the project tree. For each directory containing source code:
- Note its purpose (e.g., `src/auth/` = authentication logic)
- Count files and approximate lines of code
- Identify the primary language

Skip: `node_modules/`, `target/`, `.git/`, `vendor/`, build outputs.

### 1.2 Catalog modules and components

For each source directory, identify:
- **Modules/packages**: logical groupings of code (e.g., `auth`, `api`, `db`)
- **Entry points**: main functions, HTTP handlers, CLI commands, event handlers
- **Data types**: structs, classes, enums, interfaces that carry domain semantics
- **Key functions**: functions that implement core business logic
- **External boundaries**: API endpoints, database queries, file I/O, network calls

### 1.3 Extract domain concepts

Read through the code and documentation to identify:
- **Domain terms**: words with specific meaning in this project (e.g., "workspace",
  "tenant", "pipeline", "artifact")
- **Concept relationships**: which concepts depend on, refine, or reference others
- **Concept hierarchy**: parent-child relationships between concepts

### 1.4 Map dependencies

For each module/component, note:
- What it depends on (imports, calls)
- What depends on it (reverse imports)
- External dependencies (libraries, services)

### 1.5 Document your findings

Write a structured summary as notes. This will drive Phases 2-4. Organize by:

```
Module: <name>
  Purpose: <one sentence>
  Files: <list>
  Key types: <list>
  Key functions: <list>
  Domain concepts: <list>
  Depends on: <list of other modules>
```

**Success criteria**: You can describe every source directory's purpose and how
modules relate to each other.

---

## Phase 2: Terminology Extraction

**Goal**: Create `trm:term` elements for every domain concept.

### 2.1 Create a terminology file

If the spec doesn't already have terminology, create a file for it.
Register it in `clayers/{{PROJECT_NAME}}/index.xml`:

```xml
<!-- In index.xml, add: -->
<file href="terminology.xml" layer="urn:clayers:terminology"/>
```

Or add terms to existing topic files (preferred when terms are closely tied
to a specific topic).

### 2.2 Write term elements

For each domain concept from Phase 1, create a term:

```xml
<trm:term id="term-{concept-name}">
  <trm:name>{Concept Name}</trm:name>
  <trm:definition>{Precise, one-paragraph definition. State what it IS,
  not what it does. Include scope, constraints, and relationships to
  other terms. Reference other terms with trm:ref where appropriate.}</trm:definition>
</trm:term>
```

**Guidelines**:
- IDs must be unique across the entire spec. Use `term-` prefix.
- Definitions should be self-contained: a reader unfamiliar with the code
  should understand the concept from the definition alone.
- Cross-reference related terms: `<trm:ref term="term-other">other</trm:ref>`
- Group related terms in the same file.

### 2.3 Validate

After adding terms, validate:

```bash
clayers validate clayers/{{PROJECT_NAME}}/
```

Fix any errors before proceeding.

**Success criteria**: Every domain concept from Phase 1 has a `trm:term` with
a precise definition. Validation passes.

---

## Phase 3: Spec File Creation

**Goal**: Create spec files organized by topic, each containing all layers
for that topic.

### 3.1 Organize by topic, not by layer

Each spec file should cover one major topic (module, component, or concept
cluster). A single file contains prose, organization, relations, and LLM
descriptions for its topic. Do NOT create separate files per layer type.

File naming: `{topic-name}.xml` (kebab-case).

### 3.2 File template

Every spec file must have this structure:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!--
  {Topic title}: {Brief description of what this file covers.}
-->
<spec:clayers xmlns:spec="urn:clayers:spec"
       xmlns:pr="urn:clayers:prose"
       xmlns:trm="urn:clayers:terminology"
       xmlns:org="urn:clayers:organization"
       xmlns:rel="urn:clayers:relation"
       xmlns:art="urn:clayers:artifact"
       xmlns:llm="urn:clayers:llm"
       spec:index="index.xml">

  <!-- Vocabulary (terms specific to this topic) -->

  <trm:term id="term-{topic-specific-concept}">
    <trm:name>{Concept}</trm:name>
    <trm:definition>{Definition}</trm:definition>
  </trm:term>

  <!-- Content -->

  <pr:section id="{topic-id}">
    <pr:title>{Topic Title}</pr:title>
    <pr:shortdesc>{One-sentence summary.}</pr:shortdesc>

    <pr:p>{Detailed prose describing what this component/module does,
    how it works, its design rationale, and its role in the system.
    Use multiple paragraphs if needed.}</pr:p>

    <pr:p>{Additional paragraphs for subtopics, algorithms, data flow,
    error handling, etc.}</pr:p>
  </pr:section>

  <!-- Subsections for major parts of the topic -->

  <pr:section id="{topic-subtopic}">
    <pr:title>{Subtopic}</pr:title>
    <pr:p>{Description of this specific aspect.}</pr:p>
  </pr:section>

  <!-- Topic typing -->

  <org:concept ref="{topic-id}">
    <org:purpose>{Why this concept exists in the system.}</org:purpose>
  </org:concept>

  <!-- Use org:task for procedural/action topics -->
  <!--
  <org:task ref="{topic-id}">
    <org:purpose>{What this task accomplishes.}</org:purpose>
    <org:actor>{Who or what performs it.}</org:actor>
  </org:task>
  -->

  <!-- Use org:reference for API/config/data-structure topics -->
  <!--
  <org:reference ref="{topic-id}">
    <org:purpose>{What information this reference provides.}</org:purpose>
  </org:reference>
  -->

  <!-- Relations -->

  <rel:relation type="depends-on" from="{topic-id}" to="{other-topic-id}">
    <rel:note>{Why this dependency exists.}</rel:note>
  </rel:relation>

  <rel:relation type="refines" from="{subtopic-id}" to="{topic-id}">
    <rel:note>{How the subtopic refines the parent topic.}</rel:note>
  </rel:relation>

  <!-- Machine descriptions -->

  <llm:node ref="{topic-id}">
    {Concise description for LLM consumption. Include: what it is,
    what it does, key behaviors, relationships to other components.
    Written as plain text, not XML.}
  </llm:node>

</spec:clayers>
```

### 3.3 Register every file in index.xml

For each new spec file, add an entry to `clayers/{{PROJECT_NAME}}/index.xml`:

```xml
<file href="{filename}.xml" layer="urn:clayers:prose"/>
```

Use the primary layer as the `layer` attribute (usually `urn:clayers:prose`
for topic files).

### 3.4 Relation types

Use these relation types to connect spec nodes:

| Type | Meaning | Example |
|------|---------|---------|
| `depends-on` | A requires B to function | auth depends-on database |
| `refines` | A is a more specific version of B | jwt-auth refines auth |
| `references` | A mentions or uses B | api-handler references auth |
| `precedes` | A must happen before B | init precedes serve |
| `extends` | A adds capabilities to B | plugin extends core |

### 3.5 Validate after each file

```bash
clayers validate clayers/{{PROJECT_NAME}}/
```

Common errors:
- Duplicate IDs: rename one
- Dangling references: check spelling of `ref`, `from`, `to` attributes
- Missing index entry: add `<file>` element to index.xml

**Success criteria**: Every major module/component from Phase 1 has a spec
file with prose, organization, relations, and LLM descriptions. All files
registered in index.xml. Validation passes with zero errors.

---

## Phase 4: Artifact Mapping

**Goal**: Link every spec node to its implementing code with precise
line ranges.

### 4.1 Determine the revision name

Check `clayers/{{PROJECT_NAME}}/revision.xml` for the current revision name
(usually `draft-1`).

### 4.2 Create artifact mappings

For each spec node that describes code (sections, terms that map to
implementations), add an artifact mapping. Mappings go in the same file
as the spec node they reference.

```xml
<art:mapping id="map-{descriptive-name}">
  <art:spec-ref node="{spec-node-id}"
            revision="draft-1"
            node-hash="sha256:placeholder"/>
  <art:artifact repo="{{PROJECT_NAME}}"
            repo-revision="HEAD"
            path="{relative/path/to/file.rs}">
    <art:range hash="sha256:placeholder"
               start-line="{start}" end-line="{end}"/>
  </art:artifact>
  <art:coverage>full</art:coverage>
  <art:note>{Brief description of what the code implements.}</art:note>
</art:mapping>
```

**Guidelines**:
- `node` must reference an existing spec node ID
- `path` is relative to the repo root
- `start-line` and `end-line` define the exact code range (inclusive)
- Use multiple `<art:range>` elements for non-contiguous code regions
- `coverage` is `full` if the range covers the complete implementation,
  `partial` if it covers only part
- Use `sha256:placeholder` for hashes -- the tooling will compute real values

### 4.3 Fix hashes

After adding mappings, compute real hashes:

```bash
clayers artifact --fix-node-hash clayers/{{PROJECT_NAME}}/
clayers artifact --fix-artifact-hash clayers/{{PROJECT_NAME}}/
```

### 4.4 Check drift

Verify everything is clean:

```bash
clayers artifact --drift clayers/{{PROJECT_NAME}}/
```

All mappings should show `OK`. If any show `DRIFTED`, investigate:
- **SPEC DRIFTED**: the spec node changed after the mapping was created.
  Run `--fix-node-hash` again.
- **ARTIFACT DRIFTED**: the code changed. Update `start-line`/`end-line`
  if lines shifted, then run `--fix-artifact-hash`.
- **UNAVAILABLE**: the file path doesn't exist. Fix the `path` attribute.

### 4.5 Exempt non-code nodes

Some spec nodes are abstract concepts, design rationale, or process
descriptions that don't map to code. Mark them as exempt:

```xml
<art:exempt node="{abstract-concept-id}"/>
```

### 4.6 Validate

```bash
clayers validate clayers/{{PROJECT_NAME}}/
```

**Success criteria**: Every spec node is either mapped to code or explicitly
exempted. `--drift` shows zero drift. Validation passes.

---

## Phase 5: Quality Iteration

**Goal**: Drive all quality metrics to their targets through iterative
improvement.

This phase is a loop. Run all three checks, fix issues, repeat.

### 5.1 Coverage check

```bash
clayers artifact --coverage clayers/{{PROJECT_NAME}}/
```

**Read the output carefully:**

- **Unmapped nodes**: spec nodes with no artifact mapping and no exemption.
  For each: either add a mapping or mark as exempt.
- **Code coverage by file**: shows which lines are covered and which are not.
  For each uncovered range: either add a spec node + mapping for that code,
  or extend an existing mapping to include it.

**Target**: zero unmapped nodes, maximize code line coverage.

To check coverage for a specific code path:
```bash
clayers artifact --coverage clayers/{{PROJECT_NAME}}/ --code-path src/
```

### 5.2 Connectivity check

```bash
clayers connectivity clayers/{{PROJECT_NAME}}/
```

**Read the output carefully:**

- **Isolated nodes**: nodes with no relations. For each: add at least one
  `rel:relation` connecting it to a related node.
- **Connected components**: ideally one large component. Multiple components
  suggest missing relations between concept clusters.
- **Acyclic violations**: cycles in relation types that should be acyclic
  (like `depends-on`). Break cycles by removing or redirecting a relation.

**Target**: zero isolated nodes, one connected component, zero acyclic
violations.

### 5.3 Drift check

```bash
clayers artifact --drift clayers/{{PROJECT_NAME}}/
```

**Target**: zero drifted mappings. If any drift:
- Spec drifted: `clayers artifact --fix-node-hash clayers/{{PROJECT_NAME}}/`
- Artifact drifted: update line ranges if shifted, then
  `clayers artifact --fix-artifact-hash clayers/{{PROJECT_NAME}}/`

### 5.4 Iteration loop

Repeat 5.1-5.3 until all three checks are clean. After fixing issues in
one area, the others may need re-checking (e.g., adding a mapping for
coverage creates a new node that needs relations for connectivity).

**Success criteria**: coverage shows zero unmapped nodes, connectivity shows
zero isolated nodes and zero acyclic violations, drift shows zero drifted
mappings.

---

## Phase 6: Final Validation

**Goal**: Verify everything is consistent and complete.

Run all checks in sequence:

```bash
# 1. Structural validation
clayers validate clayers/{{PROJECT_NAME}}/

# 2. Drift detection
clayers artifact --drift clayers/{{PROJECT_NAME}}/

# 3. Coverage analysis
clayers artifact --coverage clayers/{{PROJECT_NAME}}/

# 4. Connectivity analysis
clayers connectivity clayers/{{PROJECT_NAME}}/
```

All must pass cleanly:
- Validation: `OK (no structural errors)`
- Drift: `0 drifted`
- Coverage: zero unmapped nodes
- Connectivity: zero isolated nodes, zero acyclic violations

If any check fails, return to Phase 5 and iterate.

**Success criteria**: All four checks pass. The specification is complete.

---

## Reference: Namespace Declarations

Every spec file must declare the namespaces it uses. Copy this block
as needed:

```xml
<spec:clayers xmlns:spec="urn:clayers:spec"
       xmlns:pr="urn:clayers:prose"
       xmlns:trm="urn:clayers:terminology"
       xmlns:org="urn:clayers:organization"
       xmlns:rel="urn:clayers:relation"
       xmlns:dec="urn:clayers:decision"
       xmlns:src="urn:clayers:source"
       xmlns:pln="urn:clayers:plan"
       xmlns:art="urn:clayers:artifact"
       xmlns:llm="urn:clayers:llm"
       xmlns:rev="urn:clayers:revision"
       spec:index="index.xml">
```

Only include namespaces you actually use in the file.

## Reference: Complete Element Patterns

### Term

```xml
<trm:term id="term-example">
  <trm:name>Example</trm:name>
  <trm:definition>A precise definition. References
  <trm:ref term="term-other">other term</trm:ref> inline.</trm:definition>
</trm:term>
```

### Prose section

```xml
<pr:section id="example-section">
  <pr:title>Example Section</pr:title>
  <pr:shortdesc>One sentence summary.</pr:shortdesc>
  <pr:p>Paragraph text. Use <pr:code>inline code</pr:code>,
  <pr:b>bold</pr:b>, and <pr:i>italic</pr:i> as needed.</pr:p>
  <pr:note type="info">Informational note.</pr:note>
</pr:section>
```

### Ordered and unordered lists

```xml
<pr:ol>
  <pr:li>First item</pr:li>
  <pr:li>Second item</pr:li>
</pr:ol>

<pr:ul>
  <pr:li>Bullet item</pr:li>
</pr:ul>
```

### Topic typing

```xml
<!-- For concepts (what something IS) -->
<org:concept ref="section-id">
  <org:purpose>Why this concept exists.</org:purpose>
</org:concept>

<!-- For tasks (how to DO something) -->
<org:task ref="section-id">
  <org:purpose>What this task accomplishes.</org:purpose>
  <org:actor>Who or what performs it.</org:actor>
</org:task>

<!-- For reference material (lookup information) -->
<org:reference ref="section-id">
  <org:purpose>What information this provides.</org:purpose>
</org:reference>
```

### Relation

```xml
<rel:relation type="depends-on" from="node-a" to="node-b">
  <rel:note>Why A depends on B.</rel:note>
</rel:relation>
```

Types: `depends-on`, `refines`, `references`, `precedes`, `extends`.

### Artifact mapping

```xml
<art:mapping id="map-descriptive-name">
  <art:spec-ref node="spec-node-id"
            revision="draft-1"
            node-hash="sha256:placeholder"/>
  <art:artifact repo="{{PROJECT_NAME}}"
            repo-revision="HEAD"
            path="src/module.rs">
    <art:range hash="sha256:placeholder"
               start-line="10" end-line="50"/>
  </art:artifact>
  <art:coverage>full</art:coverage>
  <art:note>What the code implements.</art:note>
</art:mapping>

<!-- For non-contiguous code: multiple ranges -->
<art:mapping id="map-split-impl">
  <art:spec-ref node="spec-node-id"
            revision="draft-1"
            node-hash="sha256:placeholder"/>
  <art:artifact repo="{{PROJECT_NAME}}"
            repo-revision="HEAD"
            path="src/module.rs">
    <art:range hash="sha256:placeholder"
               start-line="10" end-line="30"/>
    <art:range hash="sha256:placeholder"
               start-line="80" end-line="95"/>
  </art:artifact>
  <art:coverage>full</art:coverage>
</art:mapping>

<!-- Exempt abstract/design nodes from mapping -->
<art:exempt node="abstract-concept-id"/>
```

### LLM description

```xml
<llm:node ref="spec-node-id">
  Concise description for machine consumption. Include: what it is,
  what it does, key behaviors, important relationships. Plain text,
  not XML. Keep under 200 words.
</llm:node>
```

### Decision record

```xml
<dec:decision id="dec-example" status="accepted" date="2025-01-15">
  <dec:title>Use X instead of Y</dec:title>
  <dec:context ref="relevant-section-id"/>
  <dec:rationale>Why this decision was made.</dec:rationale>
  <dec:consequence>What follows from this decision.</dec:consequence>
</dec:decision>
```

## Reference: Common Pitfalls

| Mistake | Problem | Fix |
|---------|---------|-----|
| Duplicate IDs | Validation error | Ensure IDs are unique across ALL spec files |
| Missing index entry | File is ignored | Add `<file>` to index.xml |
| Wrong namespace prefix | Element not recognized | Check namespace declarations |
| `sha256:placeholder` left after fix | Drift shows unavailable | Run `--fix-node-hash` and `--fix-artifact-hash` |
| Line ranges wrong | Artifact drift | Check `start-line`/`end-line`, run `--fix-artifact-hash` |
| Organizing by layer type | Hard to maintain | Organize by topic: one file per module/component |
| Circular `depends-on` | Acyclic violation | Restructure to break the cycle |
| Overly broad mapping | Low traceability | Use precise `start-line`/`end-line` ranges |
| Missing `spec:index` attribute | File not associated with spec | Add `spec:index="index.xml"` to root element |

## Reference: Useful Commands

```bash
# Validate
clayers validate clayers/{{PROJECT_NAME}}/

# Coverage (spec nodes + code lines)
clayers artifact --coverage clayers/{{PROJECT_NAME}}/
clayers artifact --coverage clayers/{{PROJECT_NAME}}/ --code-path src/

# Connectivity (graph metrics)
clayers connectivity clayers/{{PROJECT_NAME}}/

# Drift detection
clayers artifact --drift clayers/{{PROJECT_NAME}}/

# Fix hashes (after editing spec or code)
clayers artifact --fix-node-hash clayers/{{PROJECT_NAME}}/
clayers artifact --fix-artifact-hash clayers/{{PROJECT_NAME}}/

# Query the spec (XPath)
clayers query clayers/{{PROJECT_NAME}}/ '//trm:term/trm:name' --text
clayers query clayers/{{PROJECT_NAME}}/ '//art:mapping' --count
clayers query clayers/{{PROJECT_NAME}}/ '//rel:relation[@type="depends-on"]'

# List all spec node IDs
clayers query clayers/{{PROJECT_NAME}}/ '//*[@id]/@id' --text

# Find unmapped nodes
clayers query clayers/{{PROJECT_NAME}}/ '//*[@id]/@id' --text
# Compare against mapped nodes:
clayers query clayers/{{PROJECT_NAME}}/ '//art:mapping/art:spec-ref/@node' --text
```

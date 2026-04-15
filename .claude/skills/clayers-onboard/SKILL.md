---
name: clayers-onboard
description: >
  Systematically onboard this project to clayers specifications. Discovers
  layers and their onboarding modes from the spec, executes them in
  dependency order, and iterates until all quality metrics are clean.
  Use when: "onboard to clayers", "create specs", "drive coverage to 100%",
  "catch up", "fill spec gaps".
argument-hint: "[--resume | --catch-up]"
allowed-tools:
  - Bash(clayers validate *)
  - Bash(clayers artifact *)
  - Bash(clayers connectivity *)
  - Bash(clayers query *)
  - Bash(clayers schema *)
---

# Clayers Onboard

Systematically create a complete clayers specification for **main**.

The goal: every meaningful code construct is described in the spec, mapped
via artifact traceability, and verified clean by the tooling.

## Prerequisites

1. **clayers is installed**: run `clayers --version`
2. **Project is adopted**: `.clayers/schemas/` exists and
   `clayers/main/index.xml` exists. If not, run `clayers adopt .`

## Mode Selection

- **Fresh onboarding** (default): start from the discovery phase
- **Resume** (`--resume`): check which acceptance criteria already pass
  and continue from the first incomplete mode
- **Catch-up** (`--catch-up`): query `lyr:mode[@name="catch-up"]` instead
  of onboard modes to fill gaps in an existing spec

## Phase 1: Codebase Discovery

Before executing layer modes, build a mental model:

1. **Scan the project tree**: identify source directories, languages, build system
2. **Catalog modules**: for each directory/package, note its purpose
3. **Extract domain concepts**: identify domain terms from type names,
   function names, constants, comments (feeds into terminology layer)
4. **Map dependencies**: what depends on what (feeds into relation layer)

This mental model guides the layer mode execution that follows.

## Phase 2: Layer Mode Discovery

Query the project spec for layers with onboard modes:

```bash
clayers query '//lyr:layer' clayers/main/
clayers query '//lyr:mode[@name="onboard"]' clayers/main/
```

Each `lyr:layer` describes a layer. Each `lyr:mode[@name="onboard"]` inside
declares how to onboard that layer, including:
- `lyr:requires`: prerequisite layers
- `lyr:condition`: when this mode applies
- `lyr:acceptance`: success criteria
- `pr:section` body: step-by-step guidance

## Phase 3: Topological Execution

Build a dependency graph from `lyr:requires` elements and execute modes
in topological order:

1. **For each mode in order**:
   - Check `lyr:condition` against the project state. Skip if it doesn't apply.
   - Read the mode body (the `pr:section` inside contains the guidance).
   - Follow any `pr:xref ref="tpl-*"` links to templates in `templates.xml`
     for XML boilerplate.
   - Execute the guidance: create spec elements as instructed.
   - Validate: `clayers validate clayers/main/`
   - Check `lyr:acceptance` criteria.
2. **If acceptance criteria fail**: re-read the mode guidance and fix gaps
   before moving on.

## Phase 4: Quality Iteration

After all modes execute, run the full quality suite:

```bash
clayers artifact --coverage clayers/main/
clayers connectivity clayers/main/
clayers artifact --drift clayers/main/
clayers validate clayers/main/
```

Targets:
- **Coverage**: zero unmapped nodes
- **Connectivity**: zero isolated nodes
- **Drift**: zero drifted mappings
- **Validation**: no structural errors

For any metric that fails, re-execute the relevant layer's catch-up or
review mode. Iterate until all metrics are clean.

## Reading Mode Guidance

To read a specific mode's guidance:

```bash
clayers query '//lyr:mode[@id="mode-trm-onboard"]' clayers/main/
```

The mode body contains a `pr:section` with detailed step-by-step guidance,
template references, and anti-patterns. Follow it precisely.

## Catch-Up Mode

When invoked with `--catch-up`, query for catch-up modes instead:

```bash
clayers query '//lyr:mode[@name="catch-up"]' clayers/main/
```

Execute the same phases 2-4, using catch-up modes instead of onboard modes.
Catch-up modes typically focus on gap analysis and targeted fixes rather
than full layer construction.

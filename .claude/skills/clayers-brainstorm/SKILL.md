---
name: clayers-brainstorm
description: >
  Design features as structured clayers XML specs. Discovers brainstorm
  modes from the spec and engages relevant layers to produce prose,
  terminology, relations, and LLM descriptions before code exists.
  Use when: "design feature", "brainstorm", "create spec for".
allowed-tools:
  - Bash(clayers validate *)
  - Bash(clayers query *)
---

# Clayers Brainstorm

Design a feature for **main** as a structured specification
before writing code.

## Process

1. **Understand the feature**: ask clarifying questions about purpose,
   constraints, and success criteria before touching the spec
2. **Discover brainstorm modes**: query the spec for layers with brainstorm
   modes
3. **Select relevant layers**: not all layers apply to every feature.
   Check each brainstorm mode's `lyr:condition` against the feature
4. **Execute modes in dependency order**: follow `lyr:requires`
5. **For each mode**: read guidance, create spec elements, validate

## Discover Brainstorm Modes

```bash
clayers query '//lyr:mode[@name="brainstorm"]' clayers/main/
```

Typical layers with brainstorm modes:
- Prose: write sections describing the feature
- Terminology: define new domain terms
- Relation: link to existing spec nodes
- Decision: record design choices
- Plan: create implementation plan (after design is stable)

## Execute

For each applicable brainstorm mode:
1. Read the mode body (`pr:section` with step-by-step guidance)
2. Follow any `pr:xref` references to templates in `templates.xml`
3. Create spec elements following the guidance
4. Validate: `clayers validate clayers/main/`
5. Check `lyr:acceptance` criteria

## Validate

After each layer mode:

```bash
clayers validate clayers/main/
```

Before declaring the brainstorm done, run the full quality suite:

```bash
clayers artifact --coverage clayers/main/
clayers connectivity clayers/main/
```

The feature design is complete when:
- All relevant layer brainstorm modes have their acceptance criteria met
- New spec nodes are connected to existing ones via relations
- The spec validates cleanly

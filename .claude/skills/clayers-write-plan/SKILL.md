---
name: clayers-write-plan
description: >
  Create implementation plans as pln:plan XML in the spec. Reads existing
  spec nodes and produces itemized steps with acceptance criteria.
  Use when: "write plan", "create implementation plan", "plan feature".
allowed-tools:
  - Bash(clayers validate *)
  - Bash(clayers query *)
---

# Clayers Write Plan

Create an implementation plan for **main** that lives inside
the spec as structured XML.

## Read Existing Spec

Query for nodes that need implementation:

```bash
clayers query '//pr:section/pr:title' clayers/main/ --text
clayers query '//trm:term/trm:name' clayers/main/ --text
clayers query '//dec:decision' clayers/main/
```

Understand what concepts exist, which have implementations (artifact
mappings), and which are pending design decisions.

## Read Plan Layer Guidance

```bash
clayers query '//lyr:mode[@id="mode-pln-brainstorm"]' clayers/main/
```

The plan layer's brainstorm mode contains detailed guidance for plan
structure, item decomposition, and acceptance criteria.

## Create Plan

Follow the plan layer mode guidance. Use the template referenced there:

```bash
clayers query '//cnt:content[@id="tpl-plan"]' clayers/main/
```

A plan has:
- Title and overview
- Status (proposed -> active -> completed)
- Items with titles, descriptions, acceptance criteria, and status
- Optional links to spec nodes the items implement
- Optional witness references for testable criteria

## Validate

```bash
clayers validate clayers/main/
```

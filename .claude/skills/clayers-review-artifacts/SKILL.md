---
name: clayers-review-artifacts
description: >
  Review and improve artifact mappings, relations, and coverage.
  Discovers review and catch-up modes from the spec, executes them
  to improve quality metrics. Use when: "review artifacts", "fix drift",
  "improve coverage", "check connectivity".
allowed-tools:
  - Bash(clayers validate *)
  - Bash(clayers artifact *)
  - Bash(clayers connectivity *)
  - Bash(clayers query *)
---

# Clayers Review Artifacts

Review and improve the quality of **main**'s specification.

## Establish Baseline

Before starting, record current metrics:

```bash
clayers artifact --coverage clayers/main/
clayers artifact --drift clayers/main/
clayers connectivity clayers/main/
clayers validate clayers/main/
```

Record: total nodes, mapped, exempt, unmapped, drifted, isolated.

## Discover Review and Catch-Up Modes

Query the spec for layers with review modes:

```bash
clayers query '//lyr:mode[@name="review"]' clayers/main/
```

And for catch-up modes (needed if drift was detected):

```bash
clayers query '//lyr:mode[@name="catch-up"]' clayers/main/
```

## Execute Catch-Up Modes First

If the baseline detected drift, execute catch-up modes for the relevant
layers. Each catch-up mode body contains detailed guidance for handling
its specific drift type (ARTIFACT DRIFTED, SPEC DRIFTED, UNAVAILABLE).

For each drifted layer:
1. Read the catch-up mode guidance (the `pr:section` body)
2. Follow the drift-resolution steps
3. Run `clayers artifact --fix-node-hash` and `--fix-artifact-hash` as directed
4. Verify: `clayers artifact --drift clayers/main/`

## Execute Review Modes

For each layer with a review mode:
1. Read the mode body (`pr:section` with detailed guidance)
2. Follow the review steps (coverage checks, quality audits, etc.)
3. Verify `lyr:acceptance` criteria are met

The artifact layer's review mode covers: spec-to-code coverage, code-to-spec
coverage, exemption audit, and mapping quality. The relation layer's review
mode covers connectivity.

## Final Verification

Re-run all quality checks and report improvement:

```bash
clayers artifact --coverage clayers/main/
clayers artifact --drift clayers/main/
clayers connectivity clayers/main/
clayers validate clayers/main/
```

Report deltas:
- Coverage: unmapped before vs. after
- Drift: drifted before vs. after
- Connectivity: isolated before vs. after

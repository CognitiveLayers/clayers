---
name: clayers-execute-plan
description: >
  Execute plan items from the spec with TDD, artifact mapping, and
  status tracking. Use when: "execute plan", "implement plan items",
  "start implementing".
allowed-tools:
  - Bash(clayers validate *)
  - Bash(clayers artifact *)
  - Bash(clayers connectivity *)
  - Bash(clayers query *)
---

# Clayers Execute Plan

Execute a plan from **main**'s specification.

## Find Plans

```bash
clayers query '//pln:plan/pln:title' clayers/main/ --text
clayers query '//pln:plan[pln:status="active" or pln:status="proposed"]' clayers/main/
```

Pick an active or proposed plan to execute.

## Read Artifact Layer Guidance

The artifact layer's onboard mode contains the mapping guidance you'll need
for each implemented item:

```bash
clayers query '//lyr:mode[@id="mode-art-onboard"]' clayers/main/
```

## Execute Items

For each pending item in the plan:

1. **Read the item**: title, description, acceptance criteria
2. **Implement with TDD**:
   - Write a failing test
   - Run it, verify it fails
   - Write minimal implementation
   - Run it, verify it passes
3. **Create artifact mapping**: follow the artifact layer's onboard mode
   guidance to link new code to the spec node it implements
4. **Fix hashes**:
   ```bash
   clayers artifact --fix-node-hash clayers/main/
   clayers artifact --fix-artifact-hash clayers/main/
   ```
5. **Update item status**: change `pln:item-status` from `pending` to `completed`
6. **Validate**: `clayers validate clayers/main/`
7. **Commit**: spec + code together, one commit per item

## After All Items

Run quality checks to verify clean state:

```bash
clayers artifact --coverage clayers/main/
clayers artifact --drift clayers/main/
clayers connectivity clayers/main/
```

Update the plan's status from `active` to `completed`.

@AGENTS.md
<!-- clayers:adopt -->
## Clayers Development Workflow

This project uses [clayers](https://github.com/CognitiveLayers/clayers) for
structured, layered specifications with machine-verifiable traceability.

**Spec first, code second.** Before implementing, update the spec to
describe what you are building.

1. **Update the spec** in `clayers/` — add prose, terminology, relations
2. **Validate**: `clayers validate clayers/PROJECT/`
3. **Implement** the code
4. **Map spec to code** with artifact mappings, fix hashes
5. **Iterate on quality**:
   - Coverage: `clayers artifact --coverage clayers/PROJECT/`
   - Connectivity: `clayers connectivity clayers/PROJECT/`
   - Drift: `clayers artifact --drift clayers/PROJECT/`
6. **Commit** spec + code together

**Plans go in the spec.** Use the `pln:plan` layer to write implementation
plans and save them in the knowledge base (`clayers/`). Plans are versioned,
queryable, and linked to the concepts they implement.

**Looking for what to do?** Drive spec coverage to 100%. Map every spec
node to implementing code. This naturally leads to implementing everything
that was specified.

Install: `cargo install clayers`

See [clayers documentation](https://github.com/CognitiveLayers/clayers) for
the full layer reference (prose, terminology, organization, relation,
decision, source, plan, artifact, llm, revision).
<!-- /clayers:adopt -->

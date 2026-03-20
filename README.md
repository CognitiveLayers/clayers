# Clayers: Cognitive Layers for Structured Specifications

Version control and tooling for layered XML specifications with
machine-verifiable traceability between specs and code.

## The Problem

Software specifications are either implicit (in developers' heads), scattered
across disconnected documentation, or maintained in heavyweight tools that
drift from reality. Code becomes the de facto spec, but code only captures
*how*, not *why*.

Tracking XML specifications in git compounds the problem: git treats XML as
opaque text, producing unreadable diffs, meaningless merge conflicts, and
no structural awareness.

## What Clayers Does

Clayers provides two things:

1. **A layered XML format** for writing specifications where each concern
   (prose, terminology, relations, artifact mappings, etc.) lives in its own
   namespace and can evolve independently.

2. **A content-addressed version control system** purpose-built for XML. It
   decomposes documents into a Merkle DAG of XML Infoset nodes, enabling
   structural diffs, element-level deduplication, and drift detection between
   specs and code.

## Quick Start

```bash
# Install
cargo install --path crates/clayers

# Bootstrap clayers in your project
clayers adopt .

# Validate a specification
clayers validate clayers/my-project/

# Check for drift between spec and code
clayers artifact --drift clayers/my-project/
```

### Repository Workflow

```bash
# Initialize an XML repository
clayers init

# Stage and commit XML files
clayers add *.xml
clayers commit -m "initial specification"

# Branch, modify, commit
clayers checkout -b feature-auth
# ... edit XML files ...
clayers add auth.xml
clayers commit -m "add authentication spec"

# Push to a bare repository
clayers init --bare /path/to/shared.db
clayers remote add origin /path/to/shared.db
clayers push origin

# Clone and query
clayers clone /path/to/shared.db my-copy
cd my-copy
clayers query '//trm:term/trm:name' --text
```

### Remote Server Workflow

```bash
# Generate a server config with auto-generated token
clayers serve init --repo myspec:/path/to/myspec.db -o server.yaml

# Start the server
clayers serve run server.yaml
# => clayers server listening on ws://0.0.0.0:9100

# Clone from the server (use the token from server.yaml)
clayers clone ws://server:9100/myspec --token <token>

# Push/pull over WebSocket
clayers remote add origin ws://server:9100/myspec --token <token>
clayers push
clayers pull

# List available repos on a server
clayers remote list-repos ws://server:9100 --token <token>
```

## Spec Commands

| Command | Description |
|---------|-------------|
| `validate <path>` | Validate spec structure and cross-layer references |
| `artifact <path>` | List artifact mappings |
| `artifact --drift <path>` | Detect spec/code drift via content hashes |
| `artifact --coverage <path>` | Analyze spec-to-code coverage |
| `artifact --fix-node-hash <path>` | Recompute spec-side hashes after editing |
| `artifact --fix-artifact-hash <path>` | Recompute code-side hashes after editing |
| `connectivity <path>` | Graph metrics: density, hubs, bridges, cycles |
| `schema [path]` | Export XSD schemas as RELAX NG Compact |
| `query <xpath> [path]` | XPath query against assembled spec |
| `adopt [path]` | Bootstrap clayers in a project |

## Repository Commands

| Command | Description |
|---------|-------------|
| `init [path]` | Create a new repository (`.clayers.db`) |
| `init --bare <file.db>` | Create a bare repository (no working copy) |
| `clone <source> [target]` | Clone a repository (local path or `ws://` URL) |
| `add <files...>` | Stage files for commit (`.` for all XML) |
| `rm <files...>` | Stage deletion (or `--cached` to unstage) |
| `status` | Show staged, modified, and untracked files |
| `commit -m <msg>` | Record staged changes as a commit |
| `log [-n N]` | Show commit history |
| `branch [name]` | List or create branches |
| `branch --delete <name>` | Delete a branch |
| `checkout <branch>` | Switch branches (updates files on disk) |
| `checkout -b <branch>` | Create and switch to a new branch |
| `checkout --orphan <branch>` | Create a branch with no history |
| `remote add <name> <url> [--token T]` | Add a remote (local path or `ws://` URL) |
| `remote remove <name>` | Remove a remote |
| `remote list` | List remotes |
| `remote list-repos <url> [--token T]` | List repos on a remote server |
| `push [remote]` | Push to a remote (local or WebSocket) |
| `pull [remote]` | Pull from a remote (local or WebSocket) |
| `revert <files...>` | Restore files to committed state |
| `query <xpath>` | XPath query against committed repository |
| `serve run <config.yaml>` | Start a WebSocket repository server |
| `serve init [--repo name:path]` | Generate a server config file |

### Author Resolution

Commit author is resolved in order: `--author`/`--email` flags,
`CLAYERS_AUTHOR_NAME`/`CLAYERS_AUTHOR_EMAIL` environment variables,
`git config user.name`/`user.email`.

### Query Modes

`query` works in two modes:

- **Repo mode** (default): queries all documents in the current branch's tree.
  Supports `--count`, `--text`, `--rev`, `--branch`, `--all`, `--db`.
- **Spec mode**: when given a directory path, falls back to spec-level query
  against the assembled combined document.

## Layers

| Layer | Namespace | Purpose |
|-------|-----------|---------|
| **Index** | `urn:clayers:index` | File manifest, ID uniqueness domain |
| **Revision** | `urn:clayers:revision` | Named snapshots for temporal anchoring |
| **Prose** | `urn:clayers:prose` | DITA-style technical writing (sections, paragraphs, steps) |
| **Terminology** | `urn:clayers:terminology` | Controlled vocabulary with canonical definitions |
| **Organization** | `urn:clayers:organization` | Topic typing: concept, task, reference |
| **Relation** | `urn:clayers:relation` | Typed semantic links (depends-on, refines, implements, ...) |
| **Decision** | `urn:clayers:decision` | Decision records |
| **Source** | `urn:clayers:source` | External references and citations |
| **Plan** | `urn:clayers:plan` | Implementation plans with acceptance criteria |
| **Artifact** | `urn:clayers:artifact` | Code traceability with drift detection |
| **LLM** | `urn:clayers:llm` | Machine-readable descriptions for LLM consumption |

Layers are orthogonal: editing prose doesn't require touching artifact
mappings. All layers share a common ID space (enforced by the index) and
reference each other by node ID.

## Architecture

The Rust workspace contains four crates:

```
crates/
  clayers-xml/     XML utilities: C14N, content hashing, OASIS catalog, RNC export
  clayers-repo/    Content-addressed Merkle DAG for XML with async SQLite storage
  clayers-spec/    Spec-aware tooling: validation, drift, coverage, connectivity
  clayers/         CLI binary combining spec and repository commands
```

### Object Model

The repository stores XML as a content-addressed Merkle DAG:

```
Branch ("main")
  -> Commit (author, timestamp, message)
    -> Tree { "overview.xml": hash, "auth.xml": hash, ... }
      -> Document (root element hash)
        -> Element (local name, namespace, prefix, attributes, children)
          -> Text / Comment / PI (leaf nodes)
```

Each object's identity is `SHA-256(ExclusiveC14N(xml_representation))`.
Namespace prefixes are preserved through the import/export cycle for
faithful round-tripping.

### Storage

Repositories use a single `.clayers.db` SQLite file containing:
- **Object store**: content-addressed blobs (clayers-repo)
- **Ref store**: branch and tag pointers (clayers-repo)
- **CLI tables**: `cli_meta`, `working_copy`, `staging`, `remotes`

### Server

`clayers serve run` starts a WebSocket server from a YAML config:

```yaml
listen: '0.0.0.0:9100'
users:
- name: alice
  token: <generated-by-serve-init>
repos:
  myspec:
    path: /data/myspec.db              # local SQLite backend
  upstream:
    path: ws://other:9100/original     # proxy to another server (daisy-chain)
    token: upstream-server-token       # auth for the upstream connection
```

Use `clayers serve init` to generate a config with cryptographic tokens:

```bash
clayers serve init --repo myspec:/path/to/myspec.db -o server.yaml
# Resolves paths to absolute, generates a 32-byte CSPRNG token
```

Features:
- **Multi-repo**: one server hosts multiple named repositories
- **Per-user auth**: bearer tokens validated during WebSocket handshake
- **Hot-reload**: config file changes are picked up automatically (debounced)
- **Daisy-chaining**: repos can proxy to upstream servers via `ws://` paths

## Development

```bash
# Build
cargo build --workspace

# Run tests
cargo test --workspace

# Install from source
cargo install --path crates/clayers
```

## Drift Detection Workflow

```bash
# After editing spec prose
clayers validate clayers/my-project/
clayers artifact --fix-node-hash clayers/my-project/
clayers artifact --drift clayers/my-project/

# After editing code
clayers artifact --drift clayers/my-project/
# ... update line ranges in artifact mappings if needed ...
clayers artifact --fix-artifact-hash clayers/my-project/
clayers artifact --drift clayers/my-project/

# Check coverage
clayers artifact --coverage clayers/my-project/
```

## File Structure

```
clayers/
  schemas/                    XSD 1.1 schemas (one per layer)
    catalog.xml               OASIS XML Catalog (namespace-to-file mapping)
    spec.xsd                  Root element, annotation markers
    index.xsd                 File manifest
    prose.xsd                 Writing elements
    terminology.xsd           Controlled vocabulary
    organization.xsd          Topic typing
    relation.xsd              Semantic links
    decision.xsd              Decision records
    source.xsd                External references
    plan.xsd                  Implementation plans
    artifact.xsd              Code traceability
    llm.xsd                   LLM descriptions
    revision.xsd              Named snapshots
    repository.xsd            Repository objects (commits, trees, tags)
  clayers/                    Specification instances
    clayers/                  Self-referential spec (describes the format itself)
  examples/                   Example specifications
    payment-processing/       Fintech domain example
  crates/                     Rust workspace
```

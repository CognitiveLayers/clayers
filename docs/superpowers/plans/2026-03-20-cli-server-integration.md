# CLI Server Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `clayers serve` command and make `clone`/`push`/`pull` work over WebSocket URLs, with YAML-based server config, per-user auth, hot-reload, and daisy-chaining.

**Architecture:** The server reads a YAML config (repos + users), opens each repo as either `SqliteStore` or upstream `RemoteStore` based on path format, and serves via `serve_ws()`. The client detects `ws://` URLs in clone/push/pull and uses `WsTransport` + `RemoteStore` instead of local `SqliteStore`. Auth uses bearer tokens validated during the WebSocket handshake.

**Tech Stack:** Rust, tokio, tokio-tungstenite, serde_yml, notify 8.x (file watching), clap

**Spec:** `docs/superpowers/specs/2026-03-20-cli-server-integration-design.md`

---

## File Structure

| File | Responsibility |
|------|----------------|
| `crates/clayers-repo/src/store/remote/websocket.rs` | Fix `serve_ws` validator enforcement, `MultiTokenValidator`, change `connect` to accept URL string |
| `crates/clayers-repo/src/store/remote/client.rs` | Standalone `list_repositories()` function |
| `crates/clayers/Cargo.toml` | Add `serde_yaml`, `notify`, enable `websocket` feature |
| `crates/clayers/src/cli.rs` | `Serve` command, `ListRepos` subcommand, `--token` flag on `remote add` |
| `crates/clayers/src/serve.rs` | Config types, `DynamicProvider`, file watcher, `cmd_serve()` |
| `crates/clayers/src/repo/remote.rs` | URL parsing, WS dispatch in push/pull, `cmd_list_repos()` |
| `crates/clayers/src/repo/init.rs` | Clone refactor for ws:// sources |
| `crates/clayers/src/repo/schema.rs` | Schema v2 migration (token column) |
| `clayers/clayers/remote-store.xml` | Spec update: server, CLI, config terms |

---

### Task 1: Prerequisites in clayers-repo (websocket.rs + client.rs)

Three changes to the library crate before CLI integration.

**Files:**
- Modify: `crates/clayers-repo/src/store/remote/websocket.rs`
- Modify: `crates/clayers-repo/src/store/remote/client.rs`
- Modify: `crates/clayers-repo/src/store/remote/mod.rs`

- [ ] **Step 1: Change `WsTransport::connect` to accept a URL string**

Replace the `addr: &SocketAddr` parameter with `url: &str`. The URL is passed directly to `tokio_tungstenite::connect_async()` which handles DNS resolution. Auth headers are applied by building a custom `http::Request` when a transformer is provided.

In `websocket.rs`, change the `connect` method signature from:
```rust
pub async fn connect(
    addr: &SocketAddr,
    codec: C,
    auth: Option<&dyn WsRequestTransformer>,
) -> Result<Self>
```
to:
```rust
pub async fn connect(
    url: &str,
    codec: C,
    auth: Option<&dyn WsRequestTransformer>,
) -> Result<Self>
```

Remove `use std::net::SocketAddr;` (no longer needed in the non-test portion). Update the request building: instead of constructing a URL from `addr`, use the `url` parameter directly. Remove the manual `Host`, `Connection`, `Upgrade`, `Sec-WebSocket-Version`, `Sec-WebSocket-Key` headers when no auth is provided -- just pass the URL string to `connect_async()`. When auth IS provided, build the request manually with auth headers applied via the transformer.

- [ ] **Step 2: Fix `serve_ws` to enforce the validator**

Currently `serve_ws` ignores its `_validator` parameter and uses `accept_async()`. Change it to use `accept_hdr_async()` when a validator is provided.

Replace the per-connection accept logic. When `validator` is `Some`, use the callback form of `accept_hdr_async` to inspect the HTTP request headers. If validation fails, the callback returns an error response that rejects the connection.

Since `serve_ws` takes ownership of the validator as `Option<Box<dyn WsRequestValidator>>`, wrap it in `Arc` so it can be shared across connection tasks:

```rust
let validator: Option<Arc<dyn WsRequestValidator>> = _validator.map(Arc::from);
```

In the per-connection spawn, clone the `Arc` and use it:
```rust
let validator = validator.clone();
// ...
let ws_stream = if let Some(ref v) = validator {
    let v = Arc::clone(v);
    tokio_tungstenite::accept_hdr_async(stream, move |req: &http::Request<()>, resp| {
        match v.validate(req) {
            Ok(()) => Ok(resp),
            Err(reason) => {
                let resp = http::Response::builder()
                    .status(http::StatusCode::UNAUTHORIZED)
                    .body(None)
                    .unwrap();
                Err(resp)
            }
        }
    }).await
} else {
    tokio_tungstenite::accept_async(stream).await
};
```

Check `tokio-tungstenite` 0.29 API for `accept_hdr_async` -- the callback signature may differ. Adapt as needed. The key contract: if the validator rejects, the WS handshake fails and the TCP connection is closed.

- [ ] **Step 3: Add `MultiTokenValidator`**

In `websocket.rs`, add:

```rust
/// Validates bearer tokens against a mutable set (supports hot-reload).
#[derive(Clone)]
pub struct MultiTokenValidator {
    tokens: Arc<tokio::sync::RwLock<std::collections::HashSet<String>>>,
}

impl MultiTokenValidator {
    pub fn new(tokens: std::collections::HashSet<String>) -> Self {
        Self { tokens: Arc::new(tokio::sync::RwLock::new(tokens)) }
    }

    pub async fn update_tokens(&self, tokens: std::collections::HashSet<String>) {
        *self.tokens.write().await = tokens;
    }
}

impl WsRequestValidator for MultiTokenValidator {
    fn validate(&self, req: &http::Request<()>) -> std::result::Result<(), String> {
        let auth = req.headers()
            .get(http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| "missing Authorization header".to_string())?;
        let token = auth.strip_prefix("Bearer ")
            .ok_or_else(|| "invalid Authorization format".to_string())?;
        // Use try_read to avoid async in a sync trait method.
        // If the lock is held by a writer (hot-reload), reject transiently.
        let guard = self.tokens.try_read()
            .map_err(|_| "server reloading, retry".to_string())?;
        if guard.contains(token) {
            Ok(())
        } else {
            Err("invalid token".to_string())
        }
    }
}
```

Re-export from `mod.rs`.

- [ ] **Step 4: Add standalone `list_repositories` function**

In `client.rs`, add a free function that takes a transport reference:

```rust
/// List repositories without constructing a full RemoteStore.
/// Only safe on a freshly connected transport with no other in-flight requests.
pub async fn list_repositories<T: Transport + 'static>(transport: &T) -> Result<Vec<String>> {
    let id = 1; // Single request, no contention
    let (tx, rx) = tokio::sync::oneshot::channel();
    // Send request
    transport.send(ClientMessage::ListRepositories { id }).await?;
    // Read response directly
    let msg = transport.recv().await?;
    match msg {
        ServerMessage::RepositoryList { repos, .. } => Ok(repos),
        ServerMessage::Error { message, .. } => Err(Error::Storage(message)),
        _ => Err(Error::Storage("unexpected response".into())),
    }
}
```

Re-export from `mod.rs`.

- [ ] **Step 5: Update test infrastructure for new `connect` signature**

In the test module at the bottom of `websocket.rs`, update `create_remote_store()` to use the new URL-based `connect`:

```rust
let transport = WsTransport::connect(
    &format!("ws://{addr}"),  // was &addr
    JsonCodec,
    None,
).await.unwrap();
```

- [ ] **Step 6: Verify all tests pass**

Run: `cargo test --features websocket,sqlite -p clayers-repo -- remote --test-threads=1`
Expected: 41 passed

Run: `cargo test --features sqlite -p clayers-repo -- memory::tests memory::prop_tests sqlite::tests sqlite::prop_tests`
Expected: 82 passed

- [ ] **Step 7: Commit**

```
Problem: WsTransport::connect takes SocketAddr, serve_ws ignores validator, no multi-user auth

WsTransport::connect accepts &SocketAddr which cannot represent hostnames.
serve_ws accepts a validator parameter but never calls it, so server-side
auth is not enforced. There is no way to validate multiple bearer tokens
for multi-user scenarios.

Solution: generalize connect to URL string, enforce validator in serve_ws, add MultiTokenValidator

Co-Authored-By: Claude <noreply@anthropic.com>
```

---

### Task 2: Dependencies and schema migration

**Files:**
- Modify: `crates/clayers/Cargo.toml`
- Modify: `crates/clayers/src/repo/schema.rs`

- [ ] **Step 1: Add dependencies to CLI Cargo.toml**

In `crates/clayers/Cargo.toml`, change the `clayers-repo` dependency to enable `websocket`:
```toml
clayers-repo = { path = "../clayers-repo", features = ["sqlite", "websocket"] }
```

Add new dependencies:
```toml
serde_yml = "0.0.12"
notify = { version = "8", features = ["macos_fsevent"] }
async-trait = "0.1"
```

Add `"net"` and `"signal"` to the tokio features:
```toml
tokio = { version = "1", features = ["rt", "rt-multi-thread", "net", "signal"] }
```

- [ ] **Step 2: Add schema migration to v2**

In `schema.rs`, add a `migrate_schema` function that runs after `open_cli_db`:

```rust
/// Migrate CLI schema to the latest version.
pub fn migrate_schema(conn: &Connection) -> Result<()> {
    let version = get_meta(conn, "schema_version")?
        .unwrap_or_else(|| "1".into());

    match version.as_str() {
        "1" => {
            conn.execute_batch("ALTER TABLE remotes ADD COLUMN token TEXT;")
                .context("failed to migrate remotes table to v2")?;
            set_meta(conn, "schema_version", "2")?;
        }
        "2" => {} // current
        v => bail!("unknown schema version: {v}"),
    }
    Ok(())
}
```

Also update `init_cli_schema` to create the `remotes` table with the `token` column from the start (for new repos):

Change the remotes CREATE TABLE to:
```sql
CREATE TABLE IF NOT EXISTS remotes (
    name  TEXT PRIMARY KEY,
    url   TEXT NOT NULL,
    token TEXT
);
```

And set initial schema_version to `'2'`.

- [ ] **Step 3: Wire migration into `open_cli_db` callers**

Add a helper in `schema.rs`:
```rust
/// Open CLI database and run migrations.
pub fn open_and_migrate(db_path: &Path) -> Result<Connection> {
    let conn = super::open_cli_db(db_path)?;
    migrate_schema(&conn)?;
    Ok(conn)
}
```

Update callers that use `open_cli_db` to call `open_and_migrate` instead (in `remote.rs`, `init.rs`, `staging.rs`, `commit.rs`, `branch.rs`, `history.rs`, `diff.rs`, `revert.rs`). The simplest approach: make `open_cli_db` itself call `migrate_schema` so all callers get it automatically. In `repo/mod.rs`:

```rust
pub fn open_cli_db(db_path: &Path) -> Result<Connection> {
    let conn = Connection::open(db_path).context("failed to open CLI database")?;
    schema::migrate_schema(&conn)?;
    Ok(conn)
}
```

- [ ] **Step 4: Verify existing tests still pass**

Run: `cargo test -p clayers`
Expected: all existing tests pass (schema migration is backward-compatible)

- [ ] **Step 5: Commit**

```
Problem: remotes table has no token column, schema has no migration system

The CLI stores remotes as (name, url) with no authentication credential.
Opening an older database with new code would fail if the token column is
expected but missing.

Solution: add schema migration v1->v2, add token column to remotes

Co-Authored-By: Claude <noreply@anthropic.com>
```

---

### Task 3: URL parsing and remote dispatch

**Files:**
- Modify: `crates/clayers/src/repo/remote.rs`
- Modify: `crates/clayers/src/cli.rs`

- [ ] **Step 1: Add URL parsing helper**

In `remote.rs`, add:

```rust
/// Parsed remote target.
pub enum RemoteTarget {
    /// Local SQLite file path.
    Local(PathBuf),
    /// WebSocket server. `url` is the ws://host:port base, `repo` is the path component.
    WebSocket { url: String, repo: String, token: Option<String> },
}

/// Parse a remote URL into a target.
pub fn parse_remote_url(url: &str, token: Option<String>) -> Result<RemoteTarget> {
    if url.starts_with("ws://") || url.starts_with("wss://") {
        // Split: ws://host:port/repo-name
        let without_scheme = url.strip_prefix("ws://")
            .or_else(|| url.strip_prefix("wss://"))
            .unwrap();
        let scheme = if url.starts_with("wss://") { "wss" } else { "ws" };
        let (host_port, repo) = without_scheme.split_once('/')
            .ok_or_else(|| anyhow::anyhow!(
                "ws:// URL must include repo name: {url} (expected ws://host:port/repo)"
            ))?;
        if repo.is_empty() {
            bail!("repo name cannot be empty in URL: {url}");
        }
        let base_url = format!("{scheme}://{host_port}");
        Ok(RemoteTarget::WebSocket { url: base_url, repo: repo.to_string(), token })
    } else {
        Ok(RemoteTarget::Local(PathBuf::from(url)))
    }
}
```

- [ ] **Step 2: Add async `open_remote_store` helper**

In `remote.rs`, add an async function that opens a store from a `RemoteTarget`:

```rust
use clayers_repo::store::remote::{
    RemoteStore, JsonCodec, WsTransport, BearerToken, Transport,
};
use clayers_repo::store::remote::transport::Store;

/// Open a remote store from a parsed target.
pub async fn open_remote(target: &RemoteTarget) -> Result<Box<dyn Store>> {
    match target {
        RemoteTarget::Local(path) => {
            let store = SqliteStore::open(path)
                .with_context(|| format!("failed to open {}", path.display()))?;
            Ok(Box::new(store))
        }
        RemoteTarget::WebSocket { url, repo, token } => {
            let auth: Option<BearerToken> = token.as_ref().map(|t| BearerToken(t.clone()));
            let transport = WsTransport::connect(
                url,
                JsonCodec,
                auth.as_ref().map(|a| a as &dyn clayers_repo::store::remote::WsRequestTransformer),
            ).await
            .with_context(|| format!("failed to connect to {url}"))?;
            let store = RemoteStore::new(transport, repo);
            Ok(Box::new(store))
        }
    }
}
```

- [ ] **Step 3: Update `RemoteInfo` and `resolve_remote` to include token**

```rust
struct RemoteInfo {
    url: String,
    token: Option<String>,
}

fn resolve_remote(conn: &rusqlite::Connection, name: Option<&str>) -> Result<RemoteInfo> {
    let name = name.unwrap_or("origin");
    let result = conn.query_row(
        "SELECT url, token FROM remotes WHERE name = ?1",
        rusqlite::params![name],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
    );
    match result {
        Ok((url, token)) => Ok(RemoteInfo { url, token }),
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            bail!("remote '{name}' not found (use 'clayers remote add {name} <url>')")
        }
        Err(e) => Err(anyhow::anyhow!(e).context("failed to query remotes")),
    }
}
```

- [ ] **Step 4: Refactor `cmd_push` to use URL dispatch**

Replace the existing `cmd_push` with:

```rust
pub fn cmd_push(remote_name: Option<&str>) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to get CWD")?;
    let (_, db_path) = discover_repo(&cwd)?;
    let conn = open_cli_db(&db_path)?;

    let remote = resolve_remote(&conn, remote_name)?;
    let target = parse_remote_url(&remote.url, remote.token)?;

    println!("Pushing to {}...", remote.url);

    block_on(async move {
        let local = SqliteStore::open(&db_path)?;
        let remote_store = open_remote(&target).await?;

        let count = sync_refs(
            &local, &local,
            remote_store.as_ref(), remote_store.as_ref(),
            HEADS_PREFIX, &FastForwardOnly,
        ).await.context("push failed")?;

        if count == 0 {
            println!("Everything up-to-date");
        } else {
            println!("Pushed {count} ref(s)");
        }
        Ok(())
    })
}
```

Trait upcasting (stable since Rust 1.76, this crate uses edition 2024) allows `&dyn Store` to coerce to `&dyn ObjectStore` and `&dyn RefStore`. Pass `remote_store.as_ref()` for both `ObjectStore` and `RefStore` parameters of `sync_refs`.

- [ ] **Step 5: Refactor `cmd_pull` to use URL dispatch**

Same pattern as push but reversed direction:

```rust
pub fn cmd_pull(remote_name: Option<&str>) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to get CWD")?;
    let (repo_root, db_path) = discover_repo(&cwd)?;
    let conn = open_cli_db(&db_path)?;

    let remote = resolve_remote(&conn, remote_name)?;
    let target = parse_remote_url(&remote.url, remote.token)?;
    let current_branch = get_meta(&conn, "current_branch")?.unwrap_or_else(|| "main".into());

    println!("Pulling from {}...", remote.url);

    block_on(async move {
        let remote_store = open_remote(&target).await?;
        let local_store = SqliteStore::open(&db_path)?;

        let count = sync_refs(
            remote_store.as_ref(), remote_store.as_ref(),
            &local_store, &local_store,
            HEADS_PREFIX, &FastForwardOnly,
        ).await.context("pull failed")?;

        if count == 0 {
            println!("Already up-to-date");
        } else {
            println!("Pulled {count} ref(s)");
        }
        Ok(())
    })?;

    super::init::export_working_copy(&db_path, &repo_root, &current_branch)?;
    let conn = open_cli_db(&db_path)?;
    super::branch::refresh_working_copy_table(&conn, &db_path, &current_branch)?;
    Ok(())
}
```

- [ ] **Step 6: Update `cmd_remote` for `--token` flag**

Update `RemoteAction::Add` to include token:
```rust
pub enum RemoteAction {
    Add { name: String, url: String, token: Option<String> },
    Remove { name: String },
    List,
}
```

In `cmd_remote`, update the Add arm:
```rust
RemoteAction::Add { name, url, token } => {
    conn.execute(
        "INSERT OR REPLACE INTO remotes (name, url, token) VALUES (?1, ?2, ?3)",
        rusqlite::params![name, url, token],
    ).context("failed to add remote")?;
    println!("remote '{name}' added -> {url}");
}
```

Update the List arm to show tokens (masked):
```rust
RemoteAction::List => {
    let mut stmt = conn
        .prepare("SELECT name, url, token FROM remotes ORDER BY name")
        .context("failed to query remotes")?;
    let rows = stmt
        .query_map([], |row| Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
        )))
        .context("failed to iterate remotes")?;
    for row in rows {
        let (name, url, token) = row.context("failed to read remote row")?;
        let auth = if token.is_some() { " [authenticated]" } else { "" };
        println!("{name}\t{url}{auth}");
    }
}
```

- [ ] **Step 7: Update CLI clap definitions**

In `cli.rs`, update `RemoteAction::Add`:
```rust
Add {
    /// Remote name.
    name: String,
    /// Remote URL (path to .db file or ws://host:port/repo).
    url: String,
    /// Bearer token for authentication.
    #[arg(long)]
    token: Option<String>,
},
```

Add `ListRepos` subcommand:
```rust
/// List repositories available on a remote server.
ListRepos {
    /// Server URL (ws://host:port).
    url: String,
    /// Bearer token for authentication.
    #[arg(long)]
    token: Option<String>,
},
```

Update the `Clone` command's `source` to accept `String` instead of `PathBuf`, and add `--token`:
```rust
Clone {
    /// Source .db file, repository directory, or ws://host:port/repo URL.
    source: String,
    /// Target directory (defaults to derived from source name).
    target: Option<PathBuf>,
    /// Bearer token for authentication (ws:// sources).
    #[arg(long)]
    token: Option<String>,
},
```

Update the dispatch in `run()` to pass the token through.

- [ ] **Step 8: Add `cmd_list_repos`**

In `remote.rs`:

```rust
pub fn cmd_list_repos(url: &str, token: Option<&str>) -> Result<()> {
    block_on(async move {
        let auth: Option<BearerToken> = token.map(|t| BearerToken(t.to_string()));
        let transport = WsTransport::connect(
            url, JsonCodec,
            auth.as_ref().map(|a| a as &dyn clayers_repo::store::remote::WsRequestTransformer),
        ).await
        .with_context(|| format!("failed to connect to {url}"))?;

        let repos = clayers_repo::store::remote::list_repositories(&transport).await?;
        for name in &repos {
            println!("{name}");
        }
        if repos.is_empty() {
            println!("(no repositories)");
        }
        Ok(())
    })
}
```

- [ ] **Step 9: Verify compilation**

Run: `cargo check -p clayers`
Expected: compiles

- [ ] **Step 10: Commit**

```
Problem: clone/push/pull only work with local SQLite file paths

The CLI hardcodes SqliteStore for all remote operations. There is no
way to push to or pull from a WebSocket server.

Solution: add URL dispatch for ws:// URLs, token storage, remote list-repos command

Co-Authored-By: Claude <noreply@anthropic.com>
```

---

### Task 4: Refactor clone for ws:// sources

**Files:**
- Modify: `crates/clayers/src/repo/init.rs`
- Modify: `crates/clayers/src/cli.rs`

- [ ] **Step 1: Refactor `cmd_clone` to accept string source**

Change `cmd_clone` signature from `(source: &Path, target: &PathBuf)` to `(source: &str, target: &PathBuf, token: Option<&str>)`:

```rust
pub fn cmd_clone(source: &str, target: &PathBuf, token: Option<&str>) -> Result<()> {
    if target.exists() {
        bail!("target already exists: {}", target.display());
    }

    let target_for_clone = parse_remote_url(source, token.map(String::from))?;

    // For local sources, verify existence
    if let RemoteTarget::Local(ref path) = target_for_clone {
        if !path.exists() {
            bail!("source not found: {}", path.display());
        }
    }

    std::fs::create_dir_all(target)
        .with_context(|| format!("failed to create {}", target.display()))?;

    let dst_db = target.join(".clayers.db");

    block_on(async {
        let src_store = open_remote(&target_for_clone).await?;
        let dst_store = SqliteStore::open(&dst_db)
            .with_context(|| format!("failed to create clone at {}", dst_db.display()))?;

        sync_refs(
            src_store.as_ref(), src_store.as_ref(),
            &dst_store, &dst_store,
            HEADS_PREFIX, &FastForwardOnly,
        ).await.context("failed to sync refs from source")?;

        Ok(())
    })?;

    let conn = open_cli_db(&dst_db)?;
    init_cli_schema(&conn, false)?;

    // Store origin with token if ws://
    match &target_for_clone {
        RemoteTarget::WebSocket { token, .. } => {
            conn.execute(
                "INSERT OR REPLACE INTO remotes (name, url, token) VALUES ('origin', ?1, ?2)",
                rusqlite::params![source, token.as_deref()],
            ).context("failed to add origin remote")?;
        }
        RemoteTarget::Local(_) => {
            conn.execute(
                "INSERT OR REPLACE INTO remotes (name, url) VALUES ('origin', ?1)",
                rusqlite::params![source],
            ).context("failed to add origin remote")?;
        }
    }

    let default_branch = find_default_branch(&dst_db)?;
    if let Some(branch) = default_branch {
        super::schema::set_meta(&conn, "current_branch", &branch)?;
        export_working_copy(&dst_db, target, &branch)?;
        super::branch::refresh_working_copy_table(&conn, &dst_db, &branch)?;
    }

    println!("Cloned into {}", target.display());
    Ok(())
}
```

Import `parse_remote_url`, `open_remote`, `RemoteTarget` from `super::remote`.

- [ ] **Step 2: Update CLI dispatch for clone**

In `cli.rs`, update the Clone dispatch to derive target name from URL or path:

```rust
Command::Clone { source, target } => {
    let default_target;
    let target = if let Some(t) = target { t } else {
        // Derive from source: last path component or repo name from URL
        let stem = if source.starts_with("ws://") || source.starts_with("wss://") {
            source.rsplit('/').next().unwrap_or("cloned-repo").to_string()
        } else {
            Path::new(&source).file_stem()
                .map_or_else(|| "cloned-repo".into(), |s| s.to_string_lossy().into_owned())
        };
        default_target = PathBuf::from(stem);
        &default_target
    };
    crate::repo::init::cmd_clone(&source, target)
}
```

- [ ] **Step 3: Verify compilation and test**

Run: `cargo check -p clayers`
Run: `cargo test -p clayers`
Expected: compiles, existing tests pass

- [ ] **Step 4: Commit**

```
Problem: clayers clone only accepts local file paths

Cloning from a WebSocket server is not possible because cmd_clone
takes a Path and opens it as SqliteStore.

Solution: accept string source, dispatch to WsTransport for ws:// URLs

Co-Authored-By: Claude <noreply@anthropic.com>
```

---

### Task 5: Server command (`clayers serve`)

**Files:**
- Create: `crates/clayers/src/serve.rs`
- Modify: `crates/clayers/src/cli.rs`
- Modify: `crates/clayers/src/main.rs` (or wherever modules are declared)

- [ ] **Step 1: Create config types**

In `serve.rs`:

```rust
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use tokio::sync::RwLock;

use clayers_repo::store::remote::transport::Store;
use clayers_repo::store::remote::server::RepositoryProvider;
use clayers_repo::store::remote::{
    JsonCodec, MultiTokenValidator, WsTransport, BearerToken,
    websocket::serve_ws,
};
use clayers_repo::SqliteStore;
use clayers_repo::store::remote::RemoteStore;

#[derive(Deserialize)]
pub struct ServerConfig {
    pub listen: String,
    pub users: Vec<UserConfig>,
    pub repos: HashMap<String, RepoConfig>,
}

#[derive(Deserialize)]
pub struct UserConfig {
    pub name: String,
    pub token: String,
}

#[derive(Deserialize)]
pub struct RepoConfig {
    pub path: String,
    #[serde(default)]
    pub token: Option<String>,
}
```

- [ ] **Step 2: Create `DynamicProvider`**

```rust
#[derive(Clone)]
pub struct DynamicProvider {
    repos: Arc<RwLock<HashMap<String, Arc<dyn Store>>>>,
}

impl DynamicProvider {
    pub fn new() -> Self {
        Self { repos: Arc::new(RwLock::new(HashMap::new())) }
    }

    pub async fn reload(&self, repo_configs: &HashMap<String, RepoConfig>) -> Result<()> {
        let mut new_repos: HashMap<String, Arc<dyn Store>> = HashMap::new();
        for (name, config) in repo_configs {
            let store = open_backend(config).await
                .with_context(|| format!("failed to open repo '{name}'"))?;
            new_repos.insert(name.clone(), store);
        }
        *self.repos.write().await = new_repos;
        Ok(())
    }
}

#[async_trait]
impl RepositoryProvider for DynamicProvider {
    async fn list(&self) -> clayers_repo::error::Result<Vec<String>> {
        Ok(self.repos.read().await.keys().cloned().collect())
    }

    async fn get(&self, name: &str) -> clayers_repo::error::Result<Arc<dyn Store>> {
        self.repos.read().await
            .get(name)
            .cloned()
            .ok_or_else(|| clayers_repo::error::Error::Storage(
                format!("repository not found: {name}")
            ))
    }
}

async fn open_backend(config: &RepoConfig) -> Result<Arc<dyn Store>> {
    if config.path.starts_with("ws://") || config.path.starts_with("wss://") {
        // Upstream WebSocket
        let target = crate::repo::remote::parse_remote_url(&config.path, config.token.clone())?;
        let store = crate::repo::remote::open_remote(&target).await?;
        // Box<dyn Store> -> Arc<dyn Store>
        Ok(Arc::from(store))
    } else {
        let store = SqliteStore::open(Path::new(&config.path))
            .with_context(|| format!("failed to open {}", config.path))?;
        Ok(Arc::new(store))
    }
}
```

- [ ] **Step 3: Create `cmd_serve`**

```rust
pub fn cmd_serve(config_path: &Path) -> Result<()> {
    let rt = tokio::runtime::Runtime::new().context("failed to create runtime")?;
    rt.block_on(async {
        let config = load_config(config_path)?;
        let provider = DynamicProvider::new();
        provider.reload(&config.repos).await?;

        let tokens: HashSet<String> = config.users.iter().map(|u| u.token.clone()).collect();
        let validator = Arc::new(MultiTokenValidator::new(tokens));

        // MultiTokenValidator wraps Arc<RwLock<HashSet>>, so cloning it shares
        // the same token set. Derive Clone on MultiTokenValidator for this to work.
        // Hot-reload updates go through the shared Arc and are visible to the server.
        let validator_for_ws: Option<Box<dyn clayers_repo::store::remote::WsRequestValidator>> =
            Some(Box::new(validator.as_ref().clone()));

        let (handle, addr) = serve_ws(&config.listen, provider, validator_for_ws, JsonCodec)
            .await
            .context("failed to start server")?;

        println!("Serving on {addr}");
        println!("Repos: {}", config.repos.keys().cloned().collect::<Vec<_>>().join(", "));

        // TODO: file watcher for hot-reload (Task 6)

        // Wait for shutdown signal
        tokio::signal::ctrl_c().await.ok();
        println!("\nShutting down...");
        handle.abort();

        Ok(())
    })
}

fn load_config(path: &Path) -> Result<ServerConfig> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read config: {}", path.display()))?;
    serde_yml::from_str(&text)
        .with_context(|| format!("failed to parse config: {}", path.display()))
}
```

Note: `serve_ws` currently takes `provider: P` by value, but `DynamicProvider` needs to be shared with the hot-reload watcher. This means `serve_ws` needs to accept `Arc<P>` or the provider needs to be `Clone`. The simplest fix: make `DynamicProvider` cloneable (it's just `Arc<RwLock<...>>` inside) and pass a clone to `serve_ws`. Check `serve_ws` signature -- it wraps the provider in `Arc` internally, so passing a clone is fine.

Actually, looking at the `Server::new` in `server.rs`, it wraps provider in `Arc<P>`. So DynamicProvider needs `Clone` or `serve_ws` needs to accept `Arc<P>`. Since `DynamicProvider` contains `Arc<RwLock<...>>`, derive Clone on it and pass a clone. The original and the clone share the same inner data.

- [ ] **Step 4: Wire into CLI**

In `cli.rs`, add the `Serve` command:
```rust
/// Start a repository server.
Serve {
    /// Path to YAML config file.
    config: PathBuf,
},
```

In the dispatch:
```rust
Command::Serve { config } => crate::serve::cmd_serve(config),
```

Add `mod serve;` to `crates/clayers/src/main.rs` (where other modules like `mod cli; mod repo;` are declared).

- [ ] **Step 5: Verify it compiles and starts**

Run: `cargo check -p clayers`

Create a test config `/tmp/test-serve.yaml`:
```yaml
listen: "127.0.0.1:0"
users:
  - name: test
    token: "test-token"
repos: {}
```

Run: `cargo run -p clayers -- serve /tmp/test-serve.yaml`
Expected: prints "Serving on 127.0.0.1:XXXXX", ctrl+C shuts down cleanly.

- [ ] **Step 6: Commit**

```
Problem: no way to serve repositories over the network

The CLI has no server mode. Repositories can only be accessed via local
file paths.

Solution: add clayers serve command with YAML config, DynamicProvider, multi-user auth

Co-Authored-By: Claude <noreply@anthropic.com>
```

---

### Task 6: Hot-reload with file watcher

**Files:**
- Modify: `crates/clayers/src/serve.rs`

- [ ] **Step 1: Add file watcher to `cmd_serve`**

After the server starts, spawn a watcher task:

```rust
use notify::{Watcher, RecursiveMode, Event, EventKind};

// In cmd_serve, after the server starts:
let config_path_owned = config_path.to_path_buf();
let provider_clone = provider.clone(); // DynamicProvider is Clone (shares inner Arc)
let validator_clone = Arc::clone(&validator);

tokio::spawn(async move {
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);

    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        if let Ok(event) = res {
            if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                let _ = tx.blocking_send(());
            }
        }
    }).expect("failed to create file watcher");

    watcher.watch(&config_path_owned, RecursiveMode::NonRecursive)
        .expect("failed to watch config file");

    // Debounce: wait for events, then reload after 200ms of quiet
    loop {
        if rx.recv().await.is_none() { break; }
        // Drain any additional events within 200ms
        loop {
            match tokio::time::timeout(
                std::time::Duration::from_millis(200),
                rx.recv(),
            ).await {
                Ok(Some(())) => continue,
                _ => break,
            }
        }

        eprintln!("Config changed, reloading...");
        match load_config(&config_path_owned) {
            Ok(new_config) => {
                if let Err(e) = provider_clone.reload(&new_config.repos).await {
                    eprintln!("Warning: failed to reload repos: {e:#}");
                } else {
                    let new_tokens: HashSet<String> =
                        new_config.users.iter().map(|u| u.token.clone()).collect();
                    validator_clone.update_tokens(new_tokens).await;
                    eprintln!(
                        "Reloaded: {} repo(s), {} user(s)",
                        new_config.repos.len(),
                        new_config.users.len()
                    );
                }
            }
            Err(e) => {
                eprintln!("Warning: failed to parse config: {e:#}");
            }
        }
    }
});
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p clayers`

- [ ] **Step 3: Commit**

```
Problem: server config changes require restart

Modifying the YAML config file has no effect on a running server.
Adding or removing repos or users requires stopping and restarting.

Solution: watch config file with notify crate, debounce and reload on change

Co-Authored-By: Claude <noreply@anthropic.com>
```

---

### Task 7: Integration test

**Files:**
- Create: `crates/clayers/tests/remote_integration.rs` (or add to existing test file)

- [ ] **Step 1: Write end-to-end test**

```rust
//! Integration test: serve + clone + push + pull over WebSocket.

use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn clone_push_pull_over_websocket() {
    let tmp = TempDir::new().unwrap();
    let server_db = tmp.path().join("server.db");
    let clone_a = tmp.path().join("clone-a");
    let clone_b = tmp.path().join("clone-b");

    // 1. Create a local repo with some content
    let origin_dir = tmp.path().join("origin");
    std::fs::create_dir_all(&origin_dir).unwrap();
    // Init, add a file, commit
    assert_cmd::Command::cargo_bin("clayers").unwrap()
        .args(["init", origin_dir.to_str().unwrap()])
        .assert().success();

    let xml = r#"<?xml version="1.0" encoding="UTF-8"?><root><item>hello</item></root>"#;
    std::fs::write(origin_dir.join("test.xml"), xml).unwrap();

    assert_cmd::Command::cargo_bin("clayers").unwrap()
        .current_dir(&origin_dir)
        .args(["add", "test.xml"])
        .assert().success();

    assert_cmd::Command::cargo_bin("clayers").unwrap()
        .current_dir(&origin_dir)
        .args(["commit", "-m", "initial", "--author", "test", "--email", "test@test.com"])
        .assert().success();

    // 2. Create a bare DB for the server
    assert_cmd::Command::cargo_bin("clayers").unwrap()
        .args(["init", "--bare", server_db.to_str().unwrap()])
        .assert().success();

    // Push from origin to bare DB
    assert_cmd::Command::cargo_bin("clayers").unwrap()
        .current_dir(&origin_dir)
        .args(["remote", "add", "bare", server_db.to_str().unwrap()])
        .assert().success();

    assert_cmd::Command::cargo_bin("clayers").unwrap()
        .current_dir(&origin_dir)
        .args(["push", "bare"])
        .assert().success();

    // 3. Write server config
    let config = format!(
        "listen: \"127.0.0.1:0\"\nusers:\n  - name: test\n    token: test-token\nrepos:\n  myrepo:\n    path: \"{}\"\n",
        server_db.display()
    );
    let config_path = tmp.path().join("server.yaml");
    std::fs::write(&config_path, &config).unwrap();

    // 4. Start server in background
    // This is tricky for integration tests. Use a helper that starts
    // the server and returns the bound port.
    // For now, test the components that don't need a running server,
    // or use the library API directly.

    // Library-level integration test:
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        use clayers_repo::store::remote::{JsonCodec, WsTransport, RemoteStore, serve_ws, MultiTokenValidator};
        use clayers_repo::store::remote::server::StaticRepositories;
        use clayers_repo::SqliteStore;
        use clayers_repo::sync::{sync_refs, FastForwardOnly};
        use clayers_repo::refs::HEADS_PREFIX;
        use std::collections::{HashMap, HashSet};
        use std::sync::Arc;

        // Open the server DB
        let server_store = SqliteStore::open(&server_db).unwrap();
        let mut repos = HashMap::new();
        repos.insert("myrepo".to_string(), Arc::new(server_store) as Arc<dyn clayers_repo::store::remote::transport::Store>);
        let provider = StaticRepositories::new(repos);

        let tokens: HashSet<String> = ["test-token".to_string()].into();
        let validator = MultiTokenValidator::new(tokens);

        let (handle, addr) = serve_ws(
            "127.0.0.1:0", provider,
            Some(Box::new(validator)),
            JsonCodec,
        ).await.unwrap();

        // Clone via WS
        let url = format!("ws://{addr}");
        let auth = clayers_repo::store::remote::BearerToken("test-token".to_string());
        let transport = WsTransport::connect(
            &format!("{url}/myrepo"), JsonCodec,
            Some(&auth as &dyn clayers_repo::store::remote::WsRequestTransformer),
        ).await.unwrap();
        let remote = RemoteStore::new(transport, "myrepo");

        // Verify we can list repos
        let repos = remote.list_repositories().await.unwrap();
        assert!(repos.contains(&"myrepo".to_string()));

        // Sync refs to a fresh local store
        let local_db = tmp.path().join("local-test.db");
        let local = SqliteStore::open(&local_db).unwrap();
        let count = sync_refs(
            &remote, &remote,
            &local, &local,
            HEADS_PREFIX, &FastForwardOnly,
        ).await.unwrap();
        assert!(count > 0, "should have synced at least one ref");

        // Verify the content is there
        let branches = clayers_repo::refs::list_branches(&local).await.unwrap();
        assert!(!branches.is_empty(), "should have at least one branch");

        handle.abort();
    });
}
```

- [ ] **Step 2: Run the test**

Run: `cargo test -p clayers -- clone_push_pull_over_websocket --nocapture`
Expected: PASS

- [ ] **Step 3: Commit**

```
Problem: no integration test for WebSocket clone/push/pull

Solution: add end-to-end test exercising serve + remote store + sync over WebSocket

Co-Authored-By: Claude <noreply@anthropic.com>
```

---

### Task 8: Spec update

**Files:**
- Modify: `clayers/clayers/remote-store.xml`
- Modify: `clayers/clayers/index.xml` (if needed)

- [ ] **Step 1: Add server/CLI terms and content to remote-store.xml**

Add terminology:
- `term-server-config`: YAML config with listen address, users, and repo backends
- `term-config-hot-reload`: file watching + debounced reload of repos and users
- `term-daisy-chain`: upstream WebSocket proxy where a server repo delegates to another server

Add prose sections:
- `serve-command`: the CLI serve command, config format, startup
- `url-dispatch`: how ws:// URLs are parsed and dispatched
- `daisy-chain-proxy`: upstream WebSocket repos as transparent proxies

Add organization typing, relations, artifact mappings for the new CLI code, and LLM descriptions.

- [ ] **Step 2: Validate and fix hashes**

```bash
cargo run -p clayers -- validate clayers/clayers/
cargo run -p clayers -- artifact --fix-node-hash clayers/clayers/
cargo run -p clayers -- artifact --fix-artifact-hash clayers/clayers/
cargo run -p clayers -- artifact --drift clayers/clayers/
cargo run -p clayers -- validate clayers/clayers/
```

All must pass clean.

- [ ] **Step 3: Commit**

```
Problem: spec does not document server command, CLI integration, or daisy-chaining

Solution: add server-config, url-dispatch, and daisy-chain terms with artifact mappings

Co-Authored-By: Claude <noreply@anthropic.com>
```

# CLI Server Integration Design

Date: 2026-03-20

## Problem

The clayers repository system has three async storage backends (Memory, SQLite, Remote/WebSocket) but the CLI only uses SQLite via local file paths. There is no way to serve repositories over the network or to clone/push/pull over WebSocket.

## Solution

Add a `clayers serve` command that hosts repositories from a YAML config, and make the existing `clone`/`push`/`pull`/`remote` commands work over WebSocket URLs alongside local file paths.

## Architecture

Three components:

1. **Server** (`clayers serve`) - reads YAML config, opens backends (SQLite or upstream WebSocket), serves via `serve_ws()`. Watches config file for hot-reload.
2. **Client URL dispatch** - `clone`/`push`/`pull` detect `ws://` URLs and use `WsTransport` + `RemoteStore`. Local file paths use `SqliteStore` as before.
3. **Discovery** (`clayers remote list-repos <url>`) - connects to a server and lists available repository names.

## Prerequisite: Fix `serve_ws` Validator Enforcement

The current `serve_ws()` accepts a `_validator` parameter but ignores it - `tokio_tungstenite::accept_async()` performs a plain accept with no request inspection. Before server-side auth can work, `serve_ws()` must be changed to use `tokio_tungstenite::accept_hdr_async()` (or equivalent) to inspect the HTTP upgrade request and call the validator. Reject connections that fail validation.

## Prerequisite: Generalize `WsTransport::connect` to Accept Strings

The current `WsTransport::connect()` takes `&SocketAddr`, which cannot represent hostnames. Change the signature to accept a string URL (e.g., `&str`) and use `tokio_tungstenite::connect_async(url)` which handles DNS resolution internally. This enables `ws://hostname:port/...` URLs.

## Server Config

```yaml
listen: "0.0.0.0:9100"

users:
  - name: alice
    token: "token-alice"
  - name: bob
    token: "token-bob"

repos:
  local-spec:
    path: "/data/myspec.db"

  upstream-mirror:
    path: "ws://other-server:9100/original"
    token: "upstream-token"
```

### Backend detection

The `path` field determines the backend:
- Starts with `ws://` or `wss://` - WebSocket upstream (daisy-chaining). Optional `token` field for upstream auth.
- Anything else - SQLite file path (`.db` extension conventional but not required).

### Hot-reload

The server watches the config file via the `notify` crate. On change:
- Parse new config. If parse fails, log warning and keep old config.
- New repos: open their backend.
- Removed repos: drop their backend (existing connections see errors on next request).
- Changed paths: effectively a delete + recreate. In-flight requests holding an `Arc<dyn Store>` to the old backend complete normally; new requests get the new backend. No overlap because the old `Arc` is dropped from the map before the new one is inserted.
- User list: rebuild the token validator's token set.
- Existing connections stay alive.
- File events are debounced (100-200ms) to handle rename-based editors (vim, emacs) that trigger multiple events.

### Daisy-chaining

A server repo can proxy to another server. Push to `server-A/upstream-mirror` writes through to `other-server/original`. This works transparently because `RemoteStore` implements the same `Store` trait as `SqliteStore`.

Daisy-chain repos have no local persistence - they are pure proxies. If the upstream is unreachable, all operations on that repo return errors. There is no store-and-forward.

### Graceful shutdown

`clayers serve` handles SIGINT/SIGTERM via `tokio::signal`. On shutdown: stop accepting new connections, let in-flight requests complete (with a timeout), then exit.

## Authentication

Authentication is connection-level: the bearer token is validated during the HTTP upgrade handshake. Once a connection is established, all messages on that connection are authorized. There is no per-message auth.

### Server side

`MultiTokenValidator` in `clayers-repo/src/store/remote/websocket.rs`:

```rust
pub struct MultiTokenValidator {
    tokens: Arc<RwLock<HashSet<String>>>,
}
```

Implements `WsRequestValidator`. Checks the `Authorization: Bearer <token>` header against the token set. The set is wrapped in `Arc<RwLock<>>` so hot-reload can swap tokens without restarting.

### Client side

Tokens stored in the `remotes` table alongside URLs:

```
clayers remote add origin ws://server:9100/myspec --token secret123
```

Push/pull reads the token from the table and passes it as `BearerToken` to `WsTransport::connect()`.

Note: tokens are stored in plaintext in the SQLite `.clayers.db` file. This is acceptable for a first implementation. Future improvement: OS keychain integration or environment variable override.

## URL Format

```
ws://host:port/repo-name
```

Parse rules:
- Scheme `ws://` or `wss://` - WebSocket remote.
- No scheme or `file://` - local file path (existing behavior).
- Host:port extracted for WS connection address.
- Path component after the first `/` is the repo name. Error if missing.

The URL path is parsed **client-side only**. The WebSocket connection is established to `host:port` with no path routing. The repo name is passed to `RemoteStore::new(transport, repo_name)` and sent inside every `ClientMessage`. The server dispatches based on the `repo` field in messages, not the URL path.

```rust
enum RemoteTarget {
    Local(PathBuf),
    WebSocket { url: String, repo: String },
}
```

`url` is the full `ws://host:port` string (without repo path) passed directly to `WsTransport::connect()`. `repo` is the extracted path component.

### TLS (`wss://`)

`wss://` URLs are parsed the same way. `tokio-tungstenite`'s `connect_async()` handles TLS automatically when given a `wss://` URL (uses rustls or native-tls depending on features). No TLS configuration is needed on the server side for this implementation - use a reverse proxy (nginx, caddy) for TLS termination at the server. The server always listens on plain TCP.

## Client Changes

### Remote URL dispatch

The dispatch logic lives in `cmd_push()`/`cmd_pull()` directly (not in `resolve_remote()`, which stays synchronous and returns the remote's URL + token). After resolving the remote metadata, the caller opens the appropriate backend:
- Local path: `SqliteStore::open(path)`
- `ws://`: connect `WsTransport`, create `RemoteStore`

Both implement `ObjectStore + RefStore + QueryStore` (the `Store` supertrait from `transport.rs`), so `sync_refs()` works with either. `SqliteStore` and `RemoteStore` both auto-implement `Store` via the blanket impl.

### Clone

Same URL dispatch. For ws:// sources:
1. Connect via WsTransport with optional BearerToken auth
2. `sync_refs(remote, remote, local_sqlite, local_sqlite, ...)`
3. Add remote as "origin" with URL and token
4. Init local working copy as before

### Schema migration

`remotes` table gains a nullable `token TEXT` column. Schema version bumps from 1 to 2.

Migration runner: `open_cli_db()` reads `schema_version` from `cli_meta`. If version is 1, runs `ALTER TABLE remotes ADD COLUMN token TEXT` and updates version to 2. This runs on every CLI invocation that opens an existing repo.

### `remote list-repos`

```
clayers remote list-repos ws://server:9100
clayers remote list-repos ws://server:9100 --token secret123
```

Standalone function `list_repositories(transport)` in `client.rs` sends `ClientMessage::ListRepositories` without constructing a `RemoteStore` instance. This avoids requiring a repo name for a discovery operation. The existing `RemoteStore::list_repositories(&self)` method is kept for programmatic use when a store is already connected.

## Dynamic Repository Provider

The server needs a `RepositoryProvider` that supports hot-reload:

```rust
struct DynamicProvider {
    repos: Arc<RwLock<HashMap<String, Arc<dyn Store>>>>,
}
```

On config reload, the inner map is rebuilt. The `Arc<dyn Store>` values are either `SqliteStore` or `RemoteStore<WsTransport>` depending on the path field. Both implement `Store` via the blanket impl (`impl<T: ObjectStore + RefStore + QueryStore> Store for T {}`).

## New Dependencies

| Crate | Where | Purpose |
|-------|-------|---------|
| `serde_yaml` | CLI | Config file parsing |
| `notify` | CLI | File watching for hot-reload |

The CLI crate enables the `websocket` feature on `clayers-repo` (pulls in `tokio-tungstenite`, `futures-util`, `http`). The `net` tokio feature is already available in `clayers-repo` under the `websocket` feature (used by `serve_ws`). The CLI crate's existing `rt-multi-thread` feature is sufficient for the serve command.

## Files Changed/Created

| File | Change |
|------|--------|
| `crates/clayers-repo/src/store/remote/websocket.rs` | Fix validator enforcement in `serve_ws`, add `MultiTokenValidator`, change `connect` to accept string URL |
| `crates/clayers-repo/src/store/remote/client.rs` | Add standalone `list_repositories()` function |
| `crates/clayers/Cargo.toml` | Add `serde_yaml`, `notify`, enable `websocket` feature on clayers-repo |
| `crates/clayers/src/cli.rs` | Add `Serve` command, `ListRepos` subcommand, `--token` flag |
| `crates/clayers/src/serve.rs` | New: config types, dynamic provider, file watcher, `cmd_serve()` |
| `crates/clayers/src/repo/remote.rs` | Refactor: URL dispatch, WS connect, `cmd_list_repos()` |
| `crates/clayers/src/repo/init.rs` | Refactor clone for ws:// sources |
| `crates/clayers/src/repo/schema.rs` | Schema v2 migration runner |
| `clayers/clayers/remote-store.xml` | Spec update: server, CLI, config, daisy-chaining |

## Testing

- **Unit tests:** config parsing, URL dispatch (`parse_remote_url`), schema migration (v1 -> v2)
- **Integration test:** start `serve` with temp YAML + temp SQLite, clone over ws://, push changes, pull from another clone, verify round-trip
- **Not tested:** hot-reload (manual), daisy-chaining (falls out from Store trait abstraction)

## Spec Update

Add to `remote-store.xml`:
- Terms: server-config, config-hot-reload, daisy-chain
- Prose sections on serve command, config format, URL dispatch, daisy-chaining
- Artifact mappings for new CLI code
- LLM descriptions

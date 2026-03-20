//! `serve` command: starts a WebSocket server for remote repository access.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use clayers_repo::SqliteStore;
use clayers_repo::store::remote::Store;
use clayers_repo::store::remote::{
    JsonCodec, MultiTokenValidator, RepositoryProvider, WsRequestValidator, serve_ws,
};
use serde::Deserialize;
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// Configuration types
// ---------------------------------------------------------------------------

/// Top-level server configuration (loaded from / written to YAML).
#[derive(Debug, Deserialize, serde::Serialize)]
pub struct ServerConfig {
    /// Address to bind (e.g., `"0.0.0.0:9090"`).
    pub listen: String,
    /// Authorised users.
    #[serde(default)]
    pub users: Vec<UserConfig>,
    /// Named repositories (key = repo name).
    pub repos: HashMap<String, RepoConfig>,
}

/// A user entry with a bearer token.
#[derive(Debug, Deserialize, serde::Serialize)]
pub struct UserConfig {
    /// Display name (informational).
    #[allow(dead_code)]
    pub name: String,
    /// Bearer token value.
    pub token: String,
}

/// A repository entry.
#[derive(Debug, Deserialize, serde::Serialize)]
pub struct RepoConfig {
    /// Path to local `.db` file or `ws://` URL to upstream.
    pub path: String,
    /// Optional bearer token for upstream WebSocket auth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

// ---------------------------------------------------------------------------
// Dynamic repository provider
// ---------------------------------------------------------------------------

/// A repository provider that can be reloaded at runtime.
#[derive(Clone)]
pub struct DynamicProvider {
    repos: Arc<RwLock<HashMap<String, Arc<dyn Store>>>>,
}

impl DynamicProvider {
    /// Create an empty provider.
    #[must_use]
    pub fn new() -> Self {
        Self {
            repos: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Replace the current repository set.
    pub async fn reload(&self, repos: HashMap<String, Arc<dyn Store>>) {
        *self.repos.write().await = repos;
    }
}

#[async_trait]
impl RepositoryProvider for DynamicProvider {
    async fn list(&self) -> clayers_repo::error::Result<Vec<String>> {
        Ok(self.repos.read().await.keys().cloned().collect())
    }

    async fn get(&self, name: &str) -> clayers_repo::error::Result<Arc<dyn Store>> {
        self.repos
            .read()
            .await
            .get(name)
            .cloned()
            .ok_or_else(|| {
                clayers_repo::error::Error::Storage(format!("repository not found: {name}"))
            })
    }
}

// ---------------------------------------------------------------------------
// Backend opening
// ---------------------------------------------------------------------------

/// Open a store backend from a config path string.
///
/// Currently supports local `.db` paths. WebSocket upstream is parsed but
/// would require a running event loop (future enhancement).
///
/// # Errors
///
/// Returns an error if the store cannot be opened.
fn open_backend(config_path: &str) -> Result<Arc<dyn Store>> {
    // For now, only local sqlite paths are supported as backends.
    let store = SqliteStore::open(Path::new(config_path))
        .with_context(|| format!("failed to open backend at {config_path}"))?;
    Ok(Arc::new(store))
}

// ---------------------------------------------------------------------------
// Config loading
// ---------------------------------------------------------------------------

/// Load and parse a YAML configuration file.
///
/// # Errors
///
/// Returns an error if the file cannot be read or parsed.
pub fn load_config(path: &Path) -> Result<ServerConfig> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read config file {}", path.display()))?;
    let config: ServerConfig =
        serde_yml::from_str(&text).context("failed to parse server config YAML")?;
    Ok(config)
}

/// Build the repository map from a config.
///
/// # Errors
///
/// Returns an error if any backend cannot be opened.
fn build_repos(config: &ServerConfig) -> Result<HashMap<String, Arc<dyn Store>>> {
    let mut repos = HashMap::new();
    for (name, rc) in &config.repos {
        let store = open_backend(&rc.path)?;
        repos.insert(name.clone(), store);
    }
    Ok(repos)
}

/// Collect bearer tokens from the config.
fn collect_tokens(config: &ServerConfig) -> HashSet<String> {
    config.users.iter().map(|u| u.token.clone()).collect()
}

// ---------------------------------------------------------------------------
// serve init
// ---------------------------------------------------------------------------

/// Generate a starter YAML config file with auto-generated tokens.
///
/// # Errors
///
/// Returns an error if writing the output file fails.
pub fn cmd_serve_init(repos: &[String], listen: &str, output: Option<&Path>) -> Result<()> {
    // Parse name:path pairs, resolve local paths to absolute and verify existence
    let mut repo_map = HashMap::new();
    for entry in repos {
        let (name, path) = entry
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("invalid repo format: {entry} (expected name:path)"))?;
        let resolved = if path.starts_with("ws://") || path.starts_with("wss://") {
            path.to_string()
        } else {
            let p = Path::new(path);
            anyhow::ensure!(p.exists(), "repo path does not exist: {path}");
            std::fs::canonicalize(p)
                .with_context(|| format!("failed to resolve path: {path}"))?
                .to_string_lossy()
                .into_owned()
        };
        repo_map.insert(
            name.to_string(),
            RepoConfig {
                path: resolved,
                token: None,
            },
        );
    }

    let token = generate_token();

    let config = ServerConfig {
        listen: listen.to_string(),
        users: vec![UserConfig {
            name: "admin".to_string(),
            token: token.clone(),
        }],
        repos: repo_map,
    };

    let yaml = serde_yml::to_string(&config).context("failed to serialize config")?;

    if let Some(path) = output {
        std::fs::write(path, &yaml)
            .with_context(|| format!("failed to write {}", path.display()))?;
        eprintln!("Config written to {}", path.display());
        eprintln!("Admin token: {token}");
    } else {
        print!("{yaml}");
    }

    Ok(())
}

fn generate_token() -> String {
    use base64::Engine;
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("failed to get random bytes");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

// ---------------------------------------------------------------------------
// serve run
// ---------------------------------------------------------------------------

/// Run the `serve` command: load config, start WS server, wait for shutdown.
///
/// # Errors
///
/// Returns an error if the config is invalid or the server cannot bind.
pub fn cmd_serve(config_path: &Path) -> Result<()> {
    let config = load_config(config_path)?;
    let repos = build_repos(&config)?;
    let tokens = collect_tokens(&config);
    let listen_addr = config.listen.clone();

    let provider = DynamicProvider::new();
    let validator = MultiTokenValidator::new(tokens);

    let config_path = config_path.to_path_buf();

    crate::repo::block_on(async move {
        // Populate the provider.
        provider.reload(repos).await;

        // Build the validator box.
        let validator_box: Option<Box<dyn WsRequestValidator>> =
            if validator.tokens.read().await.is_empty() {
                None
            } else {
                Some(Box::new(validator.clone()))
            };

        let (_handle, bound_addr) =
            serve_ws(&listen_addr, provider.clone(), validator_box, JsonCodec)
                .await
                .context("failed to start WebSocket server")?;

        eprintln!("clayers server listening on ws://{bound_addr}");

        // Hot-reload: watch the config file for changes.
        let reload_result =
            spawn_config_watcher(config_path, provider.clone(), validator.clone());
        if let Err(e) = reload_result {
            eprintln!("warning: could not start config watcher: {e}");
        }

        // Wait for ctrl+c.
        tokio::signal::ctrl_c()
            .await
            .context("failed to listen for ctrl+c")?;
        eprintln!("\nshutting down");

        Ok(())
    })
}

// ---------------------------------------------------------------------------
// Hot-reload (Task 6)
// ---------------------------------------------------------------------------

/// Spawn a background thread that watches the config file for changes and
/// reloads the provider and validator when the file is modified.
///
/// Uses a 200ms debounce window to coalesce rapid writes.
///
/// # Errors
///
/// Returns an error if the file watcher cannot be created.
fn spawn_config_watcher(
    config_path: PathBuf,
    provider: DynamicProvider,
    validator: MultiTokenValidator,
) -> Result<()> {
    use notify::{RecursiveMode, Watcher};
    use std::sync::mpsc;
    use std::time::Duration;

    let (tx, rx) = mpsc::channel();

    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res
            && (event.kind.is_modify() || event.kind.is_create())
        {
            let _ = tx.send(());
        }
    })
    .context("failed to create file watcher")?;

    // Canonicalize so .parent() gives a real directory, not "".
    let config_path = std::fs::canonicalize(&config_path)
        .unwrap_or(config_path);

    // Watch the parent directory (some editors do rename+write).
    let watch_path = config_path
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();
    watcher
        .watch(&watch_path, RecursiveMode::NonRecursive)
        .with_context(|| format!("failed to watch {}", watch_path.display()))?;

    // Spawn a dedicated thread for the blocking watcher loop.
    let rt_handle = tokio::runtime::Handle::current();
    std::thread::spawn(move || {
        let _watcher = watcher; // keep alive

        loop {
            // Wait for any event.
            if rx.recv().is_err() {
                break;
            }

            // Debounce: drain events for 200ms of quiet.
            loop {
                match rx.recv_timeout(Duration::from_millis(200)) {
                    Ok(()) => {}
                    Err(mpsc::RecvTimeoutError::Timeout) => break,
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
                }
            }

            // Reload.
            match load_config(&config_path) {
                Ok(new_config) => {
                    match build_repos(&new_config) {
                        Ok(new_repos) => {
                            let new_tokens = collect_tokens(&new_config);
                            let provider = provider.clone();
                            let validator = validator.clone();
                            rt_handle.spawn(async move {
                                provider.reload(new_repos).await;
                                validator.update_tokens(new_tokens).await;
                            });
                            eprintln!("config reloaded: {} repo(s)", new_config.repos.len());
                        }
                        Err(e) => {
                            eprintln!("warning: config reload failed (keeping old): {e}");
                        }
                    }
                }
                Err(e) => {
                    eprintln!("warning: could not parse config (keeping old): {e}");
                }
            }
        }
    });

    Ok(())
}

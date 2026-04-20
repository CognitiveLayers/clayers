use std::path::{Path, PathBuf};
use std::process;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "clayers", about = "Cognitive layers spec tooling", version = env!("CLAYERS_VERSION"))]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// Nested subcommands for `clayers search`.
#[cfg(feature = "semantic-search")]
#[derive(Subcommand)]
pub enum SearchCmd {
    /// Build or update the search index without running a query.
    Index {
        /// Path to the spec directory.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Rebuild the index from scratch, ignoring any cached state.
        #[arg(long)]
        rebuild: bool,
        /// Verbose logging (progress bars, model cache path, etc.).
        #[arg(long)]
        verbose: bool,
        /// Override the embedder model. Supported: `bge-small-en-v1.5`
        /// (default), `all-minilm-l6-v2`, `multilingual-e5-small`.
        #[arg(long)]
        model: Option<String>,
    },
}

#[derive(Subcommand)]
enum Command {
    // -----------------------------------------------------------------------
    // Spec commands (existing)
    // -----------------------------------------------------------------------

    /// Validate a spec structurally.
    Validate {
        /// Path to the spec directory.
        path: PathBuf,
    },
    /// Artifact mapping analysis and drift detection.
    Artifact {
        /// Path to the spec directory.
        path: PathBuf,
        /// Check for drift between stored and current hashes.
        #[arg(long)]
        drift: bool,
        /// Recompute and fix node-side hashes.
        #[arg(long)]
        fix_node_hash: bool,
        /// Recompute and fix artifact-side hashes.
        #[arg(long)]
        fix_artifact_hash: bool,
        /// Show coverage analysis.
        #[arg(long)]
        coverage: bool,
        /// Filter coverage to a specific code path.
        #[arg(long)]
        code_path: Option<String>,
    },
    /// Analyze spec connectivity (graph metrics).
    Connectivity {
        /// Path to the spec directory.
        path: PathBuf,
    },
    /// Collect a landing node's neighbors across relations, terminology
    /// refs, and artifact mappings in one pass. Phase-3 helper for the
    /// clayers-context skill — replaces 6+N separate `clayers query`
    /// calls per landing with a single assembled JSON bundle.
    Neighbors {
        /// The landing node `@id` to expand.
        id: String,
        /// Path to the spec directory.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Output results as JSON (default: yes — this command is
        /// designed for agent consumption). Pretty-prints to the
        /// terminal if stdout is a TTY.
        #[arg(long, default_value_t = true)]
        json: bool,
        /// Apply hub pre-filter when total degree exceeds this.
        #[arg(long, default_value_t = 12)]
        hub_threshold: usize,
        /// Within the hub pre-filter, keep this many per edge-kind
        /// bucket (artifact-map / relation / term-ref).
        #[arg(long, default_value_t = 2)]
        top_per_bucket: usize,
        /// Truncate peek strings to at most this many characters.
        #[arg(long, default_value_t = 350)]
        peek_chars: usize,
    },
    /// Export schemas as RELAX NG Compact notation.
    Schema {
        /// Path to the schema directory (auto-detected from schemas/ or .clayers/schemas/ if omitted).
        path: Option<PathBuf>,
        /// Filter to specific layers by prefix. Can be repeated.
        #[arg(long)]
        layer: Vec<String>,
    },
    /// Execute an `XPath` query against the assembled spec or repository.
    Query {
        /// `XPath` expression.
        xpath: String,
        /// Path to the spec directory, repo directory, or bare .db file (optional in repo mode).
        path: Option<PathBuf>,
        /// Output only the count of matching nodes.
        #[arg(long)]
        count: bool,
        /// Output text content (no XML tags).
        #[arg(long)]
        text: bool,
        /// Query all branches.
        #[arg(long)]
        all: bool,
        /// Query a specific revision/revspec.
        #[arg(long)]
        rev: Option<String>,
        /// Query a specific branch.
        #[arg(long)]
        branch: Option<String>,
        /// Path to a bare .db file.
        #[arg(long)]
        db: Option<PathBuf>,
        /// Constrain query to specific documents (substring match on path).
        #[arg(long = "file", short = 'f')]
        files: Vec<String>,
        /// Output results as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Generate HTML documentation from a spec.
    Doc {
        /// Path to spec directory, single XML file, or index.xml.
        path: PathBuf,
        /// Output file path (default: derived from spec name).
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Inline all CDN resources for fully offline HTML.
        #[arg(long)]
        self_contained: bool,
        /// Watch for changes and regenerate automatically.
        #[arg(long)]
        watch: bool,
    },
    /// Semantic search over a spec directory (behind `semantic-search` feature).
    #[cfg(feature = "semantic-search")]
    Search {
        /// Nested action. When omitted, the positional args form a
        /// ranked query (Step 5).
        #[command(subcommand)]
        cmd: Option<SearchCmd>,
        /// Natural-language query (when no subcommand is given).
        query: Option<String>,
        /// Path to the spec directory.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Emit chunker output as JSON (hidden, for Step 2 verification).
        #[arg(long, hide = true)]
        dump_chunks: bool,
        /// Output results as JSON.
        #[arg(long)]
        json: bool,
        /// Rebuild the index from scratch.
        #[arg(long)]
        rebuild: bool,
        /// Verbose logging.
        #[arg(long)]
        verbose: bool,
        /// Override the embedder model. Supported (384-dim only):
        /// `bge-small-en-v1.5` (default), `all-minilm-l6-v2`,
        /// `multilingual-e5-small`.
        #[arg(long)]
        model: Option<String>,
        /// Number of results.
        #[arg(long, default_value_t = 10)]
        k: usize,
        /// Weight on the text (cosine) distance component.
        #[arg(long, default_value_t = 0.7)]
        alpha: f32,
        /// Weight on the structural (tanimoto) distance component.
        #[arg(long, default_value_t = 0.3)]
        beta: f32,
        /// `XPath` expression to post-filter candidates.
        #[arg(long)]
        xpath: Option<String>,
        /// Layer filter(s), e.g. `--layer terminology --layer prose`.
        #[arg(long = "layer")]
        layer: Vec<String>,
        /// Additional natural-language queries. Repeatable. Results
        /// are unioned by id, keeping the highest score per node.
        /// See `search-multi-query` in the spec for semantics.
        #[arg(long = "also")]
        also: Vec<String>,
    },

    /// Bootstrap clayers in a project (plant schemas, amend agent file).
    Adopt {
        /// Path to the target project directory.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Update outdated schemas and instructions in an already-adopted project.
        #[arg(long)]
        update: bool,
        /// Generate Claude Code skill for clayers onboarding.
        #[arg(long)]
        skills: bool,
    },

    // -----------------------------------------------------------------------
    // Repository commands (new)
    // -----------------------------------------------------------------------

    /// Initialize a clayers repository.
    Init {
        /// Path to initialize (defaults to current directory).
        path: Option<PathBuf>,
        /// Initialize a bare repository (a single .db file, no working copy).
        #[arg(long)]
        bare: Option<PathBuf>,
    },
    /// Stage files for the next commit.
    Add {
        /// Files to stage (use `.` for all XML files in CWD).
        files: Vec<PathBuf>,
    },
    /// Remove files from staging area or stage a deletion.
    Rm {
        /// Files to remove.
        files: Vec<PathBuf>,
        /// Only remove from staging area, don't delete from disk.
        #[arg(long)]
        cached: bool,
    },
    /// Show working tree status.
    Status,
    /// Record staged changes as a new commit.
    Commit {
        /// Commit message.
        #[arg(short, long)]
        message: String,
        /// Author name (overrides `CLAYERS_AUTHOR_NAME` env and git config).
        #[arg(long)]
        author: Option<String>,
        /// Author email (overrides `CLAYERS_AUTHOR_EMAIL` env and git config).
        #[arg(long)]
        email: Option<String>,
    },
    /// Show commit history.
    Log {
        /// Limit number of commits shown.
        #[arg(short, long)]
        n: Option<usize>,
    },
    /// Clone a repository.
    Clone {
        /// Source bare .db file, repository directory, or ws:// URL.
        source: String,
        /// Target directory (defaults to derived from source name).
        target: Option<PathBuf>,
        /// Bearer token for ws:// authentication.
        #[arg(long)]
        token: Option<String>,
    },
    /// Push local refs to a remote.
    Push {
        /// Remote name (defaults to 'origin').
        remote: Option<String>,
    },
    /// Pull refs from a remote.
    Pull {
        /// Remote name (defaults to 'origin').
        remote: Option<String>,
    },
    /// Manage branches.
    Branch {
        /// Create a branch with this name (omit to list branches).
        name: Option<String>,
        /// Delete a branch.
        #[arg(long)]
        delete: Option<String>,
    },
    /// Switch to a branch.
    Checkout {
        /// Branch to checkout.
        branch: String,
        /// Create and checkout a new branch.
        #[arg(short)]
        b: bool,
        /// Create an orphan branch (empty tree, no parent commit).
        #[arg(long)]
        orphan: bool,
    },
    /// Manage remote repositories.
    Remote {
        #[command(subcommand)]
        action: RemoteAction,
    },
    /// Merge a branch into the current branch.
    Merge {
        /// Branch to merge.
        branch: String,
        /// Merge strategy (auto, ours, theirs, manual).
        #[arg(long, default_value = "auto")]
        strategy: String,
        /// Commit message.
        #[arg(short, long)]
        message: Option<String>,
        /// Author name.
        #[arg(long)]
        author: Option<String>,
        /// Author email.
        #[arg(long)]
        email: Option<String>,
    },
    /// Restore files to their committed state.
    Revert {
        /// Files to revert.
        files: Vec<PathBuf>,
    },
    /// Show changes between commits, branches, or working copy.
    Diff {
        /// First revspec (branch, tag, or commit hash).
        rev_a: Option<String>,
        /// Second revspec (branch, tag, or commit hash).
        rev_b: Option<String>,
        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Manage and run a WebSocket repository server.
    Serve {
        #[command(subcommand)]
        action: ServeAction,
    },
}

#[derive(Subcommand)]
enum ServeAction {
    /// Start the server from a YAML config file.
    Run {
        /// Path to the YAML configuration file.
        config: PathBuf,
    },
    /// Generate a starter YAML config file.
    Init {
        /// Repository entries as `name:path` pairs.
        #[arg(long = "repo", value_name = "NAME:PATH")]
        repos: Vec<String>,
        /// Listen address (default: 0.0.0.0:9100).
        #[arg(long, default_value = "0.0.0.0:9100")]
        listen: String,
        /// Output file (default: stdout).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum RemoteAction {
    /// Add a new remote.
    Add {
        /// Remote name.
        name: String,
        /// Remote URL (path to .db file or ws:// URL).
        url: String,
        /// Bearer token for ws:// authentication.
        #[arg(long)]
        token: Option<String>,
    },
    /// Remove a remote.
    Remove {
        /// Remote name.
        name: String,
    },
    /// List all remotes.
    List,
    /// List repositories available on a remote server.
    ListRepos {
        /// WebSocket URL of the server.
        url: String,
        /// Bearer token for authentication.
        #[arg(long)]
        token: Option<String>,
    },
}

pub fn cli_main() {
    let cli = Cli::parse();
    if let Err(e) = run(&cli) {
        eprintln!("error: {e:#}");
        process::exit(1);
    }
}

pub fn cli_main_from<I, T>(args: I)
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = Cli::parse_from(args);
    if let Err(e) = run(&cli) {
        eprintln!("error: {e:#}");
        process::exit(1);
    }
}

#[allow(clippy::too_many_lines)]
fn run(cli: &Cli) -> Result<()> {
    match &cli.command {
        Command::Validate { path } => cmd_validate(path),
        Command::Artifact {
            path,
            drift,
            fix_node_hash,
            fix_artifact_hash,
            coverage,
            code_path,
        } => cmd_artifact(
            path,
            *drift,
            *fix_node_hash,
            *fix_artifact_hash,
            *coverage,
            code_path.as_deref(),
        ),
        Command::Connectivity { path } => cmd_connectivity(path),
        Command::Neighbors {
            id,
            path,
            json,
            hub_threshold,
            top_per_bucket,
            peek_chars,
        } => cmd_neighbors(
            id,
            path,
            *json,
            *hub_threshold,
            *top_per_bucket,
            *peek_chars,
        ),
        Command::Schema { path, layer } => cmd_schema(path.as_deref(), layer),
        Command::Query {
            xpath,
            path,
            count,
            text,
            all,
            rev,
            branch,
            db,
            files,
            json,
        } => cmd_query(
            path.as_deref(),
            xpath,
            *count,
            *text,
            *all,
            rev.as_deref(),
            branch.as_deref(),
            db.as_deref(),
            files,
            *json,
        ),
        Command::Doc {
            path,
            output,
            self_contained,
            watch,
        } => crate::doc::cmd_doc(path, output.as_deref(), *self_contained, *watch),
        #[cfg(feature = "semantic-search")]
        Command::Search {
            cmd,
            query,
            path,
            dump_chunks,
            json,
            rebuild,
            verbose,
            model,
            k,
            alpha,
            beta,
            xpath,
            layer,
            also,
        } => {
            if let Some(sub) = cmd {
                return crate::search_cmd::dispatch_sub(sub);
            }
            let effective_path = if *dump_chunks {
                query.as_deref().map_or_else(|| path.clone(), PathBuf::from)
            } else {
                path.clone()
            };
            let opts = crate::search_cmd::BareSearchOpts {
                dump_chunks: *dump_chunks,
                json: *json,
                rebuild: *rebuild,
                verbose: *verbose,
                model: model.as_deref(),
                k: *k,
                alpha: *alpha,
                beta: *beta,
                xpath: xpath.as_deref(),
                layer,
                also,
            };
            crate::search_cmd::cmd_search(&effective_path, query.as_deref(), &opts)
        }
        Command::Adopt {
            path,
            update,
            skills,
        } => cmd_adopt(path, *update, *skills),

        // Repository commands.
        Command::Init { path, bare } => {
            if let Some(bare_path) = bare {
                crate::repo::init::cmd_init_bare(bare_path)
            } else {
                let p = path.as_deref().unwrap_or(std::path::Path::new("."));
                crate::repo::init::cmd_init(p)
            }
        }
        Command::Add { files } => crate::repo::staging::cmd_add(files),
        Command::Rm { files, cached } => crate::repo::staging::cmd_rm(files, *cached),
        Command::Status => crate::repo::staging::cmd_status(),
        Command::Commit {
            message,
            author,
            email,
        } => crate::repo::commit::cmd_commit(message, author.as_deref(), email.as_deref()),
        Command::Log { n } => crate::repo::history::cmd_log(*n),
        Command::Clone { source, target, token } => {
            let default_target;
            let target = if let Some(t) = target { t } else {
                // Derive name from URL (last path segment for ws://) or file stem.
                let stem = if source.starts_with("ws://") || source.starts_with("wss://") {
                    source
                        .rsplit('/')
                        .find(|s| !s.is_empty())
                        .unwrap_or("cloned-repo")
                        .to_string()
                } else {
                    Path::new(source.as_str())
                        .file_stem()
                        .map_or_else(|| "cloned-repo".into(), |s| s.to_string_lossy().into_owned())
                };
                default_target = PathBuf::from(stem);
                &default_target
            };
            crate::repo::init::cmd_clone(source, target, token.as_deref())
        }
        Command::Push { remote } => crate::repo::remote::cmd_push(remote.as_deref()),
        Command::Pull { remote } => crate::repo::remote::cmd_pull(remote.as_deref()),
        Command::Branch { name, delete } => {
            crate::repo::branch::cmd_branch(name.as_deref(), delete.as_deref())
        }
        Command::Checkout { branch, b, orphan } => {
            crate::repo::branch::cmd_checkout(branch, *b, *orphan)
        }
        Command::Remote { action } => {
            match action {
                RemoteAction::Add { name, url, token } => {
                    let remote_action = crate::repo::remote::RemoteAction::Add {
                        name: name.clone(),
                        url: url.clone(),
                        token: token.clone(),
                    };
                    crate::repo::remote::cmd_remote(remote_action)
                }
                RemoteAction::Remove { name } => {
                    let remote_action =
                        crate::repo::remote::RemoteAction::Remove { name: name.clone() };
                    crate::repo::remote::cmd_remote(remote_action)
                }
                RemoteAction::List => {
                    crate::repo::remote::cmd_remote(crate::repo::remote::RemoteAction::List)
                }
                RemoteAction::ListRepos { url, token } => {
                    crate::repo::remote::cmd_list_repos(url, token.as_deref())
                }
            }
        }
        Command::Merge {
            branch,
            strategy,
            message,
            author,
            email,
        } => crate::repo::merge::cmd_merge(
            branch,
            strategy,
            message.as_deref(),
            author.as_deref(),
            email.as_deref(),
        ),
        Command::Revert { files } => crate::repo::revert::cmd_revert(files),
        Command::Diff { rev_a, rev_b, json } => {
            crate::repo::diff::cmd_diff(rev_a.as_deref(), rev_b.as_deref(), *json)
        }
        Command::Serve { action } => match action {
            ServeAction::Run { config } => crate::serve::cmd_serve(config),
            ServeAction::Init {
                repos,
                listen,
                output,
            } => crate::serve::cmd_serve_init(repos, listen, output.as_deref()),
        },
    }
}

// -----------------------------------------------------------------------
// Spec command implementations (unchanged from Phase 1)
// -----------------------------------------------------------------------

fn cmd_validate(path: &Path) -> Result<()> {
    let result = clayers_spec::validate::validate_spec(path).context("validation failed")?;

    println!(
        "validate: {} ({} files)",
        result.spec_name, result.file_count
    );

    if result.is_valid() {
        println!("  OK (no structural errors)");
    } else {
        for err in &result.errors {
            println!("  ERROR: {}", err.message);
        }
        process::exit(1);
    }
    Ok(())
}

fn cmd_fix_hashes(path: &Path, fix_node_hash: bool, fix_artifact_hash: bool) -> Result<()> {
    if fix_node_hash {
        let report =
            clayers_spec::fix::fix_node_hashes(path).context("fix-node-hash failed")?;
        println!(
            "fix-node-hash: {} ({} mappings, {} updated)",
            report.spec_name, report.total_mappings, report.fixed_count
        );
        for r in &report.results {
            println!("  {}: updated", r.mapping_id);
        }
    }

    if fix_artifact_hash {
        let report =
            clayers_spec::fix::fix_artifact_hashes(path).context("fix-artifact-hash failed")?;
        println!(
            "fix-artifact-hash: {} ({} mappings, {} updated)",
            report.spec_name, report.total_mappings, report.fixed_count
        );
        for r in &report.results {
            println!("  {}: updated", r.mapping_id);
        }
    }

    Ok(())
}

#[allow(clippy::fn_params_excessive_bools)]
fn cmd_artifact(
    path: &Path,
    drift: bool,
    fix_node_hash: bool,
    fix_artifact_hash: bool,
    coverage: bool,
    code_path: Option<&str>,
) -> Result<()> {
    if fix_node_hash || fix_artifact_hash {
        return cmd_fix_hashes(path, fix_node_hash, fix_artifact_hash);
    }

    if drift {
        return cmd_drift(path);
    }

    if coverage {
        let report = clayers_spec::coverage::analyze_coverage(path, code_path)
            .context("coverage analysis failed")?;

        println!(
            "coverage: {} ({} nodes, {} mapped, {} exempt)",
            report.spec_name, report.total_nodes, report.mapped_nodes, report.exempt_nodes
        );

        for ac in &report.artifact_coverages {
            println!(
                "  {}: {} ({} lines, {})",
                ac.mapping_id, ac.artifact_path, ac.line_count, ac.strength
            );
        }

        if !report.unmapped_nodes.is_empty() {
            println!("  unmapped nodes:");
            for node in &report.unmapped_nodes {
                println!("    {node}");
            }
        }

        if !report.file_coverages.is_empty() {
            println!("  code coverage:");
            for fc in &report.file_coverages {
                println!(
                    "    {}: {:.1}% covered ({}/{} lines)",
                    fc.file_path, fc.coverage_percent, fc.covered_lines, fc.total_lines
                );
                for cr in &fc.covered_ranges {
                    println!(
                        "      COVERED {}-{} ({})",
                        cr.start_line,
                        cr.end_line,
                        cr.mapping_ids.join(", ")
                    );
                }
                for ur in &fc.uncovered_ranges {
                    println!(
                        "      NOT COVERED {}-{} ({} lines)",
                        ur.start_line, ur.end_line, ur.line_count
                    );
                }
            }
        }
        return Ok(());
    }

    // Default: list artifact mappings
    let index_files =
        clayers_spec::discovery::find_index_files(path).context("discovery failed")?;

    let mut all_mappings = Vec::new();
    for index_path in &index_files {
        let file_paths = clayers_spec::discovery::discover_spec_files(index_path)
            .context("file discovery failed")?;
        let mappings = clayers_spec::artifact::collect_artifact_mappings(&file_paths)
            .context("artifact mapping collection failed")?;
        all_mappings.extend(mappings);
    }

    let spec_name = path
        .file_name()
        .map_or_else(|| "unknown".into(), |n| n.to_string_lossy().into_owned());

    println!("artifact: {spec_name} ({} mappings)", all_mappings.len());

    for mapping in &all_mappings {
        println!(
            "  {}: {} -> {}",
            mapping.id, mapping.spec_ref_node, mapping.artifact_path
        );
        if let Some(ref h) = mapping.node_hash {
            println!("    node-hash: {h}");
        }
        for range in &mapping.ranges {
            if let (Some(s), Some(e)) = (range.start_line, range.end_line) {
                print!("    range: lines {s}-{e}");
            } else {
                print!("    range: whole file");
            }
            if let Some(ref h) = range.hash {
                println!(" hash={h}");
            } else {
                println!();
            }
        }
    }

    Ok(())
}

fn cmd_drift(path: &Path) -> Result<()> {
    let repo_root = clayers_spec::artifact::find_repo_root(path);
    let report = clayers_spec::drift::check_drift(path, repo_root.as_deref())
        .context("drift check failed")?;

    println!(
        "drift: {} ({} mappings, {} drifted)",
        report.spec_name, report.total_mappings, report.drifted_count
    );

    for md in &report.mapping_drifts {
        match &md.status {
            clayers_spec::drift::DriftStatus::Clean => {
                println!("  {}: OK", md.mapping_id);
            }
            clayers_spec::drift::DriftStatus::SpecDrifted {
                stored_hash,
                current_hash,
            } => {
                println!("  {}: SPEC DRIFTED", md.mapping_id);
                println!("    stored:  {stored_hash}");
                println!("    current: {current_hash}");
            }
            clayers_spec::drift::DriftStatus::ArtifactDrifted {
                stored_hash,
                current_hash,
                artifact_path,
            } => {
                println!("  {}: ARTIFACT DRIFTED", md.mapping_id);
                println!("    file: {artifact_path}");
                println!("    stored:  {stored_hash}");
                println!("    current: {current_hash}");
            }
            clayers_spec::drift::DriftStatus::Unavailable { reason } => {
                println!("  {}: UNAVAILABLE ({reason})", md.mapping_id);
            }
        }
    }

    if report.drifted_count > 0 {
        process::exit(1);
    }
    Ok(())
}

fn cmd_connectivity(path: &Path) -> Result<()> {
    let report = clayers_spec::connectivity::analyze_connectivity(path)
        .context("connectivity analysis failed")?;

    println!("connectivity: {}", report.spec_name);
    println!(
        "  nodes: {}, edges: {}, density: {:.4}",
        report.node_count, report.edge_count, report.density
    );
    println!("  connected components: {}", report.components.len());

    if !report.isolated_nodes.is_empty() {
        println!(
            "  isolated nodes ({}): {}",
            report.isolated_nodes.len(),
            report.isolated_nodes.join(", ")
        );
    }

    if !report.hub_nodes.is_empty() {
        println!("  hub nodes (top by degree):");
        for hub in &report.hub_nodes {
            println!(
                "    {} (in={}, out={}, total={})",
                hub.id, hub.in_degree, hub.out_degree, hub.total_degree
            );
        }
    }

    if !report.bridge_nodes.is_empty() {
        println!("  bridge nodes (top by betweenness):");
        for bridge in &report.bridge_nodes {
            println!("    {} (centrality={:.4})", bridge.id, bridge.centrality);
        }
    }

    if !report.relation_type_counts.is_empty() {
        println!("  relation types:");
        let mut types: Vec<_> = report.relation_type_counts.iter().collect();
        types.sort_by(|a, b| b.1.cmp(a.1));
        for (rtype, count) in types {
            println!("    {rtype}: {count}");
        }
    }

    if report.cycles.is_empty() {
        println!("  cycles: none");
    } else {
        println!(
            "  cycles: {} ({} acyclic violations)",
            report.cycles.len(),
            report.acyclic_violations
        );
        for cycle in &report.cycles {
            let violation = if cycle.has_acyclic_violation {
                " [VIOLATION]"
            } else {
                ""
            };
            println!(
                "    {} (types: {}){violation}",
                cycle.nodes.join(" -> "),
                cycle
                    .edge_types
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }

    Ok(())
}

fn cmd_neighbors(
    landing_id: &str,
    path: &Path,
    json: bool,
    hub_threshold: usize,
    top_per_bucket: usize,
    peek_chars: usize,
) -> Result<()> {
    let config = clayers_spec::neighbors::Config {
        hub_threshold,
        top_per_bucket,
        peek_chars,
    };
    let bundle = clayers_spec::neighbors::neighbors_for(path, landing_id, config)
        .context("neighbors walk failed")?;

    if json {
        println!(
            "{}",
            serde_json::to_string(&bundle)
                .context("serializing neighbor bundle to JSON")?
        );
    } else {
        // Human-readable rendering for terminal use.
        println!("neighbors: {}", bundle.landing_id);
        println!(
            "  degree={} hub_engaged={} kept={}",
            bundle.degree_observed,
            bundle.hub_engaged,
            bundle.candidates.len()
        );
        println!("  by_edge_kind: {:?}", bundle.neighbors_by_edge_kind);
        for c in &bundle.candidates {
            let kind = match &c.edge_subtype {
                Some(sub) => format!("{}:{sub}", c.edge_kind),
                None => c.edge_kind.clone(),
            };
            println!("  [{kind}] {} — {}", c.id, c.peek);
        }
    }
    Ok(())
}

fn resolve_schema_dir(path: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = path {
        return Ok(p.to_path_buf());
    }
    let cwd = std::env::current_dir().context("cannot determine current directory")?;
    clayers_spec::discovery::find_schema_dir(&cwd)
        .context("no schema directory found (looked for schemas/ and .clayers/schemas/)")
}

fn cmd_schema(path: Option<&Path>, layers: &[String]) -> Result<()> {
    let schema_dir = resolve_schema_dir(path)?;
    let schema = if layers.is_empty() {
        clayers_spec::rnc::export_rnc(&schema_dir).context("schema export failed")?
    } else {
        let prefixes: Vec<&str> = layers.iter().map(String::as_str).collect();
        clayers_spec::rnc::export_rnc_filtered(&schema_dir, &prefixes)
            .context("schema export failed")?
    };
    print!("{schema}");
    Ok(())
}

fn cmd_adopt(path: &Path, update: bool, skills: bool) -> Result<()> {
    crate::adopt::adopt(path, update, skills)
}

#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
fn cmd_query(
    path: Option<&Path>,
    xpath: &str,
    count: bool,
    text: bool,
    _all: bool,
    rev: Option<&str>,
    branch_arg: Option<&str>,
    db_arg: Option<&Path>,
    files: &[String],
    json: bool,
) -> Result<()> {
    // Determine mode: spec vs repo.
    let use_spec = path.is_some_and(|p| p.is_dir() && !p.join(".clayers.db").exists());

    if use_spec {
        let path = path.unwrap();
        let mode = if count {
            clayers_spec::query::QueryMode::Count
        } else if text {
            clayers_spec::query::QueryMode::Text
        } else {
            clayers_spec::query::QueryMode::Xml
        };
        let result =
            clayers_spec::query::execute_query(path, xpath, mode).context("query failed")?;
        let entries = vec![("(combined)".to_string(), spec_to_values(&result))];
        print_output(&entries, json);
        return Ok(());
    }

    // Repo query mode.
    let mode = if count {
        clayers_repo::QueryMode::Count
    } else if text {
        clayers_repo::QueryMode::Text
    } else {
        clayers_repo::QueryMode::Xml
    };

    let db_path = if let Some(db) = db_arg {
        db.to_path_buf()
    } else if let Some(p) = path {
        if p.extension().is_some_and(|e| e == "db") {
            p.to_path_buf()
        } else {
            p.join(".clayers.db")
        }
    } else {
        let cwd = std::env::current_dir().context("failed to get CWD")?;
        let (_, db) = crate::repo::discover_repo(&cwd)?;
        db
    };

    let conn = crate::repo::open_cli_db(&db_path)?;
    let current_branch =
        crate::repo::schema::get_meta(&conn, "current_branch")?.unwrap_or_else(|| "main".into());
    let revspec = if let Some(r) = rev {
        r.to_string()
    } else if let Some(b) = branch_arg {
        format!("refs/heads/{b}")
    } else {
        format!("refs/heads/{current_branch}")
    };

    let files = files.to_vec();
    crate::repo::block_on(async move {
        let store = clayers_repo::SqliteStore::open(&db_path)?;
        let repo = clayers_repo::Repo::init(store);
        let namespaces = vec![];
        let doc_results = repo
            .query_by_document(&revspec, xpath, mode, &namespaces, &files)
            .await?;
        let entries: Vec<(String, Vec<String>)> = doc_results
            .into_iter()
            .map(|d| (d.path, repo_to_values(&d.result)))
            .collect();
        print_output(&entries, json);
        Ok(())
    })
}

/// Extract string values from a spec query result.
fn spec_to_values(result: &clayers_spec::query::QueryResult) -> Vec<String> {
    match result {
        clayers_spec::query::QueryResult::Count(n) => vec![n.to_string()],
        clayers_spec::query::QueryResult::Text(texts) => texts.clone(),
        clayers_spec::query::QueryResult::Xml(xmls) => xmls.clone(),
    }
}

/// Extract string values from a repo query result.
fn repo_to_values(result: &clayers_repo::QueryResult) -> Vec<String> {
    match result {
        clayers_repo::QueryResult::Count(n) => vec![n.to_string()],
        clayers_repo::QueryResult::Text(texts) => texts.clone(),
        clayers_repo::QueryResult::Xml(xmls) => xmls.clone(),
    }
}

/// Print query output: per-document entries as plain text or JSON.
fn print_output(entries: &[(String, Vec<String>)], json: bool) {
    if json {
        print_json(entries);
    } else {
        print_plain(entries);
    }
}

fn print_plain(entries: &[(String, Vec<String>)]) {
    let single_doc = entries.len() == 1;
    for (path, values) in entries {
        if single_doc && path == "(combined)" {
            for v in values {
                println!("{v}");
            }
        } else {
            println!("--- {path} ---");
            for v in values {
                println!("{v}");
            }
        }
    }
}

fn print_json(entries: &[(String, Vec<String>)]) {
    let docs: Vec<serde_json::Value> = entries
        .iter()
        .map(|(path, values)| {
            // If single numeric value (count mode), emit as integer.
            if values.len() == 1 && let Ok(n) = values[0].parse::<usize>() {
                return serde_json::json!({ "file": path, "count": n });
            }
            serde_json::json!({ "file": path, "matches": values })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&docs).unwrap_or_default());
}

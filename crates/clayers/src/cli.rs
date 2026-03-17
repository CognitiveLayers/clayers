use std::path::{Path, PathBuf};
use std::process;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "clayers", about = "Cognitive layers spec tooling")]
struct Cli {
    #[command(subcommand)]
    command: Command,
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
    },
    /// Bootstrap clayers in a project (plant schemas, amend agent file).
    Adopt {
        /// Path to the target project directory.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Update outdated schemas and instructions in an already-adopted project.
        #[arg(long)]
        update: bool,
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
        /// Source bare .db file or repository directory.
        source: PathBuf,
        /// Target directory (defaults to derived from source name).
        target: Option<PathBuf>,
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
    /// Restore files to their committed state.
    Revert {
        /// Files to revert.
        files: Vec<PathBuf>,
    },
}

#[derive(Subcommand)]
enum RemoteAction {
    /// Add a new remote.
    Add {
        /// Remote name.
        name: String,
        /// Remote URL (path to .db file).
        url: String,
    },
    /// Remove a remote.
    Remove {
        /// Remote name.
        name: String,
    },
    /// List all remotes.
    List,
}

pub fn cli_main() {
    let cli = Cli::parse();
    if let Err(e) = run(&cli) {
        eprintln!("error: {e:#}");
        process::exit(1);
    }
}

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
        } => cmd_query(
            path.as_deref(),
            xpath,
            *count,
            *text,
            *all,
            rev.as_deref(),
            branch.as_deref(),
            db.as_deref(),
        ),
        Command::Adopt { path, update } => cmd_adopt(path, *update),

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
        Command::Clone { source, target } => {
            let default_target;
            let target = if let Some(t) = target { t } else {
                // Derive from source file stem.
                let stem = source
                    .file_stem().map_or_else(|| "cloned-repo".into(), |s| s.to_string_lossy().into_owned());
                default_target = PathBuf::from(stem);
                &default_target
            };
            crate::repo::init::cmd_clone(source, target)
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
            let remote_action = match action {
                RemoteAction::Add { name, url } => crate::repo::remote::RemoteAction::Add {
                    name: name.clone(),
                    url: url.clone(),
                },
                RemoteAction::Remove { name } => {
                    crate::repo::remote::RemoteAction::Remove { name: name.clone() }
                }
                RemoteAction::List => crate::repo::remote::RemoteAction::List,
            };
            crate::repo::remote::cmd_remote(remote_action)
        }
        Command::Revert { files } => crate::repo::revert::cmd_revert(files),
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

fn cmd_adopt(path: &Path, update: bool) -> Result<()> {
    crate::adopt::adopt(path, update)
}

#[allow(clippy::too_many_arguments)]
fn cmd_query(
    path: Option<&Path>,
    xpath: &str,
    count: bool,
    text: bool,
    _all: bool,
    rev: Option<&str>,
    branch_arg: Option<&str>,
    db_arg: Option<&Path>,
) -> Result<()> {
    // Determine mode: spec vs repo.
    let use_spec = path.is_some_and(|p| p.is_dir() && !p.join(".clayers.db").exists());

    if use_spec {
        // Fall back to spec query (existing behavior).
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
        print_spec_query_result(result);
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

    // Resolve db_path.
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

    // Determine revspec: --rev, --branch, or current branch.
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

    crate::repo::block_on(async move {
        let store = clayers_repo::SqliteStore::open(&db_path)?;
        let repo = clayers_repo::Repo::init(store);
        let namespaces = vec![];
        let result = repo.query(&revspec, xpath, mode, &namespaces).await?;
        print_repo_query_result(result);
        Ok(())
    })
}

fn print_spec_query_result(result: clayers_spec::query::QueryResult) {
    match result {
        clayers_spec::query::QueryResult::Count(n) => {
            println!("{n}");
        }
        clayers_spec::query::QueryResult::Text(texts) => {
            for t in &texts {
                println!("{t}");
            }
        }
        clayers_spec::query::QueryResult::Xml(xmls) => {
            for x in &xmls {
                println!("{x}");
            }
        }
    }
}

fn print_repo_query_result(result: clayers_repo::QueryResult) {
    match result {
        clayers_repo::QueryResult::Count(n) => {
            println!("{n}");
        }
        clayers_repo::QueryResult::Text(texts) => {
            for t in &texts {
                println!("{t}");
            }
        }
        clayers_repo::QueryResult::Xml(xmls) => {
            for x in &xmls {
                println!("{x}");
            }
        }
    }
}

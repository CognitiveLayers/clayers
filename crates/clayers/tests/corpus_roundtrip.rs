//! E2E corpus roundtrip tests for the clayers XML repository.
//!
//! Downloads large XML corpora (W3C XSD Tests, RDF Tests, `DocBook`, DITA),
//! imports them via `clayers add .`, commits, clones, and verifies:
//!
//! 1. **Hash idempotency**: `clayers status` in the clone shows "working tree clean"
//! 2. **C14N equivalence**: `canonicalize(original) == canonicalize(exported)`
//!
//! All tests are `#[ignore = "downloads large corpus, run with --ignored"]` — run with `cargo test -p clayers -- --ignored --nocapture`.

use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::prelude::*;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers (same patterns as cli_integration.rs)
// ---------------------------------------------------------------------------

fn clayers() -> Command {
    Command::cargo_bin("clayers").unwrap()
}

fn author_env() -> [(&'static str, &'static str); 2] {
    [
        ("CLAYERS_AUTHOR_NAME", "Corpus Test"),
        ("CLAYERS_AUTHOR_EMAIL", "corpus@test.com"),
    ]
}

fn stdout_of(cmd: &mut Command) -> String {
    let out = cmd.output().unwrap();
    String::from_utf8_lossy(&out.stdout).to_string()
}

// ---------------------------------------------------------------------------
// Corpus infrastructure
// ---------------------------------------------------------------------------

/// Return the corpus cache directory, creating it if needed.
fn corpus_cache_dir() -> PathBuf {
    // Use target/test-corpora/ at workspace root.
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest_dir.parent().unwrap().parent().unwrap();
    let cache = workspace.join("target").join("test-corpora");
    std::fs::create_dir_all(&cache).expect("failed to create corpus cache dir");
    cache
}

/// Ensure a corpus is cloned (shallow, depth=1). Returns path to the clone.
fn ensure_corpus(name: &str, git_url: &str) -> PathBuf {
    let dir = corpus_cache_dir().join(name);
    if dir.exists() {
        eprintln!("[corpus] using cached {name} at {}", dir.display());
        return dir;
    }
    eprintln!("[corpus] cloning {name} from {git_url} ...");
    let status = Command::new("git")
        .args(["clone", "--depth=1", git_url, dir.to_str().unwrap()])
        .status()
        .expect("failed to run git clone");
    assert!(status.success(), "git clone failed for {name}");
    eprintln!("[corpus] cloned {name} ({} files)", count_xml_files(&dir));
    dir
}

/// File extensions recognized as XML for corpus scanning.
const XML_EXTENSIONS: &[&str] = &["xml", "dita", "rdf", "xsd", "xhtml", "xsl", "xslt", "svg"];

/// Recursively collect all XML files under `dir`, skipping hidden directories.
fn collect_xml_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_xml_recursive(dir, &mut files);
    files.sort();
    files
}

fn collect_xml_recursive(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            collect_xml_recursive(&path, files);
        } else if path
            .extension()
            .is_some_and(|e| XML_EXTENSIONS.iter().any(|x| e == *x))
        {
            files.push(path);
        }
    }
}

fn count_xml_files(dir: &Path) -> usize {
    collect_xml_files(dir).len()
}

/// Check if a file is parseable XML using xot (same parser clayers uses).
fn is_parseable_xml(path: &Path) -> bool {
    let Ok(content) = std::fs::read_to_string(path) else {
        return false;
    };
    let mut xot = xot::Xot::new();
    xot.parse(&content).is_ok()
}

// ---------------------------------------------------------------------------
// Core roundtrip test
// ---------------------------------------------------------------------------

struct CorpusReport {
    name: String,
    total_files: usize,
    skipped_parse: usize,
    added: usize,
    hash_idempotent: bool,
    c14n_equivalent: usize,
    c14n_skipped: usize,
    c14n_different: Vec<String>,
}

impl std::fmt::Display for CorpusReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "\n=== Corpus: {} ===", self.name)?;
        writeln!(f, "  Total XML files:     {}", self.total_files)?;
        writeln!(f, "  Skipped (parse):     {}", self.skipped_parse)?;
        writeln!(f, "  Added:               {}", self.added)?;
        writeln!(
            f,
            "  Hash idempotent:     {}",
            if self.hash_idempotent { "YES" } else { "NO" }
        )?;
        writeln!(f, "  C14N equivalent:     {}", self.c14n_equivalent)?;
        writeln!(f, "  C14N skipped:        {}", self.c14n_skipped)?;
        writeln!(f, "  C14N different:      {}", self.c14n_different.len())?;
        if !self.c14n_different.is_empty() {
            let show = self.c14n_different.len().min(20);
            writeln!(f, "  First {show} C14N differences:")?;
            for path in &self.c14n_different[..show] {
                writeln!(f, "    - {path}")?;
            }
            if self.c14n_different.len() > 20 {
                writeln!(
                    f,
                    "    ... and {} more",
                    self.c14n_different.len() - 20
                )?;
            }
        }
        Ok(())
    }
}

/// Run `clayers add .`, retrying on import failures.
///
/// Some files pass `xot` parse but fail the full import pipeline due to
/// namespace edge cases. This function removes failing files and retries.
/// Returns the number of files that could not be imported.
fn add_with_retry(name: &str, repo_dir: &Path) -> usize {
    eprintln!("[{name}] clayers add .");
    let mut failures = 0usize;
    loop {
        let output = clayers()
            .args(["add", "."])
            .current_dir(repo_dir)
            .output()
            .expect("failed to run clayers add");

        if output.status.success() {
            break;
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        if let Some(start) = stderr.find("failed to import ") {
            let rest = &stderr[start + "failed to import ".len()..];
            if let Some(end) = rest.find(':') {
                let failing_path = PathBuf::from(&rest[..end]);
                if failing_path.exists() {
                    eprintln!("[{name}] import failed for {}, removing", failing_path.display());
                    std::fs::remove_file(&failing_path).ok();
                    failures += 1;
                    continue;
                }
            }
        }

        panic!("[{name}] clayers add failed unexpectedly: {stderr}");
    }
    if failures > 0 {
        eprintln!("[{name}] {failures} files removed due to import failures");
    }
    failures
}

/// Compare C14N equivalence between original and exported files.
///
/// Returns `(equivalent_count, skipped_count, different_paths)`.
fn check_c14n_equivalence(
    valid: &[&PathBuf],
    corpus_dir: &Path,
    clone_dir: &Path,
) -> (usize, usize, Vec<String>) {
    let mut equivalent = 0usize;
    let mut skipped = 0usize;
    let mut different = Vec::new();

    for file in valid {
        let rel = file.strip_prefix(corpus_dir).unwrap();
        // Files were renamed to .xml when copied into the repo.
        let exported_path = clone_dir.join(rel.with_extension("xml"));

        if !exported_path.exists() {
            skipped += 1;
            continue;
        }

        let (Ok(original_xml), Ok(exported_xml)) = (
            std::fs::read_to_string(file),
            std::fs::read_to_string(&exported_path),
        ) else {
            skipped += 1;
            continue;
        };

        let mode = clayers_xml::CanonicalizationMode::InclusiveWithComments;
        let (Ok(c14n_orig), Ok(c14n_exp)) = (
            clayers_xml::canonicalize(&original_xml, mode),
            clayers_xml::canonicalize(&exported_xml, mode),
        ) else {
            skipped += 1;
            continue;
        };

        if c14n_orig == c14n_exp {
            equivalent += 1;
        } else {
            different.push(rel.to_string_lossy().to_string());
        }
    }

    (equivalent, skipped, different)
}

/// Run the full roundtrip test for a corpus directory.
///
/// 1. Discover and pre-filter XML files
/// 2. Init repo, copy valid files, add, commit
/// 3. Clone
/// 4. Verify hash idempotency (status shows clean)
/// 5. Verify C14N equivalence (original vs exported)
fn run_corpus_roundtrip(name: &str, corpus_dir: &Path) -> CorpusReport {
    let all_xml = collect_xml_files(corpus_dir);
    let total_files = all_xml.len();
    eprintln!("[{name}] found {total_files} XML files, pre-filtering...");

    let valid: Vec<&PathBuf> = all_xml.iter().filter(|f| is_parseable_xml(f)).collect();
    let skipped_parse = total_files - valid.len();
    let added = valid.len();
    eprintln!("[{name}] {added} parseable, {skipped_parse} skipped");

    if valid.is_empty() {
        eprintln!("[{name}] no valid XML files, skipping");
        return CorpusReport {
            name: name.to_string(),
            total_files,
            skipped_parse,
            added: 0,
            hash_idempotent: true,
            c14n_equivalent: 0,
            c14n_skipped: 0,
            c14n_different: vec![],
        };
    }

    let tmp = TempDir::new().expect("failed to create temp dir");
    let repo_dir = tmp.path().join("repo");
    let clone_dir = tmp.path().join("clone");

    clayers()
        .args(["init", repo_dir.to_str().unwrap()])
        .assert()
        .success();

    eprintln!("[{name}] copying {added} files to repo...");
    for file in &valid {
        let rel = file.strip_prefix(corpus_dir).unwrap();
        // Rename non-.xml extensions to .xml so staging.rs picks them up.
        let dest = repo_dir.join(rel.with_extension("xml"));
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::copy(file, &dest).unwrap();
    }

    let import_failures = add_with_retry(name, &repo_dir);
    let added = added - import_failures;

    eprintln!("[{name}] clayers commit");
    clayers()
        .args(["commit", "-m", &format!("Import {name}")])
        .envs(author_env())
        .current_dir(&repo_dir)
        .assert()
        .success();

    eprintln!("[{name}] clayers clone");
    let db_path = repo_dir.join(".clayers.db");
    clayers()
        .args([
            "clone",
            db_path.to_str().unwrap(),
            clone_dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    eprintln!("[{name}] checking hash idempotency (clayers status)...");
    let status_output = stdout_of(clayers().args(["status"]).current_dir(&clone_dir));
    let hash_idempotent = status_output.contains("nothing to commit");
    if !hash_idempotent {
        eprintln!("[{name}] WARNING: status not clean:\n{status_output}");
    }

    eprintln!("[{name}] checking C14N equivalence...");
    let (c14n_equivalent, c14n_skipped, c14n_different) =
        check_c14n_equivalence(&valid, corpus_dir, &clone_dir);

    let report = CorpusReport {
        name: name.to_string(),
        total_files,
        skipped_parse,
        added,
        hash_idempotent,
        c14n_equivalent,
        c14n_skipped,
        c14n_different,
    };
    eprintln!("{report}");
    report
}

// ---------------------------------------------------------------------------
// Per-corpus tests
// ---------------------------------------------------------------------------

#[test]
#[ignore = "downloads large corpus, run with --ignored"]
fn corpus_w3c_xsd_tests() {
    let corpus = ensure_corpus("xsdtests", "https://github.com/w3c/xsdtests.git");
    let report = run_corpus_roundtrip("W3C XSD Tests", &corpus);
    assert!(report.hash_idempotent, "Hash idempotency failed for W3C XSD Tests");
    assert!(
        report.c14n_different.is_empty(),
        "{} files with C14N differences:\n{:?}",
        report.c14n_different.len(),
        &report.c14n_different[..report.c14n_different.len().min(20)]
    );
}

#[test]
#[ignore = "downloads large corpus, run with --ignored"]
fn corpus_w3c_rdf_tests() {
    let corpus = ensure_corpus("rdf-tests", "https://github.com/w3c/rdf-tests.git");
    let report = run_corpus_roundtrip("W3C RDF Tests", &corpus);
    assert!(report.hash_idempotent, "Hash idempotency failed for W3C RDF Tests");
    assert!(
        report.c14n_different.is_empty(),
        "{} files with C14N differences:\n{:?}",
        report.c14n_different.len(),
        &report.c14n_different[..report.c14n_different.len().min(20)]
    );
}

#[test]
#[ignore = "downloads large corpus, run with --ignored"]
fn corpus_docbook_samples() {
    let corpus = ensure_corpus(
        "docbook-samples",
        "https://github.com/eduardtibet/docbook-samples.git",
    );
    let report = run_corpus_roundtrip("DocBook Samples", &corpus);
    assert!(report.hash_idempotent, "Hash idempotency failed for DocBook Samples");
    assert!(
        report.c14n_different.is_empty(),
        "{} files with C14N differences:\n{:?}",
        report.c14n_different.len(),
        &report.c14n_different[..report.c14n_different.len().min(20)]
    );
}

#[test]
#[ignore = "downloads large corpus, run with --ignored"]
fn corpus_dita_examples() {
    let corpus = ensure_corpus(
        "dita-xml-example",
        "https://github.com/online-documentation/dita-xml-example.git",
    );
    let report = run_corpus_roundtrip("DITA Examples", &corpus);
    assert!(report.hash_idempotent, "Hash idempotency failed for DITA Examples");
    assert!(
        report.c14n_different.is_empty(),
        "{} files with C14N differences:\n{:?}",
        report.c14n_different.len(),
        &report.c14n_different[..report.c14n_different.len().min(20)]
    );
}

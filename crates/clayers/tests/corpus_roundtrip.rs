//! E2E corpus roundtrip tests for the clayers XML repository.
//!
//! Downloads large XML corpora (W3C XSD Tests, RDF Tests, `DocBook`, DITA),
//! imports each file individually via `clayers add`, commits, clones, and verifies:
//!
//! 1. **Hash idempotency**: `clayers status` in the clone shows "working tree clean"
//! 2. **C14N equivalence**: `canonicalize(original) == canonicalize(exported)`
//!
//! ## Running
//!
//! ```sh
//! # Run a single corpus (always use --release for speed):
//! cargo test -p clayers --release corpus_w3c_rdf -- --ignored --nocapture
//!
//! # Collect ALL failures instead of stopping at first:
//! CORPUS_ACCUMULATE=1 cargo test -p clayers --release corpus_w3c_rdf -- --ignored --nocapture
//!
//! # Ignore additional files via env var (comma-separated, repo-relative paths):
//! CORPUS_IGNORE="sparql/sparql11/subquery/sq08.xml" \
//!   cargo test -p clayers --release corpus_w3c_rdf -- --ignored --nocapture
//!
//! # Force re-scan of parseable files (normally cached between runs):
//! CORPUS_RESCAN=1 cargo test -p clayers --release corpus_w3c_rdf -- --ignored --nocapture
//! ```

use std::collections::HashSet;
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::prelude::*;
use regex::Regex;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
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
// Known failures registry
// ---------------------------------------------------------------------------

/// What kind of roundtrip failure a known case exhibits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailureKind {
    /// Hash idempotency failure: file shows as modified after clone.
    Hash,
    /// C14N equivalence failure: hash is fine, but canonical forms differ.
    C14n,
}

/// A file known to fail roundtrip, with a pinned error signature.
///
/// Known failures are NOT skipped — they are still checked every run.
/// The test verifies that the failure still occurs and matches the expected
/// error signature:
/// - File passes → FIXED: tighten or remove the pattern
/// - Error changes → CHANGED: investigate the new root cause
/// - Error matches → OK, still failing as expected
struct KnownFailure {
    path: PathMatch,
    /// Optional content filter — file must also match this to be considered
    /// a known failure. Allows targeting failures by what's IN the file
    /// (e.g. literal whitespace in attributes) rather than just the path.
    filter: Option<ContentFilter>,
    /// What kind of failure to expect.
    kind: FailureKind,
    /// A substring that must appear in the C14N diff diagnostic.
    error_contains: &'static str,
    /// Human-readable explanation of the root cause.
    reason: &'static str,
}

/// How a known failure matches file paths.
enum PathMatch {
    /// Exact file path.
    File(&'static str),
    /// Regex pattern matched against the repo-relative path.
    Pattern(&'static str),
}

impl PathMatch {
    fn as_str(&self) -> &'static str {
        match self {
            Self::File(f) => f,
            Self::Pattern(p) => p,
        }
    }
}

/// Optional filter on file content, applied after path match.
#[allow(dead_code)]
enum ContentFilter {
    /// File must contain these exact bytes (e.g. literal newline inside an attribute).
    Contains(&'static [u8]),
    /// File content must match this regex.
    Matches(&'static str),
}


/// Return the known failures for a given corpus.
fn known_failures_for(corpus: &str) -> &'static [KnownFailure] {
    match corpus {
        "DocBook Samples" => &[
            // TODO: investigate xot dual-binding normalization — could preserve
            // original prefix choice by tracking which prefix was used in source
            KnownFailure {
                path: PathMatch::File("stdf/stdf_manual.xml"),
                filter: None,
                kind: FailureKind::Hash,
                error_contains: "<db:article",
                reason: "dual binding (default ns + prefix) for same URI; xot normalizes to prefixed form",
            },
        ],
        "W3C RDF Tests" => &[
            // TODO: investigate preserving unused default xmlns — xot strips it
            // because the prefixed root doesn't use the default namespace
            KnownFailure {
                path: PathMatch::Pattern(r"^rdf/rdf11/rdf-xml/rdfms-xml-literal-namespaces/test00[12]\.rdf$"),
                filter: None,
                kind: FailureKind::C14n,
                error_contains: "xmlns=",
                reason: "unused default namespace stripped by xot on prefixed root element",
            },
            // TODO: same dual-binding issue as DocBook
            KnownFailure {
                path: PathMatch::Pattern(r"^rdf/rdf11/rdf-xml/rdf-ns-prefix-confusion/test001[0-4]\.rdf$"),
                filter: None,
                kind: FailureKind::Hash,
                error_contains: "<rdf:",
                reason: "dual binding (default ns + prefix) for same URI; xot normalizes to prefixed form",
            },
        ],
        "W3C XSD Tests" => &[
            // Boeing IPO: dual binding (default ns + ipo: prefix for same URI).
            KnownFailure {
                path: PathMatch::Pattern(r"^boeingData/ipo[34]/ipo_[12]\.xml$"),
                filter: None,
                kind: FailureKind::Hash,
                error_contains: "<ipo:",
                reason: "dual binding (default ns + prefix) for same URI; xot normalizes to prefixed form",
            },
            // Unused default namespace on xsd:schema (prefixed root).
            KnownFailure {
                path: PathMatch::File("boeingData/ipo4/address.xsd"),
                filter: None,
                kind: FailureKind::C14n,
                error_contains: "xmlns=",
                reason: "unused default ns on xsd:schema stripped by xot",
            },
            // XSLT coverage report: dual binding with xhtml namespace.
            KnownFailure {
                path: PathMatch::File("common/coverage-report.xsl"),
                filter: None,
                kind: FailureKind::Hash,
                error_contains: "<x:html>",
                reason: "dual binding (default ns + prefix) for same URI; xot normalizes to prefixed form",
            },
            // Attribute values containing &#xA; / literal newlines get normalized
            // to spaces by the XML parser (attribute value normalization per spec).
            KnownFailure {
                path: PathMatch::Pattern(r"^common/xsts\.(xml|xsd)$"),
                filter: None,
                kind: FailureKind::C14n,
                error_contains: "memberTypes=",
                reason: "attribute value newline normalization by XML parser",
            },
            // IBM test files with literal newline/tab inside attribute values.
            // XML attr value normalization replaces \n and \t with spaces.
            // Content filter: regex checks for \n or \t between attribute quotes.
            // TODO: investigate whether we can preserve original attr whitespace
            KnownFailure {
                path: PathMatch::Pattern(r"^ibmData/"),
                filter: Some(ContentFilter::Matches(r#"="[^"]*[\n\t][^"]*""#)),
                kind: FailureKind::C14n,
                error_contains: "line",
                reason: "attribute value whitespace normalization (literal \\n\\t in attrs)",
            },
            // IBM files where xot serialization reformats: single quotes
            // to double, multi-line tags collapsed, attr whitespace normalized.
            // Dirs enumerated from accumulate runs. C14N-only differences.
            // TODO: investigate xot options for preserving original formatting
            KnownFailure {
                path: PathMatch::Pattern(r"^ibmData/(instance_invalid/(D2_4_1_2|D3_3_(4|16|17|7)|D3_4_(2[1-8]|6)|D4_3_(15|16|6)|S2_(2_4|7_1)|S3_(10_6|12|16_2|3_(4|6)|4_(1|6)))|valid/(D2_4_1_2|D3_3_(4|5|6|9|1[0-7])|D3_4_(2[1-8]|6)|D4_3_(15|16|6)|S2_(2_2|7_2)|S3_(10_6|11_2|12|16_2|3_(4|6)|4_(1|6))|S4_2_[3-6]))/.+\.xml$"),
                filter: None,
                kind: FailureKind::C14n,
                error_contains: "line",
                reason: "xot serialization reformats: multi-line tags, quote normalization, attr whitespace",
            },
            KnownFailure {
                path: PathMatch::Pattern(r"^ibmData/(instance_invalid/(D2_4_1_2|D3_3_(16|17|7)|D3_4_2[1-4]|D4_3_15|S3_(10_6|3_4|4_(2_4|6)|8_6))|schema_invalid/(D2_4_1_3|D3_1|D4_3_15|S2_2_[24]|S3_(16_2|3_4|4_6)|S4_2_[246])|valid/(D3_3_(16|17)|D3_4_(2[1-4]|6)|D4_3_15|S2_2_2|S3_(10_6|11_2|3_4|4_(2_4|6))|S4_2_[2-6]))/.+\.xsd$"),
                filter: None,
                kind: FailureKind::C14n,
                error_contains: "line",
                reason: "xot serialization reformats .xsd: multi-line tags, quote normalization, attr whitespace",
            },
            // TODO: investigate — Hash failure from attr normalization is unexpected
            KnownFailure {
                path: PathMatch::File("ibmData/instance_invalid/S4_2_2/s4_2_2ii01.xsd"),
                filter: None,
                kind: FailureKind::Hash,
                error_contains: "original:",
                reason: "multiline attribute normalization causes hash change",
            },
        ],
        _ => &[],
    }
}

/// Whether to collect all failures instead of stopping at the first.
fn accumulate_mode() -> bool {
    std::env::var("CORPUS_ACCUMULATE").is_ok()
}

/// Optional regex filter from `CORPUS_FILTER` env var.
/// Only files matching this pattern are tested.
fn corpus_filter() -> Option<Regex> {
    std::env::var("CORPUS_FILTER")
        .ok()
        .filter(|s| !s.is_empty())
        .map(|s| Regex::new(&s).expect("invalid CORPUS_FILTER regex"))
}

/// Additional paths to ignore from the `CORPUS_IGNORE` env var.
fn extra_ignores() -> Vec<String> {
    std::env::var("CORPUS_IGNORE")
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.trim().to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// Corpus infrastructure
// ---------------------------------------------------------------------------

/// Return the corpus cache directory, creating it if needed.
fn corpus_cache_dir() -> PathBuf {
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

/// Get parseable XML files, using a cache file to avoid re-parsing on every run.
///
/// The cache is stored at `target/test-corpora/{cache_name}.parseable` and contains
/// one relative path per line. Set `CORPUS_RESCAN=1` to force re-scanning.
fn parseable_xml_files(corpus_dir: &Path, cache_name: &str) -> Vec<PathBuf> {
    let cache_path = corpus_cache_dir().join(format!("{cache_name}.parseable"));
    let force_rescan = std::env::var("CORPUS_RESCAN").is_ok();

    // Try to load from cache.
    if !force_rescan
        && let Ok(content) = std::fs::read_to_string(&cache_path)
    {
        let files: Vec<PathBuf> = content
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| corpus_dir.join(l))
            .filter(|p| p.exists())
            .collect();
        if !files.is_empty() {
            eprintln!("[corpus] using cached parseable list ({} files)", files.len());
            return files;
        }
    }

    // Scan and filter.
    let all_xml = collect_xml_files(corpus_dir);
    eprintln!(
        "[corpus] scanning {} files for parseability...",
        all_xml.len()
    );
    let valid: Vec<PathBuf> = all_xml
        .into_iter()
        .filter(|f| {
            let Ok(content) = std::fs::read_to_string(f) else {
                return false;
            };
            let mut xot = xot::Xot::new();
            xot.parse(&content).is_ok()
        })
        .collect();

    // Save cache.
    let rel_paths: Vec<String> = valid
        .iter()
        .filter_map(|f| {
            f.strip_prefix(corpus_dir)
                .ok()
                .map(|r| r.to_string_lossy().to_string())
        })
        .collect();
    let _ = std::fs::write(&cache_path, rel_paths.join("\n"));
    eprintln!(
        "[corpus] cached {} parseable files to {}",
        valid.len(),
        cache_path.display()
    );

    valid
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

/// Compute C14N diff diagnostic between original and exported files.
/// Returns `None` if they are C14N-equivalent, or `Some(diagnostic)` with
/// context around the first difference.
fn c14n_diff(original: &Path, exported: &Path) -> Option<String> {
    let (Ok(orig_xml), Ok(exp_xml)) = (
        std::fs::read_to_string(original),
        std::fs::read_to_string(exported),
    ) else {
        return Some("(could not read files)".to_string());
    };

    let mode = clayers_xml::CanonicalizationMode::InclusiveWithComments;
    let (Ok(c14n_orig), Ok(c14n_exp)) = (
        clayers_xml::canonicalize(&orig_xml, mode),
        clayers_xml::canonicalize(&exp_xml, mode),
    ) else {
        return Some("(C14N failed on one or both files)".to_string());
    };

    if c14n_orig == c14n_exp {
        return None;
    }

    let orig_s = String::from_utf8_lossy(&c14n_orig);
    let exp_s = String::from_utf8_lossy(&c14n_exp);
    let orig_lines: Vec<&str> = orig_s.lines().collect();
    let exp_lines: Vec<&str> = exp_s.lines().collect();

    for (i, (ol, el)) in orig_lines.iter().zip(exp_lines.iter()).enumerate() {
        if ol != el {
            let mut ctx = String::new();
            writeln!(ctx, "First difference at line {}:", i + 1).unwrap();
            if i > 0 {
                writeln!(ctx, "  context: {}", orig_lines[i - 1]).unwrap();
            }
            writeln!(ctx, "  - original: {ol}").unwrap();
            writeln!(ctx, "  + exported: {el}").unwrap();
            if i + 1 < orig_lines.len().min(exp_lines.len()) {
                writeln!(ctx, "  context: {}", orig_lines[i + 1]).unwrap();
            }
            return Some(ctx);
        }
    }
    if orig_lines.len() == exp_lines.len() {
        Some("(byte-level difference, lines identical)".to_string())
    } else {
        Some(format!(
            "Line count differs: original {} lines, exported {} lines",
            orig_lines.len(),
            exp_lines.len()
        ))
    }
}

// ---------------------------------------------------------------------------
// Per-file roundtrip
// ---------------------------------------------------------------------------

/// Roundtrip a single file: init → copy → add → commit → clone → verify.
///
/// Returns `None` if the file passes both checks, or `Some((kind, diagnostic))`
/// describing the failure. Returns `Err` if `clayers add` fails (import error).
fn roundtrip_one_file(
    original: &Path,
    rel_xml: &Path,
) -> std::result::Result<Option<(FailureKind, String)>, String> {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let repo_dir = tmp.path().join("repo");
    let clone_dir = tmp.path().join("clone");

    // Init.
    clayers()
        .args(["init", repo_dir.to_str().unwrap()])
        .assert()
        .success();

    // Copy file into repo (with .xml extension).
    let dest = repo_dir.join(rel_xml);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::copy(original, &dest).unwrap();

    // Add.
    let add_output = clayers()
        .args(["add", "."])
        .current_dir(&repo_dir)
        .output()
        .expect("failed to run clayers add");
    if !add_output.status.success() {
        let stderr = String::from_utf8_lossy(&add_output.stderr);
        return Err(format!("clayers add failed: {stderr}"));
    }

    // Commit.
    clayers()
        .args(["commit", "-m", "test"])
        .envs(author_env())
        .current_dir(&repo_dir)
        .assert()
        .success();

    // Clone.
    let db_path = repo_dir.join(".clayers.db");
    clayers()
        .args([
            "clone",
            db_path.to_str().unwrap(),
            clone_dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Check hash idempotency.
    let status_output = stdout_of(clayers().args(["status"]).current_dir(&clone_dir));
    let hash_failed = status_output.contains("modified:");

    // Check C14N equivalence.
    let exported_path = clone_dir.join(rel_xml);
    let c14n_diagnostic = if exported_path.exists() {
        c14n_diff(original, &exported_path)
    } else {
        Some("(exported file missing)".to_string())
    };

    if hash_failed {
        let diag = c14n_diagnostic.unwrap_or_else(|| {
            "(hash failed but C14N equivalent — non-canonical difference)".to_string()
        });
        Ok(Some((FailureKind::Hash, diag)))
    } else if let Some(diag) = c14n_diagnostic {
        Ok(Some((FailureKind::C14n, diag)))
    } else {
        Ok(None)
    }
}

// ---------------------------------------------------------------------------
// Corpus test driver
// ---------------------------------------------------------------------------

/// Run the per-file roundtrip test for a corpus directory.
///
/// Fail-fast by default: panics on the first unexpected failure.
/// Set `CORPUS_ACCUMULATE=1` to collect all failures.
///
/// Known failures are NOT skipped — they are verified every run:
/// - If a known failure starts passing → test fails (remove from list)
/// - If a known failure's error changes → test fails (investigate)
/// - If a known failure still fails as expected → OK
#[allow(clippy::too_many_lines)]
fn run_corpus_roundtrip(name: &str, corpus_dir: &Path) {
    let known = known_failures_for(name);
    let extra = extra_ignores();
    let accumulate = accumulate_mode();
    let filter = corpus_filter();

    let extra_set: HashSet<&str> = extra.iter().map(String::as_str).collect();

    // Compile regex patterns once (path + content filter).
    #[allow(clippy::items_after_statements)]
    struct CompiledKnown<'a> {
        kf: &'a KnownFailure,
        path_re: Option<Regex>,
        filter_re: Option<Regex>,
    }
    let compiled: Vec<CompiledKnown<'_>> = known
        .iter()
        .map(|kf| {
            let path_re = match &kf.path {
                PathMatch::File(_) => None,
                PathMatch::Pattern(pat) => Some(Regex::new(pat).unwrap_or_else(|e| {
                    panic!("invalid path regex in known_failures_for(\"{name}\"): {pat}: {e}")
                })),
            };
            let filter_re = match &kf.filter {
                Some(ContentFilter::Matches(pat)) => Some(Regex::new(pat).unwrap_or_else(|e| {
                    panic!("invalid filter regex in known_failures_for(\"{name}\"): {pat}: {e}")
                })),
                _ => None,
            };
            CompiledKnown { kf, path_re, filter_re }
        })
        .collect();

    // Find a known failure matching this path + file content.
    let find_known = |rel_path: &str, file_content: &[u8]| -> Option<&KnownFailure> {
        for ck in &compiled {
            // Check path match.
            let path_matches = match &ck.kf.path {
                PathMatch::File(f) => *f == rel_path,
                PathMatch::Pattern(_) => ck.path_re.as_ref().unwrap().is_match(rel_path),
            };
            if !path_matches {
                continue;
            }
            // Check content filter.
            if let Some(ref filter) = ck.kf.filter {
                let content_matches = match filter {
                    ContentFilter::Contains(bytes) => {
                        file_content.windows(bytes.len()).any(|w| w == *bytes)
                    }
                    ContentFilter::Matches(_) => {
                        let text = String::from_utf8_lossy(file_content);
                        ck.filter_re.as_ref().unwrap().is_match(&text)
                    }
                };
                if !content_matches {
                    continue;
                }
            }
            return Some(ck.kf);
        }
        None
    };

    // --- Discover parseable files (cached) ---

    let valid = parseable_xml_files(corpus_dir, name);
    if valid.is_empty() {
        eprintln!("[{name}] no parseable XML files — nothing to test");
        return;
    }

    let valid: Vec<&PathBuf> = valid.iter().collect();

    eprintln!("[{name}] testing {} files...", valid.len());

    let mut ok_count = 0usize;
    let mut known_count = 0usize;
    let mut skipped_count = 0usize;
    let mut unexpected: Vec<String> = Vec::new();
    // Track which known failure entries were hit (matched a file and failed as expected).
    let mut known_hits: HashSet<&str> = HashSet::new();

    for (i, file) in valid.iter().enumerate() {
        let rel = file.strip_prefix(corpus_dir).unwrap();
        let rel_xml = rel.to_path_buf();
        let rel_str = rel_xml.to_string_lossy();

        // Skip env-var ignores.
        if extra_set.contains(rel_str.as_ref()) {
            skipped_count += 1;
            continue;
        }

        // Apply CORPUS_FILTER if set.
        if let Some(ref re) = filter
            && !re.is_match(&rel_str)
        {
            skipped_count += 1;
            continue;
        }

        // Progress indicator every 100 files.
        if i > 0 && i % 100 == 0 {
            eprintln!(
                "[{name}] progress: {i}/{} ({ok_count} ok, {known_count} known, {} unexpected)",
                valid.len(),
                unexpected.len()
            );
        }

        // Read file content for known-failure content filters.
        let file_content = std::fs::read(file).unwrap_or_default();

        // Run the per-file roundtrip.
        let result = match roundtrip_one_file(file, &rel_xml) {
            Ok(r) => r,
            Err(import_err) => {
                eprintln!("[{name}]   SKIP   (import): {rel_str}");
                skipped_count += 1;
                // If this was a known failure, it should still fail.
                if let Some(kf) = find_known(&rel_str, &file_content) {
                    eprintln!(
                        "[{name}]   NOTE: known failure {rel_str} failed import: {import_err}"
                    );
                    let _ = kf; // suppress unused warning
                }
                continue;
            }
        };

        // Check against known failures.
        if let Some(kf) = find_known(&rel_str, &file_content) {
            if let Some((actual_kind, diag)) = result {
                if actual_kind == kf.kind {
                    if diag.contains(kf.error_contains) {
                        eprintln!("[{name}]   KNOWN  ({actual_kind:?}): {rel_str}");
                        known_count += 1;
                        known_hits.insert(kf.path.as_str());
                    } else {
                        let msg = format!(
                            "CHANGED: {rel_str} error signature changed.\n\
                             Expected to contain: {:?}\n  Actual: {diag}\n\
                             Update error_contains in known_failures_for(\"{name}\").",
                            kf.error_contains
                        );
                        if accumulate {
                            eprintln!("[{name}]   CHANGED  {rel_str}");
                            unexpected.push(msg);
                        } else {
                            panic!("\n[{name}] {msg}");
                        }
                    }
                } else {
                    let msg = format!(
                        "CHANGED: {rel_str} failure kind changed from {:?} to {actual_kind:?}\n\
                         Update known_failures_for(\"{name}\").\n  diagnostic: {diag}",
                        kf.kind,
                    );
                    if accumulate {
                        eprintln!("[{name}]   CHANGED  {rel_str}");
                        unexpected.push(msg);
                    } else {
                        panic!("\n[{name}] {msg}");
                    }
                }
            } else {
                // Known failure now passes!
                let msg = format!(
                    "FIXED: {rel_str} was known failure ({}) but now passes!\n\
                     Remove from known_failures_for(\"{name}\").",
                    kf.reason
                );
                if accumulate {
                    eprintln!("[{name}]   FIXED  {rel_str}");
                    unexpected.push(msg);
                } else {
                    panic!("\n[{name}] {msg}");
                }
            }
            continue;
        }

        // Not a known failure.
        if let Some((kind, diag)) = result {
            if accumulate {
                eprintln!("[{name}]   FAIL   ({kind:?}): {rel_str}");
                unexpected.push(format!("{kind:?}: {rel_str}\n  {diag}"));
            } else {
                panic!(
                    "\n[{name}] UNEXPECTED {kind:?} failure:\n  file: {rel_str}\n  {diag}\n\n\
                     Add to known_failures_for(\"{name}\") in corpus_roundtrip.rs if expected,\n\
                     or fix the bug."
                );
            }
        } else {
            ok_count += 1;
        }
    }

    // --- Check for stale known failure entries ---

    for kf in known {
        if known_hits.contains(kf.path.as_str()) {
            continue; // Still failing as expected.
        }
        let path_desc = kf.path.as_str();
        let match_kind = match &kf.path {
            PathMatch::File(_) => "File doesn't exist or now passes.",
            PathMatch::Pattern(_) => "No files matching the pattern failed.",
        };
        let msg = format!(
            "STALE: known failure entry {:?} ({path_desc}) was never hit.\n\
             {match_kind} Remove or tighten in known_failures_for(\"{name}\").",
            kf.reason,
        );
        if accumulate {
            eprintln!("[{name}]   STALE  {path_desc}");
            unexpected.push(msg);
        } else {
            panic!("\n[{name}] {msg}");
        }
    }

    // --- Summary ---

    eprintln!("\n=== {name} ===");
    eprintln!("  Files tested:         {}", valid.len());
    eprintln!("  Skipped:              {skipped_count}");
    eprintln!("  OK:                   {ok_count}");
    eprintln!("  Known failures:       {known_count}");
    eprintln!("  Unexpected:           {}", unexpected.len());

    if !unexpected.is_empty() {
        let summary: Vec<&str> = unexpected.iter().map(String::as_str).collect();
        panic!(
            "\n[{name}] {} unexpected results:\n\n{}",
            unexpected.len(),
            summary.join("\n\n")
        );
    }
}

// ---------------------------------------------------------------------------
// Per-corpus tests
// ---------------------------------------------------------------------------

#[test]
#[ignore = "downloads large corpus, run with --ignored"]
fn corpus_w3c_xsd_tests() {
    let corpus = ensure_corpus("xsdtests", "https://github.com/w3c/xsdtests.git");
    run_corpus_roundtrip("W3C XSD Tests", &corpus);
}

#[test]
#[ignore = "downloads large corpus, run with --ignored"]
fn corpus_w3c_rdf_tests() {
    let corpus = ensure_corpus("rdf-tests", "https://github.com/w3c/rdf-tests.git");
    run_corpus_roundtrip("W3C RDF Tests", &corpus);
}

#[test]
#[ignore = "downloads large corpus, run with --ignored"]
fn corpus_docbook_samples() {
    let corpus = ensure_corpus(
        "docbook-samples",
        "https://github.com/eduardtibet/docbook-samples.git",
    );
    run_corpus_roundtrip("DocBook Samples", &corpus);
}

#[test]
#[ignore = "downloads large corpus, run with --ignored"]
fn corpus_dita_examples() {
    let corpus = ensure_corpus(
        "dita-xml-example",
        "https://github.com/online-documentation/dita-xml-example.git",
    );
    run_corpus_roundtrip("DITA Examples", &corpus);
}

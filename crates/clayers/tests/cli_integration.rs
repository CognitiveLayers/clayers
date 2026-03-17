//! Integration tests for the clayers CLI repository commands.
//!
//! These tests exercise the full CLI through subprocess invocations using
//! `assert_cmd`, running against real `SQLite` databases in temporary directories.

use std::process::Command;

use assert_cmd::prelude::*;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn clayers() -> Command {
    Command::cargo_bin("clayers").unwrap()
}

fn author_env() -> [(&'static str, &'static str); 2] {
    [
        ("CLAYERS_AUTHOR_NAME", "Test Author"),
        ("CLAYERS_AUTHOR_EMAIL", "test@test.com"),
    ]
}

/// Init + write XML + add + commit in one shot. Returns the tmp dir path.
fn setup_committed_repo(xml_files: &[(&str, &str)]) -> TempDir {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path();

    clayers().args(["init"]).current_dir(path).assert().success();

    for (name, content) in xml_files {
        std::fs::write(path.join(name), content).unwrap();
    }

    clayers()
        .args(["add", "."])
        .current_dir(path)
        .assert()
        .success();

    clayers()
        .args(["commit", "-m", "initial"])
        .envs(author_env())
        .current_dir(path)
        .assert()
        .success();

    tmp
}

fn stdout_of(cmd: &mut Command) -> String {
    let out = cmd.output().unwrap();
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn stderr_of(cmd: &mut Command) -> String {
    let out = cmd.output().unwrap();
    String::from_utf8_lossy(&out.stderr).to_string()
}

// ===========================================================================
// init
// ===========================================================================

#[test]
fn init_creates_db() {
    let tmp = TempDir::new().unwrap();
    clayers()
        .args(["init"])
        .current_dir(tmp.path())
        .assert()
        .success();
    assert!(tmp.path().join(".clayers.db").exists());
}

#[test]
fn init_bare_creates_file() {
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("bare.db");
    clayers()
        .args(["init", "--bare", db.to_str().unwrap()])
        .assert()
        .success();
    assert!(db.exists());
}

#[test]
fn init_twice_errors() {
    let tmp = TempDir::new().unwrap();
    clayers()
        .args(["init"])
        .current_dir(tmp.path())
        .assert()
        .success();
    clayers()
        .args(["init"])
        .current_dir(tmp.path())
        .assert()
        .failure();
}

#[test]
fn init_bare_twice_errors() {
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("bare.db");
    clayers()
        .args(["init", "--bare", db.to_str().unwrap()])
        .assert()
        .success();
    clayers()
        .args(["init", "--bare", db.to_str().unwrap()])
        .assert()
        .failure();
}

#[test]
fn init_defaults_to_cwd() {
    let tmp = TempDir::new().unwrap();
    clayers()
        .args(["init"])
        .current_dir(tmp.path())
        .assert()
        .success();
    assert!(tmp.path().join(".clayers.db").exists());
}

#[test]
fn init_shows_untracked_xml() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("doc.xml"), "<r/>").unwrap();
    clayers()
        .args(["init"])
        .current_dir(tmp.path())
        .assert()
        .success();

    let out = stdout_of(
        clayers()
            .args(["status"])
            .current_dir(tmp.path()),
    );
    assert!(out.contains("doc.xml"), "status should show untracked: {out}");
    assert!(
        out.contains("Untracked"),
        "should be in Untracked section: {out}"
    );
}

// ===========================================================================
// add
// ===========================================================================

#[test]
fn add_stages_file() {
    let tmp = TempDir::new().unwrap();
    clayers()
        .args(["init"])
        .current_dir(tmp.path())
        .assert()
        .success();
    std::fs::write(tmp.path().join("doc.xml"), "<root/>").unwrap();

    clayers()
        .args(["add", "doc.xml"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("staged: doc.xml"));
}

#[test]
fn add_multiple_files() {
    let tmp = TempDir::new().unwrap();
    clayers()
        .args(["init"])
        .current_dir(tmp.path())
        .assert()
        .success();
    std::fs::write(tmp.path().join("a.xml"), "<a/>").unwrap();
    std::fs::write(tmp.path().join("b.xml"), "<b/>").unwrap();

    clayers()
        .args(["add", "a.xml", "b.xml"])
        .current_dir(tmp.path())
        .assert()
        .success();

    let out = stdout_of(
        clayers()
            .args(["status"])
            .current_dir(tmp.path()),
    );
    assert!(out.contains("a.xml"), "a.xml missing from status: {out}");
    assert!(out.contains("b.xml"), "b.xml missing from status: {out}");
}

#[test]
fn add_dot_stages_all_xml() {
    let tmp = TempDir::new().unwrap();
    clayers()
        .args(["init"])
        .current_dir(tmp.path())
        .assert()
        .success();
    std::fs::write(tmp.path().join("a.xml"), "<a/>").unwrap();
    std::fs::write(tmp.path().join("b.xml"), "<b/>").unwrap();
    // Non-XML should be ignored.
    std::fs::write(tmp.path().join("notes.txt"), "text").unwrap();

    clayers()
        .args(["add", "."])
        .current_dir(tmp.path())
        .assert()
        .success();

    let out = stdout_of(
        clayers()
            .args(["status"])
            .current_dir(tmp.path()),
    );
    assert!(out.contains("a.xml"), "a.xml not staged: {out}");
    assert!(out.contains("b.xml"), "b.xml not staged: {out}");
}

#[test]
fn add_nonexistent_errors() {
    let tmp = TempDir::new().unwrap();
    clayers()
        .args(["init"])
        .current_dir(tmp.path())
        .assert()
        .success();

    clayers()
        .args(["add", "ghost.xml"])
        .current_dir(tmp.path())
        .assert()
        .failure();
}

#[test]
fn add_malformed_xml_errors() {
    let tmp = TempDir::new().unwrap();
    clayers()
        .args(["init"])
        .current_dir(tmp.path())
        .assert()
        .success();
    std::fs::write(tmp.path().join("bad.xml"), "this is not xml <<<<").unwrap();

    clayers()
        .args(["add", "bad.xml"])
        .current_dir(tmp.path())
        .assert()
        .failure();
}

#[test]
fn add_modified_stages_modify() {
    let tmp = setup_committed_repo(&[("doc.xml", "<root>v1</root>")]);
    let path = tmp.path();

    // Modify the file.
    std::fs::write(path.join("doc.xml"), "<root>v2</root>").unwrap();
    clayers()
        .args(["add", "doc.xml"])
        .current_dir(path)
        .assert()
        .success();

    let out = stdout_of(clayers().args(["status"]).current_dir(path));
    assert!(
        out.contains("modify"),
        "should show modify action: {out}"
    );
}

// ===========================================================================
// status
// ===========================================================================

#[test]
fn status_no_staged_after_commit() {
    let tmp = setup_committed_repo(&[("doc.xml", "<root/>")]);
    let out = stdout_of(clayers().args(["status"]).current_dir(tmp.path()));
    assert!(
        !out.contains("Changes to be committed"),
        "staging should be clear after commit: {out}"
    );
}

#[test]
fn status_unchanged_file_not_shown_as_modified() {
    // After commit, an untouched file must NOT appear in the "not staged"
    // section. Status compares hashes to detect real modifications.
    let tmp = setup_committed_repo(&[("doc.xml", "<root>hello</root>")]);
    let out = stdout_of(clayers().args(["status"]).current_dir(tmp.path()));
    assert!(
        !out.contains("not staged"),
        "unchanged file should not show as modified: {out}"
    );
    assert!(
        out.contains("nothing to commit") || out.contains("working tree clean"),
        "should be clean: {out}"
    );
}

#[test]
fn status_staged_new_file() {
    let tmp = TempDir::new().unwrap();
    clayers()
        .args(["init"])
        .current_dir(tmp.path())
        .assert()
        .success();
    std::fs::write(tmp.path().join("doc.xml"), "<root/>").unwrap();
    clayers()
        .args(["add", "doc.xml"])
        .current_dir(tmp.path())
        .assert()
        .success();

    let out = stdout_of(clayers().args(["status"]).current_dir(tmp.path()));
    assert!(
        out.contains("Changes to be committed"),
        "should show staged section: {out}"
    );
    assert!(
        out.contains("add") && out.contains("doc.xml"),
        "should show add action: {out}"
    );
}

#[test]
fn status_shows_branch() {
    let tmp = TempDir::new().unwrap();
    clayers()
        .args(["init"])
        .current_dir(tmp.path())
        .assert()
        .success();

    let out = stdout_of(clayers().args(["status"]).current_dir(tmp.path()));
    assert!(out.contains("On branch main"), "should show branch: {out}");
}

#[test]
fn status_unstaged_modified() {
    let tmp = setup_committed_repo(&[("doc.xml", "<root>v1</root>")]);
    let path = tmp.path();

    // Modify but don't add.
    std::fs::write(path.join("doc.xml"), "<root>v2</root>").unwrap();

    let out = stdout_of(clayers().args(["status"]).current_dir(path));
    assert!(
        out.contains("not staged") || out.contains("modified"),
        "should show unstaged modification: {out}"
    );
}

// ===========================================================================
// commit
// ===========================================================================

#[test]
fn commit_creates_history() {
    let tmp = TempDir::new().unwrap();
    clayers()
        .args(["init"])
        .current_dir(tmp.path())
        .assert()
        .success();
    std::fs::write(tmp.path().join("doc.xml"), "<root/>").unwrap();
    clayers()
        .args(["add", "doc.xml"])
        .current_dir(tmp.path())
        .assert()
        .success();

    clayers()
        .args(["commit", "-m", "first commit"])
        .envs(author_env())
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("first commit"));
}

#[test]
fn commit_empty_staging_errors() {
    let tmp = TempDir::new().unwrap();
    clayers()
        .args(["init"])
        .current_dir(tmp.path())
        .assert()
        .success();

    clayers()
        .args(["commit", "-m", "empty"])
        .envs(author_env())
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("nothing to commit"));
}

#[test]
fn commit_clears_staging() {
    let tmp = setup_committed_repo(&[("doc.xml", "<root/>")]);
    let out = stdout_of(clayers().args(["status"]).current_dir(tmp.path()));
    assert!(
        !out.contains("Changes to be committed"),
        "staging should be clear after commit: {out}"
    );
}

#[test]
fn commit_multi_file_atomic() {
    let tmp = setup_committed_repo(&[
        ("a.xml", "<a>one</a>"),
        ("b.xml", "<b>two</b>"),
        ("c.xml", "<c>three</c>"),
    ]);
    let path = tmp.path();

    // All 3 files should be queryable.
    clayers()
        .args(["query", "//a", "--text"])
        .current_dir(path)
        .assert()
        .success()
        .stdout(predicates::str::contains("one"));
    clayers()
        .args(["query", "//c", "--text"])
        .current_dir(path)
        .assert()
        .success()
        .stdout(predicates::str::contains("three"));
}

#[test]
fn commit_second_preserves_first() {
    let tmp = setup_committed_repo(&[("a.xml", "<a>v1</a>")]);
    let path = tmp.path();

    // Second commit with a new file.
    std::fs::write(path.join("b.xml"), "<b>v2</b>").unwrap();
    clayers()
        .args(["add", "b.xml"])
        .current_dir(path)
        .assert()
        .success();
    clayers()
        .args(["commit", "-m", "second"])
        .envs(author_env())
        .current_dir(path)
        .assert()
        .success();

    // Both files should be in the tree.
    clayers()
        .args(["query", "//a", "--text"])
        .current_dir(path)
        .assert()
        .success()
        .stdout(predicates::str::contains("v1"));
    clayers()
        .args(["query", "//b", "--text"])
        .current_dir(path)
        .assert()
        .success()
        .stdout(predicates::str::contains("v2"));
}

#[test]
fn commit_modify_one_of_three() {
    let tmp = setup_committed_repo(&[
        ("a.xml", "<a>one</a>"),
        ("b.xml", "<b>two</b>"),
        ("c.xml", "<c>three</c>"),
    ]);
    let path = tmp.path();

    // Modify only b.xml.
    std::fs::write(path.join("b.xml"), "<b>TWO-MODIFIED</b>").unwrap();
    clayers()
        .args(["add", "b.xml"])
        .current_dir(path)
        .assert()
        .success();
    clayers()
        .args(["commit", "-m", "modify b"])
        .envs(author_env())
        .current_dir(path)
        .assert()
        .success();

    // All 3 should exist, b modified.
    clayers()
        .args(["query", "//a", "--text"])
        .current_dir(path)
        .assert()
        .success()
        .stdout(predicates::str::contains("one"));
    clayers()
        .args(["query", "//b", "--text"])
        .current_dir(path)
        .assert()
        .success()
        .stdout(predicates::str::contains("TWO-MODIFIED"));
    clayers()
        .args(["query", "//c", "--text"])
        .current_dir(path)
        .assert()
        .success()
        .stdout(predicates::str::contains("three"));
}

#[test]
fn commit_preserves_author() {
    let tmp = TempDir::new().unwrap();
    clayers()
        .args(["init"])
        .current_dir(tmp.path())
        .assert()
        .success();
    std::fs::write(tmp.path().join("doc.xml"), "<r/>").unwrap();
    clayers()
        .args(["add", "doc.xml"])
        .current_dir(tmp.path())
        .assert()
        .success();
    clayers()
        .args(["commit", "-m", "test", "--author", "Jane Doe", "--email", "jane@example.com"])
        .current_dir(tmp.path())
        .assert()
        .success();

    let out = stdout_of(clayers().args(["log"]).current_dir(tmp.path()));
    assert!(
        out.contains("Jane Doe"),
        "author should appear in log: {out}"
    );
}

// ===========================================================================
// log
// ===========================================================================

#[test]
fn log_empty_repo() {
    let tmp = TempDir::new().unwrap();
    clayers()
        .args(["init"])
        .current_dir(tmp.path())
        .assert()
        .success();

    clayers()
        .args(["log"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("no commits"));
}

#[test]
fn log_shows_message() {
    let tmp = setup_committed_repo(&[("doc.xml", "<r/>")]);
    let out = stdout_of(clayers().args(["log"]).current_dir(tmp.path()));
    assert!(
        out.contains("initial"),
        "should show commit message: {out}"
    );
}

#[test]
fn log_shows_author() {
    let tmp = setup_committed_repo(&[("doc.xml", "<r/>")]);
    let out = stdout_of(clayers().args(["log"]).current_dir(tmp.path()));
    assert!(
        out.contains("Test Author"),
        "should show author name: {out}"
    );
}

#[test]
fn log_limit() {
    let tmp = setup_committed_repo(&[("doc.xml", "<r>0</r>")]);
    let path = tmp.path();

    for i in 1..=3 {
        std::fs::write(path.join("doc.xml"), format!("<r>{i}</r>")).unwrap();
        clayers()
            .args(["add", "doc.xml"])
            .current_dir(path)
            .assert()
            .success();
        clayers()
            .args(["commit", "-m", &format!("commit-{i}")])
            .envs(author_env())
            .current_dir(path)
            .assert()
            .success();
    }

    let out = stdout_of(clayers().args(["log", "-n", "1"]).current_dir(path));
    assert!(
        out.contains("commit-3"),
        "should show latest commit: {out}"
    );
    assert!(
        !out.contains("commit-1"),
        "should NOT show older commits with -n 1: {out}"
    );
}

#[test]
fn log_order_newest_first() {
    let tmp = setup_committed_repo(&[("doc.xml", "<r/>")]);
    let path = tmp.path();

    std::fs::write(path.join("doc.xml"), "<r>v2</r>").unwrap();
    clayers()
        .args(["add", "doc.xml"])
        .current_dir(path)
        .assert()
        .success();
    clayers()
        .args(["commit", "-m", "second"])
        .envs(author_env())
        .current_dir(path)
        .assert()
        .success();

    let out = stdout_of(clayers().args(["log"]).current_dir(path));
    let pos_second = out.find("second").expect("'second' not in log");
    let pos_initial = out.find("initial").expect("'initial' not in log");
    assert!(
        pos_second < pos_initial,
        "newest commit should appear first: {out}"
    );
}

// ===========================================================================
// remote
// ===========================================================================

#[test]
fn remote_add_list_remove() {
    let tmp = TempDir::new().unwrap();
    clayers()
        .args(["init"])
        .current_dir(tmp.path())
        .assert()
        .success();

    clayers()
        .args(["remote", "add", "origin", "/tmp/some.db"])
        .current_dir(tmp.path())
        .assert()
        .success();

    let out = stdout_of(clayers().args(["remote", "list"]).current_dir(tmp.path()));
    assert!(out.contains("origin"), "origin not in list: {out}");

    clayers()
        .args(["remote", "remove", "origin"])
        .current_dir(tmp.path())
        .assert()
        .success();

    let out = stdout_of(clayers().args(["remote", "list"]).current_dir(tmp.path()));
    assert!(
        !out.contains("origin"),
        "origin should be removed: {out}"
    );
}

#[test]
fn remote_remove_nonexistent_errors() {
    let tmp = TempDir::new().unwrap();
    clayers()
        .args(["init"])
        .current_dir(tmp.path())
        .assert()
        .success();

    clayers()
        .args(["remote", "remove", "ghost"])
        .current_dir(tmp.path())
        .assert()
        .failure();
}

// ===========================================================================
// branch
// ===========================================================================

#[test]
fn branch_list_shows_main() {
    let tmp = setup_committed_repo(&[("doc.xml", "<r/>")]);
    let out = stdout_of(clayers().args(["branch"]).current_dir(tmp.path()));
    assert!(out.contains("main"), "should list main: {out}");
}

#[test]
fn branch_create_and_list() {
    let tmp = setup_committed_repo(&[("doc.xml", "<r/>")]);
    let path = tmp.path();

    clayers()
        .args(["branch", "feature"])
        .current_dir(path)
        .assert()
        .success();

    let out = stdout_of(clayers().args(["branch"]).current_dir(path));
    assert!(out.contains("feature"), "feature not listed: {out}");
    assert!(out.contains("main"), "main should still exist: {out}");
}

#[test]
fn branch_delete() {
    let tmp = setup_committed_repo(&[("doc.xml", "<r/>")]);
    let path = tmp.path();

    clayers()
        .args(["branch", "to-delete"])
        .current_dir(path)
        .assert()
        .success();
    clayers()
        .args(["branch", "--delete", "to-delete"])
        .current_dir(path)
        .assert()
        .success();

    let out = stdout_of(clayers().args(["branch"]).current_dir(path));
    assert!(
        !out.contains("to-delete"),
        "deleted branch still listed: {out}"
    );
}

#[test]
fn branch_delete_current_errors() {
    let tmp = setup_committed_repo(&[("doc.xml", "<r/>")]);
    clayers()
        .args(["branch", "--delete", "main"])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("cannot delete"));
}

// ===========================================================================
// checkout
// ===========================================================================

#[test]
fn checkout_switches_branch() {
    let tmp = setup_committed_repo(&[("doc.xml", "<r/>")]);
    let path = tmp.path();

    clayers()
        .args(["branch", "dev"])
        .current_dir(path)
        .assert()
        .success();
    clayers()
        .args(["checkout", "dev"])
        .current_dir(path)
        .assert()
        .success();

    let out = stdout_of(clayers().args(["status"]).current_dir(path));
    assert!(
        out.contains("On branch dev"),
        "should be on dev: {out}"
    );
}

#[test]
fn checkout_nonexistent_errors() {
    let tmp = setup_committed_repo(&[("doc.xml", "<r/>")]);
    clayers()
        .args(["checkout", "nonexistent"])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("not found"));
}

#[test]
fn checkout_dirty_aborts() {
    let tmp = setup_committed_repo(&[("doc.xml", "<r/>")]);
    let path = tmp.path();

    clayers()
        .args(["branch", "other"])
        .current_dir(path)
        .assert()
        .success();

    // Stage a change to make it dirty.
    std::fs::write(path.join("doc.xml"), "<r>dirty</r>").unwrap();
    clayers()
        .args(["add", "doc.xml"])
        .current_dir(path)
        .assert()
        .success();

    clayers()
        .args(["checkout", "other"])
        .current_dir(path)
        .assert()
        .failure()
        .stderr(predicates::str::contains("staged"));
}

#[test]
fn checkout_create_with_b() {
    let tmp = setup_committed_repo(&[("doc.xml", "<r/>")]);
    let path = tmp.path();

    clayers()
        .args(["checkout", "-b", "new-branch"])
        .current_dir(path)
        .assert()
        .success();

    let out = stdout_of(clayers().args(["status"]).current_dir(path));
    assert!(
        out.contains("On branch new-branch"),
        "should be on new-branch: {out}"
    );
}

#[test]
fn checkout_switches_file_content() {
    // Branch A has one version, branch B has a different version.
    // Checkout must update file content on disk.
    let tmp = setup_committed_repo(&[("doc.xml", "<root>version-A</root>")]);
    let path = tmp.path();

    // Create branch B with different content.
    clayers()
        .args(["checkout", "-b", "branchB"])
        .current_dir(path)
        .assert()
        .success();
    std::fs::write(path.join("doc.xml"), "<root>version-B</root>").unwrap();
    clayers()
        .args(["add", "doc.xml"])
        .current_dir(path)
        .assert()
        .success();
    clayers()
        .args(["commit", "-m", "B version"])
        .envs(author_env())
        .current_dir(path)
        .assert()
        .success();

    // Switch back to main.
    clayers()
        .args(["checkout", "main"])
        .current_dir(path)
        .assert()
        .success();
    let content = std::fs::read_to_string(path.join("doc.xml")).unwrap();
    assert!(
        content.contains("version-A"),
        "main should have version-A: {content}"
    );

    // Switch to branchB.
    clayers()
        .args(["checkout", "branchB"])
        .current_dir(path)
        .assert()
        .success();
    let content = std::fs::read_to_string(path.join("doc.xml")).unwrap();
    assert!(
        content.contains("version-B"),
        "branchB should have version-B: {content}"
    );
}

#[test]
fn checkout_removes_files_not_in_target_branch() {
    // main has {a.xml, b.xml}. Branch "fewer" has only {a.xml}.
    // Switching to "fewer" must remove b.xml from disk.
    let tmp = setup_committed_repo(&[
        ("a.xml", "<a>shared</a>"),
        ("b.xml", "<b>only-on-main</b>"),
    ]);
    let path = tmp.path();

    // Create "fewer" branch, remove b.xml, commit.
    clayers()
        .args(["checkout", "-b", "fewer"])
        .current_dir(path)
        .assert()
        .success();
    clayers()
        .args(["rm", "b.xml"])
        .current_dir(path)
        .assert()
        .success();
    clayers()
        .args(["commit", "-m", "remove b"])
        .envs(author_env())
        .current_dir(path)
        .assert()
        .success();

    // Switch back to main - b.xml must reappear.
    clayers()
        .args(["checkout", "main"])
        .current_dir(path)
        .assert()
        .success();
    assert!(
        path.join("b.xml").exists(),
        "b.xml should reappear on main"
    );
    let content = std::fs::read_to_string(path.join("b.xml")).unwrap();
    assert!(
        content.contains("only-on-main"),
        "b.xml should have main content: {content}"
    );

    // Switch to "fewer" - b.xml must be removed.
    clayers()
        .args(["checkout", "fewer"])
        .current_dir(path)
        .assert()
        .success();
    assert!(
        !path.join("b.xml").exists(),
        "b.xml should be removed on 'fewer' branch"
    );
    assert!(
        path.join("a.xml").exists(),
        "a.xml should still exist on 'fewer' branch"
    );
}

#[test]
fn checkout_adds_files_only_in_target_branch() {
    // main has {a.xml}. Branch "more" has {a.xml, extra.xml}.
    // Switching to "more" must create extra.xml on disk.
    let tmp = setup_committed_repo(&[("a.xml", "<a>base</a>")]);
    let path = tmp.path();

    // Create "more" branch, add extra.xml, commit.
    clayers()
        .args(["checkout", "-b", "more"])
        .current_dir(path)
        .assert()
        .success();
    std::fs::write(path.join("extra.xml"), "<extra>new-file</extra>").unwrap();
    clayers()
        .args(["add", "extra.xml"])
        .current_dir(path)
        .assert()
        .success();
    clayers()
        .args(["commit", "-m", "add extra"])
        .envs(author_env())
        .current_dir(path)
        .assert()
        .success();

    // Switch back to main - extra.xml must disappear.
    clayers()
        .args(["checkout", "main"])
        .current_dir(path)
        .assert()
        .success();
    assert!(
        !path.join("extra.xml").exists(),
        "extra.xml should not exist on main"
    );

    // Switch to "more" - extra.xml must appear.
    clayers()
        .args(["checkout", "more"])
        .current_dir(path)
        .assert()
        .success();
    assert!(
        path.join("extra.xml").exists(),
        "extra.xml should exist on 'more' branch"
    );
    let content = std::fs::read_to_string(path.join("extra.xml")).unwrap();
    assert!(
        content.contains("new-file"),
        "extra.xml should have correct content: {content}"
    );
}

#[test]
fn checkout_completely_different_file_sets() {
    // main has {a.xml, b.xml}. Branch "alt" has {c.xml, d.xml}.
    // No overlap. Switching must remove old files and add new ones.
    let tmp = setup_committed_repo(&[
        ("a.xml", "<a>alpha</a>"),
        ("b.xml", "<b>beta</b>"),
    ]);
    let path = tmp.path();

    // Create "alt" branch, remove a+b, add c+d.
    clayers()
        .args(["checkout", "-b", "alt"])
        .current_dir(path)
        .assert()
        .success();
    clayers()
        .args(["rm", "a.xml"])
        .current_dir(path)
        .assert()
        .success();
    clayers()
        .args(["rm", "b.xml"])
        .current_dir(path)
        .assert()
        .success();
    std::fs::write(path.join("c.xml"), "<c>gamma</c>").unwrap();
    std::fs::write(path.join("d.xml"), "<d>delta</d>").unwrap();
    clayers()
        .args(["add", "c.xml", "d.xml"])
        .current_dir(path)
        .assert()
        .success();
    clayers()
        .args(["commit", "-m", "alt files"])
        .envs(author_env())
        .current_dir(path)
        .assert()
        .success();

    // Verify alt state: c+d exist, a+b don't.
    assert!(!path.join("a.xml").exists());
    assert!(!path.join("b.xml").exists());
    assert!(path.join("c.xml").exists());
    assert!(path.join("d.xml").exists());

    // Switch to main: a+b reappear, c+d removed.
    clayers()
        .args(["checkout", "main"])
        .current_dir(path)
        .assert()
        .success();
    assert!(path.join("a.xml").exists(), "a.xml should exist on main");
    assert!(path.join("b.xml").exists(), "b.xml should exist on main");
    assert!(
        !path.join("c.xml").exists(),
        "c.xml should not exist on main"
    );
    assert!(
        !path.join("d.xml").exists(),
        "d.xml should not exist on main"
    );

    // Switch back to alt: c+d reappear, a+b removed.
    clayers()
        .args(["checkout", "alt"])
        .current_dir(path)
        .assert()
        .success();
    assert!(
        !path.join("a.xml").exists(),
        "a.xml should not exist on alt"
    );
    assert!(
        !path.join("b.xml").exists(),
        "b.xml should not exist on alt"
    );
    assert!(path.join("c.xml").exists(), "c.xml should exist on alt");
    assert!(path.join("d.xml").exists(), "d.xml should exist on alt");
}

#[test]
fn checkout_status_clean_after_switch() {
    // After switching branches, status should be clean (no false modifications).
    let tmp = setup_committed_repo(&[
        ("a.xml", "<a>one</a>"),
        ("b.xml", "<b>two</b>"),
    ]);
    let path = tmp.path();

    clayers()
        .args(["checkout", "-b", "other"])
        .current_dir(path)
        .assert()
        .success();

    let out = stdout_of(clayers().args(["status"]).current_dir(path));
    assert!(
        !out.contains("not staged"),
        "status should be clean after checkout -b: {out}"
    );
}

#[test]
fn checkout_orphan_clears_working_directory() {
    // Orphan branch starts with an empty tree. All tracked files must be
    // removed from disk.
    let tmp = setup_committed_repo(&[
        ("a.xml", "<a>alpha</a>"),
        ("b.xml", "<b>beta</b>"),
    ]);
    let path = tmp.path();

    clayers()
        .args(["checkout", "--orphan", "clean-start"])
        .current_dir(path)
        .assert()
        .success();

    assert!(
        !path.join("a.xml").exists(),
        "a.xml should be removed on orphan branch"
    );
    assert!(
        !path.join("b.xml").exists(),
        "b.xml should be removed on orphan branch"
    );

    let out = stdout_of(clayers().args(["status"]).current_dir(path));
    assert!(
        out.contains("On branch clean-start"),
        "should be on orphan branch: {out}"
    );
}

#[test]
fn checkout_orphan_commit_has_no_parents() {
    // Commits on an orphan branch are root commits (no parent).
    let tmp = setup_committed_repo(&[("doc.xml", "<root>original</root>")]);
    let path = tmp.path();

    clayers()
        .args(["checkout", "--orphan", "fresh"])
        .current_dir(path)
        .assert()
        .success();

    // Add a new file and commit on the orphan branch.
    std::fs::write(path.join("new.xml"), "<new>orphan-content</new>").unwrap();
    clayers()
        .args(["add", "new.xml"])
        .current_dir(path)
        .assert()
        .success();
    clayers()
        .args(["commit", "-m", "orphan commit"])
        .envs(author_env())
        .current_dir(path)
        .assert()
        .success();

    // Log should show only the orphan commit, not the main branch history.
    let out = stdout_of(clayers().args(["log"]).current_dir(path));
    assert!(
        out.contains("orphan commit"),
        "should show orphan commit: {out}"
    );
    assert!(
        !out.contains("initial"),
        "should NOT show main branch history: {out}"
    );
}

#[test]
fn checkout_orphan_then_switch_back() {
    // After creating an orphan branch, switching back to main must restore
    // the original files.
    let tmp = setup_committed_repo(&[("doc.xml", "<root>main-data</root>")]);
    let path = tmp.path();

    clayers()
        .args(["checkout", "--orphan", "empty"])
        .current_dir(path)
        .assert()
        .success();

    assert!(
        !path.join("doc.xml").exists(),
        "doc.xml should be gone on orphan"
    );

    // Switch back to main.
    clayers()
        .args(["checkout", "main"])
        .current_dir(path)
        .assert()
        .success();

    assert!(
        path.join("doc.xml").exists(),
        "doc.xml should reappear on main"
    );
    let content = std::fs::read_to_string(path.join("doc.xml")).unwrap();
    assert!(
        content.contains("main-data"),
        "doc.xml should have main content: {content}"
    );
}

#[test]
fn checkout_orphan_independent_history() {
    // Two branches with independent histories can coexist.
    let tmp = setup_committed_repo(&[("main.xml", "<main>data</main>")]);
    let path = tmp.path();

    // Create orphan and commit different files.
    clayers()
        .args(["checkout", "--orphan", "independent"])
        .current_dir(path)
        .assert()
        .success();
    std::fs::write(path.join("indie.xml"), "<indie>separate</indie>").unwrap();
    clayers()
        .args(["add", "indie.xml"])
        .current_dir(path)
        .assert()
        .success();
    clayers()
        .args(["commit", "-m", "indie commit"])
        .envs(author_env())
        .current_dir(path)
        .assert()
        .success();

    // independent has {indie.xml}, main has {main.xml}. No overlap.
    assert!(path.join("indie.xml").exists());
    assert!(!path.join("main.xml").exists());

    // Switch to main.
    clayers()
        .args(["checkout", "main"])
        .current_dir(path)
        .assert()
        .success();
    assert!(path.join("main.xml").exists());
    assert!(!path.join("indie.xml").exists());

    // Switch back to independent.
    clayers()
        .args(["checkout", "independent"])
        .current_dir(path)
        .assert()
        .success();
    assert!(path.join("indie.xml").exists());
    assert!(!path.join("main.xml").exists());
}

// ===========================================================================
// push / pull
// ===========================================================================

#[test]
fn push_to_bare() {
    let tmp = setup_committed_repo(&[("doc.xml", "<root>hello</root>")]);
    let path = tmp.path();
    let bare = tmp.path().join("bare.db");

    clayers()
        .args(["init", "--bare", bare.to_str().unwrap()])
        .assert()
        .success();
    clayers()
        .args(["remote", "add", "origin", bare.to_str().unwrap()])
        .current_dir(path)
        .assert()
        .success();
    clayers()
        .args(["push", "origin"])
        .current_dir(path)
        .assert()
        .success();
}

#[test]
fn push_no_remote_errors() {
    let tmp = setup_committed_repo(&[("doc.xml", "<r/>")]);
    clayers()
        .args(["push", "nonexistent"])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("not found"));
}

#[test]
fn pull_gets_new_commits() {
    let tmp = TempDir::new().unwrap();
    let src = tmp.path().join("src");
    let dst = tmp.path().join("dst");
    let bare = tmp.path().join("bare.db");

    // Set up source.
    std::fs::create_dir_all(&src).unwrap();
    clayers().args(["init"]).current_dir(&src).assert().success();
    std::fs::write(src.join("doc.xml"), "<root>hello</root>").unwrap();
    clayers()
        .args(["add", "doc.xml"])
        .current_dir(&src)
        .assert()
        .success();
    clayers()
        .args(["commit", "-m", "src-commit"])
        .envs(author_env())
        .current_dir(&src)
        .assert()
        .success();

    // Push to bare.
    clayers()
        .args(["init", "--bare", bare.to_str().unwrap()])
        .assert()
        .success();
    clayers()
        .args(["remote", "add", "origin", bare.to_str().unwrap()])
        .current_dir(&src)
        .assert()
        .success();
    clayers()
        .args(["push", "origin"])
        .current_dir(&src)
        .assert()
        .success();

    // Set up destination.
    std::fs::create_dir_all(&dst).unwrap();
    clayers().args(["init"]).current_dir(&dst).assert().success();
    clayers()
        .args(["remote", "add", "origin", bare.to_str().unwrap()])
        .current_dir(&dst)
        .assert()
        .success();

    // Pull.
    clayers()
        .args(["pull", "origin"])
        .current_dir(&dst)
        .assert()
        .success();

    // Verify: log should show the commit AND doc.xml on disk.
    let out = stdout_of(clayers().args(["log"]).current_dir(&dst));
    assert!(out.contains("src-commit"), "pull should get commit: {out}");
    assert!(
        dst.join("doc.xml").exists(),
        "pull should export doc.xml to disk"
    );
}

#[test]
fn push_pull_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let src = tmp.path().join("src");
    let bare = tmp.path().join("bare.db");
    let clone_dir = tmp.path().join("cloned");

    std::fs::create_dir_all(&src).unwrap();
    clayers().args(["init"]).current_dir(&src).assert().success();
    std::fs::write(src.join("doc.xml"), "<root>data</root>").unwrap();
    clayers()
        .args(["add", "doc.xml"])
        .current_dir(&src)
        .assert()
        .success();
    clayers()
        .args(["commit", "-m", "init"])
        .envs(author_env())
        .current_dir(&src)
        .assert()
        .success();

    clayers()
        .args(["init", "--bare", bare.to_str().unwrap()])
        .assert()
        .success();
    clayers()
        .args(["remote", "add", "origin", bare.to_str().unwrap()])
        .current_dir(&src)
        .assert()
        .success();
    clayers()
        .args(["push", "origin"])
        .current_dir(&src)
        .assert()
        .success();

    clayers()
        .args(["clone", bare.to_str().unwrap(), clone_dir.to_str().unwrap()])
        .assert()
        .success();

    // Verify content survived the roundtrip.
    let content = std::fs::read_to_string(clone_dir.join("doc.xml")).unwrap();
    assert!(content.contains("data"), "content not in clone: {content}");
}

// ===========================================================================
// clone
// ===========================================================================

#[test]
fn clone_creates_working_copy() {
    let tmp = TempDir::new().unwrap();
    let src = tmp.path().join("src");
    let bare = tmp.path().join("bare.db");
    let cloned = tmp.path().join("cloned");

    std::fs::create_dir_all(&src).unwrap();
    clayers().args(["init"]).current_dir(&src).assert().success();
    std::fs::write(src.join("doc.xml"), "<root>hello</root>").unwrap();
    clayers()
        .args(["add", "doc.xml"])
        .current_dir(&src)
        .assert()
        .success();
    clayers()
        .args(["commit", "-m", "init"])
        .envs(author_env())
        .current_dir(&src)
        .assert()
        .success();

    clayers()
        .args(["init", "--bare", bare.to_str().unwrap()])
        .assert()
        .success();
    clayers()
        .args(["remote", "add", "origin", bare.to_str().unwrap()])
        .current_dir(&src)
        .assert()
        .success();
    clayers()
        .args(["push", "origin"])
        .current_dir(&src)
        .assert()
        .success();

    clayers()
        .args(["clone", bare.to_str().unwrap(), cloned.to_str().unwrap()])
        .assert()
        .success();

    assert!(cloned.join(".clayers.db").exists(), ".clayers.db missing");
    assert!(cloned.join("doc.xml").exists(), "doc.xml missing");
}

#[test]
fn clone_preserves_history() {
    let tmp = TempDir::new().unwrap();
    let src = tmp.path().join("src");
    let bare = tmp.path().join("bare.db");
    let cloned = tmp.path().join("cloned");

    std::fs::create_dir_all(&src).unwrap();
    clayers().args(["init"]).current_dir(&src).assert().success();
    std::fs::write(src.join("doc.xml"), "<r/>").unwrap();
    clayers()
        .args(["add", "doc.xml"])
        .current_dir(&src)
        .assert()
        .success();
    clayers()
        .args(["commit", "-m", "the-message"])
        .envs(author_env())
        .current_dir(&src)
        .assert()
        .success();

    clayers()
        .args(["init", "--bare", bare.to_str().unwrap()])
        .assert()
        .success();
    clayers()
        .args(["remote", "add", "origin", bare.to_str().unwrap()])
        .current_dir(&src)
        .assert()
        .success();
    clayers()
        .args(["push", "origin"])
        .current_dir(&src)
        .assert()
        .success();

    clayers()
        .args(["clone", bare.to_str().unwrap(), cloned.to_str().unwrap()])
        .assert()
        .success();

    let out = stdout_of(clayers().args(["log"]).current_dir(&cloned));
    assert!(
        out.contains("the-message"),
        "clone log should preserve history: {out}"
    );
}

#[test]
fn clone_sets_origin() {
    let tmp = TempDir::new().unwrap();
    let src = tmp.path().join("src");
    let bare = tmp.path().join("bare.db");
    let cloned = tmp.path().join("cloned");

    std::fs::create_dir_all(&src).unwrap();
    clayers().args(["init"]).current_dir(&src).assert().success();
    std::fs::write(src.join("doc.xml"), "<r/>").unwrap();
    clayers()
        .args(["add", "."])
        .current_dir(&src)
        .assert()
        .success();
    clayers()
        .args(["commit", "-m", "init"])
        .envs(author_env())
        .current_dir(&src)
        .assert()
        .success();
    clayers()
        .args(["init", "--bare", bare.to_str().unwrap()])
        .assert()
        .success();
    clayers()
        .args(["remote", "add", "origin", bare.to_str().unwrap()])
        .current_dir(&src)
        .assert()
        .success();
    clayers()
        .args(["push", "origin"])
        .current_dir(&src)
        .assert()
        .success();

    clayers()
        .args(["clone", bare.to_str().unwrap(), cloned.to_str().unwrap()])
        .assert()
        .success();

    let out = stdout_of(clayers().args(["remote", "list"]).current_dir(&cloned));
    assert!(out.contains("origin"), "clone should set origin: {out}");
}

#[test]
fn clone_nonexistent_errors() {
    clayers()
        .args(["clone", "/nonexistent/path.db", "/tmp/somewhere"])
        .assert()
        .failure();
}

// ===========================================================================
// revert
// ===========================================================================

#[test]
fn revert_restores_file() {
    let tmp = setup_committed_repo(&[("doc.xml", "<root>original</root>")]);
    let path = tmp.path();

    std::fs::write(path.join("doc.xml"), "<root>modified</root>").unwrap();
    clayers()
        .args(["add", "doc.xml"])
        .current_dir(path)
        .assert()
        .success();

    clayers()
        .args(["revert", "doc.xml"])
        .current_dir(path)
        .assert()
        .success();

    let content = std::fs::read_to_string(path.join("doc.xml")).unwrap();
    assert!(
        content.contains("original"),
        "file should be reverted: {content}"
    );
}

#[test]
fn revert_clears_staging() {
    let tmp = setup_committed_repo(&[("doc.xml", "<root>original</root>")]);
    let path = tmp.path();

    std::fs::write(path.join("doc.xml"), "<root>modified</root>").unwrap();
    clayers()
        .args(["add", "doc.xml"])
        .current_dir(path)
        .assert()
        .success();
    clayers()
        .args(["revert", "doc.xml"])
        .current_dir(path)
        .assert()
        .success();

    let out = stdout_of(clayers().args(["status"]).current_dir(path));
    assert!(
        !out.contains("Changes to be committed"),
        "staging should be cleared after revert: {out}"
    );
}

#[test]
fn revert_untracked_warns() {
    let tmp = setup_committed_repo(&[("doc.xml", "<r/>")]);
    let out = stdout_of(
        clayers()
            .args(["revert", "nonexistent.xml"])
            .current_dir(tmp.path()),
    );
    assert!(
        out.contains("not tracked") || out.contains("skipped") || out.contains("warning"),
        "should warn about untracked file: {out}"
    );
}

// ===========================================================================
// rm
// ===========================================================================

#[test]
fn rm_cached_unstages() {
    let tmp = TempDir::new().unwrap();
    clayers()
        .args(["init"])
        .current_dir(tmp.path())
        .assert()
        .success();
    std::fs::write(tmp.path().join("doc.xml"), "<r/>").unwrap();
    clayers()
        .args(["add", "doc.xml"])
        .current_dir(tmp.path())
        .assert()
        .success();

    clayers()
        .args(["rm", "--cached", "doc.xml"])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Should no longer be staged.
    let out = stdout_of(clayers().args(["status"]).current_dir(tmp.path()));
    assert!(
        !out.contains("Changes to be committed"),
        "should be unstaged after rm --cached: {out}"
    );
}

#[test]
fn rm_deletes_tracked() {
    let tmp = setup_committed_repo(&[("doc.xml", "<r/>")]);
    let path = tmp.path();

    clayers()
        .args(["rm", "doc.xml"])
        .current_dir(path)
        .assert()
        .success();

    assert!(
        !path.join("doc.xml").exists(),
        "file should be deleted from disk"
    );

    let out = stdout_of(clayers().args(["status"]).current_dir(path));
    assert!(
        out.contains("delete"),
        "should show delete in staging: {out}"
    );
}

// ===========================================================================
// query
// ===========================================================================

#[test]
fn query_current_branch() {
    let tmp = setup_committed_repo(&[("doc.xml", "<root><item>hello</item></root>")]);
    clayers()
        .args(["query", "//item", "--text"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("hello"));
}

#[test]
fn query_count_mode() {
    let tmp = setup_committed_repo(&[
        ("a.xml", "<root><item>1</item><item>2</item></root>"),
        ("b.xml", "<root><item>3</item></root>"),
    ]);
    clayers()
        .args(["query", "//item", "--count"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains('3'));
}

#[test]
fn query_multi_doc_tree() {
    let tmp = setup_committed_repo(&[
        ("a.xml", "<root><val>alpha</val></root>"),
        ("b.xml", "<root><val>beta</val></root>"),
    ]);
    let out = stdout_of(
        clayers()
            .args(["query", "//val", "--text"])
            .current_dir(tmp.path()),
    );
    assert!(out.contains("alpha"), "should find alpha: {out}");
    assert!(out.contains("beta"), "should find beta: {out}");
}

#[test]
fn query_no_repo_errors() {
    let tmp = TempDir::new().unwrap();
    // No init, no .clayers.db.
    clayers()
        .args(["query", "//item", "--text"])
        .current_dir(tmp.path())
        .assert()
        .failure();
}

#[test]
fn query_spec_fallback() {
    // When given a directory path, query should try spec mode.
    let tmp = TempDir::new().unwrap();
    let spec_dir = tmp.path().join("spec");
    std::fs::create_dir_all(&spec_dir).unwrap();

    // Should fail gracefully (not a valid spec) but not panic.
    let err = stderr_of(
        clayers()
            .args(["query", spec_dir.to_str().unwrap(), "//test"]),
    );
    assert!(
        !err.contains("panic"),
        "should not panic on spec fallback: {err}"
    );
}

// ===========================================================================
// End-to-end workflows
// ===========================================================================

#[test]
fn full_workflow() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path();
    let bare = path.join("bare.db");
    let clone_dir = path.join("cloned");

    // 1. Init.
    clayers().args(["init"]).current_dir(path).assert().success();

    // 2. Write, add, commit.
    std::fs::write(path.join("a.xml"), "<a><child>one</child></a>").unwrap();
    std::fs::write(path.join("b.xml"), "<b><child>two</child></b>").unwrap();
    clayers()
        .args(["add", "."])
        .current_dir(path)
        .assert()
        .success();
    clayers()
        .args(["commit", "-m", "initial"])
        .envs(author_env())
        .current_dir(path)
        .assert()
        .success();

    // 3. Modify one file, commit again.
    std::fs::write(path.join("a.xml"), "<a><child>ONE-UPDATED</child></a>").unwrap();
    clayers()
        .args(["add", "a.xml"])
        .current_dir(path)
        .assert()
        .success();
    clayers()
        .args(["commit", "-m", "update a"])
        .envs(author_env())
        .current_dir(path)
        .assert()
        .success();

    // 4. Push to bare.
    clayers()
        .args(["init", "--bare", bare.to_str().unwrap()])
        .assert()
        .success();
    clayers()
        .args(["remote", "add", "origin", bare.to_str().unwrap()])
        .current_dir(path)
        .assert()
        .success();
    clayers()
        .args(["push", "origin"])
        .current_dir(path)
        .assert()
        .success();

    // 5. Clone.
    clayers()
        .args(["clone", bare.to_str().unwrap(), clone_dir.to_str().unwrap()])
        .assert()
        .success();

    // 6. Verify cloned data.
    clayers()
        .args(["query", "//child", "--text"])
        .current_dir(&clone_dir)
        .assert()
        .success()
        .stdout(predicates::str::contains("ONE-UPDATED"))
        .stdout(predicates::str::contains("two"));

    let out = stdout_of(clayers().args(["log"]).current_dir(&clone_dir));
    assert!(out.contains("update a"), "should have second commit: {out}");
    assert!(out.contains("initial"), "should have first commit: {out}");
}

#[test]
fn empty_xml_roundtrip() {
    let tmp = setup_committed_repo(&[("doc.xml", "<root/>")]);
    clayers()
        .args(["query", "//root", "--count"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains('1'));
}

#[test]
fn special_chars_in_content() {
    let tmp = setup_committed_repo(&[(
        "doc.xml",
        "<root><text>less &lt; greater &gt; ampersand &amp; quote &quot;</text></root>",
    )]);
    clayers()
        .args(["query", "//text", "--text"])
        .current_dir(tmp.path())
        .assert()
        .success();
}

#[test]
fn xml_with_namespaces() {
    let tmp = setup_committed_repo(&[(
        "doc.xml",
        r#"<root xmlns:app="urn:test"><app:item>namespaced</app:item></root>"#,
    )]);
    clayers()
        .args(["query", "//root", "--count"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains('1'));
}

#[test]
fn checkout_preserves_multi_namespace_xml() {
    // Exercises the exact pattern that caused "Duplicate attribute: xmlns:ns2"
    // during checkout: an element whose attributes share the element's namespace.
    let tmp = TempDir::new().unwrap();
    let path = tmp.path();
    clayers().args(["init"]).current_dir(path).assert().success();

    let xml = r#"<spec:clayers xmlns:spec="urn:clayers:spec" xmlns:pr="urn:clayers:prose" spec:index="index.xml"><pr:section id="s1"><pr:title>Hello</pr:title></pr:section></spec:clayers>"#;
    std::fs::write(path.join("doc.xml"), xml).unwrap();
    clayers()
        .args(["add", "doc.xml"])
        .current_dir(path)
        .assert()
        .success();
    clayers()
        .args(["commit", "-m", "init"])
        .envs(author_env())
        .current_dir(path)
        .assert()
        .success();

    // Checkout -b triggers export_working_copy which must not produce
    // duplicate xmlns declarations.
    clayers()
        .args(["checkout", "-b", "test"])
        .current_dir(path)
        .assert()
        .success();

    // Status should be clean (no false "modified").
    let out = stdout_of(clayers().args(["status"]).current_dir(path));
    assert!(
        !out.contains("not staged"),
        "checkout should not produce false modifications: {out}"
    );
}

#[test]
fn xml_with_comments_and_pis() {
    let tmp = setup_committed_repo(&[(
        "doc.xml",
        "<?xml-stylesheet type=\"text/xsl\"?><root><!-- comment -->text</root>",
    )]);
    clayers()
        .args(["query", "//root", "--text"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("text"));
}

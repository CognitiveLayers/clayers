use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::embedded;

const MARKER_OPEN: &str = "<!-- clayers:adopt -->";
const MARKER_CLOSE: &str = "<!-- /clayers:adopt -->";

const WORKFLOW_CONTENT: &str = r"## Clayers Development Workflow

This project uses [clayers](https://github.com/inferaldata/clayers) for
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

See [clayers documentation](https://github.com/inferaldata/clayers) for
the full layer reference (prose, terminology, organization, relation,
decision, source, plan, artifact, llm, revision).
";

/// Check if a project has already been adopted.
///
/// Returns `true` if `.clayers/schemas/` exists with XSD files or the
/// agent file contains the adopt markers.
fn is_adopted(target: &Path) -> bool {
    let schemas_dir = target.join(".clayers").join("schemas");
    if schemas_dir.is_dir() {
        let has_xsd = fs::read_dir(&schemas_dir)
            .ok()
            .is_some_and(|entries| {
                entries
                    .filter_map(Result::ok)
                    .any(|e| e.path().extension().is_some_and(|ext| ext == "xsd"))
            });
        if has_xsd {
            return true;
        }
    }

    for name in &["CLAUDE.md", "AGENTS.md"] {
        let path = target.join(name);
        if path.is_file()
            && let Ok(content) = fs::read_to_string(&path)
            && content.contains(MARKER_OPEN)
        {
            return true;
        }
    }

    false
}

/// Compare embedded schemas against planted ones and report freshness.
fn check_freshness(target: &Path) -> Vec<FreshnessItem> {
    let mut items = Vec::new();
    let schemas_dir = target.join(".clayers").join("schemas");

    // Check each embedded XSD
    for &(name, content) in embedded::SCHEMAS {
        let path = schemas_dir.join(name);
        let status = if !path.exists() {
            FreshnessStatus::Missing
        } else if let Ok(existing) = fs::read_to_string(&path) {
            if existing == content {
                FreshnessStatus::Current
            } else {
                FreshnessStatus::Outdated
            }
        } else {
            FreshnessStatus::Outdated
        };
        items.push(FreshnessItem {
            path: format!(".clayers/schemas/{name}"),
            status,
        });
    }

    // Check catalog.xml
    let catalog_path = schemas_dir.join("catalog.xml");
    let status = if !catalog_path.exists() {
        FreshnessStatus::Missing
    } else if let Ok(existing) = fs::read_to_string(&catalog_path) {
        if existing == embedded::CATALOG {
            FreshnessStatus::Current
        } else {
            FreshnessStatus::Outdated
        }
    } else {
        FreshnessStatus::Outdated
    };
    items.push(FreshnessItem {
        path: ".clayers/schemas/catalog.xml".into(),
        status,
    });

    // Check postprocess.xslt
    let xslt_path = schemas_dir.join("postprocess.xslt");
    let status = if !xslt_path.exists() {
        FreshnessStatus::Missing
    } else if let Ok(existing) = fs::read_to_string(&xslt_path) {
        if existing == embedded::POSTPROCESS_XSLT {
            FreshnessStatus::Current
        } else {
            FreshnessStatus::Outdated
        }
    } else {
        FreshnessStatus::Outdated
    };
    items.push(FreshnessItem {
        path: ".clayers/schemas/postprocess.xslt".into(),
        status,
    });

    // Check agent file instructions
    let agent_content = find_agent_file(target).and_then(|(_, content)| {
        extract_between_markers(&content).map(std::string::ToString::to_string)
    });
    let status = match agent_content {
        None => FreshnessStatus::Missing,
        Some(existing) if existing.trim() == WORKFLOW_CONTENT.trim() => FreshnessStatus::Current,
        Some(_) => FreshnessStatus::Outdated,
    };
    let agent_name = if target.join("CLAUDE.md").is_file() {
        "CLAUDE.md"
    } else {
        "AGENTS.md"
    };
    items.push(FreshnessItem {
        path: format!("{agent_name} instructions"),
        status,
    });

    items
}

fn find_agent_file(target: &Path) -> Option<(std::path::PathBuf, String)> {
    for name in &["CLAUDE.md", "AGENTS.md"] {
        let path = target.join(name);
        if path.is_file()
            && let Ok(content) = fs::read_to_string(&path)
        {
            return Some((path, content));
        }
    }
    None
}

fn extract_between_markers(content: &str) -> Option<&str> {
    let start = content.find(MARKER_OPEN)?;
    let after_open = start + MARKER_OPEN.len();
    let end = content[after_open..].find(MARKER_CLOSE)?;
    let inner = &content[after_open..after_open + end];
    // Strip leading newline if present
    Some(inner.strip_prefix('\n').unwrap_or(inner))
}

#[derive(Debug, PartialEq, Eq)]
enum FreshnessStatus {
    Current,
    Outdated,
    Missing,
}

struct FreshnessItem {
    path: String,
    status: FreshnessStatus,
}

/// Plant all embedded schemas into `<target>/.clayers/schemas/`.
fn plant_schemas(target: &Path) -> Result<()> {
    let schemas_dir = target.join(".clayers").join("schemas");
    fs::create_dir_all(&schemas_dir).context("failed to create .clayers/schemas/")?;

    for &(name, content) in embedded::SCHEMAS {
        fs::write(schemas_dir.join(name), content)
            .with_context(|| format!("failed to write {name}"))?;
    }

    fs::write(schemas_dir.join("catalog.xml"), embedded::CATALOG)
        .context("failed to write catalog.xml")?;
    fs::write(schemas_dir.join("postprocess.xslt"), embedded::POSTPROCESS_XSLT)
        .context("failed to write postprocess.xslt")?;

    Ok(())
}

/// Create a starter spec in `<target>/clayers/<project>/`.
fn create_starter_spec(target: &Path) -> Result<()> {
    let clayers_dir = target.join("clayers");
    if clayers_dir.exists() {
        println!("  skipped: clayers/ already exists");
        return Ok(());
    }

    let project_name = target
        .file_name()
        .map_or_else(|| "project".into(), |n| n.to_string_lossy().into_owned());

    let spec_dir = clayers_dir.join(&project_name);
    fs::create_dir_all(&spec_dir).context("failed to create spec directory")?;

    let index_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!--
  Index Layer: File manifest for the {project_name} specification.
-->
<spec:clayers xmlns:spec="urn:clayers:spec"
       xmlns="urn:clayers:index"
       spec:spec="{project_name}"
       spec:version="0.1.0">

  <file href="revision.xml" layer="urn:clayers:revision"/>
</spec:clayers>
"#
    );

    let revision_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!--
  Revision Layer: Named snapshots of the {project_name} specification.
-->
<spec:clayers xmlns:spec="urn:clayers:spec"
           xmlns="urn:clayers:revision"
           spec:spec="{project_name}">

  <revision name="draft-1"
            timestamp="1970-01-01T00:00:00Z"
            index="index.xml"
            index-hash="sha256:placeholder">
    <note>Initial specification.</note>
  </revision>
</spec:clayers>
"#
    );

    fs::write(spec_dir.join("index.xml"), index_xml).context("failed to write index.xml")?;
    fs::write(spec_dir.join("revision.xml"), revision_xml)
        .context("failed to write revision.xml")?;

    println!("  created: clayers/{project_name}/index.xml");
    println!("  created: clayers/{project_name}/revision.xml");

    Ok(())
}

/// Amend an agent file with the clayers workflow section.
fn amend_agent_file(target: &Path) -> Result<()> {
    let marked_content = format!("{MARKER_OPEN}\n{WORKFLOW_CONTENT}{MARKER_CLOSE}\n");

    if let Some((path, content)) = find_agent_file(target) {
        if content.contains(MARKER_OPEN) && content.contains(MARKER_CLOSE) {
            // Replace content between markers
            let start = content
                .find(MARKER_OPEN)
                .expect("marker already checked");
            let after_close = content
                .find(MARKER_CLOSE)
                .expect("marker already checked")
                + MARKER_CLOSE.len();
            // Include trailing newline if present
            let end = if content[after_close..].starts_with('\n') {
                after_close + 1
            } else {
                after_close
            };
            let new_content =
                format!("{}{}{}", &content[..start], marked_content, &content[end..]);
            fs::write(&path, new_content).context("failed to update agent file")?;
            println!(
                "  updated: {} (replaced between markers)",
                path.file_name().unwrap().to_string_lossy()
            );
        } else {
            // Append to existing file
            let new_content = format!("{}\n{marked_content}", content.trim_end(),);
            fs::write(&path, new_content).context("failed to append to agent file")?;
            println!(
                "  updated: {} (appended workflow)",
                path.file_name().unwrap().to_string_lossy()
            );
        }
    } else {
        // Create AGENTS.md
        let path = target.join("AGENTS.md");
        fs::write(&path, &marked_content).context("failed to create AGENTS.md")?;
        println!("  created: AGENTS.md");
    }

    Ok(())
}

/// Update outdated schemas and agent instructions.
fn update_adopted(target: &Path, items: &[FreshnessItem]) -> Result<()> {
    let schemas_dir = target.join(".clayers").join("schemas");
    fs::create_dir_all(&schemas_dir).context("failed to create .clayers/schemas/")?;

    for item in items {
        if item.status == FreshnessStatus::Current {
            continue;
        }
        if item.path.ends_with("instructions") {
            amend_agent_file(target)?;
        } else if let Some(filename) = item.path.strip_prefix(".clayers/schemas/") {
            if filename == "catalog.xml" {
                fs::write(schemas_dir.join("catalog.xml"), embedded::CATALOG)
                    .context("failed to update catalog.xml")?;
            } else if filename == "postprocess.xslt" {
                fs::write(schemas_dir.join("postprocess.xslt"), embedded::POSTPROCESS_XSLT)
                    .context("failed to update postprocess.xslt")?;
            } else {
                // Find in embedded schemas
                for &(name, content) in embedded::SCHEMAS {
                    if name == filename {
                        fs::write(schemas_dir.join(name), content)
                            .with_context(|| format!("failed to update {name}"))?;
                        break;
                    }
                }
            }
            println!("  updated: {}", item.path);
        }
    }

    Ok(())
}

pub fn adopt(target: &Path, update: bool) -> Result<()> {
    let target = target
        .canonicalize()
        .with_context(|| format!("target path does not exist: {}", target.display()))?;

    if is_adopted(&target) {
        println!("clayers: project already adopted, checking freshness...");
        let items = check_freshness(&target);

        let any_outdated = items
            .iter()
            .any(|i| i.status != FreshnessStatus::Current);

        for item in &items {
            let label = match item.status {
                FreshnessStatus::Current => "current",
                FreshnessStatus::Outdated => "outdated",
                FreshnessStatus::Missing => "missing",
            };
            println!("  {}: {label}", item.path);
        }

        if any_outdated {
            if update {
                println!();
                println!("clayers: updating outdated components...");
                update_adopted(&target, &items)?;
                println!("clayers: update complete");
            } else {
                println!();
                bail!("project already adopted; run with --update to update outdated components");
            }
        } else if update {
            println!();
            println!("clayers: everything is current");
        } else {
            println!();
            bail!("project already adopted and everything is current");
        }

        return Ok(());
    }

    println!("clayers: adopting project at {}", target.display());

    // 1. Plant schemas
    plant_schemas(&target)?;
    println!(
        "  planted: .clayers/schemas/ ({} XSD + catalog.xml + postprocess.xslt)",
        embedded::SCHEMAS.len()
    );

    // 2. Create starter spec
    create_starter_spec(&target)?;

    // 3. Amend agent file
    amend_agent_file(&target)?;

    println!();
    println!("clayers: adoption complete");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn temp_dir() -> TempDir {
        TempDir::new().unwrap()
    }

    #[test]
    fn test_fresh_adopt_creates_schemas() {
        let dir = temp_dir();
        adopt(dir.path(), false).unwrap();

        let schemas = dir.path().join(".clayers").join("schemas");
        assert!(schemas.is_dir());
        for &(name, _) in embedded::SCHEMAS {
            assert!(schemas.join(name).exists(), "{name} missing");
        }
        assert!(schemas.join("catalog.xml").exists());
        assert!(schemas.join("postprocess.xslt").exists());
    }

    #[test]
    fn test_fresh_adopt_creates_starter_spec() {
        let dir = temp_dir();
        adopt(dir.path(), false).unwrap();

        let project_name = dir
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let spec_dir = dir.path().join("clayers").join(&project_name);
        assert!(spec_dir.join("index.xml").exists());
        assert!(spec_dir.join("revision.xml").exists());
    }

    #[test]
    fn test_fresh_adopt_creates_agents_md() {
        let dir = temp_dir();
        adopt(dir.path(), false).unwrap();

        let agents = dir.path().join("AGENTS.md");
        assert!(agents.exists());
        let content = fs::read_to_string(&agents).unwrap();
        assert!(content.contains(MARKER_OPEN));
        assert!(content.contains(MARKER_CLOSE));
        assert!(content.contains("Clayers Development Workflow"));
    }

    #[test]
    fn test_adopt_prefers_claude_md() {
        let dir = temp_dir();
        fs::write(dir.path().join("CLAUDE.md"), "# Project\n").unwrap();
        adopt(dir.path(), false).unwrap();

        let claude = fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
        assert!(claude.contains(MARKER_OPEN));
        assert!(claude.contains("# Project"));
        assert!(!dir.path().join("AGENTS.md").exists());
    }

    #[test]
    fn test_detects_already_adopted() {
        let dir = temp_dir();
        adopt(dir.path(), false).unwrap();

        // Second adopt without --update should fail
        let result = adopt(dir.path(), false);
        assert!(result.is_err());
    }

    #[test]
    fn test_update_replaces_markers() {
        let dir = temp_dir();
        adopt(dir.path(), false).unwrap();

        // Tamper with agent file
        let agents = dir.path().join("AGENTS.md");
        let content = fs::read_to_string(&agents).unwrap();
        let tampered = content.replace("Clayers Development Workflow", "OLD CONTENT");
        fs::write(&agents, tampered).unwrap();

        // Update should restore
        adopt(dir.path(), true).unwrap();
        let restored = fs::read_to_string(&agents).unwrap();
        assert!(restored.contains("Clayers Development Workflow"));
        assert!(!restored.contains("OLD CONTENT"));
    }

    #[test]
    fn test_skips_existing_clayers_dir() {
        let dir = temp_dir();
        let spec_dir = dir.path().join("clayers").join("myproject");
        fs::create_dir_all(&spec_dir).unwrap();
        fs::write(spec_dir.join("custom.xml"), "<custom/>").unwrap();

        adopt(dir.path(), false).unwrap();

        assert_eq!(
            fs::read_to_string(spec_dir.join("custom.xml")).unwrap(),
            "<custom/>"
        );
    }

    #[test]
    fn test_schema_content_matches_embedded() {
        let dir = temp_dir();
        adopt(dir.path(), false).unwrap();

        let schemas = dir.path().join(".clayers").join("schemas");
        for &(name, content) in embedded::SCHEMAS {
            let planted = fs::read_to_string(schemas.join(name)).unwrap();
            assert_eq!(planted, content, "{name} content mismatch");
        }
    }

    #[test]
    fn test_update_when_current_reports_ok() {
        let dir = temp_dir();
        adopt(dir.path(), false).unwrap();

        // Update on a fresh adopt should succeed (everything current)
        let result = adopt(dir.path(), true);
        assert!(result.is_ok());
    }
}

use std::path::Path;

use anyhow::{Context, Result};

use crate::embedded;

/// Generate HTML documentation from a spec via XSLT transformation.
///
/// # Errors
///
/// Returns an error if discovery, assembly, or XSLT transformation fails.
pub fn cmd_doc(path: &Path, output: Option<&Path>) -> Result<()> {
    // Discover spec files.
    let index_files =
        clayers_spec::discovery::find_index_files(path).context("discovery failed")?;

    let mut all_file_paths = Vec::new();
    for index_path in &index_files {
        let file_paths = clayers_spec::discovery::discover_spec_files(index_path)
            .context("file discovery failed")?;
        all_file_paths.extend(file_paths);
    }

    if all_file_paths.is_empty() {
        anyhow::bail!("no spec files found at {}", path.display());
    }

    // Assemble combined XML.
    let combined_xml = clayers_spec::assembly::assemble_combined_string(&all_file_paths)
        .context("assembly failed")?;

    // Transform via XSLT.
    let html = clayers_xml::xslt::transform(&combined_xml, embedded::DOC_XSLT_FILES)
        .context("XSLT transformation failed")?;

    // Determine output path.
    let out_path = if let Some(p) = output {
        p.to_path_buf()
    } else {
        // Derive from spec directory name.
        let name = path
            .file_name()
            .map_or_else(|| "spec".into(), |n| n.to_string_lossy().into_owned());
        std::path::PathBuf::from(format!("{name}.html"))
    };

    std::fs::write(&out_path, &html)
        .with_context(|| format!("failed to write {}", out_path.display()))?;

    println!("{}", out_path.display());
    Ok(())
}

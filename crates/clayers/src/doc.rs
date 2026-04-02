use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use base64::Engine;

use crate::embedded;

/// Generate HTML documentation from a spec via XSLT transformation.
///
/// # Errors
///
/// Returns an error if discovery, assembly, or XSLT transformation fails.
pub fn cmd_doc(path: &Path, output: Option<&Path>, self_contained: bool, watch: bool) -> Result<()> {
    let out_path = generate_doc(path, output, self_contained)?;
    println!("{}", out_path.display());

    if watch && path.is_dir() {
        use notify::{RecursiveMode, Watcher};

        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                use notify::EventKind;
                match event.kind {
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                        let _ = tx.send(());
                    }
                    _ => {}
                }
            }
        })
        .context("failed to create file watcher")?;

        watcher
            .watch(path, RecursiveMode::Recursive)
            .with_context(|| format!("failed to watch {}", path.display()))?;

        eprintln!("watching {} for changes...", path.display());

        while rx.recv().is_ok() {
            // Debounce: drain any queued events and wait a short moment.
            std::thread::sleep(std::time::Duration::from_millis(100));
            while rx.try_recv().is_ok() {}

            match generate_doc(path, output, self_contained) {
                Ok(p) => println!("{}", p.display()),
                Err(e) => eprintln!("error: {e:#}"),
            }
        }
    }

    Ok(())
}

fn generate_doc(path: &Path, output: Option<&Path>, self_contained: bool) -> Result<PathBuf> {
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
    let mut combined_xml = clayers_spec::assembly::assemble_combined_string(&all_file_paths)
        .context("assembly failed")?;

    // Generate doc:report layer (drift + code fragments) and inject into combined XML.
    let repo_root = clayers_spec::artifact::find_repo_root(path);
    let report_xml = build_doc_report(path, &all_file_paths, repo_root.as_deref());
    if !report_xml.is_empty() {
        // Insert before closing </cmb:spec>
        if let Some(pos) = combined_xml.rfind("</") {
            combined_xml.insert_str(pos, &report_xml);
        }
    }

    // Transform via XSLT.
    let mut html = clayers_xml::xslt::transform(&combined_xml, embedded::DOC_XSLT_FILES)
        .context("XSLT transformation failed")?;

    if self_contained {
        html = inline_all(&html)?;
    }

    // Determine output path.
    let out_path = if let Some(p) = output {
        p.to_path_buf()
    } else {
        let name = path
            .file_name()
            .map_or_else(|| "spec".into(), |n| n.to_string_lossy().into_owned());
        PathBuf::from(format!("{name}.html"))
    };

    std::fs::write(&out_path, &html)
        .with_context(|| format!("failed to write {}", out_path.display()))?;

    Ok(out_path)
}

/// Build a `<doc:report>` XML fragment with drift status and code fragments.
///
/// This gets injected into the combined XML so XSLT can render everything
/// natively without post-processing.
fn build_doc_report(
    spec_path: &Path,
    file_paths: &[PathBuf],
    repo_root: Option<&Path>,
) -> String {
    let mut xml = String::from(
        "<doc:report xmlns:doc=\"urn:clayers:doc\">\n",
    );

    // Collect artifact mappings.
    let mappings = collect_mappings(file_paths);

    // Run drift check.
    let drift_map = build_drift_map(spec_path, repo_root);

    for m in &mappings {
        let status = drift_map
            .get(&m.mapping_id)
            .map_or("unknown", String::as_str);

        // Drift element.
        let _ = writeln!(
            xml,
            "  <doc:drift mapping=\"{}\" node=\"{}\" status=\"{}\"/>",
            xml_escape(&m.mapping_id),
            xml_escape(&m.node_id),
            xml_escape(status),
        );

        // Code fragment elements.
        let resolved = if let Some(root) = repo_root {
            root.join(&m.artifact_path)
        } else {
            PathBuf::from(&m.artifact_path)
        };

        if let Ok(content) = std::fs::read_to_string(&resolved) {
            let lines: Vec<&str> = content.lines().collect();
            let ext = resolved
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            let language = ext_to_language(ext);

            for range in &m.ranges {
                let (from, to) = match (range.start_line, range.end_line) {
                    (Some(s), Some(e)) => (
                        s.saturating_sub(1).min(lines.len()),
                        e.min(lines.len()),
                    ),
                    _ => (0, lines.len()),
                };
                if from >= to || from >= lines.len() {
                    continue;
                }

                let mut code = String::new();
                for (i, line) in lines[from..to].iter().enumerate() {
                    let line_no = from + i + 1;
                    let _ = writeln!(code, "{line_no:>5} | {line}");
                }

                let start_attr = range
                    .start_line
                    .map_or(String::new(), |s| format!(" start=\"{s}\""));
                let end_attr = range
                    .end_line
                    .map_or(String::new(), |e| format!(" end=\"{e}\""));

                let _ = writeln!(
                    xml,
                    "  <doc:fragment mapping=\"{}\" path=\"{}\" language=\"{}\"{}{}>",
                    xml_escape(&m.mapping_id),
                    xml_escape(&m.artifact_path),
                    xml_escape(language),
                    start_attr,
                    end_attr,
                );
                xml.push_str(&xml_escape(&code));
                xml.push_str("</doc:fragment>\n");
            }
        }
    }

    xml.push_str("</doc:report>\n");
    xml
}

struct MappingInfo {
    mapping_id: String,
    node_id: String,
    artifact_path: String,
    ranges: Vec<RangeInfo>,
}

struct RangeInfo {
    start_line: Option<usize>,
    end_line: Option<usize>,
}

/// Collect artifact mapping info from spec files (lightweight, no xot needed).
fn collect_mappings(file_paths: &[PathBuf]) -> Vec<MappingInfo> {
    let mut mappings = Vec::new();
    for path in file_paths {
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        // Simple XML attribute extraction - not a full parser but sufficient.
        let mut pos = 0;
        while let Some(start) = content[pos..].find("<art:mapping") {
            let abs_start = pos + start;
            let Some(block_end) = content[abs_start..].find("</art:mapping>") else {
                break;
            };
            let block = &content[abs_start..abs_start + block_end + "</art:mapping>".len()];

            let mapping_id = extract_xml_attr(block, "id").unwrap_or_default();
            let node_id = extract_xml_attr(
                &block[block.find("<art:spec-ref").unwrap_or(0)..],
                "node",
            )
            .unwrap_or_default();
            let artifact_path = extract_xml_attr(
                &block[block.find("<art:artifact").unwrap_or(0)..],
                "path",
            )
            .unwrap_or_default();

            let mut ranges = Vec::new();
            let mut rpos = 0;
            while let Some(rs) = block[rpos..].find("<art:range") {
                let rabs = rpos + rs;
                let range_tag = &block[rabs..block[rabs..].find("/>").map_or(block.len(), |i| rabs + i + 2)];
                let sl = extract_xml_attr(range_tag, "start-line")
                    .and_then(|s| s.parse().ok());
                let el = extract_xml_attr(range_tag, "end-line")
                    .and_then(|s| s.parse().ok());
                ranges.push(RangeInfo {
                    start_line: sl,
                    end_line: el,
                });
                rpos = rabs + 1;
            }

            if ranges.is_empty() && !artifact_path.is_empty() {
                ranges.push(RangeInfo {
                    start_line: None,
                    end_line: None,
                });
            }

            if !mapping_id.is_empty() && !artifact_path.is_empty() {
                mappings.push(MappingInfo {
                    mapping_id,
                    node_id,
                    artifact_path,
                    ranges,
                });
            }

            pos = abs_start + block_end + "</art:mapping>".len();
        }
    }
    mappings
}

fn extract_xml_attr(tag: &str, name: &str) -> Option<String> {
    let patterns = [format!("{name}=\""), format!("{name}='")];
    for pat in &patterns {
        if let Some(start) = tag.find(pat.as_str()) {
            let val_start = start + pat.len();
            let quote = tag.as_bytes()[start + pat.len() - 1] as char;
            let val_end = tag[val_start..].find(quote)? + val_start;
            return Some(tag[val_start..val_end].to_string());
        }
    }
    None
}

fn build_drift_map(
    path: &Path,
    repo_root: Option<&Path>,
) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    if let Ok(report) = clayers_spec::drift::check_drift(path, repo_root) {
        for md in &report.mapping_drifts {
            let status = match &md.status {
                clayers_spec::drift::DriftStatus::Clean => "clean",
                clayers_spec::drift::DriftStatus::SpecDrifted { .. } => "spec-drifted",
                clayers_spec::drift::DriftStatus::ArtifactDrifted { .. } => "artifact-drifted",
                clayers_spec::drift::DriftStatus::Unavailable { .. } => "unavailable",
            };
            map.insert(md.mapping_id.clone(), status.to_string());
        }
    }
    map
}

fn ext_to_language(ext: &str) -> &'static str {
    match ext {
        "rs" => "rust",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" => "javascript",
        "py" => "python",
        "toml" => "toml",
        "xml" | "xsd" | "xslt" => "xml",
        "sql" => "sql",
        "sh" | "bash" => "bash",
        "yaml" | "yml" => "yaml",
        "json" => "json",
        "css" => "css",
        "html" => "html",
        "md" => "markdown",
        _ => "",
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ---------------------------------------------------------------------------
// Self-contained inlining (unchanged)
// ---------------------------------------------------------------------------

/// Fetch a URL via reqwest and return bytes.
/// Uses a modern user-agent so Google Fonts serves woff2 instead of ttf.
fn fetch(url: &str) -> Result<Vec<u8>> {
    let clean_url = url.replace("&amp;", "&");
    let client = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
        .build()
        .context("failed to build HTTP client")?;
    let resp = client
        .get(&clean_url)
        .send()
        .with_context(|| format!("fetch failed for {clean_url}"))?
        .error_for_status()
        .with_context(|| format!("HTTP error for {clean_url}"))?;
    let bytes = resp.bytes().context("failed to read response body")?;
    Ok(bytes.to_vec())
}

fn to_data_uri(bytes: &[u8], mime: &str) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!("data:{mime};base64,{b64}")
}

fn mime_for(url: &str) -> &'static str {
    let path_part = url.split('?').next().unwrap_or(url);
    let p = std::path::Path::new(path_part);
    match p.extension().and_then(|e| e.to_str()) {
        Some("js") => "text/javascript",
        Some("css") => "text/css",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        _ => {
            if url.contains("fonts.googleapis.com/css") {
                "text/css"
            } else {
                "application/octet-stream"
            }
        }
    }
}

/// Inline all external resources: replace https:// URLs in href= and src=
/// attributes with data: URIs, then do the same for `url()` in CSS.
fn inline_all(html: &str) -> Result<String> {
    let mut result = html.to_string();
    let mut cache: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    result = result
        .lines()
        .filter(|line| !line.contains("rel=\"preconnect\""))
        .collect::<Vec<_>>()
        .join("\n");

    for attr in &["href=\"https://", "src=\"https://"] {
        loop {
            let Some(attr_start) = result.find(attr) else {
                break;
            };
            let url_start = attr_start + attr.len() - "https://".len();
            let url_end = result[url_start..]
                .find('"')
                .map(|i| url_start + i)
                .context("unclosed attribute")?;
            let url = result[url_start..url_end].to_string();

            if !url.contains("cdn.")
                && !url.contains("fonts.googleapis.com")
                && !url.contains("cdnjs.")
            {
                result.replace_range(
                    url_start..url_start + "https://".len(),
                    "https-skip://",
                );
                continue;
            }

            let data_uri = if let Some(cached) = cache.get(&url) {
                cached.clone()
            } else {
                eprintln!("  inlining {url}");
                let bytes = fetch(&url)?;
                let mime = mime_for(&url);
                let uri = if mime == "text/css" {
                    let css = String::from_utf8_lossy(&bytes);
                    let inlined_css = inline_css_urls(&css, &mut cache)?;
                    to_data_uri(inlined_css.as_bytes(), mime)
                } else {
                    to_data_uri(&bytes, mime)
                };
                cache.insert(url, uri.clone());
                uri
            };

            result.replace_range(url_start..url_end, &data_uri);
        }
    }

    result = result.replace("https-skip://", "https://");
    Ok(result)
}

fn inline_css_urls(
    css: &str,
    cache: &mut std::collections::HashMap<String, String>,
) -> Result<String> {
    let mut result = css.to_string();
    let needle = "url(https://";

    while let Some(start) = result.find(needle) {
        let url_start = start + "url(".len();
        let url_end = result[url_start..]
            .find(')')
            .map(|i| url_start + i)
            .context("unclosed url()")?;
        let url = result[url_start..url_end].to_string();

        let data_uri = if let Some(cached) = cache.get(&url) {
            cached.clone()
        } else {
            eprintln!("  inlining font {url}");
            let bytes = fetch(&url)?;
            let mime = mime_for(&url);
            let uri = to_data_uri(&bytes, mime);
            cache.insert(url, uri.clone());
            uri
        };

        result.replace_range(url_start..url_end, &data_uri);
    }

    Ok(result)
}

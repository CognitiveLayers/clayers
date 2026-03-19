use std::path::Path;

use anyhow::{Context, Result};
use base64::Engine;

use crate::embedded;

/// Generate HTML documentation from a spec via XSLT transformation.
///
/// # Errors
///
/// Returns an error if discovery, assembly, or XSLT transformation fails.
pub fn cmd_doc(path: &Path, output: Option<&Path>, self_contained: bool) -> Result<()> {
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
        std::path::PathBuf::from(format!("{name}.html"))
    };

    std::fs::write(&out_path, &html)
        .with_context(|| format!("failed to write {}", out_path.display()))?;

    println!("{}", out_path.display());
    Ok(())
}

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

/// Convert bytes to a data URI.
fn to_data_uri(bytes: &[u8], mime: &str) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!("data:{mime};base64,{b64}")
}

/// Guess MIME type from URL.
fn mime_for(url: &str) -> &'static str {
    // Strip query string for extension check.
    let path_part = url.split('?').next().unwrap_or(url);
    let p = std::path::Path::new(path_part);
    match p.extension().and_then(|e| e.to_str()) {
        Some("js") => "text/javascript",
        Some("css") => "text/css",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        _ => {
            // Heuristic: Google Fonts API returns CSS.
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

    // 1. Remove preconnect links first (they contain bare domain hrefs).
    result = result
        .lines()
        .filter(|line| !line.contains("rel=\"preconnect\""))
        .collect::<Vec<_>>()
        .join("\n");

    // 2. Replace href="https://..." and src="https://..." with data URIs.
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

            // Skip content links (not resources).
            if !url.contains("cdn.")
                && !url.contains("fonts.googleapis.com")
                && !url.contains("cdnjs.")
            {
                // Replace with a marker to skip on next iteration.
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

    // 3. Restore skipped URLs.
    result = result.replace("https-skip://", "https://");

    Ok(result)
}

/// Inline url(https://...) references inside CSS content (fonts).
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

//! `clayers search` subcommand handler (feature-gated).
//!
//! Steps 2–5 land here: `--dump-chunks` (hidden), `search index PATH`,
//! `--rebuild`, and the ranked natural-language query.

use std::io::IsTerminal;
use std::path::Path;

use anyhow::{Context, Result};

use crate::cli::SearchCmd;

const DEFAULT_MODEL: &str = "bge-small-en-v1.5";

/// Options collected from bare `clayers search` flags (no subcommand).
#[allow(clippy::struct_excessive_bools)]
pub struct BareSearchOpts<'a> {
    pub dump_chunks: bool,
    pub json: bool,
    pub rebuild: bool,
    pub verbose: bool,
    pub model: Option<&'a str>,
    pub k: usize,
    pub alpha: f32,
    pub beta: f32,
    pub xpath: Option<&'a str>,
    pub layer: &'a [String],
    /// Additional queries for multi-query mode. See `search-multi-query`
    /// in the spec for union-by-id-with-max-score semantics.
    pub also: &'a [String],
}

/// Dispatch for `clayers search <subcommand>`.
///
/// # Errors
/// Propagates any error from the underlying build.
pub fn dispatch_sub(sub: &SearchCmd) -> Result<()> {
    match sub {
        SearchCmd::Index {
            path,
            rebuild,
            verbose,
            model,
        } => cmd_index(path, *rebuild, *verbose, model.as_deref()),
    }
}

/// Dispatch for bare `clayers search` (no subcommand).
///
/// # Errors
/// Propagates errors from chunker, embedder, or usearch.
pub fn cmd_search(path: &Path, query: Option<&str>, opts: &BareSearchOpts<'_>) -> Result<()> {
    if opts.dump_chunks {
        return dump_chunks_cmd(path, opts.json);
    }
    if opts.rebuild && query.is_none() && opts.also.is_empty() {
        return cmd_index(path, true, opts.verbose, opts.model);
    }
    let mut queries: Vec<&str> = Vec::with_capacity(1 + opts.also.len());
    if let Some(q) = query {
        queries.push(q);
    }
    queries.extend(opts.also.iter().map(String::as_str));
    if queries.is_empty() {
        anyhow::bail!(
            "clayers search: pass a query string, one or more `--also QUERY` flags, \
             `index PATH`, or `--dump-chunks PATH`."
        );
    }
    if queries.len() == 1 {
        run_query(path, queries[0], opts)
    } else {
        run_multi_query(path, &queries, opts)
    }
}

fn cmd_index(path: &Path, rebuild: bool, verbose: bool, model: Option<&str>) -> Result<()> {
    let model = model.unwrap_or(DEFAULT_MODEL);
    let report = clayers_search::index::build_or_update(path, model, rebuild, verbose)
        .with_context(|| format!("build_or_update for {}", path.display()))?;
    if verbose {
        eprintln!(
            "clayers-search: total={} re-embedded={} reused={} removed={}",
            report.total_nodes, report.re_embedded, report.reused, report.removed,
        );
    }
    Ok(())
}

fn run_query(path: &Path, query_text: &str, opts: &BareSearchOpts<'_>) -> Result<()> {
    let model = opts.model.unwrap_or(DEFAULT_MODEL);
    let params = clayers_search::query::QueryParams {
        query_text,
        k: opts.k,
        alpha: opts.alpha,
        beta: opts.beta,
        xpath: opts.xpath,
        layer_filter: opts.layer,
        model,
        verbose: opts.verbose,
    };
    let hits = clayers_search::query::run(path, &params)
        .with_context(|| format!("search {query_text:?}"))?;
    if opts.json {
        println!("{}", serde_json::to_string(&hits)?);
    } else {
        render_hits(query_text, &hits);
    }
    Ok(())
}

/// Run multiple queries, union by id (max score wins), sort, truncate to k.
///
/// Semantics documented in `clayers/clayers/search.xml` under
/// `search-multi-query`. Each variant runs independently; results merge
/// on the way out so agents paraphrasing to hedge vocabulary mismatch pay
/// only one CLI warm-up instead of one per variant.
fn run_multi_query(
    path: &Path,
    queries: &[&str],
    opts: &BareSearchOpts<'_>,
) -> Result<()> {
    use std::cmp::Ordering;
    use std::collections::HashMap;

    let model = opts.model.unwrap_or(DEFAULT_MODEL);
    // Merge by id; keep the whole Hit record from the best-scoring variant
    // so text_score/struct_score/preview reflect the variant that won.
    let mut best: HashMap<String, clayers_search::query::Hit> = HashMap::new();
    for &q in queries {
        let params = clayers_search::query::QueryParams {
            query_text: q,
            k: opts.k,
            alpha: opts.alpha,
            beta: opts.beta,
            xpath: opts.xpath,
            layer_filter: opts.layer,
            model,
            verbose: opts.verbose,
        };
        let hits = clayers_search::query::run(path, &params)
            .with_context(|| format!("search {q:?}"))?;
        for h in hits {
            match best.get(&h.id) {
                Some(existing) if existing.score >= h.score => {}
                _ => {
                    best.insert(h.id.clone(), h);
                }
            }
        }
    }
    let mut merged: Vec<clayers_search::query::Hit> = best.into_values().collect();
    merged.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });
    merged.truncate(opts.k);

    if opts.json {
        println!("{}", serde_json::to_string(&merged)?);
    } else {
        // Use a pipe-joined label so the header shows which variants ran.
        let label = queries.join(" | ");
        render_hits(&label, &merged);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Human-readable hit renderer.
//
// Output uses ANSI colors when stdout is a TTY. Layers get colored so
// different types are easy to scan; the matching query tokens are
// emphasized in the preview so users can see what the semantic ranker
// locked onto (even synonym matches often share at least one token).
// ---------------------------------------------------------------------------

fn render_hits(query: &str, hits: &[clayers_search::query::Hit]) {
    let color = std::io::stdout().is_terminal();
    if hits.is_empty() {
        let msg = "no hits — try a broader query, or drop --xpath/--layer";
        println!("{}", dim(msg, color));
        return;
    }
    let width = term_width().saturating_sub(4).max(40);
    let query_tokens = tokenize_for_highlight(query);
    for (rank, h) in hits.iter().enumerate() {
        let filename = h.file.rsplit('/').next().unwrap_or(&h.file);
        let header = format!(
            "{rank}. {score}  {id}  {where_}",
            rank = bold(&format!("{:>2}", rank + 1), color),
            score = dim(&format!("[{:.3}]", h.score), color),
            id = bright(&h.id, color),
            where_ = dim(&format!("{filename}:{}..{}", h.line_start, h.line_end), color),
        );
        let meta_line = format!(
            "    {layer}  {scores}",
            layer = layer_tag(&h.layer, color),
            scores = dim(
                &format!("text={:.3} struct={:.3}", h.text_score, h.struct_score),
                color,
            ),
        );
        let preview_lines = format_preview(&h.preview, &query_tokens, width, color);
        println!("{header}\n{meta_line}\n{preview_lines}");
    }
}

/// Word-wrap `text` to `width`, indent each line with 4 spaces,
/// highlight query tokens, and cap at 4 visible lines with an ellipsis.
fn format_preview(
    text: &str,
    tokens: &[String],
    width: usize,
    color: bool,
) -> String {
    const MAX_LINES: usize = 4;
    let wrapped = wrap_words(text, width);
    let visible: Vec<&String> = wrapped.iter().take(MAX_LINES).collect();
    let mut out = String::new();
    for (i, line) in visible.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str("    ");
        out.push_str(&highlight(line, tokens, color));
    }
    if wrapped.len() > MAX_LINES {
        out.push('\n');
        out.push_str("    ");
        out.push_str(&dim("…", color));
    }
    out
}

/// Split `text` into lines each ≤ `width` characters at word boundaries.
fn wrap_words(text: &str, width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            current.push_str(word);
        } else if current.chars().count() + 1 + word.chars().count() <= width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

/// Terminal column width, or a reasonable default when stdout isn't a
/// TTY or the env var isn't set.
fn term_width() -> usize {
    if let Ok(cols) = std::env::var("COLUMNS")
        && let Ok(n) = cols.parse::<usize>()
        && n > 20
    {
        return n;
    }
    80
}

fn tokenize_for_highlight(q: &str) -> Vec<String> {
    q.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 3) // skip "a", "of", "in", etc.
        .map(str::to_owned)
        .collect()
}

fn highlight(preview: &str, tokens: &[String], color: bool) -> String {
    if !color || tokens.is_empty() {
        return preview.to_owned();
    }
    // Case-insensitive word-boundary match on each query token;
    // wrap matches in bold yellow so the visual anchor is obvious.
    let mut out = String::with_capacity(preview.len());
    let lower = preview.to_lowercase();
    let bytes = preview.as_bytes();
    let lower_bytes = lower.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Find the earliest match at position i across any token.
        let mut best: Option<usize> = None;
        for tok in tokens {
            if lower_bytes.len() >= i + tok.len()
                && &lower_bytes[i..i + tok.len()] == tok.as_bytes()
            {
                let prev_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
                let next_ok = i + tok.len() == bytes.len()
                    || !bytes[i + tok.len()].is_ascii_alphanumeric();
                if prev_ok && next_ok {
                    best = Some(tok.len());
                    break;
                }
            }
        }
        if let Some(len) = best {
            let original: String = preview[i..i + len].to_owned();
            out.push_str("\x1b[1;33m"); // bold yellow
            out.push_str(&original);
            out.push_str("\x1b[0m");
            i += len;
        } else {
            // Advance one UTF-8 char.
            let ch_len = (1..=4)
                .find(|n| preview.is_char_boundary(i + n))
                .unwrap_or(1);
            out.push_str(&preview[i..i + ch_len]);
            i += ch_len;
        }
    }
    out
}

fn bold(s: &str, color: bool) -> String {
    if color {
        format!("\x1b[1m{s}\x1b[0m")
    } else {
        s.to_owned()
    }
}

fn bright(s: &str, color: bool) -> String {
    if color {
        format!("\x1b[1;36m{s}\x1b[0m") // bold cyan
    } else {
        s.to_owned()
    }
}

fn dim(s: &str, color: bool) -> String {
    if color {
        format!("\x1b[2m{s}\x1b[0m")
    } else {
        s.to_owned()
    }
}

/// Small colored tag for the layer name. Color is derived
/// deterministically from the layer string's hash, so new or custom
/// layers get a stable color without needing a hardcoded table.
fn layer_tag(layer: &str, color: bool) -> String {
    if !color {
        return format!("[{layer}]");
    }
    let code = layer_color_code(layer);
    format!("\x1b[{code}m[{layer}]\x1b[0m")
}

/// Palette of visible ANSI foreground codes.
///
/// Excludes black (30) and white (37) because they're invisible on
/// dark/light terminals respectively, and excludes bright-white (97)
/// for the same reason. The palette is intentionally conservative.
const LAYER_COLORS: &[&str] = &[
    "31", // red
    "32", // green
    "33", // yellow
    "34", // blue
    "35", // magenta
    "36", // cyan
    "91", // bright red
    "92", // bright green
    "93", // bright yellow
    "94", // bright blue
    "95", // bright magenta
    "96", // bright cyan
];

fn layer_color_code(layer: &str) -> &'static str {
    // Simple 32-bit FxHash-style mix; deterministic across runs.
    let mut h: u32 = 0;
    for b in layer.as_bytes() {
        h = h.rotate_left(5) ^ u32::from(*b);
        h = h.wrapping_mul(0x9E37_79B9);
    }
    let idx = (h as usize) % LAYER_COLORS.len();
    LAYER_COLORS[idx]
}

fn dump_chunks_cmd(path: &Path, json: bool) -> Result<()> {
    let chunks = clayers_spec::chunker::extract_chunks(path)
        .with_context(|| format!("chunker failed on {}", path.display()))?;
    if json {
        let out = serde_json::to_string(&chunks)?;
        println!("{out}");
    } else {
        for c in &chunks {
            println!(
                "{}\t{}:{}-{}\t{}\t{}",
                c.id,
                c.file.display(),
                c.line_start,
                c.line_end,
                c.layer,
                c.element_name
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod render_tests {
    use super::*;

    #[test]
    fn tokenize_skips_short_words() {
        let t = tokenize_for_highlight("a detect of in mismatches");
        assert_eq!(t, vec!["detect".to_string(), "mismatches".into()]);
    }

    #[test]
    fn highlight_disabled_when_no_color() {
        let out = highlight("Divergence detection works", &["detection".into()], false);
        assert_eq!(out, "Divergence detection works");
    }

    #[test]
    fn highlight_wraps_token_in_ansi_when_color() {
        let out = highlight(
            "the Divergence of things",
            &["divergence".into()],
            true,
        );
        assert!(out.contains("\x1b[1;33m"));
        assert!(out.contains("Divergence"));
        assert!(out.contains("\x1b[0m"));
    }

    #[test]
    fn highlight_respects_word_boundaries() {
        // "cat" should NOT match inside "concatenate".
        let out = highlight("concatenate cat", &["cat".into()], true);
        let concat_idx = out.find("concatenate").unwrap();
        let before_concat = &out[..concat_idx];
        assert!(!before_concat.contains("\x1b["), "false match inside word");
        assert!(out.ends_with("\x1b[0m"));
    }

    #[test]
    fn layer_tag_has_brackets() {
        assert_eq!(layer_tag("prose", false), "[prose]");
        assert!(layer_tag("prose", true).contains("[prose]"));
    }

    #[test]
    fn layer_color_is_deterministic() {
        let a = layer_color_code("terminology");
        let b = layer_color_code("terminology");
        assert_eq!(a, b, "same layer must map to same color");
    }

    #[test]
    fn different_layers_probably_get_different_colors() {
        // Not guaranteed for all pairs (palette is finite), but two
        // common layers should differ for our hash function.
        assert_ne!(
            layer_color_code("terminology"),
            layer_color_code("prose"),
        );
    }

    #[test]
    fn layer_color_palette_excludes_invisible_codes() {
        for code in LAYER_COLORS {
            assert_ne!(*code, "30", "black invisible on dark bg");
            assert_ne!(*code, "37", "white invisible on light bg");
            assert_ne!(*code, "97", "bright-white invisible on light bg");
        }
    }

    #[test]
    fn wrap_words_respects_width_on_word_boundaries() {
        let lines = wrap_words("one two three four five six seven eight", 12);
        for l in &lines {
            assert!(l.chars().count() <= 12, "line too wide: {l:?}");
        }
        // Joining back must reconstruct original (modulo whitespace).
        let joined = lines.join(" ");
        assert_eq!(joined, "one two three four five six seven eight");
    }

    #[test]
    fn format_preview_caps_at_four_lines() {
        let text = (0..20)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let out = format_preview(&text, &[], 20, false);
        // 4 body lines + 1 ellipsis = 5 lines max.
        let line_count = out.lines().count();
        assert!(
            line_count <= 5,
            "preview exceeded cap: {line_count} lines\n{out}"
        );
        assert!(out.contains('…'), "ellipsis missing from truncated preview");
    }

    #[test]
    fn format_preview_short_text_has_no_ellipsis() {
        let out = format_preview("brief note.", &[], 60, false);
        assert!(!out.contains('…'));
        assert_eq!(out.lines().count(), 1);
    }
}

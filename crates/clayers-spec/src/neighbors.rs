//! Neighborhood collection around a landing node.
//!
//! For a given landing `@id`, walks the assembled combined document
//! once and collects neighbors across three edge kinds that the
//! `clayers-context` skill's Phase 3 traverses:
//!
//! 1. **Explicit relations** — `rel:relation[@from=ID]` and
//!    `rel:relation[@to=ID]` yield the opposing end, tagged with the
//!    relation's `@type` (implements, depends-on, …).
//! 2. **Terminology references** — `trm:ref[@term=ID]` means some
//!    node cites this term (inbound); refs inside a node-with-@id=ID
//!    are the terms it cites (outbound).
//! 3. **Artifact mappings** — `art:mapping[art:spec-ref/@node=ID]` is
//!    a mapping pointing AT the landing. If the landing IS a mapping,
//!    its `art:spec-ref/@node` is the neighbor.
//!
//! After collection, an optional hub pre-filter applies when total
//! degree exceeds `hub_threshold`: candidates are partitioned into
//! three buckets (`artifact-map`, `relation`, `term-ref`); within
//! each, ranked by relation-type priority; the top `top_per_bucket`
//! survive. Peeks are `llm:node` bodies (preferred), with fallbacks
//! to `pr:shortdesc`, `trm:definition`, and `art:note`.

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;
use xot::Xot;

use crate::assembly::assemble_combined;
use crate::discovery::discover_spec_files;
use crate::namespace;

// Type aliases — keep signatures readable.
type RawCandidate = (String, String, Option<String>);

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub hub_threshold: usize,
    pub top_per_bucket: usize,
    pub peek_chars: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hub_threshold: 12,
            top_per_bucket: 2,
            peek_chars: 350,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub id: String,
    pub edge_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_subtype: Option<String>,
    pub peek: String,
}

#[derive(Debug, Serialize)]
pub struct NeighborBundle {
    pub landing_id: String,
    pub degree_observed: usize,
    pub neighbors_by_edge_kind: HashMap<String, usize>,
    pub hub_engaged: bool,
    pub candidates: Vec<Candidate>,
}

/// Walk a spec directory and return the neighborhood of `landing_id`.
///
/// # Errors
/// Returns an error if the index can't be resolved, files can't be
/// parsed, or the combined document assembly fails.
pub fn neighbors_for(
    spec_path: &Path,
    landing_id: &str,
    config: Config,
) -> Result<NeighborBundle, crate::Error> {
    let index_path = spec_path.join("index.xml");
    let files = discover_spec_files(&index_path)?;
    let (mut xot, root) = assemble_combined(&files)?;
    let names = Names::resolve(&mut xot);
    let walker = Walker::collect(&xot, root, &names);
    Ok(build_bundle(&walker, landing_id, config))
}

// ---------------------------------------------------------------------------
// Pre-resolved element/attribute names
// ---------------------------------------------------------------------------

struct Names {
    id_attr: xot::NameId,
    type_attr: xot::NameId,
    from_attr: xot::NameId,
    to_attr: xot::NameId,
    term_attr: xot::NameId,
    node_attr: xot::NameId,
    ref_attr: xot::NameId,

    pr_shortdesc: xot::NameId,
    trm_ref: xot::NameId,
    trm_definition: xot::NameId,
    rel_relation: xot::NameId,
    art_mapping: xot::NameId,
    art_spec_ref: xot::NameId,
    art_note: xot::NameId,
    llm_node: xot::NameId,
}

impl Names {
    fn resolve(xot: &mut Xot) -> Self {
        let pr = xot.add_namespace(namespace::PROSE);
        let trm = xot.add_namespace(namespace::TERMINOLOGY);
        let rel = xot.add_namespace(namespace::RELATION);
        let art = xot.add_namespace(namespace::ARTIFACT);
        let llm = xot.add_namespace(namespace::LLM);

        Self {
            id_attr: xot.add_name("id"),
            type_attr: xot.add_name("type"),
            from_attr: xot.add_name("from"),
            to_attr: xot.add_name("to"),
            term_attr: xot.add_name("term"),
            node_attr: xot.add_name("node"),
            ref_attr: xot.add_name("ref"),

            pr_shortdesc: xot.add_name_ns("shortdesc", pr),
            trm_ref: xot.add_name_ns("ref", trm),
            trm_definition: xot.add_name_ns("definition", trm),
            rel_relation: xot.add_name_ns("relation", rel),
            art_mapping: xot.add_name_ns("mapping", art),
            art_spec_ref: xot.add_name_ns("spec-ref", art),
            art_note: xot.add_name_ns("note", art),
            llm_node: xot.add_name_ns("node", llm),
        }
    }
}

// ---------------------------------------------------------------------------
// Tree walk — one pass, collect everything needed
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct Relation {
    rel_type: String,
    from: String,
    to: String,
}

#[derive(Debug)]
struct TermRef {
    source_id: String,
    term: String,
}

#[derive(Debug)]
struct Mapping {
    id: String,
    spec_node: String,
}

#[derive(Debug, Default, Clone)]
struct PeekSources {
    shortdesc: String,
    definition: String,
    note: String,
}

struct Walker {
    relations: Vec<Relation>,
    trm_refs: Vec<TermRef>,
    mappings: Vec<Mapping>,
    peek_by_id: HashMap<String, PeekSources>,
    llm_by_ref: HashMap<String, String>,
}

impl Walker {
    fn collect(xot: &Xot, root: xot::Node, names: &Names) -> Self {
        let mut w = Walker {
            relations: Vec::new(),
            trm_refs: Vec::new(),
            mappings: Vec::new(),
            peek_by_id: HashMap::new(),
            llm_by_ref: HashMap::new(),
        };
        let mut id_stack: Vec<String> = Vec::new();
        w.walk(xot, root, names, &mut id_stack);
        w
    }

    fn walk(
        &mut self,
        xot: &Xot,
        node: xot::Node,
        names: &Names,
        id_stack: &mut Vec<String>,
    ) {
        if !xot.is_element(node) {
            return;
        }
        let Some(elem) = xot.element(node) else {
            return;
        };
        let name = elem.name();

        let own_id: Option<String> = xot
            .get_attribute(node, names.id_attr)
            .map(ToString::to_string);
        if let Some(id) = &own_id {
            // Seed an empty entry so the peek map always has keys for
            // every id we saw, even if no peek-source child exists.
            self.peek_by_id.entry(id.clone()).or_default();
            id_stack.push(id.clone());
        }

        if name == names.rel_relation {
            let typ = xot
                .get_attribute(node, names.type_attr)
                .unwrap_or_default()
                .to_string();
            let from = xot
                .get_attribute(node, names.from_attr)
                .unwrap_or_default()
                .to_string();
            let to = xot
                .get_attribute(node, names.to_attr)
                .unwrap_or_default()
                .to_string();
            if !from.is_empty() && !to.is_empty() {
                self.relations.push(Relation {
                    rel_type: typ,
                    from,
                    to,
                });
            }
        } else if name == names.trm_ref {
            let term = xot
                .get_attribute(node, names.term_attr)
                .unwrap_or_default()
                .to_string();
            if !term.is_empty()
                && let Some(src) = id_stack.last()
                && src != &term
            {
                self.trm_refs.push(TermRef {
                    source_id: src.clone(),
                    term,
                });
            }
        } else if name == names.art_mapping
            && let Some(mapping_id) = &own_id
            && let Some(sn) = find_child_attr(
                xot,
                node,
                names.art_spec_ref,
                names.node_attr,
            )
            && !sn.is_empty()
        {
            self.mappings.push(Mapping {
                id: mapping_id.clone(),
                spec_node: sn,
            });
        } else if name == names.llm_node
            && let Some(target) = xot.get_attribute(node, names.ref_attr)
        {
            let text = collect_text(xot, node);
            if !text.is_empty() {
                self.llm_by_ref.insert(target.to_string(), text);
            }
        }

        // Peek-source capture: if this element is a direct peek-source
        // child of a node-with-@id on the stack, cache its text.
        if let Some(parent_id) = id_stack.last()
            && (name == names.pr_shortdesc
                || name == names.trm_definition
                || name == names.art_note)
        {
            let entry = self
                .peek_by_id
                .entry(parent_id.clone())
                .or_default();
            let text = collect_text(xot, node);
            if name == names.pr_shortdesc && entry.shortdesc.is_empty() {
                entry.shortdesc = text;
            } else if name == names.trm_definition && entry.definition.is_empty() {
                entry.definition = text;
            } else if name == names.art_note && entry.note.is_empty() {
                entry.note = text;
            }
        }

        for child in xot.children(node) {
            self.walk(xot, child, names, id_stack);
        }

        if own_id.is_some() {
            id_stack.pop();
        }
    }
}

fn find_child_attr(
    xot: &Xot,
    node: xot::Node,
    child_tag: xot::NameId,
    attr: xot::NameId,
) -> Option<String> {
    for child in xot.children(node) {
        if !xot.is_element(child) {
            continue;
        }
        if xot.element(child).map(xot::Element::name) == Some(child_tag)
            && let Some(v) = xot.get_attribute(child, attr)
        {
            return Some(v.to_string());
        }
    }
    None
}

fn collect_text(xot: &Xot, node: xot::Node) -> String {
    let mut out = String::new();
    collect_text_into(xot, node, &mut out);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn collect_text_into(xot: &Xot, node: xot::Node, out: &mut String) {
    for child in xot.children(node) {
        if let Some(t) = xot.text(child) {
            out.push_str(t.get());
            out.push(' ');
        } else if xot.is_element(child) {
            collect_text_into(xot, child, out);
        }
    }
}

// ---------------------------------------------------------------------------
// Bundle construction
// ---------------------------------------------------------------------------

fn type_priority(t: &str) -> u32 {
    match t {
        "implements" => 6,
        "depends-on" => 5,
        "refines" => 4,
        "precedes" => 3,
        "constrains" => 2,
        "references" => 1,
        _ => 0,
    }
}

fn score(kind: &str, subtype: Option<&str>) -> u32 {
    match kind {
        "relation" => 100 + type_priority(subtype.unwrap_or("")),
        "artifact-map" => 90,
        "term-ref" => 10,
        _ => 0,
    }
}

fn build_bundle(
    walker: &Walker,
    landing_id: &str,
    config: Config,
) -> NeighborBundle {
    let raw = collect_raw_candidates(walker, landing_id);
    let deduped = dedupe_by_id(raw);
    let degree_observed = deduped.len();

    let mut by_kind: HashMap<String, usize> = HashMap::new();
    for (_, kind, _) in &deduped {
        *by_kind.entry(kind.clone()).or_insert(0) += 1;
    }

    let hub_engaged = degree_observed > config.hub_threshold;
    let surviving: Vec<RawCandidate> = if hub_engaged {
        bucket_diversity(&deduped, config.top_per_bucket)
    } else {
        deduped
    };

    let candidates = surviving
        .into_iter()
        .map(|(id, kind, sub)| {
            let peek = fetch_peek(walker, &id, config.peek_chars);
            Candidate {
                edge_kind: kind,
                edge_subtype: sub,
                peek,
                id,
            }
        })
        .collect();

    NeighborBundle {
        landing_id: landing_id.to_string(),
        degree_observed,
        neighbors_by_edge_kind: by_kind,
        hub_engaged,
        candidates,
    }
}

fn collect_raw_candidates(walker: &Walker, landing_id: &str) -> Vec<RawCandidate> {
    let mut raw: Vec<RawCandidate> = Vec::new();
    for rel in &walker.relations {
        if rel.from == landing_id && rel.to != landing_id {
            raw.push((
                rel.to.clone(),
                "relation".into(),
                Some(rel.rel_type.clone()),
            ));
        }
        if rel.to == landing_id && rel.from != landing_id {
            raw.push((
                rel.from.clone(),
                "relation".into(),
                Some(rel.rel_type.clone()),
            ));
        }
    }
    for tref in &walker.trm_refs {
        if tref.source_id == landing_id && tref.term != landing_id {
            raw.push((tref.term.clone(), "term-ref".into(), None));
        }
        if tref.term == landing_id && tref.source_id != landing_id {
            raw.push((tref.source_id.clone(), "term-ref".into(), None));
        }
    }
    for mp in &walker.mappings {
        if mp.spec_node == landing_id {
            raw.push((mp.id.clone(), "artifact-map".into(), None));
        }
        if mp.id == landing_id && mp.spec_node != landing_id {
            raw.push((mp.spec_node.clone(), "artifact-map".into(), None));
        }
    }
    raw
}

fn dedupe_by_id(raw: Vec<RawCandidate>) -> Vec<RawCandidate> {
    let mut by_id: HashMap<String, (String, Option<String>)> = HashMap::new();
    for (id, kind, subtype) in raw {
        let incoming = score(&kind, subtype.as_deref());
        let replace = match by_id.get(&id) {
            Some((existing_kind, existing_sub)) => {
                incoming > score(existing_kind, existing_sub.as_deref())
            }
            None => true,
        };
        if replace {
            by_id.insert(id, (kind, subtype));
        }
    }
    let mut out: Vec<RawCandidate> = by_id
        .into_iter()
        .map(|(id, (kind, sub))| (id, kind, sub))
        .collect();
    out.sort_by(|a, b| {
        score(&b.1, b.2.as_deref())
            .cmp(&score(&a.1, a.2.as_deref()))
            .then_with(|| a.0.cmp(&b.0))
    });
    out
}

fn bucket_diversity(
    candidates: &[RawCandidate],
    top_per_bucket: usize,
) -> Vec<RawCandidate> {
    let mut buckets: HashMap<&str, Vec<&RawCandidate>> = HashMap::new();
    for c in candidates {
        buckets.entry(c.1.as_str()).or_default().push(c);
    }
    for list in buckets.values_mut() {
        list.sort_by(|a, b| {
            score(&b.1, b.2.as_deref())
                .cmp(&score(&a.1, a.2.as_deref()))
                .then_with(|| a.0.cmp(&b.0))
        });
    }
    let kind_order = ["artifact-map", "relation", "term-ref"];
    let mut out = Vec::new();
    for kind in kind_order {
        if let Some(list) = buckets.get(kind) {
            for c in list.iter().take(top_per_bucket) {
                out.push((*c).clone());
            }
        }
    }
    out
}

fn fetch_peek(walker: &Walker, id: &str, max_chars: usize) -> String {
    if let Some(text) = walker.llm_by_ref.get(id) {
        return truncate(text, max_chars);
    }
    if let Some(sources) = walker.peek_by_id.get(id) {
        for cand in [&sources.shortdesc, &sources.definition, &sources.note] {
            if !cand.is_empty() {
                return truncate(cand, max_chars);
            }
        }
    }
    "(no peek available)".to_string()
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars).collect();
    out.push('…');
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn self_spec() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../clayers/clayers")
            .canonicalize()
            .expect("clayers/clayers/ not found")
    }

    #[test]
    fn drift_tooling_hits_all_three_edge_kinds() {
        let bundle = neighbors_for(
            self_spec().as_path(),
            "drift-detection-tooling",
            Config::default(),
        )
        .expect("walk failed");
        assert_eq!(bundle.landing_id, "drift-detection-tooling");
        assert!(
            bundle.degree_observed >= 10,
            "expected combined degree ≥10, got {}",
            bundle.degree_observed
        );
        assert!(bundle.hub_engaged);
        assert_eq!(bundle.candidates.len(), 6);
        let kinds: std::collections::HashSet<&str> = bundle
            .candidates
            .iter()
            .map(|c| c.edge_kind.as_str())
            .collect();
        assert!(kinds.contains("relation"));
        assert!(kinds.contains("term-ref"));
        assert!(kinds.contains("artifact-map"));
        for c in &bundle.candidates {
            assert!(!c.peek.is_empty(), "empty peek for {}", c.id);
        }
    }

    #[test]
    fn unknown_id_returns_empty_bundle() {
        let bundle = neighbors_for(
            self_spec().as_path(),
            "this-id-does-not-exist",
            Config::default(),
        )
        .expect("walk failed");
        assert_eq!(bundle.degree_observed, 0);
        assert!(!bundle.hub_engaged);
        assert!(bundle.candidates.is_empty());
    }

    #[test]
    fn dedupe_scoring_respects_priority() {
        assert!(score("relation", Some("implements")) > score("relation", Some("depends-on")));
        assert!(score("relation", Some("depends-on")) > score("artifact-map", None));
        assert!(score("artifact-map", None) > score("term-ref", None));
    }

    #[test]
    fn bucket_diversity_picks_two_per_bucket() {
        let mut cands: Vec<RawCandidate> = Vec::new();
        for i in 0..5 {
            cands.push((format!("am-{i}"), "artifact-map".into(), None));
        }
        for i in 0..3 {
            cands.push((
                format!("rel-{i}"),
                "relation".into(),
                Some("depends-on".into()),
            ));
        }
        cands.push(("tref-0".into(), "term-ref".into(), None));
        let out = bucket_diversity(&cands, 2);
        assert_eq!(out.len(), 5);
        let am_count = out.iter().filter(|c| c.1 == "artifact-map").count();
        let rel_count = out.iter().filter(|c| c.1 == "relation").count();
        let trm_count = out.iter().filter(|c| c.1 == "term-ref").count();
        assert_eq!(am_count, 2);
        assert_eq!(rel_count, 2);
        assert_eq!(trm_count, 1);
    }
}

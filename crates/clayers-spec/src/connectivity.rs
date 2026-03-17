use std::collections::{HashMap, HashSet};
use std::path::Path;

use petgraph::Direction;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;

use crate::namespace;

/// Graph metrics from connectivity analysis.
#[derive(Debug)]
pub struct ConnectivityReport {
    pub spec_name: String,
    pub node_count: usize,
    pub edge_count: usize,
    pub density: f64,
    pub components: Vec<Vec<String>>,
    pub isolated_nodes: Vec<String>,
    pub hub_nodes: Vec<HubNode>,
    pub bridge_nodes: Vec<BridgeNode>,
    pub cycles: Vec<Cycle>,
    pub acyclic_violations: usize,
    pub relation_type_counts: HashMap<String, usize>,
}

#[derive(Debug)]
pub struct HubNode {
    pub id: String,
    pub in_degree: usize,
    pub out_degree: usize,
    pub total_degree: usize,
}

#[derive(Debug)]
pub struct BridgeNode {
    pub id: String,
    pub centrality: f64,
}

#[derive(Debug)]
pub struct Cycle {
    pub nodes: Vec<String>,
    pub edge_types: HashSet<String>,
    pub has_acyclic_violation: bool,
}

/// Analyze the connectivity of a spec's nodes and relations.
///
/// Builds a directed graph from all elements with `@id` attributes and
/// all `rel:relation` elements. Reports components, isolated nodes,
/// hub nodes, bridge nodes, and cycles.
///
/// # Errors
///
/// Returns an error if spec files cannot be read.
pub fn analyze_connectivity(spec_dir: &Path) -> Result<ConnectivityReport, crate::Error> {
    let schema_dir = crate::discovery::find_schema_dir(spec_dir);
    let acyclic_types = if let Some(ref sd) = schema_dir {
        crate::schema::discover_acyclic_types(sd)?
    } else {
        HashSet::new()
    };

    let index_files = crate::discovery::find_index_files(spec_dir)?;
    let spec_name = spec_dir
        .file_name()
        .map_or_else(|| "unknown".into(), |n| n.to_string_lossy().into_owned());

    if index_files.is_empty() {
        return Ok(ConnectivityReport {
            spec_name,
            node_count: 0,
            edge_count: 0,
            density: 0.0,
            components: vec![],
            isolated_nodes: vec![],
            hub_nodes: vec![],
            bridge_nodes: vec![],
            cycles: vec![],
            acyclic_violations: 0,
            relation_type_counts: HashMap::new(),
        });
    }

    let mut all_nodes: HashMap<String, String> = HashMap::new(); // id -> tag
    let mut all_relations: Vec<Relation> = Vec::new();

    for index_path in &index_files {
        let file_paths = crate::discovery::discover_spec_files(index_path)?;
        collect_nodes_and_relations(&file_paths, &mut all_nodes, &mut all_relations)?;
    }

    // Build petgraph
    let (graph, _id_to_idx, idx_to_id) = build_graph(&all_nodes, &all_relations);

    let node_count = graph.node_count();
    let edge_count = graph.edge_count();
    #[allow(clippy::cast_precision_loss)]
    let density = if node_count > 1 {
        edge_count as f64 / (node_count as f64 * (node_count as f64 - 1.0))
    } else {
        0.0
    };

    // Connected components (weakly connected)
    let components = weakly_connected_components(&graph, &idx_to_id);

    // Isolated nodes
    let isolated_nodes = find_isolated_nodes(&graph, &idx_to_id);

    // Hub nodes (top 5 by total degree)
    let hub_nodes = find_hub_nodes(&graph, &idx_to_id, 5);

    // Bridge nodes (betweenness centrality)
    let bridge_nodes = find_bridge_nodes(&graph, &idx_to_id, 5);

    // Cycles
    let (cycles, acyclic_violations) = find_cycles(&graph, &idx_to_id, &acyclic_types);

    // Relation type distribution
    let mut relation_type_counts: HashMap<String, usize> = HashMap::new();
    for rel in &all_relations {
        if rel.to_spec.is_none() {
            *relation_type_counts
                .entry(rel.rel_type.clone())
                .or_insert(0) += 1;
        }
    }

    Ok(ConnectivityReport {
        spec_name,
        node_count,
        edge_count,
        density,
        components,
        isolated_nodes,
        hub_nodes,
        bridge_nodes,
        cycles,
        acyclic_violations,
        relation_type_counts,
    })
}


struct Relation {
    rel_type: String,
    from: String,
    to: String,
    to_spec: Option<String>,
}

fn collect_nodes_and_relations(
    file_paths: &[impl AsRef<Path>],
    nodes: &mut HashMap<String, String>,
    relations: &mut Vec<Relation>,
) -> Result<(), crate::Error> {
    for file_path in file_paths {
        let content = std::fs::read_to_string(file_path.as_ref())?;
        let mut xot = xot::Xot::new();
        let doc = xot.parse(&content)?;
        let root = xot.document_element(doc)?;

        let id_attr = xot.add_name("id");
        let xml_ns = xot.add_namespace(namespace::XML);
        let xml_id_attr = xot.add_name_ns("id", xml_ns);
        let relation_ns = xot.add_namespace(namespace::RELATION);
        let relation_tag = xot.add_name_ns("relation", relation_ns);
        let art_ns = xot.add_namespace(namespace::ARTIFACT);
        let mapping_tag = xot.add_name_ns("mapping", art_ns);
        let revision_ns = xot.add_namespace(namespace::REVISION);
        let revision_tag = xot.add_name_ns("revision", revision_ns);
        let type_attr = xot.add_name("type");
        let from_attr = xot.add_name("from");
        let to_attr = xot.add_name("to");
        let to_spec_attr = xot.add_name("to-spec");

        collect_from_tree(
            &xot,
            root,
            id_attr,
            xml_id_attr,
            relation_tag,
            mapping_tag,
            revision_tag,
            type_attr,
            from_attr,
            to_attr,
            to_spec_attr,
            nodes,
            relations,
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn collect_from_tree(
    xot: &xot::Xot,
    node: xot::Node,
    id_attr: xot::NameId,
    xml_id_attr: xot::NameId,
    relation_tag: xot::NameId,
    mapping_tag: xot::NameId,
    revision_tag: xot::NameId,
    type_attr: xot::NameId,
    from_attr: xot::NameId,
    to_attr: xot::NameId,
    to_spec_attr: xot::NameId,
    nodes: &mut HashMap<String, String>,
    relations: &mut Vec<Relation>,
) {
    if xot.is_element(node) {
        let name = xot.element(node).map(xot::Element::name);

        // Skip art:mapping and rev:revision from graph nodes
        if name != Some(mapping_tag) && name != Some(revision_tag) {
            // Check bare @id
            if let Some(id) = xot.element(node).and_then(|e| e.get_attribute(id_attr)) {
                let tag = name
                    .map(|n| xot.name_ns_str(n).0.to_string())
                    .unwrap_or_default();
                nodes.insert(id.to_string(), tag.clone());
            }
            // Check xml:id (W3C standard, used by XMI/UML elements)
            if let Some(xml_id) = xot.element(node).and_then(|e| e.get_attribute(xml_id_attr)) {
                let tag = name
                    .map(|n| xot.name_ns_str(n).0.to_string())
                    .unwrap_or_default();
                nodes.insert(xml_id.to_string(), tag);
            }
        }

        if name == Some(relation_tag) {
            let elem = xot.element(node);
            let rel = Relation {
                rel_type: elem
                    .and_then(|e| e.get_attribute(type_attr))
                    .unwrap_or("")
                    .to_string(),
                from: elem
                    .and_then(|e| e.get_attribute(from_attr))
                    .unwrap_or("")
                    .to_string(),
                to: elem
                    .and_then(|e| e.get_attribute(to_attr))
                    .unwrap_or("")
                    .to_string(),
                to_spec: elem
                    .and_then(|e| e.get_attribute(to_spec_attr))
                    .map(String::from),
            };
            relations.push(rel);
        }
    }
    for child in xot.children(node) {
        collect_from_tree(
            xot,
            child,
            id_attr,
            xml_id_attr,
            relation_tag,
            mapping_tag,
            revision_tag,
            type_attr,
            from_attr,
            to_attr,
            to_spec_attr,
            nodes,
            relations,
        );
    }
}

fn build_graph(
    nodes: &HashMap<String, String>,
    relations: &[Relation],
) -> (
    DiGraph<String, String>,
    HashMap<String, NodeIndex>,
    HashMap<NodeIndex, String>,
) {
    let mut graph = DiGraph::new();
    let mut id_to_idx: HashMap<String, NodeIndex> = HashMap::new();
    let mut idx_to_id: HashMap<NodeIndex, String> = HashMap::new();

    for id in nodes.keys() {
        let idx = graph.add_node(id.clone());
        id_to_idx.insert(id.clone(), idx);
        idx_to_id.insert(idx, id.clone());
    }

    for rel in relations {
        if rel.to_spec.is_some() {
            continue;
        }
        if let (Some(&from_idx), Some(&to_idx)) = (id_to_idx.get(&rel.from), id_to_idx.get(&rel.to))
        {
            graph.add_edge(from_idx, to_idx, rel.rel_type.clone());
        }
    }

    (graph, id_to_idx, idx_to_id)
}

fn weakly_connected_components(
    graph: &DiGraph<String, String>,
    idx_to_id: &HashMap<NodeIndex, String>,
) -> Vec<Vec<String>> {
    let undirected = petgraph::algo::condensation(graph.clone(), false);
    // Use Tarjan-based approach: convert to undirected and find components
    let mut visited = HashSet::new();
    let mut components = Vec::new();

    for node_idx in graph.node_indices() {
        if visited.contains(&node_idx) {
            continue;
        }
        let mut component = Vec::new();
        let mut stack = vec![node_idx];
        while let Some(current) = stack.pop() {
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current);
            if let Some(id) = idx_to_id.get(&current) {
                component.push(id.clone());
            }
            // Follow both outgoing and incoming edges (weakly connected)
            for neighbor in graph.neighbors_directed(current, Direction::Outgoing) {
                if !visited.contains(&neighbor) {
                    stack.push(neighbor);
                }
            }
            for neighbor in graph.neighbors_directed(current, Direction::Incoming) {
                if !visited.contains(&neighbor) {
                    stack.push(neighbor);
                }
            }
        }
        component.sort();
        components.push(component);
    }

    let _ = undirected; // suppress unused warning
    components.sort_by_key(|c| std::cmp::Reverse(c.len()));
    components
}

fn find_isolated_nodes(
    graph: &DiGraph<String, String>,
    idx_to_id: &HashMap<NodeIndex, String>,
) -> Vec<String> {
    let mut isolated = Vec::new();
    for node_idx in graph.node_indices() {
        let in_d = graph
            .neighbors_directed(node_idx, Direction::Incoming)
            .count();
        let out_d = graph
            .neighbors_directed(node_idx, Direction::Outgoing)
            .count();
        if in_d == 0
            && out_d == 0
            && let Some(id) = idx_to_id.get(&node_idx)
        {
            isolated.push(id.clone());
        }
    }
    isolated.sort();
    isolated
}

fn find_hub_nodes(
    graph: &DiGraph<String, String>,
    idx_to_id: &HashMap<NodeIndex, String>,
    top_n: usize,
) -> Vec<HubNode> {
    let mut degrees: Vec<_> = graph
        .node_indices()
        .filter_map(|idx| {
            let id = idx_to_id.get(&idx)?;
            let in_d = graph.neighbors_directed(idx, Direction::Incoming).count();
            let out_d = graph.neighbors_directed(idx, Direction::Outgoing).count();
            Some(HubNode {
                id: id.clone(),
                in_degree: in_d,
                out_degree: out_d,
                total_degree: in_d + out_d,
            })
        })
        .collect();

    degrees.sort_by(|a, b| b.total_degree.cmp(&a.total_degree));
    degrees.truncate(top_n);
    degrees
}

fn find_bridge_nodes(
    graph: &DiGraph<String, String>,
    idx_to_id: &HashMap<NodeIndex, String>,
    top_n: usize,
) -> Vec<BridgeNode> {
    // Simple betweenness centrality approximation
    let nodes: Vec<NodeIndex> = graph.node_indices().collect();
    let n = nodes.len();
    let mut centrality: HashMap<NodeIndex, f64> = HashMap::new();

    for &source in &nodes {
        // BFS from source
        let mut stack = Vec::new();
        let mut predecessors: HashMap<NodeIndex, Vec<NodeIndex>> = HashMap::new();
        let mut sigma: HashMap<NodeIndex, f64> = HashMap::new();
        let mut dist: HashMap<NodeIndex, i64> = HashMap::new();

        sigma.insert(source, 1.0);
        dist.insert(source, 0);
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(source);

        while let Some(v) = queue.pop_front() {
            stack.push(v);
            let d_v = dist[&v];
            for neighbor in graph.neighbors_directed(v, Direction::Outgoing) {
                if let std::collections::hash_map::Entry::Vacant(entry) = dist.entry(neighbor) {
                    queue.push_back(neighbor);
                    entry.insert(d_v + 1);
                }
                if dist[&neighbor] == d_v + 1 {
                    *sigma.entry(neighbor).or_insert(0.0) += sigma[&v];
                    predecessors.entry(neighbor).or_default().push(v);
                }
            }
        }

        let mut delta: HashMap<NodeIndex, f64> = HashMap::new();
        while let Some(w) = stack.pop() {
            if let Some(preds) = predecessors.get(&w) {
                for &v in preds {
                    let d = sigma.get(&v).copied().unwrap_or(0.0)
                        / sigma.get(&w).copied().unwrap_or(1.0)
                        * (1.0 + delta.get(&w).copied().unwrap_or(0.0));
                    *delta.entry(v).or_insert(0.0) += d;
                }
            }
            if w != source {
                *centrality.entry(w).or_insert(0.0) += delta.get(&w).copied().unwrap_or(0.0);
            }
        }
    }

    // Normalize
    #[allow(clippy::cast_precision_loss)]
    let norm = if n > 2 {
        1.0 / ((n - 1) as f64 * (n - 2) as f64)
    } else {
        1.0
    };

    let mut bridges: Vec<BridgeNode> = centrality
        .into_iter()
        .filter(|(_, c)| *c > 0.0)
        .filter_map(|(idx, c)| {
            let id = idx_to_id.get(&idx)?;
            Some(BridgeNode {
                id: id.clone(),
                centrality: c * norm,
            })
        })
        .collect();

    bridges.sort_by(|a, b| {
        b.centrality
            .partial_cmp(&a.centrality)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    bridges.truncate(top_n);
    bridges
}

fn find_cycles(
    graph: &DiGraph<String, String>,
    idx_to_id: &HashMap<NodeIndex, String>,
    acyclic_types: &HashSet<String>,
) -> (Vec<Cycle>, usize) {
    // Use petgraph's SCC to find cycles
    let sccs = petgraph::algo::tarjan_scc(graph);
    let mut cycles = Vec::new();
    let mut violations = 0;

    for scc in &sccs {
        if scc.len() <= 1 {
            // Check self-loop
            if scc.len() == 1 {
                let idx = scc[0];
                if graph.contains_edge(idx, idx) {
                    let node_ids: Vec<String> = scc
                        .iter()
                        .filter_map(|i| idx_to_id.get(i).cloned())
                        .collect();
                    let mut edge_types = HashSet::new();
                    for e in graph.edges(idx) {
                        if e.target() == idx {
                            edge_types.insert(e.weight().clone());
                        }
                    }
                    let has_violation = !edge_types.is_disjoint(acyclic_types);
                    if has_violation {
                        violations += 1;
                    }
                    cycles.push(Cycle {
                        nodes: node_ids,
                        edge_types,
                        has_acyclic_violation: has_violation,
                    });
                }
            }
            continue;
        }

        // SCC with multiple nodes implies cycles
        let node_ids: Vec<String> = scc
            .iter()
            .filter_map(|i| idx_to_id.get(i).cloned())
            .collect();

        let mut edge_types = HashSet::new();
        let scc_set: HashSet<_> = scc.iter().copied().collect();
        for &idx in scc {
            for e in graph.edges(idx) {
                if scc_set.contains(&e.target()) {
                    edge_types.insert(e.weight().clone());
                }
            }
        }

        let has_violation = !edge_types.is_disjoint(acyclic_types);
        if has_violation {
            violations += 1;
        }
        cycles.push(Cycle {
            nodes: node_ids,
            edge_types,
            has_acyclic_violation: has_violation,
        });
    }

    (cycles, violations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn spec_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../clayers/clayers")
            .canonicalize()
            .expect("clayers/clayers/ not found")
    }

    #[test]
    fn shipped_spec_has_sufficient_nodes_and_edges() {
        let report = analyze_connectivity(&spec_dir()).expect("analysis failed");
        assert!(
            report.node_count >= 50,
            "expected 50+ nodes, got {}",
            report.node_count
        );
        assert!(
            report.edge_count >= 30,
            "expected 30+ edges, got {}",
            report.edge_count
        );
    }

    #[test]
    fn shipped_spec_reports_components() {
        let report = analyze_connectivity(&spec_dir()).expect("analysis failed");
        assert!(
            !report.components.is_empty(),
            "should have at least one component"
        );
    }

    #[test]
    fn shipped_spec_identifies_isolated_nodes() {
        let report = analyze_connectivity(&spec_dir()).expect("analysis failed");
        // The spec has some isolated nodes (plan items, criteria, etc.)
        // Just verify we detect some
        assert!(
            report.isolated_nodes.len() >= 2,
            "expected some isolated nodes, got {}",
            report.isolated_nodes.len()
        );
    }

    #[test]
    fn shipped_spec_has_hub_nodes() {
        let report = analyze_connectivity(&spec_dir()).expect("analysis failed");
        assert!(!report.hub_nodes.is_empty(), "should identify hub nodes");
        // layered-architecture should be among the top hubs
        let hub_ids: Vec<&str> = report.hub_nodes.iter().map(|h| h.id.as_str()).collect();
        assert!(
            hub_ids.contains(&"layered-architecture"),
            "layered-architecture should be a hub, top hubs: {hub_ids:?}"
        );
    }

    #[test]
    fn shipped_spec_no_acyclic_violations() {
        let report = analyze_connectivity(&spec_dir()).expect("analysis failed");
        assert_eq!(
            report.acyclic_violations, 0,
            "shipped spec should have no acyclic violations"
        );
    }

    #[test]
    fn synthetic_cycle_detected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let index_xml = r#"<?xml version="1.0"?>
<spec:clayers xmlns:spec="urn:clayers:spec"
              xmlns:idx="urn:clayers:index"
              xmlns:pr="urn:clayers:prose"
              xmlns:rel="urn:clayers:relation">
  <idx:file href="content.xml"/>
</spec:clayers>"#;
        std::fs::write(dir.path().join("index.xml"), index_xml).expect("write");

        let content_xml = r#"<?xml version="1.0"?>
<spec:clayers xmlns:spec="urn:clayers:spec"
              xmlns:pr="urn:clayers:prose"
              xmlns:rel="urn:clayers:relation"
              spec:index="index.xml">
  <pr:section id="a"><pr:title>A</pr:title></pr:section>
  <pr:section id="b"><pr:title>B</pr:title></pr:section>
  <rel:relation type="references" from="a" to="b"/>
  <rel:relation type="references" from="b" to="a"/>
</spec:clayers>"#;
        std::fs::write(dir.path().join("content.xml"), content_xml).expect("write");

        let report = analyze_connectivity(dir.path()).expect("analysis failed");
        assert!(
            !report.cycles.is_empty(),
            "should detect the cycle between a and b"
        );
    }
}

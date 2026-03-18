//! XSD-to-RNC conversion: pure XML concern, domain-agnostic.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use xot::{NameId, Node, Xot};

use super::model::{
    RncAttribute, RncBodyItem, RncElement, RncEnumSummary, RncGlobalElement, RncLayer,
    RncNamespace, RncPattern, RncQuantifier, RncSchema,
};

/// Map XSD type string to RNC type notation.
fn rnc_type(xsd_type: &str) -> String {
    let t = xsd_type
        .strip_prefix("xs:")
        .or_else(|| xsd_type.strip_prefix("xsd:"))
        .unwrap_or(xsd_type);
    match t {
        "string" => "text".to_string(),
        "ID" => "xsd:ID".to_string(),
        "IDREF" => "xsd:IDREF".to_string(),
        "anyURI" => "xsd:anyURI".to_string(),
        "boolean" => "xsd:boolean".to_string(),
        "date" => "xsd:date".to_string(),
        "dateTime" => "xsd:dateTime".to_string(),
        "positiveInteger" => "xsd:positiveInteger".to_string(),
        "nonNegativeInteger" => "xsd:nonNegativeInteger".to_string(),
        other => other.to_string(),
    }
}

/// Determine quantifier from `minOccurs`/`maxOccurs` attributes.
fn rnc_quantifier(
    xot: &Xot,
    elem: Node,
    min_occurs: NameId,
    max_occurs: NameId,
) -> RncQuantifier {
    let min_occ = xot.get_attribute(elem, min_occurs).unwrap_or("1");
    let max_occ = xot.get_attribute(elem, max_occurs).unwrap_or("1");
    if max_occ == "unbounded" {
        if min_occ == "0" {
            RncQuantifier::ZeroOrMore
        } else {
            RncQuantifier::OneOrMore
        }
    } else if min_occ == "0" {
        RncQuantifier::Optional
    } else {
        RncQuantifier::One
    }
}

type TypeKey = (String, String);

/// Split a possibly-prefixed name `pfx:Name` into `(prefix, local)`.
fn split_prefixed<'b>(name: &'b str, default_pfx: &'b str) -> (&'b str, &'b str) {
    name.split_once(':').unwrap_or((default_pfx, name))
}

fn is_el(xot: &Xot, node: Node, name_id: NameId) -> bool {
    xot.is_element(node) && xot.element(node).is_some_and(|e| e.name() == name_id)
}

fn attr(xot: &Xot, node: Node, a: NameId) -> Option<&str> {
    xot.get_attribute(node, a)
}

fn child_elements(xot: &Xot, parent: Node) -> Vec<Node> {
    xot.children(parent)
        .filter(|n| xot.is_element(*n))
        .collect()
}

fn find_child(xot: &Xot, parent: Node, name_id: NameId) -> Option<Node> {
    xot.children(parent).find(|n| is_el(xot, *n, name_id))
}

/// Pre-registered XSD element/attribute `NameId` values.
struct XsdNames {
    complex_type: NameId,
    simple_type: NameId,
    element: NameId,
    attribute: NameId,
    attribute_group: NameId,
    sequence: NameId,
    choice: NameId,
    any: NameId,
    restriction: NameId,
    enumeration: NameId,
    pattern: NameId,
    extension: NameId,
    simple_content: NameId,
    complex_content: NameId,
    name_attr: NameId,
    type_attr: NameId,
    ref_attr: NameId,
    use_attr: NameId,
    default_attr: NameId,
    base_attr: NameId,
    mixed_attr: NameId,
    target_namespace_attr: NameId,
    min_occurs_attr: NameId,
    max_occurs_attr: NameId,
    value_attr: NameId,
}

impl XsdNames {
    fn register(xot: &mut Xot) -> Self {
        let xs_ns = xot.add_namespace("http://www.w3.org/2001/XMLSchema");
        Self {
            complex_type: xot.add_name_ns("complexType", xs_ns),
            simple_type: xot.add_name_ns("simpleType", xs_ns),
            element: xot.add_name_ns("element", xs_ns),
            attribute: xot.add_name_ns("attribute", xs_ns),
            attribute_group: xot.add_name_ns("attributeGroup", xs_ns),
            sequence: xot.add_name_ns("sequence", xs_ns),
            choice: xot.add_name_ns("choice", xs_ns),
            any: xot.add_name_ns("any", xs_ns),
            restriction: xot.add_name_ns("restriction", xs_ns),
            enumeration: xot.add_name_ns("enumeration", xs_ns),
            pattern: xot.add_name_ns("pattern", xs_ns),
            extension: xot.add_name_ns("extension", xs_ns),
            simple_content: xot.add_name_ns("simpleContent", xs_ns),
            complex_content: xot.add_name_ns("complexContent", xs_ns),
            name_attr: xot.add_name("name"),
            type_attr: xot.add_name("type"),
            ref_attr: xot.add_name("ref"),
            use_attr: xot.add_name("use"),
            default_attr: xot.add_name("default"),
            base_attr: xot.add_name("base"),
            mixed_attr: xot.add_name("mixed"),
            target_namespace_attr: xot.add_name("targetNamespace"),
            min_occurs_attr: xot.add_name("minOccurs"),
            max_occurs_attr: xot.add_name("maxOccurs"),
            value_attr: xot.add_name("value"),
        }
    }
}

/// Internal context for XSD-to-RNC emission.
struct Ctx {
    types: HashMap<TypeKey, Node>,
    simple_types: HashMap<TypeKey, Node>,
    attr_groups: HashMap<TypeKey, Node>,
    named_types: HashSet<TypeKey>,
    emitted_names: HashSet<TypeKey>,
    expanding: HashSet<TypeKey>,
    n: XsdNames,
}

impl Ctx {
    fn resolve_type(&self, type_ref: &str, default_pfx: &str) -> (Option<Node>, Option<Node>) {
        let (p, local) = split_prefixed(type_ref, default_pfx);
        let key = (p.to_string(), local.to_string());
        let fallback = (default_pfx.to_string(), local.to_string());
        let ct = self
            .types
            .get(&key)
            .or_else(|| self.types.get(&fallback))
            .copied();
        let st = self
            .simple_types
            .get(&key)
            .or_else(|| self.simple_types.get(&fallback))
            .copied();
        (ct, st)
    }

    fn type_rnc(&self, xot: &Xot, type_ref: &str, default_pfx: &str) -> String {
        let (_ct, st) = self.resolve_type(type_ref, default_pfx);
        if let Some(st_node) = st {
            if let Some(vals) = self.resolve_enum(xot, st_node) {
                return vals
                    .iter()
                    .map(|v| format!("\"{v}\""))
                    .collect::<Vec<_>>()
                    .join(" | ");
            }
            if let Some(pat) = self.resolve_pattern(xot, st_node) {
                return format!("text  # pattern: {pat}");
            }
        }
        rnc_type(type_ref)
    }

    fn resolve_enum(&self, xot: &Xot, st_node: Node) -> Option<Vec<String>> {
        let restriction = find_child(xot, st_node, self.n.restriction)?;
        let enums: Vec<String> = child_elements(xot, restriction)
            .into_iter()
            .filter(|n| is_el(xot, *n, self.n.enumeration))
            .filter_map(|n| attr(xot, n, self.n.value_attr).map(String::from))
            .collect();
        if enums.is_empty() {
            None
        } else {
            Some(enums)
        }
    }

    fn resolve_pattern(&self, xot: &Xot, st_node: Node) -> Option<String> {
        let restriction = find_child(xot, st_node, self.n.restriction)?;
        let pat_node = find_child(xot, restriction, self.n.pattern)?;
        attr(xot, pat_node, self.n.value_attr).map(String::from)
    }

    fn emit_attrs(&self, xot: &Xot, parent: Node, pfx: &str) -> Vec<RncBodyItem> {
        let mut attrs = Vec::new();
        for child in child_elements(xot, parent) {
            if is_el(xot, child, self.n.attribute) {
                let name = attr(xot, child, self.n.name_attr);
                let ref_val = attr(xot, child, self.n.ref_attr);
                let use_val = attr(xot, child, self.n.use_attr).unwrap_or("optional");
                let default = attr(xot, child, self.n.default_attr).map(String::from);
                let quantifier = if use_val == "required" {
                    RncQuantifier::One
                } else {
                    RncQuantifier::Optional
                };

                if let Some(ref_val) = ref_val {
                    let (rp, rn) = split_prefixed(ref_val, pfx);
                    attrs.push(RncBodyItem::Attribute(RncAttribute {
                        name: format!("{rp}:{rn}"),
                        type_str: "text".to_string(),
                        quantifier,
                        default,
                    }));
                } else if let Some(name) = name {
                    let type_ref = attr(xot, child, self.n.type_attr).unwrap_or("xs:string");
                    let atype = self.type_rnc(xot, type_ref, pfx);
                    attrs.push(RncBodyItem::Attribute(RncAttribute {
                        name: name.to_string(),
                        type_str: atype,
                        quantifier,
                        default,
                    }));
                }
            } else if is_el(xot, child, self.n.attribute_group)
                && let Some(ref_val) = attr(xot, child, self.n.ref_attr)
            {
                let (rpfx, rlocal) = split_prefixed(ref_val, pfx);
                let key = (rpfx.to_string(), rlocal.to_string());
                if let Some(&ag_node) = self.attr_groups.get(&key) {
                    attrs.extend(self.emit_attrs(xot, ag_node, rpfx));
                }
            }
        }
        attrs
    }

    #[allow(clippy::too_many_lines)]
    fn emit_child(&mut self, xot: &Xot, elem: Node, pfx: &str) -> RncBodyItem {
        let name = attr(xot, elem, self.n.name_attr).map(String::from);
        let ref_val = attr(xot, elem, self.n.ref_attr).map(String::from);
        let q = rnc_quantifier(xot, elem, self.n.min_occurs_attr, self.n.max_occurs_attr);

        if let Some(ref_val) = ref_val {
            return if q == RncQuantifier::One {
                RncBodyItem::PatternRef(ref_val)
            } else {
                let (rp, rn) = split_prefixed(&ref_val, pfx);
                RncBodyItem::Element(RncElement {
                    prefix: rp.to_string(),
                    name: rn.to_string(),
                    body: vec![RncBodyItem::PatternRef(ref_val.clone())],
                    quantifier: q,
                })
            };
        }

        let Some(name) = name else {
            return RncBodyItem::Empty;
        };
        let ns_pfx = pfx.to_string();
        let type_ref = attr(xot, elem, self.n.type_attr).map(String::from);

        if let Some(type_ref) = type_ref {
            return self.emit_typed_child(xot, pfx, &ns_pfx, &name, &type_ref, q);
        }

        // Inline complexType.
        if let Some(ct_node) = find_child(xot, elem, self.n.complex_type) {
            let body = self.emit_body(xot, ct_node, pfx);
            return RncBodyItem::Element(RncElement {
                prefix: ns_pfx,
                name,
                body: if body.is_empty() {
                    vec![RncBodyItem::Empty]
                } else {
                    body
                },
                quantifier: q,
            });
        }

        RncBodyItem::Element(RncElement {
            prefix: ns_pfx,
            name,
            body: vec![RncBodyItem::Type("text".to_string())],
            quantifier: q,
        })
    }

    fn emit_typed_child(
        &mut self,
        xot: &Xot,
        pfx: &str,
        ns_pfx: &str,
        name: &str,
        type_ref: &str,
        q: RncQuantifier,
    ) -> RncBodyItem {
        let (ct, st) = self.resolve_type(type_ref, pfx);
        if let Some(ct_node) = ct {
            let type_local = split_prefixed(type_ref, pfx).1.to_string();
            let type_pfx = split_prefixed(type_ref, pfx).0.to_string();
            let type_key = (type_pfx, type_local.clone());

            if self.named_types.contains(&type_key) {
                return RncBodyItem::Element(RncElement {
                    prefix: ns_pfx.to_string(),
                    name: name.to_string(),
                    body: vec![RncBodyItem::PatternRef(type_local)],
                    quantifier: q,
                });
            }
            if self.expanding.contains(&type_key) {
                return RncBodyItem::Element(RncElement {
                    prefix: ns_pfx.to_string(),
                    name: name.to_string(),
                    body: vec![RncBodyItem::Empty],
                    quantifier: q,
                });
            }
            self.expanding.insert(type_key.clone());
            let body = self.emit_body(xot, ct_node, pfx);
            self.expanding.remove(&type_key);
            return RncBodyItem::Element(RncElement {
                prefix: ns_pfx.to_string(),
                name: name.to_string(),
                body,
                quantifier: q,
            });
        }
        if let Some(st_node) = st {
            if let Some(vals) = self.resolve_enum(xot, st_node) {
                return RncBodyItem::Element(RncElement {
                    prefix: ns_pfx.to_string(),
                    name: name.to_string(),
                    body: vec![RncBodyItem::InlineEnum(vals)],
                    quantifier: q,
                });
            }
            return RncBodyItem::Element(RncElement {
                prefix: ns_pfx.to_string(),
                name: name.to_string(),
                body: vec![RncBodyItem::Type("text".to_string())],
                quantifier: q,
            });
        }
        RncBodyItem::Element(RncElement {
            prefix: ns_pfx.to_string(),
            name: name.to_string(),
            body: vec![RncBodyItem::Type(rnc_type(type_ref))],
            quantifier: q,
        })
    }

    fn emit_children(&mut self, xot: &Xot, parent: Node, pfx: &str) -> Vec<RncBodyItem> {
        let children: Vec<Node> = child_elements(xot, parent);
        let mut items = Vec::new();
        for child in children {
            if is_el(xot, child, self.n.element) {
                items.push(self.emit_child(xot, child, pfx));
            } else if is_el(xot, child, self.n.sequence) {
                items.extend(self.emit_children(xot, child, pfx));
            } else if is_el(xot, child, self.n.choice) {
                items.push(self.emit_choice(xot, child, pfx));
            } else if is_el(xot, child, self.n.any) {
                let max_occ = attr(xot, child, self.n.max_occurs_attr).unwrap_or("1");
                let q = if max_occ == "unbounded" {
                    RncQuantifier::ZeroOrMore
                } else {
                    RncQuantifier::One
                };
                items.push(RncBodyItem::AnyElement(q));
            }
        }
        items
    }

    fn emit_choice(&mut self, xot: &Xot, child: Node, pfx: &str) -> RncBodyItem {
        let max_occ = attr(xot, child, self.n.max_occurs_attr).unwrap_or("1");
        let min_occ = attr(xot, child, self.n.min_occurs_attr).unwrap_or("1");
        let q = if max_occ == "unbounded" {
            RncQuantifier::ZeroOrMore
        } else if min_occ == "0" {
            RncQuantifier::Optional
        } else {
            RncQuantifier::One
        };
        let mut options = Vec::new();
        for opt in child_elements(xot, child) {
            if is_el(xot, opt, self.n.element) {
                options.push(self.emit_child(xot, opt, pfx));
            }
        }
        RncBodyItem::Choice {
            options,
            quantifier: q,
        }
    }

    fn emit_body(&mut self, xot: &Xot, ct: Node, pfx: &str) -> Vec<RncBodyItem> {
        let is_mixed = attr(xot, ct, self.n.mixed_attr).is_some_and(|v| v == "true");

        if let Some(sc) = find_child(xot, ct, self.n.simple_content) {
            return self.emit_simple_content(xot, sc, pfx);
        }
        if let Some(cc) = find_child(xot, ct, self.n.complex_content) {
            return self.emit_complex_content(xot, cc, pfx, is_mixed);
        }

        let mut lines = self.emit_attrs(xot, ct, pfx);
        lines.extend(self.emit_children(xot, ct, pfx));
        wrap_mixed(lines, is_mixed)
    }

    fn emit_simple_content(&self, xot: &Xot, sc: Node, pfx: &str) -> Vec<RncBodyItem> {
        if let Some(ext) = find_child(xot, sc, self.n.extension) {
            let mut lines = self.emit_attrs(xot, ext, pfx);
            lines.push(RncBodyItem::Type("text".to_string()));
            return lines;
        }
        vec![RncBodyItem::Type("text".to_string())]
    }

    fn emit_complex_content(
        &mut self,
        xot: &Xot,
        cc: Node,
        pfx: &str,
        parent_mixed: bool,
    ) -> Vec<RncBodyItem> {
        let cc_mixed =
            parent_mixed || attr(xot, cc, self.n.mixed_attr).is_some_and(|v| v == "true");

        if let Some(ext) = find_child(xot, cc, self.n.extension) {
            let mut lines = Vec::new();
            if let Some(base) = attr(xot, ext, self.n.base_attr).map(String::from) {
                let (base_pfx, base_local) = split_prefixed(&base, pfx);
                let type_key = (base_pfx.to_string(), base_local.to_string());
                let fallback = (pfx.to_string(), base_local.to_string());
                let base_ct = self
                    .types
                    .get(&type_key)
                    .or_else(|| self.types.get(&fallback))
                    .copied();
                if let Some(base_ct) = base_ct {
                    if self.named_types.contains(&type_key) {
                        lines.push(RncBodyItem::PatternRef(base_local.to_string()));
                    } else if !self.expanding.contains(&type_key) {
                        self.expanding.insert(type_key.clone());
                        lines.extend(self.emit_body(xot, base_ct, pfx));
                        self.expanding.remove(&type_key);
                    }
                }
            }
            lines.extend(self.emit_attrs(xot, ext, pfx));
            lines.extend(self.emit_children(xot, ext, pfx));
            return wrap_mixed(lines, cc_mixed);
        }

        if cc_mixed {
            vec![RncBodyItem::Type("text".to_string())]
        } else {
            vec![]
        }
    }
}

fn wrap_mixed(lines: Vec<RncBodyItem>, is_mixed: bool) -> Vec<RncBodyItem> {
    if !is_mixed {
        return lines;
    }
    if lines.is_empty() {
        vec![RncBodyItem::Type("text".to_string())]
    } else {
        vec![RncBodyItem::Mixed(lines)]
    }
}

/// Parsed XSD document.
struct ParsedXsd {
    root: Node,
    pfx: String,
    target_ns: String,
    stem: String,
}

/// Extract the xmlns prefix that matches `target_ns` from raw XSD content.
///
/// Looks for `xmlns:PREFIX="URI"` declarations where URI equals `target_ns`.
fn discover_prefix(content: &str, target_ns: &str) -> Option<String> {
    for piece in content.split("xmlns:") {
        // piece looks like: `pr="urn:clayers:prose" ...`
        let Some((name, rest)) = piece.split_once('=') else {
            continue;
        };
        let name = name.trim();
        let rest = rest.trim();
        let quote = rest.as_bytes().first().copied();
        if quote != Some(b'"') && quote != Some(b'\'') {
            continue;
        }
        let qchar = quote.unwrap() as char;
        let rest = &rest[1..];
        let Some(end) = rest.find(qchar) else {
            continue;
        };
        let uri = &rest[..end];
        if uri == target_ns {
            return Some(name.to_string());
        }
    }
    None
}

/// Convert XSD schemas to RNC representation.
///
/// Prefixes are auto-discovered from `xmlns:PREFIX` declarations in each XSD.
/// `skip_ns` is a set of namespace URIs to skip (e.g. combined).
///
/// # Errors
///
/// Returns an error if schema files cannot be read or parsed.
pub fn xsd_to_rnc(
    schema_dir: &Path,
    skip_ns: &[&str],
) -> Result<RncSchema, crate::Error> {
    let mut xot = Xot::new();
    let names = XsdNames::register(&mut xot);

    let skip_set: HashSet<&str> = skip_ns.iter().copied().collect();

    let mut xsd_paths: Vec<_> = std::fs::read_dir(schema_dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|ext| ext == "xsd"))
        .collect();
    xsd_paths.sort();

    // Parse all XSD files upfront so nodes remain valid.
    let parsed = parse_all(&mut xot, &xsd_paths, &names, &skip_set)?;

    // Build type registries.
    let (types, simple_types, attr_groups) = build_registries(&xot, &parsed, &names);

    // Count usage and detect recursion.
    let named_types = find_named_types(&xot, &parsed, &types, &names);

    // Build schema.
    let discovered: Vec<(String, String)> = parsed
        .iter()
        .map(|p| (p.pfx.clone(), p.target_ns.clone()))
        .collect();
    let mut schema = build_schema_header(&discovered);

    let mut ctx = Ctx {
        types: types.clone(),
        simple_types: simple_types.clone(),
        attr_groups,
        named_types,
        emitted_names: HashSet::new(),
        expanding: HashSet::new(),
        n: names,
    };

    for p in &parsed {
        let layer = emit_layer(&xot, &mut ctx, p, &simple_types);
        schema.layers.push(layer);
    }

    Ok(schema)
}

fn parse_all(
    xot: &mut Xot,
    xsd_paths: &[std::path::PathBuf],
    names: &XsdNames,
    skip_set: &HashSet<&str>,
) -> Result<Vec<ParsedXsd>, crate::Error> {
    let mut parsed = Vec::new();
    for xsd_path in xsd_paths {
        let content = std::fs::read_to_string(xsd_path)?;
        let doc = xot.parse(&content).map_err(xot::Error::from)?;
        let root = xot.document_element(doc)?;
        let tns = attr(xot, root, names.target_namespace_attr)
            .unwrap_or("")
            .to_string();
        if tns.is_empty() || skip_set.contains(tns.as_str()) {
            continue;
        }
        let Some(pfx) = discover_prefix(&content, &tns) else {
            continue;
        };
        let stem = xsd_path
            .file_stem()
            .map_or_else(|| "unknown".to_string(), |s| s.to_string_lossy().to_string());
        parsed.push(ParsedXsd { root, pfx, target_ns: tns, stem });
    }
    Ok(parsed)
}

#[allow(clippy::type_complexity)]
fn build_registries(
    xot: &Xot,
    parsed: &[ParsedXsd],
    names: &XsdNames,
) -> (
    HashMap<TypeKey, Node>,
    HashMap<TypeKey, Node>,
    HashMap<TypeKey, Node>,
) {
    let mut types = HashMap::new();
    let mut simple_types = HashMap::new();
    let mut attr_groups = HashMap::new();

    for p in parsed {
        for child in xot.children(p.root) {
            if !xot.is_element(child) {
                continue;
            }
            let Some(el) = xot.element(child) else {
                continue;
            };
            let child_name = el.name();
            let name_val = xot.get_attribute(child, names.name_attr).map(String::from);

            if child_name == names.complex_type {
                if let Some(n) = name_val {
                    types.insert((p.pfx.clone(), n), child);
                }
            } else if child_name == names.simple_type {
                if let Some(n) = name_val {
                    simple_types.insert((p.pfx.clone(), n), child);
                }
            } else if child_name == names.attribute_group
                && let Some(n) = &name_val
            {
                let has_attr = xot
                    .children(child)
                    .any(|c| xot.element(c).is_some_and(|e| e.name() == names.attribute));
                if has_attr {
                    attr_groups.insert((p.pfx.clone(), n.clone()), child);
                }
            }
        }
    }

    (types, simple_types, attr_groups)
}

fn find_named_types(
    xot: &Xot,
    parsed: &[ParsedXsd],
    types: &HashMap<TypeKey, Node>,
    names: &XsdNames,
) -> HashSet<TypeKey> {
    let mut type_usage: HashMap<TypeKey, usize> = HashMap::new();

    for p in parsed {
        count_type_refs_recursive(xot, p.root, p.root, &p.pfx, names, &mut type_usage);
    }
    for ((tp, _), ct_node) in types {
        count_extension_bases(xot, *ct_node, tp, names, &mut type_usage);
    }
    for ((tp, tl), ct_node) in types {
        if has_self_reference(xot, *ct_node, tl, names) {
            *type_usage.entry((tp.clone(), tl.clone())).or_insert(0) += 2;
        }
    }
    detect_mutual_recursion(xot, types, names, &mut type_usage);

    type_usage
        .into_iter()
        .filter(|(_, count)| *count >= 2)
        .map(|(k, _)| k)
        .collect()
}

fn build_schema_header(discovered: &[(String, String)]) -> RncSchema {
    let mut schema = RncSchema {
        header_comments: vec![
            "Clayers Schema \u{2013} RELAX NG Compact Notation".to_string(),
            "Auto-generated from XSD. For LLM/agent consumption.".to_string(),
            "All files share root: <spec:clayers>".to_string(),
        ],
        namespaces: Vec::new(),
        layers: Vec::new(),
    };
    let mut ns_pairs: Vec<&(String, String)> = discovered.iter().collect();
    ns_pairs.sort_by_key(|(pfx, _)| pfx.clone());
    for (pfx, uri) in ns_pairs {
        schema.namespaces.push(RncNamespace {
            prefix: pfx.clone(),
            uri: uri.clone(),
        });
    }
    schema
}

fn emit_layer(
    xot: &Xot,
    ctx: &mut Ctx,
    p: &ParsedXsd,
    simple_types: &HashMap<TypeKey, Node>,
) -> RncLayer {
    let pfx = &p.pfx;
    let mut layer = RncLayer {
        name: p.stem.clone(),
        prefix: pfx.clone(),
        description: None,
        patterns: Vec::new(),
        elements: Vec::new(),
        enum_summaries: Vec::new(),
    };

    // Named patterns.
    let pattern_keys: Vec<TypeKey> = {
        let mut keys: Vec<TypeKey> = ctx
            .types
            .keys()
            .filter(|(tp, _)| tp == pfx)
            .filter(|k| ctx.named_types.contains(*k) && !ctx.emitted_names.contains(*k))
            .cloned()
            .collect();
        keys.sort();
        keys
    };
    for key in &pattern_keys {
        let ct_node = ctx.types[key];
        ctx.expanding.insert(key.clone());
        let tbody = ctx.emit_body(xot, ct_node, pfx);
        ctx.expanding.remove(key);
        layer.patterns.push(RncPattern {
            name: key.1.clone(),
            body: tbody,
            description: None,
        });
        ctx.emitted_names.insert(key.clone());
    }

    // Global elements.
    let root_children: Vec<Node> = xot
        .children(p.root)
        .filter(|n| is_el(xot, *n, ctx.n.element))
        .collect();
    for child in root_children {
        if let Some(ge) = emit_global_element(xot, ctx, child, pfx) {
            layer.elements.push(ge);
        }
    }

    // Enum summaries.
    let mut st_keys: Vec<&TypeKey> = simple_types.keys().collect();
    st_keys.sort();
    let mut emitted_enums = HashSet::new();
    for key in st_keys {
        if key.0 != *pfx || emitted_enums.contains(&key.1) {
            continue;
        }
        if let Some(vals) = ctx.resolve_enum(xot, simple_types[key]) {
            layer.enum_summaries.push(RncEnumSummary {
                type_name: key.1.clone(),
                values: vals,
            });
            emitted_enums.insert(key.1.clone());
        }
    }

    layer
}

fn emit_global_element(
    xot: &Xot,
    ctx: &mut Ctx,
    child: Node,
    pfx: &str,
) -> Option<RncGlobalElement> {
    let name = attr(xot, child, ctx.n.name_attr)?.to_string();
    let type_ref = attr(xot, child, ctx.n.type_attr).map(String::from);

    let body = if let Some(type_ref) = type_ref {
        resolve_global_element_body(xot, ctx, pfx, &type_ref)
    } else {
        let inline_ct = find_child(xot, child, ctx.n.complex_type);
        if let Some(ct_node) = inline_ct {
            let b = ctx.emit_body(xot, ct_node, pfx);
            if b.is_empty() {
                vec![RncBodyItem::Empty]
            } else {
                b
            }
        } else {
            vec![RncBodyItem::Type("text".to_string())]
        }
    };

    let body = if body.is_empty() {
        vec![RncBodyItem::Empty]
    } else {
        body
    };

    Some(RncGlobalElement {
        prefix: pfx.to_string(),
        name,
        body,
        description: None,
    })
}

fn resolve_global_element_body(
    xot: &Xot,
    ctx: &mut Ctx,
    pfx: &str,
    type_ref: &str,
) -> Vec<RncBodyItem> {
    let (ct, st) = ctx.resolve_type(type_ref, pfx);
    if let Some(ct_node) = ct {
        let type_local = split_prefixed(type_ref, pfx).1.to_string();
        let type_key = (pfx.to_string(), type_local.clone());
        if ctx.named_types.contains(&type_key) {
            return vec![RncBodyItem::PatternRef(type_local)];
        }
        ctx.expanding.insert(type_key.clone());
        let b = ctx.emit_body(xot, ct_node, pfx);
        ctx.expanding.remove(&type_key);
        return b;
    }
    if let Some(st_node) = st {
        if let Some(vals) = ctx.resolve_enum(xot, st_node) {
            return vec![RncBodyItem::InlineEnum(vals)];
        }
        if let Some(pat) = ctx.resolve_pattern(xot, st_node) {
            return vec![RncBodyItem::PatternedText(pat)];
        }
        return vec![RncBodyItem::Type(rnc_type(type_ref))];
    }
    vec![RncBodyItem::Type(rnc_type(type_ref))]
}

// --- Helper functions for type counting and recursion detection ---

fn count_type_refs_recursive(
    xot: &Xot,
    node: Node,
    root: Node,
    pfx: &str,
    names: &XsdNames,
    usage: &mut HashMap<TypeKey, usize>,
) {
    for child in xot.children(node) {
        if !xot.is_element(child) {
            continue;
        }
        let Some(el) = xot.element(child) else {
            continue;
        };
        if el.name() == names.element {
            let is_global = xot.parent(child).is_some_and(|p| p == root);
            if !is_global
                && let Some(type_ref) = xot.get_attribute(child, names.type_attr)
            {
                let (tp, tl) = split_prefixed(type_ref, pfx);
                *usage.entry((tp.to_string(), tl.to_string())).or_insert(0) += 1;
            }
        }
        count_type_refs_recursive(xot, child, root, pfx, names, usage);
    }
}

fn count_extension_bases(
    xot: &Xot,
    node: Node,
    pfx: &str,
    names: &XsdNames,
    usage: &mut HashMap<TypeKey, usize>,
) {
    for child in xot.children(node) {
        if !xot.is_element(child) {
            continue;
        }
        if xot
            .element(child)
            .is_some_and(|e| e.name() == names.extension)
            && let Some(base) = xot.get_attribute(child, names.base_attr)
        {
            let (bp, bl) = split_prefixed(base, pfx);
            *usage.entry((bp.to_string(), bl.to_string())).or_insert(0) += 1;
        }
        count_extension_bases(xot, child, pfx, names, usage);
    }
}

fn has_self_reference(xot: &Xot, node: Node, type_name: &str, names: &XsdNames) -> bool {
    for child in xot.children(node) {
        if !xot.is_element(child) {
            continue;
        }
        if xot
            .element(child)
            .is_some_and(|e| e.name() == names.element)
            && let Some(type_ref) = xot.get_attribute(child, names.type_attr)
        {
            let local = split_prefixed(type_ref, "").1;
            if local == type_name {
                return true;
            }
        }
        if has_self_reference(xot, child, type_name, names) {
            return true;
        }
    }
    false
}

fn detect_mutual_recursion(
    xot: &Xot,
    types: &HashMap<TypeKey, Node>,
    names: &XsdNames,
    usage: &mut HashMap<TypeKey, usize>,
) {
    let mut refs: HashMap<TypeKey, HashSet<TypeKey>> = HashMap::new();
    for (key, &ct_node) in types {
        let mut referenced = HashSet::new();
        collect_type_refs(xot, ct_node, &key.0, names, &mut referenced);
        refs.insert(key.clone(), referenced);
    }
    let keys: Vec<TypeKey> = refs.keys().cloned().collect();
    for a in &keys {
        if let Some(a_refs) = refs.get(a) {
            for b in a_refs {
                if b != a
                    && refs
                        .get(b)
                        .is_some_and(|b_refs| b_refs.contains(a))
                {
                    *usage.entry(a.clone()).or_insert(0) += 2;
                    *usage.entry(b.clone()).or_insert(0) += 2;
                }
            }
        }
    }
}

fn collect_type_refs(
    xot: &Xot,
    node: Node,
    default_pfx: &str,
    names: &XsdNames,
    out: &mut HashSet<TypeKey>,
) {
    for child in xot.children(node) {
        if !xot.is_element(child) {
            continue;
        }
        if xot
            .element(child)
            .is_some_and(|e| e.name() == names.element)
            && let Some(type_ref) = xot.get_attribute(child, names.type_attr)
        {
            let (tp, tl) = split_prefixed(type_ref, default_pfx);
            out.insert((tp.to_string(), tl.to_string()));
        }
        if xot
            .element(child)
            .is_some_and(|e| e.name() == names.extension)
            && let Some(base) = xot.get_attribute(child, names.base_attr)
        {
            let (bp, bl) = split_prefixed(base, default_pfx);
            out.insert((bp.to_string(), bl.to_string()));
        }
        collect_type_refs(xot, child, default_pfx, names, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rnc_type_mapping() {
        assert_eq!(rnc_type("xs:string"), "text");
        assert_eq!(rnc_type("xs:ID"), "xsd:ID");
        assert_eq!(rnc_type("xs:boolean"), "xsd:boolean");
        assert_eq!(rnc_type("xs:anyURI"), "xsd:anyURI");
        assert_eq!(rnc_type("SomeCustomType"), "SomeCustomType");
    }

    #[test]
    fn split_prefixed_with_colon() {
        let (p, l) = split_prefixed("pr:SectionType", "default");
        assert_eq!(p, "pr");
        assert_eq!(l, "SectionType");
    }

    #[test]
    fn split_prefixed_without_colon() {
        let (p, l) = split_prefixed("SectionType", "pr");
        assert_eq!(p, "pr");
        assert_eq!(l, "SectionType");
    }

    #[test]
    fn xsd_to_rnc_minimal() {
        let dir = tempfile::tempdir().unwrap();
        let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           xmlns:foo="urn:test:foo"
           targetNamespace="urn:test:foo"
           elementFormDefault="qualified">
  <xs:element name="bar" type="xs:string"/>
</xs:schema>"#;
        std::fs::write(dir.path().join("foo.xsd"), xsd).unwrap();

        let schema = xsd_to_rnc(dir.path(), &[]).unwrap();
        assert_eq!(schema.layers.len(), 1);
        assert_eq!(schema.layers[0].prefix, "foo");
        assert_eq!(schema.layers[0].elements.len(), 1);
        assert_eq!(schema.layers[0].elements[0].name, "bar");

        let output = schema.to_string();
        assert!(output.contains("namespace foo"));
        assert!(output.contains("foo:bar = element foo:bar { text }"));
    }

    #[test]
    fn xsd_to_rnc_complex_type() {
        let dir = tempfile::tempdir().unwrap();
        let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           xmlns:t="urn:test:types"
           targetNamespace="urn:test:types"
           elementFormDefault="qualified">
  <xs:element name="item" type="t:ItemType"/>
  <xs:complexType name="ItemType">
    <xs:sequence>
      <xs:element name="title" type="xs:string"/>
      <xs:element name="value" type="xs:positiveInteger"/>
    </xs:sequence>
    <xs:attribute name="id" type="xs:ID" use="required"/>
  </xs:complexType>
</xs:schema>"#;
        std::fs::write(dir.path().join("types.xsd"), xsd).unwrap();

        let schema = xsd_to_rnc(dir.path(), &[]).unwrap();
        let output = schema.to_string();
        assert!(output.contains("attribute id { xsd:ID }"));
        assert!(output.contains("element t:title { text }"));
        assert!(output.contains("element t:value { xsd:positiveInteger }"));
    }

    #[test]
    fn xsd_to_rnc_enum_type() {
        let dir = tempfile::tempdir().unwrap();
        let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           xmlns:t="urn:test:enums"
           targetNamespace="urn:test:enums"
           elementFormDefault="qualified">
  <xs:simpleType name="Color">
    <xs:restriction base="xs:string">
      <xs:enumeration value="red"/>
      <xs:enumeration value="green"/>
      <xs:enumeration value="blue"/>
    </xs:restriction>
  </xs:simpleType>
  <xs:element name="paint" type="t:Color"/>
</xs:schema>"#;
        std::fs::write(dir.path().join("enums.xsd"), xsd).unwrap();

        let schema = xsd_to_rnc(dir.path(), &[]).unwrap();
        let output = schema.to_string();
        assert!(output.contains("\"red\" | \"green\" | \"blue\""));
        assert!(output.contains("# Color: red | green | blue"));
    }

    #[test]
    fn xsd_to_rnc_recursive_type() {
        let dir = tempfile::tempdir().unwrap();
        let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           xmlns:t="urn:test:recurse"
           targetNamespace="urn:test:recurse"
           elementFormDefault="qualified">
  <xs:element name="tree" type="t:TreeNode"/>
  <xs:complexType name="TreeNode">
    <xs:sequence>
      <xs:element name="label" type="xs:string"/>
      <xs:element name="child" type="t:TreeNode" minOccurs="0" maxOccurs="unbounded"/>
    </xs:sequence>
  </xs:complexType>
</xs:schema>"#;
        std::fs::write(dir.path().join("recurse.xsd"), xsd).unwrap();

        let schema = xsd_to_rnc(dir.path(), &[]).unwrap();
        let output = schema.to_string();
        assert!(
            output.contains("TreeNode ="),
            "TreeNode should be a named pattern: {output}"
        );
    }

    #[test]
    fn xsd_to_rnc_extension() {
        let dir = tempfile::tempdir().unwrap();
        let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           xmlns:t="urn:test:ext"
           targetNamespace="urn:test:ext"
           elementFormDefault="qualified">
  <xs:complexType name="BaseType">
    <xs:sequence>
      <xs:element name="name" type="xs:string"/>
    </xs:sequence>
  </xs:complexType>
  <xs:complexType name="ExtendedType">
    <xs:complexContent>
      <xs:extension base="t:BaseType">
        <xs:sequence>
          <xs:element name="extra" type="xs:string"/>
        </xs:sequence>
      </xs:extension>
    </xs:complexContent>
  </xs:complexType>
  <xs:element name="ext" type="t:ExtendedType"/>
</xs:schema>"#;
        std::fs::write(dir.path().join("ext.xsd"), xsd).unwrap();

        let schema = xsd_to_rnc(dir.path(), &[]).unwrap();
        let output = schema.to_string();
        assert!(
            output.contains("element t:name { text }"),
            "Missing base element: {output}"
        );
        assert!(
            output.contains("element t:extra { text }"),
            "Missing extension element: {output}"
        );
    }

    #[test]
    fn xsd_to_rnc_mixed_content() {
        let dir = tempfile::tempdir().unwrap();
        let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           xmlns:t="urn:test:mixed"
           targetNamespace="urn:test:mixed"
           elementFormDefault="qualified">
  <xs:complexType name="InlineContent" mixed="true">
    <xs:choice minOccurs="0" maxOccurs="unbounded">
      <xs:element name="b" type="xs:string"/>
      <xs:element name="i" type="xs:string"/>
    </xs:choice>
  </xs:complexType>
  <xs:element name="para" type="t:InlineContent"/>
</xs:schema>"#;
        std::fs::write(dir.path().join("mixed.xsd"), xsd).unwrap();

        let schema = xsd_to_rnc(dir.path(), &[]).unwrap();
        let output = schema.to_string();
        assert!(output.contains("mixed"), "Missing mixed content: {output}");
    }

    #[test]
    fn xsd_to_rnc_skip_namespace() {
        let dir = tempfile::tempdir().unwrap();
        let xsd1 = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           xmlns:k="urn:test:keep"
           targetNamespace="urn:test:keep"
           elementFormDefault="qualified">
  <xs:element name="kept" type="xs:string"/>
</xs:schema>"#;
        let xsd2 = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           xmlns:s="urn:test:skip"
           targetNamespace="urn:test:skip"
           elementFormDefault="qualified">
  <xs:element name="skipped" type="xs:string"/>
</xs:schema>"#;
        std::fs::write(dir.path().join("keep.xsd"), xsd1).unwrap();
        std::fs::write(dir.path().join("skip.xsd"), xsd2).unwrap();

        let schema = xsd_to_rnc(
            dir.path(),
            &["urn:test:skip"],
        )
        .unwrap();
        assert_eq!(schema.layers.len(), 1);
        assert_eq!(schema.layers[0].prefix, "k");
    }

    #[test]
    fn discover_prefix_finds_matching_xmlns() {
        let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           xmlns:pr="urn:clayers:prose"
           targetNamespace="urn:clayers:prose">"#;
        assert_eq!(discover_prefix(xsd, "urn:clayers:prose"), Some("pr".to_string()));
    }

    #[test]
    fn discover_prefix_returns_none_when_no_match() {
        let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="urn:test:foo">"#;
        assert_eq!(discover_prefix(xsd, "urn:test:foo"), None);
    }
}

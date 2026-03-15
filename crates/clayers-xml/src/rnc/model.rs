//! RNC data model: structs and enums encodable to text via `Display`.

use std::fmt;

/// Wrap text into `# ` comment lines at the given width.
#[must_use]
pub fn wrap_comment(text: &str, width: usize) -> Vec<String> {
    let effective = if width > 2 { width - 2 } else { width };
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let paragraph = paragraph.trim();
        if paragraph.is_empty() {
            lines.push("# ".to_string());
            continue;
        }
        let words: Vec<&str> = paragraph.split_whitespace().collect();
        let mut current_line = String::new();
        for word in words {
            if current_line.is_empty() {
                current_line.push_str(word);
            } else if current_line.len() + 1 + word.len() <= effective {
                current_line.push(' ');
                current_line.push_str(word);
            } else {
                lines.push(format!("# {current_line}"));
                current_line = word.to_string();
            }
        }
        if !current_line.is_empty() {
            lines.push(format!("# {current_line}"));
        }
    }
    lines
}

/// Complete RNC document.
#[derive(Debug, Clone)]
pub struct RncSchema {
    pub header_comments: Vec<String>,
    pub namespaces: Vec<RncNamespace>,
    pub layers: Vec<RncLayer>,
}

/// A namespace declaration: `namespace pfx = "uri"`.
#[derive(Debug, Clone)]
pub struct RncNamespace {
    pub prefix: String,
    pub uri: String,
}

/// One layer's definitions.
#[derive(Debug, Clone)]
pub struct RncLayer {
    pub name: String,
    pub prefix: String,
    pub description: Option<String>,
    pub patterns: Vec<RncPattern>,
    pub elements: Vec<RncGlobalElement>,
    pub enum_summaries: Vec<RncEnumSummary>,
}

/// A named pattern: `TypeName = body`.
#[derive(Debug, Clone)]
pub struct RncPattern {
    pub name: String,
    pub body: Vec<RncBodyItem>,
    pub description: Option<String>,
}

/// A global element: `pfx:name = element pfx:name { body }`.
#[derive(Debug, Clone)]
pub struct RncGlobalElement {
    pub prefix: String,
    pub name: String,
    pub body: Vec<RncBodyItem>,
    pub description: Option<String>,
}

/// Enum value summary: `# TypeName: val1 | val2`.
#[derive(Debug, Clone)]
pub struct RncEnumSummary {
    pub type_name: String,
    pub values: Vec<String>,
}

/// A body item in an RNC definition (recursive).
#[derive(Debug, Clone)]
pub enum RncBodyItem {
    Attribute(RncAttribute),
    Element(RncElement),
    PatternRef(String),
    Choice {
        options: Vec<RncBodyItem>,
        quantifier: RncQuantifier,
    },
    Mixed(Vec<RncBodyItem>),
    Type(String),
    Empty,
    AnyElement(RncQuantifier),
    PatternedText(String),
    InlineEnum(Vec<String>),
}

/// An attribute declaration.
#[derive(Debug, Clone)]
pub struct RncAttribute {
    pub name: String,
    pub type_str: String,
    pub quantifier: RncQuantifier,
    pub default: Option<String>,
}

/// A child element declaration.
#[derive(Debug, Clone)]
pub struct RncElement {
    pub prefix: String,
    pub name: String,
    pub body: Vec<RncBodyItem>,
    pub quantifier: RncQuantifier,
}

/// Occurrence quantifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RncQuantifier {
    One,
    Optional,
    ZeroOrMore,
    OneOrMore,
}

// --- Display implementations ---

impl fmt::Display for RncQuantifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::One => Ok(()),
            Self::Optional => f.write_str("?"),
            Self::ZeroOrMore => f.write_str("*"),
            Self::OneOrMore => f.write_str("+"),
        }
    }
}

impl fmt::Display for RncAttribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "attribute {} {{ {} }}{}",
            self.name, self.type_str, self.quantifier
        )?;
        if let Some(ref d) = self.default {
            write!(f, "  # default: {d}")?;
        }
        Ok(())
    }
}

impl fmt::Display for RncElement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ns_name = format!("{}:{}", self.prefix, self.name);
        let inner = format_body_items(&self.body);
        write!(f, "element {ns_name} {{ {inner} }}{}", self.quantifier)
    }
}

impl fmt::Display for RncBodyItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Attribute(a) => write!(f, "{a}"),
            Self::Element(e) => write!(f, "{e}"),
            Self::PatternRef(name) => write!(f, "{name}"),
            Self::Choice {
                options,
                quantifier,
            } => {
                let opts: Vec<String> = options.iter().map(ToString::to_string).collect();
                write!(f, "({}){quantifier}", opts.join(" | "))
            }
            Self::Mixed(items) => {
                let inner = format_body_items(items);
                write!(f, "mixed {{ {inner} }}")
            }
            Self::Type(t) => write!(f, "{t}"),
            Self::Empty => write!(f, "empty"),
            Self::AnyElement(q) => write!(f, "anyElement{q}"),
            Self::PatternedText(pat) => write!(f, "text  # pattern: {pat}"),
            Self::InlineEnum(vals) => {
                let parts: Vec<String> = vals.iter().map(|v| format!("\"{v}\"")).collect();
                write!(f, "{}", parts.join(" | "))
            }
        }
    }
}

fn format_body_items(items: &[RncBodyItem]) -> String {
    items
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

impl fmt::Display for RncEnumSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "# {}: {}", self.type_name, self.values.join(" | "))
    }
}

impl fmt::Display for RncNamespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "namespace {} = \"{}\"", self.prefix, self.uri)
    }
}

impl fmt::Display for RncPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref desc) = self.description {
            for line in wrap_comment(desc, 78) {
                writeln!(f, "{line}")?;
            }
        }
        let body_str = format_body_items(&self.body);
        write!(f, "{} = {body_str}", self.name)
    }
}

impl fmt::Display for RncGlobalElement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref desc) = self.description {
            for line in wrap_comment(desc, 78) {
                writeln!(f, "{line}")?;
            }
        }
        let ns_name = format!("{}:{}", self.prefix, self.name);
        let flat = format_body_items(&self.body);
        if flat.len() < 70 {
            write!(f, "{ns_name} = element {ns_name} {{ {flat} }}")
        } else {
            writeln!(f, "{ns_name} = element {ns_name} {{")?;
            for item in &self.body {
                writeln!(f, "  {item}")?;
            }
            write!(f, "}}")
        }
    }
}

impl fmt::Display for RncLayer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "# {}", "=".repeat(50))?;
        writeln!(f, "# {} LAYER ({}:)", self.name.to_uppercase(), self.prefix)?;
        if let Some(ref desc) = self.description {
            for line in wrap_comment(desc, 78) {
                writeln!(f, "{line}")?;
            }
        }
        writeln!(f)?;

        for pat in &self.patterns {
            writeln!(f, "{pat}")?;
            writeln!(f)?;
        }

        for elem in &self.elements {
            writeln!(f, "{elem}")?;
            writeln!(f)?;
        }

        if !self.enum_summaries.is_empty() {
            for es in &self.enum_summaries {
                writeln!(f, "{es}")?;
            }
            writeln!(f)?;
        }
        Ok(())
    }
}

impl fmt::Display for RncSchema {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for comment in &self.header_comments {
            writeln!(f, "# {comment}")?;
        }
        writeln!(f)?;

        for ns in &self.namespaces {
            writeln!(f, "{ns}")?;
        }
        writeln!(f)?;

        for layer in &self.layers {
            write!(f, "{layer}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantifier_display() {
        assert_eq!(RncQuantifier::One.to_string(), "");
        assert_eq!(RncQuantifier::Optional.to_string(), "?");
        assert_eq!(RncQuantifier::ZeroOrMore.to_string(), "*");
        assert_eq!(RncQuantifier::OneOrMore.to_string(), "+");
    }

    #[test]
    fn attribute_display_required() {
        let attr = RncAttribute {
            name: "id".to_string(),
            type_str: "xsd:ID".to_string(),
            quantifier: RncQuantifier::One,
            default: None,
        };
        assert_eq!(attr.to_string(), "attribute id { xsd:ID }");
    }

    #[test]
    fn attribute_display_optional_with_default() {
        let attr = RncAttribute {
            name: "type".to_string(),
            type_str: "text".to_string(),
            quantifier: RncQuantifier::Optional,
            default: Some("info".to_string()),
        };
        assert_eq!(
            attr.to_string(),
            "attribute type { text }?  # default: info"
        );
    }

    #[test]
    fn element_display() {
        let elem = RncElement {
            prefix: "pr".to_string(),
            name: "title".to_string(),
            body: vec![RncBodyItem::Type("text".to_string())],
            quantifier: RncQuantifier::One,
        };
        assert_eq!(elem.to_string(), "element pr:title { text }");
    }

    #[test]
    fn element_display_optional() {
        let elem = RncElement {
            prefix: "pr".to_string(),
            name: "shortdesc".to_string(),
            body: vec![RncBodyItem::Type("text".to_string())],
            quantifier: RncQuantifier::Optional,
        };
        assert_eq!(elem.to_string(), "element pr:shortdesc { text }?");
    }

    #[test]
    fn body_item_choice() {
        let choice = RncBodyItem::Choice {
            options: vec![
                RncBodyItem::Element(RncElement {
                    prefix: "pr".to_string(),
                    name: "p".to_string(),
                    body: vec![RncBodyItem::Type("text".to_string())],
                    quantifier: RncQuantifier::One,
                }),
                RncBodyItem::Element(RncElement {
                    prefix: "pr".to_string(),
                    name: "ul".to_string(),
                    body: vec![RncBodyItem::Empty],
                    quantifier: RncQuantifier::One,
                }),
            ],
            quantifier: RncQuantifier::ZeroOrMore,
        };
        assert_eq!(
            choice.to_string(),
            "(element pr:p { text } | element pr:ul { empty })*"
        );
    }

    #[test]
    fn body_item_mixed() {
        let mixed = RncBodyItem::Mixed(vec![RncBodyItem::Type("text".to_string())]);
        assert_eq!(mixed.to_string(), "mixed { text }");
    }

    #[test]
    fn body_item_any_element() {
        assert_eq!(
            RncBodyItem::AnyElement(RncQuantifier::ZeroOrMore).to_string(),
            "anyElement*"
        );
    }

    #[test]
    fn body_item_patterned_text() {
        let pt = RncBodyItem::PatternedText("[a-z]+".to_string());
        assert_eq!(pt.to_string(), "text  # pattern: [a-z]+");
    }

    #[test]
    fn body_item_inline_enum() {
        let ie = RncBodyItem::InlineEnum(vec!["info".to_string(), "warning".to_string()]);
        assert_eq!(ie.to_string(), "\"info\" | \"warning\"");
    }

    #[test]
    fn namespace_display() {
        let ns = RncNamespace {
            prefix: "pr".to_string(),
            uri: "urn:clayers:prose".to_string(),
        };
        assert_eq!(ns.to_string(), "namespace pr = \"urn:clayers:prose\"");
    }

    #[test]
    fn global_element_single_line() {
        let elem = RncGlobalElement {
            prefix: "org".to_string(),
            name: "concept".to_string(),
            body: vec![
                RncBodyItem::Attribute(RncAttribute {
                    name: "ref".to_string(),
                    type_str: "text".to_string(),
                    quantifier: RncQuantifier::One,
                    default: None,
                }),
                RncBodyItem::Type("text".to_string()),
            ],
            description: None,
        };
        assert_eq!(
            elem.to_string(),
            "org:concept = element org:concept { attribute ref { text }, text }"
        );
    }

    #[test]
    fn global_element_multi_line() {
        let elem = RncGlobalElement {
            prefix: "pr".to_string(),
            name: "section".to_string(),
            body: vec![
                RncBodyItem::Attribute(RncAttribute {
                    name: "id".to_string(),
                    type_str: "xsd:ID".to_string(),
                    quantifier: RncQuantifier::One,
                    default: None,
                }),
                RncBodyItem::Element(RncElement {
                    prefix: "pr".to_string(),
                    name: "title".to_string(),
                    body: vec![RncBodyItem::Type("text".to_string())],
                    quantifier: RncQuantifier::One,
                }),
                RncBodyItem::Choice {
                    options: vec![
                        RncBodyItem::Element(RncElement {
                            prefix: "pr".to_string(),
                            name: "p".to_string(),
                            body: vec![RncBodyItem::Type("text".to_string())],
                            quantifier: RncQuantifier::One,
                        }),
                        RncBodyItem::Element(RncElement {
                            prefix: "pr".to_string(),
                            name: "section".to_string(),
                            body: vec![RncBodyItem::PatternRef("SectionType".to_string())],
                            quantifier: RncQuantifier::One,
                        }),
                    ],
                    quantifier: RncQuantifier::ZeroOrMore,
                },
            ],
            description: None,
        };
        let s = elem.to_string();
        assert!(s.contains("pr:section = element pr:section {"));
        assert!(s.contains('}'));
    }

    #[test]
    fn pattern_with_description() {
        let pat = RncPattern {
            name: "SectionType".to_string(),
            body: vec![RncBodyItem::Type("text".to_string())],
            description: Some("A structural section.".to_string()),
        };
        let s = pat.to_string();
        assert!(s.contains("# A structural section."));
        assert!(s.contains("SectionType = text"));
    }

    #[test]
    fn enum_summary_display() {
        let es = RncEnumSummary {
            type_name: "NoteKind".to_string(),
            values: vec![
                "info".to_string(),
                "important".to_string(),
                "warning".to_string(),
            ],
        };
        assert_eq!(es.to_string(), "# NoteKind: info | important | warning");
    }

    #[test]
    fn wrap_comment_short() {
        let lines = wrap_comment("Short text.", 78);
        assert_eq!(lines, vec!["# Short text."]);
    }

    #[test]
    fn wrap_comment_long() {
        let text = "This is a very long description that should be wrapped across multiple lines because it exceeds the maximum width.";
        let lines = wrap_comment(text, 40);
        assert!(lines.len() > 1);
        for line in &lines {
            assert!(line.starts_with("# "));
        }
    }

    #[test]
    fn schema_display() {
        let schema = RncSchema {
            header_comments: vec!["Test schema".to_string()],
            namespaces: vec![RncNamespace {
                prefix: "pr".to_string(),
                uri: "urn:test:prose".to_string(),
            }],
            layers: vec![],
        };
        let s = schema.to_string();
        assert!(s.contains("# Test schema"));
        assert!(s.contains("namespace pr = \"urn:test:prose\""));
    }
}

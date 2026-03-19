pub const SCHEMAS: &[(&str, &str)] = &[
    ("artifact.xsd", include_str!("../../../schemas/artifact.xsd")),
    ("decision.xsd", include_str!("../../../schemas/decision.xsd")),
    ("index.xsd", include_str!("../../../schemas/index.xsd")),
    ("llm.xsd", include_str!("../../../schemas/llm.xsd")),
    (
        "organization.xsd",
        include_str!("../../../schemas/organization.xsd"),
    ),
    ("plan.xsd", include_str!("../../../schemas/plan.xsd")),
    ("prose.xsd", include_str!("../../../schemas/prose.xsd")),
    ("relation.xsd", include_str!("../../../schemas/relation.xsd")),
    ("revision.xsd", include_str!("../../../schemas/revision.xsd")),
    ("source.xsd", include_str!("../../../schemas/source.xsd")),
    ("spec.xsd", include_str!("../../../schemas/spec.xsd")),
    (
        "terminology.xsd",
        include_str!("../../../schemas/terminology.xsd"),
    ),
];

pub const CATALOG: &str = include_str!("../../../schemas/catalog.xml");
pub const POSTPROCESS_XSLT: &str = include_str!("../../../schemas/postprocess.xslt");

pub const DOC_XSLT_FILES: &[(&str, &str)] = &[
    ("main.xslt", include_str!("../../../schemas/doc/main.xslt")),
    (
        "catchall.xslt",
        include_str!("../../../schemas/doc/catchall.xslt"),
    ),
    (
        "prose.xslt",
        include_str!("../../../schemas/doc/prose.xslt"),
    ),
    (
        "terminology.xslt",
        include_str!("../../../schemas/doc/terminology.xslt"),
    ),
    (
        "organization.xslt",
        include_str!("../../../schemas/doc/organization.xslt"),
    ),
    (
        "relation.xslt",
        include_str!("../../../schemas/doc/relation.xslt"),
    ),
    (
        "decision.xslt",
        include_str!("../../../schemas/doc/decision.xslt"),
    ),
    (
        "source.xslt",
        include_str!("../../../schemas/doc/source.xslt"),
    ),
    ("plan.xslt", include_str!("../../../schemas/doc/plan.xslt")),
    (
        "artifact.xslt",
        include_str!("../../../schemas/doc/artifact.xslt"),
    ),
    ("llm.xslt", include_str!("../../../schemas/doc/llm.xslt")),
    (
        "revision.xslt",
        include_str!("../../../schemas/doc/revision.xslt"),
    ),
];

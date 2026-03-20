# Python API Layer Design

A new clayers layer (`urn:clayers:python`, prefix `py`) for describing Python API surfaces: modules, classes, methods, properties, exceptions, and functions. Replaces prose tables with structured, artifact-mappable, renderable elements.

## Motivation

The Python integration spec (`integration.xml`) describes the clayers Python API using prose tables. These tables are not queryable, not individually artifact-mappable, and not semantically distinct from any other table in the spec. A dedicated layer makes each API element a first-class spec node that can be:

- Artifact-mapped to the implementing Rust/Python source lines
- Queried via XPath (`//py:method[@of="py-repo"]`)
- Rendered as structured API documentation via XSLT
- Connected to other spec nodes via relations
- Described for LLMs via the llm layer

## Namespace

- URI: `urn:clayers:python`
- Prefix: `py`
- Schema: `schemas/python.xsd`
- Catalog entry: `urn:clayers:python` -> `python.xsd`
- XSLT: `schemas/doc/python.xslt`

## Elements

### Content elements (top-level in `spec:clayers`)

All elements that carry an `id` must also carry `spec:content-element` appinfo so the combined schema generator discovers them and allows them as top-level children of `spec:clayers`. This includes member elements, since the `of` pattern places them at the root level.

| Element | Children | Attributes |
|---------|----------|------------|
| `py:module` | class, function, exception, constant, doc | `id` (required), `name` |
| `py:class` | method, property, param, doc | `id` (required), `name`, `of`, `bases` |
| `py:function` | param, returns, raises, doc | `id` (required), `name`, `of` |
| `py:exception` | doc | `id` (required), `name`, `bases`, `of` |

### Member elements (inside parents or via `of`)

These also carry `spec:content-element` appinfo because `of` usage places them at the spec root.

| Element | Children | Attributes |
|---------|----------|------------|
| `py:method` | param, returns, raises, doc | `id` (required), `name`, `of`, `kind` |
| `py:property` | doc | `id` (required), `name`, `type`, `of` |
| `py:constant` | doc | `id` (required), `name`, `type`, `of` |

The `kind` attribute on `py:method` is optional. Values: `method` (default), `staticmethod`, `classmethod`. This avoids three nearly-identical elements and XSD types while preserving the semantic distinction for rendering (decorator display).

### Leaf elements (no id, not spec nodes)

| Element | Attributes | Content |
|---------|------------|---------|
| `py:param` | `name`, `type`, `default`, `keyword`, `positional`, `variadic` | none |
| `py:returns` | `type` | none |
| `py:raises` | `type` | simple text content (xs:string): when/why the exception is raised |
| `py:doc` | none | any non-`py:` elements via `xs:any namespace="##other" processContents="lax"` |

## Containment Model

Elements attach to parents in two ways:

1. **Nesting:** XML containment. A `py:method` inside a `py:class` belongs to that class.
2. **Reference:** The `of` attribute references a parent's `id`. A `py:method` with `of="py-repo"` belongs to the class with `id="py-repo"`.

These are equivalent:

```xml
<!-- Nested -->
<py:class id="py-repo" name="Repo">
  <py:method id="py-repo-import" name="import_xml">
    <py:param name="xml" type="str"/>
    <py:returns type="ContentHash"/>
  </py:method>
</py:class>

<!-- Detached via of -->
<py:class id="py-repo" name="Repo"/>
<py:method id="py-repo-import" name="import_xml" of="py-repo">
  <py:param name="xml" type="str"/>
  <py:returns type="ContentHash"/>
</py:method>
```

When both nested and `of`-attached children exist for the same parent, nested children render first, then `of`-attached ones in document order across the entire combined document.

## Child Ordering

Within any `py:` element, children follow this sequence in the XSD:

1. `py:param` (zero or more) - constructor params for class, function/method params
2. `py:returns` (zero or one)
3. `py:raises` (zero or more)
4. `py:doc` (zero or one)
5. Member elements: `py:property`, `py:constant`, `py:method` (zero or more, in any order via `xs:choice`)

This means `py:doc` always comes after the signature elements and before nested members.

## Parameter Attributes

All `py:param` attributes beyond `name` are optional:

| Attribute | Type | Description |
|-----------|------|-------------|
| `name` | xs:string (required) | Parameter name |
| `type` | xs:string | Python type annotation |
| `default` | xs:string | Default value as Python expression |
| `keyword` | xs:boolean | If true, keyword-only (appears after `*` separator) |
| `positional` | xs:boolean | If true, positional-only (appears before `/` separator) |
| `variadic` | `"args"` or `"kwargs"` | `*args` or `**kwargs` parameter |

The XSLT renders the full Python signature syntax from these attributes:
- Inserts `/` after the last `positional="true"` param
- Inserts `*` before the first `keyword="true"` param (unless a `variadic="args"` param already provides the `*`)
- Prefixes `*` on `variadic="args"` and `**` on `variadic="kwargs"`

Example:

```xml
<py:function id="py-example" name="example">
  <py:param name="x" type="int" positional="true"/>
  <py:param name="y" type="str"/>
  <py:param name="args" type="int" variadic="args"/>
  <py:param name="z" type="bool" keyword="true"/>
  <py:param name="kwargs" type="Any" variadic="kwargs"/>
</py:function>
```

Renders as: `example(x: int, /, y: str, *args: int, z: bool, **kwargs: Any)`

## Constructor Parameters

`py:param` children of `py:class` represent `__init__` parameters. This is a fixed convention: Python classes are constructed by calling the class, so the class-level params describe `__init__`. Class-level attributes (not constructor params) are described as `py:property` elements.

## Multiple Inheritance

The `bases` attribute (note: plural) on `py:class` and `py:exception` is a comma-separated `xs:string` of base class names. For single inheritance, it contains one name. For multiple inheritance: `bases="Bar, Baz"`. These are display names, not ID refs.

## Documentation Wrapper

The `py:doc` element wraps prose content inside any `py:` element. It uses `xs:any namespace="##other" processContents="lax"`, which accepts elements from any namespace except `urn:clayers:python`. This means prose elements (`pr:p`, `pr:codeblock`, etc.) are accepted, while `py:` elements are not, avoiding ambiguity with the structural children.

Each element can have at most one `py:doc` child (enforced by `maxOccurs="1"` in the XSD sequence).

```xml
<py:method id="py-repo-import" name="import_xml" of="py-repo">
  <py:param name="xml" type="str"/>
  <py:returns type="ContentHash"/>
  <py:doc>
    <pr:p>Decomposes the XML string into the content-addressed
    Merkle DAG and returns the document hash.</pr:p>
    <pr:codeblock language="python">h = repo.import_xml("&lt;root/&gt;")</pr:codeblock>
  </py:doc>
</py:method>
```

## Schema Design Notes

- `id` attributes use `xs:ID` and are `use="required"` on all content and member elements.
- `of` uses `xs:string` (not `xs:IDREF`) following the cross-document reference pattern. A `spec:keyref` declaration named `python-of-refs` with selector `.//py:*[@of]` and field `@of` validates references against the combined document's ID space.
- `py:raises` is `xs:string` simple content with a required `type` attribute (simple content extension).
- `bases` on `py:class` and `py:exception` is `xs:string` (comma-separated display names).
- `type` attributes on `py:param`, `py:returns`, `py:property`, `py:constant` are `xs:string` (Python type annotation syntax).
- `default` on `py:param` is `xs:string` (Python expression as text).
- `keyword` and `positional` on `py:param` are `xs:boolean` (default false, omit when not applicable).
- `variadic` on `py:param` is a restricted `xs:string` with enumeration values `"args"` and `"kwargs"`.
- All complex types carry `llm:describe` appinfo annotations following the convention of all existing layers.
- The combined schema generator discovers `py:` content elements via `spec:content-element` appinfo automatically (standard mechanism, no special integration needed).

## XSLT Rendering

New stylesheet `schemas/doc/python.xslt`, imported by `main.xslt`.

### Collecting children

The XSLT defines a named template `py:collect-children` that merges nested children with `of`-attached children from the combined document. This uses an xsl:key for efficient lookup:

```xml
<xsl:key name="py-of" match="py:*[@of]" use="@of"/>
```

Each parent template calls this to get its full child set: nested children in document order, then key-matched `of` children in document order.

### Module

Renders as a section with heading. Children rendered in order: exceptions, classes, functions, constants.

```html
<section class="py-module" id="py-clayers-repo">
  <h3><span class="py-keyword">module</span> clayers.repo</h3>
  <!-- py:doc prose -->
  <!-- children -->
</section>
```

### Class

Renders as a card with constructor signature (from `py:param` children), then properties table, then methods.

```html
<div class="py-class" id="py-repo">
  <div class="py-signature">
    <span class="py-keyword">class</span>
    <span class="py-name">Repo</span>(<span class="py-params">store: MemoryStore | SqliteStore</span>)
  </div>
  <!-- py:doc prose -->
  <!-- properties table if any -->
  <!-- methods -->
</div>
```

### Method / function

Renders as a signature line. Parameters from `py:param`, return type from `py:returns`. The `kind` attribute controls decorator display:

- `kind="staticmethod"` renders `@staticmethod` above the signature
- `kind="classmethod"` renders `@classmethod` above the signature
- `kind="method"` or absent: no decorator

```html
<div class="py-method" id="py-repo-import-xml">
  <div class="py-signature">
    <span class="py-name">import_xml</span>(<span class="py-params">xml: str</span>)
    <span class="py-returns">-> ContentHash</span>
  </div>
  <!-- py:doc prose -->
  <!-- py:raises if any -->
</div>
```

### Property

Properties of a class are grouped into a table:

```html
<table class="py-properties">
  <tr id="py-km-name">
    <td><code>name</code></td>
    <td><code>str</code></td>
    <td><!-- py:doc inline --></td>
  </tr>
</table>
```

### Exception

Compact rendering with base class(es):

```html
<div class="py-exception" id="py-spec-error">
  <span class="py-keyword">exception</span>
  <span class="py-name">SpecError</span>(<span class="py-base">ClayersError</span>)
  <!-- py:doc -->
</div>
```

### Artifact mapping integration

Since each `py:` element with an `id` is a spec node, existing artifact rendering in `prose.xslt` picks up their mappings automatically. The python.xslt templates emit `id` attributes on HTML elements so artifact links anchor correctly.

## Migration Plan

1. Create `schemas/python.xsd` with all element and type definitions, including `llm:describe` appinfo and `spec:keyref` for `of`
2. Add catalog entry (`urn:clayers:python` -> `python.xsd`)
3. Add `import href="python.xslt"` to `main.xslt`, register `py` namespace
4. Create `schemas/doc/python.xslt` with rendering templates and `xsl:key` for `of` lookup
5. Replace prose tables in `clayers/clayers/integration.xml` with `py:` elements
6. Add per-method/per-class artifact mappings to the implementing source
7. Validate, fix hashes, verify rendering
8. Update the self-referential spec to describe the python layer itself

## Example: Full KnowledgeModel in py: elements

```xml
<py:class id="py-km" name="KnowledgeModel">
  <py:param name="path" type="str"/>
  <py:param name="repo_root" type="str | None" default="None"/>
  <py:doc>
    <pr:p>The central class for working with clayers specs from Python.
    Constructed from a filesystem path pointing to a spec directory.</pr:p>
  </py:doc>

  <py:property id="py-km-name" name="name" type="str">
    <py:doc><pr:p>Spec name (directory basename).</pr:p></py:doc>
  </py:property>
  <py:property id="py-km-files" name="files" type="list[str]">
    <py:doc><pr:p>Discovered file paths.</pr:p></py:doc>
  </py:property>

  <py:method id="py-km-validate" name="validate">
    <py:returns type="ValidationResult"/>
    <py:doc>
      <pr:p>Well-formedness, ID uniqueness, cross-layer keyrefs.</pr:p>
      <pr:codeblock language="python">result = km.validate()
assert result.is_valid</pr:codeblock>
    </py:doc>
  </py:method>

  <py:method id="py-km-query" name="query">
    <py:param name="xpath" type="str"/>
    <py:param name="mode" type="str" default="'xml'" keyword="true"/>
    <py:returns type="QueryResult"/>
    <py:doc><pr:p>XPath query; mode is "count", "text", or "xml".</pr:p></py:doc>
  </py:method>
</py:class>
```

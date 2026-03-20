# Python API Layer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `urn:clayers:python` XML layer for describing Python API surfaces, with XSD schema, XSLT rendering, and migration of integration.xml prose tables to structured `py:` elements with per-node artifact mappings.

**Architecture:** New XSD schema defines module/class/method/property/exception/function/constant elements with nesting and `of`-based detached containment. An XSLT stylesheet renders these as structured API documentation. The existing combined schema generator discovers `py:` elements via `spec:content-element` appinfo automatically.

**Tech Stack:** XSD 1.1, XSLT 3.0, clayers validation tooling (Rust CLI)

**Spec:** `docs/superpowers/specs/2026-03-20-python-api-layer-design.md`

---

### Task 1: Create python.xsd schema

**Files:**
- Create: `schemas/python.xsd`

This is the largest task. The schema defines all element types for the Python API layer.

- [ ] **Step 1: Create the schema file with namespace, imports, and top-level annotations**

```xml
<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           xmlns:py="urn:clayers:python"
           xmlns:llm="urn:clayers:llm"
           xmlns:spec="urn:clayers:spec"
           targetNamespace="urn:clayers:python"
           elementFormDefault="qualified"
           version="1.1">

  <xs:annotation>
    <xs:documentation>
      Python API Layer Schema.
      Describes Python API surfaces: modules, classes, methods, properties,
      exceptions, functions, and constants. Each element with an id is a
      spec node that can be artifact-mapped, queried, and related.
      Supports both nesting and detached containment via the 'of' attribute.
    </xs:documentation>
    <xs:appinfo>
      <llm:describe>
        The Python layer describes Python API surfaces as structured elements.
        py:module, py:class, py:function, py:exception are top-level containers.
        py:method, py:property, py:constant are members attached via nesting or
        the 'of' attribute. py:param, py:returns, py:raises are leaf elements
        for signatures. py:doc wraps prose content. Every element with an id
        is individually artifact-mappable.
      </llm:describe>
      <spec:keyref name="python-of-refs" selector=".//py:*[@of]" field="@of"/>
    </xs:appinfo>
  </xs:annotation>
```

- [ ] **Step 2: Define leaf types (ParamType, ReturnsType, RaisesType, DocType)**

ParamType: empty element with name (required), type, default, keyword (boolean), positional (boolean), variadic (enum: args|kwargs).

ReturnsType: empty element with type (required).

RaisesType: simple content extension of xs:string with type attribute (required).

DocType: `xs:any namespace="##other" processContents="lax"` wrapped in a sequence.

```xml
  <!-- Variadic kind restriction -->
  <xs:simpleType name="VariadicKind">
    <xs:restriction base="xs:string">
      <xs:enumeration value="args"/>
      <xs:enumeration value="kwargs"/>
    </xs:restriction>
  </xs:simpleType>

  <!-- Method kind restriction -->
  <xs:simpleType name="MethodKind">
    <xs:restriction base="xs:string">
      <xs:enumeration value="method"/>
      <xs:enumeration value="staticmethod"/>
      <xs:enumeration value="classmethod"/>
    </xs:restriction>
  </xs:simpleType>

  <!-- Parameter -->
  <xs:element name="param">
    <xs:complexType>
      <xs:annotation><xs:appinfo>
        <llm:describe>A function/method/constructor parameter. Attributes describe
        name, type annotation, default value, and parameter kind (keyword-only,
        positional-only, variadic).</llm:describe>
      </xs:appinfo></xs:annotation>
      <xs:attribute name="name" type="xs:string" use="required"/>
      <xs:attribute name="type" type="xs:string"/>
      <xs:attribute name="default" type="xs:string"/>
      <xs:attribute name="keyword" type="xs:boolean"/>
      <xs:attribute name="positional" type="xs:boolean"/>
      <xs:attribute name="variadic" type="py:VariadicKind"/>
    </xs:complexType>
  </xs:element>

  <!-- Returns -->
  <xs:element name="returns">
    <xs:complexType>
      <xs:annotation><xs:appinfo>
        <llm:describe>Return type annotation for a function or method.</llm:describe>
      </xs:appinfo></xs:annotation>
      <xs:attribute name="type" type="xs:string" use="required"/>
    </xs:complexType>
  </xs:element>

  <!-- Raises -->
  <xs:element name="raises">
    <xs:complexType>
      <xs:annotation><xs:appinfo>
        <llm:describe>An exception a function or method may raise. The type
        attribute names the exception class. Text content describes when/why.</llm:describe>
      </xs:appinfo></xs:annotation>
      <xs:simpleContent>
        <xs:extension base="xs:string">
          <xs:attribute name="type" type="xs:string" use="required"/>
        </xs:extension>
      </xs:simpleContent>
    </xs:complexType>
  </xs:element>

  <!-- Doc wrapper -->
  <xs:element name="doc" type="py:DocType"/>

  <xs:complexType name="DocType">
    <xs:annotation><xs:appinfo>
      <llm:describe>Documentation wrapper accepting prose elements (pr:p,
      pr:codeblock, pr:note, etc.) from the prose namespace. Each py: element
      can have at most one py:doc child.</llm:describe>
    </xs:appinfo></xs:annotation>
    <xs:sequence>
      <xs:any namespace="##other" processContents="lax"
              minOccurs="0" maxOccurs="unbounded"/>
    </xs:sequence>
  </xs:complexType>
```

- [ ] **Step 3: Define member types (PropertyType, ConstantType, MethodType)**

Each follows the child ordering: param*, returns?, raises*, doc?, then nested members via xs:choice.

PropertyType: id (required), name, type, of. Children: doc only.

ConstantType: same as PropertyType.

MethodType: id (required), name, of, kind. Children: param*, returns?, raises*, doc?.

```xml
  <!-- Property -->
  <xs:element name="property" type="py:PropertyType">
    <xs:annotation><xs:appinfo><spec:content-element/></xs:appinfo></xs:annotation>
  </xs:element>

  <xs:complexType name="PropertyType">
    <xs:annotation><xs:appinfo>
      <llm:describe>A read-only property on a class. The type attribute is a
      Python type annotation. Attach to a class via nesting or the 'of' attribute.</llm:describe>
    </xs:appinfo></xs:annotation>
    <xs:sequence>
      <xs:element ref="py:doc" minOccurs="0"/>
    </xs:sequence>
    <xs:attribute name="id" type="xs:ID" use="required"/>
    <xs:attribute name="name" type="xs:string" use="required"/>
    <xs:attribute name="type" type="xs:string"/>
    <xs:attribute name="of" type="xs:string"/>
  </xs:complexType>

  <!-- Constant -->
  <xs:element name="constant" type="py:ConstantType">
    <xs:annotation><xs:appinfo><spec:content-element/></xs:appinfo></xs:annotation>
  </xs:element>

  <xs:complexType name="ConstantType">
    <xs:annotation><xs:appinfo>
      <llm:describe>A module-level or class-level constant value. Distinguished
      from property by being a fixed value rather than an instance attribute.</llm:describe>
    </xs:appinfo></xs:annotation>
    <xs:sequence>
      <xs:element ref="py:doc" minOccurs="0"/>
    </xs:sequence>
    <xs:attribute name="id" type="xs:ID" use="required"/>
    <xs:attribute name="name" type="xs:string" use="required"/>
    <xs:attribute name="type" type="xs:string"/>
    <xs:attribute name="of" type="xs:string"/>
  </xs:complexType>

  <!-- Method -->
  <xs:element name="method" type="py:MethodType">
    <xs:annotation><xs:appinfo><spec:content-element/></xs:appinfo></xs:annotation>
  </xs:element>

  <xs:complexType name="MethodType">
    <xs:annotation><xs:appinfo>
      <llm:describe>A method on a class, or a standalone callable when used with
      'of' pointing to a module. The 'kind' attribute distinguishes regular methods,
      staticmethods, and classmethods. Children define the signature: param*,
      returns?, raises*, doc?.</llm:describe>
    </xs:appinfo></xs:annotation>
    <xs:sequence>
      <xs:element ref="py:param" minOccurs="0" maxOccurs="unbounded"/>
      <xs:element ref="py:returns" minOccurs="0"/>
      <xs:element ref="py:raises" minOccurs="0" maxOccurs="unbounded"/>
      <xs:element ref="py:doc" minOccurs="0"/>
    </xs:sequence>
    <xs:attribute name="id" type="xs:ID" use="required"/>
    <xs:attribute name="name" type="xs:string" use="required"/>
    <xs:attribute name="of" type="xs:string"/>
    <xs:attribute name="kind" type="py:MethodKind"/>
  </xs:complexType>
```

- [ ] **Step 4: Define container types (FunctionType, ExceptionType, ClassType, ModuleType)**

FunctionType: same children as MethodType (param*, returns?, raises*, doc?). No kind attribute.

ExceptionType: id, name, bases, of. Children: doc only.

ClassType: id, name, bases, of. Children: param*, doc?, then xs:choice of property/constant/method.

ModuleType: id, name. Children: doc?, then xs:choice of class/function/exception/constant.

```xml
  <!-- Function -->
  <xs:element name="function" type="py:FunctionType">
    <xs:annotation><xs:appinfo><spec:content-element/></xs:appinfo></xs:annotation>
  </xs:element>

  <xs:complexType name="FunctionType">
    <xs:annotation><xs:appinfo>
      <llm:describe>A standalone function. Attach to a module via nesting or
      the 'of' attribute. Children: param*, returns?, raises*, doc?.</llm:describe>
    </xs:appinfo></xs:annotation>
    <xs:sequence>
      <xs:element ref="py:param" minOccurs="0" maxOccurs="unbounded"/>
      <xs:element ref="py:returns" minOccurs="0"/>
      <xs:element ref="py:raises" minOccurs="0" maxOccurs="unbounded"/>
      <xs:element ref="py:doc" minOccurs="0"/>
    </xs:sequence>
    <xs:attribute name="id" type="xs:ID" use="required"/>
    <xs:attribute name="name" type="xs:string" use="required"/>
    <xs:attribute name="of" type="xs:string"/>
  </xs:complexType>

  <!-- Exception -->
  <xs:element name="exception" type="py:ExceptionType">
    <xs:annotation><xs:appinfo><spec:content-element/></xs:appinfo></xs:annotation>
  </xs:element>

  <xs:complexType name="ExceptionType">
    <xs:annotation><xs:appinfo>
      <llm:describe>A Python exception class. The bases attribute is a
      comma-separated list of base class names. Attach to a module via nesting
      or 'of'.</llm:describe>
    </xs:appinfo></xs:annotation>
    <xs:sequence>
      <xs:element ref="py:doc" minOccurs="0"/>
    </xs:sequence>
    <xs:attribute name="id" type="xs:ID" use="required"/>
    <xs:attribute name="name" type="xs:string" use="required"/>
    <xs:attribute name="bases" type="xs:string"/>
    <xs:attribute name="of" type="xs:string"/>
  </xs:complexType>

  <!-- Class -->
  <xs:element name="class" type="py:ClassType">
    <xs:annotation><xs:appinfo><spec:content-element/></xs:appinfo></xs:annotation>
  </xs:element>

  <xs:complexType name="ClassType">
    <xs:annotation><xs:appinfo>
      <llm:describe>A Python class. Constructor parameters are py:param children.
      Members (properties, constants, methods) nest inside or attach via 'of'.
      The bases attribute is comma-separated base class names.</llm:describe>
    </xs:appinfo></xs:annotation>
    <xs:sequence>
      <xs:element ref="py:param" minOccurs="0" maxOccurs="unbounded"/>
      <xs:element ref="py:doc" minOccurs="0"/>
      <xs:choice minOccurs="0" maxOccurs="unbounded">
        <xs:element ref="py:property"/>
        <xs:element ref="py:constant"/>
        <xs:element ref="py:method"/>
      </xs:choice>
    </xs:sequence>
    <xs:attribute name="id" type="xs:ID" use="required"/>
    <xs:attribute name="name" type="xs:string" use="required"/>
    <xs:attribute name="bases" type="xs:string"/>
    <xs:attribute name="of" type="xs:string"/>
  </xs:complexType>

  <!-- Module -->
  <xs:element name="module" type="py:ModuleType">
    <xs:annotation><xs:appinfo><spec:content-element/></xs:appinfo></xs:annotation>
  </xs:element>

  <xs:complexType name="ModuleType">
    <xs:annotation><xs:appinfo>
      <llm:describe>A Python module (importable package or file). Contains classes,
      functions, exceptions, and constants as nested children.</llm:describe>
    </xs:appinfo></xs:annotation>
    <xs:sequence>
      <xs:element ref="py:doc" minOccurs="0"/>
      <xs:choice minOccurs="0" maxOccurs="unbounded">
        <xs:element ref="py:class"/>
        <xs:element ref="py:function"/>
        <xs:element ref="py:exception"/>
        <xs:element ref="py:constant"/>
      </xs:choice>
    </xs:sequence>
    <xs:attribute name="id" type="xs:ID" use="required"/>
    <xs:attribute name="name" type="xs:string" use="required"/>
  </xs:complexType>

</xs:schema>
```

- [ ] **Step 5: Validate the schema is well-formed**

Run: `xmllint --noout schemas/python.xsd`
Expected: no output (success)

- [ ] **Step 6: Commit**

```
Problem: clayers has no structured way to describe Python API surfaces

Solution: add schemas/python.xsd defining the urn:clayers:python layer
```

---

### Task 2: Register the namespace

**Files:**
- Modify: `schemas/catalog.xml`
- Modify: `crates/clayers-spec/src/namespace.rs`

- [ ] **Step 1: Add catalog entry**

Add to `schemas/catalog.xml` after the `urn:clayers:llm` entry:

```xml
  <uri name="urn:clayers:python"      uri="python.xsd"/>
```

- [ ] **Step 2: Add namespace constant and prefix mapping to namespace.rs**

In `crates/clayers-spec/src/namespace.rs`:

Add constant after `LLM`:
```rust
pub const PYTHON: &str = "urn:clayers:python";
```

Add to `ALL_LAYERS` array (update the count comment to 13):
```rust
pub const ALL_LAYERS: &[&str] = &[
    SPEC, INDEX, REVISION, PROSE, TERMINOLOGY, ORGANIZATION,
    RELATION, DECISION, SOURCE, PLAN, ARTIFACT, LLM, PYTHON,
];
```

Add to `PREFIX_MAP` before `("cmb", COMBINED)`:
```rust
    ("py", PYTHON),
```

Update the count comment on `PREFIX_MAP` from 17 to 18.

- [ ] **Step 3: Fix the test assertion**

The test `prefix_map_covers_all_layers_plus_combined` asserts `PREFIX_MAP.len()` equals 17. Update to 18.

- [ ] **Step 4: Run tests**

Run: `cargo test -p clayers-spec -- namespace`
Expected: all namespace tests pass

- [ ] **Step 5: Commit**

```
Problem: the python layer namespace is not registered in the catalog or Rust code

Solution: add urn:clayers:python to catalog.xml and namespace.rs
```

---

### Task 3: Create python.xslt rendering stylesheet

**Files:**
- Create: `schemas/doc/python.xslt`
- Modify: `schemas/doc/main.xslt`

- [ ] **Step 1: Create python.xslt with namespace declarations and xsl:key**

```xml
<?xml version="1.0" encoding="UTF-8"?>
<xsl:stylesheet version="3.0"
    xmlns:xsl="http://www.w3.org/1999/XSL/Transform"
    xmlns:py="urn:clayers:python"
    xmlns:pr="urn:clayers:prose"
    xmlns:art="urn:clayers:artifact"
    xmlns:llm="urn:clayers:llm"
    xmlns:cmb="urn:clayers:combined"
    xmlns:doc="urn:clayers:doc"
    exclude-result-prefixes="py pr art llm cmb doc">

  <!-- Key for detached children via 'of' attribute -->
  <xsl:key name="py-of" match="py:*[@of]" use="@of"/>
```

- [ ] **Step 2: Add signature rendering helper template**

A named template that renders `py:param` children as a Python signature string, handling positional `/`, keyword `*`, variadic `*args`/`**kwargs`, types, and defaults.

```xml
  <!-- Render parameter list as Python signature -->
  <xsl:template name="py-signature-params">
    <xsl:param name="params"/>
    <xsl:variable name="has-positional" select="$params[@positional = 'true']"/>
    <xsl:variable name="has-keyword" select="$params[@keyword = 'true']"/>
    <xsl:variable name="has-variadic-args" select="$params[@variadic = 'args']"/>
    <xsl:for-each select="$params">
      <!-- Insert / after last positional param -->
      <xsl:if test="not(@positional = 'true') and $has-positional
                    and not(preceding-sibling::py:param[not(@positional = 'true')])">
        <xsl:text>/, </xsl:text>
      </xsl:if>
      <!-- Insert * before first keyword param (if no *args already) -->
      <xsl:if test="@keyword = 'true' and not($has-variadic-args)
                    and not(preceding-sibling::py:param[@keyword = 'true'])">
        <xsl:text>*, </xsl:text>
      </xsl:if>
      <!-- Variadic prefix -->
      <xsl:if test="@variadic = 'args'">*</xsl:if>
      <xsl:if test="@variadic = 'kwargs'">**</xsl:if>
      <!-- Name -->
      <xsl:value-of select="@name"/>
      <!-- Type annotation -->
      <xsl:if test="@type">
        <xsl:text>: </xsl:text>
        <xsl:value-of select="@type"/>
      </xsl:if>
      <!-- Default -->
      <xsl:if test="@default">
        <xsl:text> = </xsl:text>
        <xsl:value-of select="@default"/>
      </xsl:if>
      <!-- Separator -->
      <xsl:if test="position() != last()">
        <xsl:text>, </xsl:text>
      </xsl:if>
    </xsl:for-each>
  </xsl:template>
```

- [ ] **Step 3: Add module template**

```xml
  <!-- Module -->
  <xsl:template match="py:module">
    <section class="py-module" id="{@id}">
      <h3>
        <span class="py-keyword">module </span>
        <xsl:value-of select="@name"/>
      </h3>
      <xsl:apply-templates select="py:doc"/>
      <!-- Nested children, then of-attached -->
      <xsl:apply-templates select="py:exception | py:class | py:function | py:constant"/>
      <xsl:apply-templates select="key('py-of', @id)"/>
    </section>
  </xsl:template>
```

- [ ] **Step 4: Add class template**

```xml
  <!-- Class -->
  <xsl:template match="py:class">
    <div class="py-class" id="{@id}">
      <div class="py-signature">
        <span class="py-keyword">class </span>
        <span class="py-name"><xsl:value-of select="@name"/></span>
        <xsl:if test="py:param or @bases">
          <xsl:text>(</xsl:text>
          <xsl:if test="@bases and not(py:param)">
            <xsl:value-of select="@bases"/>
          </xsl:if>
          <xsl:if test="py:param">
            <xsl:call-template name="py-signature-params">
              <xsl:with-param name="params" select="py:param"/>
            </xsl:call-template>
          </xsl:if>
          <xsl:text>)</xsl:text>
        </xsl:if>
      </div>
      <xsl:apply-templates select="py:doc"/>
      <!-- Properties table -->
      <xsl:variable name="props" select="py:property | key('py-of', @id)[self::py:property]"/>
      <xsl:if test="$props">
        <table class="py-properties">
          <thead><tr><th>Property</th><th>Type</th><th>Description</th></tr></thead>
          <tbody>
            <xsl:for-each select="$props">
              <tr id="{@id}">
                <td><code><xsl:value-of select="@name"/></code></td>
                <td><code><xsl:value-of select="@type"/></code></td>
                <td><xsl:apply-templates select="py:doc/node()"/></td>
              </tr>
            </xsl:for-each>
          </tbody>
        </table>
      </xsl:if>
      <!-- Constants -->
      <xsl:apply-templates select="py:constant | key('py-of', @id)[self::py:constant]"/>
      <!-- Methods -->
      <xsl:apply-templates select="py:method | key('py-of', @id)[self::py:method]"/>
    </div>
  </xsl:template>
```

- [ ] **Step 5: Add method/function templates**

```xml
  <!-- Method -->
  <xsl:template match="py:method">
    <div class="py-method" id="{@id}">
      <xsl:if test="@kind = 'staticmethod' or @kind = 'classmethod'">
        <div class="py-decorator">@<xsl:value-of select="@kind"/></div>
      </xsl:if>
      <div class="py-signature">
        <span class="py-name"><xsl:value-of select="@name"/></span>
        <xsl:text>(</xsl:text>
        <xsl:call-template name="py-signature-params">
          <xsl:with-param name="params" select="py:param"/>
        </xsl:call-template>
        <xsl:text>)</xsl:text>
        <xsl:if test="py:returns">
          <span class="py-returns">
            <xsl:text> &#x2192; </xsl:text>
            <xsl:value-of select="py:returns/@type"/>
          </span>
        </xsl:if>
      </div>
      <xsl:apply-templates select="py:doc"/>
      <xsl:if test="py:raises">
        <div class="py-raises">
          <xsl:text>Raises: </xsl:text>
          <xsl:for-each select="py:raises">
            <code><xsl:value-of select="@type"/></code>
            <xsl:if test="text()"> &#x2013; <xsl:value-of select="."/></xsl:if>
            <xsl:if test="position() != last()">, </xsl:if>
          </xsl:for-each>
        </div>
      </xsl:if>
    </div>
  </xsl:template>

  <!-- Function (same rendering as method, no kind) -->
  <xsl:template match="py:function">
    <div class="py-function" id="{@id}">
      <div class="py-signature">
        <span class="py-name"><xsl:value-of select="@name"/></span>
        <xsl:text>(</xsl:text>
        <xsl:call-template name="py-signature-params">
          <xsl:with-param name="params" select="py:param"/>
        </xsl:call-template>
        <xsl:text>)</xsl:text>
        <xsl:if test="py:returns">
          <span class="py-returns">
            <xsl:text> &#x2192; </xsl:text>
            <xsl:value-of select="py:returns/@type"/>
          </span>
        </xsl:if>
      </div>
      <xsl:apply-templates select="py:doc"/>
    </div>
  </xsl:template>
```

- [ ] **Step 6: Add exception, property, constant, doc templates**

```xml
  <!-- Exception -->
  <xsl:template match="py:exception">
    <div class="py-exception" id="{@id}">
      <div class="py-signature">
        <span class="py-keyword">exception </span>
        <span class="py-name"><xsl:value-of select="@name"/></span>
        <xsl:if test="@bases">
          <xsl:text>(</xsl:text>
          <xsl:value-of select="@bases"/>
          <xsl:text>)</xsl:text>
        </xsl:if>
      </div>
      <xsl:apply-templates select="py:doc"/>
    </div>
  </xsl:template>

  <!-- Standalone property (outside class context, rendered as a block) -->
  <xsl:template match="py:property">
    <div class="py-property" id="{@id}">
      <code><xsl:value-of select="@name"/></code>
      <xsl:if test="@type">
        <xsl:text>: </xsl:text>
        <code><xsl:value-of select="@type"/></code>
      </xsl:if>
      <xsl:apply-templates select="py:doc"/>
    </div>
  </xsl:template>

  <!-- Constant -->
  <xsl:template match="py:constant">
    <div class="py-constant" id="{@id}">
      <code><xsl:value-of select="@name"/></code>
      <xsl:if test="@type">
        <xsl:text>: </xsl:text>
        <code><xsl:value-of select="@type"/></code>
      </xsl:if>
      <xsl:apply-templates select="py:doc"/>
    </div>
  </xsl:template>

  <!-- Doc wrapper: delegate to prose templates -->
  <xsl:template match="py:doc">
    <div class="py-doc">
      <xsl:apply-templates/>
    </div>
  </xsl:template>

</xsl:stylesheet>
```

- [ ] **Step 7: Register in main.xslt**

Add to `schemas/doc/main.xslt`:

After the existing `xmlns:llm="urn:clayers:llm"` namespace declaration, add:
```xml
    xmlns:py="urn:clayers:python"
```

Add to the `exclude-result-prefixes` attribute: ` py`

After the `<xsl:import href="llm.xslt"/>` line, add:
```xml
  <xsl:import href="python.xslt"/>
```

- [ ] **Step 8: Commit**

```
Problem: py: elements have no doc rendering

Solution: add schemas/doc/python.xslt and register in main.xslt
```

---

### Task 4: Replace prose tables in integration.xml with py: elements

**Files:**
- Modify: `clayers/clayers/integration.xml`

This replaces the `pr:table` elements in the KnowledgeModel API, Repository API, Shared Query Protocol, and Error Hierarchy sections with structured `py:` elements. The prose narrative and code examples stay; only the tables become `py:` elements.

- [ ] **Step 1: Add py namespace declaration to integration.xml root**

Add `xmlns:py="urn:clayers:python"` to the `spec:clayers` root element.

- [ ] **Step 2: Replace KnowledgeModel tables with py:class**

Remove the two `pr:table` elements (properties table and methods table) and the constructor `pr:p` line from the `python-knowledge-model-api` section. Replace with:

```xml
    <py:class id="py-km" name="KnowledgeModel">
      <py:param name="path" type="str"/>
      <py:param name="repo_root" type="str | None" default="None"/>

      <py:property id="py-km-name" name="name" type="str">
        <py:doc><pr:p>Spec name (directory basename).</pr:p></py:doc>
      </py:property>
      <py:property id="py-km-files" name="files" type="list[str]">
        <py:doc><pr:p>Discovered file paths.</pr:p></py:doc>
      </py:property>
      <py:property id="py-km-combined-xml" name="combined_xml" type="str">
        <py:doc><pr:p>Assembled combined document.</pr:p></py:doc>
      </py:property>
      <py:property id="py-km-schema-dir" name="schema_dir" type="str | None">
        <py:doc><pr:p>Path to schemas/ if found.</pr:p></py:doc>
      </py:property>

      <py:method id="py-km-validate" name="validate">
        <py:returns type="ValidationResult"/>
        <py:doc><pr:p>Well-formedness, ID uniqueness, cross-layer keyrefs.</pr:p></py:doc>
      </py:method>
      <py:method id="py-km-check-drift" name="check_drift">
        <py:returns type="DriftReport"/>
        <py:doc><pr:p>Compare stored hashes against current content.</pr:p></py:doc>
      </py:method>
      <py:method id="py-km-coverage" name="coverage">
        <py:param name="code_path" type="str | None" default="None" keyword="true"/>
        <py:returns type="CoverageReport"/>
        <py:doc><pr:p>Spec-to-code and code-to-spec coverage analysis.</pr:p></py:doc>
      </py:method>
      <py:method id="py-km-connectivity" name="connectivity">
        <py:returns type="ConnectivityReport"/>
        <py:doc><pr:p>Graph metrics: components, hubs, bridges, cycles.</pr:p></py:doc>
      </py:method>
      <py:method id="py-km-query" name="query">
        <py:param name="xpath" type="str"/>
        <py:param name="mode" type="str" default="'xml'" keyword="true"/>
        <py:returns type="QueryResult"/>
        <py:doc><pr:p>XPath query; mode is "count", "text", or "xml".</pr:p></py:doc>
      </py:method>
      <py:method id="py-km-fix-node-hashes" name="fix_node_hashes">
        <py:returns type="FixReport"/>
        <py:doc><pr:p>Recompute spec-side hashes in artifact mappings.</pr:p></py:doc>
      </py:method>
      <py:method id="py-km-fix-artifact-hashes" name="fix_artifact_hashes">
        <py:returns type="FixReport"/>
        <py:doc><pr:p>Recompute code-side hashes in artifact mappings.</pr:p></py:doc>
      </py:method>
    </py:class>
```

- [ ] **Step 3: Replace Repo tables with py:class**

Remove the two `pr:table` elements (methods table and supporting types table) and the constructor `pr:p` line from `python-repo-api`. Replace with:

```xml
    <py:class id="py-repo" name="Repo">
      <py:param name="store" type="MemoryStore | SqliteStore"/>

      <py:method id="py-repo-import-xml" name="import_xml">
        <py:param name="xml" type="str"/>
        <py:returns type="ContentHash"/>
        <py:doc><pr:p>Decompose XML into the Merkle DAG, return document hash.</pr:p></py:doc>
      </py:method>
      <py:method id="py-repo-export-xml" name="export_xml">
        <py:param name="hash" type="ContentHash"/>
        <py:returns type="str"/>
        <py:doc><pr:p>Reconstruct XML from its content hash.</pr:p></py:doc>
      </py:method>
      <py:method id="py-repo-build-tree" name="build_tree">
        <py:param name="entries" type="list[tuple[str, ContentHash]]"/>
        <py:returns type="ContentHash"/>
        <py:doc><pr:p>Build tree from (path, doc_hash) pairs.</pr:p></py:doc>
      </py:method>
      <py:method id="py-repo-commit" name="commit">
        <py:param name="branch" type="str"/>
        <py:param name="tree" type="ContentHash"/>
        <py:param name="author" type="Author"/>
        <py:param name="message" type="str"/>
        <py:returns type="ContentHash"/>
        <py:doc><pr:p>Create commit on branch pointing to tree.</pr:p></py:doc>
      </py:method>
      <py:method id="py-repo-create-branch" name="create_branch">
        <py:param name="name" type="str"/>
        <py:param name="target" type="ContentHash"/>
      </py:method>
      <py:method id="py-repo-delete-branch" name="delete_branch">
        <py:param name="name" type="str"/>
      </py:method>
      <py:method id="py-repo-list-branches" name="list_branches">
        <py:returns type="list[tuple[str, ContentHash]]"/>
      </py:method>
      <py:method id="py-repo-create-tag" name="create_tag">
        <py:param name="name" type="str"/>
        <py:param name="target" type="ContentHash"/>
        <py:param name="tagger" type="Author"/>
        <py:param name="message" type="str"/>
      </py:method>
      <py:method id="py-repo-list-tags" name="list_tags">
        <py:returns type="list[tuple[str, ContentHash]]"/>
      </py:method>
      <py:method id="py-repo-log" name="log">
        <py:param name="from_hash" type="ContentHash"/>
        <py:param name="limit" type="int | None" default="None"/>
        <py:returns type="list[CommitObject]"/>
      </py:method>
      <py:method id="py-repo-diff-trees" name="diff_trees">
        <py:param name="a" type="ContentHash"/>
        <py:param name="b" type="ContentHash"/>
        <py:returns type="list[FileChange]"/>
      </py:method>
      <py:method id="py-repo-query" name="query">
        <py:param name="xpath" type="str"/>
        <py:param name="mode" type="str" default="'xml'" keyword="true"/>
        <py:returns type="QueryResult"/>
      </py:method>
    </py:class>
```

Also add supporting types as separate elements (detached via `of` or standalone):

```xml
    <py:class id="py-author" name="Author">
      <py:param name="name" type="str"/>
      <py:param name="email" type="str"/>
      <py:property id="py-author-name" name="name" type="str"/>
      <py:property id="py-author-email" name="email" type="str"/>
    </py:class>

    <py:class id="py-commit-object" name="CommitObject">
      <py:property id="py-commit-tree" name="tree" type="ContentHash"/>
      <py:property id="py-commit-parents" name="parents" type="list[ContentHash]"/>
      <py:property id="py-commit-author" name="author" type="Author"/>
      <py:property id="py-commit-timestamp" name="timestamp" type="str"/>
      <py:property id="py-commit-message" name="message" type="str"/>
    </py:class>

    <py:class id="py-file-change" name="FileChange">
      <py:property id="py-fc-kind" name="kind" type="str">
        <py:doc><pr:p>"added", "removed", or "modified".</pr:p></py:doc>
      </py:property>
      <py:property id="py-fc-path" name="path" type="str"/>
      <py:property id="py-fc-old-hash" name="old_hash" type="ContentHash | None"/>
      <py:property id="py-fc-new-hash" name="new_hash" type="ContentHash | None"/>
    </py:class>

    <py:class id="py-content-hash" name="ContentHash">
      <py:property id="py-ch-hex" name="hex" type="str"/>
      <py:property id="py-ch-prefixed" name="prefixed" type="str"/>
      <py:method id="py-ch-from-canonical" name="from_canonical" kind="staticmethod">
        <py:param name="data" type="bytes"/>
        <py:returns type="ContentHash"/>
      </py:method>
      <py:method id="py-ch-from-hex" name="from_hex" kind="staticmethod">
        <py:param name="s" type="str"/>
        <py:returns type="ContentHash"/>
      </py:method>
      <py:method id="py-ch-from-bytes" name="from_bytes" kind="staticmethod">
        <py:param name="data" type="bytes"/>
        <py:returns type="ContentHash"/>
      </py:method>
    </py:class>
```

- [ ] **Step 4: Replace QueryResult table with py:class and error hierarchy table with py:exception elements**

Replace QueryResult `pr:table` in `python-queryable` section:

```xml
    <py:class id="py-query-result" name="QueryResult">
      <py:property id="py-qr-kind" name="kind" type="str">
        <py:doc><pr:p>"count", "text", or "xml".</pr:p></py:doc>
      </py:property>
      <py:property id="py-qr-count" name="count" type="int | None">
        <py:doc><pr:p>Set when kind == "count".</pr:p></py:doc>
      </py:property>
      <py:property id="py-qr-values" name="values" type="list[str] | None">
        <py:doc><pr:p>Set when kind == "text" or "xml".</pr:p></py:doc>
      </py:property>
    </py:class>
```

Replace error hierarchy `pr:table` in `python-error-hierarchy` section:

```xml
    <py:exception id="py-clayers-error" name="ClayersError" bases="Exception">
      <py:doc><pr:p>Base exception for all clayers errors.</pr:p></py:doc>
    </py:exception>
    <py:exception id="py-xml-error" name="XmlError" bases="ClayersError">
      <py:doc><pr:p>Raised by ContentHash.from_hex(), canonicalization.</pr:p></py:doc>
    </py:exception>
    <py:exception id="py-spec-error" name="SpecError" bases="ClayersError">
      <py:doc><pr:p>Raised by KnowledgeModel constructor and all methods.</pr:p></py:doc>
    </py:exception>
    <py:exception id="py-repo-error" name="RepoError" bases="ClayersError">
      <py:doc><pr:p>Raised by Repo constructor and all methods.</pr:p></py:doc>
    </py:exception>
```

- [ ] **Step 5: Validate the spec**

Run: `cargo run -p clayers -- validate clayers/clayers/`
Expected: `OK (no structural errors)`

- [ ] **Step 6: Commit**

```
Problem: Python API is described in unstructured prose tables

Solution: replace tables in integration.xml with py: layer elements
```

---

### Task 5: Add per-node artifact mappings

**Files:**
- Modify: `clayers/clayers/integration.xml`

Replace the coarse section-level artifact mappings with per-node mappings. Each `py:method`, `py:class`, `py:property`, `py:exception` gets its own `art:mapping` pointing to the exact source lines that implement it.

- [ ] **Step 1: Remove old coarse-grained mappings**

Remove these existing mappings, which are now replaced by per-node mappings: `map-py-knowledge-model`, `map-py-spec-types`, `map-py-query`, `map-py-queryable-protocol`, `map-py-content-hash`, `map-py-repo-sync`, `map-py-repo-async`, `map-py-stores`, `map-py-objects`, `map-py-package-init`, `map-py-errors`.

Keep `map-py-module-root` (maps the overall pymodule root to lib.rs) and `map-py-repo-inner` (maps the RepoInner term to inner.rs).

- [ ] **Step 2: Add per-node artifact mappings**

Add an `art:mapping` element for every new `py:` node with an `id`. Use `sha256:placeholder` for all hashes (the fix-hash commands will compute real values in Step 3). For line ranges, read each source file and identify the exact lines implementing each method/class/property. Example pattern:

```xml
  <art:mapping id="map-py-km">
    <art:spec-ref node="py-km" revision="draft-1" node-hash="sha256:placeholder"/>
    <art:artifact repo="clayers" repo-revision="e3ff14a"
              path="crates/clayers-py/src/knowledge_model.rs">
      <art:range hash="sha256:placeholder" start-line="1" end-line="148"/>
    </art:artifact>
    <art:coverage>full</art:coverage>
  </art:mapping>
```

**File-to-node mapping guide** (determine exact line ranges by reading each file):

| Source file | Nodes to map |
|-------------|-------------|
| `knowledge_model.rs` | py-km (whole class), py-km-name, py-km-files, py-km-combined-xml, py-km-schema-dir, py-km-validate, py-km-check-drift, py-km-coverage, py-km-connectivity, py-km-query, py-km-fix-node-hashes, py-km-fix-artifact-hashes |
| `repo/repo_sync.rs` | py-repo (whole class), py-repo-import-xml, py-repo-export-xml, py-repo-build-tree, py-repo-commit, py-repo-create-branch, py-repo-delete-branch, py-repo-list-branches, py-repo-create-tag, py-repo-list-tags, py-repo-log, py-repo-diff-trees, py-repo-query |
| `repo/objects.rs` | py-author, py-author-name, py-author-email, py-commit-object and its properties, py-file-change and its properties |
| `xml/hash.rs` | py-content-hash, py-ch-hex, py-ch-prefixed, py-ch-from-canonical, py-ch-from-hex, py-ch-from-bytes |
| `query.rs` | py-query-result, py-qr-kind, py-qr-count, py-qr-values |
| `errors.rs` | py-clayers-error, py-xml-error, py-spec-error, py-repo-error |

For classes and their properties, use the whole class range. For individual methods, use just the method's lines. Properties that are single `#[getter]` functions get narrow ranges.

- [ ] **Step 3: Fix all hashes**

Run:
```bash
cargo run -p clayers -- artifact --fix-node-hash clayers/clayers/
cargo run -p clayers -- artifact --fix-artifact-hash clayers/clayers/
```

- [ ] **Step 4: Verify clean**

Run:
```bash
cargo run -p clayers -- validate clayers/clayers/
cargo run -p clayers -- artifact --drift clayers/clayers/
cargo run -p clayers -- artifact --coverage clayers/clayers/
```

Expected: validation OK, 0 drifted, 0 unmapped nodes.

- [ ] **Step 5: Commit**

```
Problem: artifact mappings are at section granularity, not per-API-node

Solution: add per-method/per-class artifact mappings for all py: nodes
```

---

### Task 6: Verify rendering

**Files:** none modified (verification only)

- [ ] **Step 1: Generate HTML doc and visually verify**

Run:
```bash
cargo run -p clayers -- doc clayers/clayers/ > /tmp/clayers-py-doc.html
open /tmp/clayers-py-doc.html
```

Verify: the Python Integration sections now render with structured API signatures instead of tables. Check that:
- Class signatures show constructor params
- Method signatures show params, types, defaults, return types
- Properties render as tables within their class
- Exceptions show base classes
- `py:doc` prose renders inline
- Keyword params show `*` separator
- Artifact mapping links still appear on nodes

- [ ] **Step 2: Verify XPath queryability**

Run:
```bash
cargo run -p clayers -- query '//py:class/@name' clayers/clayers/ --text
cargo run -p clayers -- query '//py:method[@of="py-repo"]/@name' clayers/clayers/ --text
cargo run -p clayers -- query '//py:method' clayers/clayers/ --count
```

Note: the CLI takes `<XPATH> [PATH]` (xpath first, path second).

Expected: class names listed, repo method names listed, method count > 0.

---

### Task 7: Update self-referential spec and LLM descriptions

**Files:**
- Modify: `clayers/clayers/overview.xml` (add Python to layer table)
- Modify: `clayers/clayers/integration.xml` (update llm:node entries for migrated sections)

- [ ] **Step 1: Add Python row to the layer table in overview.xml**

Add after the LLM row in the `layered-architecture` section's table:

```xml
        <pr:tr>
          <pr:td>Python</pr:td>
          <pr:td>Python API surface descriptions with structured signatures</pr:td>
        </pr:tr>
```

- [ ] **Step 2: Update llm:node entries in integration.xml**

The existing `llm:node` entries for the migrated sections still describe prose tables. Update them to reflect the new `py:` element structure. For example, `llm:node ref="python-knowledge-model-api"` should now say:

```xml
  <llm:node ref="python-knowledge-model-api">
    Describes the KnowledgeModel class using structured py: elements.
    The py:class contains py:property elements for name, files,
    combined_xml, schema_dir, and py:method elements for validate,
    check_drift, coverage, connectivity, query, fix_node_hashes,
    fix_artifact_hashes. Each method has typed parameters and return
    types. Prose examples show CI gates, knowledge base queries,
    coverage thresholds, and connectivity exploration.
  </llm:node>
```

Update similarly for `python-repo-api`, `python-queryable`, and `python-error-hierarchy`.

- [ ] **Step 3: Fix hashes and validate**

Run:
```bash
cargo run -p clayers -- artifact --fix-node-hash clayers/clayers/
cargo run -p clayers -- validate clayers/clayers/
cargo run -p clayers -- artifact --drift clayers/clayers/
cargo run -p clayers -- artifact --coverage clayers/clayers/ | grep -E "^coverage|unmapped"
```

Expected: validation OK, 0 drifted, 0 unmapped nodes.

- [ ] **Step 4: Commit**

```
Problem: the layered architecture table does not list the Python layer and LLM descriptions are stale

Solution: add Python row to overview.xml, update llm:node descriptions for migrated sections
```

<?xml version="1.0" encoding="UTF-8"?>
<xsl:stylesheet version="3.0"
    xmlns:xsl="http://www.w3.org/1999/XSL/Transform"
    xmlns:cmb="urn:clayers:combined"
    xmlns:pr="urn:clayers:prose"
    xmlns:trm="urn:clayers:terminology"
    xmlns:org="urn:clayers:organization"
    xmlns:rel="urn:clayers:relation"
    xmlns:dec="urn:clayers:decision"
    xmlns:src="urn:clayers:source"
    xmlns:pln="urn:clayers:plan"
    xmlns:art="urn:clayers:artifact"
    exclude-result-prefixes="cmb pr trm org rel dec src pln art">

  <!-- Escape a string for safe JSON embedding -->
  <xsl:template name="json-escape">
    <xsl:param name="text"/>
    <xsl:variable name="s1" select="replace($text, '\\', '\\\\')"/>
    <xsl:variable name="s2" select="replace($s1, '&quot;', '\\&quot;')"/>
    <xsl:variable name="s3" select="replace($s2, '&#10;', '\\n')"/>
    <xsl:variable name="s4" select="replace($s3, '&#13;', '\\r')"/>
    <xsl:variable name="s5" select="replace($s4, '&#9;', '\\t')"/>
    <xsl:value-of select="$s5"/>
  </xsl:template>

  <!-- Determine the page a node lives on (data-page attribute) -->
  <xsl:template name="node-page">
    <xsl:param name="node"/>
    <xsl:choose>
      <!-- Top-level prose section: its own page -->
      <xsl:when test="$node/self::pr:section and $node/parent::cmb:spec">
        <xsl:value-of select="$node/@id"/>
      </xsl:when>
      <!-- Nested prose section: ancestor top-level section's page -->
      <xsl:when test="$node/self::pr:section">
        <xsl:value-of select="$node/ancestor::pr:section[parent::cmb:spec]/@id"/>
      </xsl:when>
      <!-- Terms: glossary page -->
      <xsl:when test="$node/self::trm:term">glossary</xsl:when>
      <!-- Decisions, plans, maps: their own page -->
      <xsl:when test="$node/self::dec:decision or $node/self::pln:plan or $node/self::org:map">
        <xsl:value-of select="$node/@id"/>
      </xsl:when>
      <!-- Sources: sources page -->
      <xsl:when test="$node/self::src:source">sources</xsl:when>
      <!-- Artifact mappings: artifacts page -->
      <xsl:when test="$node/self::art:mapping">artifacts</xsl:when>
      <!-- Fallback -->
      <xsl:otherwise><xsl:value-of select="$node/@id"/></xsl:otherwise>
    </xsl:choose>
  </xsl:template>

  <!-- Determine the type string for a node -->
  <xsl:template name="node-type">
    <xsl:param name="node"/>
    <xsl:param name="root"/>
    <xsl:choose>
      <xsl:when test="$node/self::pr:section and $root//org:concept[@ref = $node/@id]">concept</xsl:when>
      <xsl:when test="$node/self::pr:section and $root//org:task[@ref = $node/@id]">task</xsl:when>
      <xsl:when test="$node/self::pr:section and $root//org:reference[@ref = $node/@id]">reference</xsl:when>
      <xsl:when test="$node/self::pr:section">section</xsl:when>
      <xsl:when test="$node/self::trm:term">term</xsl:when>
      <xsl:when test="$node/self::dec:decision">decision</xsl:when>
      <xsl:when test="$node/self::pln:plan">plan</xsl:when>
      <xsl:when test="$node/self::org:map">map</xsl:when>
      <xsl:when test="$node/self::src:source">source</xsl:when>
      <xsl:when test="$node/self::art:mapping">artifact</xsl:when>
      <xsl:otherwise>section</xsl:otherwise>
    </xsl:choose>
  </xsl:template>

  <!-- Determine the label for a node -->
  <xsl:template name="node-label">
    <xsl:param name="node"/>
    <xsl:variable name="raw">
      <xsl:choose>
        <xsl:when test="$node/self::pr:section"><xsl:value-of select="$node/pr:title"/></xsl:when>
        <xsl:when test="$node/self::trm:term"><xsl:value-of select="$node/trm:name"/></xsl:when>
        <xsl:when test="$node/self::pln:plan"><xsl:value-of select="$node/pln:title"/></xsl:when>
        <xsl:when test="$node/self::dec:decision"><xsl:value-of select="($node/dec:title, $node/@id)[1]"/></xsl:when>
        <xsl:when test="$node/self::src:source"><xsl:value-of select="($node/src:title, $node/@id)[1]"/></xsl:when>
        <xsl:when test="$node/self::org:map"><xsl:value-of select="($node/org:title, $node/@id)[1]"/></xsl:when>
        <xsl:otherwise><xsl:value-of select="$node/@id"/></xsl:otherwise>
      </xsl:choose>
    </xsl:variable>
    <xsl:call-template name="json-escape">
      <xsl:with-param name="text" select="normalize-space($raw)"/>
    </xsl:call-template>
  </xsl:template>

  <!-- Emit the full graph data JSON block -->
  <xsl:template name="emit-graph-data">
    <xsl:variable name="root" select="ancestor-or-self::cmb:spec"/>

    <!-- Collect all graph-eligible nodes -->
    <xsl:variable name="sections" select="$root//pr:section[@id]"/>
    <xsl:variable name="terms" select="$root//trm:term[@id]"/>
    <xsl:variable name="decisions" select="$root//dec:decision[@id]"/>
    <xsl:variable name="plans" select="$root//pln:plan[@id]"/>
    <xsl:variable name="maps" select="$root//org:map[@id]"/>
    <xsl:variable name="sources" select="$root//src:source[@id]"/>
    <xsl:variable name="artifacts" select="$root//art:mapping[@id]"/>
    <xsl:variable name="all-nodes" select="$sections | $terms | $decisions | $plans | $maps | $sources | $artifacts"/>

    <!-- Collect node IDs for edge filtering -->
    <xsl:variable name="node-ids" select="$all-nodes/@id"/>

    <script id="clayers-graph-data" type="application/json">
      <xsl:text>{"nodes":[</xsl:text>
      <xsl:for-each select="$all-nodes">
        <xsl:if test="position() > 1">,</xsl:if>
        <xsl:text>{"id":"</xsl:text>
        <xsl:call-template name="json-escape"><xsl:with-param name="text" select="@id"/></xsl:call-template>
        <xsl:text>","type":"</xsl:text>
        <xsl:call-template name="node-type">
          <xsl:with-param name="node" select="."/>
          <xsl:with-param name="root" select="$root"/>
        </xsl:call-template>
        <xsl:text>","label":"</xsl:text>
        <xsl:call-template name="node-label"><xsl:with-param name="node" select="."/></xsl:call-template>
        <xsl:text>","page":"</xsl:text>
        <xsl:call-template name="node-page"><xsl:with-param name="node" select="."/></xsl:call-template>
        <xsl:text>"}</xsl:text>
      </xsl:for-each>
      <xsl:text>],"edges":[</xsl:text>
      <!-- Only emit edges where both endpoints are in our node set -->
      <xsl:variable name="valid-rels" select="$root//rel:relation[@from = $node-ids and @to = $node-ids]"/>
      <xsl:for-each select="$valid-rels">
        <xsl:if test="position() > 1">,</xsl:if>
        <xsl:text>{"from":"</xsl:text>
        <xsl:call-template name="json-escape"><xsl:with-param name="text" select="@from"/></xsl:call-template>
        <xsl:text>","to":"</xsl:text>
        <xsl:call-template name="json-escape"><xsl:with-param name="text" select="@to"/></xsl:call-template>
        <xsl:text>","type":"</xsl:text>
        <xsl:call-template name="json-escape"><xsl:with-param name="text" select="@type"/></xsl:call-template>
        <xsl:text>"}</xsl:text>
      </xsl:for-each>
      <xsl:text>]}</xsl:text>
    </script>
  </xsl:template>

</xsl:stylesheet>

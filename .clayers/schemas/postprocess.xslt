<?xml version="1.0" encoding="UTF-8"?>
<!--
  XSLT 1.0 transform: synthesize rel:relation type="references" from
  trm:ref and src:cite elements.

  Identity transform that copies all nodes unchanged, then appends one
  rel:relation element for each unique (ancestor-id, term) pair found
  in trm:ref elements, and one for each unique (ancestor-id, source)
  pair found in src:cite elements. The "from" attribute is the nearest
  ancestor with an @id attribute; "to" is the referenced ID.

  Applied by clayers-cli during combined document assembly.
-->
<xsl:stylesheet version="1.0"
    xmlns:xsl="http://www.w3.org/1999/XSL/Transform"
    xmlns:trm="urn:clayers:terminology"
    xmlns:src="urn:clayers:source"
    xmlns:rel="urn:clayers:relation">

  <!-- Muenchian grouping key: unique (ancestor-id, term) pairs -->
  <xsl:key name="ref-pair"
           match="//trm:ref[@term]"
           use="concat(ancestor::*[@id][1]/@id, '|', @term)"/>

  <!-- Muenchian grouping key: unique (ancestor-id, source) pairs for citations -->
  <xsl:key name="cite-pair"
           match="//src:cite[@source]"
           use="concat(ancestor::*[@id][1]/@id, '|', @source)"/>

  <!-- Identity transform: copy everything -->
  <xsl:template match="@*|node()">
    <xsl:copy>
      <xsl:apply-templates select="@*|node()"/>
    </xsl:copy>
  </xsl:template>

  <!-- At the root element, copy children then append synthesized relations -->
  <xsl:template match="/*">
    <xsl:copy>
      <xsl:apply-templates select="@*|node()"/>

      <!-- For each unique (ancestor-id, term) pair, emit a relation -->
      <xsl:for-each select="//trm:ref[@term]
                            [ancestor::*[@id]]
                            [generate-id() = generate-id(key('ref-pair',
                              concat(ancestor::*[@id][1]/@id, '|', @term))[1])]">
        <xsl:variable name="from-id" select="ancestor::*[@id][1]/@id"/>
        <xsl:variable name="to-id" select="@term"/>
        <!-- Skip self-references -->
        <xsl:if test="$from-id != $to-id">
          <rel:relation type="references" from="{$from-id}" to="{$to-id}"/>
        </xsl:if>
      </xsl:for-each>

      <!-- For each unique (ancestor-id, source) pair, emit a relation -->
      <xsl:for-each select="//src:cite[@source]
                            [ancestor::*[@id]]
                            [generate-id() = generate-id(key('cite-pair',
                              concat(ancestor::*[@id][1]/@id, '|', @source))[1])]">
        <xsl:variable name="from-id" select="ancestor::*[@id][1]/@id"/>
        <xsl:variable name="to-id" select="@source"/>
        <!-- Skip self-references -->
        <xsl:if test="$from-id != $to-id">
          <rel:relation type="references" from="{$from-id}" to="{$to-id}"/>
        </xsl:if>
      </xsl:for-each>
    </xsl:copy>
  </xsl:template>

</xsl:stylesheet>

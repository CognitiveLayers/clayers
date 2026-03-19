<?xml version="1.0" encoding="UTF-8"?>
<xsl:stylesheet version="3.0"
    xmlns:xsl="http://www.w3.org/1999/XSL/Transform"
    xmlns:rel="urn:clayers:relation"
    xmlns:pr="urn:clayers:prose"
    xmlns:trm="urn:clayers:terminology"
    xmlns:cmb="urn:clayers:combined"
    exclude-result-prefixes="rel pr trm cmb">

  <!-- Named template to render relation links for a section -->
  <xsl:template name="render-relations">
    <xsl:param name="rels-from"/>
    <xsl:param name="rels-to"/>

    <div class="relations">
      <div class="relations-title">Relations</div>

      <!-- Outgoing relations grouped by type -->
      <xsl:for-each-group select="$rels-from" group-by="@type">
        <xsl:sort select="current-grouping-key()"/>
        <div class="relation-group">
          <span class="relation-type relation-type-{current-grouping-key()}">
            <xsl:value-of select="translate(current-grouping-key(), '-', ' ')"/>
          </span>
          <span class="relation-targets">
            <xsl:for-each select="current-group()">
              <xsl:if test="position() > 1">, </xsl:if>
              <xsl:variable name="target" select="@to"/>
              <xsl:call-template name="resolve-node-link">
                <xsl:with-param name="node-id" select="$target"/>
              </xsl:call-template>
            </xsl:for-each>
          </span>
          <!-- Relation notes -->
          <xsl:for-each select="current-group()[rel:note]">
            <div class="relation-note">
              <xsl:variable name="target" select="@to"/>
              <xsl:call-template name="resolve-node-link">
                <xsl:with-param name="node-id" select="$target"/>
              </xsl:call-template>
              <xsl:text>: </xsl:text>
              <xsl:value-of select="rel:note"/>
            </div>
          </xsl:for-each>
        </div>
      </xsl:for-each-group>

      <!-- Incoming relations: show the inverse label so it reads naturally -->
      <xsl:for-each-group select="$rels-to" group-by="@type">
        <xsl:sort select="current-grouping-key()"/>
        <div class="relation-group">
          <span class="relation-type relation-type-{current-grouping-key()}" style="opacity:0.7;">
            <xsl:choose>
              <xsl:when test="current-grouping-key() = 'depends-on'">depended on by</xsl:when>
              <xsl:when test="current-grouping-key() = 'refines'">refined by</xsl:when>
              <xsl:when test="current-grouping-key() = 'implements'">implemented by</xsl:when>
              <xsl:when test="current-grouping-key() = 'precedes'">preceded by</xsl:when>
              <xsl:when test="current-grouping-key() = 'constrains'">constrained by</xsl:when>
              <xsl:when test="current-grouping-key() = 'references'">referenced by</xsl:when>
              <xsl:when test="current-grouping-key() = 'conflicts-with'">conflicts with</xsl:when>
              <xsl:otherwise><xsl:value-of select="current-grouping-key()"/> by</xsl:otherwise>
            </xsl:choose>
          </span>
          <span class="relation-targets">
            <xsl:for-each select="current-group()">
              <xsl:if test="position() > 1">, </xsl:if>
              <xsl:variable name="source" select="@from"/>
              <xsl:call-template name="resolve-node-link">
                <xsl:with-param name="node-id" select="$source"/>
              </xsl:call-template>
            </xsl:for-each>
          </span>
          <xsl:for-each select="current-group()[rel:note]">
            <div class="relation-note">
              <xsl:variable name="source" select="@from"/>
              <xsl:call-template name="resolve-node-link">
                <xsl:with-param name="node-id" select="$source"/>
              </xsl:call-template>
              <xsl:text>: </xsl:text>
              <xsl:value-of select="rel:note"/>
            </div>
          </xsl:for-each>
        </div>
      </xsl:for-each-group>
    </div>
  </xsl:template>

  <!-- Resolve a node ID to a clickable link with best-effort title -->
  <xsl:template name="resolve-node-link">
    <xsl:param name="node-id"/>
    <xsl:variable name="sec-title" select="ancestor::cmb:spec//pr:section[@id = $node-id]/pr:title"/>
    <xsl:variable name="term-name" select="ancestor::cmb:spec//trm:term[@id = $node-id]/trm:name"/>
    <a href="#{$node-id}">
      <xsl:choose>
        <xsl:when test="$sec-title"><xsl:value-of select="$sec-title"/></xsl:when>
        <xsl:when test="$term-name"><xsl:value-of select="$term-name"/></xsl:when>
        <xsl:otherwise><xsl:value-of select="$node-id"/></xsl:otherwise>
      </xsl:choose>
    </a>
  </xsl:template>

</xsl:stylesheet>

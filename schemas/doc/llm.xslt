<?xml version="1.0" encoding="UTF-8"?>
<xsl:stylesheet version="3.0"
    xmlns:xsl="http://www.w3.org/1999/XSL/Transform"
    xmlns:xs="http://www.w3.org/2001/XMLSchema"
    xmlns:llm="urn:clayers:llm"
    xmlns:md="urn:clayers:markdown"
    exclude-result-prefixes="llm xs md">

  <xsl:import href="markdown.xslt"/>

  <!-- LLM node description: plain text (default) -->
  <xsl:template match="llm:node[not(@format) or @format='text']" mode="inline">
    <details>
      <summary>Machine description (<xsl:value-of select="@ref"/>)</summary>
      <p style="color: var(--badge-gray); font-size: 0.9em;">
        <xsl:value-of select="normalize-space(.)"/>
      </p>
    </details>
  </xsl:template>

  <!-- LLM node description: markdown format -->
  <xsl:template match="llm:node[@format='markdown']" mode="inline">
    <details class="llm-markdown">
      <summary>Agent guidance (<xsl:value-of select="@ref"/>)</summary>
      <div class="markdown-body">
        <xsl:sequence select="md:to-html(string(.))"/>
      </div>
    </details>
  </xsl:template>

  <!-- LLM schema description (collapsible, muted) -->
  <xsl:template match="llm:schema" mode="inline">
    <details>
      <summary>
        <xsl:text>Schema description (</xsl:text>
        <xsl:value-of select="@namespace"/>
        <xsl:if test="@element">
          <xsl:text>::</xsl:text>
          <xsl:value-of select="@element"/>
        </xsl:if>
        <xsl:text>)</xsl:text>
      </summary>
      <p style="color: var(--badge-gray); font-size: 0.9em;">
        <xsl:value-of select="normalize-space(.)"/>
      </p>
    </details>
  </xsl:template>

</xsl:stylesheet>

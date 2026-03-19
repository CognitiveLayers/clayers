<?xml version="1.0" encoding="UTF-8"?>
<xsl:stylesheet version="3.0"
    xmlns:xsl="http://www.w3.org/1999/XSL/Transform"
    xmlns:llm="urn:clayers:llm"
    exclude-result-prefixes="llm">

  <!-- LLM node description (collapsible, muted) -->
  <xsl:template match="llm:node" mode="inline">
    <details>
      <summary>Machine description (<xsl:value-of select="@ref"/>)</summary>
      <p style="color: var(--badge-gray); font-size: 0.9em;">
        <xsl:value-of select="normalize-space(.)"/>
      </p>
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

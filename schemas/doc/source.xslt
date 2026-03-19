<?xml version="1.0" encoding="UTF-8"?>
<xsl:stylesheet version="3.0"
    xmlns:xsl="http://www.w3.org/1999/XSL/Transform"
    xmlns:src="urn:clayers:source"
    xmlns:cmb="urn:clayers:combined"
    exclude-result-prefixes="src cmb">

  <!-- Source entry in bibliography -->
  <xsl:template match="src:source" mode="bibliography">
    <div class="card" id="{@id}">
      <h4>
        <xsl:choose>
          <xsl:when test="src:title"><xsl:value-of select="src:title"/></xsl:when>
          <xsl:otherwise><xsl:value-of select="@id"/></xsl:otherwise>
        </xsl:choose>
      </h4>
      <xsl:if test="src:author">
        <p><strong>Author:</strong> <xsl:value-of select="src:author"/></p>
      </xsl:if>
      <xsl:if test="@url">
        <p><a href="{@url}"><xsl:value-of select="@url"/></a></p>
      </xsl:if>
      <xsl:if test="src:overview">
        <p><xsl:value-of select="src:overview"/></p>
      </xsl:if>
      <div style="margin-top:0.5rem;font-size:0.8125rem;color:hsl(var(--muted-foreground));">
        <xsl:if test="@type">
          <span class="badge badge-gray"><xsl:value-of select="@type"/></span>
        </xsl:if>
        <xsl:if test="src:published">
          <xsl:text> published </xsl:text>
          <xsl:value-of select="src:published"/>
        </xsl:if>
        <xsl:if test="@accessed">
          <xsl:text> | accessed </xsl:text>
          <xsl:value-of select="@accessed"/>
        </xsl:if>
      </div>
    </div>
  </xsl:template>

  <!-- Inline citation -->
  <xsl:template match="src:cite">
    <xsl:variable name="source-id" select="@source"/>
    <a href="#{$source-id}" class="term-ref">
      <xsl:apply-templates/>
    </a>
  </xsl:template>

</xsl:stylesheet>

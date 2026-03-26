<?xml version="1.0" encoding="UTF-8"?>
<xsl:stylesheet version="3.0"
    xmlns:xsl="http://www.w3.org/1999/XSL/Transform"
    xmlns:cnt="urn:clayers:content"
    exclude-result-prefixes="cnt">

  <!-- Content node -->
  <xsl:template match="cnt:content">
    <div class="card" id="{@id}">
      <h4>
        <xsl:value-of select="@id"/>
        <xsl:if test="@media-type">
          <xsl:text> </xsl:text>
          <span class="badge badge-gray" style="font-size:0.65rem;">
            <xsl:value-of select="@media-type"/>
          </span>
        </xsl:if>
      </h4>
      <xsl:if test="@url">
        <p><strong>URL:</strong> <code><xsl:value-of select="@url"/></code></p>
      </xsl:if>
      <p><strong>Hash:</strong> <code style="font-size:0.85em;"><xsl:value-of select="@hash"/></code></p>
      <xsl:if test="cnt:body">
        <details>
          <summary>Embedded content</summary>
          <pre style="font-size:0.85em;"><code><xsl:value-of select="cnt:body"/></code></pre>
        </details>
      </xsl:if>
    </div>
  </xsl:template>

  <!-- Inline content reference -->
  <xsl:template match="cnt:ref">
    <a href="#{@content}" class="cnt-ref">
      <xsl:apply-templates/>
    </a>
  </xsl:template>

</xsl:stylesheet>

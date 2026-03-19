<?xml version="1.0" encoding="UTF-8"?>
<xsl:stylesheet version="3.0"
    xmlns:xsl="http://www.w3.org/1999/XSL/Transform">

  <!-- Catch-all for any urn:clayers:* element without a specific template.
       This file is imported FIRST so it has the lowest import precedence.
       Any element matched here has no template in any layer stylesheet. -->
  <xsl:template match="*[starts-with(namespace-uri(), 'urn:clayers:')]">
    <div style="border:2px solid var(--clr-red);border-radius:var(--radius);padding:0.5rem 0.75rem;margin:0.5rem 0;font-size:0.8125rem;background:color-mix(in srgb, var(--clr-red) 8%, transparent);">
      <strong style="color:var(--clr-red);">Unhandled element</strong>
      <xsl:text> </xsl:text>
      <code><xsl:value-of select="name()"/></code>
      <xsl:if test="@id">
        <xsl:text> id="</xsl:text><xsl:value-of select="@id"/><xsl:text>"</xsl:text>
      </xsl:if>
      <xsl:if test="@ref">
        <xsl:text> ref="</xsl:text><xsl:value-of select="@ref"/><xsl:text>"</xsl:text>
      </xsl:if>
    </div>
  </xsl:template>

</xsl:stylesheet>

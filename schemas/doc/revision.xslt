<?xml version="1.0" encoding="UTF-8"?>
<xsl:stylesheet version="3.0"
    xmlns:xsl="http://www.w3.org/1999/XSL/Transform"
    xmlns:rev="urn:clayers:revision"
    exclude-result-prefixes="rev">

  <!-- Revision metadata -->
  <xsl:template match="rev:revision" mode="inline">
    <div class="card" id="{@id}">
      <p>
        <strong>Revision:</strong>
        <xsl:text> </xsl:text>
        <xsl:value-of select="@id"/>
        <xsl:if test="rev:date">
          <xsl:text> (</xsl:text>
          <xsl:value-of select="rev:date"/>
          <xsl:text>)</xsl:text>
        </xsl:if>
      </p>
    </div>
  </xsl:template>

</xsl:stylesheet>

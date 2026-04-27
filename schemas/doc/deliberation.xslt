<?xml version="1.0" encoding="UTF-8"?>
<xsl:stylesheet version="3.0"
    xmlns:xsl="http://www.w3.org/1999/XSL/Transform"
    xmlns:delib="urn:clayers:deliberation"
    exclude-result-prefixes="delib">

  <xsl:template match="delib:choice-set">
    <div class="card deliberation-choice-set" id="{@id}">
      <h4>
        <xsl:value-of select="delib:title"/>
        <xsl:text> </xsl:text>
        <span class="badge badge-blue"><xsl:value-of select="delib:status"/></span>
      </h4>
      <xsl:if test="@ref">
        <p><strong>Context:</strong> <a href="#{@ref}"><xsl:value-of select="@ref"/></a></p>
      </xsl:if>
      <xsl:if test="delib:context">
        <p><xsl:apply-templates select="delib:context/node()"/></p>
      </xsl:if>
      <xsl:for-each select="delib:option">
        <div class="deliberation-option" id="{@id}" style="margin:0.75rem 0;padding:0.75rem;border:1px solid hsl(var(--border));border-radius:var(--radius);">
          <strong><xsl:value-of select="delib:title"/></strong>
          <xsl:text> </xsl:text>
          <span class="badge badge-gray"><xsl:value-of select="@outcome"/></span>
          <xsl:if test="delib:condition">
            <p><strong>Condition:</strong> <xsl:apply-templates select="delib:condition/node()"/></p>
          </xsl:if>
          <xsl:if test="delib:rationale">
            <p><strong>Rationale:</strong> <xsl:apply-templates select="delib:rationale/node()"/></p>
          </xsl:if>
        </div>
      </xsl:for-each>
    </div>
  </xsl:template>

</xsl:stylesheet>

<?xml version="1.0" encoding="UTF-8"?>
<xsl:stylesheet version="3.0"
    xmlns:xsl="http://www.w3.org/1999/XSL/Transform"
    xmlns:trm="urn:clayers:terminology"
    xmlns:cmb="urn:clayers:combined"
    exclude-result-prefixes="trm cmb">

  <!-- Term card in glossary -->
  <xsl:template match="trm:term" mode="glossary">
    <div class="card" id="{@id}">
      <h4><xsl:value-of select="trm:name"/></h4>
      <p><xsl:value-of select="trm:definition"/></p>
    </div>
  </xsl:template>

  <!-- Inline term reference with tooltip -->
  <xsl:template match="trm:ref">
    <xsl:variable name="term-id" select="@term"/>
    <xsl:variable name="def" select="ancestor::cmb:spec//trm:term[@id = $term-id]/trm:definition"/>
    <a class="term-ref" href="#{$term-id}">
      <xsl:if test="$def">
        <xsl:attribute name="title" select="$def"/>
      </xsl:if>
      <xsl:apply-templates/>
    </a>
  </xsl:template>

</xsl:stylesheet>

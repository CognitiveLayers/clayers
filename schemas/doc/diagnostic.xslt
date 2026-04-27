<?xml version="1.0" encoding="UTF-8"?>
<xsl:stylesheet version="3.0"
    xmlns:xsl="http://www.w3.org/1999/XSL/Transform"
    xmlns:diag="urn:clayers:diagnostic"
    exclude-result-prefixes="diag">

  <xsl:template match="diag:issue">
    <section class="troubleshooting diagnostic-issue" id="{@id}">
      <h4>
        <xsl:value-of select="diag:title"/>
        <xsl:if test="diag:severity">
          <xsl:text> </xsl:text>
          <span class="badge badge-gray"><xsl:value-of select="diag:severity"/></span>
        </xsl:if>
      </h4>
      <xsl:if test="@ref">
        <p><strong>Affects:</strong> <a href="#{@ref}"><xsl:value-of select="@ref"/></a></p>
      </xsl:if>
      <xsl:apply-templates select="diag:condition | diag:cause | diag:remedy | diag:responsible-party | diag:verifies-with"/>
    </section>
  </xsl:template>

  <xsl:template match="diag:condition | diag:cause | diag:remedy | diag:responsible-party | diag:verifies-with">
    <div class="trouble-part trouble-{local-name()}">
      <div class="trouble-label"><xsl:value-of select="replace(local-name(), '-', ' ')"/></div>
      <xsl:choose>
        <xsl:when test="self::diag:verifies-with and @test">
          <a href="#{@test}"><xsl:value-of select="@test"/></a>
          <xsl:if test="node()">
            <xsl:text> </xsl:text>
            <xsl:apply-templates/>
          </xsl:if>
        </xsl:when>
        <xsl:otherwise>
          <xsl:apply-templates/>
        </xsl:otherwise>
      </xsl:choose>
    </div>
  </xsl:template>

</xsl:stylesheet>

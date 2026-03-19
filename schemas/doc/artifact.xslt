<?xml version="1.0" encoding="UTF-8"?>
<xsl:stylesheet version="3.0"
    xmlns:xsl="http://www.w3.org/1999/XSL/Transform"
    xmlns:art="urn:clayers:artifact"
    exclude-result-prefixes="art">

  <!-- Artifact mapping card -->
  <xsl:template match="art:mapping">
    <div class="card" id="{@id}">
      <h4>
        <xsl:value-of select="@id"/>
        <xsl:text> </xsl:text>
        <xsl:variable name="cov" select="art:coverage"/>
        <xsl:choose>
          <xsl:when test="$cov = 'full'">
            <span class="badge badge-green">full</span>
          </xsl:when>
          <xsl:when test="$cov = 'partial'">
            <span class="badge badge-yellow">partial</span>
          </xsl:when>
          <xsl:when test="$cov = 'none'">
            <span class="badge badge-red">none</span>
          </xsl:when>
        </xsl:choose>
      </h4>
      <p>
        <strong>Spec node:</strong>
        <xsl:text> </xsl:text>
        <a href="#{art:spec-ref/@node}"><xsl:value-of select="art:spec-ref/@node"/></a>
        <xsl:if test="art:spec-ref/@revision">
          <xsl:text> (rev: </xsl:text>
          <xsl:value-of select="art:spec-ref/@revision"/>
          <xsl:text>)</xsl:text>
        </xsl:if>
      </p>
      <xsl:if test="art:artifact">
        <p>
          <strong>File:</strong>
          <xsl:text> </xsl:text>
          <code><xsl:value-of select="art:artifact/@path"/></code>
          <xsl:for-each select="art:artifact/art:range">
            <xsl:if test="@start-line and @end-line">
              <xsl:text> L</xsl:text>
              <xsl:value-of select="@start-line"/>
              <xsl:text>-</xsl:text>
              <xsl:value-of select="@end-line"/>
            </xsl:if>
          </xsl:for-each>
        </p>
      </xsl:if>
      <xsl:if test="art:note">
        <p><xsl:value-of select="art:note"/></p>
      </xsl:if>
    </div>
  </xsl:template>

</xsl:stylesheet>

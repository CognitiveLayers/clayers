<?xml version="1.0" encoding="UTF-8"?>
<xsl:stylesheet version="3.0"
    xmlns:xsl="http://www.w3.org/1999/XSL/Transform"
    xmlns:dec="urn:clayers:decision"
    xmlns:pr="urn:clayers:prose"
    xmlns:cmb="urn:clayers:combined"
    exclude-result-prefixes="dec pr cmb">

  <!-- Decision card in the Decisions section -->
  <xsl:template match="dec:decision">
    <div class="card" id="{@id}">
      <h4>
        <!-- Find the prose section this decision references -->
        <xsl:variable name="ref" select="@ref"/>
        <xsl:variable name="title" select="ancestor::cmb:spec//pr:section[@id = $ref]/pr:title"/>
        <xsl:choose>
          <xsl:when test="$title"><xsl:value-of select="$title"/></xsl:when>
          <xsl:otherwise><xsl:value-of select="@id"/></xsl:otherwise>
        </xsl:choose>
        <xsl:text> </xsl:text>
        <xsl:call-template name="decision-status-badge">
          <xsl:with-param name="status" select="dec:status"/>
        </xsl:call-template>
      </h4>

      <!-- Decision-level rationale -->
      <xsl:if test="dec:rationale">
        <p><xsl:value-of select="dec:rationale"/></p>
      </xsl:if>

      <xsl:if test="dec:supersedes">
        <p>
          <xsl:text>Supersedes: </xsl:text>
          <a href="#{dec:supersedes/@decision}">
            <xsl:value-of select="dec:supersedes/@decision"/>
          </a>
        </p>
      </xsl:if>

      <xsl:if test="dec:alternative">
        <h5>Alternatives considered</h5>
        <xsl:for-each select="dec:alternative">
          <div style="margin-left: 1rem; margin-bottom: 0.5rem;">
            <strong><xsl:value-of select="dec:title"/></strong>
            <xsl:if test="dec:rationale">
              <xsl:text> &#8212; </xsl:text>
              <xsl:value-of select="dec:rationale"/>
            </xsl:if>
          </div>
        </xsl:for-each>
      </xsl:if>
    </div>
  </xsl:template>

  <!-- Decision status badge -->
  <xsl:template name="decision-status-badge">
    <xsl:param name="status"/>
    <xsl:choose>
      <xsl:when test="$status = 'accepted'">
        <span class="badge badge-green">accepted</span>
      </xsl:when>
      <xsl:when test="$status = 'proposed'">
        <span class="badge badge-yellow">proposed</span>
      </xsl:when>
      <xsl:when test="$status = 'deprecated'">
        <span class="badge badge-red">deprecated</span>
      </xsl:when>
      <xsl:when test="$status = 'superseded'">
        <span class="badge badge-gray">superseded</span>
      </xsl:when>
      <xsl:otherwise>
        <span class="badge badge-gray"><xsl:value-of select="$status"/></span>
      </xsl:otherwise>
    </xsl:choose>
  </xsl:template>

</xsl:stylesheet>

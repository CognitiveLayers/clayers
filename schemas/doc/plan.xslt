<?xml version="1.0" encoding="UTF-8"?>
<xsl:stylesheet version="3.0"
    xmlns:xsl="http://www.w3.org/1999/XSL/Transform"
    xmlns:pln="urn:clayers:plan"
    exclude-result-prefixes="pln">

  <!-- Plan card -->
  <xsl:template match="pln:plan">
    <div class="card" id="{@id}">
      <h4>
        <xsl:value-of select="pln:title"/>
        <xsl:text> </xsl:text>
        <xsl:call-template name="plan-status-badge">
          <xsl:with-param name="status" select="pln:status"/>
        </xsl:call-template>
      </h4>
      <xsl:if test="pln:overview">
        <p class="shortdesc"><xsl:value-of select="pln:overview"/></p>
      </xsl:if>
      <xsl:if test="pln:item">
        <ol>
          <xsl:apply-templates select="pln:item"/>
        </ol>
      </xsl:if>
    </div>
  </xsl:template>

  <!-- Plan item -->
  <xsl:template match="pln:item">
    <li>
      <xsl:if test="@id"><xsl:attribute name="id" select="@id"/></xsl:if>
      <strong><xsl:value-of select="pln:title"/></strong>
      <xsl:if test="pln:item-status">
        <xsl:text> </xsl:text>
        <xsl:call-template name="plan-status-badge">
          <xsl:with-param name="status" select="pln:item-status"/>
        </xsl:call-template>
      </xsl:if>
      <xsl:if test="pln:description">
        <p><xsl:apply-templates select="pln:description/node()"/></p>
      </xsl:if>
      <xsl:if test="pln:acceptance">
        <xsl:apply-templates select="pln:acceptance"/>
      </xsl:if>
      <xsl:if test="pln:item">
        <ol><xsl:apply-templates select="pln:item"/></ol>
      </xsl:if>
    </li>
  </xsl:template>

  <!-- Acceptance criteria -->
  <xsl:template match="pln:acceptance">
    <div style="margin-left: 1rem; font-size: 0.9em;">
      <xsl:for-each select="pln:criterion">
        <div style="margin-bottom: 0.75rem;">
          <div>
            <xsl:text>&#10003; </xsl:text>
            <xsl:apply-templates select="text()"/>
            <xsl:if test="@test">
              <xsl:text> </xsl:text>
              <a href="#{@test}" class="badge badge-blue" style="font-size:0.65rem; text-decoration:none;">
                <xsl:value-of select="@test"/>
              </a>
            </xsl:if>
          </div>
        </div>
      </xsl:for-each>
    </div>
  </xsl:template>

  <!-- Plan status badge -->
  <xsl:template name="plan-status-badge">
    <xsl:param name="status"/>
    <xsl:choose>
      <xsl:when test="$status = 'completed' or $status = 'done'">
        <span class="badge badge-green"><xsl:value-of select="$status"/></span>
      </xsl:when>
      <xsl:when test="$status = 'proposed' or $status = 'active' or $status = 'in-progress'">
        <span class="badge badge-yellow"><xsl:value-of select="$status"/></span>
      </xsl:when>
      <xsl:when test="$status = 'abandoned' or $status = 'rejected'">
        <span class="badge badge-red"><xsl:value-of select="$status"/></span>
      </xsl:when>
      <xsl:when test="$status = 'pending'">
        <span class="badge badge-blue"><xsl:value-of select="$status"/></span>
      </xsl:when>
      <xsl:otherwise>
        <span class="badge badge-gray"><xsl:value-of select="$status"/></span>
      </xsl:otherwise>
    </xsl:choose>
  </xsl:template>

</xsl:stylesheet>

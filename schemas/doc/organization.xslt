<?xml version="1.0" encoding="UTF-8"?>
<xsl:stylesheet version="3.0"
    xmlns:xsl="http://www.w3.org/1999/XSL/Transform"
    xmlns:org="urn:clayers:organization"
    xmlns:pr="urn:clayers:prose"
    xmlns:cmb="urn:clayers:combined"
    exclude-result-prefixes="org pr cmb">

  <!-- Badge for concept sections -->
  <xsl:template match="org:concept" mode="badge">
    <span class="org-badge org-concept">concept</span>
  </xsl:template>

  <!-- Badge for task sections -->
  <xsl:template match="org:task" mode="badge">
    <span class="org-badge org-task">task</span>
  </xsl:template>

  <!-- Badge for reference sections -->
  <xsl:template match="org:reference" mode="badge">
    <span class="org-badge org-reference">reference</span>
  </xsl:template>

  <!-- Reading map -->
  <xsl:template match="org:map">
    <div class="card" id="{@id}">
      <h4>
        <xsl:value-of select="org:title"/>
        <span class="org-badge" style="background:var(--accent);color:#fff;margin-left:0.5rem;">reading map</span>
      </h4>
      <xsl:if test="@audience">
        <p style="font-size:0.85em;color:var(--badge-gray);">
          <xsl:text>Audience: </xsl:text><xsl:value-of select="@audience"/>
          <xsl:if test="@experience-level">
            <xsl:text> | Level: </xsl:text><xsl:value-of select="@experience-level"/>
          </xsl:if>
        </p>
      </xsl:if>
      <xsl:apply-templates select="org:part"/>
      <!-- Direct topicrefs (no parts) -->
      <xsl:if test="org:topicref">
        <ol>
          <xsl:apply-templates select="org:topicref"/>
        </ol>
      </xsl:if>
    </div>
  </xsl:template>

  <!-- Reading map part -->
  <xsl:template match="org:part">
    <div style="margin-bottom: 0.75rem;">
      <h5 style="margin-bottom:0.25rem;">
        <xsl:value-of select="org:title"/>
        <xsl:if test="@audience">
          <span class="badge badge-gray" style="font-size:0.6rem;margin-left:0.35rem;">
            <xsl:value-of select="@audience"/>
          </span>
        </xsl:if>
        <xsl:if test="@role">
          <span class="badge badge-blue" style="font-size:0.6rem;margin-left:0.35rem;">
            <xsl:value-of select="@role"/>
          </span>
        </xsl:if>
      </h5>
      <ol style="margin-top:0.25rem;">
        <xsl:apply-templates select="org:topicref"/>
      </ol>
    </div>
  </xsl:template>

  <!-- Topic reference in a reading map -->
  <xsl:template match="org:topicref">
    <li>
      <xsl:if test="@required = 'false'">
        <xsl:attribute name="style">opacity: 0.7;</xsl:attribute>
      </xsl:if>
      <a href="#{@ref}">
        <xsl:choose>
          <xsl:when test="org:title"><xsl:value-of select="org:title"/></xsl:when>
          <xsl:otherwise>
            <xsl:variable name="ref" select="@ref"/>
            <xsl:variable name="sec-title" select="ancestor::cmb:spec//pr:section[@id = $ref]/pr:title"/>
            <xsl:choose>
              <xsl:when test="$sec-title"><xsl:value-of select="$sec-title"/></xsl:when>
              <xsl:otherwise><xsl:value-of select="@ref"/></xsl:otherwise>
            </xsl:choose>
          </xsl:otherwise>
        </xsl:choose>
      </a>
      <xsl:if test="@required = 'false'">
        <span style="font-size:0.8em;color:var(--badge-gray);"> (optional)</span>
      </xsl:if>
      <xsl:if test="@role">
        <span class="badge badge-blue" style="font-size:0.55rem;margin-left:0.25rem;">
          <xsl:value-of select="@role"/>
        </span>
      </xsl:if>
      <xsl:if test="org:purpose">
        <div style="font-size:0.85em;color:var(--badge-gray);margin-left:0.5rem;">
          <xsl:value-of select="org:purpose"/>
        </div>
      </xsl:if>
      <!-- Nested topicrefs -->
      <xsl:if test="org:topicref">
        <ol style="margin-top:0.15rem;">
          <xsl:apply-templates select="org:topicref"/>
        </ol>
      </xsl:if>
    </li>
  </xsl:template>

</xsl:stylesheet>

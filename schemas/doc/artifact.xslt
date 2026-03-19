<?xml version="1.0" encoding="UTF-8"?>
<xsl:stylesheet version="3.0"
    xmlns:xsl="http://www.w3.org/1999/XSL/Transform"
    xmlns:art="urn:clayers:artifact"
    xmlns:doc="urn:clayers:doc"
    xmlns:cmb="urn:clayers:combined"
    exclude-result-prefixes="art doc cmb">

  <!-- Artifact mapping card -->
  <xsl:template match="art:mapping">
    <xsl:variable name="mid" select="@id"/>
    <xsl:variable name="drift" select="ancestor::cmb:spec//doc:drift[@mapping = $mid]"/>
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
        <xsl:if test="$drift/@status = 'spec-drifted' or $drift/@status = 'artifact-drifted'">
          <xsl:text> </xsl:text>
          <span class="badge {if ($drift/@status = 'spec-drifted') then 'badge-drift-spec' else 'badge-drift-artifact'}">
            <xsl:value-of select="if ($drift/@status = 'spec-drifted') then 'spec drifted' else 'artifact drifted'"/>
          </span>
        </xsl:if>
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
      <!-- Code fragments from doc:report -->
      <xsl:variable name="fragments" select="ancestor::cmb:spec//doc:fragment[@mapping = $mid]"/>
      <xsl:for-each select="$fragments">
        <div class="code-fragment">
          <details>
            <summary>
              <xsl:text>View source (</xsl:text>
              <xsl:value-of select="@path"/>
              <xsl:if test="@start and @end">
                <xsl:text>:</xsl:text>
                <xsl:value-of select="@start"/>
                <xsl:text>-</xsl:text>
                <xsl:value-of select="@end"/>
              </xsl:if>
              <xsl:text>)</xsl:text>
            </summary>
            <pre><code class="language-{@language}"><xsl:value-of select="."/></code></pre>
          </details>
        </div>
      </xsl:for-each>
    </div>
  </xsl:template>

</xsl:stylesheet>

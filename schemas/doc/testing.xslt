<?xml version="1.0" encoding="UTF-8"?>
<xsl:stylesheet version="3.0"
    xmlns:xsl="http://www.w3.org/1999/XSL/Transform"
    xmlns:tst="urn:clayers:testing"
    exclude-result-prefixes="tst">

  <!-- Manual test -->
  <xsl:template match="tst:manual">
    <div class="card" id="{@id}">
      <h4>
        <span class="badge badge-yellow" style="font-size:0.65rem;">manual</span>
        <xsl:text> </xsl:text>
        <xsl:value-of select="tst:title"/>
      </h4>
      <div>
        <strong>Procedure:</strong>
        <xsl:apply-templates select="tst:procedure/node()"/>
      </div>
      <div>
        <strong>Expected:</strong>
        <xsl:apply-templates select="tst:expected/node()"/>
      </div>
      <xsl:if test="tst:role">
        <p><strong>Role:</strong> <xsl:value-of select="tst:role"/></p>
      </xsl:if>
    </div>
  </xsl:template>

  <!-- Test vectors -->
  <xsl:template match="tst:vectors">
    <div class="card" id="{@id}">
      <h4>
        <span class="badge badge-blue" style="font-size:0.65rem;">vectors</span>
        <xsl:text> </xsl:text>
        <xsl:value-of select="tst:title"/>
      </h4>
      <table>
        <thead><tr><th>Vector</th><th>Input</th><th>Output</th><th>Note</th></tr></thead>
        <tbody>
          <xsl:for-each select="tst:vector">
            <tr id="{@id}">
              <td><code><xsl:value-of select="@id"/></code></td>
              <td><code><xsl:value-of select="tst:input/@content"/></code></td>
              <td><code><xsl:value-of select="tst:output/@content"/></code></td>
              <td><xsl:value-of select="tst:note"/></td>
            </tr>
          </xsl:for-each>
        </tbody>
      </table>
    </div>
  </xsl:template>

  <!-- Property test -->
  <xsl:template match="tst:property">
    <div class="card" id="{@id}">
      <h4>
        <span class="badge badge-green" style="font-size:0.65rem;">property</span>
        <xsl:text> </xsl:text>
        <xsl:value-of select="tst:title"/>
      </h4>
      <div>
        <strong>Invariant:</strong>
        <xsl:apply-templates select="tst:invariant/node()"/>
      </div>
      <xsl:if test="tst:generator">
        <div>
          <strong>Generator:</strong>
          <xsl:apply-templates select="tst:generator/node()"/>
        </div>
      </xsl:if>
      <xsl:if test="tst:bounds">
        <p>
          <xsl:if test="tst:bounds/@sample-count">
            <strong>Samples:</strong> <xsl:value-of select="tst:bounds/@sample-count"/>
          </xsl:if>
          <xsl:if test="tst:bounds/@timeout">
            <xsl:text> </xsl:text><strong>Timeout:</strong> <xsl:value-of select="tst:bounds/@timeout"/>
          </xsl:if>
        </p>
      </xsl:if>
    </div>
  </xsl:template>

  <!-- Program test -->
  <xsl:template match="tst:program">
    <div class="card" id="{@id}">
      <h4>
        <span class="badge badge-blue" style="font-size:0.65rem;">program</span>
        <xsl:if test="@lang">
          <xsl:text> </xsl:text>
          <span class="badge badge-gray" style="font-size:0.65rem;">
            <xsl:value-of select="@lang"/>
          </span>
        </xsl:if>
        <xsl:text> </xsl:text>
        <xsl:value-of select="tst:title"/>
      </h4>
      <p><strong>Source:</strong> <code><xsl:value-of select="@source"/></code></p>
    </div>
  </xsl:template>

  <!-- Witness test -->
  <xsl:template match="tst:witness">
    <div class="card" id="{@id}">
      <h4>
        <xsl:choose>
          <xsl:when test="@type = 'command'">
            <span class="badge badge-blue" style="font-size:0.65rem;">command</span>
          </xsl:when>
          <xsl:when test="@type = 'script'">
            <span class="badge badge-green" style="font-size:0.65rem;">script</span>
          </xsl:when>
          <xsl:when test="@type = 'api'">
            <span class="badge badge-yellow" style="font-size:0.65rem;">api</span>
          </xsl:when>
          <xsl:otherwise>
            <span class="badge badge-gray" style="font-size:0.65rem;">
              <xsl:value-of select="@type"/>
            </span>
          </xsl:otherwise>
        </xsl:choose>
        <xsl:text> </xsl:text>
        <xsl:value-of select="tst:title"/>
      </h4>
      <xsl:if test="tst:command">
        <p><strong>Command:</strong> <code><xsl:value-of select="tst:command"/></code></p>
      </xsl:if>
      <xsl:if test="tst:endpoint">
        <p><strong>Endpoint:</strong> <code><xsl:value-of select="tst:method"/> <xsl:value-of select="tst:endpoint"/></code></p>
      </xsl:if>
      <xsl:if test="tst:status">
        <p><strong>Expected status:</strong> <xsl:value-of select="tst:status"/></p>
      </xsl:if>
      <xsl:if test="@source">
        <p><strong>Source:</strong> <code><xsl:value-of select="@source"/></code></p>
      </xsl:if>
    </div>
  </xsl:template>

  <!-- Suite -->
  <xsl:template match="tst:suite">
    <div class="card" id="{@id}">
      <h4>
        <span class="badge badge-gray" style="font-size:0.65rem;">suite</span>
        <xsl:text> </xsl:text>
        <xsl:value-of select="tst:title"/>
      </h4>
      <ul>
        <xsl:for-each select="tst:test">
          <li><a href="#{@ref}"><xsl:value-of select="@ref"/></a></li>
        </xsl:for-each>
      </ul>
    </div>
  </xsl:template>

</xsl:stylesheet>

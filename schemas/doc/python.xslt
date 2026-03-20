<?xml version="1.0" encoding="UTF-8"?>
<xsl:stylesheet version="3.0"
    xmlns:xsl="http://www.w3.org/1999/XSL/Transform"
    xmlns:py="urn:clayers:python"
    xmlns:pr="urn:clayers:prose"
    xmlns:art="urn:clayers:artifact"
    xmlns:llm="urn:clayers:llm"
    xmlns:cmb="urn:clayers:combined"
    xmlns:doc="urn:clayers:doc"
    exclude-result-prefixes="py pr art llm cmb doc">

  <!-- Key for detached children: elements with @of referencing a parent -->
  <xsl:key name="py-of" match="py:*[@of]" use="@of"/>

  <!-- ================================================================== -->
  <!-- Named template: render Python signature parameters                  -->
  <!-- ================================================================== -->

  <xsl:template name="py-signature-params">
    <xsl:param name="params"/>
    <xsl:variable name="last-positional-pos">
      <xsl:for-each select="$params[@positional = 'true']">
        <xsl:if test="position() = last()">
          <xsl:value-of select="count(preceding-sibling::py:param[parent::* = current()/parent::*
            and . &lt;&lt; current()/../py:param[. is current()]] | preceding-sibling::py:param) + 1"/>
        </xsl:if>
      </xsl:for-each>
    </xsl:variable>
    <xsl:variable name="has-variadic-args" select="$params[@variadic = 'args']"/>
    <xsl:variable name="first-keyword" select="$params[@keyword = 'true'][1]"/>
    <xsl:for-each select="$params">
      <xsl:variable name="pos" select="position()"/>
      <!-- Insert * before first keyword-only param if no variadic args present -->
      <xsl:if test="not($has-variadic-args) and $first-keyword and generate-id(.) = generate-id($first-keyword)">
        <xsl:text>*, </xsl:text>
      </xsl:if>
      <!-- Variadic prefix -->
      <xsl:choose>
        <xsl:when test="@variadic = 'args'">
          <xsl:text>*</xsl:text>
        </xsl:when>
        <xsl:when test="@variadic = 'kwargs'">
          <xsl:text>**</xsl:text>
        </xsl:when>
      </xsl:choose>
      <!-- Parameter name -->
      <xsl:value-of select="@name"/>
      <!-- Type annotation -->
      <xsl:if test="@type">
        <xsl:text>: </xsl:text>
        <xsl:value-of select="@type"/>
      </xsl:if>
      <!-- Default value -->
      <xsl:if test="@default">
        <xsl:text> = </xsl:text>
        <xsl:value-of select="@default"/>
      </xsl:if>
      <!-- Insert / after last positional-only param -->
      <xsl:if test="@positional = 'true' and not(following-sibling::py:param[@positional = 'true'])">
        <xsl:text>, /</xsl:text>
      </xsl:if>
      <!-- Comma separator -->
      <xsl:if test="position() != last()">
        <xsl:text>, </xsl:text>
      </xsl:if>
    </xsl:for-each>
  </xsl:template>

  <!-- ================================================================== -->
  <!-- py:module                                                           -->
  <!-- ================================================================== -->

  <xsl:template match="py:module">
    <section class="py-module" id="{@id}">
      <h3>
        <xsl:text>module </xsl:text>
        <xsl:value-of select="@name"/>
      </h3>
      <xsl:apply-templates select="py:doc"/>
      <xsl:apply-templates select="py:exception"/>
      <xsl:apply-templates select="py:class"/>
      <xsl:apply-templates select="py:function"/>
      <xsl:apply-templates select="py:constant"/>
      <!-- Detached children via @of -->
      <xsl:apply-templates select="key('py-of', @id)"/>
    </section>
  </xsl:template>

  <!-- ================================================================== -->
  <!-- py:class                                                            -->
  <!-- ================================================================== -->

  <xsl:template match="py:class">
    <div class="py-class" id="{@id}">
      <h4>
        <code>
          <xsl:text>class </xsl:text>
          <xsl:value-of select="@name"/>
          <xsl:choose>
            <xsl:when test="py:param">
              <xsl:text>(</xsl:text>
              <xsl:call-template name="py-signature-params">
                <xsl:with-param name="params" select="py:param"/>
              </xsl:call-template>
              <xsl:text>)</xsl:text>
            </xsl:when>
            <xsl:when test="@bases">
              <xsl:text>(</xsl:text>
              <xsl:value-of select="@bases"/>
              <xsl:text>)</xsl:text>
            </xsl:when>
          </xsl:choose>
        </code>
      </h4>

      <xsl:apply-templates select="py:doc"/>

      <!-- Properties table (merged: nested + of-attached) -->
      <xsl:variable name="nested-props" select="py:property"/>
      <xsl:variable name="attached-props" select="key('py-of', @id)[self::py:property]"/>
      <xsl:variable name="all-props" select="$nested-props | $attached-props"/>
      <xsl:if test="$all-props">
        <table>
          <thead>
            <tr>
              <th>Property</th>
              <th>Type</th>
              <th>Description</th>
            </tr>
          </thead>
          <tbody>
            <xsl:for-each select="$all-props">
              <tr id="{@id}">
                <td><code><xsl:value-of select="@name"/></code></td>
                <td>
                  <xsl:if test="@type"><code><xsl:value-of select="@type"/></code></xsl:if>
                </td>
                <td>
                  <xsl:if test="py:doc">
                    <xsl:apply-templates select="py:doc/node()"/>
                  </xsl:if>
                </td>
              </tr>
            </xsl:for-each>
          </tbody>
        </table>
      </xsl:if>

      <!-- Constants -->
      <xsl:apply-templates select="py:constant"/>
      <xsl:apply-templates select="key('py-of', @id)[self::py:constant]"/>

      <!-- Methods table (merged: nested + of-attached) -->
      <xsl:variable name="nested-methods" select="py:method"/>
      <xsl:variable name="attached-methods" select="key('py-of', @id)[self::py:method]"/>
      <xsl:variable name="all-methods" select="$nested-methods | $attached-methods"/>
      <xsl:if test="$all-methods">
        <table class="py-methods">
          <thead>
            <tr>
              <th>Method</th>
              <th>Returns</th>
              <th>Description</th>
            </tr>
          </thead>
          <tbody>
            <xsl:for-each select="$all-methods">
              <tr id="{@id}">
                <td>
                  <code>
                    <xsl:if test="@kind = 'staticmethod' or @kind = 'classmethod'">
                      <xsl:text>@</xsl:text>
                      <xsl:value-of select="@kind"/>
                      <xsl:text> </xsl:text>
                    </xsl:if>
                    <xsl:value-of select="@name"/>
                    <xsl:text>(</xsl:text>
                    <xsl:call-template name="py-signature-params">
                      <xsl:with-param name="params" select="py:param"/>
                    </xsl:call-template>
                    <xsl:text>)</xsl:text>
                  </code>
                </td>
                <td>
                  <xsl:if test="py:returns">
                    <code><xsl:value-of select="py:returns/@type"/></code>
                  </xsl:if>
                </td>
                <td>
                  <xsl:if test="py:doc">
                    <xsl:apply-templates select="py:doc/node()"/>
                  </xsl:if>
                </td>
              </tr>
            </xsl:for-each>
          </tbody>
        </table>
      </xsl:if>
    </div>
  </xsl:template>

  <!-- ================================================================== -->
  <!-- py:method                                                           -->
  <!-- ================================================================== -->

  <xsl:template match="py:method">
    <div class="py-method" id="{@id}">
      <xsl:if test="@kind = 'staticmethod' or @kind = 'classmethod'">
        <div class="py-decorator">
          <code>
            <xsl:text>@</xsl:text>
            <xsl:value-of select="@kind"/>
          </code>
        </div>
      </xsl:if>
      <h5>
        <code>
          <xsl:value-of select="@name"/>
          <xsl:text>(</xsl:text>
          <xsl:call-template name="py-signature-params">
            <xsl:with-param name="params" select="py:param"/>
          </xsl:call-template>
          <xsl:text>)</xsl:text>
          <xsl:if test="py:returns">
            <xsl:text> -> </xsl:text>
            <xsl:value-of select="py:returns/@type"/>
          </xsl:if>
        </code>
      </h5>
      <xsl:apply-templates select="py:doc"/>
      <xsl:if test="py:raises">
        <div class="py-raises">
          <strong>Raises:</strong>
          <ul>
            <xsl:for-each select="py:raises">
              <li>
                <code><xsl:value-of select="@type"/></code>
                <xsl:if test="normalize-space(.)">
                  <xsl:text> &#x2013; </xsl:text>
                  <xsl:value-of select="normalize-space(.)"/>
                </xsl:if>
              </li>
            </xsl:for-each>
          </ul>
        </div>
      </xsl:if>
    </div>
  </xsl:template>

  <!-- ================================================================== -->
  <!-- py:function                                                         -->
  <!-- ================================================================== -->

  <xsl:template match="py:function">
    <div class="py-function" id="{@id}">
      <h4>
        <code>
          <xsl:value-of select="@name"/>
          <xsl:text>(</xsl:text>
          <xsl:call-template name="py-signature-params">
            <xsl:with-param name="params" select="py:param"/>
          </xsl:call-template>
          <xsl:text>)</xsl:text>
          <xsl:if test="py:returns">
            <xsl:text> -> </xsl:text>
            <xsl:value-of select="py:returns/@type"/>
          </xsl:if>
        </code>
      </h4>
      <xsl:apply-templates select="py:doc"/>
      <xsl:if test="py:raises">
        <div class="py-raises">
          <strong>Raises:</strong>
          <ul>
            <xsl:for-each select="py:raises">
              <li>
                <code><xsl:value-of select="@type"/></code>
                <xsl:if test="normalize-space(.)">
                  <xsl:text> &#x2013; </xsl:text>
                  <xsl:value-of select="normalize-space(.)"/>
                </xsl:if>
              </li>
            </xsl:for-each>
          </ul>
        </div>
      </xsl:if>
    </div>
  </xsl:template>

  <!-- ================================================================== -->
  <!-- py:exception                                                        -->
  <!-- ================================================================== -->

  <xsl:template match="py:exception">
    <div class="py-exception" id="{@id}">
      <h4>
        <code>
          <xsl:text>exception </xsl:text>
          <xsl:value-of select="@name"/>
          <xsl:if test="@bases">
            <xsl:text>(</xsl:text>
            <xsl:value-of select="@bases"/>
            <xsl:text>)</xsl:text>
          </xsl:if>
        </code>
      </h4>
      <xsl:apply-templates select="py:doc"/>
    </div>
  </xsl:template>

  <!-- ================================================================== -->
  <!-- py:property (standalone, outside class table context)               -->
  <!-- ================================================================== -->

  <xsl:template match="py:property">
    <div class="py-property" id="{@id}">
      <h5>
        <code>
          <xsl:value-of select="@name"/>
          <xsl:if test="@type">
            <xsl:text>: </xsl:text>
            <xsl:value-of select="@type"/>
          </xsl:if>
        </code>
      </h5>
      <xsl:apply-templates select="py:doc"/>
    </div>
  </xsl:template>

  <!-- ================================================================== -->
  <!-- py:constant                                                         -->
  <!-- ================================================================== -->

  <xsl:template match="py:constant">
    <div class="py-constant" id="{@id}">
      <h5>
        <code>
          <xsl:value-of select="@name"/>
          <xsl:if test="@type">
            <xsl:text>: </xsl:text>
            <xsl:value-of select="@type"/>
          </xsl:if>
        </code>
      </h5>
      <xsl:apply-templates select="py:doc"/>
    </div>
  </xsl:template>

  <!-- ================================================================== -->
  <!-- py:doc                                                              -->
  <!-- ================================================================== -->

  <xsl:template match="py:doc">
    <div class="py-doc">
      <xsl:apply-templates/>
    </div>
  </xsl:template>

</xsl:stylesheet>

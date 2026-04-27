<?xml version="1.0" encoding="UTF-8"?>
<xsl:stylesheet version="3.0"
    xmlns:xsl="http://www.w3.org/1999/XSL/Transform"
    xmlns:pr="urn:clayers:prose"
    xmlns:org="urn:clayers:organization"
    xmlns:rel="urn:clayers:relation"
    xmlns:llm="urn:clayers:llm"
    xmlns:art="urn:clayers:artifact"
    xmlns:cnt="urn:clayers:content"
    xmlns:doc="urn:clayers:doc"
    xmlns:cmb="urn:clayers:combined"
    exclude-result-prefixes="pr org rel llm art cnt doc cmb">

  <!-- Section: depth-based headings -->
  <xsl:template match="pr:section">
    <xsl:variable name="depth" select="count(ancestor::pr:section) + 1"/>
    <xsl:variable name="level" select="if ($depth le 6) then $depth else 6"/>
    <section>
      <xsl:element name="h{$level}">
        <xsl:attribute name="id" select="@id"/>
        <xsl:value-of select="pr:title"/>
        <a class="heading-anchor" href="#{@id}">#</a>
        <!-- Organization badge if this section is typed -->
        <xsl:variable name="sid" select="@id"/>
        <!-- Drift indicator on heading, links to first drifted mapping -->
        <xsl:variable name="spec-drifts" select="ancestor::cmb:spec//doc:drift[@node = $sid and @status = 'spec-drifted']"/>
        <xsl:variable name="art-drifts" select="ancestor::cmb:spec//doc:drift[@node = $sid and @status = 'artifact-drifted']"/>
        <xsl:if test="$spec-drifts or $art-drifts">
          <xsl:variable name="first-drift" select="($spec-drifts | $art-drifts)[1]"/>
          <xsl:variable name="drift-class">
            <xsl:choose>
              <xsl:when test="$spec-drifts and $art-drifts">drift-both</xsl:when>
              <xsl:when test="$spec-drifts">drift-spec</xsl:when>
              <xsl:otherwise>drift-artifact</xsl:otherwise>
            </xsl:choose>
          </xsl:variable>
          <xsl:variable name="drift-title">
            <xsl:if test="$spec-drifts"><xsl:value-of select="count($spec-drifts)"/> spec drifted</xsl:if>
            <xsl:if test="$spec-drifts and $art-drifts">, </xsl:if>
            <xsl:if test="$art-drifts"><xsl:value-of select="count($art-drifts)"/> artifact drifted</xsl:if>
          </xsl:variable>
          <a href="#{$first-drift/@mapping}" class="drift-dot {$drift-class}" title="{$drift-title}">&#x25CF;</a>
        </xsl:if>
        <xsl:apply-templates select="ancestor::cmb:spec//org:concept[@ref = $sid]" mode="badge"/>
        <xsl:apply-templates select="ancestor::cmb:spec//org:task[@ref = $sid]" mode="badge"/>
        <xsl:apply-templates select="ancestor::cmb:spec//org:reference[@ref = $sid]" mode="badge"/>
      </xsl:element>

      <xsl:if test="pr:shortdesc">
        <p class="shortdesc"><xsl:apply-templates select="pr:shortdesc/node()"/></p>
      </xsl:if>

      <!-- Relations for this section -->
      <xsl:variable name="sid" select="@id"/>
      <xsl:variable name="rels-from" select="ancestor::cmb:spec//rel:relation[@from = $sid]"/>
      <xsl:variable name="rels-to" select="ancestor::cmb:spec//rel:relation[@to = $sid]"/>
      <!-- Task actor -->
      <xsl:variable name="task-actor" select="ancestor::cmb:spec//org:task[@ref = $sid]/org:actor"/>
      <xsl:if test="$task-actor">
        <div class="task-actor">
          <xsl:text>Performed by: </xsl:text>
          <strong><xsl:value-of select="$task-actor"/></strong>
        </div>
      </xsl:if>

      <xsl:if test="$rels-from or $rels-to">
        <xsl:call-template name="render-relations">
          <xsl:with-param name="rels-from" select="$rels-from"/>
          <xsl:with-param name="rels-to" select="$rels-to"/>
        </xsl:call-template>
      </xsl:if>

      <!-- LLM machine description -->
      <xsl:variable name="llm-desc" select="ancestor::cmb:spec//llm:node[@ref = $sid]"/>
      <xsl:if test="$llm-desc">
        <xsl:apply-templates select="$llm-desc" mode="inline"/>
      </xsl:if>

      <xsl:apply-templates select="*[not(self::pr:title or self::pr:shortdesc)]"/>

      <!-- Artifact mappings for this node -->
      <xsl:variable name="mappings" select="ancestor::cmb:spec//art:mapping[art:spec-ref/@node = $sid]"/>
      <xsl:if test="$mappings">
        <div class="node-artifacts">
          <div class="node-artifacts-title">Artifacts</div>
          <xsl:for-each select="$mappings">
            <div class="node-artifact-entry">
              <code><xsl:value-of select="art:artifact/@path"/><xsl:for-each select="art:artifact/art:range[@start-line and @end-line]"><xsl:text>:</xsl:text><xsl:value-of select="@start-line"/>-<xsl:value-of select="@end-line"/></xsl:for-each></code>
              <xsl:text> </xsl:text>
              <xsl:variable name="cov" select="art:coverage"/>
              <xsl:choose>
                <xsl:when test="$cov = 'full'"><span class="badge badge-green">full</span></xsl:when>
                <xsl:when test="$cov = 'partial'"><span class="badge badge-yellow">partial</span></xsl:when>
                <xsl:when test="$cov = 'none'"><span class="badge badge-red">none</span></xsl:when>
              </xsl:choose>
              <xsl:if test="art:note">
                <div style="font-size:0.8em;color:hsl(var(--muted-foreground));margin-top:0.15rem;"><xsl:value-of select="art:note"/></div>
              </xsl:if>
              <!-- Drift badge + code fragment from doc:report -->
              <xsl:variable name="mid" select="@id"/>
              <xsl:variable name="drift" select="ancestor::cmb:spec//doc:drift[@mapping = $mid]"/>
              <xsl:if test="$drift/@status = 'spec-drifted' or $drift/@status = 'artifact-drifted'">
                <span class="badge {if ($drift/@status = 'spec-drifted') then 'badge-drift-spec' else 'badge-drift-artifact'}" style="margin-top:0.25rem;display:inline-block;">
                  <xsl:value-of select="if ($drift/@status = 'spec-drifted') then 'spec drifted' else 'artifact drifted'"/>
                </span>
              </xsl:if>
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
          </xsl:for-each>
        </div>
      </xsl:if>
    </section>
  </xsl:template>

  <!-- Paragraph -->
  <xsl:template match="pr:p">
    <p><xsl:apply-templates/></p>
  </xsl:template>

  <!-- Unordered list -->
  <xsl:template match="pr:ul">
    <ul><xsl:apply-templates/></ul>
  </xsl:template>

  <!-- Ordered list -->
  <xsl:template match="pr:ol">
    <ol><xsl:apply-templates/></ol>
  </xsl:template>

  <!-- Definition list -->
  <xsl:template match="pr:dl">
    <dl><xsl:apply-templates/></dl>
  </xsl:template>

  <xsl:template match="pr:dlentry">
    <xsl:apply-templates/>
  </xsl:template>

  <xsl:template match="pr:dt">
    <dt><xsl:apply-templates/></dt>
  </xsl:template>

  <xsl:template match="pr:dd">
    <dd><xsl:apply-templates/></dd>
  </xsl:template>

  <!-- List item -->
  <xsl:template match="pr:li">
    <li><xsl:apply-templates/></li>
  </xsl:template>

  <!-- Steps (ordered) -->
  <xsl:template match="pr:steps">
    <ol class="steps"><xsl:apply-templates/></ol>
  </xsl:template>

  <xsl:template match="pr:steps-unordered">
    <ul class="steps steps-unordered"><xsl:apply-templates/></ul>
  </xsl:template>

  <!-- Step -->
  <xsl:template match="pr:step">
    <li>
      <xsl:if test="@id"><xsl:attribute name="id" select="@id"/></xsl:if>
      <xsl:apply-templates/>
    </li>
  </xsl:template>

  <xsl:template match="pr:stepsection">
    <li class="stepsection"><xsl:apply-templates/></li>
  </xsl:template>

  <xsl:template match="pr:cmd">
    <div class="task-cmd"><xsl:apply-templates/></div>
  </xsl:template>

  <xsl:template match="pr:info | pr:prereq | pr:context | pr:result | pr:postreq | pr:stepresult">
    <div class="task-detail task-{local-name()}">
      <div class="task-detail-label"><xsl:value-of select="replace(local-name(), '-', ' ')"/></div>
      <xsl:apply-templates/>
    </div>
  </xsl:template>

  <xsl:template match="pr:substeps">
    <ol class="steps substeps"><xsl:apply-templates/></ol>
  </xsl:template>

  <xsl:template match="pr:choices">
    <ul class="choices"><xsl:apply-templates/></ul>
  </xsl:template>

  <xsl:template match="pr:choice">
    <li><xsl:apply-templates/></li>
  </xsl:template>

  <xsl:template match="pr:choicetable">
    <table class="choice-table">
      <xsl:choose>
        <xsl:when test="pr:chhead">
          <thead><xsl:apply-templates select="pr:chhead"/></thead>
        </xsl:when>
        <xsl:otherwise>
          <thead>
            <tr><th>Option</th><th>Description</th></tr>
          </thead>
        </xsl:otherwise>
      </xsl:choose>
      <tbody><xsl:apply-templates select="pr:chrow"/></tbody>
    </table>
  </xsl:template>

  <xsl:template match="pr:chhead">
    <tr><xsl:apply-templates/></tr>
  </xsl:template>

  <xsl:template match="pr:choptionhd | pr:chdeschd">
    <th><xsl:apply-templates/></th>
  </xsl:template>

  <xsl:template match="pr:chrow">
    <tr><xsl:apply-templates/></tr>
  </xsl:template>

  <xsl:template match="pr:choption | pr:chdesc">
    <td><xsl:apply-templates/></td>
  </xsl:template>

  <xsl:template match="pr:stepxmp">
    <xsl:call-template name="render-example">
      <xsl:with-param name="class" select="'step-example'"/>
    </xsl:call-template>
  </xsl:template>


  <!-- Note callout -->
  <xsl:template match="pr:note">
    <xsl:variable name="type" select="(@type, 'info')[1]"/>
    <div class="note note-{$type}">
      <div class="note-label"><xsl:value-of select="$type"/></div>
      <xsl:apply-templates/>
    </div>
  </xsl:template>

  <!-- Code block -->
  <xsl:template match="pr:codeblock">
    <pre><code><xsl:if test="@language">
      <xsl:attribute name="class">language-<xsl:value-of select="@language"/></xsl:attribute>
    </xsl:if><xsl:value-of select="."/></code></pre>
  </xsl:template>

  <xsl:template match="pr:synopsis">
    <pre class="synopsis {if (@kind) then concat('synopsis-', @kind) else ''}"><code><xsl:if test="@language">
      <xsl:attribute name="class">language-<xsl:value-of select="@language"/></xsl:attribute>
    </xsl:if><xsl:value-of select="."/></code></pre>
  </xsl:template>

  <!-- Examples, figures, and media -->
  <xsl:template match="pr:example">
    <xsl:call-template name="render-example">
      <xsl:with-param name="class" select="'example'"/>
    </xsl:call-template>
  </xsl:template>

  <xsl:template name="render-example">
    <xsl:param name="class"/>
    <div class="{$class}">
      <xsl:if test="@id"><xsl:attribute name="id" select="@id"/></xsl:if>
      <xsl:if test="pr:title">
        <div class="example-title"><xsl:value-of select="pr:title"/></div>
      </xsl:if>
      <xsl:apply-templates select="node()[not(self::pr:title)]"/>
    </div>
  </xsl:template>

  <xsl:template match="pr:figure">
    <figure>
      <xsl:if test="@id"><xsl:attribute name="id" select="@id"/></xsl:if>
      <xsl:apply-templates select="node()[not(self::pr:title or self::pr:caption)]"/>
      <xsl:if test="pr:title or pr:caption">
        <figcaption>
          <xsl:if test="pr:title"><strong><xsl:value-of select="pr:title"/></strong></xsl:if>
          <xsl:if test="pr:title and pr:caption"><xsl:text>: </xsl:text></xsl:if>
          <xsl:apply-templates select="pr:caption/node()"/>
        </figcaption>
      </xsl:if>
    </figure>
  </xsl:template>

  <xsl:template match="pr:media">
    <xsl:variable name="content-id" select="@content"/>
    <xsl:variable name="content" select="ancestor::cmb:spec//cnt:content[@id = $content-id][1]"/>
    <xsl:variable name="href" select="if (@href) then string(@href) else string($content/@url)"/>
    <xsl:variable name="format" select="if (@format) then string(@format) else replace(string($content/@media-type), '^.*/', '')"/>
    <xsl:choose>
      <xsl:when test="matches($href, '\.(png|jpg|jpeg|gif|webp|svg)$', 'i') or lower-case($format) = ('png','jpg','jpeg','gif','webp','svg')">
        <img src="{$href}" alt="{@alt}">
          <xsl:if test="@width"><xsl:attribute name="width" select="@width"/></xsl:if>
          <xsl:if test="@height"><xsl:attribute name="height" select="@height"/></xsl:if>
          <xsl:if test="@scale"><xsl:attribute name="style" select="concat('max-width:', @scale, '%;')"/></xsl:if>
        </img>
      </xsl:when>
      <xsl:otherwise>
        <a class="media-link" href="{$href}"><xsl:value-of select="@alt"/></a>
      </xsl:otherwise>
    </xsl:choose>
  </xsl:template>

  <!-- Table -->
  <xsl:template match="pr:table">
    <xsl:if test="pr:title">
      <div class="table-title"><xsl:value-of select="pr:title"/></div>
    </xsl:if>
    <table><xsl:apply-templates select="node()[not(self::pr:title)]"/></table>
  </xsl:template>

  <xsl:template match="pr:thead">
    <thead><xsl:apply-templates/></thead>
  </xsl:template>

  <xsl:template match="pr:tbody">
    <tbody><xsl:apply-templates/></tbody>
  </xsl:template>

  <xsl:template match="pr:tr">
    <tr><xsl:apply-templates/></tr>
  </xsl:template>

  <xsl:template match="pr:th">
    <th><xsl:apply-templates/></th>
  </xsl:template>

  <xsl:template match="pr:td">
    <td><xsl:apply-templates/></td>
  </xsl:template>

  <!-- Reference/property tables -->
  <xsl:template match="pr:properties">
    <xsl:if test="pr:title">
      <div class="table-title"><xsl:value-of select="pr:title"/></div>
    </xsl:if>
    <table class="properties">
      <thead>
        <tr>
          <th><xsl:value-of select="(pr:prophead/pr:proptypehd, 'Type')[1]"/></th>
          <th><xsl:value-of select="(pr:prophead/pr:propvaluehd, 'Value')[1]"/></th>
          <th><xsl:value-of select="(pr:prophead/pr:propdeschd, 'Description')[1]"/></th>
        </tr>
      </thead>
      <tbody><xsl:apply-templates select="pr:property"/></tbody>
    </table>
  </xsl:template>

  <xsl:template match="pr:property">
    <tr>
      <td><xsl:apply-templates select="pr:proptype/node()"/></td>
      <td><xsl:apply-templates select="pr:propvalue/node()"/></td>
      <td><xsl:apply-templates select="pr:propdesc/node()"/></td>
    </tr>
  </xsl:template>

  <!-- Troubleshooting -->
  <xsl:template match="pr:troubleshooting | pr:steptroubleshooting">
    <section class="troubleshooting {if (self::pr:steptroubleshooting) then 'step-troubleshooting' else ''}">
      <xsl:if test="@id"><xsl:attribute name="id" select="@id"/></xsl:if>
      <xsl:if test="pr:title"><h4><xsl:value-of select="pr:title"/></h4></xsl:if>
      <xsl:apply-templates select="pr:condition | pr:cause | pr:remedy | pr:responsible-party"/>
    </section>
  </xsl:template>

  <xsl:template match="pr:condition | pr:cause | pr:remedy | pr:responsible-party">
    <div class="trouble-part trouble-{local-name()}">
      <div class="trouble-label"><xsl:value-of select="replace(local-name(), '-', ' ')"/></div>
      <xsl:apply-templates/>
    </div>
  </xsl:template>

  <!-- Publishing and navigation blocks -->
  <xsl:template match="pr:blockquote">
    <blockquote>
      <xsl:apply-templates select="node()[not(self::pr:attribution)]"/>
      <xsl:if test="pr:attribution">
        <footer><xsl:apply-templates select="pr:attribution/node()"/></footer>
      </xsl:if>
    </blockquote>
  </xsl:template>

  <xsl:template match="pr:sidebar">
    <aside class="sidebar-block">
      <xsl:if test="@id"><xsl:attribute name="id" select="@id"/></xsl:if>
      <xsl:if test="pr:title"><h4><xsl:value-of select="pr:title"/></h4></xsl:if>
      <xsl:apply-templates select="node()[not(self::pr:title)]"/>
    </aside>
  </xsl:template>

  <xsl:template match="pr:related-links">
    <nav class="related-links" aria-label="Related links">
      <div class="related-links-title">Related links</div>
      <ul><xsl:apply-templates/></ul>
    </nav>
  </xsl:template>

  <xsl:template match="pr:link">
    <li>
      <a href="#{@ref}">
        <xsl:choose>
          <xsl:when test="node()"><xsl:apply-templates/></xsl:when>
          <xsl:otherwise>
            <xsl:variable name="target" select="@ref"/>
            <xsl:variable name="title" select="ancestor::cmb:spec//pr:section[@id = $target]/pr:title"/>
            <xsl:value-of select="($title, @ref)[1]"/>
          </xsl:otherwise>
        </xsl:choose>
      </a>
      <xsl:if test="@role"><span class="link-role"><xsl:value-of select="@role"/></span></xsl:if>
    </li>
  </xsl:template>

  <!-- Inline elements -->
  <xsl:template match="pr:b">
    <strong><xsl:apply-templates/></strong>
  </xsl:template>

  <xsl:template match="pr:i">
    <em><xsl:apply-templates/></em>
  </xsl:template>

  <xsl:template match="pr:u">
    <u><xsl:apply-templates/></u>
  </xsl:template>

  <xsl:template match="pr:q">
    <q><xsl:apply-templates/></q>
  </xsl:template>

  <xsl:template match="pr:sup">
    <sup><xsl:apply-templates/></sup>
  </xsl:template>

  <xsl:template match="pr:sub">
    <sub><xsl:apply-templates/></sub>
  </xsl:template>

  <xsl:template match="pr:code">
    <code><xsl:apply-templates/></code>
  </xsl:template>

  <xsl:template match="pr:var">
    <var><xsl:apply-templates/></var>
  </xsl:template>

  <xsl:template match="pr:kbd">
    <kbd><xsl:apply-templates/></kbd>
  </xsl:template>

  <xsl:template match="pr:filepath | pr:option | pr:cmdname | pr:parmname | pr:apiname | pr:msgph">
    <code class="{local-name()}"><xsl:apply-templates/></code>
  </xsl:template>

  <!-- Cross-reference -->
  <xsl:template match="pr:xref">
    <a href="#{@ref}">
      <xsl:choose>
        <xsl:when test="node()"><xsl:apply-templates/></xsl:when>
        <xsl:otherwise>
          <!-- Try to find the title of the referenced section -->
          <xsl:variable name="target" select="@ref"/>
          <xsl:variable name="title" select="ancestor::cmb:spec//pr:section[@id = $target]/pr:title"/>
          <xsl:choose>
            <xsl:when test="$title"><xsl:value-of select="$title"/></xsl:when>
            <xsl:otherwise><xsl:value-of select="@ref"/></xsl:otherwise>
          </xsl:choose>
        </xsl:otherwise>
      </xsl:choose>
    </a>
  </xsl:template>

</xsl:stylesheet>

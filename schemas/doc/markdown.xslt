<?xml version="1.0" encoding="UTF-8"?>
<!--
  Markdown-to-HTML renderer for XSLT 3.0 (Saxon).

  Inspired by md2doc (https://github.com/msmid/markdown2docbook)
  by Martin Smid, MIT License, Copyright (c) 2014.

  This is a clean-room reimplementation for direct HTML output,
  focused on the subset of GitHub-flavored Markdown used in
  clayers skill templates and agent guidance.

  Supported syntax:
    - YAML frontmatter (stripped)
    - ATX headings (# through ######)
    - Paragraphs (blank-line separated)
    - Fenced code blocks (``` with optional language)
    - Inline code (`code`)
    - Bold (**text**) and italic (*text*)
    - Unordered lists (- item)
    - Ordered lists (1. item)
    - Tables (| col | col |)
    - Links ([text](url))
    - Horizontal rules (three or more dashes)
    - Blockquotes (> text)
-->
<xsl:stylesheet version="3.0"
    xmlns:xsl="http://www.w3.org/1999/XSL/Transform"
    xmlns:xs="http://www.w3.org/2001/XMLSchema"
    xmlns:md="urn:clayers:markdown"
    exclude-result-prefixes="xs md">

  <!--
    Main entry point: convert a markdown string to HTML elements.
    Call as: <xsl:sequence select="md:to-html($text)"/>
  -->
  <xsl:function name="md:to-html" as="item()*">
    <xsl:param name="input" as="xs:string"/>

    <!-- Strip YAML frontmatter if present -->
    <xsl:variable name="text" select="md:strip-frontmatter($input)"/>

    <!-- Normalize line endings -->
    <xsl:variable name="normalized" select="replace(replace($text, '\r\n', '&#10;'), '\r', '&#10;')"/>

    <!-- Split into lines and process blocks -->
    <xsl:variable name="lines" select="tokenize($normalized, '&#10;')"/>

    <xsl:sequence select="md:process-blocks($lines, 1, count($lines))"/>
  </xsl:function>

  <!-- Strip YAML frontmatter from the beginning -->
  <xsl:function name="md:strip-frontmatter" as="xs:string">
    <xsl:param name="text" as="xs:string"/>
    <xsl:choose>
      <xsl:when test="matches($text, '^\s*---\s*\n')">
        <xsl:variable name="after-first" select="replace($text, '^\s*---[^\n]*\n', '')"/>
        <xsl:choose>
          <xsl:when test="contains($after-first, '&#10;---')">
            <xsl:sequence select="substring-after(substring-after($after-first, '&#10;---'), '&#10;')"/>
          </xsl:when>
          <xsl:otherwise>
            <xsl:sequence select="$text"/>
          </xsl:otherwise>
        </xsl:choose>
      </xsl:when>
      <xsl:otherwise>
        <xsl:sequence select="$text"/>
      </xsl:otherwise>
    </xsl:choose>
  </xsl:function>

  <!--
    Process a range of lines into HTML block elements.
    This is the block-level parser: it identifies headings, code blocks,
    lists, tables, blockquotes, horizontal rules, and paragraphs.
  -->
  <xsl:function name="md:process-blocks" as="item()*">
    <xsl:param name="lines" as="xs:string*"/>
    <xsl:param name="start" as="xs:integer"/>
    <xsl:param name="end" as="xs:integer"/>

    <xsl:if test="$start le $end">
      <xsl:variable name="line" select="$lines[$start]"/>

      <xsl:choose>
        <!-- Blank line: skip -->
        <xsl:when test="matches($line, '^\s*$')">
          <xsl:sequence select="md:process-blocks($lines, $start + 1, $end)"/>
        </xsl:when>

        <!-- Fenced code block -->
        <xsl:when test="matches($line, '^\s*```')">
          <xsl:variable name="lang" select="replace($line, '^\s*```\s*', '')"/>
          <xsl:variable name="close-idx" select="md:find-closing-fence($lines, $start + 1, $end)"/>
          <xsl:variable name="code-lines" select="
            for $i in ($start + 1) to ($close-idx - 1) return $lines[$i]
          "/>
          <pre><code>
            <xsl:if test="$lang != ''">
              <xsl:attribute name="class" select="concat('language-', $lang)"/>
            </xsl:if>
            <xsl:value-of select="string-join($code-lines, '&#10;')"/>
          </code></pre>
          <xsl:sequence select="md:process-blocks($lines, $close-idx + 1, $end)"/>
        </xsl:when>

        <!-- ATX heading -->
        <xsl:when test="matches($line, '^\s*#{1,6}\s')">
          <xsl:variable name="level" select="string-length(replace($line, '^\s*(#+).*', '$1'))"/>
          <xsl:variable name="text" select="replace($line, '^\s*#+\s*', '')"/>
          <xsl:element name="h{$level}">
            <xsl:sequence select="md:inline($text)"/>
          </xsl:element>
          <xsl:sequence select="md:process-blocks($lines, $start + 1, $end)"/>
        </xsl:when>

        <!-- Horizontal rule -->
        <xsl:when test="matches($line, '^\s*[-*_]{3,}\s*$')">
          <hr/>
          <xsl:sequence select="md:process-blocks($lines, $start + 1, $end)"/>
        </xsl:when>

        <!-- Table (starts with |) -->
        <xsl:when test="matches($line, '^\s*\|')">
          <xsl:variable name="table-end" select="md:find-block-end($lines, $start, $end, '^\s*\|')"/>
          <xsl:sequence select="md:render-table($lines, $start, $table-end)"/>
          <xsl:sequence select="md:process-blocks($lines, $table-end + 1, $end)"/>
        </xsl:when>

        <!-- Unordered list -->
        <xsl:when test="matches($line, '^\s*[-*+]\s')">
          <xsl:variable name="list-end" select="md:find-list-end($lines, $start, $end, '^\s*[-*+]\s|^\s{2,}\S')"/>
          <ul>
            <xsl:sequence select="md:render-list-items($lines, $start, $list-end, '^\s*[-*+]\s')"/>
          </ul>
          <xsl:sequence select="md:process-blocks($lines, $list-end + 1, $end)"/>
        </xsl:when>

        <!-- Ordered list -->
        <xsl:when test="matches($line, '^\s*\d+\.\s')">
          <xsl:variable name="list-end" select="md:find-list-end($lines, $start, $end, '^\s*\d+\.\s|^\s{2,}\S')"/>
          <ol>
            <xsl:sequence select="md:render-list-items($lines, $start, $list-end, '^\s*\d+\.\s')"/>
          </ol>
          <xsl:sequence select="md:process-blocks($lines, $list-end + 1, $end)"/>
        </xsl:when>

        <!-- Blockquote -->
        <xsl:when test="matches($line, '^\s*>\s?')">
          <xsl:variable name="bq-end" select="md:find-block-end($lines, $start, $end, '^\s*>')"/>
          <blockquote>
            <xsl:variable name="bq-lines" select="
              for $i in $start to $bq-end return replace($lines[$i], '^\s*>\s?', '')
            "/>
            <xsl:sequence select="md:process-blocks($bq-lines, 1, count($bq-lines))"/>
          </blockquote>
          <xsl:sequence select="md:process-blocks($lines, $bq-end + 1, $end)"/>
        </xsl:when>

        <!-- Paragraph (default) -->
        <xsl:otherwise>
          <xsl:variable name="para-end" select="md:find-para-end($lines, $start, $end)"/>
          <xsl:variable name="para-text" select="string-join(
            for $i in $start to $para-end return $lines[$i], '&#10;'
          )"/>
          <p><xsl:sequence select="md:inline($para-text)"/></p>
          <xsl:sequence select="md:process-blocks($lines, $para-end + 1, $end)"/>
        </xsl:otherwise>
      </xsl:choose>
    </xsl:if>
  </xsl:function>

  <!-- Find closing ``` for a fenced code block -->
  <xsl:function name="md:find-closing-fence" as="xs:integer">
    <xsl:param name="lines" as="xs:string*"/>
    <xsl:param name="start" as="xs:integer"/>
    <xsl:param name="end" as="xs:integer"/>
    <xsl:choose>
      <xsl:when test="$start gt $end">
        <xsl:sequence select="$end + 1"/>
      </xsl:when>
      <xsl:when test="matches($lines[$start], '^\s*```\s*$')">
        <xsl:sequence select="$start"/>
      </xsl:when>
      <xsl:otherwise>
        <xsl:sequence select="md:find-closing-fence($lines, $start + 1, $end)"/>
      </xsl:otherwise>
    </xsl:choose>
  </xsl:function>

  <!-- Find end of a contiguous block matching a pattern -->
  <xsl:function name="md:find-block-end" as="xs:integer">
    <xsl:param name="lines" as="xs:string*"/>
    <xsl:param name="start" as="xs:integer"/>
    <xsl:param name="end" as="xs:integer"/>
    <xsl:param name="pattern" as="xs:string"/>
    <xsl:choose>
      <xsl:when test="$start gt $end">
        <xsl:sequence select="$start - 1"/>
      </xsl:when>
      <xsl:when test="not(matches($lines[$start], $pattern))">
        <xsl:sequence select="$start - 1"/>
      </xsl:when>
      <xsl:otherwise>
        <xsl:sequence select="md:find-block-end($lines, $start + 1, $end, $pattern)"/>
      </xsl:otherwise>
    </xsl:choose>
  </xsl:function>

  <!-- Find end of a list (items + continuation lines) -->
  <xsl:function name="md:find-list-end" as="xs:integer">
    <xsl:param name="lines" as="xs:string*"/>
    <xsl:param name="start" as="xs:integer"/>
    <xsl:param name="end" as="xs:integer"/>
    <xsl:param name="pattern" as="xs:string"/>
    <xsl:choose>
      <xsl:when test="$start gt $end">
        <xsl:sequence select="$start - 1"/>
      </xsl:when>
      <xsl:when test="matches($lines[$start], '^\s*$')">
        <!-- Blank line: check if next line continues the list -->
        <xsl:choose>
          <xsl:when test="$start + 1 le $end and matches($lines[$start + 1], $pattern)">
            <xsl:sequence select="md:find-list-end($lines, $start + 1, $end, $pattern)"/>
          </xsl:when>
          <xsl:otherwise>
            <xsl:sequence select="$start - 1"/>
          </xsl:otherwise>
        </xsl:choose>
      </xsl:when>
      <xsl:when test="not(matches($lines[$start], $pattern))">
        <xsl:sequence select="$start - 1"/>
      </xsl:when>
      <xsl:otherwise>
        <xsl:sequence select="md:find-list-end($lines, $start + 1, $end, $pattern)"/>
      </xsl:otherwise>
    </xsl:choose>
  </xsl:function>

  <!-- Find end of a paragraph (next blank line or block element) -->
  <xsl:function name="md:find-para-end" as="xs:integer">
    <xsl:param name="lines" as="xs:string*"/>
    <xsl:param name="start" as="xs:integer"/>
    <xsl:param name="end" as="xs:integer"/>
    <xsl:choose>
      <xsl:when test="$start gt $end">
        <xsl:sequence select="$end"/>
      </xsl:when>
      <xsl:when test="matches($lines[$start], '^\s*$')">
        <xsl:sequence select="$start - 1"/>
      </xsl:when>
      <xsl:when test="$start gt $start and (
        matches($lines[$start], '^\s*#{1,6}\s') or
        matches($lines[$start], '^\s*```') or
        matches($lines[$start], '^\s*[-*_]{3,}\s*$') or
        matches($lines[$start], '^\s*\|') or
        matches($lines[$start], '^\s*[-*+]\s') or
        matches($lines[$start], '^\s*\d+\.\s') or
        matches($lines[$start], '^\s*>\s?')
      )">
        <xsl:sequence select="$start - 1"/>
      </xsl:when>
      <xsl:otherwise>
        <xsl:sequence select="md:find-para-end($lines, $start + 1, $end)"/>
      </xsl:otherwise>
    </xsl:choose>
  </xsl:function>

  <!-- Render a markdown table -->
  <xsl:function name="md:render-table" as="item()*">
    <xsl:param name="lines" as="xs:string*"/>
    <xsl:param name="start" as="xs:integer"/>
    <xsl:param name="end" as="xs:integer"/>
    <table>
      <xsl:for-each select="$start to $end">
        <xsl:variable name="i" select="."/>
        <xsl:variable name="line" select="$lines[$i]"/>
        <!-- Skip table separator rows -->
        <xsl:if test="not(matches($line, '^\s*\|[\s\-:|]+\|\s*$'))">
          <tr>
            <xsl:variable name="cells" select="tokenize(replace(replace($line, '^\s*\|', ''), '\|\s*$', ''), '\|')"/>
            <xsl:for-each select="$cells">
              <xsl:choose>
                <xsl:when test="$i = $start">
                  <th><xsl:sequence select="md:inline(normalize-space(.))"/></th>
                </xsl:when>
                <xsl:otherwise>
                  <td><xsl:sequence select="md:inline(normalize-space(.))"/></td>
                </xsl:otherwise>
              </xsl:choose>
            </xsl:for-each>
          </tr>
        </xsl:if>
      </xsl:for-each>
    </table>
  </xsl:function>

  <!-- Render list items -->
  <xsl:function name="md:render-list-items" as="item()*">
    <xsl:param name="lines" as="xs:string*"/>
    <xsl:param name="start" as="xs:integer"/>
    <xsl:param name="end" as="xs:integer"/>
    <xsl:param name="item-pattern" as="xs:string"/>

    <xsl:if test="$start le $end">
      <xsl:variable name="line" select="$lines[$start]"/>
      <xsl:choose>
        <!-- Skip blank lines between items -->
        <xsl:when test="matches($line, '^\s*$')">
          <xsl:sequence select="md:render-list-items($lines, $start + 1, $end, $item-pattern)"/>
        </xsl:when>
        <!-- List item -->
        <xsl:when test="matches($line, $item-pattern)">
          <xsl:variable name="text" select="replace($line, '^\s*[-*+]\s|^\s*\d+\.\s', '')"/>
          <li><xsl:sequence select="md:inline($text)"/></li>
          <xsl:sequence select="md:render-list-items($lines, $start + 1, $end, $item-pattern)"/>
        </xsl:when>
        <!-- Continuation line -->
        <xsl:otherwise>
          <xsl:sequence select="md:render-list-items($lines, $start + 1, $end, $item-pattern)"/>
        </xsl:otherwise>
      </xsl:choose>
    </xsl:if>
  </xsl:function>

  <!--
    Inline formatting: process inline markdown syntax within a text string.
    Handles: code spans, bold, italic, links, and escapes.
  -->
  <xsl:function name="md:inline" as="item()*">
    <xsl:param name="text" as="xs:string"/>
    <xsl:choose>
      <xsl:when test="$text = ''"/>

      <!-- Inline code: `code` -->
      <xsl:when test="contains($text, '`')">
        <xsl:analyze-string select="$text" regex="`([^`]+)`">
          <xsl:matching-substring>
            <code><xsl:value-of select="regex-group(1)"/></code>
          </xsl:matching-substring>
          <xsl:non-matching-substring>
            <xsl:sequence select="md:inline-emphasis(.)"/>
          </xsl:non-matching-substring>
        </xsl:analyze-string>
      </xsl:when>

      <xsl:otherwise>
        <xsl:sequence select="md:inline-emphasis($text)"/>
      </xsl:otherwise>
    </xsl:choose>
  </xsl:function>

  <!-- Process bold and italic -->
  <xsl:function name="md:inline-emphasis" as="item()*">
    <xsl:param name="text" as="xs:string"/>
    <xsl:choose>
      <xsl:when test="$text = ''"/>

      <!-- Bold: **text** (must check before italic) -->
      <xsl:when test="contains($text, '**')">
        <xsl:analyze-string select="$text" regex="\*\*([^\*]+)\*\*">
          <xsl:matching-substring>
            <strong><xsl:sequence select="md:inline-italic(regex-group(1))"/></strong>
          </xsl:matching-substring>
          <xsl:non-matching-substring>
            <xsl:sequence select="md:inline-italic(.)"/>
          </xsl:non-matching-substring>
        </xsl:analyze-string>
      </xsl:when>

      <xsl:otherwise>
        <xsl:sequence select="md:inline-italic($text)"/>
      </xsl:otherwise>
    </xsl:choose>
  </xsl:function>

  <!-- Process italic: *text* -->
  <xsl:function name="md:inline-italic" as="item()*">
    <xsl:param name="text" as="xs:string"/>
    <xsl:choose>
      <xsl:when test="$text = ''"/>

      <xsl:when test="matches($text, '\*[^\s*][^*]*\*')">
        <xsl:analyze-string select="$text" regex="\*([^\s*][^*]*)\*">
          <xsl:matching-substring>
            <em><xsl:sequence select="md:inline-links(regex-group(1))"/></em>
          </xsl:matching-substring>
          <xsl:non-matching-substring>
            <xsl:sequence select="md:inline-links(.)"/>
          </xsl:non-matching-substring>
        </xsl:analyze-string>
      </xsl:when>

      <xsl:otherwise>
        <xsl:sequence select="md:inline-links($text)"/>
      </xsl:otherwise>
    </xsl:choose>
  </xsl:function>

  <!-- Process links: [text](url) -->
  <xsl:function name="md:inline-links" as="item()*">
    <xsl:param name="text" as="xs:string"/>
    <xsl:choose>
      <xsl:when test="$text = ''"/>

      <xsl:when test="matches($text, '\[.*?\]\(.*?\)')">
        <xsl:analyze-string select="$text" regex="\[([^\]]+)\]\(([^\)]+)\)">
          <xsl:matching-substring>
            <a href="{regex-group(2)}"><xsl:value-of select="regex-group(1)"/></a>
          </xsl:matching-substring>
          <xsl:non-matching-substring>
            <xsl:value-of select="."/>
          </xsl:non-matching-substring>
        </xsl:analyze-string>
      </xsl:when>

      <xsl:otherwise>
        <xsl:value-of select="$text"/>
      </xsl:otherwise>
    </xsl:choose>
  </xsl:function>

</xsl:stylesheet>

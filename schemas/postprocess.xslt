<?xml version="1.0" encoding="UTF-8"?>
<!--
  XSLT 1.0 transform: synthesize rel:relation type="references" from
  trm:ref and src:cite elements.

  Identity transform that copies all nodes unchanged, then appends one
  rel:relation element for each unique (ancestor-id, term) pair found
  in trm:ref elements, and one for each unique (ancestor-id, source)
  pair found in src:cite elements. The "from" attribute is the nearest
  ancestor with an @id attribute; "to" is the referenced ID.

  Applied by clayers-cli during combined document assembly.
-->
<xsl:stylesheet version="1.0"
    xmlns:xsl="http://www.w3.org/1999/XSL/Transform"
    xmlns:trm="urn:clayers:terminology"
    xmlns:src="urn:clayers:source"
    xmlns:rel="urn:clayers:relation"
    xmlns:xmi="http://www.omg.org/spec/XMI/20131001"
    xmlns:uml="http://www.omg.org/spec/UML/20131001"
    xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">

  <!-- Muenchian grouping key: unique (ancestor-id, term) pairs -->
  <xsl:key name="ref-pair"
           match="//trm:ref[@term]"
           use="concat(ancestor::*[@id][1]/@id, '|', @term)"/>

  <!-- Muenchian grouping key: unique (ancestor-id, source) pairs for citations -->
  <xsl:key name="cite-pair"
           match="//src:cite[@source]"
           use="concat(ancestor::*[@id][1]/@id, '|', @source)"/>

  <!-- UML relationship keys: group by relationship identity to deduplicate.
       UML Association: both memberEnd endpoints with xml:id produce "references" relations.
       UML Generalization: child refines parent, keyed by (child-xml:id, general).
       UML InterfaceRealization: concrete implements interface, keyed by (client-xml:id, contract-xml:id).
       UML Dependency/Usage: source depends-on target, keyed by (client-xml:id, supplier-xml:id). -->

  <!-- Generalization: keyed by child classifier xml:id + general attribute -->
  <xsl:key name="uml-generalization"
           match="//*[local-name()='generalization']"
           use="concat(ancestor::*[@xml:id][1]/@xml:id, '|', @general)"/>

  <!-- InterfaceRealization: keyed by client xml:id + contract xml:id -->
  <xsl:key name="uml-realization"
           match="//*[local-name()='interfaceRealization']"
           use="concat(ancestor::*[@xml:id][1]/@xml:id, '|', @contract)"/>

  <!-- Identity transform: copy everything -->
  <xsl:template match="@*|node()">
    <xsl:copy>
      <xsl:apply-templates select="@*|node()"/>
    </xsl:copy>
  </xsl:template>

  <!-- At the root element, copy children then append synthesized relations -->
  <xsl:template match="/*">
    <xsl:copy>
      <xsl:apply-templates select="@*|node()"/>

      <!-- For each unique (ancestor-id, term) pair, emit a relation -->
      <xsl:for-each select="//trm:ref[@term]
                            [ancestor::*[@id]]
                            [generate-id() = generate-id(key('ref-pair',
                              concat(ancestor::*[@id][1]/@id, '|', @term))[1])]">
        <xsl:variable name="from-id" select="ancestor::*[@id][1]/@id"/>
        <xsl:variable name="to-id" select="@term"/>
        <!-- Skip self-references -->
        <xsl:if test="$from-id != $to-id">
          <rel:relation type="references" from="{$from-id}" to="{$to-id}"/>
        </xsl:if>
      </xsl:for-each>

      <!-- For each unique (ancestor-id, source) pair, emit a relation -->
      <xsl:for-each select="//src:cite[@source]
                            [ancestor::*[@id]]
                            [generate-id() = generate-id(key('cite-pair',
                              concat(ancestor::*[@id][1]/@id, '|', @source))[1])]">
        <xsl:variable name="from-id" select="ancestor::*[@id][1]/@id"/>
        <xsl:variable name="to-id" select="@source"/>
        <!-- Skip self-references -->
        <xsl:if test="$from-id != $to-id">
          <rel:relation type="references" from="{$from-id}" to="{$to-id}"/>
        </xsl:if>
      </xsl:for-each>

      <!-- === UML relationship synthesis ===
           Synthesize rel:relation entries from UML structural relationships.
           Only elements with xml:id are Clayers-addressable; elements without
           xml:id are invisible to the relation layer. -->

      <!-- UML Association: emit "references" for each association whose
           memberEnd endpoints both resolve to classifiers with xml:id.
           Associations are packagedElement with xsi:type="uml:Association". -->
      <xsl:for-each select="//*[@xsi:type='uml:Association']">
        <xsl:variable name="assoc" select="."/>
        <xsl:for-each select="*[local-name()='memberEnd']">
          <xsl:variable name="end-ref" select="@xmi:idref"/>
          <!-- Find the classifier owning this end and get its xml:id -->
          <xsl:variable name="owner"
                        select="//*[@xmi:id=$end-ref]/ancestor::*[@xml:id][1]"/>
          <!-- We handle this at the association level below -->
        </xsl:for-each>
      </xsl:for-each>

      <!-- UML Association (simplified): for each Association element with xml:id
           on the containing classifiers, synthesize references.
           Match ownedEnd or memberEnd patterns to find connected classifiers. -->
      <xsl:for-each select="//*[@xsi:type='uml:Association' and @xml:id]">
        <xsl:variable name="assoc-id" select="@xml:id"/>
        <xsl:for-each select="*[local-name()='ownedEnd'][@type]">
          <xsl:variable name="type-ref" select="@type"/>
          <xsl:variable name="target" select="//*[@xmi:id=$type-ref]/@xml:id"/>
          <xsl:if test="$target and $assoc-id != $target">
            <rel:relation type="references" from="{$assoc-id}" to="{$target}"/>
          </xsl:if>
        </xsl:for-each>
      </xsl:for-each>

      <!-- UML Association between two classifiers with xml:id via ownedAttribute navigableOwnedEnd:
           Find classifiers that reference each other through association. -->
      <xsl:for-each select="//*[@xml:id]/*[local-name()='ownedAttribute' and @association]">
        <xsl:variable name="from-id" select="ancestor::*[@xml:id][1]/@xml:id"/>
        <xsl:variable name="type-ref" select="@type"/>
        <xsl:variable name="to-id" select="//*[@xmi:id=$type-ref]/@xml:id"/>
        <xsl:if test="$to-id and $from-id != $to-id">
          <rel:relation type="references" from="{$from-id}" to="{$to-id}"/>
        </xsl:if>
      </xsl:for-each>

      <!-- UML Generalization: child "refines" parent.
           Generalization elements live inside their owning classifier.
           The @general attribute references the parent classifier's xmi:id. -->
      <xsl:for-each select="//*[local-name()='generalization'][@general]
                            [ancestor::*[@xml:id]]
                            [generate-id() = generate-id(key('uml-generalization',
                              concat(ancestor::*[@xml:id][1]/@xml:id, '|', @general))[1])]">
        <xsl:variable name="from-id" select="ancestor::*[@xml:id][1]/@xml:id"/>
        <xsl:variable name="general-ref" select="@general"/>
        <xsl:variable name="to-id" select="//*[@xmi:id=$general-ref]/@xml:id"/>
        <xsl:if test="$to-id and $from-id != $to-id">
          <rel:relation type="refines" from="{$from-id}" to="{$to-id}"/>
        </xsl:if>
      </xsl:for-each>

      <!-- UML InterfaceRealization: concrete "implements" interface.
           interfaceRealization lives inside the implementing classifier.
           The @contract attribute references the interface's xmi:id. -->
      <xsl:for-each select="//*[local-name()='interfaceRealization'][@contract]
                            [ancestor::*[@xml:id]]
                            [generate-id() = generate-id(key('uml-realization',
                              concat(ancestor::*[@xml:id][1]/@xml:id, '|', @contract))[1])]">
        <xsl:variable name="from-id" select="ancestor::*[@xml:id][1]/@xml:id"/>
        <xsl:variable name="contract-ref" select="@contract"/>
        <xsl:variable name="to-id" select="//*[@xmi:id=$contract-ref]/@xml:id"/>
        <xsl:if test="$to-id and $from-id != $to-id">
          <rel:relation type="implements" from="{$from-id}" to="{$to-id}"/>
        </xsl:if>
      </xsl:for-each>

      <!-- UML Dependency: source "depends-on" target.
           Dependency elements are packagedElement with xsi:type="uml:Dependency"
           or "uml:Usage". They have @client and @supplier attributes. -->
      <xsl:for-each select="//*[@xsi:type='uml:Dependency' or @xsi:type='uml:Usage']">
        <xsl:variable name="client-ref" select="@client"/>
        <xsl:variable name="supplier-ref" select="@supplier"/>
        <xsl:variable name="from-id" select="//*[@xmi:id=$client-ref]/@xml:id"/>
        <xsl:variable name="to-id" select="//*[@xmi:id=$supplier-ref]/@xml:id"/>
        <xsl:if test="$from-id and $to-id and $from-id != $to-id">
          <rel:relation type="depends-on" from="{$from-id}" to="{$to-id}"/>
        </xsl:if>
      </xsl:for-each>
    </xsl:copy>
  </xsl:template>

</xsl:stylesheet>

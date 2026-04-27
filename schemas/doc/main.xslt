<?xml version="1.0" encoding="UTF-8"?>
<xsl:stylesheet version="3.0"
    xmlns:xsl="http://www.w3.org/1999/XSL/Transform"
    xmlns:xs="http://www.w3.org/2001/XMLSchema"
    xmlns:cmb="urn:clayers:combined"
    xmlns:spec="urn:clayers:spec"
    xmlns:pr="urn:clayers:prose"
    xmlns:trm="urn:clayers:terminology"
    xmlns:org="urn:clayers:organization"
    xmlns:rel="urn:clayers:relation"
    xmlns:dec="urn:clayers:decision"
    xmlns:diag="urn:clayers:diagnostic"
    xmlns:delib="urn:clayers:deliberation"
    xmlns:src="urn:clayers:source"
    xmlns:pln="urn:clayers:plan"
    xmlns:art="urn:clayers:artifact"
    xmlns:llm="urn:clayers:llm"
    xmlns:py="urn:clayers:python"
    xmlns:rev="urn:clayers:revision"
    xmlns:idx="urn:clayers:index"
    xmlns:doc="urn:clayers:doc"
    exclude-result-prefixes="xs cmb spec pr trm org rel dec diag delib src pln art llm py rev idx doc">

  <xsl:import href="catchall.xslt"/>
  <xsl:import href="prose.xslt"/>
  <xsl:import href="terminology.xslt"/>
  <xsl:import href="organization.xslt"/>
  <xsl:import href="relation.xslt"/>
  <xsl:import href="decision.xslt"/>
  <xsl:import href="diagnostic.xslt"/>
  <xsl:import href="deliberation.xslt"/>
  <xsl:import href="source.xslt"/>
  <xsl:import href="plan.xslt"/>
  <xsl:import href="artifact.xslt"/>
  <xsl:import href="llm.xslt"/>
  <xsl:import href="python.xslt"/>
  <xsl:import href="revision.xslt"/>
  <xsl:import href="graph.xslt"/>

  <xsl:output method="html" encoding="UTF-8" indent="yes"/>

  <!-- Root template -->
  <xsl:template match="/">
    <xsl:apply-templates select="cmb:spec"/>
  </xsl:template>

  <xsl:template match="cmb:spec">
    <html lang="en">
      <head>
        <meta charset="UTF-8"/>
        <meta name="viewport" content="width=device-width, initial-scale=1.0"/>
        <title>
          <xsl:choose>
            <xsl:when test="pr:section[1]/pr:title">
              <xsl:value-of select="pr:section[1]/pr:title"/>
            </xsl:when>
            <xsl:otherwise>Specification Documentation</xsl:otherwise>
          </xsl:choose>
        </title>
        <link rel="preconnect" href="https://fonts.googleapis.com"/>
        <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="crossorigin"/>
        <link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600&amp;family=Newsreader:opsz,wght@6..72,400;6..72,600&amp;family=JetBrains+Mono:wght@400&amp;display=swap" rel="stylesheet"/>
        <link id="hljs-light" rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.11.1/styles/github.min.css"/>
        <link id="hljs-dark" rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.11.1/styles/github-dark.min.css" disabled="disabled"/>
        <script src="https://cdn.jsdelivr.net/npm/fuse.js@7.1.0/dist/fuse.min.js"></script>
        <style>
          <xsl:text>
/* shadcn v4 design tokens */
:root {
  --radius: 0.625rem;
  --background: 0 0% 100%;
  --foreground: 240 10% 3.9%;
  --card: 0 0% 100%;
  --card-foreground: 240 10% 3.9%;
  --popover: 0 0% 100%;
  --popover-foreground: 240 10% 3.9%;
  --primary: 240 5.9% 10%;
  --primary-foreground: 0 0% 98%;
  --secondary: 240 4.8% 95.9%;
  --secondary-foreground: 240 5.9% 10%;
  --muted: 240 4.8% 95.9%;
  --muted-foreground: 240 3.8% 46.1%;
  --accent: 240 4.8% 95.9%;
  --accent-foreground: 240 5.9% 10%;
  --destructive: 0 84.2% 60.2%;
  --border: 240 5.9% 90%;
  --input: 240 5.9% 90%;
  --ring: 240 5.9% 10%;
  --sidebar-width: 280px;
  --clr-accent: #18181b;
  --clr-accent-hover: #3f3f46;
  --clr-green: #16a34a;
  --clr-yellow: #ca8a04;
  --clr-red: #dc2626;
  --clr-blue: #2563eb;
  --clr-gray: #71717a;
  --clr-purple: #7c3aed;
}

[data-theme="dark"] {
  --background: 240 10% 3.9%;
  --foreground: 0 0% 98%;
  --card: 240 10% 3.9%;
  --card-foreground: 0 0% 98%;
  --popover: 240 10% 3.9%;
  --popover-foreground: 0 0% 98%;
  --primary: 0 0% 98%;
  --primary-foreground: 240 5.9% 10%;
  --secondary: 240 3.7% 15.9%;
  --secondary-foreground: 0 0% 98%;
  --muted: 240 3.7% 15.9%;
  --muted-foreground: 240 5% 64.9%;
  --accent: 240 3.7% 15.9%;
  --accent-foreground: 0 0% 98%;
  --destructive: 0 62.8% 30.6%;
  --border: 240 3.7% 15.9%;
  --input: 240 3.7% 15.9%;
  --ring: 240 4.9% 83.9%;
  --clr-accent: #fafafa;
  --clr-accent-hover: #d4d4d8;
  --clr-green: #4ade80;
  --clr-yellow: #facc15;
  --clr-red: #f87171;
  --clr-blue: #60a5fa;
  --clr-gray: #a1a1aa;
  --clr-purple: #a78bfa;
}

@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
    --background: 240 10% 3.9%;
    --foreground: 0 0% 98%;
    --card: 240 10% 3.9%;
    --card-foreground: 0 0% 98%;
    --popover: 240 10% 3.9%;
    --popover-foreground: 0 0% 98%;
    --primary: 0 0% 98%;
    --primary-foreground: 240 5.9% 10%;
    --secondary: 240 3.7% 15.9%;
    --secondary-foreground: 0 0% 98%;
    --muted: 240 3.7% 15.9%;
    --muted-foreground: 240 5% 64.9%;
    --accent: 240 3.7% 15.9%;
    --accent-foreground: 0 0% 98%;
    --destructive: 0 62.8% 30.6%;
    --border: 240 3.7% 15.9%;
    --input: 240 3.7% 15.9%;
    --ring: 240 4.9% 83.9%;
    --clr-accent: #fafafa;
    --clr-accent-hover: #d4d4d8;
    --clr-green: #4ade80;
    --clr-yellow: #facc15;
    --clr-red: #f87171;
    --clr-blue: #60a5fa;
    --clr-gray: #a1a1aa;
    --clr-purple: #a78bfa;
  }
}

*, *::before, *::after { box-sizing: border-box; }

body {
  margin: 0;
  font-family: "Inter", -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  font-size: 15px;
  line-height: 1.55;
  font-optical-sizing: auto;
  letter-spacing: -0.011em;
  color: hsl(var(--foreground));
  background: hsl(var(--background));
  -webkit-font-smoothing: antialiased;
}

/* Sidebar */
nav#sidebar {
  position: fixed;
  top: 0; left: 0;
  width: var(--sidebar-width);
  height: 100vh;
  overflow-y: auto;
  background: hsl(var(--muted));
  border-right: 1px solid hsl(var(--border));
  padding: 0.75rem;
  font-size: 0.8125rem;
}

/* Theme toggle - top-right of search row */
.sidebar-header {
  display: flex;
  gap: 0.375rem;
  align-items: center;
  margin-bottom: 0.5rem;
}
.sidebar-header #search { margin-bottom: 0; flex: 1; }
nav#sidebar .theme-toggle {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 2.25rem;
  height: 2.25rem;
  flex-shrink: 0;
  padding: 0;
  background: hsl(var(--background));
  border: 1px solid hsl(var(--input));
  border-radius: calc(var(--radius) - 2px);
  color: hsl(var(--muted-foreground));
  cursor: pointer;
  transition: background 0.15s, color 0.15s;
}
nav#sidebar .theme-toggle:hover { background: hsl(var(--accent)); color: hsl(var(--accent-foreground)); }
nav#sidebar .theme-toggle svg { width: 14px; height: 14px; }
.icon-sun, .icon-moon { display: none; }
:root:not([data-theme="dark"]) .icon-sun { display: block; }
[data-theme="dark"] .icon-moon { display: block; }
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) .icon-sun { display: none; }
  :root:not([data-theme="light"]) .icon-moon { display: block; }
}

nav#sidebar ul { list-style: none; padding: 0; margin: 0; }
nav#sidebar li { margin: 1px 0; }
nav#sidebar li li { padding-left: 0.75rem; }
nav#sidebar li li li { padding-left: 0.75rem; }

nav#sidebar a {
  color: hsl(var(--muted-foreground));
  text-decoration: none;
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  padding: 0.25rem 0.5rem;
  border-radius: calc(var(--radius) - 4px);
  line-height: 1.4;
  font-weight: 400;
  transition: background 0.1s, color 0.1s;
}
nav#sidebar .conn-count {
  font-size: 0.625rem;
  font-weight: 500;
  font-variant-numeric: tabular-nums;
  color: hsl(var(--muted-foreground));
  opacity: 0.6;
  flex-shrink: 0;
  margin-left: 0.5rem;
  min-width: 1rem;
  text-align: right;
}
nav#sidebar a:hover {
  background: hsl(var(--accent));
  color: hsl(var(--accent-foreground));
}
nav#sidebar a.active {
  background: hsl(var(--primary));
  color: hsl(var(--primary-foreground));
  font-weight: 500;
}

nav#sidebar .toc-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-weight: 600;
  font-size: 0.6875rem;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: hsl(var(--muted-foreground));
  margin-top: 1rem;
  margin-bottom: 0.25rem;
  padding-left: 0.5rem;
  padding-right: 0.25rem;
}
nav#sidebar .toc-heading .heading-left {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  cursor: pointer;
  user-select: none;
}
nav#sidebar .toc-heading .chevron {
  display: inline-block;
  width: 10px;
  height: 10px;
  transition: transform 0.15s;
}
nav#sidebar .nav-group.collapsed .chevron { transform: rotate(-90deg); }
nav#sidebar .nav-group.collapsed > ul { display: none; }
nav#sidebar .nav-group > ul { padding-left: 0.75rem; }

/* Sort controls */
.sort-controls { display: flex; gap: 2px; }
.sort-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 1.375rem;
  height: 1.375rem;
  padding: 0;
  border: 1px solid transparent;
  border-radius: calc(var(--radius) - 4px);
  background: none;
  color: hsl(var(--muted-foreground));
  cursor: pointer;
  font-size: 0.625rem;
  font-weight: 600;
  transition: all 0.1s;
}
.sort-btn:hover { background: hsl(var(--accent)); color: hsl(var(--accent-foreground)); }
.sort-btn.active {
  background: hsl(var(--accent));
  color: hsl(var(--accent-foreground));
  border-color: hsl(var(--border));
}

/* Main content */
.content-wrapper {
  margin-left: var(--sidebar-width);
  display: flex;
  min-height: 100vh;
}
.content-wrapper:has(#graph-panel:not(.collapsed)) {
  height: 100vh;
  overflow: hidden;
}
.content-wrapper:has(#graph-panel.collapsed) {
  display: block;
}
main {
  max-width: 48rem;
  padding: 2rem 2.5rem 4rem;
}
.content-wrapper:has(#graph-panel:not(.collapsed)) main {
  flex: 1;
  min-width: 0;
  max-width: none;
  overflow-y: auto;
}

/* Graph panel */
#graph-panel {
  width: 40%;
  min-width: 280px;
  height: 100vh;
  border-left: 1px solid hsl(var(--border));
  background: hsl(var(--background));
  display: flex;
  flex-direction: column;
}
#graph-panel.collapsed { display: none; }
.panel-divider {
  width: 6px;
  cursor: col-resize;
  background: hsl(var(--border));
  flex-shrink: 0;
  transition: background 0.15s;
}
.panel-divider:hover, .panel-divider.dragging { background: hsl(var(--ring)); }
/* Divider hidden via JS when panel is collapsed */
.graph-toolbar {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.5rem 0.75rem;
  border-bottom: 1px solid hsl(var(--border));
  font-size: 0.75rem;
  flex-shrink: 0;
}
#graph-search {
  flex: 1;
  min-width: 80px;
  max-width: 160px;
  font-size: 0.7rem;
  padding: 0.2rem 0.4rem;
  border-radius: var(--radius);
  border: 1px solid hsl(var(--border));
  background: hsl(var(--background));
  color: hsl(var(--foreground));
  outline: none;
}
#graph-search:focus { border-color: hsl(var(--ring)); }
.graph-btn-group {
  display: inline-flex;
  border: 1px solid hsl(var(--border));
  border-radius: var(--radius);
  overflow: hidden;
}
.graph-btn-group .graph-tb { border-radius: 0; border: none; border-right: 1px solid hsl(var(--border)); }
.graph-btn-group .graph-tb:last-child { border-right: none; }
.graph-tb {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 24px;
  padding: 0;
  border-radius: var(--radius);
  border: 1px solid hsl(var(--border));
  background: hsl(var(--background));
  color: hsl(var(--muted-foreground));
  cursor: pointer;
}
.graph-tb:hover { background: hsl(var(--accent)); color: hsl(var(--accent-foreground)); }
.graph-tb.active { background: hsl(var(--ring)); color: hsl(var(--primary-foreground)); }
#graph-container {
  flex: 1;
  overflow: hidden;
  position: relative;
}

/* Graph toggle button */
nav#sidebar .graph-toggle {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: none;
  background: transparent;
  color: hsl(var(--muted-foreground));
  border-radius: var(--radius);
  cursor: pointer;
  padding: 0;
  flex-shrink: 0;
}
nav#sidebar .graph-toggle:hover { background: hsl(var(--accent)); color: hsl(var(--accent-foreground)); }
nav#sidebar .graph-toggle.active { background: hsl(var(--ring)); color: hsl(var(--primary-foreground)); }
nav#sidebar .graph-toggle svg { width: 14px; height: 14px; }

/* Content highlight for bidirectional hover */
.content-highlight {
  outline: 2px solid hsl(var(--ring));
  outline-offset: 2px;
  border-radius: var(--radius);
  transition: outline-color 0.2s;
}

/* Mobile graph */
@media (max-width: 768px) {
  .content-wrapper { margin-left: 0; height: auto; overflow: visible; }
  main { overflow: visible; height: auto; }
  .panel-divider { display: none !important; }
  #graph-panel { display: none; }
  #graph-panel.mobile-active {
    display: flex;
    position: fixed;
    top: 0; left: 0; right: 0; bottom: 0;
    width: 100% !important;
    height: 100%;
    z-index: 999;
    border-left: none;
  }
}

/* Headings */
h1, h2, h3, h4, h5, h6 {
  font-family: "Newsreader", Georgia, serif;
  margin-top: 2rem;
  margin-bottom: 0.75rem;
  line-height: 1.2;
  color: hsl(var(--foreground));
  letter-spacing: -0.01em;
}
h1 { font-size: 2.25rem; border-bottom: 1px solid hsl(var(--border)); padding-bottom: 0.5rem; }
h2 { font-size: 1.75rem; border-bottom: 1px solid hsl(var(--border)); padding-bottom: 0.25rem; }
h3 { font-size: 1.375rem; }
h4 { font-size: 1.125rem; }

.xml-source-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 1.75rem;
  height: 1.75rem;
  margin-left: 0.35rem;
  padding: 0;
  vertical-align: text-bottom;
  border: 1px solid hsl(var(--border));
  border-radius: calc(var(--radius) - 3px);
  background: hsl(var(--background));
  color: hsl(var(--muted-foreground));
  cursor: pointer;
  transition: background 0.15s, color 0.15s, border-color 0.15s;
}
.xml-source-button:hover {
  background: hsl(var(--accent));
  color: hsl(var(--accent-foreground));
  border-color: hsl(var(--ring));
}
.xml-source-button svg { width: 0.95rem; height: 0.95rem; }

body.xml-modal-open { overflow: hidden; }
.xml-source-modal {
  position: fixed;
  inset: 0;
  z-index: 1300;
  display: none;
  align-items: center;
  justify-content: center;
  padding: 1.25rem;
}
.xml-source-modal.open { display: flex; }
.xml-source-backdrop {
  position: absolute;
  inset: 0;
  background: rgb(0 0 0 / 0.42);
}
.xml-source-window {
  position: relative;
  z-index: 1;
  display: flex;
  flex-direction: column;
  width: min(1120px, calc(100vw - 2.5rem));
  max-height: min(82vh, 900px);
  border: 1px solid hsl(var(--border));
  border-radius: var(--radius);
  background: hsl(var(--card));
  color: hsl(var(--card-foreground));
  box-shadow: 0 24px 70px rgb(0 0 0 / 0.28);
  overflow: hidden;
}
.xml-source-header {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.75rem 0.9rem;
  border-bottom: 1px solid hsl(var(--border));
  background: hsl(var(--muted));
}
.xml-source-title {
  min-width: 0;
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 0.85rem;
  font-weight: 600;
}
.xml-source-action {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 2rem;
  height: 2rem;
  padding: 0;
  border: 1px solid hsl(var(--border));
  border-radius: calc(var(--radius) - 3px);
  background: hsl(var(--background));
  color: hsl(var(--muted-foreground));
  cursor: pointer;
}
.xml-source-action:hover { color: hsl(var(--foreground)); background: hsl(var(--accent)); }
.xml-source-action svg { width: 1rem; height: 1rem; }
.xml-source-browser {
  flex: 1;
  max-height: calc(min(82vh, 900px) - 3.6rem);
  overflow: auto;
  background: hsl(var(--background));
  padding: 0.85rem 1rem 1rem;
  font-family: "JetBrains Mono", "SFMono-Regular", Consolas, monospace;
  font-size: 0.78rem;
  line-height: 1.55;
}
.xml-source-node { min-width: 0; }
.xml-source-line {
  display: flex;
  align-items: baseline;
  gap: 0.15rem;
  min-width: 0;
  min-height: 1.35rem;
  white-space: pre;
}
.xml-source-text-line {
  max-width: 92ch;
  white-space: normal;
}
.xml-source-text-line .xml-source-text {
  flex: 1 1 auto;
  min-width: 0;
  overflow-wrap: anywhere;
  white-space: normal;
}
.xml-source-attr-line {
  max-width: 92ch;
  padding-left: 1.4rem;
  white-space: normal;
}
.xml-source-attr-line .xml-source-attr-value {
  overflow-wrap: anywhere;
  white-space: normal;
}
.xml-source-toggle,
.xml-source-spacer {
  flex: 0 0 1.25rem;
  width: 1.25rem;
  height: 1.25rem;
}
.xml-source-toggle {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: 1px solid hsl(var(--border));
  border-radius: calc(var(--radius) - 4px);
  background: hsl(var(--card));
  color: hsl(var(--muted-foreground));
  font: inherit;
  line-height: 1;
  cursor: pointer;
}
.xml-source-toggle:hover {
  color: hsl(var(--foreground));
  background: hsl(var(--accent));
}
.xml-source-children { margin-left: 1.5rem; }
.xml-source-node.collapsed > .xml-source-children,
.xml-source-node.collapsed > .xml-source-close { display: none; }
.xml-source-punct { color: hsl(var(--muted-foreground)); }
.xml-source-name { color: hsl(var(--primary)); font-weight: 600; }
.xml-source-attr-name { color: #b45309; }
.dark .xml-source-attr-name { color: #fbbf24; }
.xml-source-attr-value { color: #047857; }
.dark .xml-source-attr-value { color: #34d399; }
.xml-source-text { color: hsl(var(--foreground)); }
.xml-source-comment { color: hsl(var(--muted-foreground)); font-style: italic; }
.xml-source-error {
  margin: 0 0 0.75rem;
  color: hsl(var(--destructive));
  font-weight: 600;
}
.xml-source-raw {
  margin: 0;
  white-space: pre;
}
@media (max-width: 768px) {
  .xml-source-modal { padding: 0.75rem; }
  .xml-source-window { width: 100%; max-height: 88vh; }
}

.shortdesc {
  color: hsl(var(--muted-foreground));
  font-style: italic;
  margin-bottom: 1rem;
}

/* Tables */
table {
  border-collapse: collapse;
  width: 100%;
  margin: 1rem 0;
  font-size: 0.875rem;
}
th, td {
  border: 1px solid hsl(var(--border));
  padding: 0.5rem 0.75rem;
  text-align: left;
}
th { background: hsl(var(--muted)); font-weight: 500; }

/* Code */
code {
  font-family: "JetBrains Mono", "SF Mono", Menlo, Consolas, monospace;
  font-size: 0.85em;
  background: hsl(var(--muted));
  padding: 0.15em 0.35em;
  border-radius: calc(var(--radius) - 4px);
}
pre {
  background: hsl(var(--muted));
  padding: 1rem;
  border-radius: var(--radius);
  border: 1px solid hsl(var(--border));
  overflow-x: auto;
  font-size: 0.8125rem;
  line-height: 1.6;
}
pre code { background: none; padding: 0; border: none; }

/* Links */
a { color: var(--clr-accent); text-underline-offset: 4px; }
a:hover { color: var(--clr-accent-hover); }

/* Badges */
.badge {
  display: inline-flex;
  align-items: center;
  font-size: 0.6875rem;
  font-weight: 500;
  padding: 0.1em 0.5em;
  border-radius: 9999px;
  text-transform: lowercase;
  letter-spacing: 0.02em;
  border: 1px solid transparent;
}
.badge-green { background: color-mix(in srgb, var(--clr-green) 15%, transparent); color: var(--clr-green); border-color: color-mix(in srgb, var(--clr-green) 30%, transparent); }
.badge-yellow { background: color-mix(in srgb, var(--clr-yellow) 15%, transparent); color: var(--clr-yellow); border-color: color-mix(in srgb, var(--clr-yellow) 30%, transparent); }
.badge-red { background: color-mix(in srgb, var(--clr-red) 15%, transparent); color: var(--clr-red); border-color: color-mix(in srgb, var(--clr-red) 30%, transparent); }
.badge-blue { background: color-mix(in srgb, var(--clr-blue) 15%, transparent); color: var(--clr-blue); border-color: color-mix(in srgb, var(--clr-blue) 30%, transparent); }
.badge-gray { background: hsl(var(--muted)); color: hsl(var(--muted-foreground)); border-color: hsl(var(--border)); }

/* Cards */
.card {
  border: 1px solid hsl(var(--border));
  border-radius: var(--radius);
  padding: 1.25rem 1.5rem;
  margin: 1rem 0;
  background: hsl(var(--card));
  box-shadow: 0 1px 2px 0 rgb(0 0 0 / 0.05);
}
.card h4 { margin-top: 0; }

/* Note callouts */
.note {
  border-left: 3px solid var(--clr-blue);
  padding: 0.75rem 1rem;
  margin: 1rem 0;
  background: hsl(var(--muted));
  border-radius: 0 var(--radius) var(--radius) 0;
}
.note-warning { border-left-color: var(--clr-yellow); }
.note-danger { border-left-color: var(--clr-red); }
.note-tip { border-left-color: var(--clr-green); }
.note-label {
  font-weight: 600;
  font-size: 0.8rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin-bottom: 0.25rem;
}

/* Technical documentation blocks */
dl {
  margin: 1rem 0;
}
dt {
  font-weight: 600;
  margin-top: 0.75rem;
}
dd {
  margin-left: 1.25rem;
  color: hsl(var(--foreground));
}
.task-detail,
.troubleshooting,
.example,
.step-example,
.sidebar-block,
.related-links {
  border: 1px solid hsl(var(--border));
  border-radius: calc(var(--radius) - 2px);
  padding: 0.75rem 1rem;
  margin: 1rem 0;
  background: hsl(var(--card));
}
.task-detail-label,
.trouble-label,
.example-title,
.related-links-title,
.table-title {
  font-weight: 600;
  font-size: 0.8rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: hsl(var(--muted-foreground));
  margin-bottom: 0.35rem;
}
.task-cmd {
  font-weight: 600;
  margin: 0.35rem 0;
}
.stepsection {
  list-style: none;
  margin-left: -1.25rem;
  color: hsl(var(--muted-foreground));
}
figure {
  margin: 1.25rem 0;
}
figure img,
.content img {
  max-width: 100%;
  height: auto;
  border: 1px solid hsl(var(--border));
  border-radius: calc(var(--radius) - 2px);
}
figcaption {
  margin-top: 0.4rem;
  font-size: 0.85rem;
  color: hsl(var(--muted-foreground));
}
blockquote {
  border-left: 3px solid hsl(var(--border));
  margin: 1rem 0;
  padding-left: 1rem;
  color: hsl(var(--muted-foreground));
}
blockquote footer {
  margin-top: 0.5rem;
  font-size: 0.85rem;
}
kbd {
  font-family: "JetBrains Mono", "SF Mono", Menlo, Consolas, monospace;
  font-size: 0.8em;
  border: 1px solid hsl(var(--border));
  border-bottom-width: 2px;
  border-radius: calc(var(--radius) - 4px);
  padding: 0.05em 0.35em;
  background: hsl(var(--background));
}
.link-role {
  margin-left: 0.5rem;
  color: hsl(var(--muted-foreground));
  font-size: 0.8rem;
}

/* Term tooltip */
a.term-ref {
  text-decoration: underline dotted;
  text-underline-offset: 3px;
  cursor: help;
}

/* Collapsible */
details { margin: 0.5rem 0; }
details summary {
  cursor: pointer;
  font-weight: 500;
  color: var(--clr-accent);
}

/* Org badges */
.org-badge {
  font-size: 0.625rem;
  font-weight: 600;
  padding: 0.1em 0.5em;
  border-radius: calc(var(--radius) - 4px);
  margin-left: 0.5rem;
  vertical-align: middle;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}
.org-concept { background: color-mix(in srgb, var(--clr-blue) 15%, transparent); color: var(--clr-blue); }
.org-task { background: color-mix(in srgb, var(--clr-green) 15%, transparent); color: var(--clr-green); }
.org-reference { background: hsl(var(--muted)); color: hsl(var(--muted-foreground)); }

/* Relations */
.relations {
  background: hsl(var(--muted));
  border: 1px solid hsl(var(--border));
  border-radius: var(--radius);
  padding: 0.75rem 1rem;
  margin: 1rem 0;
  font-size: 0.875rem;
}
.relations-title {
  font-weight: 600;
  font-size: 0.6875rem;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: hsl(var(--muted-foreground));
  margin-bottom: 0.5rem;
}
.relation-group {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 0.3rem;
  margin-bottom: 0.35rem;
}
.relation-type {
  display: inline-flex;
  align-items: center;
  font-size: 0.6875rem;
  font-weight: 500;
  padding: 0.1em 0.5em;
  border-radius: 9999px;
  margin-right: 0.25rem;
  white-space: nowrap;
  border: 1px solid transparent;
}
.relation-type-references { background: hsl(var(--muted)); color: hsl(var(--muted-foreground)); border-color: hsl(var(--border)); }
.relation-type-depends-on { background: color-mix(in srgb, var(--clr-yellow) 15%, transparent); color: var(--clr-yellow); border-color: color-mix(in srgb, var(--clr-yellow) 30%, transparent); }
.relation-type-refines { background: color-mix(in srgb, var(--clr-blue) 15%, transparent); color: var(--clr-blue); border-color: color-mix(in srgb, var(--clr-blue) 30%, transparent); }
.relation-type-implements { background: color-mix(in srgb, var(--clr-green) 15%, transparent); color: var(--clr-green); border-color: color-mix(in srgb, var(--clr-green) 30%, transparent); }
.relation-type-constrains { background: color-mix(in srgb, var(--clr-red) 15%, transparent); color: var(--clr-red); border-color: color-mix(in srgb, var(--clr-red) 30%, transparent); }
.relation-type-precedes { background: color-mix(in srgb, var(--clr-purple) 15%, transparent); color: var(--clr-purple); border-color: color-mix(in srgb, var(--clr-purple) 30%, transparent); }
.relation-targets { display: inline; }
.relation-targets a { margin-right: 0.25rem; }
.relation-note {
  font-size: 0.75rem;
  color: hsl(var(--muted-foreground));
  font-style: italic;
  margin-left: 1.75rem;
  line-height: 1.4;
}

/* Search */
#search {
  width: 100%;
  height: 2.25rem;
  padding: 0 0.75rem;
  font-size: 0.8125rem;
  font-family: inherit;
  border: 1px solid hsl(var(--input));
  border-radius: calc(var(--radius) - 2px);
  background: hsl(var(--background));
  color: hsl(var(--foreground));
  outline: none;
  transition: border-color 0.15s, box-shadow 0.15s;
}
#search:focus {
  border-color: hsl(var(--ring));
  box-shadow: 0 0 0 2px hsl(var(--ring) / 0.2);
}
#search.has-results {
  border-bottom-left-radius: 0;
  border-bottom-right-radius: 0;
  border-bottom-color: transparent;
}
#search-results {
  display: none;
  list-style: none;
  padding: 0.25rem;
  margin: -1px 0 0.5rem;
  max-height: 60vh;
  overflow-y: auto;
  border: 1px solid hsl(var(--border));
  border-top: none;
  border-radius: 0 0 calc(var(--radius) - 2px) calc(var(--radius) - 2px);
  background: hsl(var(--popover));
  color: hsl(var(--popover-foreground));
  box-shadow: 0 4px 6px -1px rgb(0 0 0 / 0.1), 0 2px 4px -2px rgb(0 0 0 / 0.1);
}
#search-results.visible { display: block; }
#search-results li {
  padding: 0.4rem 0.5rem;
  cursor: pointer;
  font-size: 0.8125rem;
  border-radius: calc(var(--radius) - 4px);
  display: flex;
  align-items: center;
  justify-content: space-between;
}
#search-results li + li { margin-top: 1px; }
#search-results li:hover,
#search-results li.selected {
  background: hsl(var(--accent));
  color: hsl(var(--accent-foreground));
}
#search-results .result-type {
  font-size: 0.625rem;
  font-weight: 500;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: hsl(var(--muted-foreground));
  flex-shrink: 0;
  margin-left: 0.5rem;
}

/* Task actor */
.task-actor {
  font-size: 0.8125rem;
  color: hsl(var(--muted-foreground));
  margin-bottom: 0.5rem;
}

/* LLM machine description */
.llm-desc {
  margin: 0.5rem 0;
  font-size: 0.8125rem;
}
.llm-desc summary {
  color: hsl(var(--muted-foreground));
  font-weight: 500;
  font-size: 0.75rem;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  cursor: pointer;
}
.llm-desc p {
  color: hsl(var(--muted-foreground));
  margin: 0.25rem 0 0;
  line-height: 1.5;
}

/* Markdown-formatted LLM descriptions (agent guidance) */
.llm-markdown {
  margin: 0.5rem 0;
}
.llm-markdown summary {
  color: hsl(var(--muted-foreground));
  font-weight: 500;
  font-size: 0.75rem;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  cursor: pointer;
}
.markdown-body {
  font-size: 0.875rem;
  line-height: 1.6;
  padding: 0.5rem 0;
}
.markdown-body h1, .markdown-body h2, .markdown-body h3,
.markdown-body h4, .markdown-body h5, .markdown-body h6 {
  margin: 1.25rem 0 0.5rem;
  line-height: 1.3;
}
.markdown-body h1 { font-size: 1.5rem; border-bottom: 1px solid hsl(var(--border)); padding-bottom: 0.25rem; }
.markdown-body h2 { font-size: 1.25rem; border-bottom: 1px solid hsl(var(--border)); padding-bottom: 0.2rem; }
.markdown-body h3 { font-size: 1.1rem; }
.markdown-body h4 { font-size: 1rem; }
.markdown-body p { margin: 0.5rem 0; }
.markdown-body pre {
  background: hsl(var(--muted));
  border: 1px solid hsl(var(--border));
  border-radius: 0.375rem;
  padding: 0.75rem 1rem;
  overflow-x: auto;
  font-size: 0.8125rem;
  margin: 0.5rem 0;
}
.markdown-body code {
  background: hsl(var(--muted));
  padding: 0.15rem 0.35rem;
  border-radius: 0.25rem;
  font-size: 0.85em;
}
.markdown-body pre code {
  background: none;
  padding: 0;
  border-radius: 0;
  font-size: inherit;
}
.markdown-body table {
  border-collapse: collapse;
  margin: 0.5rem 0;
  font-size: 0.8125rem;
  width: auto;
}
.markdown-body th, .markdown-body td {
  border: 1px solid hsl(var(--border));
  padding: 0.35rem 0.75rem;
  text-align: left;
}
.markdown-body th {
  background: hsl(var(--muted));
  font-weight: 600;
}
.markdown-body ul, .markdown-body ol {
  margin: 0.5rem 0;
  padding-left: 1.5rem;
}
.markdown-body li { margin: 0.2rem 0; }
.markdown-body blockquote {
  border-left: 3px solid hsl(var(--border));
  margin: 0.5rem 0;
  padding: 0.25rem 1rem;
  color: hsl(var(--muted-foreground));
}
.markdown-body hr {
  border: none;
  border-top: 1px solid hsl(var(--border));
  margin: 1rem 0;
}
.markdown-body strong { font-weight: 600; }
.markdown-body em { font-style: italic; }
.markdown-body a { color: hsl(var(--primary)); text-decoration: underline; }

/* Inline artifact mappings per node */
.node-artifacts {
  margin-top: 1.25rem;
  padding-top: 0.75rem;
  border-top: 1px dashed hsl(var(--border));
  font-size: 0.8125rem;
}
.node-artifacts-title {
  font-weight: 600;
  font-size: 0.6875rem;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: hsl(var(--muted-foreground));
  margin-bottom: 0.35rem;
}
.node-artifact-entry {
  margin-bottom: 0.35rem;
}

/* Revision banner */
.revision-banner {
  display: flex;
  gap: 0.35rem;
  margin-bottom: 0.75rem;
}

/* Drift indicators */
a.drift-dot, .drift-dot {
  font-size: 0.6em;
  vertical-align: middle;
  text-decoration: none;
}
.drift-artifact { color: var(--clr-red); }
.drift-spec { color: var(--clr-yellow); }
.drift-both { background: linear-gradient(135deg, var(--clr-yellow) 50%, var(--clr-red) 50%); -webkit-background-clip: text; -webkit-text-fill-color: transparent; background-clip: text; }
a.drift-dot:hover { opacity: 0.7; }

.badge-drift-spec { background: color-mix(in srgb, var(--clr-yellow) 15%, transparent); color: var(--clr-yellow); border-color: color-mix(in srgb, var(--clr-yellow) 30%, transparent); }
.badge-drift-artifact { background: color-mix(in srgb, var(--clr-red) 15%, transparent); color: var(--clr-red); border-color: color-mix(in srgb, var(--clr-red) 30%, transparent); }

.highlight-flash {
  animation: flash 1.5s ease-out;
}
@keyframes flash {
  0% { background: color-mix(in srgb, var(--clr-red) 25%, transparent); }
  100% { background: transparent; }
}

/* Code fragments */
.code-fragment details {
  margin-top: 0.35rem;
}
.code-fragment summary {
  font-size: 0.75rem;
  font-weight: 500;
  color: hsl(var(--muted-foreground));
  cursor: pointer;
  user-select: none;
}
.code-fragment summary:hover { color: hsl(var(--foreground)); }
.code-fragment pre {
  margin: 0.25rem 0 0;
  font-size: 0.75rem;
  line-height: 1.5;
  max-height: 24rem;
  overflow-y: auto;
}
.code-fragment .line-num {
  display: inline-block;
  width: 3.5em;
  text-align: right;
  padding-right: 1em;
  color: hsl(var(--muted-foreground));
  opacity: 0.5;
  user-select: none;
}

/* Breadcrumbs */
.breadcrumbs {
  font-size: 0.75rem;
  color: hsl(var(--muted-foreground));
  margin-bottom: 1rem;
  display: flex;
  align-items: center;
  gap: 0.35rem;
}
.breadcrumbs a {
  color: hsl(var(--muted-foreground));
  text-decoration: none;
}
.breadcrumbs a:hover { color: hsl(var(--foreground)); }
.breadcrumbs .sep { opacity: 0.4; }

/* Anchor links on headings */
.heading-anchor {
  opacity: 0;
  color: hsl(var(--muted-foreground));
  text-decoration: none;
  margin-left: 0.35rem;
  font-size: 0.75em;
  transition: opacity 0.15s;
}
h1:hover .heading-anchor,
h2:hover .heading-anchor,
h3:hover .heading-anchor,
h4:hover .heading-anchor,
h5:hover .heading-anchor,
h6:hover .heading-anchor { opacity: 1; }
.heading-anchor:hover { color: hsl(var(--foreground)); }

/* Smooth page transitions */
.page {
  display: none;
  animation: fadeIn 0.15s ease-out;
}
.page.active { display: block; }
@keyframes fadeIn {
  from { opacity: 0; transform: translateY(4px); }
  to { opacity: 1; transform: translateY(0); }
}

/* Responsive: hamburger on narrow screens */
.sidebar-toggle {
  display: none;
  position: fixed;
  top: 0.5rem;
  left: 0.5rem;
  z-index: 1001;
  width: 2.25rem;
  height: 2.25rem;
  padding: 0;
  border: 1px solid hsl(var(--border));
  border-radius: calc(var(--radius) - 2px);
  background: hsl(var(--background));
  color: hsl(var(--foreground));
  cursor: pointer;
  align-items: center;
  justify-content: center;
}
.sidebar-toggle svg { width: 18px; height: 18px; }

@media (max-width: 768px) {
  :root { --sidebar-width: 280px; }
  .sidebar-toggle { display: inline-flex; }
  nav#sidebar {
    transform: translateX(-100%);
    transition: transform 0.2s ease;
    z-index: 1000;
    box-shadow: 2px 0 8px rgb(0 0 0 / 0.1);
  }
  nav#sidebar.open { transform: translateX(0); }
  .content-wrapper { margin-left: 0; }
  main {
    padding: 3rem 1.25rem 4rem;
  }
}

/* Print stylesheet */
@media print {
  nav#sidebar, .sidebar-toggle, .theme-toggle, .graph-toggle, #search,
  #search-results, .sort-controls, .heading-anchor,
  #graph-panel, .panel-divider { display: none !important; }
  .content-wrapper {
    margin-left: 0 !important;
    display: block;
    height: auto;
    overflow: visible;
  }
  main {
    margin-left: 0 !important;
    overflow: visible !important;
    height: auto !important;
    max-width: 100% !important;
    padding: 0 !important;
  }
  .page { display: block !important; animation: none !important; }
  .page + .page { page-break-before: always; }
  body { font-size: 11pt; color: #000; background: #fff; }
  h1, h2, h3, h4 { page-break-after: avoid; }
  .card { border: 1px solid #ccc; box-shadow: none; }
  .badge { border: 1px solid #999; background: #eee !important; color: #333 !important; }
  a { color: #000; text-decoration: underline; }
  pre { border: 1px solid #ccc; background: #f5f5f5 !important; }
  .relations { border: 1px solid #ccc; background: #f9f9f9 !important; }
  .breadcrumbs { display: none !important; }
}
          </xsl:text>
        </style>
      </head>
      <body>
        <button class="sidebar-toggle" onclick="document.getElementById('sidebar').classList.toggle('open')" aria-label="Toggle sidebar">
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="3" y1="6" x2="21" y2="6"/><line x1="3" y1="12" x2="21" y2="12"/><line x1="3" y1="18" x2="21" y2="18"/></svg>
        </button>
        <nav id="sidebar">
          <div class="sidebar-header">
            <input id="search" type="search" placeholder="Find..." autocomplete="off"/>
            <button class="graph-toggle" onclick="toggleGraphPanel()" aria-label="Toggle graph" title="Toggle knowledge graph">
              <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="6" cy="6" r="3"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="18" r="3"/><line x1="8.5" y1="7.5" x2="15.5" y2="16.5"/><line x1="15.5" y1="7.5" x2="8.5" y2="16.5"/></svg>
            </button>
            <button class="theme-toggle" onclick="toggleTheme()" aria-label="Toggle theme">
              <svg class="icon-sun" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="4"/><path d="M12 2v2"/><path d="M12 20v2"/><path d="m4.93 4.93 1.41 1.41"/><path d="m17.66 17.66 1.41 1.41"/><path d="M2 12h2"/><path d="M20 12h2"/><path d="m6.34 17.66-1.41 1.41"/><path d="m19.07 4.93-1.41 1.41"/></svg>
              <svg class="icon-moon" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/></svg>
            </button>
          </div>
          <ul id="search-results"></ul>

          <!-- Reading Maps (top of sidebar, collapsible) -->
          <xsl:if test=".//org:map">
            <div class="nav-group nav-group-reading-maps">
              <div class="toc-heading">
                <span class="heading-left" onclick="this.closest('.nav-group').classList.toggle('collapsed')"><svg class="chevron" viewBox="0 0 10 10"><path d="M3 1l4 4-4 4" fill="none" stroke="currentColor" stroke-width="1.5"/></svg>Reading Maps</span>
              </div>
              <ul>
                <xsl:for-each select=".//org:map">
                  <li>
                    <a href="#{@id}"><xsl:value-of select="org:title"/></a>
                  </li>
                </xsl:for-each>
              </ul>
            </div>
          </xsl:if>

          <!-- Sections grouped by organization type -->
          <xsl:variable name="all-concepts" select=".//org:concept/@ref"/>
          <xsl:variable name="all-tasks" select=".//org:task/@ref"/>
          <xsl:variable name="all-references" select=".//org:reference/@ref"/>

          <!-- Concepts -->
          <xsl:variable name="concept-sections" select="pr:section[@id = $all-concepts]"/>
          <xsl:if test="$concept-sections">
            <div class="nav-group collapsed" data-sortable="true">
              <div class="toc-heading">
                <span class="heading-left" onclick="this.closest('.nav-group').classList.toggle('collapsed')"><svg class="chevron" viewBox="0 0 10 10"><path d="M3 1l4 4-4 4" fill="none" stroke="currentColor" stroke-width="1.5"/></svg>Concepts</span>
                <span class="sort-controls">
                  <button class="sort-btn active" data-sort="alpha" data-dir="asc" title="Sort alphabetically">A&#x2193;</button>
                  <button class="sort-btn" data-sort="conn" data-dir="desc" title="Sort by connectivity">&#x26A1;</button>
                </span>
              </div>
              <ul>
                <xsl:for-each select="$concept-sections">
                  <xsl:variable name="sid" select="@id"/>
                  <xsl:call-template name="nav-item">
                    <xsl:with-param name="section" select="."/>
                  </xsl:call-template>
                </xsl:for-each>
              </ul>
            </div>
          </xsl:if>

          <!-- Tasks -->
          <xsl:variable name="task-sections" select="pr:section[@id = $all-tasks]"/>
          <xsl:if test="$task-sections">
            <div class="nav-group collapsed" data-sortable="true">
              <div class="toc-heading">
                <span class="heading-left" onclick="this.closest('.nav-group').classList.toggle('collapsed')"><svg class="chevron" viewBox="0 0 10 10"><path d="M3 1l4 4-4 4" fill="none" stroke="currentColor" stroke-width="1.5"/></svg>Tasks</span>
                <span class="sort-controls">
                  <button class="sort-btn active" data-sort="alpha" data-dir="asc" title="Sort alphabetically">A&#x2193;</button>
                  <button class="sort-btn" data-sort="conn" data-dir="desc" title="Sort by connectivity">&#x26A1;</button>
                </span>
              </div>
              <ul>
                <xsl:for-each select="$task-sections">
                  <xsl:variable name="sid" select="@id"/>
                  <xsl:call-template name="nav-item">
                    <xsl:with-param name="section" select="."/>
                  </xsl:call-template>
                </xsl:for-each>
              </ul>
            </div>
          </xsl:if>

          <!-- References -->
          <xsl:variable name="ref-sections" select="pr:section[@id = $all-references]"/>
          <xsl:if test="$ref-sections">
            <div class="nav-group collapsed" data-sortable="true">
              <div class="toc-heading">
                <span class="heading-left" onclick="this.closest('.nav-group').classList.toggle('collapsed')"><svg class="chevron" viewBox="0 0 10 10"><path d="M3 1l4 4-4 4" fill="none" stroke="currentColor" stroke-width="1.5"/></svg>Reference</span>
                <span class="sort-controls">
                  <button class="sort-btn active" data-sort="alpha" data-dir="asc" title="Sort alphabetically">A&#x2193;</button>
                  <button class="sort-btn" data-sort="conn" data-dir="desc" title="Sort by connectivity">&#x26A1;</button>
                </span>
              </div>
              <ul>
                <xsl:for-each select="$ref-sections">
                  <xsl:variable name="sid" select="@id"/>
                  <xsl:call-template name="nav-item">
                    <xsl:with-param name="section" select="."/>
                  </xsl:call-template>
                </xsl:for-each>
              </ul>
            </div>
          </xsl:if>

          <!-- Untyped sections -->
          <xsl:variable name="typed-ids" select="($all-concepts, $all-tasks, $all-references)"/>
          <xsl:variable name="untyped-sections" select="pr:section[not(@id = $typed-ids)]"/>
          <xsl:if test="$untyped-sections">
            <div class="nav-group collapsed" data-sortable="true">
              <div class="toc-heading">
                <span class="heading-left" onclick="this.closest('.nav-group').classList.toggle('collapsed')"><svg class="chevron" viewBox="0 0 10 10"><path d="M3 1l4 4-4 4" fill="none" stroke="currentColor" stroke-width="1.5"/></svg>Sections</span>
                <span class="sort-controls">
                  <button class="sort-btn active" data-sort="alpha" data-dir="asc" title="Sort alphabetically">A&#x2193;</button>
                  <button class="sort-btn" data-sort="conn" data-dir="desc" title="Sort by connectivity">&#x26A1;</button>
                </span>
              </div>
              <ul>
                <xsl:for-each select="$untyped-sections">
                  <xsl:variable name="sid" select="@id"/>
                  <xsl:call-template name="nav-item">
                    <xsl:with-param name="section" select="."/>
                  </xsl:call-template>
                </xsl:for-each>
              </ul>
            </div>
          </xsl:if>

          <!-- Terminology -->
          <xsl:if test=".//trm:term">
            <div class="nav-group collapsed">
              <div class="toc-heading">
                <span class="heading-left" onclick="this.closest('.nav-group').classList.toggle('collapsed')"><svg class="chevron" viewBox="0 0 10 10"><path d="M3 1l4 4-4 4" fill="none" stroke="currentColor" stroke-width="1.5"/></svg>Terminology</span>
              </div>
              <ul>
                <li>
                  <a href="#glossary">Glossary (<xsl:value-of select="count(.//trm:term)"/>)</a>
                  <ul>
                    <xsl:for-each select=".//trm:term">
                      <xsl:sort select="trm:name"/>
                      <li><a href="#{@id}"><xsl:value-of select="trm:name"/></a></li>
                    </xsl:for-each>
                  </ul>
                </li>
              </ul>
            </div>
          </xsl:if>

          <!-- Decisions -->
          <xsl:if test=".//dec:decision">
            <div class="nav-group collapsed">
              <div class="toc-heading">
                <span class="heading-left" onclick="this.closest('.nav-group').classList.toggle('collapsed')"><svg class="chevron" viewBox="0 0 10 10"><path d="M3 1l4 4-4 4" fill="none" stroke="currentColor" stroke-width="1.5"/></svg>Decisions</span>
              </div>
              <ul>
                <xsl:for-each select=".//dec:decision">
                  <li>
                    <a href="#{@id}">
                      <xsl:variable name="ref" select="@ref"/>
                      <xsl:variable name="title" select="ancestor::cmb:spec//pr:section[@id = $ref]/pr:title"/>
                      <xsl:choose>
                        <xsl:when test="$title"><xsl:value-of select="$title"/></xsl:when>
                        <xsl:otherwise><xsl:value-of select="@id"/></xsl:otherwise>
                      </xsl:choose>
                      <xsl:text> </xsl:text>
                      <span class="badge badge-{if (dec:status = 'accepted') then 'green' else if (dec:status = 'proposed') then 'yellow' else 'gray'}" style="font-size:0.6rem;">
                        <xsl:value-of select="dec:status"/>
                      </span>
                    </a>
                  </li>
                </xsl:for-each>
              </ul>
            </div>
          </xsl:if>

          <!-- Plans -->
          <xsl:if test=".//pln:plan">
            <div class="nav-group collapsed">
              <div class="toc-heading">
                <span class="heading-left" onclick="this.closest('.nav-group').classList.toggle('collapsed')"><svg class="chevron" viewBox="0 0 10 10"><path d="M3 1l4 4-4 4" fill="none" stroke="currentColor" stroke-width="1.5"/></svg>Plans</span>
              </div>
              <ul>
                <xsl:for-each select=".//pln:plan">
                  <li>
                    <a href="#{@id}">
                      <xsl:value-of select="pln:title"/>
                      <xsl:text> </xsl:text>
                      <span class="badge badge-{if (pln:status = 'completed' or pln:status = 'done') then 'green' else if (pln:status = 'active' or pln:status = 'in-progress') then 'yellow' else 'blue'}" style="font-size:0.6rem;">
                        <xsl:value-of select="pln:status"/>
                      </span>
                    </a>
                  </li>
                </xsl:for-each>
              </ul>
            </div>
          </xsl:if>

          <!-- Sources -->
          <xsl:if test=".//src:source">
            <div class="nav-group collapsed">
              <div class="toc-heading">
                <span class="heading-left" onclick="this.closest('.nav-group').classList.toggle('collapsed')"><svg class="chevron" viewBox="0 0 10 10"><path d="M3 1l4 4-4 4" fill="none" stroke="currentColor" stroke-width="1.5"/></svg>Sources</span>
              </div>
              <ul>
                <xsl:for-each select=".//src:source">
                  <li>
                    <a href="#{@id}">
                      <xsl:choose>
                        <xsl:when test="src:title"><xsl:value-of select="src:title"/></xsl:when>
                        <xsl:otherwise><xsl:value-of select="@id"/></xsl:otherwise>
                      </xsl:choose>
                    </a>
                  </li>
                </xsl:for-each>
              </ul>
            </div>
          </xsl:if>

          <!-- Artifacts -->
          <xsl:if test=".//art:mapping">
            <div class="nav-group collapsed">
              <div class="toc-heading">
                <span class="heading-left" onclick="this.closest('.nav-group').classList.toggle('collapsed')"><svg class="chevron" viewBox="0 0 10 10"><path d="M3 1l4 4-4 4" fill="none" stroke="currentColor" stroke-width="1.5"/></svg>Artifacts</span>
              </div>
              <ul>
                <li>
                  <a href="#artifacts">Mappings (<xsl:value-of select="count(.//art:mapping)"/>)</a>
                  <ul>
                    <xsl:for-each select=".//art:mapping">
                      <xsl:variable name="mid" select="@id"/>
                      <xsl:variable name="adrift" select="ancestor::cmb:spec//doc:drift[@mapping = $mid and (@status = 'spec-drifted' or @status = 'artifact-drifted')]"/>
                      <li>
                        <a href="#{@id}">
                          <span>
                            <xsl:if test="$adrift">
                              <span class="drift-dot {if ($adrift/@status = 'spec-drifted') then 'drift-spec' else 'drift-artifact'}" title="{$adrift/@status}">&#x25CF;</span>
                              <xsl:text> </xsl:text>
                            </xsl:if>
                            <xsl:value-of select="@id"/>
                          </span>
                        </a>
                      </li>
                    </xsl:for-each>
                  </ul>
                </li>
              </ul>
            </div>
          </xsl:if>
        </nav>

        <div id="xml-source-modal" class="xml-source-modal" role="dialog" aria-modal="true" aria-hidden="true" aria-labelledby="xml-source-title">
          <div class="xml-source-backdrop" data-xml-close="true"></div>
          <div class="xml-source-window">
            <div class="xml-source-header">
              <div id="xml-source-title" class="xml-source-title">XML source</div>
              <button id="xml-source-copy" class="xml-source-action" type="button" title="Copy XML" aria-label="Copy XML">
                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>
              </button>
              <button class="xml-source-action" type="button" data-xml-close="true" title="Close" aria-label="Close XML source">
                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>
              </button>
            </div>
            <div id="xml-source-browser" class="xml-source-browser"></div>
          </div>
        </div>

        <div class="content-wrapper">
        <main>
          <div id="breadcrumbs" class="breadcrumbs"></div>

          <!-- Revision metadata banner -->
          <xsl:if test=".//rev:revision">
            <div class="revision-banner">
              <xsl:for-each select=".//rev:revision">
                <span class="badge badge-gray"><xsl:value-of select="(@name, @id)[1]"/></span>
              </xsl:for-each>
            </div>
          </xsl:if>

          <!-- Each top-level prose section is its own page -->
          <xsl:for-each select="pr:section">
            <div class="page" data-page="{@id}">
              <xsl:apply-templates select="."/>
            </div>
          </xsl:for-each>

          <!-- Glossary -->
          <xsl:if test=".//trm:term">
            <div class="page" data-page="glossary">
              <h2 id="glossary">Glossary</h2>
              <xsl:for-each select=".//trm:term">
                <xsl:sort select="trm:name"/>
                <xsl:apply-templates select="." mode="glossary"/>
              </xsl:for-each>
            </div>
          </xsl:if>

          <!-- Each decision is its own page -->
          <xsl:for-each select=".//dec:decision">
            <div class="page" data-page="{@id}">
              <xsl:apply-templates select="."/>
            </div>
          </xsl:for-each>

          <!-- Each plan is its own page -->
          <xsl:for-each select=".//pln:plan">
            <div class="page" data-page="{@id}">
              <xsl:apply-templates select="."/>
            </div>
          </xsl:for-each>

          <!-- Each reading map is its own page -->
          <xsl:for-each select=".//org:map">
            <div class="page" data-page="{@id}">
              <xsl:apply-templates select="."/>
            </div>
          </xsl:for-each>

          <!-- Sources -->
          <xsl:if test=".//src:source">
            <div class="page" data-page="sources">
              <h2 id="sources">Sources / Bibliography</h2>
              <xsl:apply-templates select=".//src:source" mode="bibliography"/>
            </div>
          </xsl:if>

          <!-- Artifacts -->
          <xsl:if test=".//art:mapping">
            <div class="page" data-page="artifacts">
              <h2 id="artifacts">Artifact Mappings</h2>
              <xsl:apply-templates select=".//art:mapping"/>
            </div>
          </xsl:if>
        </main>

        <div class="panel-divider" id="panel-divider"></div>
        <div id="graph-panel">
          <div class="graph-toolbar">
            <input id="graph-search" type="search" placeholder="Filter nodes..." autocomplete="off"/>
            <span class="graph-btn-group" id="graph-layout-group">
              <button class="graph-tb active" data-layout="elk-layered" title="ELK Layered: directed graph with layers">
                <svg viewBox="0 0 16 16" width="14" height="14"><path d="M8 1v4M4 9v4M12 9v4M8 5l-4 4M8 5l4 4" stroke="currentColor" fill="none" stroke-width="1.5" stroke-linecap="round"/><circle cx="8" cy="2" r="1.5" fill="currentColor"/><circle cx="4" cy="12" r="1.5" fill="currentColor"/><circle cx="12" cy="12" r="1.5" fill="currentColor"/></svg>
              </button>
              <button class="graph-tb" data-layout="elk-force" title="ELK Force: organic clustering">
                <svg viewBox="0 0 16 16" width="14" height="14"><circle cx="5" cy="5" r="1.5" fill="currentColor"/><circle cx="11" cy="4" r="1.5" fill="currentColor"/><circle cx="4" cy="11" r="1.5" fill="currentColor"/><circle cx="12" cy="10" r="1.5" fill="currentColor"/><circle cx="8" cy="8" r="1.5" fill="currentColor"/><path d="M5 5l3 3M11 4l-3 4M4 11l4-3M12 10l-4-2" stroke="currentColor" fill="none" stroke-width="1" opacity="0.5"/></svg>
              </button>
              <button class="graph-tb" data-layout="hierarchical" title="Dagre: top-down hierarchy">
                <svg viewBox="0 0 16 16" width="14" height="14"><rect x="5" y="1" width="6" height="3" rx="1" fill="currentColor"/><rect x="1" y="7" width="5" height="3" rx="1" fill="currentColor"/><rect x="10" y="7" width="5" height="3" rx="1" fill="currentColor"/><path d="M8 4v1.5M3.5 7V5.5H8M12.5 7V5.5H8" stroke="currentColor" fill="none" stroke-width="1"/></svg>
              </button>
              <button class="graph-tb" data-layout="force" title="Force-directed: spring simulation">
                <svg viewBox="0 0 16 16" width="14" height="14"><circle cx="3" cy="8" r="1.5" fill="currentColor"/><circle cx="13" cy="4" r="1.5" fill="currentColor"/><circle cx="8" cy="13" r="1.5" fill="currentColor"/><circle cx="10" cy="9" r="1.5" fill="currentColor"/><path d="M3 8l7 1M13 4l-3 5M10 9l-2 4" stroke="currentColor" fill="none" stroke-width="1" stroke-dasharray="2 2"/></svg>
              </button>
              <button class="graph-tb" data-layout="grid" title="Grid: grouped by type">
                <svg viewBox="0 0 16 16" width="14" height="14"><rect x="1" y="1" width="4" height="4" rx="0.5" fill="currentColor" opacity="0.7"/><rect x="6" y="1" width="4" height="4" rx="0.5" fill="currentColor" opacity="0.7"/><rect x="11" y="1" width="4" height="4" rx="0.5" fill="currentColor" opacity="0.7"/><rect x="1" y="6" width="4" height="4" rx="0.5" fill="currentColor" opacity="0.5"/><rect x="6" y="6" width="4" height="4" rx="0.5" fill="currentColor" opacity="0.5"/><rect x="1" y="11" width="4" height="4" rx="0.5" fill="currentColor" opacity="0.3"/></svg>
              </button>
            </span>
            <span class="graph-btn-group" id="graph-scope-group">
              <button class="graph-tb active" data-scope="full" title="Full graph: all nodes">
                <svg viewBox="0 0 16 16" width="14" height="14"><circle cx="4" cy="4" r="2" fill="currentColor"/><circle cx="12" cy="4" r="2" fill="currentColor"/><circle cx="4" cy="12" r="2" fill="currentColor"/><circle cx="12" cy="12" r="2" fill="currentColor"/><path d="M6 4h4M4 6v4M12 6v4M6 12h4" stroke="currentColor" fill="none" stroke-width="1"/></svg>
              </button>
              <button class="graph-tb" data-scope="neighborhood" title="Neighborhood: current node + connections">
                <svg viewBox="0 0 16 16" width="14" height="14"><circle cx="8" cy="8" r="2.5" fill="currentColor"/><circle cx="3" cy="4" r="1.5" fill="currentColor" opacity="0.5"/><circle cx="13" cy="4" r="1.5" fill="currentColor" opacity="0.5"/><circle cx="3" cy="12" r="1.5" fill="currentColor" opacity="0.5"/><circle cx="13" cy="12" r="1.5" fill="currentColor" opacity="0.5"/><path d="M6 7l-2-2M10 7l2-2M6 9l-2 2M10 9l2 2" stroke="currentColor" fill="none" stroke-width="1"/></svg>
              </button>
            </span>
            <button class="graph-tb" id="graph-fit" title="Fit to view">
              <svg viewBox="0 0 16 16" width="14" height="14"><path d="M2 5V2h3M11 2h3v3M14 11v3h-3M5 14H2v-3" stroke="currentColor" fill="none" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>
            </button>
            <button class="graph-tb" onclick="toggleGraphPanel()" title="Close graph panel">
              <svg viewBox="0 0 16 16" width="14" height="14"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" fill="none" stroke-width="1.5" stroke-linecap="round"/></svg>
            </button>
          </div>
          <div id="graph-container"></div>
        </div>
        </div><!-- /content-wrapper -->

        <xsl:call-template name="emit-graph-data"/>

        <script>
          <xsl:text>
function toggleTheme() {
  var html = document.documentElement;
  var current = html.getAttribute('data-theme');
  var next = current === 'dark' ? 'light' : 'dark';
  html.setAttribute('data-theme', next);
  try { localStorage.setItem('theme', next); } catch(e) {}
  syncHljsTheme(next);
}
function syncHljsTheme(theme) {
  var light = document.getElementById('hljs-light');
  var dark = document.getElementById('hljs-dark');
  if (!light || !dark) return;
  if (theme === 'dark') {
    light.disabled = true; dark.disabled = false;
  } else {
    light.disabled = false; dark.disabled = true;
  }
}

function closeXmlSource() {
  var modal = document.getElementById('xml-source-modal');
  if (!modal) return;
  modal.classList.remove('open');
  modal.setAttribute('aria-hidden', 'true');
  document.body.classList.remove('xml-modal-open');
}

function copyTextFallback(text) {
  var area = document.createElement('textarea');
  area.value = text;
  area.setAttribute('readonly', 'readonly');
  area.style.position = 'fixed';
  area.style.left = '-9999px';
  document.body.appendChild(area);
  area.select();
  try { document.execCommand('copy'); } catch(e) {}
  document.body.removeChild(area);
}

function xmlPart(className, text) {
  var span = document.createElement('span');
  span.className = className;
  span.textContent = text;
  return span;
}

function xmlVisibleChildren(node) {
  return Array.prototype.filter.call(node.childNodes || [], function(child) {
    return child.nodeType !== 3 || child.nodeValue.trim();
  });
}

function appendXmlName(line, node) {
  var lt = String.fromCharCode(60);
  line.appendChild(xmlPart('xml-source-punct', lt));
  line.appendChild(xmlPart('xml-source-name', node.nodeName));
}

function appendXmlAttribute(line, attr) {
  line.appendChild(xmlPart('xml-source-text', ' '));
  line.appendChild(xmlPart('xml-source-attr-name', attr.name));
  line.appendChild(xmlPart('xml-source-punct', '="'));
  line.appendChild(xmlPart('xml-source-attr-value', attr.value));
  line.appendChild(xmlPart('xml-source-punct', '"'));
}

function appendXmlOpen(line, node, selfClosing) {
  var gt = String.fromCharCode(62);
  appendXmlName(line, node);
  Array.prototype.forEach.call(node.attributes || [], function(attr) {
    appendXmlAttribute(line, attr);
  });
  line.appendChild(xmlPart('xml-source-punct', selfClosing ? '/' + gt : gt));
}

function xmlAttributeWidth(node) {
  return Array.prototype.reduce.call(node.attributes || [], function(total, attr) {
    return total + attr.name.length + attr.value.length + 4;
  }, node.nodeName.length + 2);
}

function xmlShouldWrapAttributes(node) {
  var attrs = node.attributes || [];
  return attrs.length > 3 || xmlAttributeWidth(node) > 96;
}

function renderXmlLine() {
  var line = document.createElement('div');
  line.className = 'xml-source-line';
  return line;
}

function xmlTextValue(node) {
  var text = node.nodeValue || '';
  var parentName = node.parentNode ? node.parentNode.localName || node.parentNode.nodeName : '';
  if (parentName === 'codeblock' || parentName === 'synopsis') return text.trim();
  return text.replace(/\s+/g, ' ').trim();
}

function renderXmlNode(node) {
  var lt = String.fromCharCode(60);
  var gt = String.fromCharCode(62);

  if (node.nodeType === 1) {
    var wrapper = document.createElement('div');
    wrapper.className = 'xml-source-node';
    var children = xmlVisibleChildren(node);
    var hasChildren = children.length > 0;
    var line = renderXmlLine();

    if (hasChildren) {
      var toggle = document.createElement('button');
      toggle.className = 'xml-source-toggle';
      toggle.type = 'button';
      toggle.textContent = '-';
      toggle.setAttribute('aria-label', 'Collapse ' + node.nodeName);
      line.appendChild(toggle);
      toggle.addEventListener('click', function(e) {
        e.stopPropagation();
        var collapsed = wrapper.classList.toggle('collapsed');
        toggle.textContent = collapsed ? '+' : '-';
        toggle.setAttribute('aria-label', (collapsed ? 'Expand ' : 'Collapse ') + node.nodeName);
      });
    } else {
      var spacer = document.createElement('span');
      spacer.className = 'xml-source-spacer';
      line.appendChild(spacer);
    }

    appendXmlOpen(line, node, !hasChildren);
    wrapper.appendChild(line);

    if (node.attributes &amp;&amp; node.attributes.length &amp;&amp; xmlShouldWrapAttributes(node)) {
      line.innerHTML = '';
      if (hasChildren) {
        var multilineToggle = document.createElement('button');
        multilineToggle.className = 'xml-source-toggle';
        multilineToggle.type = 'button';
        multilineToggle.textContent = '-';
        multilineToggle.setAttribute('aria-label', 'Collapse ' + node.nodeName);
        line.appendChild(multilineToggle);
        toggle = multilineToggle;
        toggle.addEventListener('click', function(e) {
          e.stopPropagation();
          var collapsed = wrapper.classList.toggle('collapsed');
          toggle.textContent = collapsed ? '+' : '-';
          toggle.setAttribute('aria-label', (collapsed ? 'Expand ' : 'Collapse ') + node.nodeName);
        });
      } else {
        var multilineSpacer = document.createElement('span');
        multilineSpacer.className = 'xml-source-spacer';
        line.appendChild(multilineSpacer);
      }
      appendXmlName(line, node);
      Array.prototype.forEach.call(node.attributes, function(attr) {
        var attrLine = renderXmlLine();
        attrLine.classList.add('xml-source-attr-line');
        var attrSpacer = document.createElement('span');
        attrSpacer.className = 'xml-source-spacer';
        attrLine.appendChild(attrSpacer);
        appendXmlAttribute(attrLine, attr);
        wrapper.appendChild(attrLine);
      });
      var endLine = renderXmlLine();
      endLine.classList.add('xml-source-attr-line');
      var endSpacer = document.createElement('span');
      endSpacer.className = 'xml-source-spacer';
      endLine.appendChild(endSpacer);
      endLine.appendChild(xmlPart('xml-source-punct', (hasChildren ? '' : '/') + gt));
      wrapper.appendChild(endLine);
    }

    if (hasChildren) {
      var childBox = document.createElement('div');
      childBox.className = 'xml-source-children';
      children.forEach(function(child) {
        var rendered = renderXmlNode(child);
        if (rendered) childBox.appendChild(rendered);
      });
      wrapper.appendChild(childBox);

      var closeLine = renderXmlLine();
      var closeSpacer = document.createElement('span');
      closeSpacer.className = 'xml-source-spacer';
      closeLine.appendChild(closeSpacer);
      closeLine.appendChild(xmlPart('xml-source-punct', lt + '/'));
      closeLine.appendChild(xmlPart('xml-source-name', node.nodeName));
      closeLine.appendChild(xmlPart('xml-source-punct', gt));
      closeLine.classList.add('xml-source-close');
      wrapper.appendChild(closeLine);
    }

    return wrapper;
  }

  if (node.nodeType === 3) {
    var text = xmlTextValue(node);
    if (!text) return null;
    var textLine = renderXmlLine();
    textLine.classList.add('xml-source-text-line');
    var textSpacer = document.createElement('span');
    textSpacer.className = 'xml-source-spacer';
    textLine.appendChild(textSpacer);
    textLine.appendChild(xmlPart('xml-source-text', text));
    return textLine;
  }

  if (node.nodeType === 4) {
    var cdataLine = renderXmlLine();
    var cdataSpacer = document.createElement('span');
    cdataSpacer.className = 'xml-source-spacer';
    cdataLine.appendChild(cdataSpacer);
    cdataLine.appendChild(xmlPart('xml-source-text', lt + '![CDATA[' + node.nodeValue + ']]' + gt));
    return cdataLine;
  }

  if (node.nodeType === 8) {
    var commentLine = renderXmlLine();
    var commentSpacer = document.createElement('span');
    commentSpacer.className = 'xml-source-spacer';
    commentLine.appendChild(commentSpacer);
    commentLine.appendChild(xmlPart('xml-source-comment', lt + '!--' + node.nodeValue + '--' + gt));
    return commentLine;
  }

  if (node.nodeType === 7) {
    var piLine = renderXmlLine();
    var piSpacer = document.createElement('span');
    piSpacer.className = 'xml-source-spacer';
    piLine.appendChild(piSpacer);
    piLine.appendChild(xmlPart('xml-source-comment', lt + '?' + node.nodeName + ' ' + node.nodeValue + '?' + gt));
    return piLine;
  }

  return null;
}

function renderXmlBrowser(container, source) {
  container.innerHTML = '';
  if (!source) {
    container.appendChild(xmlPart('xml-source-error', 'No XML source is available for this section.'));
    return;
  }

  var parser = new DOMParser();
  var doc = parser.parseFromString(source, 'application/xml');
  var parserError = doc.getElementsByTagName('parsererror')[0];
  if (parserError || !doc.documentElement) {
    var error = document.createElement('div');
    error.className = 'xml-source-error';
    error.textContent = 'XML parse error; showing raw source.';
    container.appendChild(error);
    var raw = document.createElement('pre');
    raw.className = 'xml-source-raw';
    raw.textContent = source;
    container.appendChild(raw);
    return;
  }

  container.appendChild(renderXmlNode(doc.documentElement));
}

function openXmlSource(id, title) {
  var template = document.getElementById('xml-source-' + id);
  var modal = document.getElementById('xml-source-modal');
  var browser = document.getElementById('xml-source-browser');
  var heading = document.getElementById('xml-source-title');
  if (!template || !modal || !browser || !heading) return;
  heading.textContent = (title || id || 'Node') + ' XML';
  var source = template.content ? template.content.textContent : template.textContent;
  source = (source || '').trim();
  modal.setAttribute('data-xml-source-text', source);
  renderXmlBrowser(browser, source);
  modal.classList.add('open');
  modal.setAttribute('aria-hidden', 'false');
  document.body.classList.add('xml-modal-open');
  var close = modal.querySelector('[data-xml-close="true"][type="button"]');
  if (close) close.focus();
}

function updateBreadcrumbs(pageId) {
  var bc = document.getElementById('breadcrumbs');
  if (!bc) return;
  bc.innerHTML = '';
  var link = document.querySelector('#sidebar a[href="#' + pageId + '"]');
  if (!link) return;
  var group = link.closest('.nav-group');
  if (group) {
    var heading = group.querySelector('.heading-left');
    if (heading) {
      var catName = heading.textContent.trim();
      var span = document.createElement('span');
      span.textContent = catName;
      bc.appendChild(span);
      var sep = document.createElement('span');
      sep.className = 'sep';
      sep.textContent = '/';
      bc.appendChild(sep);
    }
  }
  var current = document.createElement('span');
  current.style.color = 'hsl(var(--foreground))';
  current.textContent = link.textContent.trim();
  bc.appendChild(current);
}

var activeNavTargetId = null;

function sidebarLinkForId(id) {
  if (!id) return null;
  var links = document.querySelectorAll('#sidebar a[href^="#"]');
  for (var i = 0; i &lt; links.length; i++) {
    if (links[i].getAttribute('href') === '#' + id) return links[i];
  }
  return null;
}

function pageForTargetId(id) {
  if (!id) return '';
  var page = document.querySelector('.page[data-page="' + id + '"]');
  if (page) return id;
  var el = document.getElementById(id);
  var parentPage = el ? el.closest('.page') : null;
  return parentPage ? parentPage.getAttribute('data-page') : '';
}

function focusSidebarGroupForLink(link) {
  var groups = document.querySelectorAll('#sidebar .nav-group');
  groups.forEach(function(group) { group.classList.add('collapsed'); });
  if (link) {
    var group = link.closest('.nav-group');
    if (group) group.classList.remove('collapsed');
  }
}

function focusDefaultReadingMapsSidebar() {
  var readingMaps = document.querySelector('#sidebar .nav-group-reading-maps');
  var groups = document.querySelectorAll('#sidebar .nav-group');
  groups.forEach(function(group) {
    group.classList.toggle('collapsed', group !== readingMaps);
  });
}

function setSidebarActive(targetId, pageId) {
  document.querySelectorAll('#sidebar a').forEach(function(a) {
    a.classList.remove('active');
  });
  var link = sidebarLinkForId(targetId) || sidebarLinkForId(pageId);
  if (link) link.classList.add('active');
  focusSidebarGroupForLink(link);
}

function scrollToTarget(targetId, smooth) {
  if (!targetId) return;
  var el = document.getElementById(targetId);
  if (!el || el.classList.contains('page')) return;
  setTimeout(function() {
    el.scrollIntoView({ behavior: smooth ? 'smooth' : 'auto' });
    el.classList.add('highlight-flash');
    setTimeout(function() { el.classList.remove('highlight-flash'); }, 1600);
  }, 50);
}

function showPage(pageId, pushHistory, targetId) {
  targetId = targetId || pageId;
  activeNavTargetId = targetId;
  document.querySelectorAll('.page').forEach(function(p) {
    p.classList.remove('active');
  });
  var target = document.querySelector('.page[data-page="' + pageId + '"]');
  if (target) {
    target.classList.add('active');
    window.scrollTo(0, 0);
  }
  setSidebarActive(targetId, pageId);
  if (pushHistory !== false) {
    history.pushState({ page: pageId, target: targetId }, '', '#' + targetId);
  }
  updateBreadcrumbs(pageId);
  // Close mobile sidebar on navigation
  var sidebar = document.getElementById('sidebar');
  if (sidebar) sidebar.classList.remove('open');
}

window.addEventListener('popstate', function(e) {
  var pageId, targetId;
  if (e.state &amp;&amp; e.state.page) {
    pageId = e.state.page;
    targetId = e.state.target || pageId;
  } else {
    targetId = window.location.hash.substring(1);
    pageId = pageForTargetId(targetId);
  }
  if (pageId) {
    showPage(pageId, false, targetId);
    scrollToTarget(targetId, false);
  }
});

(function() {
  try {
    var saved = localStorage.getItem('theme');
    if (saved) {
      document.documentElement.setAttribute('data-theme', saved);
      syncHljsTheme(saved);
    } else if (window.matchMedia('(prefers-color-scheme: dark)').matches) {
      syncHljsTheme('dark');
    }
  } catch(e) {}

  function navigateTo(id) {
    // 1. Direct page match
    var page = document.querySelector('.page[data-page="' + id + '"]');
    if (page) {
      showPage(id, true, id);
      return;
    }
    // 2. Element inside a page (term in glossary, source in sources, etc.)
    var el = document.getElementById(id);
    if (el) {
      var parentPage = el.closest('.page');
      if (parentPage) {
        showPage(parentPage.getAttribute('data-page'), true, id);
        scrollToTarget(id, true);
      }
    }
  }

  // Intercept all internal anchor clicks (sidebar + content)
  document.addEventListener('click', function(e) {
    var xmlButton = e.target.closest('[data-xml-source]');
    if (xmlButton) {
      e.preventDefault();
      e.stopPropagation();
      openXmlSource(xmlButton.getAttribute('data-xml-source'), xmlButton.getAttribute('data-xml-title'));
      return;
    }

    var xmlClose = e.target.closest('[data-xml-close]');
    if (xmlClose) {
      e.preventDefault();
      closeXmlSource();
      return;
    }

    var link = e.target.closest('a[href^="#"]');
    if (!link) return;
    var id = link.getAttribute('href').substring(1);
    if (!id) return;
    e.preventDefault();
    navigateTo(id);
  });

  // Show initial page from hash or first page (don't push to history)
  var hash = window.location.hash.substring(1);
  var pages = document.querySelectorAll('.page');
  if (hash) {
    var initialPage = pageForTargetId(hash);
    if (initialPage) {
      showPage(initialPage, false, hash);
      scrollToTarget(hash, false);
    } else if (pages.length > 0) {
      showPage(pages[0].getAttribute('data-page'), false);
    }
  } else {
    var firstMapLink = document.querySelector('#sidebar .nav-group-reading-maps a[href^="#"]');
    if (firstMapLink) {
      var firstMapId = firstMapLink.getAttribute('href').substring(1);
      showPage(firstMapId, false, firstMapId);
      focusDefaultReadingMapsSidebar();
    } else if (pages.length > 0) {
      showPage(pages[0].getAttribute('data-page'), false);
    }
  }
  // Replace initial state so popstate has something to land on
  var initialState = {
    page: document.querySelector('.page.active')?.getAttribute('data-page') || '',
    target: activeNavTargetId || document.querySelector('.page.active')?.getAttribute('data-page') || ''
  };
  if (hash &amp;&amp; activeNavTargetId) {
    history.replaceState(initialState, '', '#' + activeNavTargetId);
  } else {
    history.replaceState(initialState, '');
  }

  // Build search index from all navigable nodes
  var searchData = [];
  document.querySelectorAll('[id]').forEach(function(el) {
    var id = el.getAttribute('id');
    if (!id || id === 'sidebar' || id === 'search' || id === 'search-results') return;
    if (id.indexOf('xml-source-') === 0) return;
    var label = '', type = '', content = '';
    var h = el.matches('h1,h2,h3,h4,h5,h6') ? el : el.querySelector('h1,h2,h3,h4,h5,h6');
    if (h) {
      label = h.textContent.trim();
      type = 'section';
      content = el.textContent.trim();
    } else if (el.classList.contains('card')) {
      var h4 = el.querySelector('h4');
      if (h4) label = h4.textContent.trim();
      content = el.textContent.trim();
      var page = el.closest('.page');
      if (page) {
        var pid = page.getAttribute('data-page');
        if (pid === 'glossary') type = 'term';
        else if (pid === 'sources') type = 'source';
        else if (pid === 'artifacts') type = 'artifact';
        else type = 'node';
      }
    }
    if (!label) {
      var text = el.textContent.trim().substring(0, 80);
      if (text) { label = text; type = type || 'node'; }
    }
    if (label &amp;&amp; type) {
      searchData.push({ id: id, label: label, type: type, content: content });
    }
  });

  var fuse = new Fuse(searchData, {
    keys: [{ name: 'label', weight: 2 }, { name: 'content', weight: 1 }],
    threshold: 0.4,
    includeMatches: true,
    minMatchCharLength: 2
  });

  var searchInput = document.getElementById('search');
  var searchResults = document.getElementById('search-results');
  var selectedIdx = -1;

  function highlightMatch(text, indices) {
    if (!indices || !indices.length) return document.createTextNode(text);
    var frag = document.createDocumentFragment();
    var last = 0;
    indices.forEach(function(pair) {
      if (pair[0] > last) frag.appendChild(document.createTextNode(text.substring(last, pair[0])));
      var mark = document.createElement('mark');
      mark.style.cssText = 'background:hsl(var(--ring)/0.25);color:inherit;border-radius:2px;padding:0 1px;';
      mark.textContent = text.substring(pair[0], pair[1] + 1);
      frag.appendChild(mark);
      last = pair[1] + 1;
    });
    if (last !== text.length) frag.appendChild(document.createTextNode(text.substring(last)));
    return frag;
  }

  function getSnippet(text, indices, around) {
    if (!indices || !indices.length) return null;
    around = around || 40;
    var first = indices[0];
    var start = Math.max(0, first[0] - around);
    var end = Math.min(text.length, first[1] + around + 1);
    var slice = (start > 0 ? '...' : '') + text.substring(start, end) + (end !== text.length ? '...' : '');
    var shifted = indices.filter(function(p) { return p[0] >= start &amp;&amp; p[1] &lt; end; })
      .map(function(p) { return [p[0] - start + (start > 0 ? 3 : 0), p[1] - start + (start > 0 ? 3 : 0)]; });
    return { text: slice, indices: shifted };
  }

  function renderResults(results) {
    searchResults.innerHTML = '';
    results.slice(0, 15).forEach(function(r) {
      var li = document.createElement('li');
      li.setAttribute('data-id', r.item.id);
      li.style.cssText = 'display:block;';

      var titleRow = document.createElement('div');
      titleRow.style.cssText = 'display:flex;align-items:center;justify-content:space-between;';

      // Title with highlights
      var titleSpan = document.createElement('span');
      var titleMatch = r.matches ? r.matches.find(function(m) { return m.key === 'label'; }) : null;
      if (titleMatch) {
        titleSpan.appendChild(highlightMatch(r.item.label, titleMatch.indices));
      } else {
        titleSpan.textContent = r.item.label;
      }
      titleRow.appendChild(titleSpan);

      var sp = document.createElement('span');
      sp.className = 'result-type';
      sp.textContent = r.item.type;
      titleRow.appendChild(sp);
      li.appendChild(titleRow);

      // Content snippet if matched
      var contentMatch = r.matches ? r.matches.find(function(m) { return m.key === 'content'; }) : null;
      if (contentMatch) {
        var snippet = getSnippet(r.item.content, contentMatch.indices, 40);
        if (snippet) {
          var preview = document.createElement('div');
          preview.style.cssText = 'font-size:0.7rem;color:hsl(var(--muted-foreground));margin-top:2px;line-height:1.3;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;';
          preview.appendChild(highlightMatch(snippet.text, snippet.indices));
          li.appendChild(preview);
        }
      }

      searchResults.appendChild(li);
    });
    var vis = results.length > 0;
    searchResults.classList.toggle('visible', vis);
    searchInput.classList.toggle('has-results', vis);
  }

  searchInput.addEventListener('input', function() {
    var q = this.value.trim();
    selectedIdx = -1;
    if (!q) { searchResults.classList.remove('visible'); searchInput.classList.remove('has-results'); searchResults.innerHTML = ''; return; }
    renderResults(fuse.search(q));
  });

  searchInput.addEventListener('keydown', function(e) {
    var items = searchResults.querySelectorAll('li');
    if (!items.length) return;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      selectedIdx = Math.min(selectedIdx + 1, items.length - 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      selectedIdx = Math.max(selectedIdx - 1, 0);
    } else if (e.key === 'Enter' &amp;&amp; selectedIdx >= 0) {
      e.preventDefault();
      navigateTo(items[selectedIdx].getAttribute('data-id'));
      searchInput.value = '';
      searchResults.classList.remove('visible'); searchInput.classList.remove('has-results');
      return;
    } else if (e.key === 'Escape') {
      searchInput.value = '';
      searchResults.classList.remove('visible'); searchInput.classList.remove('has-results');
      return;
    } else { return; }
    items.forEach(function(li, i) { li.classList.toggle('selected', i === selectedIdx); });
    items[selectedIdx].scrollIntoView({ block: 'nearest' });
  });

  searchResults.addEventListener('click', function(e) {
    var li = e.target.closest('li');
    if (!li) return;
    navigateTo(li.getAttribute('data-id'));
    searchInput.value = '';
    searchResults.classList.remove('visible'); searchInput.classList.remove('has-results');
  });

  // Keyboard shortcuts: / to focus search, Escape to blur
  document.addEventListener('keydown', function(e) {
    if (e.key === 'Escape' &amp;&amp; document.getElementById('xml-source-modal')?.classList.contains('open')) {
      e.preventDefault();
      closeXmlSource();
      return;
    }
    // Don't trigger shortcuts when typing in input
    if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') return;
    if (e.key === '/') {
      e.preventDefault();
      searchInput.focus();
    }
  });

  document.getElementById('xml-source-copy')?.addEventListener('click', function() {
    var modal = document.getElementById('xml-source-modal');
    var text = modal ? modal.getAttribute('data-xml-source-text') || '' : '';
    if (navigator.clipboard &amp;&amp; navigator.clipboard.writeText) {
      navigator.clipboard.writeText(text).catch(function() { copyTextFallback(text); });
    } else {
      copyTextFallback(text);
    }
  });

  // Nav group sorting
  document.querySelectorAll('.nav-group[data-sortable]').forEach(function(group) {
    var ul = group.querySelector('ul');
    var btns = group.querySelectorAll('.sort-btn');

    function sortList(mode, dir) {
      var items = Array.from(ul.children);
      items.sort(function(a, b) {
        var va, vb;
        if (mode === 'alpha') {
          va = (a.getAttribute('data-label') || '').toLowerCase();
          vb = (b.getAttribute('data-label') || '').toLowerCase();
          return dir === 'asc' ? va.localeCompare(vb) : vb.localeCompare(va);
        } else {
          va = parseInt(a.getAttribute('data-conn') || '0', 10);
          vb = parseInt(b.getAttribute('data-conn') || '0', 10);
          return dir === 'asc' ? va - vb : vb - va;
        }
      });
      items.forEach(function(li) { ul.appendChild(li); });
    }

    // Initial sort: alpha asc to match default button state
    sortList('alpha', 'asc');

    btns.forEach(function(btn) {
      btn.addEventListener('click', function(e) {
        e.stopPropagation();
        var mode = this.getAttribute('data-sort');
        var dir = this.getAttribute('data-dir');
        // If already active, toggle direction
        if (this.classList.contains('active')) {
          dir = dir === 'asc' ? 'desc' : 'asc';
          this.setAttribute('data-dir', dir);
        }
        btns.forEach(function(b) { b.classList.remove('active'); });
        this.classList.add('active');
        // Update button text and tooltip
        if (mode === 'alpha') {
          this.textContent = dir === 'asc' ? 'A\u2193' : 'A\u2191';
          this.title = dir === 'asc' ? 'Alphabetical A\u2192Z' : 'Alphabetical Z\u2192A';
        } else {
          this.textContent = dir === 'desc' ? '\u26A1\u2193' : '\u26A1\u2191';
          this.title = dir === 'desc' ? 'Most connected first' : 'Least connected first';
        }
        sortList(mode, dir);
      });
    });
  });

  // Expose navigateTo globally for graph JS (separate script block)
  window.navigateTo = navigateTo;
})();
          </xsl:text>
        </script>
        <script src="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.11.1/highlight.min.js"></script>
        <script>
          <xsl:text>
function rehighlight() {
  var active = document.querySelector('.page.active');
  if (!active) return;
  active.querySelectorAll('pre code:not([data-highlighted])').forEach(function(el) {
    // Re-set text content to auto-escape any raw HTML before highlighting
    el.textContent = el.textContent;
    hljs.highlightElement(el);
  });
}
rehighlight();
// Re-highlight when switching pages
var origShowPage = showPage;
showPage = function(id, push, target) { origShowPage(id, push, target); rehighlight(); };
          </xsl:text>
        </script>

        <!-- Graph visualization dependencies -->
        <script src="https://cdn.jsdelivr.net/npm/@joint/core@4/dist/joint.min.js"></script>
        <script src="https://cdn.jsdelivr.net/npm/dagre@0.8/dist/dagre.min.js"></script>
        <script src="https://cdn.jsdelivr.net/npm/elkjs@0.9/lib/elk.bundled.js"></script>

        <script>
          <xsl:text>
// --- Graph Visualization (Steps 4-11) ---

var clayersGraph = null;
var nodeMap = {};
var currentNodeId = null;
var graphInitialized = false;

// --- Node shape/color config ---
// --- Node design system ---
// Each type: gradient top/bottom, accent color, icon symbol, dimensions
var NODE_TYPES = {
  concept:   { top: '#818cf8', bot: '#6366f1', accent: '#4f46e5', icon: '\u25C6', w: 150, h: 44 },
  task:      { top: '#34d399', bot: '#10b981', accent: '#059669', icon: '\u2713', w: 150, h: 44 },
  reference: { top: '#94a3b8', bot: '#64748b', accent: '#475569', icon: '\u2261', w: 150, h: 44 },
  term:      { top: '#c084fc', bot: '#a855f7', accent: '#7c3aed', icon: 'T',      w: 130, h: 40 },
  decision:  { top: '#fbbf24', bot: '#f59e0b', accent: '#d97706', icon: '?',      w: 150, h: 44, dark: true },
  plan:      { top: '#22d3ee', bot: '#06b6d4', accent: '#0891b2', icon: '\u25B6', w: 150, h: 44 },
  source:    { top: '#7dd3fc', bot: '#38bdf8', accent: '#0284c7', icon: '\u2197', w: 130, h: 38 },
  map:       { top: '#5eead4', bot: '#2dd4bf', accent: '#0d9488', icon: '\u2637', w: 140, h: 42, dark: true },
  artifact:  { top: '#fdba74', bot: '#fb923c', accent: '#ea580c', icon: '\u2699', w: 120, h: 36, dark: true },
  section:   { top: '#93c5fd', bot: '#60a5fa', accent: '#2563eb', icon: '\u00A7', w: 150, h: 44 }
};

var EDGE_STYLES = {
  'depends-on':    { color: '#f59e0b', dash: '', width: 1.8 },
  'refines':       { color: '#818cf8', dash: '', width: 1.5 },
  'implements':    { color: '#34d399', dash: '', width: 1.5 },
  'precedes':      { color: '#c084fc', dash: '8 4', width: 1.5 },
  'constrains':    { color: '#f87171', dash: '', width: 1.8 },
  'references':    { color: '#94a3b8', dash: '4 4', width: 1.2 },
  'conflicts-with':{ color: '#f87171', dash: '6 4', width: 1.8 }
};

function getActiveLayout() {
  var btn = document.querySelector('#graph-layout-group .graph-tb.active');
  return btn ? btn.getAttribute('data-layout') : 'elk-layered';
}

function getActiveScope() {
  var btn = document.querySelector('#graph-scope-group .graph-tb.active');
  return btn ? btn.getAttribute('data-scope') : 'full';
}

function setActiveScope(scope) {
  var group = document.getElementById('graph-scope-group');
  if (!group) return;
  group.querySelectorAll('.graph-tb').forEach(function(b) {
    b.classList.toggle('active', b.getAttribute('data-scope') === scope);
  });
}

function getThemeColors() {
  var isDark = document.documentElement.getAttribute('data-theme') === 'dark';
  return {
    text: isDark ? '#e2e8f0' : '#1e293b',
    bg: isDark ? '#0f172a' : '#ffffff',
    nodeBg: isDark ? 0.8 : 1.0
  };
}

// Inject SVG defs for gradients and shadow filter
function injectSvgDefs(paper) {
  var svg = paper.svg;
  var defs = document.createElementNS('http://www.w3.org/2000/svg', 'defs');

  // Shadow filter
  defs.innerHTML = '&lt;filter id="node-shadow" x="-10%" y="-10%" width="130%" height="140%">' +
    '&lt;feDropShadow dx="0" dy="2" stdDeviation="3" flood-color="rgba(0,0,0,0.15)" flood-opacity="1"/>' +
    '&lt;/filter>' +
    '&lt;filter id="node-glow" x="-20%" y="-20%" width="140%" height="140%">' +
    '&lt;feDropShadow dx="0" dy="0" stdDeviation="4" flood-color="#f59e0b" flood-opacity="0.6"/>' +
    '&lt;/filter>';

  // Gradient for each node type
  Object.keys(NODE_TYPES).forEach(function(type) {
    var t = NODE_TYPES[type];
    var grad = document.createElementNS('http://www.w3.org/2000/svg', 'linearGradient');
    grad.id = 'grad-' + type;
    grad.setAttribute('x1', '0');
    grad.setAttribute('y1', '0');
    grad.setAttribute('x2', '0');
    grad.setAttribute('y2', '1');
    var s1 = document.createElementNS('http://www.w3.org/2000/svg', 'stop');
    s1.setAttribute('offset', '0%');
    s1.setAttribute('stop-color', t.top);
    var s2 = document.createElementNS('http://www.w3.org/2000/svg', 'stop');
    s2.setAttribute('offset', '100%');
    s2.setAttribute('stop-color', t.bot);
    grad.appendChild(s1);
    grad.appendChild(s2);
    defs.appendChild(grad);
  });

  svg.insertBefore(defs, svg.firstChild);
}

// Custom shape with accent bar, icon badge, and gradient body
var CLayersNode = joint.dia.Element.define('clayers.Node', {
  attrs: {
    body: {
      width: 'calc(w)',
      height: 'calc(h)',
      rx: 10, ry: 10,
      strokeWidth: 0,
      filter: 'url(#node-shadow)',
      cursor: 'pointer'
    },
    accent: {
      width: 4,
      height: 'calc(h - 8)',
      x: 3, y: 4,
      rx: 2, ry: 2,
      fill: '#4f46e5',
      strokeWidth: 0
    },
    icon: {
      x: 14,
      y: 'calc(h / 2)',
      textAnchor: 'middle',
      textVerticalAnchor: 'middle',
      fontSize: 12,
      fontFamily: 'Inter, system-ui, sans-serif',
      fontWeight: 700,
      fill: 'rgba(255,255,255,0.7)'
    },
    label: {
      x: 24,
      y: 'calc(h / 2)',
      textAnchor: 'start',
      textVerticalAnchor: 'middle',
      fontSize: 11,
      fontFamily: 'Inter, system-ui, sans-serif',
      fontWeight: 500,
      fill: '#ffffff',
      letterSpacing: '0.01em'
    }
  }
}, {
  markup: [
    { tagName: 'rect', selector: 'body' },
    { tagName: 'rect', selector: 'accent' },
    { tagName: 'text', selector: 'icon' },
    { tagName: 'text', selector: 'label' }
  ]
});

function createNodeElement(node) {
  var t = NODE_TYPES[node.type] || NODE_TYPES.section;
  var maxLen = Math.floor((t.w - 30) / 7);
  var label = node.label.length > maxLen ? node.label.substring(0, maxLen - 1) + '\u2026' : node.label;
  var textColor = t.dark ? 'rgba(0,0,0,0.85)' : '#ffffff';
  var accentAlpha = t.dark ? 'rgba(0,0,0,0.25)' : 'rgba(255,255,255,0.35)';

  var el = new CLayersNode();
  el.resize(t.w, t.h);
  el.attr('body/fill', 'url(#grad-' + node.type + ')');
  el.attr('accent/fill', accentAlpha);
  el.attr('icon/text', t.icon);
  el.attr('icon/fill', t.dark ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.55)');
  el.attr('label/text', label);
  el.attr('label/fill', textColor);

  el.prop('nodeId', node.id);
  el.prop('nodeType', node.type);
  el.prop('nodePage', node.page);

  return el;
}

function createLink(edge) {
  var link = new joint.shapes.standard.Link();
  var style = EDGE_STYLES[edge.type] || { color: '#94a3b8', dash: '', width: 1.2 };
  link.attr('line/stroke', style.color);
  link.attr('line/strokeWidth', style.width);
  link.attr('line/strokeOpacity', 0.7);
  link.attr('line/strokeLinecap', 'round');
  if (style.dash) link.attr('line/strokeDasharray', style.dash);
  link.attr('line/targetMarker/type', 'path');
  link.attr('line/targetMarker/d', 'M 10 -5 0 0 10 5 Z');
  link.attr('line/targetMarker/fill', style.color);
  link.attr('line/targetMarker/stroke', 'none');
  link.attr('line/targetMarker/opacity', 0.8);
  link.connector({ name: 'rounded' });
  return link;
}

function buildGraph(data, filterFn) {
  var graph = clayersGraph.graph;
  graph.clear();
  nodeMap = {};

  var nodes = filterFn ? data.nodes.filter(filterFn) : data.nodes;
  var nodeIds = {};
  nodes.forEach(function(n) { nodeIds[n.id] = true; });

  clayersGraph._edges = data.edges.filter(function(e) {
    return nodeIds[e.from] &amp;&amp; nodeIds[e.to];
  });

  // Add only nodes - links added after layout positions them
  nodes.forEach(function(n) {
    var el = createNodeElement(n);
    el.addTo(graph);
    nodeMap[n.id] = el;
  });
}

function addLinks() {
  var graph = clayersGraph.graph;
  var edges = clayersGraph._edges || [];
  // Remove any existing links
  graph.getLinks().forEach(function(l) { l.remove(); });
  edges.forEach(function(e) {
    var link = createLink(e);
    var src = nodeMap[e.from];
    var tgt = nodeMap[e.to];
    if (src &amp;&amp; tgt) {
      link.source({ id: src.id });
      link.target({ id: tgt.id });
      link.addTo(graph);
    }
  });
}

// --- Layout algorithms (Step 6) ---

function applyDagreLayout(graph, rankDir) {
  var edges = clayersGraph._edges || [];
  var elements = graph.getElements();

  // Find connected node IDs
  var connectedIds = {};
  edges.forEach(function(e) {
    var src = nodeMap[e.from];
    var tgt = nodeMap[e.to];
    if (src &amp;&amp; tgt) {
      connectedIds[src.id] = true;
      connectedIds[tgt.id] = true;
    }
  });

  // Split into connected and unconnected
  var connected = [];
  var unconnected = [];
  elements.forEach(function(el) {
    if (connectedIds[el.id]) connected.push(el);
    else unconnected.push(el);
  });

  // Layout connected nodes with dagre
  var maxY = 0;
  if (connected.length > 0) {
    var g = new dagre.graphlib.Graph();
    g.setGraph({ rankdir: rankDir, ranksep: 80, nodesep: 40, marginx: 20, marginy: 20 });
    g.setDefaultEdgeLabel(function() { return {}; });
    connected.forEach(function(el) {
      var size = el.size();
      g.setNode(el.id, { width: size.width, height: size.height });
    });
    edges.forEach(function(e) {
      var src = nodeMap[e.from];
      var tgt = nodeMap[e.to];
      if (src &amp;&amp; tgt &amp;&amp; connectedIds[src.id] &amp;&amp; connectedIds[tgt.id]) {
        g.setEdge(src.id, tgt.id);
      }
    });
    dagre.layout(g);
    g.nodes().forEach(function(id) {
      var node = g.node(id);
      var el = graph.getCell(id);
      if (el &amp;&amp; node) {
        el.position(node.x - node.width/2, node.y - node.height/2);
        var bottom = node.y + node.height/2;
        if (bottom > maxY) maxY = bottom;
      }
    });
  }

  // Place unconnected nodes in a compact grid below
  if (unconnected.length > 0) {
    var startY = maxY + 60;
    var x = 20, rowH = 0;
    unconnected.forEach(function(el) {
      var s = el.size();
      el.position(x, startY);
      x += s.width + 12;
      if (s.height > rowH) rowH = s.height;
      if (x > 1200) { x = 20; startY += rowH + 12; rowH = 0; }
    });
  }
}

function applyForceLayout(graph) {
  var elements = graph.getElements();
  var edges = clayersGraph._edges || [];
  if (!elements.length) return;

  var width = document.getElementById('graph-container').clientWidth || 800;
  var height = document.getElementById('graph-container').clientHeight || 600;
  var area = width * height;
  var k = Math.sqrt(area / elements.length) * 0.8;

  // Build element ID map (JointJS UUID -> our nodeId)
  var elById = {};
  elements.forEach(function(el) { elById[el.id] = el; });

  // Initialize random positions
  var pos = {};
  elements.forEach(function(el) {
    pos[el.id] = { x: Math.random() * width, y: Math.random() * height, dx: 0, dy: 0 };
  });

  // Build edge list using JointJS UUIDs
  var forceEdges = [];
  edges.forEach(function(e) {
    var src = nodeMap[e.from];
    var tgt = nodeMap[e.to];
    if (src &amp;&amp; tgt) forceEdges.push({ s: src.id, t: tgt.id });
  });

  // Fruchterman-Reingold iterations
  for (var iter = 0; iter &lt; 80; iter++) {
    var temp = k * (1 - iter / 80);
    var ids = Object.keys(pos);

    // Repulsive forces
    for (var i = 0; i &lt; ids.length; i++) {
      pos[ids[i]].dx = 0;
      pos[ids[i]].dy = 0;
      for (var j = 0; j &lt; ids.length; j++) {
        if (i === j) continue;
        var dx = pos[ids[i]].x - pos[ids[j]].x;
        var dy = pos[ids[i]].y - pos[ids[j]].y;
        var dist = Math.sqrt(dx*dx + dy*dy) || 1;
        var force = (k * k) / dist;
        pos[ids[i]].dx += (dx / dist) * force;
        pos[ids[i]].dy += (dy / dist) * force;
      }
    }

    // Attractive forces
    forceEdges.forEach(function(fe) {
      var s = fe.s, t = fe.t;
      if (!pos[s] || !pos[t]) return;
      var dx = pos[s].x - pos[t].x;
      var dy = pos[s].y - pos[t].y;
      var dist = Math.sqrt(dx*dx + dy*dy) || 1;
      var force = (dist * dist) / k;
      var fx = (dx / dist) * force;
      var fy = (dy / dist) * force;
      pos[s].dx -= fx; pos[s].dy -= fy;
      pos[t].dx += fx; pos[t].dy += fy;
    });

    // Apply with temperature
    ids.forEach(function(id) {
      var mag = Math.sqrt(pos[id].dx*pos[id].dx + pos[id].dy*pos[id].dy) || 1;
      pos[id].x += (pos[id].dx / mag) * Math.min(mag, temp);
      pos[id].y += (pos[id].dy / mag) * Math.min(mag, temp);
      pos[id].x = Math.max(20, Math.min(width - 20, pos[id].x));
      pos[id].y = Math.max(20, Math.min(height - 20, pos[id].y));
    });
  }

  elements.forEach(function(el) {
    if (pos[el.id]) {
      var s = el.size();
      el.position(pos[el.id].x - s.width/2, pos[el.id].y - s.height/2);
    }
  });
}

function applyGridLayout(graph) {
  var elements = graph.getElements();
  if (!elements.length) return;

  // Group by type
  var groups = {};
  elements.forEach(function(el) {
    var t = el.prop('nodeType') || 'section';
    if (!groups[t]) groups[t] = [];
    groups[t].push(el);
  });

  var y = 20;
  var typeOrder = ['concept', 'task', 'reference', 'term', 'decision', 'plan', 'map', 'source', 'artifact', 'section'];
  typeOrder.forEach(function(type) {
    if (!groups[type]) return;
    var x = 20;
    var maxH = 0;
    groups[type].forEach(function(el) {
      var s = el.size();
      el.position(x, y);
      x += s.width + 20;
      if (s.height > maxH) maxH = s.height;
      if (x > 900) { x = 20; y += maxH + 20; maxH = 0; }
    });
    y += maxH + 40;
  });
}

function applyElkLayout(graph, algorithm) {
  var elements = graph.getElements();
  var edges = clayersGraph._edges || [];
  if (!elements.length) return Promise.resolve();

  var elk = new ELK();
  var elkNodes = elements.map(function(el) {
    var s = el.size();
    return { id: el.id, width: s.width, height: s.height };
  });

  // Map our nodeIds to JointJS UUIDs for edge lookup
  var elkEdges = [];
  var edgeIdx = 0;
  edges.forEach(function(e) {
    var src = nodeMap[e.from];
    var tgt = nodeMap[e.to];
    if (src &amp;&amp; tgt) {
      elkEdges.push({ id: 'e' + (edgeIdx++), sources: [src.id], targets: [tgt.id] });
    }
  });

  var elkGraph = {
    id: 'root',
    layoutOptions: {
      'elk.algorithm': algorithm,
      'elk.spacing.nodeNode': '40',
      'elk.layered.spacing.nodeNodeBetweenLayers': '80',
      'elk.force.temperature': '0.1',
      'elk.separateConnectedComponents': 'true',
      'elk.spacing.componentComponent': '60'
    },
    children: elkNodes,
    edges: elkEdges
  };

  return elk.layout(elkGraph).then(function(result) {
    result.children.forEach(function(elkNode) {
      var el = graph.getCell(elkNode.id);
      if (el) el.position(elkNode.x, elkNode.y);
    });
  });
}

function applyLayout(type) {
  if (!clayersGraph) return;
  var graph = clayersGraph.graph;
  var container = document.getElementById('graph-container');

  // Hide during layout to prevent visual jump
  container.style.visibility = 'hidden';

  function finishLayout() {
    addLinks();
    clayersGraph.paper.scaleContentToFit({ padding: 40, minScale: 0.1, maxScale: 1.5 });
    container.style.visibility = '';
  }

  if (type === 'elk-layered' || type === 'elk-force') {
    var algo = type === 'elk-layered' ? 'layered' : 'force';
    applyElkLayout(graph, algo).then(finishLayout);
    return;
  }

  if (type === 'hierarchical') {
    applyDagreLayout(graph, 'TB');
  } else if (type === 'force') {
    applyForceLayout(graph);
  } else if (type === 'grid') {
    applyGridLayout(graph);
  }
  finishLayout();
}

// --- Scope switching (Step 9) ---

function showFullGraph() {
  if (!clayersGraph) return;
  buildGraph(clayersGraph.data, null);
  applyLayout(getActiveLayout());
}

function showNeighborhood(nodeId) {
  if (!clayersGraph || !nodeId) return;
  var data = clayersGraph.data;
  var neighbors = {};
  neighbors[nodeId] = true;
  data.edges.forEach(function(e) {
    if (e.from === nodeId) neighbors[e.to] = true;
    if (e.to === nodeId) neighbors[e.from] = true;
  });
  buildGraph(data, function(n) { return neighbors[n.id]; });
  applyLayout(getActiveLayout());
  // Highlight the center node
  if (nodeMap[nodeId]) {
    nodeMap[nodeId].attr('body/filter', 'url(#node-glow)');
  }
}

function updateGraphScope() {
  var scope = getActiveScope();
  if (scope === 'neighborhood') {
    showNeighborhood(currentNodeId);
  } else {
    showFullGraph();
  }
}

// --- Init (Step 4) ---

function initGraph() {
  if (graphInitialized) return;
  var dataEl = document.getElementById('clayers-graph-data');
  if (!dataEl) return;

  var data;
  try { data = JSON.parse(dataEl.textContent); } catch(e) { return; }

  graphInitialized = true;
  var container = document.getElementById('graph-container');

  // Defer to next frame so the panel has been laid out after removing display:none
  requestAnimationFrame(function() {
    var w = container.clientWidth || 600;
    var h = container.clientHeight || 500;
    _initGraphInner(data, container, w, h);
  });
}

function _initGraphInner(data, container, w, h) {
  var graph = new joint.dia.Graph();
  var paper = new joint.dia.Paper({
    el: container,
    model: graph,
    width: 20000,
    height: 20000,
    gridSize: 1,
    background: { color: 'transparent' },
    interactive: { linkMove: false, elementMove: true }
  });

  // Pan and zoom
  var dragState = null;
  paper.on('blank:pointerdown', function(evt) {
    dragState = { x: evt.clientX, y: evt.clientY, tx: paper.translate().tx, ty: paper.translate().ty };
  });
  paper.el.addEventListener('mousemove', function(evt) {
    if (!dragState) return;
    paper.translate(dragState.tx + evt.clientX - dragState.x, dragState.ty + evt.clientY - dragState.y);
  });
  document.addEventListener('mouseup', function() { dragState = null; });
  paper.el.addEventListener('wheel', function(evt) {
    evt.preventDefault();
    var delta = evt.deltaY &lt; 0 ? 1.1 : 0.9;
    var s = paper.scale();
    var newS = Math.max(0.1, Math.min(3, s.sx * delta));
    var t = paper.translate();
    var rect = paper.el.getBoundingClientRect();
    var cursorX = evt.clientX - rect.left;
    var cursorY = evt.clientY - rect.top;
    var localX = (cursorX - t.tx) / s.sx;
    var localY = (cursorY - t.ty) / s.sy;
    paper.scale(newS, newS);
    paper.translate(cursorX - localX * newS, cursorY - localY * newS);
  });

  clayersGraph = { graph: graph, paper: paper, data: data };
  injectSvgDefs(paper);
  graphInitialized = true;

  // Set initial node from URL hash
  var initHash = window.location.hash.substring(1);
  if (initHash) currentNodeId = initHash;
  else if (activeNavTargetId) currentNodeId = activeNavTargetId;

  // Build initial graph and fit to view (hidden until layout completes)
  container.style.visibility = 'hidden';

  // --- Click-to-navigate (Step 7) ---
  paper.on('element:pointerclick', function(elementView) {
    var nId = elementView.model.prop('nodeId');
    if (nId &amp;&amp; window.navigateTo) {
      currentNodeId = nId;
      window.navigateTo(nId);
      // Close mobile overlay
      var panel = document.getElementById('graph-panel');
      if (panel &amp;&amp; panel.classList.contains('mobile-active')) {
        panel.classList.remove('mobile-active');
        panel.classList.add('collapsed');
      }
    }
  });

  // --- Bidirectional highlighting (Step 8) ---

  // Graph hover -> Content highlight
  paper.on('element:mouseenter', function(elementView) {
    var nId = elementView.model.prop('nodeId');
    var nPage = elementView.model.prop('nodePage');
    var activePage = document.querySelector('.page.active');
    if (activePage &amp;&amp; activePage.getAttribute('data-page') === nPage) {
      var target = document.getElementById(nId);
      if (target) target.classList.add('content-highlight');
    }
    // Highlight in graph
    elementView.model.attr('body/filter', 'url(#node-glow)');
  });

  paper.on('element:mouseleave', function(elementView) {
    var nId = elementView.model.prop('nodeId');
    var target = document.getElementById(nId);
    if (target) target.classList.remove('content-highlight');
    // Unhighlight in graph
    elementView.model.attr('body/filter', 'url(#node-shadow)');
  });

  // Content hover -> Graph highlight
  var mainEl = document.querySelector('main');
  if (mainEl) {
    mainEl.addEventListener('mouseenter', function(evt) {
      var link = evt.target.closest('a[href^="#"]');
      if (!link || !clayersGraph) return;
      var targetId = link.getAttribute('href').substring(1);
      if (nodeMap[targetId]) {
        nodeMap[targetId].attr('body/filter', 'url(#node-glow)');
      }
    }, true);

    mainEl.addEventListener('mouseleave', function(evt) {
      var link = evt.target.closest('a[href^="#"]');
      if (!link || !clayersGraph) return;
      var targetId = link.getAttribute('href').substring(1);
      if (nodeMap[targetId]) {
        nodeMap[targetId].attr('body/filter', 'url(#node-shadow)');
      }
    }, true);
  }

  // --- Toolbar events ---
  // Layout buttons
  document.getElementById('graph-layout-group').addEventListener('click', function(e) {
    var btn = e.target.closest('[data-layout]');
    if (!btn) return;
    this.querySelectorAll('.graph-tb').forEach(function(b) { b.classList.remove('active'); });
    btn.classList.add('active');
    applyLayout(btn.getAttribute('data-layout'));
  });

  // Scope buttons
  document.getElementById('graph-scope-group').addEventListener('click', function(e) {
    var btn = e.target.closest('[data-scope]');
    if (!btn) return;
    this.querySelectorAll('.graph-tb').forEach(function(b) { b.classList.remove('active'); });
    btn.classList.add('active');
    var scope = btn.getAttribute('data-scope');
    try { localStorage.setItem('clayers-graph-scope', scope); } catch(e2) {}
    updateGraphScope();
  });

  document.getElementById('graph-fit').addEventListener('click', function() {
    if (clayersGraph) clayersGraph.paper.scaleContentToFit({ padding: 40, minScale: 0.1, maxScale: 1.5 });
  });

  // Graph search (debounced)
  var graphSearchTimer = null;
  document.getElementById('graph-search').addEventListener('input', function() {
    var q = this.value.trim().toLowerCase();
    if (graphSearchTimer) clearTimeout(graphSearchTimer);
    graphSearchTimer = setTimeout(function() {
      if (!clayersGraph) return;
      if (!q) {
        // Clear filter - rebuild full graph
        updateGraphScope();
        return;
      }
      // Filter nodes matching search text
      buildGraph(clayersGraph.data, function(n) {
        return n.label.toLowerCase().indexOf(q) >= 0 ||
               n.id.toLowerCase().indexOf(q) >= 0 ||
               n.type.toLowerCase().indexOf(q) >= 0;
      });
      applyLayout(getActiveLayout());
    }, 250);
  });

  // Determine initial scope and build graph accordingly
  var initialScope = 'full';
  try {
    var savedScope = localStorage.getItem('clayers-graph-scope');
    if (savedScope) initialScope = savedScope;
  } catch(e) {}
  if (data.nodes.length > 200) initialScope = 'neighborhood';
  setActiveScope(initialScope);

  if (initialScope === 'neighborhood' &amp;&amp; currentNodeId) {
    showNeighborhood(currentNodeId);
  } else {
    buildGraph(data, null);
    applyLayout('elk-layered');
  }

  // ResizeObserver to keep paper in sync
  if (window.ResizeObserver) {
    new ResizeObserver(function() {
      if (clayersGraph) {
        clayersGraph.paper.setDimensions(container.clientWidth, container.clientHeight);
      }
    }).observe(container);
  }
}

// --- Toggle graph panel (Step 4) ---

function toggleGraphPanel() {
  var panel = document.getElementById('graph-panel');
  var divider = document.getElementById('panel-divider');
  var toggleBtn = document.querySelector('.graph-toggle');
  if (!panel) return;

  var isMobile = window.innerWidth &lt; 768;
  if (isMobile) {
    if (panel.classList.contains('mobile-active')) {
      panel.classList.remove('mobile-active');
      panel.classList.add('collapsed');
    } else {
      panel.classList.remove('collapsed');
      panel.classList.add('mobile-active');
      initGraph();
    }
  } else {
    var isOpen = !panel.classList.contains('collapsed');
    if (isOpen) {
      // Close
      panel.classList.add('collapsed');
      if (divider) divider.style.display = 'none';
    } else {
      // Open
      panel.classList.remove('collapsed');
      if (divider) divider.style.display = '';
      initGraph();
      try {
        var savedW = localStorage.getItem('clayers-graph-width');
        if (savedW) panel.style.width = savedW + 'px';
      } catch(e) {}
      // Re-fit after opening
      if (clayersGraph) {
        requestAnimationFrame(function() {
          var c = document.getElementById('graph-container');
          clayersGraph.paper.setDimensions(c.clientWidth, c.clientHeight);
          clayersGraph.paper.scaleContentToFit({ padding: 40, minScale: 0.1, maxScale: 1.5 });
        });
      }
    }
  }
  // Update toggle button active state
  if (toggleBtn) {
    var nowOpen = !panel.classList.contains('collapsed') || panel.classList.contains('mobile-active');
    toggleBtn.classList.toggle('active', nowOpen);
  }
}

// --- Resizable divider (Step 5) ---

(function() {
  var divider = document.getElementById('panel-divider');
  var panel = document.getElementById('graph-panel');
  if (!divider || !panel) return;
  var dragging = false;

  divider.addEventListener('mousedown', function(e) {
    e.preventDefault();
    dragging = true;
    divider.classList.add('dragging');
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
  });

  document.addEventListener('mousemove', function(e) {
    if (!dragging) return;
    var wrapper = document.querySelector('.content-wrapper');
    if (!wrapper) return;
    var wrapperRect = wrapper.getBoundingClientRect();
    var newWidth = wrapperRect.right - e.clientX;
    newWidth = Math.max(280, Math.min(wrapperRect.width * 0.7, newWidth));
    panel.style.width = newWidth + 'px';
    if (clayersGraph) {
      var c = document.getElementById('graph-container');
      clayersGraph.paper.setDimensions(c.clientWidth, c.clientHeight);
    }
  });

  document.addEventListener('mouseup', function() {
    if (!dragging) return;
    dragging = false;
    divider.classList.remove('dragging');
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
    try { localStorage.setItem('clayers-graph-width', panel.offsetWidth); } catch(e) {}
  });
})();

// --- Hook showPage for scope updates (Step 9) ---
var origShowPage2 = showPage;
showPage = function(id, push, target) {
  origShowPage2(id, push, target);
  currentNodeId = target || id;
  if (clayersGraph &amp;&amp; getActiveScope() === 'neighborhood') {
    showNeighborhood(currentNodeId);
  }
  // Highlight current node in graph
  if (clayersGraph &amp;&amp; nodeMap[currentNodeId]) {
    // Reset all highlights
    Object.keys(nodeMap).forEach(function(nid) {
      nodeMap[nid].attr('body/filter', 'url(#node-shadow)');
    });
    nodeMap[currentNodeId].attr('body/filter', 'url(#node-glow)');
  }
};

// --- Theme recolor (Step 11) ---
function recolorGraph() {
  if (!clayersGraph) return;
  var theme = getThemeColors();
  Object.keys(nodeMap).forEach(function(id) {
    nodeMap[id].attr('body/opacity', theme.nodeBg);
  });
}

// Hook into theme toggle
var origToggleTheme = toggleTheme;
toggleTheme = function() {
  origToggleTheme();
  recolorGraph();
};

// Init graph on load (panel starts open)
initGraph();
// Mark toggle as active since panel starts open
var _gtb = document.querySelector('.graph-toggle');
if (_gtb) _gtb.classList.add('active');
          </xsl:text>
        </script>

      </body>
    </html>
  </xsl:template>

  <!-- Nav item with connectivity + nested subsections -->
  <xsl:template name="nav-item">
    <xsl:param name="section"/>
    <xsl:variable name="sid" select="$section/@id"/>
    <xsl:variable name="out" select="count($section/ancestor::cmb:spec//rel:relation[@from = $sid])"/>
    <xsl:variable name="inc" select="count($section/ancestor::cmb:spec//rel:relation[@to = $sid])"/>
    <xsl:variable name="spec-drifts" select="$section/ancestor::cmb:spec//doc:drift[@node = $sid and @status = 'spec-drifted']"/>
    <xsl:variable name="art-drifts" select="$section/ancestor::cmb:spec//doc:drift[@node = $sid and @status = 'artifact-drifted']"/>
    <xsl:variable name="has-spec-drift" select="boolean($spec-drifts)"/>
    <xsl:variable name="has-art-drift" select="boolean($art-drifts)"/>
    <li data-label="{$section/pr:title}" data-conn="{$out + $inc}">
      <a href="#{$sid}">
        <span>
          <xsl:if test="$has-spec-drift or $has-art-drift">
            <xsl:variable name="drift-class">
              <xsl:choose>
                <xsl:when test="$has-spec-drift and $has-art-drift">drift-both</xsl:when>
                <xsl:when test="$has-spec-drift">drift-spec</xsl:when>
                <xsl:otherwise>drift-artifact</xsl:otherwise>
              </xsl:choose>
            </xsl:variable>
            <xsl:variable name="drift-title">
              <xsl:if test="$has-spec-drift"><xsl:value-of select="count($spec-drifts)"/> spec drifted</xsl:if>
              <xsl:if test="$has-spec-drift and $has-art-drift">, </xsl:if>
              <xsl:if test="$has-art-drift"><xsl:value-of select="count($art-drifts)"/> artifact drifted</xsl:if>
            </xsl:variable>
            <span class="drift-dot {$drift-class}" title="{$drift-title}">&#x25CF;</span>
            <xsl:text> </xsl:text>
          </xsl:if>
          <xsl:value-of select="$section/pr:title"/>
        </span>
        <xsl:if test="$out + $inc > 0">
          <span class="conn-count" title="{$out} outgoing, {$inc} incoming relations">
            <xsl:if test="$out > 0"><xsl:value-of select="$out"/>&#x2197;</xsl:if>
            <xsl:if test="$out > 0 and $inc > 0"><xsl:text> </xsl:text></xsl:if>
            <xsl:if test="$inc > 0"><xsl:value-of select="$inc"/>&#x2199;</xsl:if>
          </span>
        </xsl:if>
      </a>
      <xsl:if test="$section/pr:section">
        <ul>
          <xsl:for-each select="$section/pr:section">
            <xsl:call-template name="nav-item">
              <xsl:with-param name="section" select="."/>
            </xsl:call-template>
          </xsl:for-each>
        </ul>
      </xsl:if>
    </li>
  </xsl:template>

  <!-- TOC generation: walk pr:section elements -->
  <xsl:template name="toc">
    <xsl:for-each select="pr:section">
      <li>
        <a href="#{@id}">
          <xsl:value-of select="pr:title"/>
        </a>
        <xsl:if test="pr:section">
          <ul>
            <xsl:for-each select="pr:section">
              <li>
                <a href="#{@id}">
                  <xsl:value-of select="pr:title"/>
                </a>
                <xsl:if test="pr:section">
                  <ul>
                    <xsl:for-each select="pr:section">
                      <li>
                        <a href="#{@id}">
                          <xsl:value-of select="pr:title"/>
                        </a>
                      </li>
                    </xsl:for-each>
                  </ul>
                </xsl:if>
              </li>
            </xsl:for-each>
          </ul>
        </xsl:if>
      </li>
    </xsl:for-each>
  </xsl:template>

  <!-- Suppress elements that are only rendered in explicit for-each or collected sections.
       Do NOT suppress pln:plan, dec:decision, art:mapping, src:source here because
       the for-each in cmb:spec calls apply-templates on them and needs to reach
       the imported layer templates. -->
  <xsl:template match="trm:term"/>
  <xsl:template match="org:concept | org:task | org:reference"/>
  <xsl:template match="rel:relation"/>
  <xsl:template match="art:exempt"/>
  <xsl:template match="llm:node | llm:schema"/>
  <xsl:template match="rev:revision"/>
  <xsl:template match="idx:*"/>

  <!-- Suppress internal child elements consumed by parent templates -->
  <xsl:template match="dec:status | dec:title | dec:rationale | dec:alternative | dec:supersedes"/>
  <xsl:template match="art:spec-ref | art:artifact | art:range | art:coverage | art:note"/>
  <xsl:template match="src:title | src:author | src:overview | src:published"/>
  <xsl:template match="pln:title | pln:overview | pln:status | pln:item-status | pln:description | pln:criterion | pln:witness"/>
  <xsl:template match="trm:name | trm:definition"/>
  <xsl:template match="org:purpose | org:actor"/>
  <xsl:template match="rel:note"/>
  <xsl:template match="rev:date"/>
  <xsl:template match="spec:clayers"/>
  <xsl:template match="doc:report | doc:drift | doc:fragment"/>


</xsl:stylesheet>

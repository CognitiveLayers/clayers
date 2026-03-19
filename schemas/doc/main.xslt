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
    xmlns:src="urn:clayers:source"
    xmlns:pln="urn:clayers:plan"
    xmlns:art="urn:clayers:artifact"
    xmlns:llm="urn:clayers:llm"
    xmlns:rev="urn:clayers:revision"
    xmlns:idx="urn:clayers:index"
    exclude-result-prefixes="xs cmb spec pr trm org rel dec src pln art llm rev idx">

  <xsl:import href="catchall.xslt"/>
  <xsl:import href="prose.xslt"/>
  <xsl:import href="terminology.xslt"/>
  <xsl:import href="organization.xslt"/>
  <xsl:import href="relation.xslt"/>
  <xsl:import href="decision.xslt"/>
  <xsl:import href="source.xslt"/>
  <xsl:import href="plan.xslt"/>
  <xsl:import href="artifact.xslt"/>
  <xsl:import href="llm.xslt"/>
  <xsl:import href="revision.xslt"/>

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
        <link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&amp;family=Newsreader:ital,opsz,wght@0,6..72,400;0,6..72,500;0,6..72,600;0,6..72,700;1,6..72,400;1,6..72,500&amp;family=JetBrains+Mono:wght@400;500&amp;display=swap" rel="stylesheet"/>
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
main {
  margin-left: var(--sidebar-width);
  max-width: 48rem;
  padding: 2rem 2.5rem 4rem;
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

/* SPA pages */
.page { display: none; }
.page.active { display: block; }
          </xsl:text>
        </style>
      </head>
      <body>
        <nav id="sidebar">
          <div class="sidebar-header">
            <input id="search" type="search" placeholder="Find..." autocomplete="off"/>
            <button class="theme-toggle" onclick="toggleTheme()" aria-label="Toggle theme">
              <svg class="icon-sun" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="4"/><path d="M12 2v2"/><path d="M12 20v2"/><path d="m4.93 4.93 1.41 1.41"/><path d="m17.66 17.66 1.41 1.41"/><path d="M2 12h2"/><path d="M20 12h2"/><path d="m6.34 17.66-1.41 1.41"/><path d="m19.07 4.93-1.41 1.41"/></svg>
              <svg class="icon-moon" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/></svg>
            </button>
          </div>
          <ul id="search-results"></ul>

          <!-- Reading Maps (top of sidebar) -->
          <xsl:if test=".//org:map">
            <div class="toc-heading">Reading Maps</div>
            <ul>
              <xsl:for-each select=".//org:map">
                <li>
                  <a href="#{@id}"><xsl:value-of select="org:title"/></a>
                </li>
              </xsl:for-each>
            </ul>
          </xsl:if>

          <!-- Sections grouped by organization type -->
          <xsl:variable name="all-concepts" select=".//org:concept/@ref"/>
          <xsl:variable name="all-tasks" select=".//org:task/@ref"/>
          <xsl:variable name="all-references" select=".//org:reference/@ref"/>

          <!-- Concepts -->
          <xsl:variable name="concept-sections" select="pr:section[@id = $all-concepts]"/>
          <xsl:if test="$concept-sections">
            <div class="nav-group" data-sortable="true">
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
            <div class="nav-group" data-sortable="true">
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
            <div class="nav-group" data-sortable="true">
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
            <div class="nav-group" data-sortable="true">
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
            <div class="toc-heading">Terminology</div>
            <ul>
              <li><a href="#glossary">Glossary (<xsl:value-of select="count(.//trm:term)"/>)</a></li>
            </ul>
          </xsl:if>

          <!-- Decisions -->
          <xsl:if test=".//dec:decision">
            <div class="toc-heading">Decisions</div>
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
          </xsl:if>

          <!-- Plans -->
          <xsl:if test=".//pln:plan">
            <div class="toc-heading">Plans</div>
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
          </xsl:if>

          <!-- Sources -->
          <xsl:if test=".//src:source">
            <div class="toc-heading">Sources</div>
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
          </xsl:if>

          <!-- Artifacts -->
          <xsl:if test=".//art:mapping">
            <div class="toc-heading">Artifacts</div>
            <ul>
              <li><a href="#artifacts">Mappings (<xsl:value-of select="count(.//art:mapping)"/>)</a></li>
            </ul>
          </xsl:if>
        </nav>

        <main>
          <!-- Revision metadata banner -->
          <xsl:if test=".//rev:revision">
            <div class="revision-banner">
              <xsl:for-each select=".//rev:revision">
                <span class="badge badge-gray"><xsl:value-of select="@id"/></span>
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

function showPage(pageId, pushHistory) {
  document.querySelectorAll('.page').forEach(function(p) {
    p.classList.remove('active');
  });
  var target = document.querySelector('.page[data-page="' + pageId + '"]');
  if (target) {
    target.classList.add('active');
    window.scrollTo(0, 0);
  }
  document.querySelectorAll('#sidebar a').forEach(function(a) {
    a.classList.remove('active');
  });
  var link = document.querySelector('#sidebar a[href="#' + pageId + '"]');
  if (link) link.classList.add('active');
  if (pushHistory !== false) {
    history.pushState({ page: pageId }, '', '#' + pageId);
  }
}

window.addEventListener('popstate', function(e) {
  var pageId;
  if (e.state &amp;&amp; e.state.page) {
    pageId = e.state.page;
  } else {
    pageId = window.location.hash.substring(1);
  }
  if (pageId) {
    showPage(pageId, false);
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
      showPage(id);
      return;
    }
    // 2. Element inside a page (term in glossary, source in sources, etc.)
    var el = document.getElementById(id);
    if (el) {
      var parentPage = el.closest('.page');
      if (parentPage) {
        showPage(parentPage.getAttribute('data-page'));
        setTimeout(function() { el.scrollIntoView({ behavior: 'smooth' }); }, 50);
      }
    }
  }

  // Intercept all internal anchor clicks (sidebar + content)
  document.addEventListener('click', function(e) {
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
    var target = document.querySelector('.page[data-page="' + hash + '"]');
    if (target) {
      showPage(hash, false);
    } else {
      var el = document.getElementById(hash);
      if (el) {
        var page = el.closest('.page');
        if (page) {
          showPage(page.getAttribute('data-page'), false);
          setTimeout(function() { el.scrollIntoView(); }, 50);
        }
      } else if (pages.length > 0) {
        showPage(pages[0].getAttribute('data-page'), false);
      }
    }
  } else if (pages.length > 0) {
    showPage(pages[0].getAttribute('data-page'), false);
  }
  // Replace initial state so popstate has something to land on
  history.replaceState({ page: document.querySelector('.page.active')?.getAttribute('data-page') || '' }, '');

  // Build search index from all navigable nodes
  var searchData = [];
  document.querySelectorAll('[id]').forEach(function(el) {
    var id = el.getAttribute('id');
    if (!id || id === 'sidebar' || id === 'search' || id === 'search-results') return;
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
})();
          </xsl:text>
        </script>
        <script src="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.11.1/highlight.min.js"></script>
        <script>
          <xsl:text>
hljs.highlightAll();
// Re-highlight when switching pages
var origShowPage = showPage;
showPage = function(id, push) { origShowPage(id, push); hljs.highlightAll(); };
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
    <li data-label="{$section/pr:title}" data-conn="{$out + $inc}">
      <a href="#{$sid}">
        <span><xsl:value-of select="$section/pr:title"/></span>
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
  <xsl:template match="pln:title | pln:overview | pln:status | pln:item | pln:item-status | pln:description | pln:acceptance | pln:criterion | pln:witness"/>
  <xsl:template match="trm:name | trm:definition"/>
  <xsl:template match="org:purpose | org:actor"/>
  <xsl:template match="rel:note"/>
  <xsl:template match="rev:date"/>
  <xsl:template match="spec:clayers"/>


</xsl:stylesheet>

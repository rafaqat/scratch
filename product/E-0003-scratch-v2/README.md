# Scratch v2 Roadmap

> Close the gap with Notion. Epic **E-0003-scratch-v2** — 15 stories across 5 features.

**Current version:** v1.0.0 · **Target:** v1.1.0
**Created:** 2026-02-13 · **Last updated:** 2026-02-13

## Status

| # | Feature | Stories | Points |
|---|---------|---------|--------|
| 1 | [Databases (Markdown-native)](#1-databases-markdown-native) | [01](stories/S-0003-01.md) [02](stories/S-0003-02.md) [03](stories/S-0003-03.md) [04](stories/S-0003-04.md) | 21 |
| 2 | [Nested Pages & Wikilinks](#2-nested-pages--wikilinks) | [05](stories/S-0003-05.md) [06](stories/S-0003-06.md) | 8 |
| 3 | [Richer Blocks](#3-richer-blocks) | [07](stories/S-0003-07.md) [08](stories/S-0003-08.md) [09](stories/S-0003-09.md) [10](stories/S-0003-10.md) | 11 |
| 4 | [Templates](#4-templates) | [11](stories/S-0003-11.md) [12](stories/S-0003-12.md) | 5 |
| 5 | [MCP Plugin Ecosystem](#5-mcp-plugin-ecosystem) | [13](stories/S-0003-13.md) [14](stories/S-0003-14.md) [15](stories/S-0003-15.md) | 10 |
| | **Total** | **15** | **55** |

## Priority Order

1. **Wikilinks** — Highest value/effort ratio, unlocks knowledge graph
2. **Richer blocks** — Editor parity with Notion
3. **Templates** — Quick win, reduces friction
4. **Databases** — Biggest effort, biggest payoff
5. **MCP plugins** — Multiplier once core is solid

---

## 1. Databases (Markdown-native)

The single biggest gap. Store structured data as YAML/frontmatter markdown files, render as tables/boards in the editor, expose via MCP.

| Story | Title | Points | Status | Depends On |
|-------|-------|--------|--------|------------|
| [S-0003-01](stories/S-0003-01.md) | Database schema & file format | 8 | In Progress | — |
| [S-0003-02](stories/S-0003-02.md) | Table view block | 5 | Backlog | [01](stories/S-0003-01.md) |
| [S-0003-03](stories/S-0003-03.md) | Board/kanban view block | 5 | Backlog | [01](stories/S-0003-01.md) |
| [S-0003-04](stories/S-0003-04.md) | MCP database tools | 3 | Backlog | [01](stories/S-0003-01.md) |

## 2. Nested Pages & Wikilinks

Allow notes to reference and nest other notes. Full wikilink support with backlinks.

| Story | Title | Points | Status | Depends On |
|-------|-------|--------|--------|------------|
| [S-0003-05](stories/S-0003-05.md) | Wikilink parser & inline block | 5 | Done | — |
| [S-0003-06](stories/S-0003-06.md) | Backlinks panel | 3 | In Progress | [05](stories/S-0003-05.md) |

## 3. Richer Blocks

BlockNote supports custom blocks. Add the most-requested Notion block types.

| Story | Title | Points | Status | Depends On |
|-------|-------|--------|--------|------------|
| [S-0003-07](stories/S-0003-07.md) | Callout block | 3 | In Progress | — |
| [S-0003-08](stories/S-0003-08.md) | Equation block | 3 | In Progress | — |
| [S-0003-09](stories/S-0003-09.md) | Bookmark block | 3 | Backlog | — |
| [S-0003-10](stories/S-0003-10.md) | Table of contents block | 2 | In Progress | — |

## 4. Templates

Note and database templates stored as markdown files.

| Story | Title | Points | Status | Depends On |
|-------|-------|--------|--------|------------|
| [S-0003-11](stories/S-0003-11.md) | Template system | 3 | In Progress | — |
| [S-0003-12](stories/S-0003-12.md) | Database row templates | 2 | Backlog | [01](stories/S-0003-01.md) [11](stories/S-0003-11.md) |

## 5. MCP Plugin Ecosystem

Let AI agents build integrations. The MCP server already exposes 24 tools — this extends it.

| Story | Title | Points | Status | Depends On |
|-------|-------|--------|--------|------------|
| [S-0003-13](stories/S-0003-13.md) | Webhook receiver | 3 | In Progress | — |
| [S-0003-14](stories/S-0003-14.md) | Plugin manifest system | 5 | Backlog | — |
| [S-0003-15](stories/S-0003-15.md) | Built-in reference plugins | 2 | Backlog | [14](stories/S-0003-14.md) |

---

## File Format Decisions

**Databases:** Folder-per-database with `_schema.md` (YAML columns/views) + row `.md` files with matching frontmatter. Generalizes the existing stories/kanban pattern from `src-tauri/src/stories.rs`.

**Wikilinks:** `[[Note Title]]` in markdown. Backlinks index at `.scratch/backlinks.json`, rebuilt on startup, updated incrementally on save.

**Blocks:** Callouts as `> [!type]`, equations as `$...$`/`$$...$$`, bookmarks as fenced blocks, TOC as `[toc]`.

**Templates:** `.scratch/templates/*.md` with `{{date}}`, `{{title}}`, `{{cursor}}` variables.

---

## Changelog

| Date | Version | Event | Stories | Commit |
|------|---------|-------|---------|--------|
| 2026-02-13 | v1.0.0 | Roadmap created | All 15 stories in Backlog | — |
| 2026-02-13 | v1.0.0 | Started | S-0003-05 Wikilink parser & inline block | — |
| 2026-02-13 | v1.0.0 | Completed | S-0003-05 Wikilink parser & inline block | `c64bc7d` |
| 2026-02-13 | v1.0.0 | Started | S-0003-06 Backlinks panel | — |
| 2026-02-13 | v1.0.0 | Started | S-0003-10 Table of contents block | — |
| 2026-02-13 | v1.0.0 | Started | S-0003-01 Database schema & file format | — |
| 2026-02-13 | v1.0.0 | Started | S-0003-13 Webhook receiver | — |
| 2026-02-13 | v1.0.0 | Started | S-0003-11 Template system | — |
| 2026-02-13 | v1.0.0 | Started | S-0003-07 Callout block | — |
| 2026-02-13 | v1.0.0 | Started | S-0003-08 Equation block | — |

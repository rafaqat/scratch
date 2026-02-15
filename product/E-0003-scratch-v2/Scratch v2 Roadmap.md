# Scratch v2 Roadmap

> Close the gap with Notion. Epic **E-0003-scratch-v2** — 15 stories across 5 features.

**Current version:** v1.0.0 · **Target:** v1.1.0

**Created:** 2026-02-13 · **Last updated:** 2026-02-13

## Status

| # | Feature                                                     | Stories     | Points |
| - | ----------------------------------------------------------- | ----------- | ------ |
| 1 | [Databases (Markdown-native)](#1-databases-markdown-native) | 01 02 03 04 | 21     |
| 2 | [Nested Pages & Wikilinks](#2-nested-pages--wikilinks)      | 05 06       | 8      |
| 3 | [Richer Blocks](#3-richer-blocks)                           | 07 08 09 10 | 11     |
| 4 | [Templates](#4-templates)                                   | 11 12       | 5      |
| 5 | [MCP Plugin Ecosystem](#5-mcp-plugin-ecosystem)             | 13 14 15    | 10     |
|   | **Total**                                                   | **15**      | **55** |

## Priority Order

1. **Wikilinks** — Highest value/effort ratio, unlocks knowledge graph

2. **Richer blocks** — Editor parity with Notion

3. **Templates** — Quick win, reduces friction

4. **Databases** — Biggest effort, biggest payoff

5. **MCP plugins** — Multiplier once core is solid

***

## 1. Databases (Markdown-native)

The single biggest gap. Store structured data as YAML/frontmatter markdown files, render as tables/boards in the editor, expose via MCP.

| Story     | Title                         | Points | Status | Depends On |
| --------- | ----------------------------- | ------ | ------ | ---------- |
| S-0003-01 | Database schema & file format | 8      | Done   | —          |
| S-0003-02 | Table view block              | 5      | Done   | 01         |
| S-0003-03 | Board/kanban view block       | 5      | Done   | 01         |
| S-0003-04 | MCP database tools            | 3      | Done   | 01         |

## 2. Nested Pages & Wikilinks

Allow notes to reference and nest other notes. Full wikilink support with backlinks.

| Story     | Title                          | Points | Status | Depends On |
| --------- | ------------------------------ | ------ | ------ | ---------- |
| S-0003-05 | Wikilink parser & inline block | 5      | Done   | —          |
| S-0003-06 | Backlinks panel                | 3      | Done   | 05         |

## 3. Richer Blocks

BlockNote supports custom blocks. Add the most-requested Notion block types.

| Story     | Title                   | Points | Status | Depends On |
| --------- | ----------------------- | ------ | ------ | ---------- |
| S-0003-07 | Callout block           | 3      | Done   | —          |
| S-0003-08 | Equation block          | 3      | Done   | —          |
| S-0003-09 | Bookmark block          | 3      | Done   | —          |
| S-0003-10 | Table of contents block | 2      | Done   | —          |

## 4. Templates

Note and database templates stored as markdown files.

| Story     | Title                  | Points | Status | Depends On |
| --------- | ---------------------- | ------ | ------ | ---------- |
| S-0003-11 | Template system        | 3      | Done   | —          |
| S-0003-12 | Database row templates | 2      | Done   | 01 11      |

## 5. MCP Plugin Ecosystem

Let AI agents build integrations. The MCP server already exposes 24 tools — this extends it.

| Story     | Title                      | Points | Status | Depends On |
| --------- | -------------------------- | ------ | ------ | ---------- |
| S-0003-13 | Webhook receiver           | 3      | Done   | —          |
| S-0003-14 | Plugin manifest system     | 5      | Done   | —          |
| S-0003-15 | Built-in reference plugins | 2      | Done   | 14         |

***

## File Format Decisions

**Databases:** Folder-per-database with `_schema.md` (YAML columns/views) + row `.md` files with matching frontmatter. Generalizes the existing stories/kanban pattern from `src-tauri/src/stories.rs`.

**Wikilinks:** `[[Note Title]]` in markdown. Backlinks index at `.scratch/backlinks.json`, rebuilt on startup, updated incrementally on save.

**Blocks:** Callouts as `> [!type]`, equations as `$...$`/`$$...$$`, bookmarks as fenced blocks, TOC as `[toc]`.

**Templates:** `.scratch/templates/*.md` with `{{date}}`, `{{title}}`, `{{cursor}}` variables.

***

## Changelog

| Date       | Version | Event             | Stories                                  | Commit    |
| ---------- | ------- | ----------------- | ---------------------------------------- | --------- |
| 2026-02-13 | v1.0.0  | Roadmap created   | All 15 stories in Backlog                | —         |
| 2026-02-13 | v1.0.0  | Started           | S-0003-05 Wikilink parser & inline block | —         |
| 2026-02-13 | v1.0.0  | Completed         | S-0003-05 Wikilink parser & inline block | `c64bc7d` |
| 2026-02-13 | v1.0.0  | Started           | S-0003-06 Backlinks panel                | —         |
| 2026-02-13 | v1.0.0  | Started           | S-0003-10 Table of contents block        | —         |
| 2026-02-13 | v1.0.0  | Started           | S-0003-01 Database schema & file format  | —         |
| 2026-02-13 | v1.0.0  | Started           | S-0003-13 Webhook receiver               | —         |
| 2026-02-13 | v1.0.0  | Started           | S-0003-11 Template system                | —         |
| 2026-02-13 | v1.0.0  | Started           | S-0003-07 Callout block                  | —         |
| 2026-02-13 | v1.0.0  | Started           | S-0003-08 Equation block                 | —         |
| 2026-02-13 | v1.0.0  | Completed         | S-0003-10 Table of contents block        | `8228c0f` |
| 2026-02-13 | v1.0.0  | Completed         | S-0003-13 Webhook receiver               | `a556d40` |
| 2026-02-13 | v1.0.0  | Completed         | S-0003-01 Database schema & file format  | `a556d40` |
| 2026-02-13 | v1.0.0  | Started           | S-0003-14 Plugin manifest system         | —         |
| 2026-02-13 | v1.0.0  | Completed         | S-0003-07 Callout block                  | `118491f` |
| 2026-02-13 | v1.0.0  | Completed         | S-0003-06 Backlinks panel                | `24a0e9a` |
| 2026-02-13 | v1.0.0  | Completed         | S-0003-14 Plugin manifest system         | `e5d8d49` |
| 2026-02-13 | v1.0.0  | Completed         | S-0003-08 Equation block                 | `9ee89d7` |
| 2026-02-13 | v1.0.0  | Completed         | S-0003-11 Template system                | `24470cf` |
| 2026-02-13 | v1.0.0  | Started           | S-0003-09 Bookmark block                 | —         |
| 2026-02-13 | v1.0.0  | Completed         | S-0003-09 Bookmark block                 | `01abf14` |
| 2026-02-13 | v1.0.0  | Completed         | S-0003-02 Table view block               | `648e63c` |
| 2026-02-13 | v1.0.0  | Completed         | S-0003-03 Board/kanban view block        | `648e63c` |
| 2026-02-13 | v1.0.0  | Completed         | S-0003-04 MCP database tools             | `648e63c` |
| 2026-02-13 | v1.0.0  | Completed         | S-0003-12 Database row templates         | `ecc90c9` |
| 2026-02-13 | v1.0.0  | Completed         | S-0003-15 Built-in reference plugins     | `fe828e4` |
| 2026-02-13 | v1.0.0  | **Epic complete** | All 15 stories Done (55 points)          | —         |

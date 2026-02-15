# Scratch v3 Roadmap

> Level up UX, editor power, and data management. Epic **E-0004-scratch-v3** — 16 stories across 6 features.

**Current version:** v1.1.0 · **Target:** v1.2.0

**Created:** 2026-02-14 · **Last updated:** 2026-02-14

## Status

| # | Feature                                              | Stories  | Points |
| - | ---------------------------------------------------- | -------- | ------ |
| 1 | [Folder Hierarchy & Navigation](#1-folder-hierarchy--navigation) | 01 02 03 | 16     |
| 2 | [Editor Blocks](#2-editor-blocks)                               | 04 05 06 | 10     |
| 3 | [Page Decoration](#3-page-decoration)                | 07 08    | 8      |
| 4 | [Trash & Version History](#4-trash--version-history) | 09 10    | 10     |
| 5 | [Database Enhancements](#5-database-enhancements)    | 11 12 13 | 13     |
| 6 | [Quality of Life](#6-quality-of-life)                | 14 15 16 | 8      |
|   | **Total**                                            | **16**   | **65** |

## Priority Order

1. **Folder hierarchy** — Most impactful UX gap, unlocks organization

2. **Editor blocks** — Toggle, divider, columns complete the block toolkit

3. **Page decoration** — Icons & covers make notes feel polished

4. **Trash & history** — Safety net, users expect undo/recovery

5. **Database enhancements** — Filters, calendar view, relations

6. **Quality of life** — Word count, import/export, page width

***

## 1. Folder Hierarchy & Navigation

The biggest structural gap. Scratch currently uses a flat note list — no subfolders, no tree navigation, no breadcrumbs.

| Story     | Title                           | Points | Status  | Depends On |
| --------- | ------------------------------- | ------ | ------- | ---------- |
| S-0004-01 | Folder tree sidebar             | 8      | Backlog | —          |
| S-0004-02 | Breadcrumb navigation           | 3      | Backlog | 01         |
| S-0004-03 | Drag-and-drop note organization | 5      | Backlog | 01         |

## 2. Editor Blocks

Complete the block toolkit with the most-requested Notion block types not yet implemented.

| Story     | Title               | Points | Status  | Depends On |
| --------- | ------------------- | ------ | ------- | ---------- |
| S-0004-04 | Toggle block        | 3      | Backlog | —          |
| S-0004-05 | Divider block       | 2      | Backlog | —          |
| S-0004-06 | Column layout block | 5      | Backlog | —          |

## 3. Page Decoration

Give notes a visual identity with icons and cover images, like Notion pages.

| Story     | Title            | Points | Status  | Depends On |
| --------- | ---------------- | ------ | ------- | ---------- |
| S-0004-07 | Page icon picker | 3      | Backlog | —          |
| S-0004-08 | Page cover image | 5      | Backlog | —          |

## 4. Trash & Version History

Safety net features. Currently, delete is permanent and there's no way to see past versions.

| Story     | Title                  | Points | Status  | Depends On |
| --------- | ---------------------- | ------ | ------- | ---------- |
| S-0004-09 | Trash bin with restore | 5      | Backlog | —          |
| S-0004-10 | Note version history   | 5      | Backlog | —          |

## 5. Database Enhancements

Build on the database system from E-0003 with filters, a calendar view, and relations.

| Story     | Title                        | Points | Status  | Depends On |
| --------- | ---------------------------- | ------ | ------- | ---------- |
| S-0004-11 | Database filter & sort UI    | 5      | Backlog | —          |
| S-0004-12 | Calendar database view       | 5      | Backlog | —          |
| S-0004-13 | Database relations & rollups | 3      | Backlog | —          |

## 6. Quality of Life

Small but impactful improvements for daily use.

| Story     | Title                  | Points | Status  | Depends On |
| --------- | ---------------------- | ------ | ------- | ---------- |
| S-0004-14 | Word & character count | 2      | Backlog | —          |
| S-0004-15 | Import & export        | 3      | Backlog | —          |
| S-0004-16 | Page width toggle      | 3      | Backlog | —          |

***

## File Format Decisions

**Folders:** Subfolders within the notes directory map directly to sidebar tree nodes. No config needed — scan the filesystem recursively.

**Page decoration:** Stored in note frontmatter: `icon: "emoji"` and `cover: "path-or-url"`. Cover images stored in `.assets/covers/`.

**Trash:** Soft delete — move files to `.scratch/trash/` with original path metadata. Auto-purge after 30 days.

**Version history:** Snapshots stored in `.scratch/history/{note-id}/` as timestamped copies. Keep last 50 versions per note.

**Database filters:** Stored in the database `_schema.md` view config alongside existing sort/group settings.

***

## Changelog

| Date       | Version | Event           | Stories                   | Commit |
| ---------- | ------- | --------------- | ------------------------- | ------ |
| 2026-02-14 | v1.1.0  | Roadmap created | All 16 stories in Backlog | —      |

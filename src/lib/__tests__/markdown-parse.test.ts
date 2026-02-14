/**
 * @vitest-environment jsdom
 *
 * Tests for markdown → BlockNote block conversion pipeline.
 * Reproduces the blank editor issue where complex markdown
 * (tables, horizontal rules, blockquotes) fails to render.
 */
import { describe, it, expect } from "vitest";
import { BlockNoteEditor, BlockNoteSchema, defaultBlockSpecs, defaultInlineContentSpecs } from "@blocknote/core";

// Minimal schema (no custom blocks, to isolate the issue)
const minimalSchema = BlockNoteSchema.create({
  blockSpecs: defaultBlockSpecs,
  inlineContentSpecs: defaultInlineContentSpecs,
});

function createEditor() {
  return BlockNoteEditor.create({ schema: minimalSchema });
}

const SIMPLE_MARKDOWN = `# Hello World

This is a simple note.

## Section 2

Some text here.
`;

const TABLE_MARKDOWN = `# Table Test

| Name | Value |
|------|-------|
| A    | 1     |
| B    | 2     |
`;

const COMPLEX_MARKDOWN = `# Scratch v3 Roadmap

> Level up UX, editor power, and data management.

**Current version:** v1.1.0

## Status

| # | Feature | Points |
|---|---------|--------|
| 1 | Folder Hierarchy | 16 |
| 2 | Editor Blocks | 10 |

---

## 1. Folder Hierarchy

The biggest structural gap.

| Story | Title | Status |
|-------|-------|--------|
| [S-0004-01](stories/S-0004-01.md) | Folder tree sidebar | Backlog |
| [S-0004-02](stories/S-0004-02.md) | Breadcrumb navigation | Backlog |
`;

const FULL_README = `# Scratch v3 Roadmap

> Level up UX, editor power, and data management. Epic **E-0004-scratch-v3** — 16 stories across 6 features.

**Current version:** v1.1.0 · **Target:** v1.2.0
**Created:** 2026-02-14 · **Last updated:** 2026-02-14

## Status

| # | Feature | Stories | Points |
|---|---------|---------|--------|
| 1 | [Folder Hierarchy & Navigation](#1-folder-hierarchy--navigation) | [01](stories/S-0004-01.md) [02](stories/S-0004-02.md) [03](stories/S-0004-03.md) | 16 |
| 2 | [Editor Blocks](#2-editor-blocks) | [04](stories/S-0004-04.md) [05](stories/S-0004-05.md) [06](stories/S-0004-06.md) | 10 |
| 3 | [Page Decoration](#3-page-decoration) | [07](stories/S-0004-07.md) [08](stories/S-0004-08.md) | 8 |

## Priority Order

1. **Folder hierarchy** — Most impactful UX gap, unlocks organization
2. **Editor blocks** — Toggle, divider, columns complete the block toolkit
3. **Page decoration** — Icons & covers make notes feel polished

---

## 1. Folder Hierarchy & Navigation

The biggest structural gap. Scratch currently uses a flat note list.

| Story | Title | Points | Status | Depends On |
|-------|-------|--------|--------|------------|
| [S-0004-01](stories/S-0004-01.md) | Folder tree sidebar | 8 | Backlog | — |
| [S-0004-02](stories/S-0004-02.md) | Breadcrumb navigation | 3 | Backlog | [01](stories/S-0004-01.md) |
| [S-0004-03](stories/S-0004-03.md) | Drag-and-drop note organization | 5 | Backlog | [01](stories/S-0004-01.md) |

## 2. Editor Blocks

Complete the block toolkit.

| Story | Title | Points | Status | Depends On |
|-------|-------|--------|--------|------------|
| [S-0004-04](stories/S-0004-04.md) | Toggle block | 3 | Backlog | — |
| [S-0004-05](stories/S-0004-05.md) | Divider block | 2 | Backlog | — |
| [S-0004-06](stories/S-0004-06.md) | Column layout block | 5 | Backlog | — |

---

## Changelog

| Date | Version | Event | Stories | Commit |
|------|---------|-------|---------|--------|
| 2026-02-14 | v1.1.0 | Roadmap created | All 16 stories in Backlog | — |
`;

describe("markdown → BlockNote blocks", () => {
  it("parses simple markdown", () => {
    const editor = createEditor();
    const blocks = editor.tryParseMarkdownToBlocks(SIMPLE_MARKDOWN);
    expect(blocks.length).toBeGreaterThan(0);
    expect(blocks[0].type).toBe("heading");
  });

  it("parses markdown with a table", () => {
    const editor = createEditor();
    const blocks = editor.tryParseMarkdownToBlocks(TABLE_MARKDOWN);
    expect(blocks.length).toBeGreaterThan(0);
    const types = blocks.map((b: { type: string }) => b.type);
    expect(types).toContain("heading");
    expect(types).toContain("table");
  });

  it("parses complex markdown with tables, blockquotes, and horizontal rules", () => {
    const editor = createEditor();
    const blocks = editor.tryParseMarkdownToBlocks(COMPLEX_MARKDOWN);
    expect(blocks.length).toBeGreaterThan(0);
    const types = blocks.map((b: { type: string }) => b.type);
    console.log("Complex block types:", types);
    expect(types).toContain("heading");
  });

  it("parses full README with many tables and links", () => {
    const editor = createEditor();
    const blocks = editor.tryParseMarkdownToBlocks(FULL_README);
    expect(blocks.length).toBeGreaterThan(0);
    const types = blocks.map((b: { type: string }) => b.type);
    console.log("Full README block types:", types);
    console.log("Block count:", blocks.length);
    expect(types).toContain("heading");
  });
});

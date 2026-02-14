/**
 * @vitest-environment jsdom
 *
 * Tests for injectWikilinks processing of BlockNote blocks.
 * Since tryParseMarkdownToBlocks works fine, the blank editor
 * issue must be in injectWikilinks or replaceBlocks.
 */
import { describe, it, expect } from "vitest";
import { BlockNoteEditor, BlockNoteSchema, defaultBlockSpecs, defaultInlineContentSpecs } from "@blocknote/core";
import { injectWikilinks } from "../wikilink";

const minimalSchema = BlockNoteSchema.create({
  blockSpecs: defaultBlockSpecs,
  inlineContentSpecs: defaultInlineContentSpecs,
});

function createEditor() {
  return BlockNoteEditor.create({ schema: minimalSchema });
}

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

1. **Folder hierarchy** — Most impactful UX gap
2. **Editor blocks** — Toggle, divider, columns

---

## 1. Folder Hierarchy & Navigation

The biggest structural gap.

| Story | Title | Points | Status | Depends On |
|-------|-------|--------|--------|------------|
| [S-0004-01](stories/S-0004-01.md) | Folder tree sidebar | 8 | Backlog | — |
| [S-0004-02](stories/S-0004-02.md) | Breadcrumb navigation | 3 | Backlog | [01](stories/S-0004-01.md) |

## Changelog

| Date | Version | Event | Stories | Commit |
|------|---------|-------|---------|--------|
| 2026-02-14 | v1.1.0 | Roadmap created | All 16 stories in Backlog | — |
`;

describe("injectWikilinks with table blocks", () => {
  it("processes blocks with tables without throwing", () => {
    const editor = createEditor();
    const blocks = editor.tryParseMarkdownToBlocks(FULL_README);
    expect(blocks.length).toBeGreaterThan(0);

    // This is where we suspect the failure occurs
    const result = injectWikilinks(blocks);
    expect(result.length).toBeGreaterThan(0);
    console.log("injectWikilinks succeeded with", result.length, "blocks");
  });

  it("handles table block content structure", () => {
    const editor = createEditor();
    const blocks = editor.tryParseMarkdownToBlocks(`
| Name | Value |
|------|-------|
| A    | 1     |
`);

    const tableBlock = blocks.find((b: { type: string }) => b.type === "table");
    expect(tableBlock).toBeDefined();

    // Log the actual table structure for debugging
    console.log("Table block structure:", JSON.stringify(tableBlock, null, 2).slice(0, 500));

    // Process with injectWikilinks
    const result = injectWikilinks(blocks);
    expect(result.length).toBeGreaterThan(0);
  });

  it("replaceBlocks works with parsed and injected blocks", () => {
    const editor = createEditor();
    const blocks = editor.tryParseMarkdownToBlocks(FULL_README);
    const injected = injectWikilinks(blocks);

    // This simulates what the editor does
    editor.replaceBlocks(editor.document, injected);
    expect(editor.document.length).toBeGreaterThan(0);
    console.log("replaceBlocks succeeded, document has", editor.document.length, "blocks");
  });
});

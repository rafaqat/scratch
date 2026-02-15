import {
  BlockNoteSchema,
  defaultBlockSpecs,
  defaultInlineContentSpecs,
} from "@blocknote/core";
import { Wikilink } from "./Wikilink";
import { TableOfContents } from "./TableOfContents";
import { Callout } from "./Callout";
import { EquationBlock } from "./EquationBlock";
import { InlineEquation } from "./InlineEquation";
import { BookmarkBlock } from "./BookmarkBlock";
import { DatabaseTableBlock } from "./DatabaseTable";
import { DividerBlock } from "./DividerBlock";
import { ToggleBlock } from "./ToggleBlock";
import { ColumnsBlock } from "./ColumnsBlock";

/**
 * Shared BlockNote schema that combines:
 * - All default block specs + custom blocks (TOC, Callout, Equation, Bookmark, Database Table)
 * - All default inline content specs + custom inline content (wikilinks, inline equations)
 *
 * Import this schema (and the ScratchEditor type) wherever the editor
 * instance is created or consumed.
 */
export const schema = BlockNoteSchema.create({
  blockSpecs: {
    ...defaultBlockSpecs,
    toc: TableOfContents(),
    callout: Callout(),
    equation: EquationBlock(),
    bookmark: BookmarkBlock(),
    databaseTable: DatabaseTableBlock(),
    divider: DividerBlock(),
    toggle: ToggleBlock(),
    columns: ColumnsBlock(),
  },
  inlineContentSpecs: {
    ...defaultInlineContentSpecs,
    wikilink: Wikilink,
    inlineEquation: InlineEquation,
  },
});

/**
 * Type helper for the custom editor instance.
 */
export type ScratchEditor = typeof schema.BlockNoteEditor;

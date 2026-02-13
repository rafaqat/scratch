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

/**
 * Shared BlockNote schema that combines:
 * - All default block specs + custom blocks (TOC, Callout, Equation)
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

/**
 * Markdown round-trip utilities for custom inline content and blocks.
 *
 * On import: post-processes a BlockNote block tree to convert:
 * - [[Title]] patterns into wikilink inline content nodes
 * - [toc] paragraphs into TOC block nodes
 * - $$...$$ blocks into equation block nodes
 * - $...$ inline patterns into inlineEquation inline content nodes
 *
 * On export: custom components handle serialization via toExternalHTML.
 * Additional postprocessing restores equation block syntax.
 */

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type AnyBlock = any;
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type AnyInlineContent = any;

const WIKILINK_RE = /\[\[([^\]]+)\]\]/g;
// Match $...$ inline equations, but NOT $$ (block equations)
// Negative lookbehind/lookahead for $ to avoid matching $$...$$ markers
const INLINE_EQUATION_RE = /(?<!\$)\$(?!\$)([^\n$]+?)\$(?!\$)/g;

/**
 * Walk a BlockNote block tree and convert text patterns
 * into custom inline content and block nodes.
 */
export function injectWikilinks(blocks: AnyBlock[]): AnyBlock[] {
  const result: AnyBlock[] = [];
  let i = 0;

  while (i < blocks.length) {
    const block = blocks[i];

    // Check for $$...$$ block equation pattern
    const eqResult = tryExtractBlockEquation(blocks, i);
    if (eqResult) {
      result.push({
        type: "equation",
        props: { equation: eqResult.equation },
        children: [],
      });
      i = eqResult.nextIndex;
      continue;
    }

    // Convert [toc] paragraph blocks into TOC blocks
    if (block.type === "paragraph" && isTocParagraph(block)) {
      result.push({ type: "toc", props: {}, children: [] });
      i++;
      continue;
    }

    const processed = { ...block };

    // Process inline content if present
    if (Array.isArray(processed.content)) {
      processed.content = processInlineContent(processed.content);
    }

    // Process table cells
    if (processed.type === "table" && processed.content?.type === "tableContent") {
      processed.content = {
        ...processed.content,
        rows: processed.content.rows.map((row: { cells: AnyInlineContent[] }) => ({
          ...row,
          cells: row.cells.map((cell: AnyInlineContent) => {
            // Cell may be { type: "tableCell", content: [...], props: {...} }
            if (cell && typeof cell === "object" && Array.isArray(cell.content)) {
              return { ...cell, content: processInlineContent(cell.content) };
            }
            // Or a flat array of inline content
            if (Array.isArray(cell)) {
              return processInlineContent(cell);
            }
            return cell;
          }),
        })),
      };
    }

    // Recurse into children
    if (Array.isArray(processed.children) && processed.children.length > 0) {
      processed.children = injectWikilinks(processed.children);
    }

    result.push(processed);
    i++;
  }

  return result;
}

/**
 * Try to extract a block equation ($$...$$) starting at blocks[index].
 * Returns the equation text and the next index to continue from,
 * or null if no block equation found.
 */
function tryExtractBlockEquation(
  blocks: AnyBlock[],
  index: number,
): { equation: string; nextIndex: number } | null {
  const block = blocks[index];
  if (block.type !== "paragraph" || !Array.isArray(block.content)) return null;

  const text = getBlockText(block);

  // Case 1: Single block $$equation$$
  const singleMatch = text.match(/^\$\$([\s\S]*?)\$\$$/);
  if (singleMatch) {
    return { equation: singleMatch[1].trim(), nextIndex: index + 1 };
  }

  // Case 2: Block starts with $$ - look for closing $$ in subsequent blocks
  if (text.trimStart().startsWith("$$")) {
    const lines: string[] = [text.trimStart().slice(2)];

    for (let j = index + 1; j < blocks.length; j++) {
      const nextBlock = blocks[j];
      if (nextBlock.type !== "paragraph") break;

      const nextText = getBlockText(nextBlock);

      if (nextText.trimEnd().endsWith("$$")) {
        lines.push(nextText.trimEnd().slice(0, -2));
        const equation = lines.join("\n").trim();
        return { equation, nextIndex: j + 1 };
      }

      lines.push(nextText);
    }
  }

  return null;
}

/**
 * Check if a paragraph block contains exactly `[toc]` as its text content.
 */
function isTocParagraph(block: AnyBlock): boolean {
  if (!Array.isArray(block.content)) return false;
  if (block.content.length !== 1) return false;
  const node = block.content[0];
  if (node.type !== "text" || typeof node.text !== "string") return false;
  return node.text.trim() === "[toc]";
}

/**
 * Extract plain text from a block's inline content.
 */
function getBlockText(block: AnyBlock): string {
  if (!Array.isArray(block.content)) return "";
  return block.content
    .map((node: AnyInlineContent) => {
      if (typeof node === "string") return node;
      if (node.type === "text" && typeof node.text === "string") return node.text;
      return "";
    })
    .join("");
}

function processInlineContent(content: AnyInlineContent[]): AnyInlineContent[] {
  const result: AnyInlineContent[] = [];

  for (const node of content) {
    if (node.type !== "text" || typeof node.text !== "string") {
      result.push(node);
      continue;
    }

    if (node.styles?.code) {
      result.push(node);
      continue;
    }

    const text = node.text as string;
    const styles = node.styles || {};

    const segments = splitInlinePatterns(text, styles);
    result.push(...segments);
  }

  return result;
}

/**
 * Split text into segments, replacing [[wikilink]] and $equation$ patterns
 * with their respective inline content nodes.
 */
function splitInlinePatterns(text: string, styles: Record<string, unknown>): AnyInlineContent[] {
  const result: AnyInlineContent[] = [];

  const matches: Array<{
    index: number;
    length: number;
    node: AnyInlineContent;
  }> = [];

  // Wikilinks: [[Title]] or [[Title|alias]]
  WIKILINK_RE.lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = WIKILINK_RE.exec(text)) !== null) {
    const inner = match[1].trim();
    if (!inner) continue;

    const pipeIndex = inner.indexOf("|");
    const title = pipeIndex >= 0 ? inner.slice(0, pipeIndex).trim() : inner;
    const alias = pipeIndex >= 0 ? inner.slice(pipeIndex + 1).trim() : "";

    if (!title) continue;

    matches.push({
      index: match.index,
      length: match[0].length,
      node: { type: "wikilink", props: { title, alias } },
    });
  }

  // Inline equations: $...$
  INLINE_EQUATION_RE.lastIndex = 0;
  while ((match = INLINE_EQUATION_RE.exec(text)) !== null) {
    const equation = match[1].trim();
    if (!equation) continue;

    const overlaps = matches.some(
      (m) =>
        (match!.index >= m.index && match!.index < m.index + m.length) ||
        (m.index >= match!.index && m.index < match!.index + match![0].length),
    );
    if (overlaps) continue;

    matches.push({
      index: match.index,
      length: match[0].length,
      node: { type: "inlineEquation", props: { equation } },
    });
  }

  if (matches.length === 0) {
    result.push({ type: "text", text, styles });
    return result;
  }

  matches.sort((a, b) => a.index - b.index);

  let lastIndex = 0;
  for (const m of matches) {
    if (m.index > lastIndex) {
      result.push({
        type: "text",
        text: text.slice(lastIndex, m.index),
        styles,
      });
    }

    result.push(m.node);
    lastIndex = m.index + m.length;
  }

  if (lastIndex < text.length) {
    result.push({
      type: "text",
      text: text.slice(lastIndex),
      styles,
    });
  }

  return result;
}

/**
 * Convert wikilink references back to [[Title]] in markdown text.
 */
export function restoreWikilinks(markdown: string): string {
  return markdown;
}

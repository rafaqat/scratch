/**
 * Wikilink markdown round-trip utilities.
 *
 * On import: post-processes a BlockNote block tree to convert text
 * containing [[Title]] patterns into wikilink inline content nodes.
 *
 * On export: converts wikilink inline content back to [[Title]] text.
 * (BlockNote's blocksToMarkdownLossy renders unknown inline content
 * via toExternalHTML, so we handle it there instead.)
 */

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type AnyBlock = any;
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type AnyInlineContent = any;

const WIKILINK_RE = /\[\[([^\]]+)\]\]/g;

/**
 * Walk a BlockNote block tree and convert [[Title]] text patterns
 * in inline content arrays into wikilink inline content nodes.
 * Also converts `[toc]` paragraphs into TOC block nodes.
 */
export function injectWikilinks(blocks: AnyBlock[]): AnyBlock[] {
  return blocks.map((block) => {
    // Convert [toc] paragraph blocks into TOC blocks
    if (block.type === "paragraph" && isTocParagraph(block)) {
      return { type: "toc", props: {}, children: [] };
    }

    const result = { ...block };

    // Process inline content if present
    if (Array.isArray(result.content)) {
      result.content = processInlineContent(result.content);
    }

    // Process table cells
    if (result.type === "table" && result.content?.type === "tableContent") {
      result.content = {
        ...result.content,
        rows: result.content.rows.map((row: { cells: AnyInlineContent[][] }) => ({
          ...row,
          cells: row.cells.map((cell: AnyInlineContent[]) =>
            processInlineContent(cell),
          ),
        })),
      };
    }

    // Recurse into children
    if (Array.isArray(result.children) && result.children.length > 0) {
      result.children = injectWikilinks(result.children);
    }

    return result;
  });
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

function processInlineContent(content: AnyInlineContent[]): AnyInlineContent[] {
  const result: AnyInlineContent[] = [];

  for (const node of content) {
    // Only process text nodes (not links, not existing wikilinks)
    if (node.type !== "text" || typeof node.text !== "string") {
      result.push(node);
      continue;
    }

    // Skip text nodes that are styled as code (inline code)
    if (node.styles?.code) {
      result.push(node);
      continue;
    }

    const text = node.text as string;
    const styles = node.styles || {};

    let lastIndex = 0;
    let match: RegExpExecArray | null;
    WIKILINK_RE.lastIndex = 0;
    let hasMatch = false;

    while ((match = WIKILINK_RE.exec(text)) !== null) {
      const inner = match[1].trim();
      if (!inner) continue; // skip empty [[]]

      // Support alias syntax: [[Title|display text]]
      const pipeIndex = inner.indexOf("|");
      const title = pipeIndex >= 0 ? inner.slice(0, pipeIndex).trim() : inner;
      const alias = pipeIndex >= 0 ? inner.slice(pipeIndex + 1).trim() : "";

      if (!title) continue;

      hasMatch = true;

      // Add text before the match
      if (match.index > lastIndex) {
        result.push({
          type: "text",
          text: text.slice(lastIndex, match.index),
          styles,
        });
      }

      // Add wikilink node
      result.push({
        type: "wikilink",
        props: { title, alias },
      });

      lastIndex = match.index + match[0].length;
    }

    if (hasMatch) {
      // Add remaining text after the last match
      if (lastIndex < text.length) {
        result.push({
          type: "text",
          text: text.slice(lastIndex),
          styles,
        });
      }
    } else {
      // No matches, keep original node
      result.push(node);
    }
  }

  return result;
}

/**
 * Convert wikilink references back to [[Title]] in markdown text.
 * This is used as a safety net for the export path — normally
 * toExternalHTML handles this via the Wikilink component's
 * toExternalHTML render function.
 */
export function restoreWikilinks(markdown: string): string {
  // No-op: the toExternalHTML render in Wikilink.tsx outputs [[Title]]
  // which survives the HTML → markdown conversion. This function exists
  // as a hook point if we ever need post-processing.
  return markdown;
}

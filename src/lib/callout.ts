/**
 * Callout markdown round-trip utilities.
 *
 * On import (markdown -> blocks): Converts GFM alert blockquote syntax
 *   > [!NOTE]
 *   > Content here
 * into callout block JSON that BlockNote can render.
 *
 * On export (blocks -> markdown): Converts callout block HTML output
 * back into GFM alert syntax.
 */

import type { CalloutType } from "../components/editor/Callout";
import { CALLOUT_TYPES, GFM_TO_CALLOUT } from "../components/editor/Callout";

/**
 * Regex matching the GFM alert/callout opening line: > [!TYPE]
 * Captures the type keyword (NOTE, TIP, WARNING, CAUTION, IMPORTANT)
 */
const GFM_CALLOUT_RE = /^>\s*\[!(NOTE|TIP|WARNING|CAUTION|IMPORTANT)\]\s*$/i;

/**
 * Pre-process markdown before passing to BlockNote's parser.
 * Converts GFM callout blockquotes into HTML <div> blocks that
 * BlockNote's HTML parser can pick up via the callout block's parse function.
 *
 * Input:
 *   > [!NOTE]
 *   > This is a note callout
 *
 * Output:
 *   <div data-callout-type="info">This is a note callout</div>
 */
export function preprocessCallouts(markdown: string): string {
  const lines = markdown.split("\n");
  const result: string[] = [];
  let i = 0;

  while (i < lines.length) {
    const match = lines[i].match(GFM_CALLOUT_RE);
    if (match) {
      const gfmType = match[1].toUpperCase();
      const calloutType: CalloutType = GFM_TO_CALLOUT[gfmType] || "info";

      // Collect all subsequent blockquote lines as content
      const contentLines: string[] = [];
      i++;
      while (i < lines.length) {
        const line = lines[i];
        // Continue collecting lines that are part of the blockquote
        if (line.startsWith("> ")) {
          contentLines.push(line.slice(2));
          i++;
        } else if (line === ">") {
          // Empty blockquote line
          contentLines.push("");
          i++;
        } else {
          break;
        }
      }

      const content = contentLines.join("\n").trim();

      // Emit an HTML block that BlockNote's parser will pick up
      result.push(
        `<div data-callout-type="${calloutType}">${escapeHtml(content)}</div>`,
      );
      result.push("");
    } else {
      result.push(lines[i]);
      i++;
    }
  }

  return result.join("\n");
}

/**
 * Post-process markdown output from BlockNote's blocksToMarkdownLossy.
 * Converts callout HTML blocks back to GFM alert syntax.
 *
 * BlockNote's blocksToMarkdownLossy will output something like:
 *   <div data-callout-type="info" data-callout-gfm="NOTE">content</div>
 *
 * We convert that back to:
 *   > [!NOTE]
 *   > content
 */
export function postprocessCallouts(markdown: string): string {
  // Match the HTML div tags that our toExternalHTML emits
  const calloutHtmlRe =
    /<div data-callout-type="([^"]*)"(?: data-callout-gfm="([^"]*)")?>([\s\S]*?)<\/div>/g;

  return markdown.replace(calloutHtmlRe, (_match, typeStr, gfmStr, content) => {
    const calloutType = typeStr as CalloutType;
    const gfm =
      gfmStr || CALLOUT_TYPES[calloutType]?.gfm || "NOTE";
    const unescaped = unescapeHtml(content.trim());
    const contentLines = unescaped.split("\n");
    const quotedLines = contentLines.map((line: string) =>
      line ? `> ${line}` : ">",
    );
    return `> [!${gfm}]\n${quotedLines.join("\n")}`;
  });
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function unescapeHtml(text: string): string {
  return text
    .replace(/&quot;/g, '"')
    .replace(/&gt;/g, ">")
    .replace(/&lt;/g, "<")
    .replace(/&amp;/g, "&");
}

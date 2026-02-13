/**
 * Equation markdown round-trip utilities.
 *
 * On import (markdown -> blocks): Handled by injectWikilinks in wikilink.ts:
 * - Block equations ($$...$$) are converted to equation block nodes
 * - Inline equations ($...$) are converted to inlineEquation inline content nodes
 *
 * On export (blocks -> markdown): This module provides postprocessEquations()
 * to clean up the HTML output from BlockNote's blocksToMarkdownLossy and
 * ensure proper $$...$$ / $...$ markdown syntax.
 */

/**
 * Post-process markdown output from BlockNote's blocksToMarkdownLossy.
 * Converts equation block HTML back to proper $$...$$ markdown syntax.
 *
 * BlockNote's blocksToMarkdownLossy will output something like:
 *   <div data-equation-block="true">$$\nE=mc^2\n$$</div>
 *
 * We convert that back to:
 *   $$
 *   E=mc^2
 *   $$
 */
export function postprocessEquations(markdown: string): string {
  // Match the HTML div tags that our toExternalHTML emits
  const equationHtmlRe =
    /<div data-equation-block="true">([\s\S]*?)<\/div>/g;

  return markdown.replace(equationHtmlRe, (_match, content: string) => {
    // The content should already be $$\nequation\n$$, just clean it up
    const trimmed = content.trim();
    // If content already has $$ delimiters, use as-is
    if (trimmed.startsWith("$$") && trimmed.endsWith("$$")) {
      return trimmed;
    }
    // Otherwise wrap in $$
    return `$$\n${trimmed}\n$$`;
  });
}

/**
 * Pre-process markdown before passing to BlockNote's parser.
 * Converts $$...$$ block equations to HTML that BlockNote can parse
 * via the equation block's parse function.
 *
 * This is optional — the block-level injection in wikilink.ts handles
 * the conversion at the block tree level. This preprocessor provides
 * an additional HTML-based path for the parse function.
 */
export function preprocessEquations(markdown: string): string {
  // No preprocessing needed — block equations are handled at the block
  // tree level by injectWikilinks in wikilink.ts, which converts
  // paragraphs containing $$...$$ into equation block nodes.
  return markdown;
}

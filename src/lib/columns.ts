/**
 * Preprocessor / postprocessor for :::columns markdown blocks.
 *
 * Markdown format:
 * :::columns
 * ::: col
 * Left content
 * :::
 * ::: col
 * Right content
 * :::
 * :::
 *
 * In editor, stored as a columns block with JSON props.
 */

/**
 * Preprocess: convert :::columns markdown into HTML data attributes for the parser.
 * This runs before the markdown is parsed by BlockNote.
 */
export function preprocessColumns(md: string): string {
  // Match :::columns ... ::: blocks
  const regex = /^:::columns\s*\n([\s\S]*?)^:::\s*$/gm;
  return md.replace(regex, (_match, inner: string) => {
    // Split into columns by "::: col"
    const colBlocks = inner.split(/^::: col\s*$/gm).filter((s) => s.trim());
    // Each column content ends with ":::" - remove trailing :::
    const columns = colBlocks.map((block) => {
      return block.replace(/^:::\s*$/gm, "").trim();
    });
    const json = JSON.stringify(columns);
    // Return an HTML placeholder that the block parser can pick up
    return `<div data-columns='${json.replace(/'/g, "&#39;")}'></div>`;
  });
}

/**
 * Postprocess: convert columns block back to :::columns markdown format.
 * This runs after BlockNote serializes to markdown.
 */
export function postprocessColumns(md: string): string {
  // BlockNote will serialize columns blocks - we need to detect our format
  // Since our block uses content: "none", it may serialize as empty or with props
  // We handle this in the getMarkdown postprocessing
  return md;
}

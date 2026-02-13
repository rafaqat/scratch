/**
 * Database reference markdown round-trip utilities.
 *
 * Markdown format for database table views:
 *   [database:my-tasks](view:table)
 *
 * On import (markdown -> blocks): preprocessDatabaseRefs converts the
 * markdown link-style reference into an HTML div that BlockNote's parser
 * picks up via the databaseTable block's parse function.
 *
 * On export (blocks -> markdown): postprocessDatabaseRefs converts the
 * block's HTML output back into the markdown reference format.
 */

/**
 * Pre-process markdown before passing to BlockNote's parser.
 * Converts database reference links into HTML div blocks.
 *
 * Input:
 *   [database:my-tasks](view:table)
 *
 * Output:
 *   <p data-database-table="my-tasks">[database:my-tasks](view:table)</p>
 */
export function preprocessDatabaseRefs(markdown: string): string {
  // Match [database:name](view:table) on its own line (or inline)
  return markdown.replace(
    /\[database:([^\]]+)\]\(view:table\)/g,
    (_match, dbName: string) => {
      return `<p data-database-table="${escapeAttr(dbName)}">[database:${dbName}](view:table)</p>`;
    },
  );
}

/**
 * Post-process markdown output from BlockNote's blocksToMarkdownLossy.
 * Converts database table HTML blocks back to markdown reference format.
 *
 * Input:
 *   <p data-database-table="my-tasks">[database:my-tasks](view:table)</p>
 *
 * Output:
 *   [database:my-tasks](view:table)
 */
export function postprocessDatabaseRefs(markdown: string): string {
  // Match the HTML output from toExternalHTML
  return markdown.replace(
    /<p data-database-table="([^"]*)">[^<]*<\/p>/g,
    (_match, dbName: string) => {
      const name = dbName
        .replace(/&quot;/g, '"')
        .replace(/&lt;/g, "<")
        .replace(/&gt;/g, ">")
        .replace(/&amp;/g, "&");
      return `[database:${name}](view:table)`;
    },
  );
}

function escapeAttr(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/"/g, "&quot;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

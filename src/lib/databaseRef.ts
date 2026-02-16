/**
 * Database reference markdown round-trip utilities.
 *
 * Markdown format for database views:
 *   [database:my-tasks](view:table)
 *   [database:my-tasks](view:calendar)
 *
 * On import (markdown -> blocks): preprocessDatabaseRefs converts the
 * markdown link into a plain-text %%DB:name:view%% marker that survives
 * BlockNote's parser as a paragraph. injectDatabaseBlocks (in Editor.tsx)
 * then converts those paragraphs into databaseTable blocks.
 *
 * On export (blocks -> markdown): postprocessDatabaseRefs converts the
 * block's HTML output back into the markdown reference format.
 */

/**
 * Pre-process markdown before passing to BlockNote's parser.
 * Converts database reference links into plain-text markers.
 *
 * Input:  [database:my-tasks](view:table)
 * Output: %%DB:my-tasks:table%%
 */
export function preprocessDatabaseRefs(markdown: string): string {
  return markdown.replace(
    /\[database:([^\]]+)\]\(view:(\w+)\)/g,
    (_match, dbName: string, viewType: string) => {
      return `%%DB:${dbName}:${viewType}%%`;
    },
  );
}

/**
 * Post-process markdown output from BlockNote's blocksToMarkdownLossy.
 * Converts database table HTML blocks back to markdown reference format.
 */
export function postprocessDatabaseRefs(markdown: string): string {
  // Match the HTML output from toExternalHTML (div or p, with or without view attr)
  return markdown.replace(
    /<(?:div|p) data-database-table="([^"]*)"(?:\s+data-database-view="([^"]*)")?(?:>[^<]*<\/(?:div|p)>|\s*\/>)/g,
    (_match, dbName: string, viewType?: string) => {
      const name = dbName
        .replace(/&quot;/g, '"')
        .replace(/&lt;/g, "<")
        .replace(/&gt;/g, ">")
        .replace(/&amp;/g, "&");
      const view = viewType || "table";
      return `[database:${name}](view:${view})`;
    },
  );
}

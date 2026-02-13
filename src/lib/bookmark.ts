/**
 * Bookmark markdown round-trip utilities.
 *
 * Markdown format for bookmarks:
 *   <!-- bookmark:{"title":"...","description":"...","favicon":"...","domain":"..."} -->
 *   <https://example.com>
 *
 * On import (markdown -> blocks): preprocessBookmarks converts the HTML comment +
 * URL pattern into an HTML div that BlockNote's parser picks up via the bookmark
 * block's parse function.
 *
 * On export (blocks -> markdown): postprocessBookmarks converts the bookmark block's
 * HTML output back into the comment + URL format.
 */

interface BookmarkMeta {
  title?: string;
  description?: string;
  favicon?: string;
  domain?: string;
}

/**
 * Pre-process markdown before passing to BlockNote's parser.
 * Converts bookmark comment + URL patterns into HTML div blocks.
 *
 * Input:
 *   <!-- bookmark:{"title":"Example","description":"A site","favicon":"...","domain":"example.com"} -->
 *   <https://example.com>
 *
 * Output:
 *   <div data-bookmark-block="true" data-bookmark-url="https://example.com" ...>https://example.com</div>
 */
export function preprocessBookmarks(markdown: string): string {
  const lines = markdown.split("\n");
  const result: string[] = [];
  let i = 0;

  while (i < lines.length) {
    const commentMatch = lines[i].match(
      /^<!--\s*bookmark:(.*?)\s*-->$/
    );
    if (commentMatch && i + 1 < lines.length) {
      const nextLine = lines[i + 1].trim();
      // Match <URL> or bare URL on the next line
      const urlMatch = nextLine.match(/^<(https?:\/\/[^>]+)>$/) ||
        nextLine.match(/^(https?:\/\/\S+)$/);
      if (urlMatch) {
        const url = urlMatch[1];
        let meta: BookmarkMeta = {};
        try {
          meta = JSON.parse(commentMatch[1]);
        } catch {
          // Ignore parse errors, use defaults
        }

        const attrs = [
          `data-bookmark-block="true"`,
          `data-bookmark-url="${escapeAttr(url)}"`,
          `data-bookmark-title="${escapeAttr(meta.title || "")}"`,
          `data-bookmark-description="${escapeAttr(meta.description || "")}"`,
          `data-bookmark-favicon="${escapeAttr(meta.favicon || "")}"`,
          `data-bookmark-domain="${escapeAttr(meta.domain || "")}"`,
        ].join(" ");

        result.push(`<div ${attrs}>${escapeHtml(url)}</div>`);
        result.push("");
        i += 2;
        continue;
      }
    }

    result.push(lines[i]);
    i++;
  }

  return result.join("\n");
}

/**
 * Post-process markdown output from BlockNote's blocksToMarkdownLossy.
 * Converts bookmark HTML blocks back to comment + URL format.
 *
 * Input:
 *   <div data-bookmark-block="true" data-bookmark-url="..." ...>...</div>
 *
 * Output:
 *   <!-- bookmark:{"title":"...","description":"...","favicon":"...","domain":"..."} -->
 *   <https://...>
 */
export function postprocessBookmarks(markdown: string): string {
  const bookmarkHtmlRe =
    /<div data-bookmark-block="true"([^>]*)>[^<]*<\/div>/g;

  return markdown.replace(bookmarkHtmlRe, (_match, attrsStr: string) => {
    const url = extractAttr(attrsStr, "data-bookmark-url");
    const title = extractAttr(attrsStr, "data-bookmark-title");
    const description = extractAttr(attrsStr, "data-bookmark-description");
    const favicon = extractAttr(attrsStr, "data-bookmark-favicon");
    const domain = extractAttr(attrsStr, "data-bookmark-domain");

    const meta: BookmarkMeta = {};
    if (title) meta.title = title;
    if (description) meta.description = description;
    if (favicon) meta.favicon = favicon;
    if (domain) meta.domain = domain;

    const metaJson = JSON.stringify(meta);
    return `<!-- bookmark:${metaJson} -->\n<${url}>`;
  });
}

function escapeAttr(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/"/g, "&quot;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

function extractAttr(attrsStr: string, name: string): string {
  const re = new RegExp(`${name}="([^"]*)"`, "i");
  const match = attrsStr.match(re);
  if (!match) return "";
  return match[1]
    .replace(/&quot;/g, '"')
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&amp;/g, "&");
}

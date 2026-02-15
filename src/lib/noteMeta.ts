/**
 * Generic note metadata stored in YAML frontmatter.
 * Used for page decoration (icon, cover) and layout (wide) on regular notes.
 * Story notes use their own frontmatter format (see frontmatter.ts).
 */

export interface NotePageMeta {
  icon?: string;
  wide?: boolean;
  cover?: string;
  cover_position?: number;
}

const DELIMITER = "---";

/**
 * Parse generic note metadata from frontmatter.
 * Returns null if:
 * - No frontmatter found
 * - Frontmatter contains story fields (id + title + status) — handled by story parser
 * - No recognized page meta fields present
 */
export function parseNoteMeta(
  content: string
): { meta: NotePageMeta; body: string } | null {
  if (!content.startsWith(DELIMITER + "\n")) return null;

  const endIdx = content.indexOf("\n" + DELIMITER + "\n", DELIMITER.length);
  const endIdx2 = content.indexOf("\n" + DELIMITER, DELIMITER.length);

  let yamlStr: string;
  let body: string;

  if (endIdx !== -1) {
    yamlStr = content.slice(DELIMITER.length + 1, endIdx);
    body = content.slice(endIdx + 1 + DELIMITER.length + 1);
  } else if (endIdx2 !== -1 && endIdx2 + 1 + DELIMITER.length === content.length) {
    yamlStr = content.slice(DELIMITER.length + 1, endIdx2);
    body = "";
  } else {
    return null;
  }

  // Skip if this is a story file (has id + title + status)
  if (
    /^id:\s/m.test(yamlStr) &&
    /^title:\s/m.test(yamlStr) &&
    /^status:\s/m.test(yamlStr)
  ) {
    return null;
  }

  const meta: NotePageMeta = {};
  for (const line of yamlStr.split("\n")) {
    const match = line.match(/^(\w+):\s*(.*)/);
    if (!match) continue;
    const [, key, rawValue] = match;
    const value = rawValue.trim().replace(/^["']|["']$/g, "");

    switch (key) {
      case "icon":
        if (value) meta.icon = value;
        break;
      case "wide":
        meta.wide = value === "true";
        break;
      case "cover":
        if (value) meta.cover = value;
        break;
      case "cover_position":
        if (value) meta.cover_position = Number(value) || 50;
        break;
    }
  }

  // Only return if we found at least one recognized field
  if (Object.keys(meta).length === 0) return null;

  return { meta, body };
}

/**
 * Serialize note page metadata to YAML frontmatter string.
 * Returns empty string if no meta fields are set.
 */
export function serializeNoteMeta(meta: NotePageMeta): string {
  const lines: string[] = [];
  if (meta.icon) lines.push(`icon: "${meta.icon}"`);
  if (meta.wide) lines.push("wide: true");
  if (meta.cover) lines.push(`cover: "${meta.cover}"`);
  if (meta.cover_position !== undefined && meta.cover_position !== 50) {
    lines.push(`cover_position: ${meta.cover_position}`);
  }

  if (lines.length === 0) return "";
  return DELIMITER + "\n" + lines.join("\n") + "\n" + DELIMITER + "\n";
}

/**
 * Recombine note meta with body content.
 */
export function recombineNoteMeta(meta: NotePageMeta, body: string): string {
  const header = serializeNoteMeta(meta);
  if (!header) return body;
  return header + body;
}

export interface StoryFrontmatter {
  id: string;
  epic: string;
  title: string;
  status: string;
  owner?: string;
  estimate_points?: number;
  tags?: string[];
  links?: Record<string, string>;
  timestamps: { created_at: string; updated_at: string };
}

const DELIMITER = "---";
const STORY_STATUSES = ["Backlog", "Ready", "In Progress", "In Review", "Done", "Blocked"];

function trimQuotes(s: string): string {
  if ((s.startsWith('"') && s.endsWith('"')) || (s.startsWith("'") && s.endsWith("'"))) {
    return s.slice(1, -1);
  }
  return s;
}

function parseYamlValue(raw: string): string | number | boolean | null {
  const trimmed = raw.trim();
  if (trimmed === "" || trimmed === "null" || trimmed === "~") return null;
  if (trimmed === "true") return true;
  if (trimmed === "false") return false;
  const num = Number(trimmed);
  if (!isNaN(num) && trimmed !== "") return num;
  return trimQuotes(trimmed);
}

export function parseFrontmatter(
  content: string
): { frontmatter: StoryFrontmatter; body: string } | null {
  if (!content.startsWith(DELIMITER + "\n")) return null;

  const endIdx = content.indexOf("\n" + DELIMITER + "\n", DELIMITER.length);
  if (endIdx === -1) {
    // Check for --- at very end with no trailing newline
    const endIdx2 = content.indexOf("\n" + DELIMITER, DELIMITER.length);
    if (endIdx2 === -1 || endIdx2 + 1 + DELIMITER.length !== content.length) return null;
    // frontmatter with no body
    const yamlStr = content.slice(DELIMITER.length + 1, endIdx2);
    const fm = parseYamlBlock(yamlStr);
    if (!fm) return null;
    return { frontmatter: fm, body: "" };
  }

  const yamlStr = content.slice(DELIMITER.length + 1, endIdx);
  const body = content.slice(endIdx + 1 + DELIMITER.length + 1);
  const fm = parseYamlBlock(yamlStr);
  if (!fm) return null;
  return { frontmatter: fm, body };
}

function parseYamlBlock(yaml: string): StoryFrontmatter | null {
  const lines = yaml.split("\n");
  const result: Record<string, unknown> = {};
  let currentKey: string | null = null;
  let currentArray: string[] | null = null;
  let currentObject: Record<string, string> | null = null;

  for (const line of lines) {
    if (line.trim() === "") continue;

    // Array item (- value) under a key
    if (/^\s*-\s+/.test(line) && currentKey && currentArray !== null) {
      const val = line.replace(/^\s*-\s+/, "").trim();
      currentArray.push(trimQuotes(val));
      continue;
    }

    // Nested object key (  key: value) under a key
    if (/^\s{2,}\S/.test(line) && currentKey && currentObject !== null) {
      const match = line.match(/^\s+(\S+):\s*(.*)/);
      if (match) {
        currentObject[match[1]] = trimQuotes(match[2].trim());
      }
      continue;
    }

    // Flush previous collection
    if (currentKey && currentArray !== null) {
      result[currentKey] = currentArray;
      currentArray = null;
    }
    if (currentKey && currentObject !== null) {
      result[currentKey] = currentObject;
      currentObject = null;
    }

    // Top-level key: value
    const kvMatch = line.match(/^(\S+):\s*(.*)/);
    if (!kvMatch) continue;

    const key = kvMatch[1];
    const rawValue = kvMatch[2].trim();

    if (rawValue === "" || rawValue === undefined) {
      // Could be start of array or object — peek ahead
      currentKey = key;
      currentArray = [];
      currentObject = {};
    } else {
      currentKey = null;
      currentArray = null;
      currentObject = null;
      result[key] = parseYamlValue(rawValue);
    }
  }

  // Flush final collection
  if (currentKey && currentArray !== null && currentArray.length > 0) {
    result[currentKey] = currentArray;
  } else if (currentKey && currentObject !== null && Object.keys(currentObject).length > 0) {
    result[currentKey] = currentObject;
  }

  // Validate required story fields
  if (!result.id || !result.epic || !result.title || !result.status || !result.timestamps) {
    return null;
  }

  const ts = result.timestamps as Record<string, string>;
  if (!ts.created_at || !ts.updated_at) return null;

  return {
    id: String(result.id),
    epic: String(result.epic),
    title: String(result.title),
    status: String(result.status),
    owner: result.owner ? String(result.owner) : undefined,
    estimate_points: typeof result.estimate_points === "number" ? result.estimate_points : undefined,
    tags: Array.isArray(result.tags) ? result.tags : undefined,
    links: result.links && typeof result.links === "object" && !Array.isArray(result.links)
      ? (result.links as Record<string, string>)
      : undefined,
    timestamps: { created_at: ts.created_at, updated_at: ts.updated_at },
  };
}

export function serializeFrontmatter(fm: StoryFrontmatter): string {
  const lines: string[] = [];
  lines.push(`id: ${fm.id}`);
  lines.push(`epic: ${fm.epic}`);
  lines.push(`title: ${fm.title}`);
  lines.push(`status: ${fm.status}`);
  if (fm.owner) lines.push(`owner: ${fm.owner}`);
  if (fm.estimate_points !== undefined) lines.push(`estimate_points: ${fm.estimate_points}`);
  if (fm.tags && fm.tags.length > 0) {
    lines.push("tags:");
    for (const tag of fm.tags) {
      lines.push(`- ${tag}`);
    }
  }
  if (fm.links && Object.keys(fm.links).length > 0) {
    lines.push("links:");
    for (const [k, v] of Object.entries(fm.links)) {
      lines.push(`  ${k}: ${v}`);
    }
  }
  lines.push("timestamps:");
  lines.push(`  created_at: ${fm.timestamps.created_at}`);
  lines.push(`  updated_at: ${fm.timestamps.updated_at}`);

  return DELIMITER + "\n" + lines.join("\n") + "\n" + DELIMITER;
}

export function isStoryFrontmatter(content: string): boolean {
  return parseFrontmatter(content) !== null;
}

export function recombine(fm: StoryFrontmatter, body: string): string {
  const header = serializeFrontmatter(fm);
  if (!body || body.trim() === "") return header + "\n";
  return header + "\n" + body;
}

export { STORY_STATUSES };

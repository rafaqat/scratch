export interface StoryFrontmatter {
  id: string;
  title: string;
  status: string;
  estimate_points?: number;
  tags?: string[];
  owner?: string;
  created_at?: string;
  started_at?: string;
  completed_at?: string;
  target_version?: string;
  commits?: string[];
  links?: {
    epic?: string;
    blocks?: string[];
    blockedBy?: string[];
  };
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

// Parse inline array: [a, b, c]
function parseInlineArray(raw: string): string[] | null {
  const trimmed = raw.trim();
  if (!trimmed.startsWith("[") || !trimmed.endsWith("]")) return null;
  const inner = trimmed.slice(1, -1).trim();
  if (inner === "") return [];
  return inner.split(",").map((s) => trimQuotes(s.trim()));
}

export function parseFrontmatter(
  content: string
): { frontmatter: StoryFrontmatter; body: string } | null {
  if (!content.startsWith(DELIMITER + "\n")) return null;

  const endIdx = content.indexOf("\n" + DELIMITER + "\n", DELIMITER.length);
  if (endIdx === -1) {
    const endIdx2 = content.indexOf("\n" + DELIMITER, DELIMITER.length);
    if (endIdx2 === -1 || endIdx2 + 1 + DELIMITER.length !== content.length) return null;
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
  let currentObject: Record<string, unknown> | null = null;

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
        const nestedRaw = match[2].trim();
        // Check for inline array in nested value
        const inlineArr = parseInlineArray(nestedRaw);
        if (inlineArr !== null) {
          currentObject[match[1]] = inlineArr;
        } else {
          currentObject[match[1]] = trimQuotes(nestedRaw);
        }
      }
      continue;
    }

    // Flush previous collection
    if (currentKey && currentArray !== null && currentArray.length > 0) {
      result[currentKey] = currentArray;
    } else if (currentKey && currentObject !== null && Object.keys(currentObject).length > 0) {
      result[currentKey] = currentObject;
    }
    currentArray = null;
    currentObject = null;

    // Top-level key: value
    const kvMatch = line.match(/^(\S+):\s*(.*)/);
    if (!kvMatch) continue;

    const key = kvMatch[1];
    const rawValue = kvMatch[2].trim();

    // Check for inline array at top level
    const inlineArr = parseInlineArray(rawValue);
    if (inlineArr !== null) {
      currentKey = null;
      result[key] = inlineArr;
      continue;
    }

    if (rawValue === "" || rawValue === undefined) {
      currentKey = key;
      currentArray = [];
      currentObject = {};
    } else {
      currentKey = null;
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
  if (!result.id || !result.title || !result.status) {
    return null;
  }

  // Build links object
  let links: StoryFrontmatter["links"] = undefined;
  if (result.links && typeof result.links === "object" && !Array.isArray(result.links)) {
    const raw = result.links as Record<string, unknown>;
    links = {
      epic: typeof raw.epic === "string" ? raw.epic : undefined,
      blocks: Array.isArray(raw.blocks) ? raw.blocks as string[] : undefined,
      blockedBy: Array.isArray(raw.blockedBy) ? raw.blockedBy as string[] : undefined,
    };
  }

  return {
    id: String(result.id),
    title: String(result.title),
    status: String(result.status),
    estimate_points: typeof result.estimate_points === "number" ? result.estimate_points : undefined,
    tags: Array.isArray(result.tags) ? result.tags : undefined,
    owner: result.owner ? String(result.owner) : undefined,
    created_at: result.created_at ? String(result.created_at) : undefined,
    started_at: result.started_at ? String(result.started_at) : undefined,
    completed_at: result.completed_at ? String(result.completed_at) : undefined,
    target_version: result.target_version ? String(result.target_version) : undefined,
    commits: Array.isArray(result.commits) ? result.commits : undefined,
    links,
  };
}

export function serializeFrontmatter(fm: StoryFrontmatter): string {
  const lines: string[] = [];
  lines.push(`id: ${fm.id}`);
  lines.push(`title: "${fm.title}"`);
  lines.push(`status: ${fm.status}`);
  if (fm.estimate_points !== undefined) lines.push(`estimate_points: ${fm.estimate_points}`);
  if (fm.tags && fm.tags.length > 0) {
    lines.push(`tags: [${fm.tags.join(", ")}]`);
  }
  lines.push(`owner: "${fm.owner || ""}"`);
  lines.push(`created_at: "${fm.created_at || ""}"`);
  lines.push(`started_at: "${fm.started_at || ""}"`);
  lines.push(`completed_at: "${fm.completed_at || ""}"`);
  lines.push(`target_version: "${fm.target_version || ""}"`);
  if (fm.commits && fm.commits.length > 0) {
    lines.push("commits:");
    for (const c of fm.commits) {
      lines.push(`  - ${c}`);
    }
  } else {
    lines.push("commits: []");
  }
  if (fm.links) {
    lines.push("links:");
    if (fm.links.epic) lines.push(`  epic: ${fm.links.epic}`);
    if (fm.links.blocks && fm.links.blocks.length > 0) {
      lines.push(`  blocks: [${fm.links.blocks.join(", ")}]`);
    }
    if (fm.links.blockedBy && fm.links.blockedBy.length > 0) {
      lines.push(`  blockedBy: [${fm.links.blockedBy.join(", ")}]`);
    }
  }

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

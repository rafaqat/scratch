/**
 * SVG preprocessing for BlockNote editor.
 *
 * Before feeding markdown to BlockNote, SVG code fences and raw SVG blocks
 * are converted to data-URI image markdown so they render visually.
 * On save, data-URI images are converted back to SVG code fences.
 */

const SVG_MARKER = "SVG Diagram";

function svgToDataUri(svg: string): string {
  try {
    return `data:image/svg+xml;base64,${btoa(unescape(encodeURIComponent(svg.trim())))}`;
  } catch {
    return "";
  }
}

function dataUriToSvg(base64: string): string {
  try {
    return decodeURIComponent(escape(atob(base64)));
  } catch {
    return "";
  }
}

/**
 * Convert SVG code blocks and raw <svg> elements to image markdown
 * with data URIs so BlockNote renders them visually.
 *
 * Handles:
 *  - ```svg ... ```
 *  - ```svg artifact="..." title="..." ... ```  (Claude artifacts)
 *  - Raw <svg>...</svg> blocks outside code fences
 *  - Existing data-URI SVG images (left as-is)
 */
export function preprocessSvg(markdown: string): string {
  // Step 1: Process ALL code fences. Convert SVG fences to images,
  // protect non-SVG fences from the raw SVG regex.
  const fences: string[] = [];
  let result = markdown.replace(
    /```(\w*)[^\n]*\n([\s\S]*?)```/g,
    (match, lang, content) => {
      if (/^svg$/i.test(lang)) {
        // Extract just the <svg>...</svg> from the content (ignore non-SVG preamble)
        const svgMatch = content.match(/<svg[\s\S]*<\/svg>/i);
        const svgContent = svgMatch ? svgMatch[0] : content;
        const uri = svgToDataUri(svgContent);
        return uri ? `![${SVG_MARKER}](${uri})` : match;
      }
      // Protect non-SVG code fences
      fences.push(match);
      return `\x00FENCE${fences.length - 1}\x00`;
    },
  );

  // Step 2: Convert raw <svg>...</svg> blocks (outside code fences)
  result = result.replace(/<svg[\s\S]*?<\/svg>/gi, (match) => {
    const uri = svgToDataUri(match);
    return uri ? `![${SVG_MARKER}](${uri})` : match;
  });

  // Step 3: Restore protected code fences
  result = result.replace(/\x00FENCE(\d+)\x00/g, (_, i) => fences[+i]);

  return result;
}

/**
 * Convert data-URI SVG images back to SVG code fences for clean storage.
 */
export function postprocessSvg(markdown: string): string {
  const re = new RegExp(
    `!\\[${escapeRegex(SVG_MARKER)}\\]\\(data:image/svg\\+xml;base64,([A-Za-z0-9+/=]+)\\)`,
    "g",
  );
  return markdown.replace(re, (match, base64) => {
    const svg = dataUriToSvg(base64);
    return svg ? "```svg\n" + svg + "\n```" : match;
  });
}

function escapeRegex(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

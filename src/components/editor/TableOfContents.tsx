import { useState, useEffect, useRef, useCallback } from "react";
import { createReactBlockSpec } from "@blocknote/react";

/**
 * Table of Contents custom block for BlockNote.
 *
 * Scans all heading blocks (H1-H3) in the document and renders
 * a clickable, auto-updating list of heading entries with
 * indentation by level.
 *
 * Serializes as `[toc]` in markdown for round-trip support.
 */

interface HeadingEntry {
  id: string;
  text: string;
  level: number;
}

/**
 * Extract heading entries from the editor's document.
 * Uses `any` for the editor/blocks because this is cross-block
 * introspection (reading heading blocks from within a toc block's render).
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
function extractHeadings(editor: any): HeadingEntry[] {
  const entries: HeadingEntry[] = [];
  for (const block of editor.document) {
    if (block.type === "heading") {
      const level = block.props?.level ?? 1;
      if (level > 3) continue;

      // Extract text from inline content
      let text = "";
      if (Array.isArray(block.content)) {
        for (const node of block.content) {
          if (typeof node === "string") {
            text += node;
          } else if (node && typeof node === "object" && "text" in node) {
            text += (node as { text: string }).text;
          }
        }
      }

      if (text.trim()) {
        entries.push({ id: block.id, text: text.trim(), level });
      }
    }
  }
  return entries;
}

export const TableOfContents = createReactBlockSpec(
  {
    type: "toc" as const,
    propSchema: {},
    content: "none",
  },
  {
    render: (props) => {
      const { editor } = props;
      const [headings, setHeadings] = useState<HeadingEntry[]>([]);
      const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

      const scanHeadings = useCallback(() => {
        setHeadings(extractHeadings(editor));
      }, [editor]);

      // Initial scan and subscribe to changes
      useEffect(() => {
        scanHeadings();

        const handleChange = () => {
          if (debounceRef.current) {
            clearTimeout(debounceRef.current);
          }
          debounceRef.current = setTimeout(() => {
            scanHeadings();
          }, 100);
        };

        // Subscribe to editor changes
        editor.onChange(handleChange);

        return () => {
          if (debounceRef.current) {
            clearTimeout(debounceRef.current);
          }
          // Unsubscribe by passing the same callback
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          (editor as any).onChange(handleChange, { remove: true });
        };
      }, [editor, scanHeadings]);

      const handleClick = (blockId: string) => {
        try {
          editor.setTextCursorPosition(blockId, "start");

          // Scroll the heading block into view
          const blockElement = document.querySelector(
            `[data-id="${blockId}"]`,
          );
          if (blockElement) {
            blockElement.scrollIntoView({ behavior: "smooth", block: "center" });
          }
        } catch {
          // Block may have been removed
        }
      };

      return (
        <div className="toc-block" contentEditable={false}>
          <div className="toc-header">Table of Contents</div>
          {headings.length === 0 ? (
            <div className="toc-empty">
              Add headings to your document to generate a table of contents.
            </div>
          ) : (
            <nav className="toc-list">
              {headings.map((heading) => (
                <button
                  key={heading.id}
                  className={`toc-entry toc-level-${heading.level}`}
                  onClick={() => handleClick(heading.id)}
                  type="button"
                >
                  {heading.text}
                </button>
              ))}
            </nav>
          )}
        </div>
      );
    },

    toExternalHTML: () => {
      // Render as [toc] placeholder for markdown round-trip
      return <p>[toc]</p>;
    },

    parse: (element: HTMLElement) => {
      const text = element.textContent?.trim();
      if (text === "[toc]") {
        return {};
      }
      return undefined;
    },
  },
);

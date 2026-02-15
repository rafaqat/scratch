import { useState } from "react";
import { createReactBlockSpec } from "@blocknote/react";

/**
 * Collapsible toggle block (disclosure triangle).
 * Renders a summary line with a chevron; clicking toggles child blocks.
 * Serializes to `<details>/<summary>` HTML in markdown.
 * Toggle state is per-session only (not persisted).
 */
export const ToggleBlock = createReactBlockSpec(
  {
    type: "toggle" as const,
    propSchema: {},
    content: "inline",
  },
  {
    render: (props) => {
      const [collapsed, setCollapsed] = useState(false);

      return (
        <div
          className={`toggle-block ${collapsed ? "toggle-collapsed" : "toggle-open"}`}
          data-toggle-state={collapsed ? "closed" : "open"}
        >
          <div className="toggle-header">
            <button
              className="toggle-chevron"
              onClick={() => setCollapsed((c) => !c)}
              contentEditable={false}
              aria-label={collapsed ? "Expand" : "Collapse"}
            >
              <svg
                width="16"
                height="16"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth={2}
                strokeLinecap="round"
                strokeLinejoin="round"
                className={`toggle-chevron-icon ${collapsed ? "" : "toggle-chevron-open"}`}
              >
                <path d="M9 18l6-6-6-6" />
              </svg>
            </button>
            <div className="toggle-content" ref={props.contentRef} />
          </div>
        </div>
      );
    },

    toExternalHTML: (props) => {
      // Get the inline content as text
      const text = props.block.content
        ?.map((c: { type: string; text?: string }) =>
          c.type === "text" ? c.text || "" : ""
        )
        .join("") || "Toggle";

      return (
        <details open>
          <summary>{text}</summary>
        </details>
      );
    },

    parse: (element: HTMLElement) => {
      if (element.tagName === "DETAILS") {
        return {};
      }
      return undefined;
    },
  },
);

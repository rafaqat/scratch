import { createReactBlockSpec } from "@blocknote/react";

/**
 * Horizontal divider/separator block.
 * Renders as a styled horizontal line.
 * Serializes to `---` in markdown.
 */
export const DividerBlock = createReactBlockSpec(
  {
    type: "divider" as const,
    propSchema: {},
    content: "none",
  },
  {
    render: () => {
      return (
        <div className="divider-block" contentEditable={false}>
          <hr />
        </div>
      );
    },

    toExternalHTML: () => {
      return <hr />;
    },

    parse: (element: HTMLElement) => {
      if (element.tagName === "HR") {
        return {};
      }
      // Also parse from markdown --- (rendered as <p>---</p> sometimes)
      if (element.textContent?.trim() === "---") {
        return {};
      }
      return undefined;
    },
  },
);

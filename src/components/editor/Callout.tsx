import { createReactBlockSpec } from "@blocknote/react";
import { defaultProps } from "@blocknote/core";
import { useState, useRef, useEffect } from "react";

/**
 * Callout types with their display properties.
 * Maps to GitHub-flavored markdown alert syntax: > [!TYPE]
 */
export const CALLOUT_TYPES = {
  note: { label: "Note", icon: "pencil-icon", emoji: "\u270F\uFE0F", color: "gray", gfm: "NOTE" },
  info: { label: "Info", icon: "info-icon", emoji: "\u2139\uFE0F", color: "blue", gfm: "NOTE" },
  tip: { label: "Tip", icon: "tip-icon", emoji: "\uD83D\uDCA1", color: "green", gfm: "TIP" },
  warning: { label: "Warning", icon: "warning-icon", emoji: "\u26A0\uFE0F", color: "yellow", gfm: "WARNING" },
  danger: { label: "Danger", icon: "danger-icon", emoji: "\uD83D\uDED1", color: "red", gfm: "CAUTION" },
} as const;

export type CalloutType = keyof typeof CALLOUT_TYPES;

/**
 * Reverse lookup: GFM alert type string -> CalloutType
 */
export const GFM_TO_CALLOUT: Record<string, CalloutType> = {
  NOTE: "info",
  TIP: "tip",
  WARNING: "warning",
  CAUTION: "danger",
  IMPORTANT: "info",
};

/**
 * Callout block spec for BlockNote.
 *
 * Renders a colored box with an icon and inline text content.
 * Supports type switching via a dropdown picker.
 */
export const Callout = createReactBlockSpec(
  {
    type: "callout" as const,
    propSchema: {
      ...defaultProps,
      type: {
        default: "info" as const,
        values: ["info", "warning", "tip", "danger", "note"] as const,
      },
    },
    content: "inline",
  },
  {
    render: (props) => {
      const calloutType = (props.block.props.type as CalloutType) || "info";
      const typeInfo = CALLOUT_TYPES[calloutType] || CALLOUT_TYPES.info;
      const [pickerOpen, setPickerOpen] = useState(false);
      const pickerRef = useRef<HTMLDivElement>(null);

      // Close picker when clicking outside
      useEffect(() => {
        if (!pickerOpen) return;
        const handleClickOutside = (e: MouseEvent) => {
          if (pickerRef.current && !pickerRef.current.contains(e.target as Node)) {
            setPickerOpen(false);
          }
        };
        document.addEventListener("mousedown", handleClickOutside);
        return () => document.removeEventListener("mousedown", handleClickOutside);
      }, [pickerOpen]);

      return (
        <div className={`callout callout-${typeInfo.color}`} data-callout-type={calloutType}>
          <div className="callout-icon-wrapper" ref={pickerRef}>
            <button
              className="callout-icon-button"
              onClick={() => setPickerOpen(!pickerOpen)}
              contentEditable={false}
              title="Change callout type"
              type="button"
            >
              <span className="callout-emoji">{typeInfo.emoji}</span>
            </button>
            {pickerOpen && (
              <div className="callout-type-picker" contentEditable={false}>
                {(Object.entries(CALLOUT_TYPES) as [CalloutType, (typeof CALLOUT_TYPES)[CalloutType]][]).map(
                  ([key, info]) => (
                    <button
                      key={key}
                      className={`callout-type-option ${key === calloutType ? "active" : ""}`}
                      onClick={() => {
                        props.editor.updateBlock(props.block, {
                          props: { type: key },
                        });
                        setPickerOpen(false);
                      }}
                      type="button"
                    >
                      <span className="callout-emoji">{info.emoji}</span>
                      <span>{info.label}</span>
                    </button>
                  ),
                )}
              </div>
            )}
          </div>
          <div className="callout-content" ref={props.contentRef} />
        </div>
      );
    },

    toExternalHTML: (props) => {
      const calloutType = (props.block.props.type as CalloutType) || "info";
      const typeInfo = CALLOUT_TYPES[calloutType] || CALLOUT_TYPES.info;

      // Render as a blockquote with GFM alert syntax marker.
      // The [!TYPE] marker will be used by postprocessCallouts to convert
      // the HTML back to GFM callout markdown syntax.
      return (
        <div data-callout-type={calloutType} data-callout-gfm={typeInfo.gfm}>
          <div ref={props.contentRef} />
        </div>
      );
    },

    parse: (element) => {
      // Parse callout blocks from HTML (blockquote with data-callout-type)
      const calloutType = element.getAttribute("data-callout-type");
      if (calloutType && calloutType in CALLOUT_TYPES) {
        return { type: calloutType as CalloutType };
      }
      return undefined;
    },
  },
);

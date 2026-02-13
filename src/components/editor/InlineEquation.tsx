import { useState, useRef, useEffect, useCallback } from "react";
import { createReactInlineContentSpec } from "@blocknote/react";
import katex from "katex";

/**
 * Inline equation component for BlockNote.
 *
 * Renders inline math ($...$) within text.
 * Click to edit the LaTeX source.
 */
export const InlineEquation = createReactInlineContentSpec(
  {
    type: "inlineEquation" as const,
    propSchema: {
      equation: { default: "" },
    },
    content: "none",
  },
  {
    render: (props) => {
      const { equation } = props.inlineContent.props;
      const [editing, setEditing] = useState(!equation);
      const [localValue, setLocalValue] = useState(equation);
      const inputRef = useRef<HTMLInputElement>(null);
      const renderRef = useRef<HTMLSpanElement>(null);

      // Sync localValue when equation prop changes externally
      useEffect(() => {
        setLocalValue(equation);
      }, [equation]);

      // Auto-focus input when editing
      useEffect(() => {
        if (editing && inputRef.current) {
          inputRef.current.focus();
          inputRef.current.select();
        }
      }, [editing]);

      // Render KaTeX when not editing
      useEffect(() => {
        if (renderRef.current && !editing && localValue) {
          try {
            katex.render(localValue, renderRef.current, {
              displayMode: false,
              throwOnError: false,
              errorColor: "#dc2626",
              trust: true,
            });
          } catch {
            renderRef.current.textContent = localValue;
          }
        }
      }, [localValue, editing]);

      const handleSave = useCallback(() => {
        props.updateInlineContent({
          type: "inlineEquation" as const,
          props: { equation: localValue },
        });
        if (localValue.trim()) {
          setEditing(false);
        }
      }, [localValue, props]);

      const handleKeyDown = useCallback(
        (e: React.KeyboardEvent) => {
          if (e.key === "Enter" || e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            handleSave();
          }
        },
        [handleSave],
      );

      if (editing) {
        return (
          <span className="inline-equation inline-equation-editing" contentEditable={false}>
            <span className="inline-equation-dollar">$</span>
            <input
              ref={inputRef}
              className="inline-equation-input"
              value={localValue}
              onChange={(e) => setLocalValue(e.target.value)}
              onBlur={handleSave}
              onKeyDown={handleKeyDown}
              placeholder="LaTeX..."
              size={Math.max(5, localValue.length + 1)}
            />
            <span className="inline-equation-dollar">$</span>
          </span>
        );
      }

      return (
        <span
          className="inline-equation inline-equation-display"
          contentEditable={false}
          onClick={(e) => {
            e.preventDefault();
            e.stopPropagation();
            setEditing(true);
          }}
          title="Click to edit equation"
        >
          {equation ? (
            <span ref={renderRef} className="inline-equation-rendered" />
          ) : (
            <span className="inline-equation-placeholder">$...$</span>
          )}
        </span>
      );
    },

    toExternalHTML: (props) => {
      const { equation } = props.inlineContent.props;
      return <span>{`$${equation}$`}</span>;
    },
  },
);

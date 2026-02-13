import { useState, useRef, useEffect, useCallback } from "react";
import { createReactBlockSpec } from "@blocknote/react";
import katex from "katex";

/**
 * Block equation component for BlockNote.
 *
 * Renders display math ($$...$$) as a centered block.
 * Click to edit the LaTeX source, with live preview.
 */
export const EquationBlock = createReactBlockSpec(
  {
    type: "equation" as const,
    propSchema: {
      equation: { default: "" },
    },
    content: "none",
  },
  {
    render: (props) => {
      const { equation } = props.block.props;
      const [editing, setEditing] = useState(!equation);
      const [localValue, setLocalValue] = useState(equation);
      const inputRef = useRef<HTMLTextAreaElement>(null);
      const previewRef = useRef<HTMLDivElement>(null);
      const displayRef = useRef<HTMLDivElement>(null);

      // Sync localValue when equation prop changes externally
      useEffect(() => {
        setLocalValue(equation);
      }, [equation]);

      // Auto-focus the textarea when entering edit mode
      useEffect(() => {
        if (editing && inputRef.current) {
          inputRef.current.focus();
          inputRef.current.select();
        }
      }, [editing]);

      // Render KaTeX live preview in edit mode
      useEffect(() => {
        if (previewRef.current && localValue) {
          try {
            katex.render(localValue, previewRef.current, {
              displayMode: true,
              throwOnError: false,
              errorColor: "#dc2626",
              trust: true,
            });
          } catch {
            previewRef.current.innerHTML = `<span class="equation-error">Invalid equation</span>`;
          }
        }
      }, [localValue]);

      // Render KaTeX in display mode
      useEffect(() => {
        if (!editing && displayRef.current && equation) {
          try {
            katex.render(equation, displayRef.current, {
              displayMode: true,
              throwOnError: false,
              errorColor: "#dc2626",
              trust: true,
            });
          } catch {
            displayRef.current.innerHTML = `<span class="equation-error">Invalid equation</span>`;
          }
        }
      }, [equation, editing]);

      const handleSave = useCallback(() => {
        props.editor.updateBlock(props.block, {
          props: { equation: localValue },
        });
        if (localValue.trim()) {
          setEditing(false);
        }
      }, [localValue, props.editor, props.block]);

      const handleKeyDown = useCallback(
        (e: React.KeyboardEvent) => {
          if (e.key === "Escape") {
            e.preventDefault();
            handleSave();
          }
          // Allow Enter for multi-line, but Shift+Enter saves
          if (e.key === "Enter" && e.shiftKey) {
            e.preventDefault();
            handleSave();
          }
        },
        [handleSave],
      );

      if (editing) {
        return (
          <div className="equation-block equation-block-editing" contentEditable={false}>
            <div className="equation-edit-container">
              <textarea
                ref={inputRef}
                className="equation-input"
                value={localValue}
                onChange={(e) => setLocalValue(e.target.value)}
                onBlur={handleSave}
                onKeyDown={handleKeyDown}
                placeholder="Enter LaTeX equation..."
                rows={Math.max(1, localValue.split("\n").length)}
              />
              {localValue && (
                <div className="equation-preview">
                  <div ref={previewRef} />
                </div>
              )}
            </div>
            <div className="equation-hint">
              Shift+Enter to save, Escape to close
            </div>
          </div>
        );
      }

      return (
        <div
          className="equation-block equation-block-display"
          contentEditable={false}
          onClick={() => setEditing(true)}
          title="Click to edit equation"
        >
          {equation ? (
            <div
              ref={displayRef}
              className="equation-rendered"
            />
          ) : (
            <div className="equation-placeholder">
              Click to add equation
            </div>
          )}
        </div>
      );
    },

    toExternalHTML: (props) => {
      const { equation } = props.block.props;
      return (
        <div data-equation-block="true">
          {`$$\n${equation}\n$$`}
        </div>
      );
    },

    parse: (element: HTMLElement) => {
      if (element.getAttribute("data-equation-block") === "true") {
        const text = element.textContent?.trim() || "";
        const match = text.match(/^\$\$([\s\S]*?)\$\$$/);
        if (match) {
          return { equation: match[1].trim() };
        }
      }
      return undefined;
    },
  },
);

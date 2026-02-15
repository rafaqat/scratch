import { useState, useRef, useCallback } from "react";
import { createReactBlockSpec } from "@blocknote/react";

/**
 * Multi-column layout block.
 * Renders 2-4 columns side-by-side with editable content.
 * Serializes to :::columns format in markdown.
 */
export const ColumnsBlock = createReactBlockSpec(
  {
    type: "columns" as const,
    propSchema: {
      // JSON array of column content strings
      columnData: { default: '["",""]' },
      // JSON array of column widths as percentages
      columnWidths: { default: "" },
    },
    content: "none",
  },
  {
    render: (props) => {
      return <ColumnsRenderer {...props} />;
    },

    toExternalHTML: (props) => {
      const data = parseColumnData(props.block.props.columnData);
      return (
        <div>
          {data.map((col, i) => (
            <div key={i}>
              <p>{col}</p>
            </div>
          ))}
        </div>
      );
    },

    parse: (element: HTMLElement) => {
      // Parse :::columns blocks from rendered HTML
      if (element.dataset?.columns) {
        return {
          columnData: element.dataset.columns,
          columnWidths: element.dataset.widths || "",
        };
      }
      return undefined;
    },
  },
);

function parseColumnData(json: string): string[] {
  try {
    const parsed = JSON.parse(json);
    return Array.isArray(parsed) ? parsed : ["", ""];
  } catch {
    return ["", ""];
  }
}

function parseWidths(json: string, count: number): number[] {
  if (!json) {
    const w = 100 / count;
    return Array(count).fill(w);
  }
  try {
    const parsed = JSON.parse(json);
    if (Array.isArray(parsed) && parsed.length === count) return parsed;
  } catch {
    // fall through
  }
  const w = 100 / count;
  return Array(count).fill(w);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function ColumnsRenderer(props: any) {
  const columns = parseColumnData(props.block.props.columnData);
  const widths = parseWidths(props.block.props.columnWidths, columns.length);
  const [localCols, setLocalCols] = useState(columns);
  const [localWidths, setLocalWidths] = useState(widths);
  const containerRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<{ colIndex: number; startX: number; startWidths: number[] } | null>(null);

  const updateColumns = useCallback(
    (newCols: string[]) => {
      setLocalCols(newCols);
      props.editor.updateBlock(props.block, {
        props: {
          columnData: JSON.stringify(newCols),
          columnWidths: JSON.stringify(localWidths),
        },
      });
    },
    [props.editor, props.block, localWidths],
  );

  const handleInput = useCallback(
    (index: number, value: string) => {
      const updated = [...localCols];
      updated[index] = value;
      updateColumns(updated);
    },
    [localCols, updateColumns],
  );

  const addColumn = useCallback(() => {
    if (localCols.length >= 4) return;
    const newCols = [...localCols, ""];
    const w = 100 / newCols.length;
    const newWidths = Array(newCols.length).fill(w);
    setLocalCols(newCols);
    setLocalWidths(newWidths);
    props.editor.updateBlock(props.block, {
      props: {
        columnData: JSON.stringify(newCols),
        columnWidths: JSON.stringify(newWidths),
      },
    });
  }, [localCols, props.editor, props.block]);

  const removeColumn = useCallback(
    (index: number) => {
      if (localCols.length <= 2) return;
      const newCols = localCols.filter((_, i) => i !== index);
      const w = 100 / newCols.length;
      const newWidths = Array(newCols.length).fill(w);
      setLocalCols(newCols);
      setLocalWidths(newWidths);
      props.editor.updateBlock(props.block, {
        props: {
          columnData: JSON.stringify(newCols),
          columnWidths: JSON.stringify(newWidths),
        },
      });
    },
    [localCols, props.editor, props.block],
  );

  const handleDragStart = useCallback(
    (index: number, e: React.MouseEvent) => {
      e.preventDefault();
      dragRef.current = {
        colIndex: index,
        startX: e.clientX,
        startWidths: [...localWidths],
      };

      const handleMouseMove = (ev: MouseEvent) => {
        if (!dragRef.current || !containerRef.current) return;
        const { colIndex, startX, startWidths } = dragRef.current;
        const containerWidth = containerRef.current.offsetWidth;
        const deltaPercent = ((ev.clientX - startX) / containerWidth) * 100;

        const newWidths = [...startWidths];
        const minWidth = 15;
        newWidths[colIndex] = Math.max(minWidth, startWidths[colIndex] + deltaPercent);
        newWidths[colIndex + 1] = Math.max(minWidth, startWidths[colIndex + 1] - deltaPercent);
        setLocalWidths(newWidths);
      };

      const handleMouseUp = () => {
        document.removeEventListener("mousemove", handleMouseMove);
        document.removeEventListener("mouseup", handleMouseUp);
        if (dragRef.current) {
          props.editor.updateBlock(props.block, {
            props: {
              columnData: JSON.stringify(localCols),
              columnWidths: JSON.stringify(localWidths),
            },
          });
          dragRef.current = null;
        }
      };

      document.addEventListener("mousemove", handleMouseMove);
      document.addEventListener("mouseup", handleMouseUp);
    },
    [localWidths, localCols, props.editor, props.block],
  );

  return (
    <div className="columns-block" contentEditable={false} ref={containerRef}>
      <div
        className="columns-grid"
        style={{
          display: "grid",
          gridTemplateColumns: localWidths.map((w) => `${w}%`).join(" "),
          gap: "12px",
        }}
      >
        {localCols.map((col, i) => (
          <div key={i} className="column-cell" style={{ position: "relative" }}>
            <div
              className="column-content"
              contentEditable
              suppressContentEditableWarning
              onBlur={(e) => handleInput(i, e.currentTarget.textContent || "")}
              dangerouslySetInnerHTML={{ __html: col }}
              data-placeholder="Type here..."
            />
            {localCols.length > 2 && (
              <button
                className="column-remove"
                onClick={() => removeColumn(i)}
                title="Remove column"
              >
                &times;
              </button>
            )}
            {i < localCols.length - 1 && (
              <div
                className="column-resize-handle"
                onMouseDown={(e) => handleDragStart(i, e)}
              />
            )}
          </div>
        ))}
      </div>
      <div className="columns-controls">
        {localCols.length < 4 && (
          <button className="columns-add-btn" onClick={addColumn}>
            + Add column
          </button>
        )}
      </div>
    </div>
  );
}

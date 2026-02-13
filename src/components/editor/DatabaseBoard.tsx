import { useState, useEffect, useCallback, useRef } from "react";
import { createReactBlockSpec } from "@blocknote/react";
import { defaultProps } from "@blocknote/core";
import * as dbService from "../../services/database";
import type {
  DatabaseSchema,
  DatabaseRow,
  DatabaseInfo,
  ColumnDef,
} from "../../types/database";
import { DatabaseTemplateMenu } from "./DatabaseTemplateMenu";

// ---- Types ----

interface BoardColumn {
  /** The select option value (or "__uncategorized__") */
  value: string;
  /** Display label */
  label: string;
  /** Rows belonging to this column */
  rows: DatabaseRow[];
}

// ---- Helpers ----

/** Get the "title" column: first text column, or fallback to first column. */
function getTitleColumn(schema: DatabaseSchema): ColumnDef | undefined {
  return (
    schema.columns.find((c) => c.type === "text") ?? schema.columns[0]
  );
}

/** Get the display title for a row. */
function getRowTitle(row: DatabaseRow, titleCol?: ColumnDef): string {
  if (!titleCol) return row.id;
  const val = row.fields[titleCol.id];
  if (typeof val === "string" && val.trim()) return val;
  return row.id;
}

/** Get the first select column suitable for grouping. */
function getDefaultGroupByColumn(
  schema: DatabaseSchema,
): ColumnDef | undefined {
  return schema.columns.find((c) => c.type === "select");
}

/** Format a field value for display on a card. */
function formatFieldValue(
  value: unknown,
  col: ColumnDef,
): React.ReactNode {
  if (value == null || value === "") return null;

  switch (col.type) {
    case "checkbox":
      return (
        <span className="db-board-field-checkbox">
          {value ? "\u2611" : "\u2610"}
        </span>
      );
    case "select":
      return <span className="db-board-field-badge">{String(value)}</span>;
    case "multi-select":
      if (Array.isArray(value)) {
        return (
          <span className="db-board-field-badges">
            {value.map((v, i) => (
              <span key={i} className="db-board-field-badge">
                {String(v)}
              </span>
            ))}
          </span>
        );
      }
      return null;
    case "date":
      return (
        <span className="db-board-field-date">{String(value)}</span>
      );
    case "number":
      return (
        <span className="db-board-field-number">{String(value)}</span>
      );
    case "url":
      return (
        <span className="db-board-field-url" title={String(value)}>
          {String(value)}
        </span>
      );
    default:
      return <span>{String(value)}</span>;
  }
}

// ---- Database Picker (shown when no database is linked) ----

function DatabasePicker({
  onSelect,
}: {
  onSelect: (dbId: string) => void;
}) {
  const [databases, setDatabases] = useState<DatabaseInfo[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    dbService
      .listDatabases()
      .then(setDatabases)
      .catch(() => setDatabases([]))
      .finally(() => setLoading(false));
  }, []);

  if (loading) {
    return (
      <div className="db-board-picker">
        <span className="db-board-picker-loading">Loading databases...</span>
      </div>
    );
  }

  if (databases.length === 0) {
    return (
      <div className="db-board-picker">
        <span className="db-board-picker-empty">
          No databases found. Create a database first.
        </span>
      </div>
    );
  }

  return (
    <div className="db-board-picker">
      <div className="db-board-picker-title">Select a database for board view</div>
      <div className="db-board-picker-list">
        {databases.map((db) => (
          <button
            key={db.id}
            className="db-board-picker-item"
            onClick={() => onSelect(db.id)}
            type="button"
          >
            <span className="db-board-picker-item-name">{db.name}</span>
            <span className="db-board-picker-item-meta">
              {db.row_count} rows &middot; {db.column_count} columns
            </span>
          </button>
        ))}
      </div>
    </div>
  );
}

// ---- Board Card ----

function BoardCard({
  row,
  titleCol,
  previewCols,
  dbId,
  onDragStart,
}: {
  row: DatabaseRow;
  titleCol?: ColumnDef;
  previewCols: ColumnDef[];
  dbId: string;
  onDragStart: (e: React.DragEvent, rowId: string) => void;
}) {
  return (
    <div
      className="db-board-card"
      draggable
      onDragStart={(e) => {
        e.stopPropagation();
        onDragStart(e, row.id);
      }}
      data-row-id={row.id}
      data-db-id={dbId}
    >
      <div className="db-board-card-title">{getRowTitle(row, titleCol)}</div>
      {previewCols.map((col) => {
        const val = row.fields[col.id];
        const rendered = formatFieldValue(val, col);
        if (!rendered) return null;
        return (
          <div key={col.id} className="db-board-card-field">
            <span className="db-board-card-field-label">{col.name}</span>
            {rendered}
          </div>
        );
      })}
    </div>
  );
}

// ---- Board Column ----

function BoardColumnComponent({
  column,
  titleCol,
  previewCols,
  dbId,
  onDragStart,
  onDrop,
  onAddCard,
}: {
  column: BoardColumn;
  titleCol?: ColumnDef;
  previewCols: ColumnDef[];
  dbId: string;
  onDragStart: (e: React.DragEvent, rowId: string) => void;
  onDrop: (columnValue: string) => void;
  onAddCard: (columnValue: string) => void;
}) {
  const [dragOver, setDragOver] = useState(false);

  const handleDragOver = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      setDragOver(true);
    },
    [],
  );

  const handleDragLeave = useCallback(
    (e: React.DragEvent) => {
      e.stopPropagation();
      setDragOver(false);
    },
    [],
  );

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      setDragOver(false);
      onDrop(column.value);
    },
    [onDrop, column.value],
  );

  return (
    <div
      className={`db-board-column ${dragOver ? "db-board-column-drag-over" : ""}`}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      <div className="db-board-column-header">
        <span className="db-board-column-title">{column.label}</span>
        <span className="db-board-column-count">{column.rows.length}</span>
      </div>
      <div className="db-board-column-cards">
        {column.rows.map((row) => (
          <BoardCard
            key={row.id}
            row={row}
            titleCol={titleCol}
            previewCols={previewCols}
            dbId={dbId}
            onDragStart={onDragStart}
          />
        ))}
      </div>
      <button
        className="db-board-add-card"
        onClick={() => onAddCard(column.value)}
        type="button"
      >
        + New
      </button>
    </div>
  );
}

// ---- Group-By Selector ----

function GroupBySelector({
  schema,
  currentGroupBy,
  onChange,
}: {
  schema: DatabaseSchema;
  currentGroupBy: string;
  onChange: (colId: string) => void;
}) {
  const selectColumns = schema.columns.filter((c) => c.type === "select");

  if (selectColumns.length <= 1) return null;

  return (
    <select
      className="db-board-group-select"
      value={currentGroupBy}
      onChange={(e) => onChange(e.target.value)}
    >
      {selectColumns.map((col) => (
        <option key={col.id} value={col.id}>
          Group by: {col.name}
        </option>
      ))}
    </select>
  );
}

// ---- Main Board View ----

function BoardView({
  dbId,
  initialGroupBy,
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  editor,
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  block,
}: {
  dbId: string;
  initialGroupBy?: string;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  editor: any;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  block: any;
}) {
  const [schema, setSchema] = useState<DatabaseSchema | null>(null);
  const [rows, setRows] = useState<DatabaseRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [groupByColId, setGroupByColId] = useState<string>(
    initialGroupBy || "",
  );
  const draggedRowIdRef = useRef<string | null>(null);

  // Load database data
  const loadData = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const result = await dbService.getDatabase(dbId);
      setSchema(result.schema);
      setRows(result.rows);

      // Set default group-by if not set
      if (!groupByColId) {
        const defaultCol = getDefaultGroupByColumn(result.schema);
        if (defaultCol) {
          setGroupByColId(defaultCol.id);
        }
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, [dbId, groupByColId]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  // Build board columns from schema + rows
  const buildColumns = useCallback((): BoardColumn[] => {
    if (!schema) return [];

    const groupCol = schema.columns.find((c) => c.id === groupByColId);
    if (!groupCol || groupCol.type !== "select" || !groupCol.options) {
      return [];
    }

    const columnMap = new Map<string, DatabaseRow[]>();

    // Initialize columns from options order
    for (const opt of groupCol.options) {
      columnMap.set(opt, []);
    }
    // Add uncategorized bucket
    columnMap.set("__uncategorized__", []);

    // Distribute rows into columns
    for (const row of rows) {
      const val = row.fields[groupByColId];
      const strVal = typeof val === "string" ? val : "";
      if (strVal && columnMap.has(strVal)) {
        columnMap.get(strVal)!.push(row);
      } else {
        columnMap.get("__uncategorized__")!.push(row);
      }
    }

    // Build ordered column list
    const result: BoardColumn[] = [];
    for (const opt of groupCol.options) {
      result.push({
        value: opt,
        label: opt,
        rows: columnMap.get(opt) || [],
      });
    }

    // Add uncategorized if it has rows
    const uncategorized = columnMap.get("__uncategorized__") || [];
    if (uncategorized.length > 0) {
      result.push({
        value: "__uncategorized__",
        label: "Uncategorized",
        rows: uncategorized,
      });
    }

    return result;
  }, [schema, rows, groupByColId]);

  const columns = buildColumns();

  // Get title column and preview columns
  const titleCol = schema ? getTitleColumn(schema) : undefined;
  const previewCols = schema
    ? schema.columns
        .filter(
          (c) =>
            c.id !== titleCol?.id &&
            c.id !== groupByColId &&
            c.type !== "relation",
        )
        .slice(0, 2)
    : [];

  // Drag and drop handlers
  const handleDragStart = useCallback(
    (e: React.DragEvent, rowId: string) => {
      draggedRowIdRef.current = rowId;
      e.dataTransfer.effectAllowed = "move";
      e.dataTransfer.setData("text/plain", rowId);
    },
    [],
  );

  const handleDrop = useCallback(
    async (targetColumnValue: string) => {
      const rowId = draggedRowIdRef.current;
      draggedRowIdRef.current = null;

      if (!rowId || !schema) return;

      const row = rows.find((r) => r.id === rowId);
      if (!row) return;

      // Check if the row is already in this column
      const currentVal = row.fields[groupByColId];
      const currentStr = typeof currentVal === "string" ? currentVal : "";
      const targetStr =
        targetColumnValue === "__uncategorized__" ? "" : targetColumnValue;

      if (currentStr === targetStr) return;

      // Optimistically update the UI
      setRows((prev) =>
        prev.map((r) =>
          r.id === rowId
            ? { ...r, fields: { ...r.fields, [groupByColId]: targetStr } }
            : r,
        ),
      );

      // Persist to backend
      try {
        await dbService.updateRow(dbId, rowId, {
          [groupByColId]: targetStr,
        });
      } catch (err) {
        console.error("Failed to update row:", err);
        // Revert on failure
        loadData();
      }
    },
    [schema, rows, groupByColId, dbId, loadData],
  );

  // Add new card in a column
  const handleAddCard = useCallback(
    async (columnValue: string) => {
      if (!schema) return;

      const fields: Record<string, unknown> = {};

      // Set the group-by field
      if (columnValue !== "__uncategorized__") {
        fields[groupByColId] = columnValue;
      }

      // Set default title
      const tc = getTitleColumn(schema);
      if (tc) {
        fields[tc.id] = "New item";
      }

      try {
        const newRow = await dbService.createRow(dbId, fields);
        setRows((prev) => [...prev, newRow]);
      } catch (err) {
        console.error("Failed to create row:", err);
      }
    },
    [schema, groupByColId, dbId],
  );

  // Handle group-by change
  const handleGroupByChange = useCallback(
    (colId: string) => {
      setGroupByColId(colId);
      // Persist the group-by preference in block props
      editor.updateBlock(block, {
        props: { groupBy: colId },
      });
    },
    [editor, block],
  );

  if (loading) {
    return (
      <div className="db-board-loading">Loading board...</div>
    );
  }

  if (error) {
    return (
      <div className="db-board-error">
        Error: {error}
        <button
          className="db-board-retry"
          onClick={loadData}
          type="button"
        >
          Retry
        </button>
      </div>
    );
  }

  if (!schema) {
    return <div className="db-board-error">Database not found.</div>;
  }

  const groupCol = schema.columns.find((c) => c.id === groupByColId);
  if (!groupCol || groupCol.type !== "select") {
    return (
      <div className="db-board-no-group">
        <p>No select column found for grouping.</p>
        <p className="db-board-no-group-hint">
          Add a &quot;select&quot; column to your database to use board view.
        </p>
      </div>
    );
  }

  return (
    <div
      className="db-board-container"
      onDragOver={(e) => {
        e.preventDefault();
        e.stopPropagation();
      }}
      onDrop={(e) => {
        e.stopPropagation();
      }}
    >
      <div className="db-board-toolbar">
        <span className="db-board-name">{schema.name}</span>
        <GroupBySelector
          schema={schema}
          currentGroupBy={groupByColId}
          onChange={handleGroupByChange}
        />
        {schema.templates && Object.keys(schema.templates).length > 0 && (
          <DatabaseTemplateMenu
            dbId={dbId}
            onRowCreated={(newRow) => setRows((prev) => [...prev, newRow])}
          />
        )}
        <button
          className="db-board-refresh"
          onClick={loadData}
          type="button"
          title="Refresh data"
        >
          &#x21bb;
        </button>
      </div>
      <div className="db-board-columns">
        {columns.map((col) => (
          <BoardColumnComponent
            key={col.value}
            column={col}
            titleCol={titleCol}
            previewCols={previewCols}
            dbId={dbId}
            onDragStart={handleDragStart}
            onDrop={handleDrop}
            onAddCard={handleAddCard}
          />
        ))}
      </div>
    </div>
  );
}

// ---- BlockNote Block Spec ----

export const DatabaseBoard = createReactBlockSpec(
  {
    type: "databaseBoard" as const,
    propSchema: {
      ...defaultProps,
      dbId: { default: "" },
      groupBy: { default: "" },
    },
    content: "none",
  },
  {
    render: (props) => {
      const { dbId, groupBy } = props.block.props;

      const handleSelectDatabase = useCallback(
        (selectedDbId: string) => {
          props.editor.updateBlock(props.block, {
            props: { dbId: selectedDbId },
          });
        },
        [props.editor, props.block],
      );

      if (!dbId) {
        return (
          <div className="db-board-wrapper" contentEditable={false}>
            <DatabasePicker onSelect={handleSelectDatabase} />
          </div>
        );
      }

      return (
        <div className="db-board-wrapper" contentEditable={false}>
          <BoardView
            dbId={dbId}
            initialGroupBy={groupBy || undefined}
            editor={props.editor}
            block={props.block}
          />
        </div>
      );
    },

    toExternalHTML: (props) => {
      const { dbId } = props.block.props;
      return (
        <p>
          {dbId
            ? `[database:${dbId}](view:board)`
            : "[database](view:board)"}
        </p>
      );
    },

    parse: (element: HTMLElement) => {
      const text = element.textContent?.trim() || "";
      const match = text.match(
        /^\[database:([^\]]+)\]\(view:board\)$/,
      );
      if (match) {
        return { dbId: match[1], groupBy: "" };
      }
      return undefined;
    },
  },
);

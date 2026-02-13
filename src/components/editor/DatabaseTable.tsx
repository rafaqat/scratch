import { useState, useEffect, useCallback, useRef, useMemo } from "react";
import { createReactBlockSpec } from "@blocknote/react";
import * as db from "../../services/database";
import type {
  ColumnDef,
  ColumnType,
  DatabaseSchema,
  DatabaseRow,
  DatabaseInfo,
} from "../../types/database";
import { defaultFieldValue, COLUMN_TYPES } from "../../types/database";
import { DatabaseTemplateMenu } from "./DatabaseTemplateMenu";

// ---- Helper: format a cell value for display ----
function formatCellValue(value: unknown, colType: ColumnType): string {
  if (value === null || value === undefined) return "";
  switch (colType) {
    case "checkbox":
      return value ? "true" : "false";
    case "multi-select":
      return Array.isArray(value) ? (value as string[]).join(", ") : "";
    case "number":
      return String(value);
    default:
      return String(value);
  }
}

// ---- Sorting helpers ----
function compareValues(a: unknown, b: unknown, colType: ColumnType): number {
  if (a === null || a === undefined) return -1;
  if (b === null || b === undefined) return 1;

  switch (colType) {
    case "number":
      return (Number(a) || 0) - (Number(b) || 0);
    case "checkbox":
      return (a ? 1 : 0) - (b ? 1 : 0);
    case "date":
      return String(a).localeCompare(String(b));
    default:
      return String(a).localeCompare(String(b));
  }
}

// ---- Filter match helper ----
function matchesFilter(
  value: unknown,
  filter: string,
  colType: ColumnType,
): boolean {
  if (!filter) return true;
  const filterLower = filter.toLowerCase();

  switch (colType) {
    case "checkbox":
      if (filterLower === "true" || filterLower === "yes") return value === true;
      if (filterLower === "false" || filterLower === "no") return value === false;
      return true;
    case "multi-select":
      if (Array.isArray(value)) {
        return (value as string[]).some((v) =>
          v.toLowerCase().includes(filterLower),
        );
      }
      return false;
    case "number":
      return String(value).includes(filter);
    default:
      return String(value || "")
        .toLowerCase()
        .includes(filterLower);
  }
}

// ---- Database picker (shown when no database is selected) ----
function DatabasePicker({
  onSelect,
}: {
  onSelect: (dbId: string) => void;
}) {
  const [databases, setDatabases] = useState<DatabaseInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");

  useEffect(() => {
    db.listDatabases()
      .then(setDatabases)
      .catch((err) => setError(String(err)))
      .finally(() => setLoading(false));
  }, []);

  const handleCreate = async () => {
    if (!newName.trim()) return;
    setCreating(true);
    try {
      const info = await db.createDatabase(newName.trim(), [
        { id: "title", name: "Title", type: "text" },
        {
          id: "status",
          name: "Status",
          type: "select",
          options: ["Todo", "In Progress", "Done"],
        },
      ]);
      onSelect(info.id);
    } catch (err) {
      setError(String(err));
    } finally {
      setCreating(false);
    }
  };

  if (loading) {
    return (
      <div className="db-table-picker">
        <div className="db-table-picker-loading">Loading databases...</div>
      </div>
    );
  }

  return (
    <div className="db-table-picker" contentEditable={false}>
      <div className="db-table-picker-header">Select a Database</div>
      {error && <div className="db-table-error">{error}</div>}
      {databases.length > 0 ? (
        <div className="db-table-picker-list">
          {databases.map((d) => (
            <button
              key={d.id}
              type="button"
              className="db-table-picker-item"
              onClick={() => onSelect(d.id)}
            >
              <span className="db-table-picker-item-name">{d.name}</span>
              <span className="db-table-picker-item-meta">
                {d.row_count} rows, {d.column_count} cols
              </span>
            </button>
          ))}
        </div>
      ) : (
        <div className="db-table-picker-empty">No databases found.</div>
      )}
      <div className="db-table-picker-create">
        <input
          type="text"
          className="db-table-picker-input"
          placeholder="New database name..."
          value={newName}
          onChange={(e) => setNewName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") handleCreate();
          }}
        />
        <button
          type="button"
          className="db-table-picker-create-btn"
          onClick={handleCreate}
          disabled={creating || !newName.trim()}
        >
          {creating ? "Creating..." : "Create"}
        </button>
      </div>
    </div>
  );
}

// ---- Cell editor component ----
function CellEditor({
  value,
  column,
  onSave,
  onCancel,
}: {
  value: unknown;
  column: ColumnDef;
  onSave: (val: unknown) => void;
  onCancel: () => void;
}) {
  const inputRef = useRef<HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onCancel();
    }
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      e.stopPropagation();
      // The onBlur handler will save
      inputRef.current?.blur();
    }
    // Stop propagation to prevent BlockNote from handling keys
    e.stopPropagation();
  };

  switch (column.type) {
    case "checkbox":
      return (
        <input
          ref={inputRef as React.RefObject<HTMLInputElement>}
          type="checkbox"
          checked={!!value}
          onChange={(e) => onSave(e.target.checked)}
          onKeyDown={handleKeyDown}
          className="db-table-checkbox"
        />
      );

    case "number":
      return (
        <input
          ref={inputRef as React.RefObject<HTMLInputElement>}
          type="number"
          defaultValue={value != null ? Number(value) : 0}
          onBlur={(e) => onSave(Number(e.target.value) || 0)}
          onKeyDown={handleKeyDown}
          className="db-table-cell-input"
        />
      );

    case "date":
      return (
        <input
          ref={inputRef as React.RefObject<HTMLInputElement>}
          type="date"
          defaultValue={String(value || "")}
          onBlur={(e) => onSave(e.target.value)}
          onKeyDown={handleKeyDown}
          className="db-table-cell-input"
        />
      );

    case "select":
      return (
        <select
          ref={inputRef as React.RefObject<HTMLSelectElement>}
          defaultValue={String(value || "")}
          onBlur={(e) => onSave(e.target.value)}
          onChange={(e) => onSave(e.target.value)}
          onKeyDown={handleKeyDown}
          className="db-table-cell-input"
        >
          <option value="">--</option>
          {column.options?.map((opt) => (
            <option key={opt} value={opt}>
              {opt}
            </option>
          ))}
        </select>
      );

    case "multi-select": {
      const selected = Array.isArray(value) ? (value as string[]) : [];
      return (
        <div className="db-table-multiselect" onKeyDown={handleKeyDown}>
          {column.options?.map((opt) => (
            <label key={opt} className="db-table-multiselect-option">
              <input
                type="checkbox"
                checked={selected.includes(opt)}
                onChange={(e) => {
                  const next = e.target.checked
                    ? [...selected, opt]
                    : selected.filter((s) => s !== opt);
                  onSave(next);
                }}
              />
              <span>{opt}</span>
            </label>
          ))}
        </div>
      );
    }

    case "url":
      return (
        <input
          ref={inputRef as React.RefObject<HTMLInputElement>}
          type="url"
          defaultValue={String(value || "")}
          onBlur={(e) => onSave(e.target.value)}
          onKeyDown={handleKeyDown}
          className="db-table-cell-input"
          placeholder="https://..."
        />
      );

    default:
      // text, relation
      return (
        <input
          ref={inputRef as React.RefObject<HTMLInputElement>}
          type="text"
          defaultValue={String(value || "")}
          onBlur={(e) => onSave(e.target.value)}
          onKeyDown={handleKeyDown}
          className="db-table-cell-input"
        />
      );
  }
}

// ---- Column header menu ----
function ColumnHeaderMenu({
  column,
  columnIndex,
  totalColumns,
  onClose,
  onRemove,
  onMoveLeft,
  onMoveRight,
  onSort,
}: {
  column: ColumnDef;
  columnIndex: number;
  totalColumns: number;
  onClose: () => void;
  onRemove: () => void;
  onMoveLeft: () => void;
  onMoveRight: () => void;
  onSort: (desc: boolean) => void;
}) {
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        onClose();
      }
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [onClose]);

  return (
    <div ref={menuRef} className="db-table-col-menu">
      <div className="db-table-col-menu-label">{column.name}</div>
      <button type="button" className="db-table-col-menu-item" onClick={() => { onSort(false); onClose(); }}>
        Sort A to Z
      </button>
      <button type="button" className="db-table-col-menu-item" onClick={() => { onSort(true); onClose(); }}>
        Sort Z to A
      </button>
      <div className="db-table-col-menu-divider" />
      {columnIndex > 0 && (
        <button type="button" className="db-table-col-menu-item" onClick={() => { onMoveLeft(); onClose(); }}>
          Move Left
        </button>
      )}
      {columnIndex < totalColumns - 1 && (
        <button type="button" className="db-table-col-menu-item" onClick={() => { onMoveRight(); onClose(); }}>
          Move Right
        </button>
      )}
      <div className="db-table-col-menu-divider" />
      <button type="button" className="db-table-col-menu-item db-table-col-menu-danger" onClick={() => { onRemove(); onClose(); }}>
        Delete Column
      </button>
    </div>
  );
}

// ---- Add column popover ----
function AddColumnPopover({
  onAdd,
  onClose,
}: {
  onAdd: (name: string, type: ColumnType) => void;
  onClose: () => void;
}) {
  const [name, setName] = useState("");
  const [colType, setColType] = useState<ColumnType>("text");
  const [options, setOptions] = useState("");
  const ref = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
    const handleClickOutside = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        onClose();
      }
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [onClose]);

  const needsOptions = colType === "select" || colType === "multi-select";

  const handleAdd = () => {
    if (!name.trim()) return;
    onAdd(name.trim(), colType);
    onClose();
  };

  return (
    <div ref={ref} className="db-table-add-col-popover">
      <div className="db-table-add-col-field">
        <label className="db-table-add-col-label">Name</label>
        <input
          ref={inputRef}
          type="text"
          className="db-table-add-col-input"
          placeholder="Column name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") handleAdd();
            if (e.key === "Escape") onClose();
            e.stopPropagation();
          }}
        />
      </div>
      <div className="db-table-add-col-field">
        <label className="db-table-add-col-label">Type</label>
        <select
          className="db-table-add-col-input"
          value={colType}
          onChange={(e) => setColType(e.target.value as ColumnType)}
        >
          {COLUMN_TYPES.map((ct) => (
            <option key={ct.value} value={ct.value}>
              {ct.label}
            </option>
          ))}
        </select>
      </div>
      {needsOptions && (
        <div className="db-table-add-col-field">
          <label className="db-table-add-col-label">Options (comma-separated)</label>
          <input
            type="text"
            className="db-table-add-col-input"
            placeholder="Option 1, Option 2, ..."
            value={options}
            onChange={(e) => setOptions(e.target.value)}
            onKeyDown={(e) => e.stopPropagation()}
          />
        </div>
      )}
      <button
        type="button"
        className="db-table-add-col-btn"
        onClick={handleAdd}
        disabled={!name.trim() || (needsOptions && !options.trim())}
      >
        Add Column
      </button>
    </div>
  );
}

// ---- Main table renderer component ----
function DatabaseTableRenderer({
  databaseName,
}: {
  databaseName: string;
}) {
  const [schema, setSchema] = useState<DatabaseSchema | null>(null);
  const [rows, setRows] = useState<DatabaseRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Sort state
  const [sortCol, setSortCol] = useState<string | null>(null);
  const [sortDesc, setSortDesc] = useState(false);

  // Filter state: columnId -> filter string
  const [filters, setFilters] = useState<Record<string, string>>({});
  const [activeFilter, setActiveFilter] = useState<string | null>(null);

  // Editing state
  const [editingCell, setEditingCell] = useState<{
    rowId: string;
    colId: string;
  } | null>(null);

  // Column management
  const [menuCol, setMenuCol] = useState<string | null>(null);
  const [showAddCol, setShowAddCol] = useState(false);

  // Load data
  const loadData = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await db.getDatabase(databaseName);
      setSchema(result.schema);
      setRows(result.rows);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [databaseName]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  // Sorted and filtered rows
  const displayRows = useMemo(() => {
    let result = [...rows];

    // Apply filters
    if (schema) {
      for (const [colId, filterVal] of Object.entries(filters)) {
        if (!filterVal) continue;
        const col = schema.columns.find((c) => c.id === colId);
        if (!col) continue;
        result = result.filter((row) =>
          matchesFilter(row.fields[colId], filterVal, col.type),
        );
      }
    }

    // Apply sort
    if (sortCol && schema) {
      const col = schema.columns.find((c) => c.id === sortCol);
      if (col) {
        result.sort((a, b) => {
          const cmp = compareValues(
            a.fields[sortCol],
            b.fields[sortCol],
            col.type,
          );
          return sortDesc ? -cmp : cmp;
        });
      }
    }

    return result;
  }, [rows, filters, sortCol, sortDesc, schema]);

  // Cell save handler
  const handleCellSave = useCallback(
    async (rowId: string, colId: string, value: unknown) => {
      try {
        const updatedRow = await db.updateRow(databaseName, rowId, {
          [colId]: value,
        });
        setRows((prev) =>
          prev.map((r) => (r.id === rowId ? updatedRow : r)),
        );
      } catch (err) {
        console.error("Failed to update cell:", err);
      }
      setEditingCell(null);
    },
    [databaseName],
  );

  // Add row handler
  const handleAddRow = useCallback(async () => {
    if (!schema) return;
    try {
      const fields: Record<string, unknown> = {};
      for (const col of schema.columns) {
        fields[col.id] = defaultFieldValue(col.type);
      }
      const newRow = await db.createRow(databaseName, fields);
      setRows((prev) => [...prev, newRow]);
    } catch (err) {
      console.error("Failed to add row:", err);
    }
  }, [databaseName, schema]);

  // Delete row handler
  const handleDeleteRow = useCallback(
    async (rowId: string) => {
      try {
        await db.deleteRow(databaseName, rowId);
        setRows((prev) => prev.filter((r) => r.id !== rowId));
      } catch (err) {
        console.error("Failed to delete row:", err);
      }
    },
    [databaseName],
  );

  // Add column handler
  const handleAddColumn = useCallback(
    async (name: string, type: ColumnType) => {
      const id = name
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, "_")
        .replace(/^_|_$/g, "");
      const column: ColumnDef = { id, name, type };
      if (type === "select" || type === "multi-select") {
        // Will be set from options input; for now use empty, this is handled by AddColumnPopover
        column.options = ["Option 1", "Option 2"];
      }
      try {
        const updatedSchema = await db.addColumn(databaseName, column);
        setSchema(updatedSchema);
        // Reload rows to get updated fields
        const result = await db.getDatabase(databaseName);
        setRows(result.rows);
      } catch (err) {
        console.error("Failed to add column:", err);
      }
    },
    [databaseName],
  );

  // Remove column handler
  const handleRemoveColumn = useCallback(
    async (colId: string) => {
      try {
        const updatedSchema = await db.removeColumn(databaseName, colId);
        setSchema(updatedSchema);
        setRows((prev) =>
          prev.map((r) => {
            const fields = { ...r.fields };
            delete fields[colId];
            return { ...r, fields };
          }),
        );
      } catch (err) {
        console.error("Failed to remove column:", err);
      }
    },
    [databaseName],
  );

  // Reorder column handler (move left or right)
  const handleMoveColumn = useCallback(
    async (colId: string, direction: "left" | "right") => {
      if (!schema) return;
      const idx = schema.columns.findIndex((c) => c.id === colId);
      if (idx < 0) return;
      const newIdx = direction === "left" ? idx - 1 : idx + 1;
      if (newIdx < 0 || newIdx >= schema.columns.length) return;

      const newCols = [...schema.columns];
      [newCols[idx], newCols[newIdx]] = [newCols[newIdx], newCols[idx]];

      const newSchema = { ...schema, columns: newCols };
      try {
        const updated = await db.updateSchema(databaseName, newSchema);
        setSchema(updated);
      } catch (err) {
        console.error("Failed to reorder columns:", err);
      }
    },
    [databaseName, schema],
  );

  // Sort handler
  const handleSort = useCallback((colId: string, desc: boolean) => {
    setSortCol(colId);
    setSortDesc(desc);
  }, []);

  // Header click handler (toggle sort)
  const handleHeaderClick = useCallback(
    (colId: string) => {
      if (sortCol === colId) {
        setSortDesc((d) => !d);
      } else {
        setSortCol(colId);
        setSortDesc(false);
      }
    },
    [sortCol],
  );

  if (loading) {
    return (
      <div className="db-table-block" contentEditable={false}>
        <div className="db-table-loading">Loading database...</div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="db-table-block" contentEditable={false}>
        <div className="db-table-error">{error}</div>
      </div>
    );
  }

  if (!schema) return null;

  return (
    <div className="db-table-block" contentEditable={false}>
      <div className="db-table-title-bar">
        <span className="db-table-title">{schema.name}</span>
        <span className="db-table-meta">
          {displayRows.length} of {rows.length} rows
        </span>
      </div>

      <div className="db-table-scroll">
        <table className="db-table">
          <thead>
            <tr>
              {schema.columns.map((col, colIdx) => (
                <th key={col.id} className="db-table-th">
                  <div className="db-table-th-inner">
                    <button
                      type="button"
                      className="db-table-th-label"
                      onClick={() => handleHeaderClick(col.id)}
                      title={`Sort by ${col.name}`}
                    >
                      <span>{col.name}</span>
                      {sortCol === col.id && (
                        <span className="db-table-sort-arrow">
                          {sortDesc ? "\u2193" : "\u2191"}
                        </span>
                      )}
                    </button>
                    <div className="db-table-th-actions">
                      <button
                        type="button"
                        className="db-table-th-action-btn"
                        onClick={(e) => {
                          e.stopPropagation();
                          setActiveFilter(activeFilter === col.id ? null : col.id);
                        }}
                        title="Filter"
                      >
                        <FilterIcon />
                      </button>
                      <button
                        type="button"
                        className="db-table-th-action-btn"
                        onClick={(e) => {
                          e.stopPropagation();
                          setMenuCol(menuCol === col.id ? null : col.id);
                        }}
                        title="Column options"
                      >
                        <MoreIcon />
                      </button>
                    </div>
                  </div>
                  {/* Filter dropdown */}
                  {activeFilter === col.id && (
                    <div className="db-table-filter-dropdown">
                      <input
                        type="text"
                        className="db-table-filter-input"
                        placeholder={`Filter ${col.name}...`}
                        value={filters[col.id] || ""}
                        onChange={(e) =>
                          setFilters((f) => ({
                            ...f,
                            [col.id]: e.target.value,
                          }))
                        }
                        onKeyDown={(e) => {
                          if (e.key === "Escape") setActiveFilter(null);
                          e.stopPropagation();
                        }}
                        autoFocus
                      />
                      {filters[col.id] && (
                        <button
                          type="button"
                          className="db-table-filter-clear"
                          onClick={() => {
                            setFilters((f) => {
                              const next = { ...f };
                              delete next[col.id];
                              return next;
                            });
                            setActiveFilter(null);
                          }}
                        >
                          Clear
                        </button>
                      )}
                    </div>
                  )}
                  {/* Column menu */}
                  {menuCol === col.id && (
                    <ColumnHeaderMenu
                      column={col}
                      columnIndex={colIdx}
                      totalColumns={schema.columns.length}
                      onClose={() => setMenuCol(null)}
                      onRemove={() => handleRemoveColumn(col.id)}
                      onMoveLeft={() => handleMoveColumn(col.id, "left")}
                      onMoveRight={() => handleMoveColumn(col.id, "right")}
                      onSort={(desc) => handleSort(col.id, desc)}
                    />
                  )}
                </th>
              ))}
              <th className="db-table-th db-table-th-add">
                <button
                  type="button"
                  className="db-table-add-col-btn-header"
                  onClick={() => setShowAddCol(true)}
                  title="Add column"
                >
                  +
                </button>
                {showAddCol && (
                  <AddColumnPopover
                    onAdd={handleAddColumn}
                    onClose={() => setShowAddCol(false)}
                  />
                )}
              </th>
              <th className="db-table-th db-table-th-actions-col" />
            </tr>
          </thead>
          <tbody>
            {displayRows.map((row) => (
              <tr key={row.id} className="db-table-tr">
                {schema.columns.map((col) => (
                  <td key={col.id} className="db-table-td">
                    {editingCell?.rowId === row.id &&
                    editingCell?.colId === col.id ? (
                      <CellEditor
                        value={row.fields[col.id]}
                        column={col}
                        onSave={(val) => handleCellSave(row.id, col.id, val)}
                        onCancel={() => setEditingCell(null)}
                      />
                    ) : (
                      <button
                        type="button"
                        className="db-table-cell-display"
                        onClick={() =>
                          setEditingCell({ rowId: row.id, colId: col.id })
                        }
                      >
                        {col.type === "checkbox" ? (
                          <input
                            type="checkbox"
                            checked={!!row.fields[col.id]}
                            onChange={(e) => {
                              e.stopPropagation();
                              handleCellSave(row.id, col.id, e.target.checked);
                            }}
                            className="db-table-checkbox"
                          />
                        ) : col.type === "url" && row.fields[col.id] ? (
                          <span className="db-table-url-cell">
                            {String(row.fields[col.id])}
                          </span>
                        ) : col.type === "multi-select" &&
                          Array.isArray(row.fields[col.id]) ? (
                          <span className="db-table-tags">
                            {(row.fields[col.id] as string[]).map((tag) => (
                              <span key={tag} className="db-table-tag">
                                {tag}
                              </span>
                            ))}
                          </span>
                        ) : col.type === "select" && row.fields[col.id] ? (
                          <span className="db-table-select-badge">
                            {String(row.fields[col.id])}
                          </span>
                        ) : (
                          <span>
                            {formatCellValue(row.fields[col.id], col.type)}
                          </span>
                        )}
                      </button>
                    )}
                  </td>
                ))}
                <td className="db-table-td db-table-td-add" />
                <td className="db-table-td db-table-td-actions">
                  <button
                    type="button"
                    className="db-table-row-delete"
                    onClick={() => handleDeleteRow(row.id)}
                    title="Delete row"
                  >
                    <TrashIcon />
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <div className="db-table-add-row-bar">
        <button
          type="button"
          className="db-table-add-row"
          onClick={handleAddRow}
        >
          + New Row
        </button>
        {schema.templates && Object.keys(schema.templates).length > 0 && (
          <DatabaseTemplateMenu
            dbId={databaseName}
            onRowCreated={(newRow) => setRows((prev) => [...prev, newRow])}
          />
        )}
      </div>
    </div>
  );
}

// ---- Small SVG icons ----
function FilterIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
      <polygon points="22 3 2 3 10 12.46 10 19 14 21 14 12.46 22 3" />
    </svg>
  );
}

function MoreIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
      <circle cx="12" cy="12" r="1" />
      <circle cx="12" cy="5" r="1" />
      <circle cx="12" cy="19" r="1" />
    </svg>
  );
}

function TrashIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
      <path d="M3 6h18" />
      <path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6" />
      <path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" />
    </svg>
  );
}

// ---- Database icon for slash menu ----
export function DatabaseIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
      <ellipse cx="12" cy="5" rx="9" ry="3" />
      <path d="M3 5V19A9 3 0 0 0 21 19V5" />
      <path d="M3 12A9 3 0 0 0 21 12" />
    </svg>
  );
}

// ---- BlockNote block spec ----
export const DatabaseTableBlock = createReactBlockSpec(
  {
    type: "databaseTable" as const,
    propSchema: {
      databaseName: { default: "" },
    },
    content: "none",
  },
  {
    render: (props) => {
      const { databaseName } = props.block.props;

      if (!databaseName) {
        return (
          <DatabasePicker
            onSelect={(dbId) => {
              props.editor.updateBlock(props.block, {
                props: { databaseName: dbId },
              });
            }}
          />
        );
      }

      return (
        <DatabaseTableRenderer
          databaseName={databaseName}
        />
      );
    },

    toExternalHTML: (props) => {
      const { databaseName } = props.block.props;
      if (!databaseName) return <p />;
      return (
        <p data-database-table={databaseName}>
          {`[database:${databaseName}](view:table)`}
        </p>
      );
    },

    parse: (element: HTMLElement) => {
      // Parse from data attribute
      const dbName = element.getAttribute("data-database-table");
      if (dbName) {
        return { databaseName: dbName };
      }

      // Parse from markdown reference: [database:name](view:table)
      const text = element.textContent?.trim() || "";
      const match = text.match(/^\[database:([^\]]+)\]\(view:table\)$/);
      if (match) {
        return { databaseName: match[1] };
      }

      return undefined;
    },
  },
);

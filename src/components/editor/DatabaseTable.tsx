import { useState, useEffect, useCallback, useRef, useMemo } from "react";
import { createReactBlockSpec } from "@blocknote/react";
import * as db from "../../services/database";
import { DatabaseCalendar } from "./DatabaseCalendar";
import type {
  ColumnDef,
  ColumnType,
  DatabaseSchema,
  DatabaseRow,
  DatabaseInfo,
  FilterCondition,
  FilterOperator,
  SortRule,
} from "../../types/database";
import { defaultFieldValue, COLUMN_TYPES, getOperatorsForType } from "../../types/database";
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

// ---- Structured filter condition matcher ----
function matchesCondition(
  value: unknown,
  condition: FilterCondition,
  colType: ColumnType,
): boolean {
  const { operator, value: filterVal } = condition;
  const str = String(value ?? "");
  const strLower = str.toLowerCase();
  const filterLower = filterVal.toLowerCase();

  switch (operator) {
    case "is_empty":
      return !value || str === "";
    case "is_not_empty":
      return !!value && str !== "";
    case "contains":
      if (colType === "multi-select" && Array.isArray(value)) {
        return (value as string[]).some((v) => v.toLowerCase().includes(filterLower));
      }
      return strLower.includes(filterLower);
    case "not_contains":
      if (colType === "multi-select" && Array.isArray(value)) {
        return !(value as string[]).some((v) => v.toLowerCase().includes(filterLower));
      }
      return !strLower.includes(filterLower);
    case "equals":
      if (colType === "number") return Number(value) === Number(filterVal);
      return strLower === filterLower;
    case "not_equals":
      if (colType === "number") return Number(value) !== Number(filterVal);
      return strLower !== filterLower;
    case "gt":
      return Number(value) > Number(filterVal);
    case "lt":
      return Number(value) < Number(filterVal);
    case "gte":
      return Number(value) >= Number(filterVal);
    case "lte":
      return Number(value) <= Number(filterVal);
    case "before":
      return str < filterVal;
    case "after":
      return str > filterVal;
    case "is":
      if (colType === "checkbox") return String(!!value) === filterVal;
      return strLower === filterLower;
    case "is_not":
      if (colType === "checkbox") return String(!!value) !== filterVal;
      return strLower !== filterLower;
    default:
      return true;
  }
}

// ---- Filter Builder Popover ----
function FilterBuilder({
  columns,
  conditions,
  filterLogic,
  onChange,
  onLogicChange,
  onClose,
}: {
  columns: ColumnDef[];
  conditions: FilterCondition[];
  filterLogic: "and" | "or";
  onChange: (conditions: FilterCondition[]) => void;
  onLogicChange: (logic: "and" | "or") => void;
  onClose: () => void;
}) {
  const addCondition = () => {
    if (columns.length === 0) return;
    const ops = getOperatorsForType(columns[0].type);
    onChange([...conditions, { column: columns[0].id, operator: ops[0].value, value: "" }]);
  };

  const updateCondition = (idx: number, patch: Partial<FilterCondition>) => {
    const next = conditions.map((c, i) => (i === idx ? { ...c, ...patch } : c));
    // If column changed, reset operator to first valid one for the new type
    if (patch.column) {
      const col = columns.find((c) => c.id === patch.column);
      if (col) {
        const ops = getOperatorsForType(col.type);
        if (!ops.find((o) => o.value === next[idx].operator)) {
          next[idx].operator = ops[0].value;
        }
      }
    }
    onChange(next);
  };

  const removeCondition = (idx: number) => {
    onChange(conditions.filter((_, i) => i !== idx));
  };

  return (
    <div className="db-filter-builder" onClick={(e) => e.stopPropagation()}>
      <div className="db-filter-builder-header">
        <span className="db-filter-builder-title">Filters</span>
        <button type="button" className="db-filter-builder-close" onClick={onClose}>×</button>
      </div>
      {conditions.length > 1 && (
        <div className="db-filter-logic">
          <span className="db-filter-logic-label">Match</span>
          <select
            className="db-filter-logic-select"
            value={filterLogic}
            onChange={(e) => onLogicChange(e.target.value as "and" | "or")}
          >
            <option value="and">All (AND)</option>
            <option value="or">Any (OR)</option>
          </select>
        </div>
      )}
      {conditions.map((cond, idx) => {
        const col = columns.find((c) => c.id === cond.column);
        const ops = col ? getOperatorsForType(col.type) : [];
        const needsValue = cond.operator !== "is_empty" && cond.operator !== "is_not_empty";
        return (
          <div key={idx} className="db-filter-row">
            <select
              className="db-filter-select"
              value={cond.column}
              onChange={(e) => updateCondition(idx, { column: e.target.value })}
            >
              {columns.map((c) => (
                <option key={c.id} value={c.id}>{c.name}</option>
              ))}
            </select>
            <select
              className="db-filter-select"
              value={cond.operator}
              onChange={(e) => updateCondition(idx, { operator: e.target.value as FilterOperator })}
            >
              {ops.map((o) => (
                <option key={o.value} value={o.value}>{o.label}</option>
              ))}
            </select>
            {needsValue && (
              col?.type === "select" && col.options ? (
                <select
                  className="db-filter-select db-filter-value"
                  value={cond.value}
                  onChange={(e) => updateCondition(idx, { value: e.target.value })}
                >
                  <option value="">—</option>
                  {col.options.map((opt) => (
                    <option key={opt} value={opt}>{opt}</option>
                  ))}
                </select>
              ) : col?.type === "checkbox" ? (
                <select
                  className="db-filter-select db-filter-value"
                  value={cond.value}
                  onChange={(e) => updateCondition(idx, { value: e.target.value })}
                >
                  <option value="true">Checked</option>
                  <option value="false">Unchecked</option>
                </select>
              ) : (
                <input
                  type={col?.type === "number" ? "number" : col?.type === "date" ? "date" : "text"}
                  className="db-filter-input"
                  placeholder="Value..."
                  value={cond.value}
                  onChange={(e) => updateCondition(idx, { value: e.target.value })}
                  onKeyDown={(e) => e.stopPropagation()}
                />
              )
            )}
            <button type="button" className="db-filter-remove" onClick={() => removeCondition(idx)}>×</button>
          </div>
        );
      })}
      <button type="button" className="db-filter-add" onClick={addCondition}>+ Add filter</button>
    </div>
  );
}

// ---- Sort Builder Popover ----
function SortBuilder({
  columns,
  sorts,
  onChange,
  onClose,
}: {
  columns: ColumnDef[];
  sorts: SortRule[];
  onChange: (sorts: SortRule[]) => void;
  onClose: () => void;
}) {
  const addSort = () => {
    if (columns.length === 0) return;
    // Pick the first column not already in sorts
    const used = new Set(sorts.map((s) => s.column));
    const next = columns.find((c) => !used.has(c.id)) || columns[0];
    onChange([...sorts, { column: next.id, direction: "asc" }]);
  };

  const updateSort = (idx: number, patch: Partial<SortRule>) => {
    onChange(sorts.map((s, i) => (i === idx ? { ...s, ...patch } : s)));
  };

  const removeSort = (idx: number) => {
    onChange(sorts.filter((_, i) => i !== idx));
  };

  return (
    <div className="db-sort-builder" onClick={(e) => e.stopPropagation()}>
      <div className="db-filter-builder-header">
        <span className="db-filter-builder-title">Sort</span>
        <button type="button" className="db-filter-builder-close" onClick={onClose}>×</button>
      </div>
      {sorts.map((sort, idx) => (
        <div key={idx} className="db-filter-row">
          {idx > 0 && <span className="db-sort-then">then</span>}
          <select
            className="db-filter-select"
            value={sort.column}
            onChange={(e) => updateSort(idx, { column: e.target.value })}
          >
            {columns.map((c) => (
              <option key={c.id} value={c.id}>{c.name}</option>
            ))}
          </select>
          <select
            className="db-filter-select"
            value={sort.direction}
            onChange={(e) => updateSort(idx, { direction: e.target.value as "asc" | "desc" })}
          >
            <option value="asc">Ascending</option>
            <option value="desc">Descending</option>
          </select>
          <button type="button" className="db-filter-remove" onClick={() => removeSort(idx)}>×</button>
        </div>
      ))}
      <button type="button" className="db-filter-add" onClick={addSort}>+ Add sort level</button>
    </div>
  );
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
// ---- Relation Cell Editor ----
function RelationCellEditor({
  value,
  column,
  onSave,
  onClose,
}: {
  value: unknown;
  column: ColumnDef;
  onSave: (val: unknown) => void;
  onClose: () => void;
}) {
  const [targetRows, setTargetRows] = useState<DatabaseRow[]>([]);
  const [targetSchema, setTargetSchema] = useState<DatabaseSchema | null>(null);
  const [search, setSearch] = useState("");
  const selectedIds = Array.isArray(value) ? (value as string[]) : [];

  useEffect(() => {
    if (!column.target) return;
    db.getDatabase(column.target).then((result) => {
      setTargetSchema(result.schema);
      setTargetRows(result.rows);
    }).catch(console.error);
  }, [column.target]);

  const titleCol = targetSchema?.columns.find((c) => c.type === "text") ?? targetSchema?.columns[0];

  const getTitle = (row: DatabaseRow) => {
    if (!titleCol) return row.id;
    const val = row.fields[titleCol.id];
    return typeof val === "string" && val.trim() ? val : row.id;
  };

  const filtered = targetRows.filter((r) => {
    if (!search) return true;
    return getTitle(r).toLowerCase().includes(search.toLowerCase());
  });

  const toggle = (rowId: string) => {
    const newIds = selectedIds.includes(rowId)
      ? selectedIds.filter((id) => id !== rowId)
      : [...selectedIds, rowId];
    onSave(newIds);
  };

  return (
    <div className="db-relation-picker" onClick={(e) => e.stopPropagation()}>
      <input
        className="db-relation-search"
        placeholder="Search..."
        value={search}
        onChange={(e) => setSearch(e.target.value)}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === "Escape") onClose();
        }}
        autoFocus
      />
      <div className="db-relation-list">
        {filtered.map((row) => (
          <label key={row.id} className="db-relation-option">
            <input
              type="checkbox"
              checked={selectedIds.includes(row.id)}
              onChange={() => toggle(row.id)}
            />
            <span>{getTitle(row)}</span>
          </label>
        ))}
        {filtered.length === 0 && (
          <div className="db-relation-empty">No rows found</div>
        )}
      </div>
      <button type="button" className="db-relation-done" onClick={onClose}>Done</button>
    </div>
  );
}

// ---- Relation Tags Display ----
function RelationTags({
  value,
  targetDb,
}: {
  value: unknown;
  targetDb?: string;
}) {
  const [titles, setTitles] = useState<Record<string, string>>({});
  const ids = Array.isArray(value) ? (value as string[]) : [];

  useEffect(() => {
    if (!targetDb || ids.length === 0) return;
    db.getDatabase(targetDb).then((result) => {
      const titleCol = result.schema.columns.find((c) => c.type === "text") ?? result.schema.columns[0];
      const map: Record<string, string> = {};
      for (const row of result.rows) {
        if (ids.includes(row.id)) {
          const val = titleCol ? row.fields[titleCol.id] : undefined;
          map[row.id] = typeof val === "string" && val.trim() ? val : row.id;
        }
      }
      setTitles(map);
    }).catch(console.error);
  }, [targetDb, ids.join(",")]);

  if (ids.length === 0) return <span className="db-table-cell-empty" />;

  return (
    <span className="db-relation-tags">
      {ids.map((id) => (
        <span key={id} className="db-relation-tag">
          {titles[id] || id}
        </span>
      ))}
    </span>
  );
}

// ---- Rollup computation ----
function RollupDisplay({
  row,
  column,
  schema,
}: {
  row: DatabaseRow;
  column: ColumnDef;
  schema: DatabaseSchema;
}) {
  const [result, setResult] = useState<string>("—");

  useEffect(() => {
    if (!column.relation || !column.target_column || !column.function) return;

    // Find the relation column
    const relCol = schema.columns.find((c) => c.id === column.relation);
    if (!relCol || relCol.type !== "relation" || !relCol.target) return;

    // Get the related row IDs
    const relatedIds = Array.isArray(row.fields[relCol.id]) ? (row.fields[relCol.id] as string[]) : [];
    if (relatedIds.length === 0) { setResult("—"); return; }

    // Load target database
    db.getDatabase(relCol.target).then((targetResult) => {
      const relatedRows = targetResult.rows.filter((r) => relatedIds.includes(r.id));
      const values = relatedRows.map((r) => r.fields[column.target_column!]);

      let computed: string;
      switch (column.function) {
        case "count":
          computed = String(relatedRows.length);
          break;
        case "sum": {
          const sum = values.reduce((acc: number, v) => acc + (Number(v) || 0), 0);
          computed = String(sum);
          break;
        }
        case "average": {
          const nums = values.filter((v) => v !== null && v !== undefined && v !== "");
          if (nums.length === 0) { computed = "—"; break; }
          const avg = nums.reduce((acc: number, v) => acc + (Number(v) || 0), 0) / nums.length;
          computed = avg.toFixed(2);
          break;
        }
        case "min": {
          const nums = values.map((v) => Number(v)).filter((n) => !isNaN(n));
          computed = nums.length > 0 ? String(Math.min(...nums)) : "—";
          break;
        }
        case "max": {
          const nums = values.map((v) => Number(v)).filter((n) => !isNaN(n));
          computed = nums.length > 0 ? String(Math.max(...nums)) : "—";
          break;
        }
        case "percent_checked": {
          const total = values.length;
          const checked = values.filter((v) => v === true).length;
          computed = total > 0 ? `${Math.round((checked / total) * 100)}%` : "—";
          break;
        }
        default:
          computed = "—";
      }
      setResult(computed);
    }).catch(() => setResult("Error"));
  }, [row, column, schema]);

  return <span className="db-rollup-value">{result}</span>;
}

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

  // Sort state (legacy single + structured multi-level)
  const [sortCol, setSortCol] = useState<string | null>(null);
  const [sortDesc, setSortDesc] = useState(false);

  // Filter state: columnId -> filter string (legacy per-column)
  const [filters, setFilters] = useState<Record<string, string>>({});
  const [activeFilter, setActiveFilter] = useState<string | null>(null);

  // Structured filter/sort state
  const [filterConditions, setFilterConditions] = useState<FilterCondition[]>([]);
  const [filterLogic, setFilterLogic] = useState<"and" | "or">("and");
  const [sortRules, setSortRules] = useState<SortRule[]>([]);
  const [filterBuilderOpen, setFilterBuilderOpen] = useState(false);
  const [sortBuilderOpen, setSortBuilderOpen] = useState(false);

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

    // Apply legacy per-column filters
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

    // Apply structured filter conditions
    if (schema && filterConditions.length > 0) {
      result = result.filter((row) => {
        const results = filterConditions.map((cond) => {
          const col = schema.columns.find((c) => c.id === cond.column);
          if (!col) return true;
          return matchesCondition(row.fields[cond.column], cond, col.type);
        });
        return filterLogic === "and"
          ? results.every(Boolean)
          : results.some(Boolean);
      });
    }

    // Apply structured multi-level sorts (takes priority)
    if (sortRules.length > 0 && schema) {
      result.sort((a, b) => {
        for (const rule of sortRules) {
          const col = schema.columns.find((c) => c.id === rule.column);
          if (!col) continue;
          const cmp = compareValues(a.fields[rule.column], b.fields[rule.column], col.type);
          if (cmp !== 0) return rule.direction === "desc" ? -cmp : cmp;
        }
        return 0;
      });
    } else if (sortCol && schema) {
      // Fallback to legacy single sort
      const col = schema.columns.find((c) => c.id === sortCol);
      if (col) {
        result.sort((a, b) => {
          const cmp = compareValues(a.fields[sortCol], b.fields[sortCol], col.type);
          return sortDesc ? -cmp : cmp;
        });
      }
    }

    return result;
  }, [rows, filters, filterConditions, filterLogic, sortRules, sortCol, sortDesc, schema]);

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
          {displayRows.length === rows.length
            ? `${rows.length} rows`
            : `${displayRows.length} of ${rows.length} rows`}
        </span>
      </div>

      {/* Filter & Sort toolbar */}
      <div className="db-toolbar">
        <div className="db-toolbar-buttons">
          <button
            type="button"
            className={`db-toolbar-btn ${filterConditions.length > 0 ? "db-toolbar-btn-active" : ""}`}
            onClick={() => { setSortBuilderOpen(false); setFilterBuilderOpen((v) => !v); }}
          >
            <FilterIcon /> Filter{filterConditions.length > 0 ? ` (${filterConditions.length})` : ""}
          </button>
          <button
            type="button"
            className={`db-toolbar-btn ${sortRules.length > 0 ? "db-toolbar-btn-active" : ""}`}
            onClick={() => { setFilterBuilderOpen(false); setSortBuilderOpen((v) => !v); }}
          >
            <SortIcon /> Sort{sortRules.length > 0 ? ` (${sortRules.length})` : ""}
          </button>
        </div>

        {/* Active filter/sort pills */}
        {(filterConditions.length > 0 || sortRules.length > 0) && (
          <div className="db-toolbar-pills">
            {filterConditions.map((cond, idx) => {
              const col = schema.columns.find((c) => c.id === cond.column);
              return (
                <span key={`f-${idx}`} className="db-toolbar-pill">
                  {col?.name || cond.column} {cond.operator.replace("_", " ")} {cond.value}
                  <button
                    type="button"
                    className="db-toolbar-pill-remove"
                    onClick={() => setFilterConditions((prev) => prev.filter((_, i) => i !== idx))}
                  >×</button>
                </span>
              );
            })}
            {sortRules.map((rule, idx) => {
              const col = schema.columns.find((c) => c.id === rule.column);
              return (
                <span key={`s-${idx}`} className="db-toolbar-pill db-toolbar-pill-sort">
                  {col?.name || rule.column} {rule.direction === "asc" ? "↑" : "↓"}
                  <button
                    type="button"
                    className="db-toolbar-pill-remove"
                    onClick={() => setSortRules((prev) => prev.filter((_, i) => i !== idx))}
                  >×</button>
                </span>
              );
            })}
            <button
              type="button"
              className="db-toolbar-clear"
              onClick={() => { setFilterConditions([]); setSortRules([]); }}
            >
              Clear all
            </button>
          </div>
        )}

        {/* Filter builder popover */}
        {filterBuilderOpen && (
          <FilterBuilder
            columns={schema.columns}
            conditions={filterConditions}
            filterLogic={filterLogic}
            onChange={setFilterConditions}
            onLogicChange={setFilterLogic}
            onClose={() => setFilterBuilderOpen(false)}
          />
        )}

        {/* Sort builder popover */}
        {sortBuilderOpen && (
          <SortBuilder
            columns={schema.columns}
            sorts={sortRules}
            onChange={setSortRules}
            onClose={() => setSortBuilderOpen(false)}
          />
        )}
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
                    {col.type === "rollup" ? (
                      <div className="db-table-cell-display db-rollup-cell">
                        <RollupDisplay row={row} column={col} schema={schema} />
                      </div>
                    ) : col.type === "relation" ? (
                      editingCell?.rowId === row.id && editingCell?.colId === col.id ? (
                        <RelationCellEditor
                          value={row.fields[col.id]}
                          column={col}
                          onSave={(val) => handleCellSave(row.id, col.id, val)}
                          onClose={() => setEditingCell(null)}
                        />
                      ) : (
                        <button
                          type="button"
                          className="db-table-cell-display"
                          onClick={() => setEditingCell({ rowId: row.id, colId: col.id })}
                        >
                          <RelationTags value={row.fields[col.id]} targetDb={col.target} />
                        </button>
                      )
                    ) : editingCell?.rowId === row.id &&
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

function SortIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
      <path d="M3 6h18" />
      <path d="M7 12h10" />
      <path d="M10 18h4" />
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

// ---- View Switcher ----
function ViewSwitcher({
  currentView,
  onSwitch,
}: {
  currentView: string;
  onSwitch: (view: string) => void;
}) {
  const views = [
    { id: "table", label: "Table", icon: <TableViewIcon /> },
    { id: "calendar", label: "Calendar", icon: <CalendarViewIcon /> },
  ];
  return (
    <div className="db-view-switcher">
      {views.map((v) => (
        <button
          key={v.id}
          type="button"
          className={`db-view-switcher-btn ${currentView === v.id ? "db-view-switcher-btn-active" : ""}`}
          onClick={() => onSwitch(v.id)}
          title={v.label}
        >
          {v.icon}
          <span>{v.label}</span>
        </button>
      ))}
    </div>
  );
}

function TableViewIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
      <rect x="3" y="3" width="18" height="18" rx="2" />
      <path d="M3 9h18" />
      <path d="M3 15h18" />
      <path d="M9 3v18" />
    </svg>
  );
}

function CalendarViewIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
      <rect x="3" y="4" width="18" height="18" rx="2" />
      <path d="M16 2v4" />
      <path d="M8 2v4" />
      <path d="M3 10h18" />
    </svg>
  );
}

// ---- BlockNote block spec ----
export const DatabaseTableBlock = createReactBlockSpec(
  {
    type: "databaseTable" as const,
    propSchema: {
      databaseName: { default: "" },
      view: { default: "table" },
    },
    content: "none",
  },
  {
    render: (props) => {
      const { databaseName, view } = props.block.props;

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
        <div contentEditable={false}>
          <ViewSwitcher
            currentView={view || "table"}
            onSwitch={(newView) => {
              props.editor.updateBlock(props.block, {
                props: { view: newView },
              });
            }}
          />
          {view === "calendar" ? (
            <DatabaseCalendar databaseName={databaseName} />
          ) : (
            <DatabaseTableRenderer databaseName={databaseName} />
          )}
        </div>
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

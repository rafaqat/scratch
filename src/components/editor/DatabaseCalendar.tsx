import { useState, useEffect, useCallback, useMemo } from "react";
import * as db from "../../services/database";
import type {
  DatabaseSchema,
  DatabaseRow,
  ColumnDef,
} from "../../types/database";
import { defaultFieldValue } from "../../types/database";

// ---- Helpers ----

function getTitleColumn(schema: DatabaseSchema): ColumnDef | undefined {
  return schema.columns.find((c) => c.type === "text") ?? schema.columns[0];
}

function getRowTitle(row: DatabaseRow, titleCol?: ColumnDef): string {
  if (!titleCol) return row.id;
  const val = row.fields[titleCol.id];
  if (typeof val === "string" && val.trim()) return val;
  return row.id;
}

function getDateColumns(schema: DatabaseSchema): ColumnDef[] {
  return schema.columns.filter((c) => c.type === "date");
}

function getDaysInMonth(year: number, month: number): number {
  return new Date(year, month + 1, 0).getDate();
}

function getFirstDayOfWeek(year: number, month: number): number {
  // 0=Sunday, adjust to Monday start: (day + 6) % 7
  const day = new Date(year, month, 1).getDay();
  return (day + 6) % 7;
}

function formatYearMonth(year: number, month: number): string {
  const d = new Date(year, month, 1);
  return d.toLocaleDateString(undefined, { month: "long", year: "numeric" });
}

function toDateStr(year: number, month: number, day: number): string {
  return `${year}-${String(month + 1).padStart(2, "0")}-${String(day).padStart(2, "0")}`;
}

function isToday(year: number, month: number, day: number): boolean {
  const now = new Date();
  return now.getFullYear() === year && now.getMonth() === month && now.getDate() === day;
}

const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

// ---- Calendar View ----

export function DatabaseCalendar({
  databaseName,
  dateColumnId,
}: {
  databaseName: string;
  dateColumnId?: string;
}) {
  const [schema, setSchema] = useState<DatabaseSchema | null>(null);
  const [rows, setRows] = useState<DatabaseRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Calendar navigation
  const now = new Date();
  const [viewYear, setViewYear] = useState(now.getFullYear());
  const [viewMonth, setViewMonth] = useState(now.getMonth());

  // Date column selection
  const [selectedDateCol, setSelectedDateCol] = useState<string | null>(dateColumnId || null);

  // Editing row
  const [editingRowId, setEditingRowId] = useState<string | null>(null);
  const [editingTitle, setEditingTitle] = useState("");

  // Drag state
  const [dragRowId, setDragRowId] = useState<string | null>(null);

  const loadData = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await db.getDatabase(databaseName);
      setSchema(result.schema);
      setRows(result.rows);
      // Auto-select first date column if none set
      if (!selectedDateCol) {
        const dateCols = getDateColumns(result.schema);
        if (dateCols.length > 0) {
          setSelectedDateCol(dateCols[0].id);
        }
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [databaseName, selectedDateCol]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  const titleCol = schema ? getTitleColumn(schema) : undefined;
  const dateColumns = schema ? getDateColumns(schema) : [];

  // Group rows by date
  const { dated, noDate } = useMemo(() => {
    const dated: Record<string, DatabaseRow[]> = {};
    const noDate: DatabaseRow[] = [];
    if (!selectedDateCol) return { dated, noDate: rows };

    for (const row of rows) {
      const dateVal = row.fields[selectedDateCol];
      if (typeof dateVal === "string" && dateVal.match(/^\d{4}-\d{2}-\d{2}/)) {
        const key = dateVal.slice(0, 10);
        if (!dated[key]) dated[key] = [];
        dated[key].push(row);
      } else {
        noDate.push(row);
      }
    }
    return { dated, noDate };
  }, [rows, selectedDateCol]);

  // Calendar grid data
  const daysInMonth = getDaysInMonth(viewYear, viewMonth);
  const firstDay = getFirstDayOfWeek(viewYear, viewMonth);
  const totalCells = Math.ceil((firstDay + daysInMonth) / 7) * 7;

  // Navigation
  const goToPrev = () => {
    if (viewMonth === 0) { setViewYear((y) => y - 1); setViewMonth(11); }
    else setViewMonth((m) => m - 1);
  };
  const goToNext = () => {
    if (viewMonth === 11) { setViewYear((y) => y + 1); setViewMonth(0); }
    else setViewMonth((m) => m + 1);
  };
  const goToToday = () => {
    const now = new Date();
    setViewYear(now.getFullYear());
    setViewMonth(now.getMonth());
  };

  // Create row on date click
  const handleDateClick = useCallback(async (dateStr: string) => {
    if (!schema || !selectedDateCol) return;
    try {
      const fields: Record<string, unknown> = {};
      for (const col of schema.columns) {
        fields[col.id] = defaultFieldValue(col.type);
      }
      fields[selectedDateCol] = dateStr;
      const newRow = await db.createRow(databaseName, fields);
      setRows((prev) => [...prev, newRow]);
      // Start editing the title
      setEditingRowId(newRow.id);
      setEditingTitle("");
    } catch (err) {
      console.error("Failed to create row:", err);
    }
  }, [databaseName, schema, selectedDateCol]);

  // Save edit
  const handleSaveEdit = useCallback(async (rowId: string, title: string) => {
    if (!titleCol) return;
    try {
      const updatedRow = await db.updateRow(databaseName, rowId, { [titleCol.id]: title });
      setRows((prev) => prev.map((r) => (r.id === rowId ? updatedRow : r)));
    } catch (err) {
      console.error("Failed to update row:", err);
    }
    setEditingRowId(null);
  }, [databaseName, titleCol]);

  // Drop handler: move row to new date
  const handleDrop = useCallback(async (rowId: string, newDate: string) => {
    if (!selectedDateCol) return;
    try {
      const updatedRow = await db.updateRow(databaseName, rowId, { [selectedDateCol]: newDate });
      setRows((prev) => prev.map((r) => (r.id === rowId ? updatedRow : r)));
    } catch (err) {
      console.error("Failed to move row:", err);
    }
    setDragRowId(null);
  }, [databaseName, selectedDateCol]);

  if (loading) {
    return <div className="db-calendar" contentEditable={false}><div className="db-calendar-loading">Loading calendar...</div></div>;
  }

  if (error) {
    return <div className="db-calendar" contentEditable={false}><div className="db-calendar-error">{error}</div></div>;
  }

  if (!schema) return null;

  if (dateColumns.length === 0) {
    return (
      <div className="db-calendar" contentEditable={false}>
        <div className="db-calendar-empty">
          No date columns found. Add a date column to use calendar view.
        </div>
      </div>
    );
  }

  return (
    <div className="db-calendar" contentEditable={false}>
      {/* Header */}
      <div className="db-calendar-header">
        <div className="db-calendar-nav">
          <button type="button" className="db-calendar-nav-btn" onClick={goToPrev}>‹</button>
          <span className="db-calendar-month">{formatYearMonth(viewYear, viewMonth)}</span>
          <button type="button" className="db-calendar-nav-btn" onClick={goToNext}>›</button>
          <button type="button" className="db-calendar-today-btn" onClick={goToToday}>Today</button>
        </div>
        {dateColumns.length > 1 && (
          <select
            className="db-calendar-col-select"
            value={selectedDateCol || ""}
            onChange={(e) => setSelectedDateCol(e.target.value)}
          >
            {dateColumns.map((c) => (
              <option key={c.id} value={c.id}>{c.name}</option>
            ))}
          </select>
        )}
      </div>

      {/* Weekday headers */}
      <div className="db-calendar-grid db-calendar-weekdays">
        {WEEKDAYS.map((day) => (
          <div key={day} className="db-calendar-weekday">{day}</div>
        ))}
      </div>

      {/* Calendar grid */}
      <div className="db-calendar-grid db-calendar-days">
        {Array.from({ length: totalCells }, (_, i) => {
          const dayNum = i - firstDay + 1;
          const isCurrentMonth = dayNum >= 1 && dayNum <= daysInMonth;
          const dateStr = isCurrentMonth ? toDateStr(viewYear, viewMonth, dayNum) : "";
          const dayRows = isCurrentMonth ? (dated[dateStr] || []) : [];
          const isTodayCell = isCurrentMonth && isToday(viewYear, viewMonth, dayNum);

          return (
            <div
              key={i}
              className={`db-calendar-day ${!isCurrentMonth ? "db-calendar-day-outside" : ""} ${isTodayCell ? "db-calendar-day-today" : ""}`}
              onClick={() => isCurrentMonth && handleDateClick(dateStr)}
              onDragOver={(e) => { if (isCurrentMonth && dragRowId) { e.preventDefault(); e.currentTarget.classList.add("db-calendar-day-drop"); } }}
              onDragLeave={(e) => e.currentTarget.classList.remove("db-calendar-day-drop")}
              onDrop={(e) => {
                e.preventDefault();
                e.currentTarget.classList.remove("db-calendar-day-drop");
                if (dragRowId && isCurrentMonth) handleDrop(dragRowId, dateStr);
              }}
            >
              {isCurrentMonth && (
                <>
                  <span className={`db-calendar-day-num ${isTodayCell ? "db-calendar-day-num-today" : ""}`}>
                    {dayNum}
                  </span>
                  <div className="db-calendar-day-rows">
                    {dayRows.map((row) => (
                      <div
                        key={row.id}
                        className="db-calendar-row-card"
                        draggable
                        onDragStart={() => setDragRowId(row.id)}
                        onDragEnd={() => setDragRowId(null)}
                        onClick={(e) => {
                          e.stopPropagation();
                          setEditingRowId(row.id);
                          setEditingTitle(getRowTitle(row, titleCol));
                        }}
                      >
                        {editingRowId === row.id ? (
                          <input
                            className="db-calendar-row-edit"
                            value={editingTitle}
                            onChange={(e) => setEditingTitle(e.target.value)}
                            onBlur={() => handleSaveEdit(row.id, editingTitle)}
                            onKeyDown={(e) => {
                              e.stopPropagation();
                              if (e.key === "Enter") handleSaveEdit(row.id, editingTitle);
                              if (e.key === "Escape") setEditingRowId(null);
                            }}
                            autoFocus
                            onClick={(e) => e.stopPropagation()}
                          />
                        ) : (
                          <span className="db-calendar-row-title">{getRowTitle(row, titleCol)}</span>
                        )}
                      </div>
                    ))}
                  </div>
                </>
              )}
            </div>
          );
        })}
      </div>

      {/* No-date section */}
      {noDate.length > 0 && (
        <div className="db-calendar-nodate">
          <span className="db-calendar-nodate-label">No date ({noDate.length})</span>
          <div className="db-calendar-nodate-rows">
            {noDate.map((row) => (
              <div
                key={row.id}
                className="db-calendar-row-card"
                draggable
                onDragStart={() => setDragRowId(row.id)}
                onDragEnd={() => setDragRowId(null)}
              >
                <span className="db-calendar-row-title">{getRowTitle(row, titleCol)}</span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

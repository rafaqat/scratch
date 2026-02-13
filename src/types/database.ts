// Database column types supported by the schema
export type ColumnType =
  | "text"
  | "number"
  | "date"
  | "select"
  | "multi-select"
  | "checkbox"
  | "relation"
  | "url";

// Column definition in a database schema
export interface ColumnDef {
  id: string;
  name: string;
  type: ColumnType;
  /** For select/multi-select: allowed option values */
  options?: string[];
  /** For relation: target database folder name */
  target?: string;
}

// View types
export type ViewType = "table" | "board";

// View definition in a database schema
export interface ViewDef {
  id: string;
  name: string;
  type: ViewType;
  /** For board view: which select column to group by */
  group_by?: string;
  /** Column IDs to display (if empty, show all) */
  columns?: string[];
  /** Sort column ID */
  sort_by?: string;
  /** Sort descending */
  sort_desc?: boolean;
}

// Full database schema (parsed from _schema.md frontmatter)
export interface DatabaseSchema {
  name: string;
  columns: ColumnDef[];
  views: ViewDef[];
  next_row_id: number;
}

// A single database row (parsed from row-NNN.md)
export interface DatabaseRow {
  /** Row filename stem (e.g., "row-001") */
  id: string;
  /** Map of column_id -> value */
  fields: Record<string, unknown>;
  /** Optional markdown body below frontmatter */
  body: string;
  /** File path */
  path: string;
  /** Last modified timestamp (unix seconds) */
  modified: number;
}

// Summary info about a database (for listing)
export interface DatabaseInfo {
  /** Database folder name relative to notes folder */
  id: string;
  /** Human-readable name from schema */
  name: string;
  /** Number of rows */
  row_count: number;
  /** Number of columns */
  column_count: number;
  /** Absolute folder path */
  path: string;
}

// Field value types mapped from ColumnType
export type FieldValue = string | number | boolean | string[] | null;

// Helper: get default value for a column type
export function defaultFieldValue(colType: ColumnType): FieldValue {
  switch (colType) {
    case "text":
    case "date":
    case "select":
    case "relation":
    case "url":
      return "";
    case "number":
      return 0;
    case "checkbox":
      return false;
    case "multi-select":
      return [];
    default:
      return "";
  }
}

// All supported column types for UI rendering
export const COLUMN_TYPES: { value: ColumnType; label: string }[] = [
  { value: "text", label: "Text" },
  { value: "number", label: "Number" },
  { value: "date", label: "Date" },
  { value: "select", label: "Select" },
  { value: "multi-select", label: "Multi-select" },
  { value: "checkbox", label: "Checkbox" },
  { value: "relation", label: "Relation" },
  { value: "url", label: "URL" },
];

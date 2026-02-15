// Database column types supported by the schema
export type ColumnType =
  | "text"
  | "number"
  | "date"
  | "select"
  | "multi-select"
  | "checkbox"
  | "relation"
  | "url"
  | "rollup";

// Rollup aggregate functions
export type RollupFunction = "count" | "sum" | "average" | "min" | "max" | "percent_checked";

// Column definition in a database schema
export interface ColumnDef {
  id: string;
  name: string;
  type: ColumnType;
  /** For select/multi-select: allowed option values */
  options?: string[];
  /** For relation: target database folder name */
  target?: string;
  /** For rollup: which relation column to aggregate through */
  relation?: string;
  /** For rollup: which column in the target database to aggregate */
  target_column?: string;
  /** For rollup: aggregation function */
  function?: RollupFunction;
}

// View types
export type ViewType = "table" | "board" | "calendar";

// Filter operators by column type
export type FilterOperator =
  | "contains" | "not_contains" | "equals" | "not_equals" | "is_empty" | "is_not_empty"  // text
  | "gt" | "lt" | "gte" | "lte"  // number
  | "before" | "after"  // date
  | "is" | "is_not";  // select / checkbox

// A single filter condition
export interface FilterCondition {
  column: string;
  operator: FilterOperator;
  value: string;
}

// A single sort rule
export interface SortRule {
  column: string;
  direction: "asc" | "desc";
}

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
  /** For calendar view: which date column to use */
  date_column?: string;
  /** Structured filter conditions */
  filters?: FilterCondition[];
  /** Structured sort rules (multiple levels) */
  sorts?: SortRule[];
  /** Filter combination logic */
  filter_logic?: "and" | "or";
}

// A named row template stored in the database schema
export interface RowTemplate {
  /** Template display name */
  name: string;
  /** Title pattern with {{variable}} placeholders */
  title?: string;
  /** Pre-filled field values (column_id -> value) */
  fields: Record<string, unknown>;
  /** Optional markdown body content */
  body?: string;
}

// Row template info returned from the backend (includes the template key)
export interface RowTemplateInfo {
  /** Template key (e.g., "bug-report") */
  id: string;
  /** Template display name */
  name: string;
  /** Title pattern with {{variable}} placeholders */
  title?: string;
  /** Pre-filled field values */
  fields: Record<string, unknown>;
  /** Optional markdown body content */
  body?: string;
}

// Full database schema (parsed from _schema.md frontmatter)
export interface DatabaseSchema {
  name: string;
  columns: ColumnDef[];
  views: ViewDef[];
  /** Named row templates for quick row creation */
  templates?: Record<string, RowTemplate>;
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
    case "url":
      return "";
    case "number":
      return 0;
    case "checkbox":
      return false;
    case "multi-select":
    case "relation":
      return [];
    case "rollup":
      return null; // computed value, not stored
    default:
      return "";
  }
}

// Available filter operators per column type
export function getOperatorsForType(colType: ColumnType): { value: FilterOperator; label: string }[] {
  const common: { value: FilterOperator; label: string }[] = [
    { value: "is_empty", label: "Is empty" },
    { value: "is_not_empty", label: "Is not empty" },
  ];
  switch (colType) {
    case "text":
    case "url":
      return [
        { value: "contains", label: "Contains" },
        { value: "not_contains", label: "Does not contain" },
        { value: "equals", label: "Equals" },
        { value: "not_equals", label: "Does not equal" },
        ...common,
      ];
    case "number":
      return [
        { value: "equals", label: "=" },
        { value: "not_equals", label: "≠" },
        { value: "gt", label: ">" },
        { value: "lt", label: "<" },
        { value: "gte", label: "≥" },
        { value: "lte", label: "≤" },
        ...common,
      ];
    case "date":
      return [
        { value: "equals", label: "Is" },
        { value: "before", label: "Before" },
        { value: "after", label: "After" },
        ...common,
      ];
    case "select":
    case "relation":
      return [
        { value: "is", label: "Is" },
        { value: "is_not", label: "Is not" },
        ...common,
      ];
    case "multi-select":
      return [
        { value: "contains", label: "Contains" },
        { value: "not_contains", label: "Does not contain" },
        ...common,
      ];
    case "checkbox":
      return [
        { value: "is", label: "Is" },
        { value: "is_not", label: "Is not" },
      ];
    default:
      return [
        { value: "contains", label: "Contains" },
        ...common,
      ];
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
  { value: "rollup", label: "Rollup" },
  { value: "url", label: "URL" },
];

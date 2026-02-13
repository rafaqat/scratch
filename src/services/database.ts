import { invoke } from "@tauri-apps/api/core";
import type {
  ColumnDef,
  DatabaseInfo,
  DatabaseRow,
  DatabaseSchema,
  ViewDef,
} from "../types/database";

// ---- Database CRUD ----

/** List all databases in the notes folder */
export async function listDatabases(): Promise<DatabaseInfo[]> {
  return invoke("db_list");
}

/** Create a new database */
export async function createDatabase(
  name: string,
  columns: ColumnDef[],
  views?: ViewDef[]
): Promise<DatabaseInfo> {
  return invoke("db_create", { name, columns, views: views ?? null });
}

/** Get full database: schema + all rows */
export async function getDatabase(
  dbId: string
): Promise<{ schema: DatabaseSchema; rows: DatabaseRow[] }> {
  return invoke("db_get", { dbId });
}

/** Get just the schema for a database */
export async function getSchema(dbId: string): Promise<DatabaseSchema> {
  return invoke("db_get_schema", { dbId });
}

/** Delete an entire database */
export async function deleteDatabase(dbId: string): Promise<void> {
  return invoke("db_delete", { dbId });
}

// ---- Row CRUD ----

/** Create a new row in a database */
export async function createRow(
  dbId: string,
  fields: Record<string, unknown>,
  body?: string
): Promise<DatabaseRow> {
  return invoke("db_create_row", { dbId, fields, body: body ?? null });
}

/** Update an existing row */
export async function updateRow(
  dbId: string,
  rowId: string,
  fields: Record<string, unknown>,
  body?: string
): Promise<DatabaseRow> {
  return invoke("db_update_row", {
    dbId,
    rowId,
    fields,
    body: body ?? null,
  });
}

/** Delete a row */
export async function deleteRow(
  dbId: string,
  rowId: string
): Promise<void> {
  return invoke("db_delete_row", { dbId, rowId });
}

// ---- Schema Migration ----

/** Add a column to a database (migrates existing rows) */
export async function addColumn(
  dbId: string,
  column: ColumnDef
): Promise<DatabaseSchema> {
  return invoke("db_add_column", { dbId, column });
}

/** Remove a column from a database (migrates existing rows) */
export async function removeColumn(
  dbId: string,
  columnId: string
): Promise<DatabaseSchema> {
  return invoke("db_remove_column", { dbId, columnId });
}

/** Rename a column in a database (migrates existing rows) */
export async function renameColumn(
  dbId: string,
  oldColumnId: string,
  newColumnId: string,
  newName?: string
): Promise<DatabaseSchema> {
  return invoke("db_rename_column", {
    dbId,
    oldColumnId,
    newColumnId,
    newName: newName ?? null,
  });
}

/** Update the full schema (handles add/remove column migrations) */
export async function updateSchema(
  dbId: string,
  schema: DatabaseSchema
): Promise<DatabaseSchema> {
  return invoke("db_update_schema", { dbId, schema });
}

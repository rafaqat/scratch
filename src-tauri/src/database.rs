use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ---- Column Types ----

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ColumnType {
    Text,
    Number,
    Date,
    Select,
    MultiSelect,
    Checkbox,
    Relation,
    Url,
}

impl ColumnType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ColumnType::Text => "text",
            ColumnType::Number => "number",
            ColumnType::Date => "date",
            ColumnType::Select => "select",
            ColumnType::MultiSelect => "multi-select",
            ColumnType::Checkbox => "checkbox",
            ColumnType::Relation => "relation",
            ColumnType::Url => "url",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        match s.trim() {
            "text" => Ok(ColumnType::Text),
            "number" => Ok(ColumnType::Number),
            "date" => Ok(ColumnType::Date),
            "select" => Ok(ColumnType::Select),
            "multi-select" => Ok(ColumnType::MultiSelect),
            "checkbox" => Ok(ColumnType::Checkbox),
            "relation" => Ok(ColumnType::Relation),
            "url" => Ok(ColumnType::Url),
            _ => Err(format!(
                "Invalid column type '{}'. Must be one of: text, number, date, select, multi-select, checkbox, relation, url",
                s
            )),
        }
    }

    /// Returns the default YAML value for this column type.
    pub fn default_value(&self) -> &'static str {
        match self {
            ColumnType::Text => "\"\"",
            ColumnType::Number => "0",
            ColumnType::Date => "\"\"",
            ColumnType::Select => "\"\"",
            ColumnType::MultiSelect => "[]",
            ColumnType::Checkbox => "false",
            ColumnType::Relation => "\"\"",
            ColumnType::Url => "\"\"",
        }
    }
}

// ---- Schema Types ----

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDef {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub col_type: ColumnType,
    /// For select/multi-select: allowed options
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<String>>,
    /// For relation: target database folder name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ViewType {
    Table,
    Board,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewDef {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub view_type: ViewType,
    /// For board view: which select column to group by
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_by: Option<String>,
    /// Column IDs to display (if empty, show all)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub columns: Option<Vec<String>>,
    /// Sort column ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_by: Option<String>,
    /// Sort direction
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_desc: Option<bool>,
}

/// A named row template stored in the database schema.
/// Templates pre-fill fields and optionally include a markdown body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RowTemplate {
    /// Display name for the template (e.g., "Bug Report")
    pub name: String,
    /// Title pattern with optional variable substitution (e.g., "Bug: {{title}}")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Pre-filled field values (column_id -> value)
    #[serde(default)]
    pub fields: HashMap<String, JsonValue>,
    /// Optional markdown body content
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseSchema {
    pub name: String,
    pub columns: Vec<ColumnDef>,
    #[serde(default)]
    pub views: Vec<ViewDef>,
    /// Named row templates for quick row creation
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub templates: HashMap<String, RowTemplate>,
    /// Auto-incrementing counter for row filenames
    #[serde(default = "default_next_row_id")]
    pub next_row_id: u32,
}

fn default_next_row_id() -> u32 {
    1
}

// ---- Row Types ----

/// A parsed database row: frontmatter fields + markdown body
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseRow {
    /// Row filename stem (e.g., "row-001")
    pub id: String,
    /// Map of column_id -> value (as JSON values for type flexibility)
    pub fields: HashMap<String, JsonValue>,
    /// Optional markdown body below frontmatter
    pub body: String,
    /// File path relative to notes folder
    pub path: String,
    /// Last modified timestamp (unix seconds)
    pub modified: i64,
}

/// Summary info about a database (returned by list/scan)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseInfo {
    /// Database folder name (relative to notes folder)
    pub id: String,
    /// Human-readable name from schema
    pub name: String,
    /// Number of rows
    pub row_count: usize,
    /// Column count
    pub column_count: usize,
    /// Folder path
    pub path: String,
}

// ---- Parsing ----

/// Split YAML frontmatter from markdown body.
/// Returns (yaml_str, body_str).
fn split_frontmatter(content: &str) -> Result<(String, String), String> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Err("Missing YAML frontmatter (no opening ---)".to_string());
    }

    let after_first = &trimmed[3..];
    let close_pos = after_first
        .find("\n---")
        .ok_or_else(|| "Missing closing --- for frontmatter".to_string())?;

    let yaml_str = after_first[..close_pos].trim().to_string();
    let body_start = close_pos + 4; // skip \n---
    let body = if body_start < after_first.len() {
        after_first[body_start..].trim_start_matches('\n').to_string()
    } else {
        String::new()
    };

    Ok((yaml_str, body))
}

/// Parse the `_schema.md` file into a DatabaseSchema.
pub fn parse_schema(content: &str) -> Result<DatabaseSchema, String> {
    let (yaml_str, _body) = split_frontmatter(content)?;
    let schema: DatabaseSchema = serde_yaml::from_str(&yaml_str)
        .map_err(|e| format!("Failed to parse schema YAML: {}", e))?;

    // Validate columns have unique IDs
    let mut seen_ids = std::collections::HashSet::new();
    for col in &schema.columns {
        if !seen_ids.insert(&col.id) {
            return Err(format!("Duplicate column ID: '{}'", col.id));
        }
        // Validate select/multi-select have options
        if (col.col_type == ColumnType::Select || col.col_type == ColumnType::MultiSelect)
            && col.options.as_ref().map_or(true, |o| o.is_empty())
        {
            return Err(format!(
                "Column '{}' of type {} must have options defined",
                col.id,
                col.col_type.as_str()
            ));
        }
        // Validate relation has target
        if col.col_type == ColumnType::Relation && col.target.is_none() {
            return Err(format!(
                "Column '{}' of type relation must have a target defined",
                col.id
            ));
        }
    }

    Ok(schema)
}

/// Serialize a DatabaseSchema back to _schema.md content.
pub fn serialize_schema(schema: &DatabaseSchema) -> Result<String, String> {
    let yaml = serde_yaml::to_string(schema)
        .map_err(|e| format!("Failed to serialize schema: {}", e))?;
    Ok(format!("---\n{}---\n", yaml))
}

/// Parse a row .md file given the schema for validation context.
pub fn parse_row(content: &str, path: &str, schema: &DatabaseSchema) -> Result<DatabaseRow, String> {
    let (yaml_str, body) = split_frontmatter(content)
        .map_err(|e| format!("Row file '{}': {}", path, e))?;

    let raw: HashMap<String, JsonValue> = serde_yaml::from_str(&yaml_str)
        .map_err(|e| format!("Row file '{}': failed to parse YAML: {}", path, e))?;

    // Build fields map, only keeping fields that match schema columns
    let mut fields = HashMap::new();
    for col in &schema.columns {
        if let Some(val) = raw.get(&col.id) {
            fields.insert(col.id.clone(), val.clone());
        }
    }

    // Extract row ID from filename
    let id = Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let modified = std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    Ok(DatabaseRow {
        id,
        fields,
        body,
        path: path.to_string(),
        modified,
    })
}

/// Serialize a DatabaseRow back to markdown file content.
pub fn serialize_row(row: &DatabaseRow, schema: &DatabaseSchema) -> Result<String, String> {
    // Build ordered YAML map matching schema column order
    let mut yaml_lines = Vec::new();
    for col in &schema.columns {
        if let Some(val) = row.fields.get(&col.id) {
            let yaml_val = json_value_to_yaml_line(&col.id, val, &col.col_type);
            yaml_lines.push(yaml_val);
        } else {
            // Use default value for missing fields
            yaml_lines.push(format!("{}: {}", col.id, col.col_type.default_value()));
        }
    }

    let yaml_block = yaml_lines.join("\n");
    if row.body.is_empty() {
        Ok(format!("---\n{}\n---\n", yaml_block))
    } else {
        Ok(format!("---\n{}\n---\n\n{}", yaml_block, row.body))
    }
}

/// Convert a JSON value to a YAML frontmatter line for the given column.
fn json_value_to_yaml_line(key: &str, value: &JsonValue, col_type: &ColumnType) -> String {
    match col_type {
        ColumnType::MultiSelect => {
            if let Some(arr) = value.as_array() {
                let items: Vec<String> = arr
                    .iter()
                    .map(|v| v.as_str().unwrap_or("").to_string())
                    .collect();
                format!("{}: [{}]", key, items.join(", "))
            } else {
                format!("{}: []", key)
            }
        }
        ColumnType::Checkbox => {
            let b = value.as_bool().unwrap_or(false);
            format!("{}: {}", key, b)
        }
        ColumnType::Number => {
            if let Some(n) = value.as_f64() {
                // Render as integer if no fractional part
                if n.fract() == 0.0 {
                    format!("{}: {}", key, n as i64)
                } else {
                    format!("{}: {}", key, n)
                }
            } else {
                format!("{}: 0", key)
            }
        }
        _ => {
            // text, date, select, relation, url: render as quoted string
            let s = value.as_str().unwrap_or("");
            format!("{}: \"{}\"", key, s)
        }
    }
}

// ---- Filesystem Operations ----

/// Check if a folder contains a `_schema.md` file (i.e., is a database).
pub fn is_database_folder(folder: &Path) -> bool {
    folder.join("_schema.md").is_file()
}

/// Load the schema from a database folder.
pub fn load_schema(db_folder: &Path) -> Result<DatabaseSchema, String> {
    let schema_path = db_folder.join("_schema.md");
    let content = std::fs::read_to_string(&schema_path)
        .map_err(|e| format!("Failed to read schema at '{}': {}", schema_path.display(), e))?;
    parse_schema(&content)
}

/// Save a schema to a database folder.
pub fn save_schema(db_folder: &Path, schema: &DatabaseSchema) -> Result<(), String> {
    let content = serialize_schema(schema)?;
    let schema_path = db_folder.join("_schema.md");
    std::fs::write(&schema_path, &content)
        .map_err(|e| format!("Failed to write schema: {}", e))?;
    Ok(())
}

/// List all row files in a database folder (excludes _schema.md).
fn list_row_files(db_folder: &Path) -> Result<Vec<PathBuf>, String> {
    let mut rows = Vec::new();
    let entries = std::fs::read_dir(db_folder)
        .map_err(|e| format!("Failed to read database folder: {}", e))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e == "md").unwrap_or(false) {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name != "_schema.md" {
                rows.push(path);
            }
        }
    }

    rows.sort();
    Ok(rows)
}

/// Load all rows from a database folder.
pub fn load_rows(db_folder: &Path, schema: &DatabaseSchema) -> Result<Vec<DatabaseRow>, String> {
    let row_files = list_row_files(db_folder)?;
    let mut rows = Vec::new();

    for path in row_files {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read row file '{}': {}", path.display(), e))?;
        let path_str = path.to_string_lossy().to_string();
        match parse_row(&content, &path_str, schema) {
            Ok(row) => rows.push(row),
            Err(e) => eprintln!("Warning: skipping invalid row file {}: {}", path.display(), e),
        }
    }

    Ok(rows)
}

/// Generate the next row filename using the auto-increment counter.
fn next_row_filename(schema: &mut DatabaseSchema) -> String {
    let id = schema.next_row_id;
    schema.next_row_id = id + 1;
    format!("row-{:03}", id)
}

// ---- CRUD Operations ----

/// Scan notes folder for all database folders.
pub fn scan_databases(notes_folder: &Path) -> Result<Vec<DatabaseInfo>, String> {
    let mut databases = Vec::new();

    if !notes_folder.exists() {
        return Ok(databases);
    }

    fn scan_dir(dir: &Path, base: &Path, results: &mut Vec<DatabaseInfo>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();

                if path.is_dir() && !name.starts_with('.') && name != "node_modules" && name != "assets" {
                    if is_database_folder(&path) {
                        if let Ok(schema) = load_schema(&path) {
                            let row_count = list_row_files(&path).map(|r| r.len()).unwrap_or(0);
                            let rel_path = path.strip_prefix(base)
                                .map(|p| p.to_string_lossy().to_string())
                                .unwrap_or_else(|_| name.clone());
                            results.push(DatabaseInfo {
                                id: rel_path.clone(),
                                name: schema.name.clone(),
                                row_count,
                                column_count: schema.columns.len(),
                                path: path.to_string_lossy().to_string(),
                            });
                        }
                    }
                    // Recurse (but not too deep)
                    scan_dir(&path, base, results);
                }
            }
        }
    }

    scan_dir(notes_folder, notes_folder, &mut databases);
    databases.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(databases)
}

/// Create a new database folder with _schema.md.
pub fn create_database(
    notes_folder: &Path,
    name: &str,
    columns: Vec<ColumnDef>,
    views: Option<Vec<ViewDef>>,
) -> Result<DatabaseInfo, String> {
    let slug = slugify(name);
    let db_folder = notes_folder.join(&slug);

    if db_folder.exists() {
        return Err(format!("Database folder '{}' already exists", slug));
    }

    std::fs::create_dir_all(&db_folder)
        .map_err(|e| format!("Failed to create database folder: {}", e))?;

    let default_views = views.unwrap_or_else(|| {
        vec![ViewDef {
            id: "default-table".to_string(),
            name: "Table".to_string(),
            view_type: ViewType::Table,
            group_by: None,
            columns: None,
            sort_by: None,
            sort_desc: None,
        }]
    });

    let schema = DatabaseSchema {
        name: name.to_string(),
        columns,
        views: default_views,
        templates: HashMap::new(),
        next_row_id: 1,
    };

    save_schema(&db_folder, &schema)?;

    Ok(DatabaseInfo {
        id: slug,
        name: name.to_string(),
        row_count: 0,
        column_count: schema.columns.len(),
        path: db_folder.to_string_lossy().to_string(),
    })
}

/// Get full database: schema + all rows.
pub fn get_database(notes_folder: &Path, db_id: &str) -> Result<(DatabaseSchema, Vec<DatabaseRow>), String> {
    let db_folder = notes_folder.join(db_id);
    if !is_database_folder(&db_folder) {
        return Err(format!("'{}' is not a database folder", db_id));
    }

    let schema = load_schema(&db_folder)?;
    let rows = load_rows(&db_folder, &schema)?;
    Ok((schema, rows))
}

/// Create a new row in a database.
pub fn create_row(
    notes_folder: &Path,
    db_id: &str,
    fields: HashMap<String, JsonValue>,
    body: Option<String>,
) -> Result<DatabaseRow, String> {
    let db_folder = notes_folder.join(db_id);
    if !is_database_folder(&db_folder) {
        return Err(format!("'{}' is not a database folder", db_id));
    }

    let mut schema = load_schema(&db_folder)?;
    let row_filename = next_row_filename(&mut schema);

    // Save updated schema (incremented next_row_id)
    save_schema(&db_folder, &schema)?;

    let row = DatabaseRow {
        id: row_filename.clone(),
        fields,
        body: body.unwrap_or_default(),
        path: db_folder.join(format!("{}.md", row_filename)).to_string_lossy().to_string(),
        modified: now_unix_secs(),
    };

    let content = serialize_row(&row, &schema)?;
    std::fs::write(&row.path, &content)
        .map_err(|e| format!("Failed to write row file: {}", e))?;

    Ok(row)
}

/// Update an existing row in a database.
pub fn update_row(
    notes_folder: &Path,
    db_id: &str,
    row_id: &str,
    fields: HashMap<String, JsonValue>,
    body: Option<String>,
) -> Result<DatabaseRow, String> {
    let db_folder = notes_folder.join(db_id);
    if !is_database_folder(&db_folder) {
        return Err(format!("'{}' is not a database folder", db_id));
    }

    let schema = load_schema(&db_folder)?;
    let row_path = db_folder.join(format!("{}.md", row_id));

    if !row_path.exists() {
        return Err(format!("Row '{}' not found in database '{}'", row_id, db_id));
    }

    // Read existing row to preserve body if not provided
    let existing_content = std::fs::read_to_string(&row_path)
        .map_err(|e| format!("Failed to read row: {}", e))?;
    let existing = parse_row(&existing_content, &row_path.to_string_lossy(), &schema)?;

    let updated_body = body.unwrap_or(existing.body);

    // Merge fields: start with existing, overlay with new
    let mut merged_fields = existing.fields;
    for (k, v) in fields {
        merged_fields.insert(k, v);
    }

    let row = DatabaseRow {
        id: row_id.to_string(),
        fields: merged_fields,
        body: updated_body,
        path: row_path.to_string_lossy().to_string(),
        modified: now_unix_secs(),
    };

    let content = serialize_row(&row, &schema)?;
    std::fs::write(&row_path, &content)
        .map_err(|e| format!("Failed to write row file: {}", e))?;

    Ok(row)
}

/// Delete a row from a database.
pub fn delete_row(notes_folder: &Path, db_id: &str, row_id: &str) -> Result<(), String> {
    let db_folder = notes_folder.join(db_id);
    let row_path = db_folder.join(format!("{}.md", row_id));

    if !row_path.exists() {
        return Err(format!("Row '{}' not found in database '{}'", row_id, db_id));
    }

    std::fs::remove_file(&row_path)
        .map_err(|e| format!("Failed to delete row: {}", e))?;

    Ok(())
}

/// Apply variable substitution to a string.
/// Replaces `{{title}}`, `{{date}}`, and any custom variables.
fn substitute_variables(template_str: &str, variables: &HashMap<String, String>) -> String {
    let mut result = template_str.to_string();

    // Built-in variables
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    result = result.replace("{{date}}", &today);

    // User-provided variables (including {{title}})
    for (key, value) in variables {
        result = result.replace(&format!("{{{{{}}}}}", key), value);
    }

    // Remove any remaining unresolved variables
    let re = regex::Regex::new(r"\{\{[^}]+\}\}").unwrap();
    result = re.replace_all(&result, "").to_string();

    result
}

/// Create a new row from a named template in the database schema.
/// Variables are substituted into the template's title and body.
pub fn create_row_from_template(
    notes_folder: &Path,
    db_id: &str,
    template_name: &str,
    variables: HashMap<String, String>,
) -> Result<DatabaseRow, String> {
    let db_folder = notes_folder.join(db_id);
    if !is_database_folder(&db_folder) {
        return Err(format!("'{}' is not a database folder", db_id));
    }

    let mut schema = load_schema(&db_folder)?;

    // Look up the template
    let template = schema.templates.get(template_name)
        .ok_or_else(|| format!("Template '{}' not found in database '{}'", template_name, db_id))?
        .clone();

    // Build fields from template, applying variable substitution to string values
    let mut fields = HashMap::new();

    // Start with template fields
    for (key, value) in &template.fields {
        let substituted = match value {
            JsonValue::String(s) => JsonValue::String(substitute_variables(s, &variables)),
            other => other.clone(),
        };
        fields.insert(key.clone(), substituted);
    }

    // Apply title template if present — find the first text column to use as title field
    if let Some(ref title_pattern) = template.title {
        let title_value = substitute_variables(title_pattern, &variables);
        // Find the title column (first text column, or column named "title")
        let title_col_id = schema.columns.iter()
            .find(|c| c.id == "title")
            .or_else(|| schema.columns.iter().find(|c| c.col_type == ColumnType::Text))
            .map(|c| c.id.clone());

        if let Some(col_id) = title_col_id {
            fields.insert(col_id, JsonValue::String(title_value));
        }
    }

    // Fill in default values for any columns not set by the template
    for col in &schema.columns {
        if !fields.contains_key(&col.id) {
            fields.insert(col.id.clone(), default_json_value(&col.col_type));
        }
    }

    // Build body with variable substitution
    let body = template.body
        .as_ref()
        .map(|b| substitute_variables(b, &variables))
        .unwrap_or_default();

    // Create the row file
    let row_filename = next_row_filename(&mut schema);
    save_schema(&db_folder, &schema)?;

    let row = DatabaseRow {
        id: row_filename.clone(),
        fields,
        body,
        path: db_folder.join(format!("{}.md", row_filename)).to_string_lossy().to_string(),
        modified: now_unix_secs(),
    };

    let content = serialize_row(&row, &schema)?;
    std::fs::write(&row.path, &content)
        .map_err(|e| format!("Failed to write row file: {}", e))?;

    Ok(row)
}

/// List available template names for a database.
pub fn list_row_templates(
    notes_folder: &Path,
    db_id: &str,
) -> Result<Vec<(String, RowTemplate)>, String> {
    let db_folder = notes_folder.join(db_id);
    if !is_database_folder(&db_folder) {
        return Err(format!("'{}' is not a database folder", db_id));
    }

    let schema = load_schema(&db_folder)?;
    let mut templates: Vec<(String, RowTemplate)> = schema.templates
        .into_iter()
        .collect();
    templates.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(templates)
}

/// Delete an entire database folder.
pub fn delete_database(notes_folder: &Path, db_id: &str) -> Result<(), String> {
    let db_folder = notes_folder.join(db_id);
    if !is_database_folder(&db_folder) {
        return Err(format!("'{}' is not a database folder", db_id));
    }

    std::fs::remove_dir_all(&db_folder)
        .map_err(|e| format!("Failed to delete database: {}", e))?;

    Ok(())
}

// ---- Schema Migration ----

/// Add a new column to a database schema and update all existing rows.
pub fn add_column(
    notes_folder: &Path,
    db_id: &str,
    column: ColumnDef,
) -> Result<DatabaseSchema, String> {
    let db_folder = notes_folder.join(db_id);
    let mut schema = load_schema(&db_folder)?;

    // Check for duplicate column ID
    if schema.columns.iter().any(|c| c.id == column.id) {
        return Err(format!("Column '{}' already exists", column.id));
    }

    let default_val = default_json_value(&column.col_type);
    schema.columns.push(column);
    save_schema(&db_folder, &schema)?;

    // Update all existing rows to include the new column with default value
    let row_files = list_row_files(&db_folder)?;
    for path in row_files {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read row: {}", e))?;
        let path_str = path.to_string_lossy().to_string();
        if let Ok(mut row) = parse_row(&content, &path_str, &schema) {
            let new_col_id = &schema.columns.last().unwrap().id;
            if !row.fields.contains_key(new_col_id) {
                row.fields.insert(new_col_id.clone(), default_val.clone());
            }
            let updated_content = serialize_row(&row, &schema)?;
            std::fs::write(&path, &updated_content)
                .map_err(|e| format!("Failed to update row: {}", e))?;
        }
    }

    Ok(schema)
}

/// Remove a column from a database schema and update all existing rows.
pub fn remove_column(
    notes_folder: &Path,
    db_id: &str,
    column_id: &str,
) -> Result<DatabaseSchema, String> {
    let db_folder = notes_folder.join(db_id);
    let mut schema = load_schema(&db_folder)?;

    let col_idx = schema.columns.iter().position(|c| c.id == column_id)
        .ok_or_else(|| format!("Column '{}' not found", column_id))?;

    schema.columns.remove(col_idx);

    // Also remove from views
    for view in &mut schema.views {
        if let Some(ref mut cols) = view.columns {
            cols.retain(|c| c != column_id);
        }
        if view.group_by.as_deref() == Some(column_id) {
            view.group_by = None;
        }
        if view.sort_by.as_deref() == Some(column_id) {
            view.sort_by = None;
            view.sort_desc = None;
        }
    }

    save_schema(&db_folder, &schema)?;

    // Update all existing rows to remove the column
    let row_files = list_row_files(&db_folder)?;
    for path in row_files {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read row: {}", e))?;
        let path_str = path.to_string_lossy().to_string();
        if let Ok(mut row) = parse_row(&content, &path_str, &schema) {
            row.fields.remove(column_id);
            let updated_content = serialize_row(&row, &schema)?;
            std::fs::write(&path, &updated_content)
                .map_err(|e| format!("Failed to update row: {}", e))?;
        }
    }

    Ok(schema)
}

/// Rename a column in a database schema and update all existing rows.
pub fn rename_column(
    notes_folder: &Path,
    db_id: &str,
    old_column_id: &str,
    new_column_id: &str,
    new_name: Option<&str>,
) -> Result<DatabaseSchema, String> {
    let db_folder = notes_folder.join(db_id);
    let mut schema = load_schema(&db_folder)?;

    // Check column exists
    if !schema.columns.iter().any(|c| c.id == old_column_id) {
        return Err(format!("Column '{}' not found", old_column_id));
    }

    // Check new ID doesn't conflict
    if old_column_id != new_column_id && schema.columns.iter().any(|c| c.id == new_column_id) {
        return Err(format!("Column '{}' already exists", new_column_id));
    }

    // Apply rename
    for col in &mut schema.columns {
        if col.id == old_column_id {
            col.id = new_column_id.to_string();
            if let Some(name) = new_name {
                col.name = name.to_string();
            }
            break;
        }
    }

    // Update views
    for view in &mut schema.views {
        if let Some(ref mut cols) = view.columns {
            for c in cols.iter_mut() {
                if c == old_column_id {
                    *c = new_column_id.to_string();
                }
            }
        }
        if view.group_by.as_deref() == Some(old_column_id) {
            view.group_by = Some(new_column_id.to_string());
        }
        if view.sort_by.as_deref() == Some(old_column_id) {
            view.sort_by = Some(new_column_id.to_string());
        }
    }

    save_schema(&db_folder, &schema)?;

    // Update all existing rows: rename the field key
    let row_files = list_row_files(&db_folder)?;
    for path in row_files {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read row: {}", e))?;
        let path_str = path.to_string_lossy().to_string();

        // Parse with updated schema, but the YAML still has old key.
        // We need to parse raw and rename.
        let (yaml_str, body) = split_frontmatter(&content)
            .map_err(|e| format!("Row file '{}': {}", path_str, e))?;
        let mut raw: HashMap<String, JsonValue> = serde_yaml::from_str(&yaml_str)
            .map_err(|e| format!("Row file '{}': {}", path_str, e))?;

        if let Some(val) = raw.remove(old_column_id) {
            raw.insert(new_column_id.to_string(), val);
        }

        // Build row and serialize
        let mut fields = HashMap::new();
        for col in &schema.columns {
            if let Some(val) = raw.get(&col.id) {
                fields.insert(col.id.clone(), val.clone());
            }
        }

        let row = DatabaseRow {
            id: Path::new(&path_str).file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default(),
            fields,
            body,
            path: path_str,
            modified: now_unix_secs(),
        };

        let updated_content = serialize_row(&row, &schema)?;
        std::fs::write(&path, &updated_content)
            .map_err(|e| format!("Failed to update row: {}", e))?;
    }

    Ok(schema)
}

/// Update schema (columns, views, name) — full replacement.
/// Handles migrations: adds default values for new columns, removes old columns from rows.
pub fn update_schema(
    notes_folder: &Path,
    db_id: &str,
    new_schema: DatabaseSchema,
) -> Result<DatabaseSchema, String> {
    let db_folder = notes_folder.join(db_id);
    let old_schema = load_schema(&db_folder)?;

    // Preserve next_row_id from old schema if not explicitly set
    let mut schema = new_schema;
    if schema.next_row_id == 1 && old_schema.next_row_id > 1 {
        schema.next_row_id = old_schema.next_row_id;
    }

    // Determine added and removed columns
    let old_col_ids: std::collections::HashSet<&str> = old_schema.columns.iter().map(|c| c.id.as_str()).collect();
    let new_col_ids: std::collections::HashSet<&str> = schema.columns.iter().map(|c| c.id.as_str()).collect();

    let added: Vec<&ColumnDef> = schema.columns.iter().filter(|c| !old_col_ids.contains(c.id.as_str())).collect();
    let removed: Vec<&str> = old_col_ids.iter().filter(|id| !new_col_ids.contains(*id)).copied().collect();

    save_schema(&db_folder, &schema)?;

    // Update rows if columns changed
    if !added.is_empty() || !removed.is_empty() {
        let row_files = list_row_files(&db_folder)?;
        for path in row_files {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read row: {}", e))?;
            let path_str = path.to_string_lossy().to_string();

            // Parse with old schema to get existing fields
            let (yaml_str, body) = split_frontmatter(&content)
                .map_err(|e| format!("Row '{}': {}", path_str, e))?;
            let mut raw: HashMap<String, JsonValue> = serde_yaml::from_str(&yaml_str)
                .unwrap_or_default();

            // Add defaults for new columns
            for col in &added {
                if !raw.contains_key(&col.id) {
                    raw.insert(col.id.clone(), default_json_value(&col.col_type));
                }
            }

            // Remove old columns
            for col_id in &removed {
                raw.remove(*col_id);
            }

            // Build row with new schema
            let mut fields = HashMap::new();
            for col in &schema.columns {
                if let Some(val) = raw.get(&col.id) {
                    fields.insert(col.id.clone(), val.clone());
                }
            }

            let row = DatabaseRow {
                id: Path::new(&path_str).file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default(),
                fields,
                body,
                path: path_str,
                modified: now_unix_secs(),
            };

            let updated_content = serialize_row(&row, &schema)?;
            std::fs::write(&path, &updated_content)
                .map_err(|e| format!("Failed to update row: {}", e))?;
        }
    }

    Ok(schema)
}

// ---- Helpers ----

fn slugify(name: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;

    for c in name.chars() {
        if c.is_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash && !slug.is_empty() {
            slug.push('-');
            last_was_dash = true;
        }
    }

    slug.trim_end_matches('-').to_string()
}

fn now_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn default_json_value(col_type: &ColumnType) -> JsonValue {
    match col_type {
        ColumnType::Text => JsonValue::String(String::new()),
        ColumnType::Number => JsonValue::Number(serde_json::Number::from(0)),
        ColumnType::Date => JsonValue::String(String::new()),
        ColumnType::Select => JsonValue::String(String::new()),
        ColumnType::MultiSelect => JsonValue::Array(Vec::new()),
        ColumnType::Checkbox => JsonValue::Bool(false),
        ColumnType::Relation => JsonValue::String(String::new()),
        ColumnType::Url => JsonValue::String(String::new()),
    }
}

// ---- Tests ----

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_schema() {
        let content = r#"---
name: My Tasks
columns:
  - id: title
    name: Title
    type: text
  - id: status
    name: Status
    type: select
    options:
      - Todo
      - In Progress
      - Done
  - id: priority
    name: Priority
    type: number
  - id: due_date
    name: Due Date
    type: date
  - id: done
    name: Done
    type: checkbox
  - id: tags
    name: Tags
    type: multi-select
    options:
      - bug
      - feature
      - docs
  - id: website
    name: Website
    type: url
views:
  - id: default-table
    name: All Tasks
    type: table
  - id: status-board
    name: Status Board
    type: board
    group_by: status
next_row_id: 1
---
"#;
        let schema = parse_schema(content).unwrap();
        assert_eq!(schema.name, "My Tasks");
        assert_eq!(schema.columns.len(), 7);
        assert_eq!(schema.columns[0].col_type, ColumnType::Text);
        assert_eq!(schema.columns[1].col_type, ColumnType::Select);
        assert_eq!(schema.columns[1].options.as_ref().unwrap().len(), 3);
        assert_eq!(schema.columns[4].col_type, ColumnType::Checkbox);
        assert_eq!(schema.columns[5].col_type, ColumnType::MultiSelect);
        assert_eq!(schema.columns[6].col_type, ColumnType::Url);
        assert_eq!(schema.views.len(), 2);
        assert_eq!(schema.next_row_id, 1);
    }

    #[test]
    fn test_parse_row() {
        let schema = DatabaseSchema {
            name: "Test".to_string(),
            columns: vec![
                ColumnDef {
                    id: "title".to_string(),
                    name: "Title".to_string(),
                    col_type: ColumnType::Text,
                    options: None,
                    target: None,
                },
                ColumnDef {
                    id: "done".to_string(),
                    name: "Done".to_string(),
                    col_type: ColumnType::Checkbox,
                    options: None,
                    target: None,
                },
            ],
            views: vec![],
            templates: HashMap::new(),
            next_row_id: 2,
        };

        let content = "---\ntitle: \"Buy groceries\"\ndone: false\n---\n\nRemember to get milk.\n";
        let row = parse_row(content, "/tmp/test/row-001.md", &schema).unwrap();
        assert_eq!(row.id, "row-001");
        assert_eq!(row.fields.get("title").unwrap(), &json!("Buy groceries"));
        assert_eq!(row.fields.get("done").unwrap(), &json!(false));
        assert_eq!(row.body, "Remember to get milk.\n");
    }

    #[test]
    fn test_serialize_row() {
        let schema = DatabaseSchema {
            name: "Test".to_string(),
            columns: vec![
                ColumnDef {
                    id: "title".to_string(),
                    name: "Title".to_string(),
                    col_type: ColumnType::Text,
                    options: None,
                    target: None,
                },
                ColumnDef {
                    id: "count".to_string(),
                    name: "Count".to_string(),
                    col_type: ColumnType::Number,
                    options: None,
                    target: None,
                },
                ColumnDef {
                    id: "tags".to_string(),
                    name: "Tags".to_string(),
                    col_type: ColumnType::MultiSelect,
                    options: Some(vec!["a".into(), "b".into()]),
                    target: None,
                },
            ],
            views: vec![],
            templates: HashMap::new(),
            next_row_id: 1,
        };

        let mut fields = HashMap::new();
        fields.insert("title".to_string(), json!("Test item"));
        fields.insert("count".to_string(), json!(42));
        fields.insert("tags".to_string(), json!(["a", "b"]));

        let row = DatabaseRow {
            id: "row-001".to_string(),
            fields,
            body: "Some notes here.\n".to_string(),
            path: "/tmp/test/row-001.md".to_string(),
            modified: 0,
        };

        let output = serialize_row(&row, &schema).unwrap();
        assert!(output.starts_with("---\n"));
        assert!(output.contains("title: \"Test item\""));
        assert!(output.contains("count: 42"));
        assert!(output.contains("tags: [a, b]"));
        assert!(output.contains("Some notes here."));
    }

    #[test]
    fn test_column_type_roundtrip() {
        let types = vec![
            ColumnType::Text,
            ColumnType::Number,
            ColumnType::Date,
            ColumnType::Select,
            ColumnType::MultiSelect,
            ColumnType::Checkbox,
            ColumnType::Relation,
            ColumnType::Url,
        ];
        for ct in types {
            let s = ct.as_str();
            let parsed = ColumnType::from_str(s).unwrap();
            assert_eq!(ct, parsed);
        }
    }

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("My Tasks"), "my-tasks");
        assert_eq!(slugify("  Hello World  "), "hello-world");
        assert_eq!(slugify("Bug/Feature Tracker"), "bug-feature-tracker");
    }

    #[test]
    fn test_schema_validation_duplicate_column() {
        let content = "---\nname: Test\ncolumns:\n  - id: x\n    name: X\n    type: text\n  - id: x\n    name: X2\n    type: number\nviews: []\n---\n";
        let result = parse_schema(content);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Duplicate column ID"));
    }

    #[test]
    fn test_schema_validation_select_without_options() {
        let content = "---\nname: Test\ncolumns:\n  - id: status\n    name: Status\n    type: select\nviews: []\n---\n";
        let result = parse_schema(content);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must have options"));
    }

    #[test]
    fn test_schema_validation_relation_without_target() {
        let content = "---\nname: Test\ncolumns:\n  - id: project\n    name: Project\n    type: relation\nviews: []\n---\n";
        let result = parse_schema(content);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must have a target"));
    }

    #[test]
    fn test_parse_schema_with_templates() {
        let content = r#"---
name: Bug Tracker
columns:
  - id: title
    name: Title
    type: text
  - id: status
    name: Status
    type: select
    options:
      - Backlog
      - In Progress
      - Done
  - id: tags
    name: Tags
    type: multi-select
    options:
      - bug
      - feature
views: []
templates:
  bug-report:
    name: Bug Report
    title: "Bug: {{title}}"
    fields:
      status: Backlog
      tags:
        - bug
    body: |
      ## Steps to Reproduce
      1.
      ## Expected
      ## Actual
next_row_id: 1
---
"#;
        let schema = parse_schema(content).unwrap();
        assert_eq!(schema.templates.len(), 1);
        let tmpl = schema.templates.get("bug-report").unwrap();
        assert_eq!(tmpl.name, "Bug Report");
        assert_eq!(tmpl.title.as_deref(), Some("Bug: {{title}}"));
        assert_eq!(tmpl.fields.get("status").unwrap(), &json!("Backlog"));
        assert!(tmpl.body.is_some());
        assert!(tmpl.body.as_ref().unwrap().contains("Steps to Reproduce"));
    }

    #[test]
    fn test_substitute_variables() {
        let mut vars = HashMap::new();
        vars.insert("title".to_string(), "Login crash".to_string());

        let result = substitute_variables("Bug: {{title}}", &vars);
        assert_eq!(result, "Bug: Login crash");

        // Test date substitution (should produce a YYYY-MM-DD format)
        let result = substitute_variables("Created on {{date}}", &vars);
        assert!(result.starts_with("Created on 20"));
        assert!(!result.contains("{{date}}"));

        // Test unresolved variables are removed
        let result = substitute_variables("{{unknown}} text", &vars);
        assert_eq!(result, " text");
    }

    #[test]
    fn test_create_row_from_template() {
        let dir = std::env::temp_dir().join(format!("scratch-test-tmpl-{}", std::process::id()));
        let db_dir = dir.join("test-db");
        std::fs::create_dir_all(&db_dir).unwrap();

        let mut templates = HashMap::new();
        templates.insert("bug-report".to_string(), RowTemplate {
            name: "Bug Report".to_string(),
            title: Some("Bug: {{title}}".to_string()),
            fields: {
                let mut f = HashMap::new();
                f.insert("status".to_string(), json!("Backlog"));
                f.insert("tags".to_string(), json!(["bug"]));
                f
            },
            body: Some("## Steps to Reproduce\n1.\n## Expected\n## Actual\n".to_string()),
        });

        let schema = DatabaseSchema {
            name: "Test DB".to_string(),
            columns: vec![
                ColumnDef { id: "title".to_string(), name: "Title".to_string(), col_type: ColumnType::Text, options: None, target: None },
                ColumnDef { id: "status".to_string(), name: "Status".to_string(), col_type: ColumnType::Select, options: Some(vec!["Backlog".into(), "Done".into()]), target: None },
                ColumnDef { id: "tags".to_string(), name: "Tags".to_string(), col_type: ColumnType::MultiSelect, options: Some(vec!["bug".into(), "feature".into()]), target: None },
            ],
            views: vec![],
            templates,
            next_row_id: 1,
        };

        save_schema(&db_dir, &schema).unwrap();

        let mut vars = HashMap::new();
        vars.insert("title".to_string(), "Login crash".to_string());

        let row = create_row_from_template(&dir, "test-db", "bug-report", vars).unwrap();

        assert_eq!(row.fields.get("title").unwrap(), &json!("Bug: Login crash"));
        assert_eq!(row.fields.get("status").unwrap(), &json!("Backlog"));
        assert_eq!(row.fields.get("tags").unwrap(), &json!(["bug"]));
        assert!(row.body.contains("Steps to Reproduce"));

        // Verify file was created
        assert!(std::path::Path::new(&row.path).exists());

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_schema_roundtrip_with_templates() {
        let mut templates = HashMap::new();
        templates.insert("feature".to_string(), RowTemplate {
            name: "Feature Request".to_string(),
            title: Some("Feature: {{title}}".to_string()),
            fields: {
                let mut f = HashMap::new();
                f.insert("status".to_string(), json!("Backlog"));
                f
            },
            body: Some("## Description\n".to_string()),
        });

        let schema = DatabaseSchema {
            name: "Test".to_string(),
            columns: vec![
                ColumnDef { id: "title".to_string(), name: "Title".to_string(), col_type: ColumnType::Text, options: None, target: None },
                ColumnDef { id: "status".to_string(), name: "Status".to_string(), col_type: ColumnType::Select, options: Some(vec!["Backlog".into()]), target: None },
            ],
            views: vec![],
            templates,
            next_row_id: 1,
        };

        let serialized = serialize_schema(&schema).unwrap();
        let parsed = parse_schema(&serialized).unwrap();

        assert_eq!(parsed.templates.len(), 1);
        let tmpl = parsed.templates.get("feature").unwrap();
        assert_eq!(tmpl.name, "Feature Request");
        assert_eq!(tmpl.title.as_deref(), Some("Feature: {{title}}"));
    }
}

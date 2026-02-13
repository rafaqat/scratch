use anyhow::Result;
use base64::Engine;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::*;
use tantivy::{doc, Index, IndexReader, IndexWriter, ReloadPolicy};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tokio::fs;

pub mod database;
mod git;
mod mcp;
pub mod stories;
pub mod webhooks;

// Note metadata for list display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteMetadata {
    pub id: String,
    pub title: String,
    pub preview: String,
    pub modified: i64,
}

// Full note content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub content: String,
    pub path: String,
    pub modified: i64,
}

// Theme color customization
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ThemeColors {
    pub bg: Option<String>,
    pub bg_secondary: Option<String>,
    pub bg_muted: Option<String>,
    pub bg_emphasis: Option<String>,
    pub text: Option<String>,
    pub text_muted: Option<String>,
    pub text_inverse: Option<String>,
    pub border: Option<String>,
    pub accent: Option<String>,
}

// Theme settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeSettings {
    pub mode: String, // "light" | "dark" | "system"
    pub custom_light_colors: Option<ThemeColors>,
    pub custom_dark_colors: Option<ThemeColors>,
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            mode: "system".to_string(),
            custom_light_colors: None,
            custom_dark_colors: None,
        }
    }
}

// Editor font settings (simplified)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EditorFontSettings {
    pub base_font_family: Option<String>, // "system-sans" | "serif" | "monospace"
    pub base_font_size: Option<f32>,      // in px, default 16
    pub bold_weight: Option<i32>,         // 600, 700, 800 for headings and bold
    pub line_height: Option<f32>,         // default 1.6
}

// App config (stored in app data directory - just the notes folder path)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub notes_folder: Option<String>,
}

// Per-folder settings (stored in .scratch/settings.json within notes folder)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    pub theme: ThemeSettings,
    #[serde(rename = "editorFont")]
    pub editor_font: Option<EditorFontSettings>,
    #[serde(rename = "gitEnabled")]
    pub git_enabled: Option<bool>,
    #[serde(rename = "pinnedNoteIds")]
    pub pinned_note_ids: Option<Vec<String>>,
    #[serde(rename = "mcpEnabled")]
    pub mcp_enabled: Option<bool>,
    #[serde(rename = "mcpPort")]
    pub mcp_port: Option<u16>,
}

// Search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub preview: String,
    pub modified: i64,
    pub score: f32,
}

// AI execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiExecutionResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
}

// File watcher state
pub struct FileWatcherState {
    #[allow(dead_code)]
    watcher: RecommendedWatcher,
}

// Tantivy search index state
pub struct SearchIndex {
    index: Index,
    reader: IndexReader,
    writer: Mutex<IndexWriter>,
    #[allow(dead_code)]
    schema: Schema,
    id_field: Field,
    title_field: Field,
    content_field: Field,
    modified_field: Field,
}

impl SearchIndex {
    fn new(index_path: &PathBuf) -> Result<Self> {
        // Build schema
        let mut schema_builder = Schema::builder();
        let id_field = schema_builder.add_text_field("id", STRING | STORED);
        let title_field = schema_builder.add_text_field("title", TEXT | STORED);
        let content_field = schema_builder.add_text_field("content", TEXT | STORED);
        let modified_field = schema_builder.add_i64_field("modified", INDEXED | STORED);
        let schema = schema_builder.build();

        // Create or open index
        std::fs::create_dir_all(index_path)?;
        let index = Index::create_in_dir(index_path, schema.clone())
            .or_else(|_| Index::open_in_dir(index_path))?;

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;

        let writer = index.writer(50_000_000)?; // 50MB buffer

        Ok(Self {
            index,
            reader,
            writer: Mutex::new(writer),
            schema,
            id_field,
            title_field,
            content_field,
            modified_field,
        })
    }

    fn index_note(&self, id: &str, title: &str, content: &str, modified: i64) -> Result<()> {
        let mut writer = self.writer.lock().expect("search writer mutex");

        // Delete existing document with this ID
        let id_term = tantivy::Term::from_field_text(self.id_field, id);
        writer.delete_term(id_term);

        // Add new document
        writer.add_document(doc!(
            self.id_field => id,
            self.title_field => title,
            self.content_field => content,
            self.modified_field => modified,
        ))?;

        writer.commit()?;
        Ok(())
    }

    fn delete_note(&self, id: &str) -> Result<()> {
        let mut writer = self.writer.lock().expect("search writer mutex");
        let id_term = tantivy::Term::from_field_text(self.id_field, id);
        writer.delete_term(id_term);
        writer.commit()?;
        Ok(())
    }

    fn search(&self, query_str: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let searcher = self.reader.searcher();
        let query_parser =
            QueryParser::for_index(&self.index, vec![self.title_field, self.content_field]);

        // Parse query, fall back to prefix query if parsing fails
        let query = query_parser
            .parse_query(query_str)
            .or_else(|_| query_parser.parse_query(&format!("{}*", query_str)))?;

        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;

        let mut results = Vec::with_capacity(top_docs.len());
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address)?;

            let id = doc
                .get_first(self.id_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let title = doc
                .get_first(self.title_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let content = doc
                .get_first(self.content_field)
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let modified = doc
                .get_first(self.modified_field)
                .and_then(|v| v.as_i64())
                .unwrap_or(0);

            let preview = generate_preview(content);

            results.push(SearchResult {
                id,
                title,
                preview,
                modified,
                score,
            });
        }

        Ok(results)
    }

    fn rebuild_index(&self, notes_folder: &PathBuf) -> Result<()> {
        let mut writer = self.writer.lock().expect("search writer mutex");
        writer.delete_all_documents()?;

        if notes_folder.exists() {
            let files = walk_md_files_sync(notes_folder, notes_folder)
                .map_err(|e| anyhow::anyhow!(e))?;

            for file_path in files {
                if let Ok(content) = std::fs::read_to_string(&file_path) {
                    let modified = std::fs::metadata(&file_path)
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);

                    let id = path_to_note_id(notes_folder, &file_path)
                        .unwrap_or_else(|| "unknown".to_string());
                    let title = extract_title(&content);

                    writer.add_document(doc!(
                        self.id_field => id.as_str(),
                        self.title_field => title,
                        self.content_field => content.as_str(),
                        self.modified_field => modified,
                    ))?;
                }
            }
        }

        writer.commit()?;
        Ok(())
    }
}

// Backlink entry: a note that links to the current note
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BacklinkEntry {
    pub note_id: String,
    pub note_title: String,
    pub context: String,
}

// Backlinks index: maps lowercase note title -> list of notes that link to it
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BacklinksIndex {
    // Key: lowercase note title, Value: list of backlink entries
    pub links: HashMap<String, Vec<BacklinkEntry>>,
}

/// Regex to match [[Title]] or [[Title|alias]] wikilinks in markdown content.
fn find_wikilinks_in_content(content: &str) -> Vec<(String, String)> {
    let re = regex::Regex::new(r"\[\[([^\]]+)\]\]").unwrap();
    let mut results = Vec::new();

    for cap in re.captures_iter(content) {
        let inner = cap[1].trim().to_string();
        if inner.is_empty() {
            continue;
        }
        // Support alias syntax: [[Title|display text]]
        let title = if let Some(pos) = inner.find('|') {
            inner[..pos].trim().to_string()
        } else {
            inner.clone()
        };
        if !title.is_empty() {
            // Extract context: get the line containing this wikilink
            let match_start = cap.get(0).unwrap().start();
            let context = extract_wikilink_context(content, match_start);
            results.push((title, context));
        }
    }

    results
}

/// Extract a context snippet around a wikilink match position.
fn extract_wikilink_context(content: &str, match_pos: usize) -> String {
    // Find the line containing the match
    let line_start = content[..match_pos].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let line_end = content[match_pos..].find('\n')
        .map(|p| match_pos + p)
        .unwrap_or(content.len());

    let line = &content[line_start..line_end];

    // Strip markdown formatting for cleaner display
    let stripped = strip_markdown(line.trim());

    // Truncate to reasonable length
    if stripped.len() > 120 {
        format!("{}...", &stripped[..120])
    } else {
        stripped
    }
}

/// Get the path for the backlinks index file.
fn get_backlinks_index_path(notes_folder: &str) -> PathBuf {
    let scratch_dir = PathBuf::from(notes_folder).join(".scratch");
    std::fs::create_dir_all(&scratch_dir).ok();
    scratch_dir.join("backlinks.json")
}

/// Load backlinks index from disk.
fn load_backlinks_index(notes_folder: &str) -> BacklinksIndex {
    let path = get_backlinks_index_path(notes_folder);
    if path.exists() {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    } else {
        BacklinksIndex::default()
    }
}

/// Save backlinks index to disk.
fn save_backlinks_index(notes_folder: &str, index: &BacklinksIndex) -> Result<()> {
    let path = get_backlinks_index_path(notes_folder);
    let content = serde_json::to_string_pretty(index)?;
    std::fs::write(path, content)?;
    Ok(())
}

/// Rebuild the entire backlinks index by scanning all notes.
fn rebuild_backlinks_index_from_folder(notes_folder: &str) -> BacklinksIndex {
    let folder_path = PathBuf::from(notes_folder);
    let mut index = BacklinksIndex::default();

    if !folder_path.exists() {
        return index;
    }

    let files = match walk_md_files_sync(&folder_path, &folder_path) {
        Ok(f) => f,
        Err(_) => return index,
    };

    for file_path in &files {
        if let Ok(content) = std::fs::read_to_string(file_path) {
            let note_id = path_to_note_id(&folder_path, file_path)
                .unwrap_or_else(|| "unknown".to_string());
            let note_title = extract_title(&content);

            let wikilinks = find_wikilinks_in_content(&content);
            for (target_title, context) in wikilinks {
                let key = target_title.to_lowercase();
                let entry = BacklinkEntry {
                    note_id: note_id.clone(),
                    note_title: note_title.clone(),
                    context,
                };
                index.links.entry(key).or_default().push(entry);
            }
        }
    }

    // Save to disk
    let _ = save_backlinks_index(notes_folder, &index);

    index
}

/// Update backlinks index incrementally when a note is saved.
/// Removes all old entries from this note, then adds new ones.
fn update_backlinks_for_note(
    index: &mut BacklinksIndex,
    note_id: &str,
    note_title: &str,
    content: &str,
) {
    // Remove all existing entries from this note
    for entries in index.links.values_mut() {
        entries.retain(|e| e.note_id != note_id);
    }
    // Clean up empty keys
    index.links.retain(|_, v| !v.is_empty());

    // Add new entries
    let wikilinks = find_wikilinks_in_content(content);
    for (target_title, context) in wikilinks {
        let key = target_title.to_lowercase();
        let entry = BacklinkEntry {
            note_id: note_id.to_string(),
            note_title: note_title.to_string(),
            context,
        };
        index.links.entry(key).or_default().push(entry);
    }
}

/// Remove all backlink entries from a deleted note.
fn remove_backlinks_for_note(index: &mut BacklinksIndex, note_id: &str) {
    for entries in index.links.values_mut() {
        entries.retain(|e| e.note_id != note_id);
    }
    index.links.retain(|_, v| !v.is_empty());
}

// Inner state shared between Tauri and MCP server via Arc
pub struct AppStateInner {
    pub app_config: RwLock<AppConfig>,  // notes_folder path (stored in app data)
    pub settings: RwLock<Settings>,      // per-folder settings (stored in .scratch/)
    pub notes_cache: RwLock<HashMap<String, NoteMetadata>>,
    pub file_watcher: Mutex<Option<FileWatcherState>>,
    pub search_index: Mutex<Option<SearchIndex>>,
    pub backlinks_index: RwLock<BacklinksIndex>,
    pub debounce_map: Arc<Mutex<HashMap<PathBuf, Instant>>>,
    pub mcp_server_handle: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    pub story_index: Mutex<Option<stories::StoryIndex>>,
}

// App state wrapper that is Clone-able for sharing with axum
#[derive(Clone)]
pub struct AppState(pub Arc<AppStateInner>);

impl std::ops::Deref for AppState {
    type Target = AppStateInner;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self(Arc::new(AppStateInner {
            app_config: RwLock::new(AppConfig::default()),
            settings: RwLock::new(Settings::default()),
            notes_cache: RwLock::new(HashMap::new()),
            file_watcher: Mutex::new(None),
            search_index: Mutex::new(None),
            backlinks_index: RwLock::new(BacklinksIndex::default()),
            debounce_map: Arc::new(Mutex::new(HashMap::new())),
            mcp_server_handle: Mutex::new(None),
            story_index: Mutex::new(None),
        }))
    }
}

// Utility: Sanitize filename from title
fn sanitize_filename(title: &str) -> String {
    let sanitized: String = title
        .chars()
        .filter(|c| *c != '\u{00A0}' && *c != '\u{FEFF}')
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            _ => c,
        })
        .collect();

    let trimmed = sanitized.trim();
    if trimmed.is_empty() || is_effectively_empty(trimmed) {
        "untitled".to_string()
    } else {
        trimmed.to_string()
    }
}

// Utility: Validate a note ID to prevent directory traversal attacks.
fn validate_note_id(id: &str) -> Result<String, String> {
    if id.contains('\0') {
        return Err("Note ID contains invalid characters".to_string());
    }

    let normalized = id.replace('\\', "/");

    if normalized.starts_with('/') {
        return Err("Note ID must be a relative path".to_string());
    }

    for component in normalized.split('/') {
        if component == ".." {
            return Err("Note ID must not contain '..'".to_string());
        }
    }

    let cleaned: Vec<&str> = normalized.split('/').filter(|s| !s.is_empty()).collect();
    if cleaned.is_empty() {
        return Err("Note ID must not be empty".to_string());
    }

    Ok(cleaned.join("/"))
}

// Utility: Resolve a note ID to a full file path within the notes folder.
fn resolve_note_path(folder: &str, id: &str) -> Result<PathBuf, String> {
    let validated_id = validate_note_id(id)?;
    let path = PathBuf::from(folder).join(format!("{}.md", validated_id));

    // Safety: ensure path stays within notes folder
    let folder_path = PathBuf::from(folder);
    let normalized_path = path
        .components()
        .collect::<PathBuf>();
    let normalized_folder = folder_path
        .components()
        .collect::<PathBuf>();

    if !normalized_path.starts_with(&normalized_folder) {
        return Err("Path escapes notes folder".to_string());
    }

    Ok(path)
}

// Directories to skip during recursive traversal
const EXCLUDED_DIRS: &[&str] = &[".scratch", ".git", ".assets", "assets", "node_modules"];

fn should_skip_dir(name: &str) -> bool {
    name.starts_with('.') || EXCLUDED_DIRS.contains(&name)
}

// Async recursive walk collecting all .md file paths
fn walk_md_files<'a>(
    base: &'a PathBuf,
    current: &'a PathBuf,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<PathBuf>, String>> + Send + 'a>>
{
    Box::pin(async move {
        let mut results = Vec::new();
        let mut entries = fs::read_dir(current).await.map_err(|e| e.to_string())?;

        while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
            let path = entry.path();
            let name_str = entry.file_name().to_string_lossy().to_string();

            if path.is_dir() {
                if !should_skip_dir(&name_str) {
                    let mut sub = walk_md_files(base, &path).await?;
                    results.append(&mut sub);
                }
            } else if path.extension().is_some_and(|ext| ext == "md") {
                results.push(path);
            }
        }

        Ok(results)
    })
}

// Sync recursive walk for file watcher and search index
fn walk_md_files_sync(base: &PathBuf, current: &PathBuf) -> Result<Vec<PathBuf>, String> {
    let mut results = Vec::new();

    for entry in std::fs::read_dir(current).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        let name_str = entry.file_name().to_string_lossy().to_string();

        if path.is_dir() {
            if !should_skip_dir(&name_str) {
                let mut sub = walk_md_files_sync(base, &path)?;
                results.append(&mut sub);
            }
        } else if path.extension().is_some_and(|ext| ext == "md") {
            results.push(path);
        }
    }

    Ok(results)
}

// Extract note ID from file path relative to notes folder base.
fn path_to_note_id(base: &PathBuf, file_path: &PathBuf) -> Option<String> {
    let relative = file_path.strip_prefix(base).ok()?;
    let without_ext = relative.with_extension("");
    Some(without_ext.to_string_lossy().replace('\\', "/"))
}

// Utility: Check if a string is effectively empty
fn is_effectively_empty(s: &str) -> bool {
    s.chars()
        .all(|c| c.is_whitespace() || c == '\u{00A0}' || c == '\u{FEFF}')
}

// Utility: Extract title from markdown content
// Handles YAML frontmatter: if a file starts with ---, skip to after the closing ---
// and also check for a title: field inside the frontmatter.
fn extract_title(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut start = 0;

    // Check for YAML frontmatter (starts with ---)
    if !lines.is_empty() && lines[0].trim() == "---" {
        let mut frontmatter_title: Option<String> = None;

        // Find closing --- and extract title: field
        for i in 1..lines.len() {
            let trimmed = lines[i].trim();
            if trimmed == "---" {
                start = i + 1;
                break;
            }
            // Extract title from frontmatter "title: ..." or 'title: "..."'
            if let Some(rest) = trimmed.strip_prefix("title:") {
                let val = rest.trim();
                let val = val.trim_matches('"').trim_matches('\'');
                if !val.is_empty() {
                    frontmatter_title = Some(val.to_string());
                }
            }
        }

        // If we found a title in frontmatter, use it
        if let Some(title) = frontmatter_title {
            return title;
        }
    }

    // Search body for a # heading or first non-empty line
    for line in lines.iter().skip(start) {
        let trimmed = line.trim();
        if let Some(title) = trimmed.strip_prefix("# ") {
            let title = title.trim();
            if !is_effectively_empty(title) {
                return title.to_string();
            }
        }
        if !is_effectively_empty(trimmed) {
            return trimmed.chars().take(50).collect();
        }
    }
    "Untitled".to_string()
}

// Utility: Generate preview from content (strip markdown formatting)
fn generate_preview(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut start = 0;

    // Skip YAML frontmatter if present
    if !lines.is_empty() && lines[0].trim() == "---" {
        for i in 1..lines.len() {
            if lines[i].trim() == "---" {
                start = i + 1;
                break;
            }
        }
    }

    // Skip the title line, find first non-empty content line
    let mut skipped_title = false;
    for line in lines.iter().skip(start) {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            if !skipped_title {
                skipped_title = true;
                continue;
            }
            let stripped = strip_markdown(trimmed);
            if !stripped.is_empty() {
                return stripped.chars().take(100).collect();
            }
        }
    }
    String::new()
}

// Strip common markdown formatting from text
fn strip_markdown(text: &str) -> String {
    let mut result = text.to_string();

    // Remove heading markers (##, ###, etc.)
    let trimmed = result.trim_start();
    if trimmed.starts_with('#') {
        result = trimmed.trim_start_matches('#').trim_start().to_string();
    }

    // Remove strikethrough (~~text~~) - before other markers
    while let Some(start) = result.find("~~") {
        if let Some(end) = result[start + 2..].find("~~") {
            let inner = &result[start + 2..start + 2 + end];
            result = format!("{}{}{}", &result[..start], inner, &result[start + 4 + end..]);
        } else {
            break;
        }
    }

    // Remove bold (**text** or __text__) - before italic
    while let Some(start) = result.find("**") {
        if let Some(end) = result[start + 2..].find("**") {
            let inner = &result[start + 2..start + 2 + end];
            result = format!("{}{}{}", &result[..start], inner, &result[start + 4 + end..]);
        } else {
            break;
        }
    }
    while let Some(start) = result.find("__") {
        if let Some(end) = result[start + 2..].find("__") {
            let inner = &result[start + 2..start + 2 + end];
            result = format!("{}{}{}", &result[..start], inner, &result[start + 4 + end..]);
        } else {
            break;
        }
    }

    // Remove inline code (`code`)
    while let Some(start) = result.find('`') {
        if let Some(end) = result[start + 1..].find('`') {
            let inner = &result[start + 1..start + 1 + end];
            result = format!("{}{}{}", &result[..start], inner, &result[start + 2 + end..]);
        } else {
            break;
        }
    }

    // Remove images ![alt](url) - must come before links
    let img_re = regex::Regex::new(r"!\[([^\]]*)\]\([^)]+\)").unwrap();
    result = img_re.replace_all(&result, "$1").to_string();

    // Remove links [text](url)
    let link_re = regex::Regex::new(r"\[([^\]]+)\]\([^)]+\)").unwrap();
    result = link_re.replace_all(&result, "$1").to_string();

    // Remove italic (*text* or _text_) - simple approach after bold is removed
    // Match *text* where text doesn't contain *
    while let Some(start) = result.find('*') {
        if let Some(end) = result[start + 1..].find('*') {
            if end > 0 {
                let inner = &result[start + 1..start + 1 + end];
                result = format!("{}{}{}", &result[..start], inner, &result[start + 2 + end..]);
            } else {
                break;
            }
        } else {
            break;
        }
    }
    // Match _text_ where text doesn't contain _
    while let Some(start) = result.find('_') {
        if let Some(end) = result[start + 1..].find('_') {
            if end > 0 {
                let inner = &result[start + 1..start + 1 + end];
                result = format!("{}{}{}", &result[..start], inner, &result[start + 2 + end..]);
            } else {
                break;
            }
        } else {
            break;
        }
    }

    // Remove task list markers
    result = result
        .replace("- [ ] ", "")
        .replace("- [x] ", "")
        .replace("- [X] ", "");

    // Remove list markers at start (-, *, +, 1.)
    let list_re = regex::Regex::new(r"^(\s*[-+*]|\s*\d+\.)\s+").unwrap();
    result = list_re.replace(&result, "").to_string();

    result.trim().to_string()
}

// Get app config file path (in app data directory)
fn get_app_config_path(app: &AppHandle) -> Result<PathBuf> {
    let app_data = app.path().app_data_dir()?;
    std::fs::create_dir_all(&app_data)?;
    Ok(app_data.join("config.json"))
}

// Get per-folder settings file path (in .scratch/ within notes folder)
fn get_settings_path(notes_folder: &str) -> PathBuf {
    let scratch_dir = PathBuf::from(notes_folder).join(".scratch");
    std::fs::create_dir_all(&scratch_dir).ok();
    scratch_dir.join("settings.json")
}

// Get search index path
fn get_search_index_path(app: &AppHandle) -> Result<PathBuf> {
    let app_data = app.path().app_data_dir()?;
    std::fs::create_dir_all(&app_data)?;
    Ok(app_data.join("search_index"))
}

// Load app config from disk (notes folder path)
fn load_app_config(app: &AppHandle) -> AppConfig {
    let path = match get_app_config_path(app) {
        Ok(p) => p,
        Err(_) => return AppConfig::default(),
    };

    if path.exists() {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    } else {
        AppConfig::default()
    }
}

// Save app config to disk
fn save_app_config(app: &AppHandle, config: &AppConfig) -> Result<()> {
    let path = get_app_config_path(app)?;
    let content = serde_json::to_string_pretty(config)?;
    std::fs::write(path, content)?;
    Ok(())
}

// Load per-folder settings from disk
fn load_settings(notes_folder: &str) -> Settings {
    let path = get_settings_path(notes_folder);

    if path.exists() {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    } else {
        Settings::default()
    }
}

// Save per-folder settings to disk
fn save_settings(notes_folder: &str, settings: &Settings) -> Result<()> {
    let path = get_settings_path(notes_folder);
    let content = serde_json::to_string_pretty(settings)?;
    std::fs::write(path, content)?;
    Ok(())
}

// Clean up old entries from debounce map (entries older than 5 seconds)
fn cleanup_debounce_map(map: &Mutex<HashMap<PathBuf, Instant>>) {
    let mut map = map.lock().expect("debounce map mutex");
    let now = Instant::now();
    map.retain(|_, last| now.duration_since(*last) < Duration::from_secs(5));
}

// TAURI COMMANDS

#[tauri::command]
fn get_notes_folder(state: State<AppState>) -> Option<String> {
    state
        .app_config
        .read()
        .expect("app_config read lock")
        .notes_folder
        .clone()
}

#[tauri::command]
fn set_notes_folder(app: AppHandle, path: String, state: State<AppState>) -> Result<(), String> {
    let path_buf = PathBuf::from(&path);

    // Verify it's a valid directory
    if !path_buf.exists() {
        std::fs::create_dir_all(&path_buf).map_err(|e| e.to_string())?;
    }

    // Create assets folder
    let assets = path_buf.join("assets");
    std::fs::create_dir_all(&assets).map_err(|e| e.to_string())?;

    // Create .scratch config folder
    let scratch_dir = path_buf.join(".scratch");
    std::fs::create_dir_all(&scratch_dir).map_err(|e| e.to_string())?;

    // Load per-folder settings (starts fresh with defaults if none exist)
    let settings = load_settings(&path);

    // Update app config
    {
        let mut app_config = state.app_config.write().expect("app_config write lock");
        app_config.notes_folder = Some(path.clone());
    }

    // Update settings in memory
    {
        let mut current_settings = state.settings.write().expect("settings write lock");
        *current_settings = settings;
    }

    // Save app config to disk
    {
        let app_config = state.app_config.read().expect("app_config read lock");
        save_app_config(&app, &app_config).map_err(|e| e.to_string())?;
    }

    // Initialize search index
    if let Ok(index_path) = get_search_index_path(&app) {
        if let Ok(search_index) = SearchIndex::new(&index_path) {
            let _ = search_index.rebuild_index(&path_buf);
            let mut index = state.search_index.lock().expect("search index mutex");
            *index = Some(search_index);
        }
    }

    // Rebuild backlinks index
    {
        let new_index = rebuild_backlinks_index_from_folder(&path);
        let mut bl_index = state.backlinks_index.write().expect("backlinks write lock");
        *bl_index = new_index;
    }

    Ok(())
}

pub async fn list_notes_impl(
    state: &AppState,
    folder_filter: Option<&str>,
    recursive: bool,
) -> Result<Vec<NoteMetadata>, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config
            .notes_folder
            .clone()
            .ok_or("Notes folder not set")?
    };

    let base_path = PathBuf::from(&folder);
    if !base_path.exists() {
        return Ok(vec![]);
    }

    // Determine scan directory
    let scan_path = if let Some(sub) = folder_filter {
        let validated = validate_note_id(sub)?;
        let p = base_path.join(&validated);
        if !p.exists() || !p.is_dir() {
            return Err(format!("Folder not found: {}", sub));
        }
        p
    } else {
        base_path.clone()
    };

    let mut notes: Vec<NoteMetadata> = Vec::new();

    if recursive {
        // Recursive walk
        let files = walk_md_files(&base_path, &scan_path).await?;
        for file_path in files {
            if let Ok(content) = fs::read_to_string(&file_path).await {
                let modified = fs::metadata(&file_path)
                    .await
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);

                let id = path_to_note_id(&base_path, &file_path)
                    .unwrap_or_else(|| "unknown".to_string());

                notes.push(NoteMetadata {
                    id,
                    title: extract_title(&content),
                    preview: generate_preview(&content),
                    modified,
                });
            }
        }
    } else {
        // Non-recursive (original behavior)
        let mut entries = fs::read_dir(&scan_path).await.map_err(|e| e.to_string())?;

        while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
            let file_path = entry.path();
            if file_path.extension().is_some_and(|ext| ext == "md") {
                if let Ok(metadata) = entry.metadata().await {
                    if let Ok(content) = fs::read_to_string(&file_path).await {
                        let modified = metadata
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs() as i64)
                            .unwrap_or(0);

                        let id = path_to_note_id(&base_path, &file_path)
                            .unwrap_or_else(|| "unknown".to_string());

                        notes.push(NoteMetadata {
                            id,
                            title: extract_title(&content),
                            preview: generate_preview(&content),
                            modified,
                        });
                    }
                }
            }
        }
    }

    // Load pinned note IDs from settings
    let pinned_ids: HashSet<String> = {
        let settings = state.settings.read().expect("settings read lock");
        settings
            .pinned_note_ids
            .as_ref()
            .map(|ids| ids.iter().cloned().collect())
            .unwrap_or_default()
    };

    // Sort: pinned notes first (by date), then unpinned notes (by date)
    notes.sort_by(|a, b| {
        let a_pinned = pinned_ids.contains(&a.id);
        let b_pinned = pinned_ids.contains(&b.id);

        match (a_pinned, b_pinned) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => b.modified.cmp(&a.modified),
        }
    });

    // Update cache efficiently
    {
        let mut cache = state.notes_cache.write().expect("cache write lock");
        cache.clear();
        for note in &notes {
            cache.insert(note.id.clone(), note.clone());
        }
    }

    Ok(notes)
}

#[tauri::command]
async fn list_notes(state: State<'_, AppState>) -> Result<Vec<NoteMetadata>, String> {
    list_notes_impl(&state, None, false).await
}

pub async fn read_note_impl(id: String, state: &AppState) -> Result<Note, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config
            .notes_folder
            .clone()
            .ok_or("Notes folder not set")?
    };

    let file_path = resolve_note_path(&folder, &id)?;
    if !file_path.exists() {
        return Err("Note not found".to_string());
    }

    let content = fs::read_to_string(&file_path)
        .await
        .map_err(|e| e.to_string())?;
    let metadata = fs::metadata(&file_path)
        .await
        .map_err(|e| e.to_string())?;

    let modified = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    Ok(Note {
        id,
        title: extract_title(&content),
        content,
        path: file_path.to_string_lossy().into_owned(),
        modified,
    })
}

#[tauri::command]
async fn read_note(id: String, state: State<'_, AppState>) -> Result<Note, String> {
    read_note_impl(id, &state).await
}

pub async fn save_note_impl(
    id: Option<String>,
    content: String,
    state: &AppState,
) -> Result<Note, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config
            .notes_folder
            .clone()
            .ok_or("Notes folder not set")?
    };
    let folder_path = PathBuf::from(&folder);

    let title = extract_title(&content);
    let desired_basename = sanitize_filename(&title);

    // Extract folder prefix from existing ID (e.g., "projects/todo" -> "projects/")
    // so renames stay within the same subfolder
    let folder_prefix = id.as_ref().and_then(|existing| {
        existing.rfind('/').map(|pos| existing[..=pos].to_string())
    });

    let _desired_id = if let Some(ref prefix) = folder_prefix {
        format!("{}{}", prefix, desired_basename)
    } else {
        desired_basename.clone()
    };

    // The directory where this note lives
    let note_dir = if let Some(ref prefix) = folder_prefix {
        let trimmed = prefix.trim_end_matches('/');
        folder_path.join(trimmed)
    } else {
        folder_path.clone()
    };

    // Determine the file ID and path, handling renames
    let (final_id, file_path, old_id) = if let Some(existing_id) = id {
        let old_file_path = resolve_note_path(&folder, &existing_id)?;

        // Compare just the basename portions for rename detection
        let existing_basename = existing_id.rsplit('/').next().unwrap_or(&existing_id);

        if existing_basename != desired_basename {
            // Find a unique name for the new ID
            let mut new_basename = desired_basename.clone();
            let mut counter = 1;

            let new_full_id = |base: &str| -> String {
                if let Some(ref prefix) = folder_prefix {
                    format!("{}{}", prefix, base)
                } else {
                    base.to_string()
                }
            };

            while new_full_id(&new_basename) != existing_id
                && note_dir.join(format!("{}.md", new_basename)).exists()
            {
                new_basename = format!("{}-{}", desired_basename, counter);
                counter += 1;
            }

            let new_id = new_full_id(&new_basename);
            let new_file_path = note_dir.join(format!("{}.md", new_basename));
            (new_id, new_file_path, Some((existing_id, old_file_path)))
        } else {
            (existing_id, old_file_path, None)
        }
    } else {
        // New note - generate unique ID from title
        let mut new_basename = desired_basename.clone();
        let mut counter = 1;

        while note_dir.join(format!("{}.md", new_basename)).exists() {
            new_basename = format!("{}-{}", desired_basename, counter);
            counter += 1;
        }

        let new_id = if let Some(ref prefix) = folder_prefix {
            format!("{}{}", prefix, new_basename)
        } else {
            new_basename.clone()
        };

        (new_id, note_dir.join(format!("{}.md", new_basename)), None)
    };

    // Write the file to the new path
    fs::write(&file_path, &content)
        .await
        .map_err(|e| e.to_string())?;

    // Delete old file AFTER successful write (to prevent data loss)
    if let Some((_, ref old_file_path)) = old_id {
        if old_file_path.exists() && *old_file_path != file_path {
            let _ = fs::remove_file(old_file_path).await;
        }
    }

    let metadata = fs::metadata(&file_path)
        .await
        .map_err(|e| e.to_string())?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // Update search index (delete old entry if renamed, then add new)
    {
        let index = state.search_index.lock().expect("search index mutex");
        if let Some(ref search_index) = *index {
            if let Some((ref old_id_str, _)) = old_id {
                let _ = search_index.delete_note(old_id_str);
            }
            let _ = search_index.index_note(&final_id, &title, &content, modified);
        }
    }

    // Update backlinks index incrementally
    {
        let mut bl_index = state.backlinks_index.write().expect("backlinks write lock");
        // If renamed, remove old entries first
        if let Some((ref old_id_str, _)) = old_id {
            remove_backlinks_for_note(&mut bl_index, old_id_str);
        }
        update_backlinks_for_note(&mut bl_index, &final_id, &title, &content);

        // Save to disk
        let folder = state.app_config.read().expect("app_config read lock")
            .notes_folder.clone();
        if let Some(ref folder) = folder {
            let _ = save_backlinks_index(folder, &bl_index);
        }
    }

    // Update cache (remove old entry if renamed)
    if let Some((ref old_id_str, _)) = old_id {
        let mut cache = state.notes_cache.write().expect("cache write lock");
        cache.remove(old_id_str);
    }

    Ok(Note {
        id: final_id,
        title,
        content,
        path: file_path.to_string_lossy().into_owned(),
        modified,
    })
}

#[tauri::command]
async fn save_note(
    id: Option<String>,
    content: String,
    state: State<'_, AppState>,
) -> Result<Note, String> {
    save_note_impl(id, content, &state).await
}

pub async fn delete_note_impl(id: String, state: &AppState) -> Result<(), String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config
            .notes_folder
            .clone()
            .ok_or("Notes folder not set")?
    };

    let file_path = resolve_note_path(&folder, &id)?;
    if file_path.exists() {
        fs::remove_file(&file_path)
            .await
            .map_err(|e| e.to_string())?;
    }

    // Update search index
    {
        let index = state.search_index.lock().expect("search index mutex");
        if let Some(ref search_index) = *index {
            let _ = search_index.delete_note(&id);
        }
    }

    // Remove from cache
    {
        let mut cache = state.notes_cache.write().expect("cache write lock");
        cache.remove(&id);
    }

    // Remove backlinks from deleted note
    {
        let mut bl_index = state.backlinks_index.write().expect("backlinks write lock");
        remove_backlinks_for_note(&mut bl_index, &id);
        let _ = save_backlinks_index(&folder, &bl_index);
    }

    Ok(())
}

#[tauri::command]
async fn delete_note(id: String, state: State<'_, AppState>) -> Result<(), String> {
    delete_note_impl(id, &state).await
}

pub async fn create_note_impl(
    subfolder: Option<String>,
    state: &AppState,
) -> Result<Note, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config
            .notes_folder
            .clone()
            .ok_or("Notes folder not set")?
    };
    let folder_path = PathBuf::from(&folder);

    // Determine target directory and ID prefix
    let (target_dir, id_prefix) = if let Some(ref sub) = subfolder {
        let validated = validate_note_id(sub)?;
        let p = folder_path.join(&validated);
        if !p.exists() || !p.is_dir() {
            return Err(format!("Folder not found: {}", sub));
        }
        (p, format!("{}/", validated))
    } else {
        (folder_path.clone(), String::new())
    };

    // Generate unique ID
    let base_name = "untitled";
    let mut file_name = base_name.to_string();
    let mut counter = 1;

    while target_dir.join(format!("{}.md", file_name)).exists() {
        file_name = format!("{}-{}", base_name, counter);
        counter += 1;
    }

    let final_id = format!("{}{}", id_prefix, file_name);
    let content = "# Untitled\n\n".to_string();
    let file_path = target_dir.join(format!("{}.md", &file_name));

    fs::write(&file_path, &content)
        .await
        .map_err(|e| e.to_string())?;

    let modified = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // Update search index
    {
        let index = state.search_index.lock().expect("search index mutex");
        if let Some(ref search_index) = *index {
            let _ = search_index.index_note(&final_id, "Untitled", &content, modified);
        }
    }

    Ok(Note {
        id: final_id,
        title: "Untitled".to_string(),
        content,
        path: file_path.to_string_lossy().into_owned(),
        modified,
    })
}

#[tauri::command]
async fn create_note(state: State<'_, AppState>) -> Result<Note, String> {
    create_note_impl(None, &state).await
}

// ── Template system ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub is_builtin: bool,
}

const BUILTIN_TEMPLATES: &[(&str, &str)] = &[
    ("daily-journal", "# Daily Journal\n\n## {{date:MMMM D, YYYY}}\n\n### Gratitude\n\n- \n\n### Today's Goals\n\n- [ ] \n\n### Notes\n\n{{cursor}}\n\n### Reflection\n\n"),
    ("meeting-notes", "# Meeting Notes\n\n**Date:** {{date:YYYY-MM-DD}}  \n**Time:** {{time}}  \n**Attendees:**\n\n- \n\n## Agenda\n\n1. \n\n## Discussion\n\n{{cursor}}\n\n## Action Items\n\n- [ ] \n\n## Next Steps\n\n"),
    ("project-brief", "# {{title}}\n\n## Overview\n\n{{cursor}}\n\n## Goals\n\n- \n\n## Scope\n\n### In Scope\n\n- \n\n### Out of Scope\n\n- \n\n## Timeline\n\n| Phase | Start | End | Status |\n|-------|-------|-----|--------|\n| Planning | {{date:YYYY-MM-DD}} | | Not Started |\n\n## Resources\n\n- \n\n## Risks\n\n| Risk | Impact | Mitigation |\n|------|--------|------------|\n| | | |\n"),
];

fn ensure_templates_dir(notes_folder: &str) -> Result<PathBuf, String> {
    let templates_dir = PathBuf::from(notes_folder).join(".scratch").join("templates");
    std::fs::create_dir_all(&templates_dir).map_err(|e| e.to_string())?;
    for (name, content) in BUILTIN_TEMPLATES {
        let path = templates_dir.join(format!("{}.md", name));
        if !path.exists() {
            std::fs::write(&path, content).map_err(|e| e.to_string())?;
        }
    }
    Ok(templates_dir)
}

fn extract_template_name(filename: &str, content: &str) -> String {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(title) = trimmed.strip_prefix("# ") {
            let title = title.trim();
            if title.starts_with("{{") && title.ends_with("}}") {
                break;
            }
            if !title.is_empty() {
                return title.to_string();
            }
        }
    }
    filename.replace('-', " ").split_whitespace()
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(c) => format!("{}{}", c.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn extract_template_description(content: &str) -> String {
    let mut past_title = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if !past_title {
            if trimmed.starts_with("# ") {
                past_title = true;
                continue;
            }
            if !trimmed.is_empty() {
                past_title = true;
                let stripped = strip_template_vars(trimmed);
                if !stripped.is_empty() {
                    return stripped.chars().take(100).collect();
                }
            }
        } else if !trimmed.is_empty() {
            let stripped = strip_template_vars(trimmed);
            if !stripped.is_empty() {
                return stripped.chars().take(100).collect();
            }
        }
    }
    String::new()
}

fn strip_template_vars(text: &str) -> String {
    let re = regex::Regex::new(r"\{\{[^}]+\}\}").unwrap();
    let result = re.replace_all(text, "").to_string();
    result.trim_start_matches('#').trim_start_matches("**").trim_end_matches("**")
        .trim_start_matches("- ").trim().to_string()
}

fn substitute_template_variables(content: &str, title: &str) -> (String, Option<usize>) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs() as i64;
    let (year, month, day, hour, minute) = unix_to_datetime(secs);
    let month_names = ["January", "February", "March", "April", "May", "June",
                       "July", "August", "September", "October", "November", "December"];
    let month_short = &month_names[month as usize - 1][..3];
    let month_full = month_names[month as usize - 1];
    let mut result = content.to_string();
    let date_format_re = regex::Regex::new(r"\{\{date:([^}]+)\}\}").unwrap();
    result = date_format_re.replace_all(&result, |caps: &regex::Captures| {
        format_date_pattern(&caps[1], year, month, day, month_full, month_short)
    }).to_string();
    result = result.replace("{{date}}", &format!("{:04}-{:02}-{:02}", year, month, day));
    result = result.replace("{{time}}", &format!("{:02}:{:02}", hour, minute));
    result = result.replace("{{title}}", title);
    let cursor_pos = result.find("{{cursor}}");
    result = result.replace("{{cursor}}", "");
    let cursor_line = cursor_pos.map(|pos| {
        result[..pos].lines().count().saturating_sub(1)
    });
    (result, cursor_line)
}

fn unix_to_datetime(secs: i64) -> (i32, u32, u32, u32, u32) {
    let days = (secs / 86400) as i32;
    let time_of_day = (secs % 86400) as u32;
    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let mut y = 1970;
    let mut remaining = days;
    loop {
        let days_in_year = if is_leap_year(y) { 366 } else { 365 };
        if remaining < days_in_year { break; }
        remaining -= days_in_year;
        y += 1;
    }
    let leap = is_leap_year(y);
    let month_days = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut m = 0;
    while m < 12 && remaining >= month_days[m] {
        remaining -= month_days[m];
        m += 1;
    }
    (y, (m + 1) as u32, (remaining + 1) as u32, hour, minute)
}

fn is_leap_year(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn format_date_pattern(pattern: &str, year: i32, month: u32, day: u32, month_full: &str, month_short: &str) -> String {
    let mut result = pattern.to_string();
    result = result.replace("YYYY", &format!("{:04}", year));
    result = result.replace("YY", &format!("{:02}", year % 100));
    result = result.replace("MMMM", month_full);
    result = result.replace("MMM", month_short);
    result = result.replace("MM", &format!("{:02}", month));
    result = result.replace("DD", &format!("{:02}", day));
    if result.contains('D') {
        result = result.replace('D', &format!("{}", day));
    }
    result
}

pub async fn list_templates_impl(state: &AppState) -> Result<Vec<TemplateInfo>, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone().ok_or("Notes folder not set")?
    };
    let templates_dir = ensure_templates_dir(&folder)?;
    let builtin_names: HashSet<&str> = BUILTIN_TEMPLATES.iter().map(|(name, _)| *name).collect();
    let mut templates = Vec::new();
    let mut entries = fs::read_dir(&templates_dir).await.map_err(|e| e.to_string())?;
    while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "md") {
            let filename = path.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let content = fs::read_to_string(&path).await.map_err(|e| e.to_string())?;
            let name = extract_template_name(&filename, &content);
            let description = extract_template_description(&content);
            let is_builtin = builtin_names.contains(filename.as_str());
            templates.push(TemplateInfo { id: filename, name, description, is_builtin });
        }
    }
    templates.sort_by(|a, b| {
        b.is_builtin.cmp(&a.is_builtin).then_with(|| a.name.cmp(&b.name))
    });
    Ok(templates)
}

#[tauri::command]
async fn list_templates(state: State<'_, AppState>) -> Result<Vec<TemplateInfo>, String> {
    list_templates_impl(&state).await
}

pub async fn read_template_impl(id: String, state: &AppState) -> Result<String, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone().ok_or("Notes folder not set")?
    };
    let templates_dir = ensure_templates_dir(&folder)?;
    if id.contains('/') || id.contains('\\') || id.contains("..") || id.contains('\0') {
        return Err("Invalid template ID".to_string());
    }
    let path = templates_dir.join(format!("{}.md", id));
    if !path.exists() {
        return Err(format!("Template not found: {}", id));
    }
    fs::read_to_string(&path).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn read_template(id: String, state: State<'_, AppState>) -> Result<String, String> {
    read_template_impl(id, &state).await
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateNoteResult {
    pub note: Note,
    pub cursor_line: Option<usize>,
}

pub async fn create_note_from_template_impl(
    template_id: String,
    title: Option<String>,
    state: &AppState,
) -> Result<TemplateNoteResult, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone().ok_or("Notes folder not set")?
    };
    let templates_dir = ensure_templates_dir(&folder)?;
    if template_id.contains('/') || template_id.contains('\\') || template_id.contains("..") || template_id.contains('\0') {
        return Err("Invalid template ID".to_string());
    }
    let template_path = templates_dir.join(format!("{}.md", template_id));
    if !template_path.exists() {
        return Err(format!("Template not found: {}", template_id));
    }
    let template_content = fs::read_to_string(&template_path).await.map_err(|e| e.to_string())?;
    let note_title = title.unwrap_or_else(|| "Untitled".to_string());
    let (content, cursor_line) = substitute_template_variables(&template_content, &note_title);
    let actual_title = extract_title(&content);
    let base_name = sanitize_filename(&actual_title);
    let folder_path = PathBuf::from(&folder);
    let mut file_name = base_name.clone();
    let mut counter = 1;
    while folder_path.join(format!("{}.md", file_name)).exists() {
        file_name = format!("{}-{}", base_name, counter);
        counter += 1;
    }
    let file_path = folder_path.join(format!("{}.md", &file_name));
    fs::write(&file_path, &content).await.map_err(|e| e.to_string())?;
    let modified = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    {
        let index = state.search_index.lock().expect("search index mutex");
        if let Some(ref search_index) = *index {
            let _ = search_index.index_note(&file_name, &actual_title, &content, modified);
        }
    }
    Ok(TemplateNoteResult {
        note: Note {
            id: file_name,
            title: actual_title,
            content,
            path: file_path.to_string_lossy().into_owned(),
            modified,
        },
        cursor_line,
    })
}

#[tauri::command]
async fn create_note_from_template(
    template_id: String,
    title: Option<String>,
    state: State<'_, AppState>,
) -> Result<TemplateNoteResult, String> {
    create_note_from_template_impl(template_id, title, &state).await
}

// ── End template system ──────────────────────────────────────────────────────

// List folders under the notes root, optionally under a parent folder.
pub async fn list_folders_impl(
    parent: Option<String>,
    state: &AppState,
) -> Result<Vec<String>, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone().ok_or("Notes folder not set")?
    };

    let base_path = PathBuf::from(&folder);
    let scan_path = if let Some(ref parent_id) = parent {
        let validated = validate_note_id(parent_id)?;
        let p = base_path.join(&validated);
        if !p.exists() || !p.is_dir() {
            return Err(format!("Folder not found: {}", parent_id));
        }
        p
    } else {
        base_path.clone()
    };

    let mut folders = Vec::new();
    let mut entries = fs::read_dir(&scan_path).await.map_err(|e| e.to_string())?;

    while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !should_skip_dir(&name) {
                if let Ok(rel) = path.strip_prefix(&base_path) {
                    folders.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }

    folders.sort();
    Ok(folders)
}

// Create a new folder under the notes root.
pub async fn create_folder_impl(
    folder_path_str: String,
    state: &AppState,
) -> Result<String, String> {
    let notes_folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone().ok_or("Notes folder not set")?
    };

    let validated = validate_note_id(&folder_path_str)?;
    let full_path = PathBuf::from(&notes_folder).join(&validated);

    // Safety: ensure path stays within notes folder
    let base = PathBuf::from(&notes_folder);
    let normalized = full_path.components().collect::<PathBuf>();
    let normalized_base = base.components().collect::<PathBuf>();
    if !normalized.starts_with(&normalized_base) {
        return Err("Path escapes notes folder".to_string());
    }

    fs::create_dir_all(&full_path)
        .await
        .map_err(|e| format!("Failed to create folder: {}", e))?;

    Ok(validated)
}

// Move a note to a different folder.
pub async fn move_note_impl(
    id: String,
    destination: String,
    state: &AppState,
) -> Result<Note, String> {
    let notes_folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone().ok_or("Notes folder not set")?
    };

    let source_path = resolve_note_path(&notes_folder, &id)?;
    if !source_path.exists() {
        return Err(format!("Note not found: {}", id));
    }

    // Resolve destination: "." means root folder
    let base_path = PathBuf::from(&notes_folder);
    let dest_dir = if destination == "." {
        base_path.clone()
    } else {
        let validated = validate_note_id(&destination)?;
        let p = base_path.join(&validated);
        if !p.exists() || !p.is_dir() {
            return Err(format!("Destination folder not found: {}", destination));
        }
        p
    };

    let file_name = source_path
        .file_name()
        .ok_or("Invalid source path")?
        .to_string_lossy()
        .to_string();

    let dest_path = dest_dir.join(&file_name);
    if dest_path.exists() {
        return Err(format!(
            "A note with the same name already exists in {}",
            destination
        ));
    }

    // Perform the move
    fs::rename(&source_path, &dest_path)
        .await
        .map_err(|e| format!("Failed to move note: {}", e))?;

    // Calculate new ID
    let new_id = path_to_note_id(&base_path, &dest_path)
        .ok_or("Failed to compute new note ID")?;

    // Read content for search index
    let content = fs::read_to_string(&dest_path)
        .await
        .map_err(|e| e.to_string())?;

    // Compute metadata before acquiring mutex
    let title = extract_title(&content);
    let modified = fs::metadata(&dest_path)
        .await
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // Update search index: delete old, add new
    {
        let index = state.search_index.lock().expect("search index mutex");
        if let Some(ref search_index) = *index {
            let _ = search_index.delete_note(&id);
            let _ = search_index.index_note(&new_id, &title, &content, modified);
        }
    }

    // Update cache
    {
        let mut cache = state.notes_cache.write().expect("cache write lock");
        cache.remove(&id);
    }

    read_note_impl(new_id, state).await
}

// --- Power Search & File Operations ---

/// Levenshtein edit distance between two strings.
fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let a_len = a_chars.len();
    let b_len = b_chars.len();
    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    let mut prev = (0..=b_len).collect::<Vec<_>>();
    let mut curr = vec![0; b_len + 1];

    for i in 1..=a_len {
        curr[0] = i;
        for j in 1..=b_len {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[b_len]
}

/// Compute best fuzzy match score for `query` within `text` (word-level).
/// Returns (best_distance, best_matching_fragment) or None if no match within threshold.
fn fuzzy_match_line(query: &str, line: &str, max_distance: usize, case_sensitive: bool) -> Option<(usize, usize, usize)> {
    let q = if case_sensitive { query.to_string() } else { query.to_lowercase() };
    let l = if case_sensitive { line.to_string() } else { line.to_lowercase() };

    let q_len = q.chars().count();
    if q_len == 0 {
        return None;
    }

    // Sliding window over text at character level
    let l_chars: Vec<char> = l.chars().collect();
    let l_len = l_chars.len();
    if l_len == 0 {
        return None;
    }

    let mut best: Option<(usize, usize, usize)> = None; // (distance, byte_start, byte_end)

    // Check windows of size q_len-max_distance .. q_len+max_distance
    let min_win = if q_len > max_distance { q_len - max_distance } else { 1 };
    let max_win = (q_len + max_distance).min(l_len);

    for win_size in min_win..=max_win {
        for start in 0..=(l_len.saturating_sub(win_size)) {
            let window: String = l_chars[start..start + win_size].iter().collect();
            let dist = levenshtein_distance(&q, &window);
            if dist <= max_distance {
                // Convert char positions to byte offsets in original line
                let byte_start: usize = line.chars().take(start).map(|c| c.len_utf8()).sum();
                let byte_end: usize = line.chars().take(start + win_size).map(|c| c.len_utf8()).sum();
                match best {
                    Some((d, _, _)) if dist < d => best = Some((dist, byte_start, byte_end)),
                    None => best = Some((dist, byte_start, byte_end)),
                    _ => {}
                }
            }
        }
    }

    best
}

/// List directory contents within the notes folder.
pub async fn list_directory_impl(
    path: Option<String>,
    state: &AppState,
) -> Result<serde_json::Value, String> {
    let notes_folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config
            .notes_folder
            .clone()
            .ok_or("Notes folder not set")?
    };

    let base = PathBuf::from(&notes_folder);
    let target = if let Some(ref p) = path {
        let validated = validate_note_id(p)?;
        let full = base.join(&validated);
        if !full.starts_with(&base) {
            return Err("Path escapes notes folder".to_string());
        }
        full
    } else {
        base.clone()
    };

    if !target.exists() || !target.is_dir() {
        return Err(format!("Directory not found: {}", path.unwrap_or_default()));
    }

    let mut dirs = Vec::new();
    let mut files = Vec::new();

    let mut entries = fs::read_dir(&target).await.map_err(|e| e.to_string())?;
    while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
        let name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();

        if name.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            if !should_skip_dir(&name) {
                let rel = path
                    .strip_prefix(&base)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or(name.clone());
                dirs.push(serde_json::json!({ "name": name, "path": rel }));
            }
        } else {
            let meta = fs::metadata(&path).await.ok();
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let modified = meta
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let rel = path
                .strip_prefix(&base)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or(name.clone());
            files.push(serde_json::json!({
                "name": name,
                "path": rel,
                "size": size,
                "modified": modified
            }));
        }
    }

    Ok(serde_json::json!({
        "directory": path.unwrap_or_else(|| ".".to_string()),
        "directories": dirs,
        "files": files,
        "total_directories": dirs.len(),
        "total_files": files.len()
    }))
}

/// Read any file within the notes folder.
pub async fn read_file_impl(
    path: String,
    state: &AppState,
) -> Result<String, String> {
    let notes_folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config
            .notes_folder
            .clone()
            .ok_or("Notes folder not set")?
    };

    let base = PathBuf::from(&notes_folder);
    let validated = validate_note_id(&path)?;
    let full_path = base.join(&validated);

    if !full_path.starts_with(&base) {
        return Err("Path escapes notes folder".to_string());
    }
    if !full_path.exists() {
        return Err(format!("File not found: {}", path));
    }
    if full_path.is_dir() {
        return Err("Path is a directory, not a file".to_string());
    }

    fs::read_to_string(&full_path)
        .await
        .map_err(|e| format!("Failed to read file: {}", e))
}

/// Powerful find across notes with exact, fuzzy, and regex modes.
pub async fn find_in_notes_impl(
    query: String,
    mode: String,
    note_id: Option<String>,
    case_sensitive: bool,
    context_lines: usize,
    max_distance: Option<usize>,
    state: &AppState,
) -> Result<serde_json::Value, String> {
    let notes_folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config
            .notes_folder
            .clone()
            .ok_or("Notes folder not set")?
    };

    let base = PathBuf::from(&notes_folder);

    // Collect files to search
    let files_to_search: Vec<(String, PathBuf)> = if let Some(ref nid) = note_id {
        let path = resolve_note_path(&notes_folder, nid)?;
        if !path.exists() {
            return Err(format!("Note not found: {}", nid));
        }
        vec![(nid.clone(), path)]
    } else {
        let all_files = walk_md_files(&base, &base).await?;
        all_files
            .into_iter()
            .filter_map(|p| {
                path_to_note_id(&base, &p).map(|id| (id, p))
            })
            .collect()
    };

    // Compile regex if needed
    let compiled_regex = if mode == "regex" {
        Some(
            regex::RegexBuilder::new(&query)
                .case_insensitive(!case_sensitive)
                .build()
                .map_err(|e| format!("Invalid regex: {}", e))?,
        )
    } else {
        None
    };

    let max_dist = max_distance.unwrap_or_else(|| {
        // Default: ~30% of query length, minimum 2
        (query.chars().count() / 3).max(2)
    });

    let mut all_matches = Vec::new();
    let notes_searched = files_to_search.len();

    for (note_id, file_path) in &files_to_search {
        let content = match fs::read_to_string(file_path).await {
            Ok(c) => c,
            Err(_) => continue,
        };

        let lines: Vec<&str> = content.lines().collect();
        let title = extract_title(&content);

        for (line_idx, line) in lines.iter().enumerate() {
            let match_info: Option<(usize, usize, f64)> = match mode.as_str() {
                "exact" => {
                    if case_sensitive {
                        line.find(&query).map(|pos| (pos, pos + query.len(), 1.0))
                    } else {
                        line.to_lowercase()
                            .find(&query.to_lowercase())
                            .map(|pos| (pos, pos + query.len(), 1.0))
                    }
                }
                "fuzzy" => fuzzy_match_line(&query, line, max_dist, case_sensitive)
                    .map(|(dist, start, end)| {
                        let similarity = 1.0
                            - (dist as f64 / query.chars().count().max(1) as f64);
                        (start, end, similarity)
                    }),
                "regex" => {
                    if let Some(ref re) = compiled_regex {
                        re.find(line).map(|m| (m.start(), m.end(), 1.0))
                    } else {
                        None
                    }
                }
                _ => return Err(format!("Unknown search mode: {}. Use 'exact', 'fuzzy', or 'regex'.", mode)),
            };

            if let Some((match_start, match_end, similarity)) = match_info {
                let ctx_start = line_idx.saturating_sub(context_lines);
                let ctx_end = (line_idx + context_lines + 1).min(lines.len());

                let context_before: Vec<String> = lines[ctx_start..line_idx]
                    .iter()
                    .map(|s| s.to_string())
                    .collect();
                let context_after: Vec<String> = lines[(line_idx + 1)..ctx_end]
                    .iter()
                    .map(|s| s.to_string())
                    .collect();

                let matched_text = if match_end <= line.len() {
                    &line[match_start..match_end]
                } else {
                    ""
                };

                all_matches.push(serde_json::json!({
                    "note_id": note_id,
                    "note_title": title,
                    "line_number": line_idx + 1,
                    "line_content": line,
                    "match_start": match_start,
                    "match_end": match_end,
                    "matched_text": matched_text,
                    "similarity": similarity,
                    "context_before": context_before,
                    "context_after": context_after
                }));
            }
        }
    }

    // Sort by similarity descending
    all_matches.sort_by(|a, b| {
        let sa = a.get("similarity").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let sb = b.get("similarity").and_then(|v| v.as_f64()).unwrap_or(0.0);
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(serde_json::json!({
        "query": query,
        "mode": mode,
        "total_matches": all_matches.len(),
        "notes_searched": notes_searched,
        "matches": all_matches
    }))
}

/// Replace text within a note. Supports first, all, and regex modes.
pub async fn replace_in_note_impl(
    id: String,
    find: String,
    replace_with: String,
    mode: String,
    case_sensitive: bool,
    state: &AppState,
) -> Result<serde_json::Value, String> {
    let notes_folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config
            .notes_folder
            .clone()
            .ok_or("Notes folder not set")?
    };

    let file_path = resolve_note_path(&notes_folder, &id)?;
    if !file_path.exists() {
        return Err(format!("Note not found: {}", id));
    }

    let content = fs::read_to_string(&file_path)
        .await
        .map_err(|e| e.to_string())?;

    let (new_content, count) = match mode.as_str() {
        "first" => {
            if case_sensitive {
                if let Some(pos) = content.find(&find) {
                    let mut result = String::with_capacity(content.len());
                    result.push_str(&content[..pos]);
                    result.push_str(&replace_with);
                    result.push_str(&content[pos + find.len()..]);
                    (result, 1)
                } else {
                    (content.clone(), 0)
                }
            } else {
                let lower_content = content.to_lowercase();
                let lower_find = find.to_lowercase();
                if let Some(pos) = lower_content.find(&lower_find) {
                    let mut result = String::with_capacity(content.len());
                    result.push_str(&content[..pos]);
                    result.push_str(&replace_with);
                    result.push_str(&content[pos + find.len()..]);
                    (result, 1)
                } else {
                    (content.clone(), 0)
                }
            }
        }
        "all" => {
            if case_sensitive {
                let count = content.matches(&find).count();
                (content.replace(&find, &replace_with), count)
            } else {
                // Case-insensitive replace all
                let re = regex::RegexBuilder::new(&regex::escape(&find))
                    .case_insensitive(true)
                    .build()
                    .map_err(|e| format!("Failed to build pattern: {}", e))?;
                let count = re.find_iter(&content).count();
                (re.replace_all(&content, replace_with.as_str()).to_string(), count)
            }
        }
        "regex" => {
            let re = regex::RegexBuilder::new(&find)
                .case_insensitive(!case_sensitive)
                .build()
                .map_err(|e| format!("Invalid regex: {}", e))?;
            let count = re.find_iter(&content).count();
            (re.replace_all(&content, replace_with.as_str()).to_string(), count)
        }
        _ => return Err(format!("Unknown replace mode: {}. Use 'first', 'all', or 'regex'.", mode)),
    };

    if count == 0 {
        return Ok(serde_json::json!({
            "note_id": id,
            "replacements_made": 0,
            "message": "No matches found"
        }));
    }

    // Save the updated content
    let note = save_note_impl(Some(id.clone()), new_content, state).await?;

    Ok(serde_json::json!({
        "note_id": note.id,
        "replacements_made": count,
        "note": note
    }))
}

pub fn get_settings_impl(state: &AppState) -> Settings {
    state.settings.read().expect("settings read lock").clone()
}

pub fn update_settings_impl(
    new_settings: Settings,
    state: &AppState,
) -> Result<(), String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone().ok_or("Notes folder not set")?
    };

    {
        let mut settings = state.settings.write().expect("settings write lock");
        *settings = new_settings;
    }

    let settings = state.settings.read().expect("settings read lock");
    save_settings(&folder, &settings).map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
fn get_settings(state: State<AppState>) -> Settings {
    get_settings_impl(&state)
}

#[tauri::command]
fn update_settings(
    new_settings: Settings,
    state: State<AppState>,
) -> Result<(), String> {
    update_settings_impl(new_settings, &state)
}

pub async fn search_notes_impl(query: String, state: &AppState) -> Result<Vec<SearchResult>, String> {
    if query.trim().is_empty() {
        return Ok(vec![]);
    }

    // Check if search index is available and use it (scoped to drop lock before await)
    let search_result = {
        let index = state.search_index.lock().expect("search index mutex");
        if let Some(ref search_index) = *index {
            Some(search_index.search(&query, 20).map_err(|e| e.to_string()))
        } else {
            None
        }
    };

    if let Some(result) = search_result {
        result
    } else {
        // Fallback to simple search if index not available
        fallback_search(&query, state).await
    }
}

#[tauri::command]
async fn search_notes(query: String, state: State<'_, AppState>) -> Result<Vec<SearchResult>, String> {
    search_notes_impl(query, &state).await
}

// Fallback search when Tantivy index isn't available - searches title and full content
async fn fallback_search(query: &str, state: &AppState) -> Result<Vec<SearchResult>, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone()
    };

    let folder = match folder {
        Some(f) => f,
        None => return Ok(vec![]),
    };

    // Collect cache data upfront to avoid holding lock during async operations
    let cache_data: Vec<(String, String, String, i64)> = {
        let cache = state.notes_cache.read().expect("cache read lock");
        cache
            .values()
            .map(|note| {
                (
                    note.id.clone(),
                    note.title.clone(),
                    note.preview.clone(),
                    note.modified,
                )
            })
            .collect()
    };

    let query_lower = query.to_lowercase();
    let mut results: Vec<SearchResult> = Vec::new();

    for (id, title, preview, modified) in cache_data {
        let title_lower = title.to_lowercase();

        let mut score = 0.0f32;
        if title_lower.contains(&query_lower) {
            score += 50.0;
        }

        // Read file content asynchronously and search in it
        let file_path = PathBuf::from(&folder).join(format!("{}.md", &id));
        if let Ok(content) = tokio::fs::read_to_string(&file_path).await {
            let content_lower = content.to_lowercase();
            if content_lower.contains(&query_lower) {
                // Higher score if in title, lower if only in content
                if score == 0.0 {
                    score += 10.0;
                } else {
                    score += 5.0;
                }
            }
        }

        if score > 0.0 {
            results.push(SearchResult {
                id,
                title,
                preview,
                modified,
                score,
            });
        }
    }

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(20);

    Ok(results)
}

// --- Stories _impl functions ---

pub async fn epics_list_impl(
    base_path: Option<String>,
    state: &AppState,
) -> Result<serde_json::Value, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone().ok_or("Notes folder not set")?
    };

    let search_path = if let Some(bp) = base_path {
        PathBuf::from(&folder).join(&bp)
    } else {
        PathBuf::from(&folder)
    };

    let epics = stories::scan_epics(&search_path)?;
    Ok(serde_json::json!({ "epics": epics }))
}

pub async fn boards_get_impl(
    epic_id: String,
    state: &AppState,
) -> Result<serde_json::Value, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone().ok_or("Notes folder not set")?
    };

    let epic_folder = stories::find_epic_folder(&PathBuf::from(&folder), &epic_id)?;
    let all_stories = stories::scan_stories_in_epic(&epic_folder)?;

    let mut lanes: Vec<serde_json::Value> = Vec::new();
    for lane_name in stories::StoryStatus::all_lanes() {
        let cards: Vec<serde_json::Value> = all_stories
            .iter()
            .filter(|s| s.frontmatter.status.as_str() == lane_name)
            .map(|s| {
                let card = stories::story_to_card(s);
                serde_json::to_value(&card).unwrap_or_default()
            })
            .collect();

        lanes.push(serde_json::json!({
            "status": lane_name,
            "cards": cards,
        }));
    }

    Ok(serde_json::json!({
        "epicId": epic_id,
        "lanes": lanes,
        "generatedAt": stories::now_iso8601(),
    }))
}

pub async fn stories_list_impl(
    epic_id: Option<String>,
    status: Option<String>,
    tag: Option<String>,
    owner: Option<String>,
    text: Option<String>,
    state: &AppState,
) -> Result<serde_json::Value, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone().ok_or("Notes folder not set")?
    };
    let notes_folder = PathBuf::from(&folder);

    let mut all_stories: Vec<stories::Story> = Vec::new();

    if let Some(ref eid) = epic_id {
        let epic_folder = stories::find_epic_folder(&notes_folder, eid)?;
        all_stories = stories::scan_stories_in_epic(&epic_folder)?;
    } else {
        // Scan all epics recursively
        fn collect_epics(dir: &std::path::Path, all: &mut Vec<stories::Story>) {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        if name.starts_with("E-") {
                            if let Ok(stories) = stories::scan_stories_in_epic(&entry.path()) {
                                all.extend(stories);
                            }
                        } else if !name.starts_with('.') {
                            collect_epics(&entry.path(), all);
                        }
                    }
                }
            }
        }
        collect_epics(&notes_folder, &mut all_stories);
    }

    // Apply filters
    if let Some(ref s) = status {
        let target = stories::StoryStatus::from_str(s)?;
        all_stories.retain(|story| story.frontmatter.status == target);
    }
    if let Some(ref t) = tag {
        let t_lower = t.to_lowercase();
        all_stories.retain(|story| {
            story
                .frontmatter
                .tags
                .as_ref()
                .map(|tags| tags.iter().any(|tg| tg.to_lowercase() == t_lower))
                .unwrap_or(false)
        });
    }
    if let Some(ref o) = owner {
        let o_lower = o.to_lowercase();
        all_stories.retain(|story| {
            story
                .frontmatter
                .owner
                .as_ref()
                .map(|ow| ow.to_lowercase() == o_lower)
                .unwrap_or(false)
        });
    }
    if let Some(ref txt) = text {
        let t_lower = txt.to_lowercase();
        all_stories.retain(|story| {
            story.frontmatter.title.to_lowercase().contains(&t_lower)
                || story.markdown_body.to_lowercase().contains(&t_lower)
        });
    }

    let result: Vec<serde_json::Value> = all_stories
        .iter()
        .map(|s| {
            serde_json::json!({
                "id": s.frontmatter.id,
                "epic": s.frontmatter.epic,
                "title": s.frontmatter.title,
                "status": s.frontmatter.status,
                "owner": s.frontmatter.owner,
                "estimate_points": s.frontmatter.estimate_points,
                "tags": s.frontmatter.tags.clone().unwrap_or_default(),
                "links": s.frontmatter.links,
                "path": s.path,
                "updated_at": s.frontmatter.timestamps.updated_at,
            })
        })
        .collect();

    Ok(serde_json::json!({ "stories": result }))
}

pub async fn stories_get_impl(
    id: String,
    state: &AppState,
) -> Result<serde_json::Value, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone().ok_or("Notes folder not set")?
    };

    let file_path = stories::find_story_file(&PathBuf::from(&folder), &id)?;
    let content = tokio::fs::read_to_string(&file_path)
        .await
        .map_err(|e| format!("Failed to read story file: {}", e))?;

    let story = stories::parse_story_file(&content, &file_path.to_string_lossy())?;

    Ok(serde_json::json!({
        "story": {
            "frontmatter": story.frontmatter,
            "markdownBody": story.markdown_body,
            "path": story.path,
            "etag": story.etag,
        }
    }))
}

pub async fn stories_create_impl(
    epic_id: String,
    title: String,
    status: Option<String>,
    owner: Option<String>,
    estimate_points: Option<f64>,
    tags: Option<Vec<String>>,
    state: &AppState,
) -> Result<serde_json::Value, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone().ok_or("Notes folder not set")?
    };
    let notes_folder = PathBuf::from(&folder);

    let epic_folder = stories::find_epic_folder(&notes_folder, &epic_id)?;
    let stories_dir = epic_folder.join("stories");
    tokio::fs::create_dir_all(&stories_dir)
        .await
        .map_err(|e| format!("Failed to create stories directory: {}", e))?;

    // Get existing story IDs to determine next sequence
    let existing = stories::scan_stories_in_epic(&epic_folder)?;
    let existing_ids: Vec<&str> = existing.iter().map(|s| s.frontmatter.id.as_str()).collect();
    let new_id = stories::next_story_id(&epic_id, &existing_ids);

    let story_status = if let Some(ref s) = status {
        stories::StoryStatus::from_str(s)?
    } else {
        stories::StoryStatus::Backlog
    };

    let now = stories::now_iso8601();
    let fm = stories::StoryFrontmatter {
        id: new_id.clone(),
        epic: epic_id.clone(),
        title: title.clone(),
        status: story_status,
        owner,
        estimate_points,
        tags,
        links: None,
        timestamps: stories::StoryTimestamps {
            created_at: now.clone(),
            updated_at: now,
        },
    };

    let body = stories::default_story_body();
    let content = stories::serialize_story(&fm, &body);
    let filename = stories::story_filename(&new_id, &title);
    let file_path = stories_dir.join(&filename);

    tokio::fs::write(&file_path, &content)
        .await
        .map_err(|e| format!("Failed to write story file: {}", e))?;

    let rel_path = file_path.to_string_lossy().to_string();

    // Audit log
    let _ = stories::append_audit_event(
        &notes_folder,
        "stories.create",
        &new_id,
        None,
        Some(serde_json::json!({ "title": title, "epic": epic_id })),
    );

    Ok(serde_json::json!({
        "story": {
            "id": new_id,
            "path": rel_path,
        }
    }))
}

pub async fn stories_update_impl(
    id: String,
    etag: String,
    patch: Option<serde_json::Value>,
    markdown_body: Option<String>,
    state: &AppState,
) -> Result<serde_json::Value, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone().ok_or("Notes folder not set")?
    };
    let notes_folder = PathBuf::from(&folder);

    let file_path = stories::find_story_file(&notes_folder, &id)?;
    let content = tokio::fs::read_to_string(&file_path)
        .await
        .map_err(|e| format!("Failed to read story file: {}", e))?;

    let current_etag = stories::compute_etag(&content);
    if current_etag != etag {
        return Err(format!(
            "CONFLICT: etag mismatch. Expected '{}', got '{}'. Refetch the story to get the latest etag.",
            etag, current_etag
        ));
    }

    let mut story = stories::parse_story_file(&content, &file_path.to_string_lossy())?;
    let before = serde_json::to_value(&story.frontmatter).ok();

    // Apply patch to frontmatter
    if let Some(ref p) = patch {
        if let Some(title) = p.get("title").and_then(|v| v.as_str()) {
            story.frontmatter.title = title.to_string();
        }
        if let Some(status) = p.get("status").and_then(|v| v.as_str()) {
            story.frontmatter.status = stories::StoryStatus::from_str(status)?;
        }
        if let Some(owner) = p.get("owner").and_then(|v| v.as_str()) {
            story.frontmatter.owner = Some(owner.to_string());
        }
        if let Some(pts) = p.get("estimate_points").and_then(|v| v.as_f64()) {
            story.frontmatter.estimate_points = Some(pts);
        }
        if let Some(tags) = p.get("tags").and_then(|v| v.as_array()) {
            story.frontmatter.tags = Some(
                tags.iter()
                    .filter_map(|t| t.as_str().map(String::from))
                    .collect(),
            );
        }
        if let Some(links) = p.get("links").and_then(|v| v.as_object()) {
            let mut link_map = story.frontmatter.links.unwrap_or_default();
            for (k, v) in links {
                if let Some(val) = v.as_str() {
                    link_map.insert(k.clone(), val.to_string());
                }
            }
            story.frontmatter.links = Some(link_map);
        }
    }

    // Update body if provided
    let body = markdown_body.unwrap_or(story.markdown_body);

    // Update timestamp
    story.frontmatter.timestamps.updated_at = stories::now_iso8601();

    let new_content = stories::serialize_story(&story.frontmatter, &body);
    tokio::fs::write(&file_path, &new_content)
        .await
        .map_err(|e| format!("Failed to write story file: {}", e))?;

    let new_etag = stories::compute_etag(&new_content);
    let after = serde_json::to_value(&story.frontmatter).ok();

    // Audit log
    let _ = stories::append_audit_event(&notes_folder, "stories.update", &id, before, after);

    Ok(serde_json::json!({
        "ok": true,
        "story": {
            "id": id,
            "etag": new_etag,
            "updated_at": story.frontmatter.timestamps.updated_at,
        }
    }))
}

pub async fn stories_move_impl(
    id: String,
    etag: String,
    status: String,
    state: &AppState,
) -> Result<serde_json::Value, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone().ok_or("Notes folder not set")?
    };
    let notes_folder = PathBuf::from(&folder);

    // Validate the target status
    let new_status = stories::StoryStatus::from_str(&status)?;

    let file_path = stories::find_story_file(&notes_folder, &id)?;
    let content = tokio::fs::read_to_string(&file_path)
        .await
        .map_err(|e| format!("Failed to read story file: {}", e))?;

    let current_etag = stories::compute_etag(&content);
    if current_etag != etag {
        return Err(format!(
            "CONFLICT: etag mismatch. Expected '{}', got '{}'. Refetch the story to get the latest etag.",
            etag, current_etag
        ));
    }

    let mut story = stories::parse_story_file(&content, &file_path.to_string_lossy())?;
    let old_status = story.frontmatter.status.as_str().to_string();

    story.frontmatter.status = new_status;
    story.frontmatter.timestamps.updated_at = stories::now_iso8601();

    let new_content = stories::serialize_story(&story.frontmatter, &story.markdown_body);
    tokio::fs::write(&file_path, &new_content)
        .await
        .map_err(|e| format!("Failed to write story file: {}", e))?;

    let new_etag = stories::compute_etag(&new_content);

    // Audit log
    let _ = stories::append_audit_event(
        &notes_folder,
        "stories.move",
        &id,
        Some(serde_json::json!({ "status": old_status })),
        Some(serde_json::json!({ "status": status })),
    );

    Ok(serde_json::json!({
        "ok": true,
        "story": {
            "id": id,
            "status": status,
            "etag": new_etag,
            "updated_at": story.frontmatter.timestamps.updated_at,
        }
    }))
}

pub async fn search_stories_impl(
    text: Option<String>,
    epic_id: Option<String>,
    tag: Option<String>,
    owner: Option<String>,
    status: Option<String>,
    limit: Option<usize>,
    state: &AppState,
) -> Result<serde_json::Value, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone().ok_or("Notes folder not set")?
    };
    let notes_folder = PathBuf::from(&folder);
    let limit = limit.unwrap_or(20);

    let mut all_stories: Vec<stories::Story> = Vec::new();

    if let Some(ref eid) = epic_id {
        let epic_folder = stories::find_epic_folder(&notes_folder, eid)?;
        all_stories = stories::scan_stories_in_epic(&epic_folder)?;
    } else {
        fn collect_all(dir: &std::path::Path, all: &mut Vec<stories::Story>) {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        if name.starts_with("E-") {
                            if let Ok(stories) = stories::scan_stories_in_epic(&entry.path()) {
                                all.extend(stories);
                            }
                        } else if !name.starts_with('.') {
                            collect_all(&entry.path(), all);
                        }
                    }
                }
            }
        }
        collect_all(&notes_folder, &mut all_stories);
    }

    // Apply filters
    if let Some(ref s) = status {
        let target = stories::StoryStatus::from_str(s)?;
        all_stories.retain(|story| story.frontmatter.status == target);
    }
    if let Some(ref t) = tag {
        let t_lower = t.to_lowercase();
        all_stories.retain(|story| {
            story
                .frontmatter
                .tags
                .as_ref()
                .map(|tags| tags.iter().any(|tg| tg.to_lowercase() == t_lower))
                .unwrap_or(false)
        });
    }
    if let Some(ref o) = owner {
        let o_lower = o.to_lowercase();
        all_stories.retain(|story| {
            story
                .frontmatter
                .owner
                .as_ref()
                .map(|ow| ow.to_lowercase() == o_lower)
                .unwrap_or(false)
        });
    }

    // Text search with snippet extraction
    let mut results: Vec<serde_json::Value> = Vec::new();
    for story in &all_stories {
        let mut snippet = String::new();
        let mut matches = true;

        if let Some(ref txt) = text {
            let t_lower = txt.to_lowercase();
            let title_match = story.frontmatter.title.to_lowercase().contains(&t_lower);
            let body_lower = story.markdown_body.to_lowercase();

            if let Some(pos) = body_lower.find(&t_lower) {
                // Extract snippet around match
                let start = if pos > 40 { pos - 40 } else { 0 };
                let end = (pos + t_lower.len() + 40).min(story.markdown_body.len());
                snippet = format!("…{}…", &story.markdown_body[start..end].replace('\n', " "));
            } else if title_match {
                snippet = story.frontmatter.title.clone();
            } else {
                matches = false;
            }
        }

        if matches {
            results.push(serde_json::json!({
                "id": story.frontmatter.id,
                "path": story.path,
                "title": story.frontmatter.title,
                "snippet": snippet,
                "updated_at": story.frontmatter.timestamps.updated_at,
            }));
        }

        if results.len() >= limit {
            break;
        }
    }

    Ok(serde_json::json!({ "results": results }))
}

pub async fn validate_story_impl(
    id: String,
    state: &AppState,
) -> Result<serde_json::Value, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone().ok_or("Notes folder not set")?
    };

    let file_path = stories::find_story_file(&PathBuf::from(&folder), &id)?;
    let content = tokio::fs::read_to_string(&file_path)
        .await
        .map_err(|e| format!("Failed to read story file: {}", e))?;

    let story = stories::parse_story_file(&content, &file_path.to_string_lossy())?;
    let (errors, warnings) = stories::validate_story(&story);

    Ok(serde_json::json!({
        "valid": errors.is_empty(),
        "errors": errors,
        "warnings": warnings,
    }))
}

// File watcher event payload
#[derive(Clone, Serialize)]
struct FileChangeEvent {
    kind: String,
    path: String,
    changed_ids: Vec<String>,
}

fn setup_file_watcher(
    app: AppHandle,
    notes_folder: &str,
    debounce_map: Arc<Mutex<HashMap<PathBuf, Instant>>>,
) -> Result<FileWatcherState, String> {
    let folder_path = PathBuf::from(notes_folder);
    let app_handle = app.clone();
    let watcher_folder = folder_path.clone();

    let watcher = RecommendedWatcher::new(
        move |res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                for path in event.paths.iter() {
                    // Handle .md files
                    if path.extension().is_some_and(|ext| ext == "md") {
                        // Skip files in excluded directories
                        let dominated_by_excluded = path.components().any(|c| {
                            let name = c.as_os_str().to_string_lossy();
                            should_skip_dir(&name)
                        });
                        if dominated_by_excluded {
                            continue;
                        }

                        // Debounce with cleanup
                        {
                            let mut map = debounce_map.lock().expect("debounce map mutex");
                            let now = Instant::now();

                            // Clean up old entries periodically
                            if map.len() > 100 {
                                map.retain(|_, last| now.duration_since(*last) < Duration::from_secs(5));
                            }

                            if let Some(last) = map.get(path) {
                                if now.duration_since(*last) < Duration::from_millis(500) {
                                    continue;
                                }
                            }
                            map.insert(path.clone(), now);
                        }

                        let kind = match event.kind {
                            notify::EventKind::Create(_) => "created",
                            notify::EventKind::Modify(_) => "modified",
                            notify::EventKind::Remove(_) => "deleted",
                            _ => continue,
                        };

                        // Extract note ID as relative path from notes folder
                        let note_id = path_to_note_id(&watcher_folder, path)
                            .unwrap_or_else(|| {
                                path.file_stem()
                                    .and_then(|s| s.to_str())
                                    .map(|s| s.to_string())
                                    .unwrap_or_default()
                            });

                        // Update search index for external file changes
                        if let Some(state) = app_handle.try_state::<AppState>() {
                            let index = state.search_index.lock().expect("search index mutex");
                            if let Some(ref search_index) = *index {
                                match kind {
                                    "created" | "modified" => {
                                        if let Ok(content) = std::fs::read_to_string(path) {
                                            let title = extract_title(&content);
                                            let modified = std::fs::metadata(path)
                                                .ok()
                                                .and_then(|m| m.modified().ok())
                                                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                                .map(|d| d.as_secs() as i64)
                                                .unwrap_or(0);
                                            let _ = search_index.index_note(&note_id, &title, &content, modified);
                                        }
                                    }
                                    "deleted" => {
                                        let _ = search_index.delete_note(&note_id);
                                    }
                                    _ => {}
                                }
                            }
                        }

                        let _ = app_handle.emit(
                            "file-change",
                            FileChangeEvent {
                                kind: kind.to_string(),
                                path: path.to_string_lossy().into_owned(),
                                changed_ids: vec![note_id.clone()],
                            },
                        );
                    }
                }
            }
        },
        Config::default(),
    )
    .map_err(|e| e.to_string())?;

    let mut watcher = watcher;

    // Watch the notes folder recursively for .md files in subfolders
    watcher
        .watch(&folder_path, RecursiveMode::Recursive)
        .map_err(|e| e.to_string())?;

    Ok(FileWatcherState { watcher })
}

#[tauri::command]
fn start_file_watcher(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config
            .notes_folder
            .clone()
            .ok_or("Notes folder not set")?
    };

    // Clean up debounce map before starting
    cleanup_debounce_map(&state.debounce_map);

    let watcher_state = setup_file_watcher(
        app,
        &folder,
        Arc::clone(&state.debounce_map),
    )?;

    let mut file_watcher = state.file_watcher.lock().expect("file watcher mutex");
    *file_watcher = Some(watcher_state);

    Ok(())
}

#[tauri::command]
fn copy_to_clipboard(app: AppHandle, text: String) -> Result<(), String> {
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_clipboard_image(
    base64_data: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    // Guard against empty clipboard payload
    if base64_data.trim().is_empty() {
        return Err("Clipboard data is empty".to_string());
    }

    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config
            .notes_folder
            .clone()
            .ok_or("Notes folder not set")?
    };

    // Decode base64
    let image_data = base64::engine::general_purpose::STANDARD
        .decode(&base64_data)
        .map_err(|e| format!("Failed to decode base64: {}", e))?;

    // Guard against zero-byte files
    if image_data.is_empty() {
        return Err("Decoded image data is empty".to_string());
    }

    // Create assets folder path
    let assets_dir = PathBuf::from(&folder).join("assets");
    fs::create_dir_all(&assets_dir)
        .await
        .map_err(|e| e.to_string())?;

    // Generate unique filename with timestamp
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut target_name = format!("screenshot-{}.png", timestamp);
    let mut counter = 1;
    let mut target_path = assets_dir.join(&target_name);

    while target_path.exists() {
        target_name = format!("screenshot-{}-{}.png", timestamp, counter);
        target_path = assets_dir.join(&target_name);
        counter += 1;
    }

    // Write the file
    fs::write(&target_path, &image_data)
        .await
        .map_err(|e| format!("Failed to write image: {}", e))?;

    // Return relative path
    Ok(format!("assets/{}", target_name))
}

#[tauri::command]
async fn copy_image_to_assets(
    source_path: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config
            .notes_folder
            .clone()
            .ok_or("Notes folder not set")?
    };

    let source = PathBuf::from(&source_path);
    if !source.exists() {
        return Err("Source image file does not exist".to_string());
    }

    // Get file extension
    let extension = source
        .extension()
        .and_then(|e| e.to_str())
        .ok_or("Invalid file extension")?;

    // Get original filename (without extension)
    let original_name = source
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or("image");

    // Sanitize the filename
    let sanitized_name = sanitize_filename(original_name);

    // Create assets folder path
    let assets_dir = PathBuf::from(&folder).join("assets");
    fs::create_dir_all(&assets_dir)
        .await
        .map_err(|e| e.to_string())?;

    // Generate unique filename
    let mut target_name = format!("{}.{}", sanitized_name, extension);
    let mut counter = 1;
    let mut target_path = assets_dir.join(&target_name);

    while target_path.exists() {
        target_name = format!("{}-{}.{}", sanitized_name, counter, extension);
        target_path = assets_dir.join(&target_name);
        counter += 1;
    }

    // Copy the file
    fs::copy(&source, &target_path)
        .await
        .map_err(|e| format!("Failed to copy image: {}", e))?;

    // Return both relative path and filename for frontend to construct the URL
    Ok(format!("assets/{}", target_name))
}

#[tauri::command]
fn rebuild_search_index(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config
            .notes_folder
            .clone()
            .ok_or("Notes folder not set")?
    };

    let index_path = get_search_index_path(&app).map_err(|e| e.to_string())?;

    // Create new index
    let search_index = SearchIndex::new(&index_path).map_err(|e| e.to_string())?;
    search_index
        .rebuild_index(&PathBuf::from(&folder))
        .map_err(|e| e.to_string())?;

    let mut index = state.search_index.lock().expect("search index mutex");
    *index = Some(search_index);

    Ok(())
}

// --- Backlinks ---

#[tauri::command]
fn get_backlinks(note_title: String, state: State<AppState>) -> Vec<BacklinkEntry> {
    let bl_index = state.backlinks_index.read().expect("backlinks read lock");
    let key = note_title.to_lowercase();
    bl_index.links.get(&key).cloned().unwrap_or_default()
}

#[tauri::command]
fn rebuild_backlinks(state: State<AppState>) -> Result<(), String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config
            .notes_folder
            .clone()
            .ok_or("Notes folder not set")?
    };

    let new_index = rebuild_backlinks_index_from_folder(&folder);
    let mut bl_index = state.backlinks_index.write().expect("backlinks write lock");
    *bl_index = new_index;

    Ok(())
}

// UI helper commands - wrap Tauri plugins for consistent invoke-based API

#[tauri::command]
async fn open_folder_dialog(
    app: AppHandle,
    default_path: Option<String>,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    // Run blocking dialog on a separate thread to avoid blocking the async runtime
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut builder = app.dialog().file().set_can_create_directories(true);

        if let Some(path) = default_path {
            builder = builder.set_directory(path);
        }

        builder.blocking_pick_folder()
    })
    .await
    .map_err(|e| format!("Dialog task failed: {}", e))?;

    Ok(result.map(|p| p.to_string()))
}

#[tauri::command]
async fn reveal_in_file_manager(path: String) -> Result<(), String> {
    let path_buf = PathBuf::from(&path);
    if !path_buf.exists() {
        return Err("Path does not exist".to_string());
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .args(["-R", &path])
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "windows")]
    {
        // Windows explorer /select requires backslashes
        let windows_path = path.replace("/", "\\");
        std::process::Command::new("explorer")
            .args(["/select,", &windows_path])
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        // Linux: open containing directory (most file managers don't support selecting)
        let parent = path_buf
            .parent()
            .ok_or_else(|| "Cannot determine parent directory".to_string())?;
        let parent_str = parent
            .to_str()
            .ok_or_else(|| "Path contains invalid UTF-8".to_string())?;
        std::process::Command::new("xdg-open")
            .arg(parent_str)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        return Err("Unsupported platform".to_string());
    }

    Ok(())
}

#[tauri::command]
async fn open_url_safe(url: String) -> Result<(), String> {
    // Validate URL scheme - only allow http, https, mailto
    let parsed = url::Url::parse(&url).map_err(|e| format!("Invalid URL: {}", e))?;

    match parsed.scheme() {
        "http" | "https" | "mailto" => {}
        scheme => {
            return Err(format!(
                "URL scheme '{}' is not allowed. Only http, https, and mailto are permitted.",
                scheme
            ))
        }
    }

    // Use system opener
    open::that(&url).map_err(|e| format!("Failed to open URL: {}", e))
}

// Git commands - run blocking git operations off the main thread

#[tauri::command]
async fn git_is_available() -> bool {
    tauri::async_runtime::spawn_blocking(git::is_available)
        .await
        .unwrap_or(false)
}

#[tauri::command]
async fn git_get_status(state: State<'_, AppState>) -> Result<git::GitStatus, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone()
    };

    match folder {
        Some(path) => {
            tauri::async_runtime::spawn_blocking(move || {
                git::get_status(&PathBuf::from(path))
            })
            .await
            .map_err(|e| e.to_string())
        }
        None => Ok(git::GitStatus::default()),
    }
}

#[tauri::command]
async fn git_init_repo(state: State<'_, AppState>) -> Result<(), String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone().ok_or("Notes folder not set")?
    };

    tauri::async_runtime::spawn_blocking(move || {
        git::git_init(&PathBuf::from(folder))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn git_commit(message: String, state: State<'_, AppState>) -> Result<git::GitResult, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone()
    };

    match folder {
        Some(path) => {
            tauri::async_runtime::spawn_blocking(move || {
                git::commit_all(&PathBuf::from(path), &message)
            })
            .await
            .map_err(|e| e.to_string())
        }
        None => Ok(git::GitResult {
            success: false,
            message: None,
            error: Some("Notes folder not set".to_string()),
        }),
    }
}

#[tauri::command]
async fn git_push(state: State<'_, AppState>) -> Result<git::GitResult, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone()
    };

    match folder {
        Some(path) => {
            tauri::async_runtime::spawn_blocking(move || {
                git::push(&PathBuf::from(path))
            })
            .await
            .map_err(|e| e.to_string())
        }
        None => Ok(git::GitResult {
            success: false,
            message: None,
            error: Some("Notes folder not set".to_string()),
        }),
    }
}

#[tauri::command]
async fn git_add_remote(url: String, state: State<'_, AppState>) -> Result<git::GitResult, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone()
    };

    match folder {
        Some(path) => {
            tauri::async_runtime::spawn_blocking(move || {
                git::add_remote(&PathBuf::from(path), &url)
            })
            .await
            .map_err(|e| e.to_string())
        }
        None => Ok(git::GitResult {
            success: false,
            message: None,
            error: Some("Notes folder not set".to_string()),
        }),
    }
}

#[tauri::command]
async fn git_push_with_upstream(state: State<'_, AppState>) -> Result<git::GitResult, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone()
    };

    match folder {
        Some(path) => {
            tauri::async_runtime::spawn_blocking(move || {
                // Get current branch first
                let status = git::get_status(&PathBuf::from(&path));
                match status.current_branch {
                    Some(branch) => git::push_with_upstream(&PathBuf::from(&path), &branch),
                    None => git::GitResult {
                        success: false,
                        message: None,
                        error: Some("No current branch found".to_string()),
                    },
                }
            })
            .await
            .map_err(|e| e.to_string())
        }
        None => Ok(git::GitResult {
            success: false,
            message: None,
            error: Some("Notes folder not set".to_string()),
        }),
    }
}

// Check if Claude CLI is installed
fn get_expanded_path() -> String {
    let system_path = std::env::var("PATH").unwrap_or_default();
    let home = std::env::var("HOME").unwrap_or_else(|_| String::new());

    if home.is_empty() {
        return system_path;
    }

    // Common locations for node-installed CLIs (nvm, volta, fnm, homebrew, global npm)
    let candidate_dirs = vec![
        format!("{home}/.nvm/versions/node"),
        format!("{home}/.fnm/node-versions"),
    ];
    let static_dirs = vec![
        format!("{home}/.volta/bin"),
        format!("{home}/.local/bin"),
        "/usr/local/bin".to_string(),
        "/opt/homebrew/bin".to_string(),
    ];

    let mut expanded = Vec::new();

    // For nvm/fnm, scan for node version dirs containing a bin/ folder
    for base in &candidate_dirs {
        if let Ok(entries) = std::fs::read_dir(base) {
            for entry in entries.flatten() {
                let bin_path = entry.path().join("bin");
                if bin_path.exists() {
                    expanded.push(bin_path.to_string_lossy().to_string());
                }
            }
        }
    }

    for dir in static_dirs {
        expanded.push(dir);
    }

    expanded.push(system_path);
    expanded.join(":")
}

#[tauri::command]
async fn ai_check_claude_cli() -> Result<bool, String> {
    use std::process::Command;

    let path = get_expanded_path();
    let which_cmd = if cfg!(target_os = "windows") {
        "where"
    } else {
        "which"
    };

    let check_output = Command::new(which_cmd)
        .arg("claude")
        .env("PATH", &path)
        .output()
        .map_err(|e| format!("Failed to check for claude CLI: {}", e))?;

    Ok(check_output.status.success())
}

// AI execute command
#[tauri::command]
async fn ai_execute_claude(
    file_path: String,
    prompt: String,
) -> Result<AiExecutionResult, String> {
    use std::process::{Child, Command, Stdio};
    use std::io::Write;

    // Check if claude CLI exists
    let path = get_expanded_path();
    let which_cmd = if cfg!(target_os = "windows") {
        "where"
    } else {
        "which"
    };

    let check_output = Command::new(which_cmd)
        .arg("claude")
        .env("PATH", &path)
        .output()
        .map_err(|e| format!("Failed to check for claude CLI: {}", e))?;

    if !check_output.status.success() {
        return Ok(AiExecutionResult {
            success: false,
            output: String::new(),
            error: Some(
                "Claude CLI not found. Please install it from https://claude.ai/code".to_string(),
            ),
        });
    }

    // Execute: echo "prompt" | claude <file> --permission-mode bypassPermissions --print
    let timeout_duration = std::time::Duration::from_secs(300); // 5 minute timeout
    let shared_child: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
    let child_for_task = Arc::clone(&shared_child);
    let mut task = tauri::async_runtime::spawn_blocking(move || {
        let child = Command::new("claude")
            .env("PATH", &path)
            .arg(&file_path)
            .arg("--permission-mode")
            .arg("bypassPermissions")
            .arg("--print")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();

        match child {
            Ok(process) => {
                if let Ok(mut child_guard) = child_for_task.lock() {
                    *child_guard = Some(process);
                } else {
                    return AiExecutionResult {
                        success: false,
                        output: String::new(),
                        error: Some("Failed to lock claude child process handle".to_string()),
                    };
                }

                // Work with the process by taking it from the shared handle.
                let mut process = match child_for_task.lock() {
                    Ok(mut child_guard) => match child_guard.take() {
                        Some(process) => process,
                        None => {
                            return AiExecutionResult {
                                success: false,
                                output: String::new(),
                                error: Some("Claude process handle was unexpectedly missing".to_string()),
                            };
                        }
                    },
                    Err(_) => {
                        return AiExecutionResult {
                            success: false,
                            output: String::new(),
                            error: Some("Failed to lock claude child process handle".to_string()),
                        };
                    }
                };

                // Write prompt to stdin, surfacing errors
                if let Some(mut stdin) = process.stdin.take() {
                    if let Err(e) = stdin.write_all(prompt.as_bytes()) {
                        let _ = process.kill();
                        let _ = process.wait();
                        return AiExecutionResult {
                            success: false,
                            output: String::new(),
                            error: Some(format!("Failed to write prompt to claude stdin: {}", e)),
                        };
                    }
                } else {
                    let _ = process.kill();
                    let _ = process.wait();
                    return AiExecutionResult {
                        success: false,
                        output: String::new(),
                        error: Some("Failed to open stdin for claude process".to_string()),
                    };
                }

                // Wait for completion and get output
                match process.wait_with_output() {
                    Ok(output) => {
                        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                        if output.status.success() {
                            AiExecutionResult {
                                success: true,
                                output: stdout,
                                error: None,
                            }
                        } else {
                            AiExecutionResult {
                                success: false,
                                output: stdout,
                                error: Some(stderr),
                            }
                        }
                    }
                    Err(e) => {
                        AiExecutionResult {
                            success: false,
                            output: String::new(),
                            error: Some(format!("Failed to wait for claude: {}", e)),
                        }
                    }
                }
            }
            Err(e) => {
                AiExecutionResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("Failed to execute claude: {}", e)),
                }
            }
        }
    });

    let result = match tokio::time::timeout(timeout_duration, &mut task).await {
        Ok(join_result) => {
            join_result.map_err(|e| format!("Failed to join Claude blocking task: {}", e))?
        }
        Err(_) => {
            if let Ok(mut child_guard) = shared_child.lock() {
                if let Some(mut process) = child_guard.take() {
                    let _ = process.kill();
                    let _ = process.wait();
                }
            }

            match tokio::time::timeout(std::time::Duration::from_secs(5), task).await {
                Ok(join_result) => {
                    if let Err(e) = join_result {
                        return Err(format!(
                            "Failed to join Claude blocking task after timeout: {}",
                            e
                        ));
                    }
                }
                Err(_) => {
                    return Err(
                        "Claude CLI timed out and failed to exit after kill signal".to_string()
                    );
                }
            }

            AiExecutionResult {
                success: false,
                output: String::new(),
                error: Some("Claude CLI timed out after 5 minutes".to_string()),
            }
        }
    };

    Ok(result)
}

// MCP server status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpStatus {
    pub running: bool,
    pub port: u16,
}

#[tauri::command]
fn mcp_get_status(state: State<AppState>) -> McpStatus {
    let settings = state.settings.read().expect("settings read lock");
    let port = settings.mcp_port.unwrap_or(3921);
    let enabled = settings.mcp_enabled.unwrap_or(false);

    let running = if enabled {
        // Check if server handle exists
        let handle = state.mcp_server_handle.lock().expect("mcp handle mutex");
        handle.is_some()
    } else {
        false
    };

    McpStatus { running, port }
}

#[tauri::command]
async fn mcp_restart(state: State<'_, AppState>) -> Result<McpStatus, String> {
    // Stop existing server if running
    {
        let mut handle = state.mcp_server_handle.lock().expect("mcp handle mutex");
        if let Some(h) = handle.take() {
            h.abort();
        }
    }

    let settings = state.settings.read().expect("settings read lock").clone();
    let enabled = settings.mcp_enabled.unwrap_or(false);
    let port = settings.mcp_port.unwrap_or(3921);

    if enabled {
        let app_state = AppState(Arc::clone(&state.0));
        let server_handle = mcp::start_mcp_server(app_state, port);
        let mut handle = state.mcp_server_handle.lock().expect("mcp handle mutex");
        *handle = Some(server_handle);
    }

    Ok(McpStatus {
        running: enabled,
        port,
    })
}

#[tauri::command]
fn webhook_get_log(state: State<AppState>) -> Vec<webhooks::WebhookLogEntry> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone()
    };
    match folder {
        Some(f) => webhooks::get_webhook_log(&f),
        None => Vec::new(),
    }
}

// ---- Database Tauri Commands ----

fn get_notes_folder_path(state: &AppState) -> Result<PathBuf, String> {
    let app_config = state.app_config.read().expect("app_config read lock");
    app_config
        .notes_folder
        .as_ref()
        .map(|f| PathBuf::from(f))
        .ok_or_else(|| "Notes folder not set".to_string())
}

#[tauri::command]
fn db_list(state: State<AppState>) -> Result<Vec<database::DatabaseInfo>, String> {
    let folder = get_notes_folder_path(&state)?;
    database::scan_databases(&folder)
}

#[tauri::command]
fn db_create(
    name: String,
    columns: Vec<database::ColumnDef>,
    views: Option<Vec<database::ViewDef>>,
    state: State<AppState>,
) -> Result<database::DatabaseInfo, String> {
    let folder = get_notes_folder_path(&state)?;
    database::create_database(&folder, &name, columns, views)
}

#[derive(Serialize, Deserialize)]
struct DatabaseGetResult {
    schema: database::DatabaseSchema,
    rows: Vec<database::DatabaseRow>,
}

#[tauri::command]
fn db_get(db_id: String, state: State<AppState>) -> Result<DatabaseGetResult, String> {
    let folder = get_notes_folder_path(&state)?;
    let (schema, rows) = database::get_database(&folder, &db_id)?;
    Ok(DatabaseGetResult { schema, rows })
}

#[tauri::command]
fn db_get_schema(db_id: String, state: State<AppState>) -> Result<database::DatabaseSchema, String> {
    let folder = get_notes_folder_path(&state)?;
    let db_folder = folder.join(&db_id);
    database::load_schema(&db_folder)
}

#[tauri::command]
fn db_delete(db_id: String, state: State<AppState>) -> Result<(), String> {
    let folder = get_notes_folder_path(&state)?;
    database::delete_database(&folder, &db_id)
}

#[tauri::command]
fn db_create_row(
    db_id: String,
    fields: std::collections::HashMap<String, serde_json::Value>,
    body: Option<String>,
    state: State<AppState>,
) -> Result<database::DatabaseRow, String> {
    let folder = get_notes_folder_path(&state)?;
    database::create_row(&folder, &db_id, fields, body)
}

#[tauri::command]
fn db_update_row(
    db_id: String,
    row_id: String,
    fields: std::collections::HashMap<String, serde_json::Value>,
    body: Option<String>,
    state: State<AppState>,
) -> Result<database::DatabaseRow, String> {
    let folder = get_notes_folder_path(&state)?;
    database::update_row(&folder, &db_id, &row_id, fields, body)
}

#[tauri::command]
fn db_delete_row(db_id: String, row_id: String, state: State<AppState>) -> Result<(), String> {
    let folder = get_notes_folder_path(&state)?;
    database::delete_row(&folder, &db_id, &row_id)
}

#[tauri::command]
fn db_add_column(
    db_id: String,
    column: database::ColumnDef,
    state: State<AppState>,
) -> Result<database::DatabaseSchema, String> {
    let folder = get_notes_folder_path(&state)?;
    database::add_column(&folder, &db_id, column)
}

#[tauri::command]
fn db_remove_column(
    db_id: String,
    column_id: String,
    state: State<AppState>,
) -> Result<database::DatabaseSchema, String> {
    let folder = get_notes_folder_path(&state)?;
    database::remove_column(&folder, &db_id, &column_id)
}

#[tauri::command]
fn db_rename_column(
    db_id: String,
    old_column_id: String,
    new_column_id: String,
    new_name: Option<String>,
    state: State<AppState>,
) -> Result<database::DatabaseSchema, String> {
    let folder = get_notes_folder_path(&state)?;
    database::rename_column(
        &folder,
        &db_id,
        &old_column_id,
        &new_column_id,
        new_name.as_deref(),
    )
}

#[tauri::command]
fn db_update_schema(
    db_id: String,
    schema: database::DatabaseSchema,
    state: State<AppState>,
) -> Result<database::DatabaseSchema, String> {
    let folder = get_notes_folder_path(&state)?;
    database::update_schema(&folder, &db_id, schema)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            // Load app config on startup (contains notes folder path)
            let app_config = load_app_config(app.handle());

            // Load per-folder settings if notes folder is set
            let settings = if let Some(ref folder) = app_config.notes_folder {
                load_settings(folder)
            } else {
                Settings::default()
            };

            // Initialize search index if notes folder is set
            let search_index = if let Some(ref folder) = app_config.notes_folder {
                if let Ok(index_path) = get_search_index_path(app.handle()) {
                    SearchIndex::new(&index_path)
                        .ok()
                        .inspect(|idx| {
                            let _ = idx.rebuild_index(&PathBuf::from(folder));
                        })
                } else {
                    None
                }
            } else {
                None
            };

            // Build backlinks index on startup
            let backlinks_index = if let Some(ref folder) = app_config.notes_folder {
                rebuild_backlinks_index_from_folder(folder)
            } else {
                BacklinksIndex::default()
            };

            let mcp_enabled = settings.mcp_enabled.unwrap_or(false);
            let mcp_port = settings.mcp_port.unwrap_or(3921);

            let state = AppState(Arc::new(AppStateInner {
                app_config: RwLock::new(app_config),
                settings: RwLock::new(settings),
                notes_cache: RwLock::new(HashMap::new()),
                file_watcher: Mutex::new(None),
                search_index: Mutex::new(search_index),
                backlinks_index: RwLock::new(backlinks_index),
                debounce_map: Arc::new(Mutex::new(HashMap::new())),
                mcp_server_handle: Mutex::new(None),
                story_index: Mutex::new(None),
            }));

            // Start MCP server if enabled
            if mcp_enabled {
                let mcp_state = state.clone();
                let server_handle = mcp::start_mcp_server(mcp_state, mcp_port);
                let mut handle = state.mcp_server_handle.lock().expect("mcp handle mutex");
                *handle = Some(server_handle);
            }

            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_notes_folder,
            set_notes_folder,
            list_notes,
            read_note,
            save_note,
            delete_note,
            create_note,
            get_settings,
            update_settings,
            search_notes,
            start_file_watcher,
            rebuild_search_index,
            copy_to_clipboard,
            copy_image_to_assets,
            save_clipboard_image,
            open_folder_dialog,
            reveal_in_file_manager,
            open_url_safe,
            git_is_available,
            git_get_status,
            git_init_repo,
            git_commit,
            git_push,
            git_add_remote,
            git_push_with_upstream,
            ai_check_claude_cli,
            ai_execute_claude,
            mcp_get_status,
            mcp_restart,
            webhook_get_log,
            get_backlinks,
            rebuild_backlinks,
            db_list,
            db_create,
            db_get,
            db_get_schema,
            db_delete,
            db_create_row,
            db_update_row,
            db_delete_row,
            db_add_column,
            db_remove_column,
            db_rename_column,
            db_update_schema,
            list_templates,
            read_template,
            create_note_from_template,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

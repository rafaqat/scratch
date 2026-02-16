#![recursion_limit = "512"]

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
use tauri::webview::{NewWindowResponse, WebviewWindowBuilder};
use tauri::WebviewUrl;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tokio::fs;

pub mod database;
mod git;
mod mcp;
pub mod plugins;
pub mod stories;
pub mod webhooks;

// Note metadata for list display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteMetadata {
    pub id: String,
    pub title: String,
    pub preview: String,
    pub modified: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
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
#[allow(dead_code)]
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

// Utility: Extract icon emoji from frontmatter (icon: "emoji")
fn extract_icon(content: &str) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() || lines[0].trim() != "---" {
        return None;
    }
    for i in 1..lines.len() {
        let trimmed = lines[i].trim();
        if trimmed == "---" {
            break;
        }
        if let Some(rest) = trimmed.strip_prefix("icon:") {
            let val = rest.trim().trim_matches('"').trim_matches('\'');
            if !val.is_empty() {
                return Some(val.to_string());
            }
        }
    }
    None
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
                    icon: extract_icon(&content),
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
                            icon: extract_icon(&content),
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
    list_notes_impl(&state, None, true).await
}

#[tauri::command]
async fn list_folders(parent: Option<String>, state: State<'_, AppState>) -> Result<Vec<String>, String> {
    list_folders_impl(parent, &state).await
}

#[tauri::command]
async fn list_notes_in_folder(folder: Option<String>, state: State<'_, AppState>) -> Result<Vec<NoteMetadata>, String> {
    list_notes_impl(&state, folder.as_deref(), false).await
}

#[tauri::command]
async fn create_folder(folder_path: String, state: State<'_, AppState>) -> Result<String, String> {
    create_folder_impl(folder_path, &state).await
}

#[tauri::command]
async fn rename_folder(old_path: String, new_name: String, state: State<'_, AppState>) -> Result<String, String> {
    let notes_folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone().ok_or("Notes folder not set")?
    };

    let base = PathBuf::from(&notes_folder);
    let validated_old = validate_note_id(&old_path)?;
    let old_full = base.join(&validated_old);

    if !old_full.exists() || !old_full.is_dir() {
        return Err(format!("Folder not found: {}", old_path));
    }

    // Build new path: same parent, new name
    let parent = old_full.parent().ok_or("Cannot rename root")?;
    let sanitized_name = new_name.trim().replace(['/', '\\'], "-");
    if sanitized_name.is_empty() {
        return Err("Folder name cannot be empty".to_string());
    }
    let new_full = parent.join(&sanitized_name);

    if new_full.exists() {
        return Err(format!("A folder named '{}' already exists", sanitized_name));
    }

    // Safety: ensure new path stays within notes folder
    let normalized_new = new_full.components().collect::<PathBuf>();
    let normalized_base = base.components().collect::<PathBuf>();
    if !normalized_new.starts_with(&normalized_base) {
        return Err("Path escapes notes folder".to_string());
    }

    fs::rename(&old_full, &new_full)
        .await
        .map_err(|e| format!("Failed to rename folder: {}", e))?;

    // Return new relative path
    let new_rel = new_full.strip_prefix(&base)
        .map_err(|_| "Failed to compute relative path".to_string())?
        .to_string_lossy()
        .replace('\\', "/");

    // Rebuild search index to update all note IDs under renamed folder
    {
        let index = state.search_index.lock().expect("search index mutex");
        if let Some(ref search_index) = *index {
            let _ = search_index.rebuild_index(&PathBuf::from(&notes_folder));
        }
    }
    // Clear notes cache since IDs changed
    {
        let mut cache = state.notes_cache.write().expect("cache write lock");
        cache.clear();
    }

    Ok(new_rel)
}

#[tauri::command]
async fn delete_folder(folder_path: String, state: State<'_, AppState>) -> Result<(), String> {
    let notes_folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone().ok_or("Notes folder not set")?
    };

    let base = PathBuf::from(&notes_folder);
    let validated = validate_note_id(&folder_path)?;
    let full_path = base.join(&validated);

    if !full_path.exists() || !full_path.is_dir() {
        return Err(format!("Folder not found: {}", folder_path));
    }

    // Safety: ensure path is within notes folder and not the root itself
    let normalized = full_path.components().collect::<PathBuf>();
    let normalized_base = base.components().collect::<PathBuf>();
    if !normalized.starts_with(&normalized_base) || normalized == normalized_base {
        return Err("Cannot delete this folder".to_string());
    }

    // Collect note IDs to remove from search index before deletion
    let mut note_ids = Vec::new();
    let mut entries = fs::read_dir(&full_path).await.map_err(|e| e.to_string())?;
    while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
        let p = entry.path();
        if p.extension().map_or(false, |ext| ext == "md") {
            if let Some(id) = path_to_note_id(&base, &p) {
                note_ids.push(id);
            }
        }
    }

    // Remove from search index
    {
        let index = state.search_index.lock().expect("search index mutex");
        if let Some(ref search_index) = *index {
            for id in &note_ids {
                let _ = search_index.delete_note(id);
            }
        }
    }

    // Remove from cache
    {
        let mut cache = state.notes_cache.write().expect("cache write lock");
        for id in &note_ids {
            cache.remove(id);
        }
    }

    fs::remove_dir_all(&full_path)
        .await
        .map_err(|e| format!("Failed to delete folder: {}", e))?;

    Ok(())
}

#[tauri::command]
async fn move_note(id: String, destination: String, state: State<'_, AppState>) -> Result<Note, String> {
    move_note_impl(id, destination, &state).await
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

    // Snapshot existing content for version history before overwriting
    if file_path.exists() {
        if let Ok(existing_content) = std::fs::read_to_string(&file_path) {
            let snapshot_id = if let Some((ref old_id_str, _)) = old_id {
                old_id_str.clone()
            } else {
                final_id.clone()
            };
            maybe_snapshot_note(&folder, &snapshot_id, &existing_content);
        }
    }

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
    // Soft delete: move to trash instead of permanent deletion
    trash_note(id, state).await
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

#[tauri::command]
async fn create_note_in_folder(folder: String, state: State<'_, AppState>) -> Result<Note, String> {
    create_note_impl(Some(folder), &state).await
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
    ("weekly-planner", "# Weekly Plan — {{date:MMMM D, YYYY}}\n\n> [!TIP]\n> Plan your week ahead. Check off tasks as you complete them.\n\n---\n\n## Monday\n\n- [ ] \n- [ ] \n\n## Tuesday\n\n- [ ] \n- [ ] \n\n## Wednesday\n\n- [ ] \n- [ ] \n\n## Thursday\n\n- [ ] \n- [ ] \n\n## Friday\n\n- [ ] \n- [ ] \n\n---\n\n## Notes\n\n{{cursor}}\n\n## End-of-Week Review\n\n> [!IMPORTANT]\n> What went well this week? What could improve next week?\n\n"),
    ("feature-spec", "# {{title}}\n\n> [!NOTE]\n> Brief description of this feature and why it matters.\n\n---\n\n## Problem\n\n{{cursor}}\n\n## Proposed Solution\n\n\n\n## Acceptance Criteria\n\n- [ ] \n- [ ] \n- [ ] \n\n---\n\n## Technical Notes\n\n| Component | Changes Needed |\n|-----------|----------------|\n| | |\n| | |\n\n## Open Questions\n\n> [!WARNING]\n> List unknowns, risks, or blockers here.\n\n- \n"),
    ("decision-doc", "# Decision: {{title}}\n\n**Date:** {{date:YYYY-MM-DD}}  \n**Status:** Proposed\n\n---\n\n## Context\n\n{{cursor}}\n\n## Options Considered\n\n### Option A\n\n\n\n### Option B\n\n\n\n---\n\n## Comparison\n\n| Criteria | Option A | Option B |\n|----------|----------|----------|\n| Effort | | |\n| Risk | | |\n| Outcome | | |\n\n## Decision\n\n> [!IMPORTANT]\n> Record the final decision and rationale so future readers understand the \"why.\"\n\n\n\n## Consequences\n\n- \n"),
    ("reading-notes", "# {{title}}\n\n**Author:**  \n**Date read:** {{date:YYYY-MM-DD}}\n\n---\n\n## Summary\n\n{{cursor}}\n\n## Key Takeaways\n\n> [!TIP]\n> The most important ideas from this reading.\n\n1. \n2. \n3. \n\n---\n\n## Detailed Notes\n\n\n\n## Quotes\n\n> \n\n## Action Items\n\n- [ ] \n"),
    ("sprint-retro", "# Sprint Retrospective — {{date:MMMM D, YYYY}}\n\n---\n\n## What Went Well\n\n- \n- \n\n## What Could Improve\n\n- \n- \n\n## Action Items\n\n- [ ] \n- [ ] \n\n---\n\n> [!IMPORTANT]\n> Follow up on action items at the start of next sprint.\n\n{{cursor}}\n"),
    ("habit-tracker", "# Habit Tracker — {{date:MMMM YYYY}}\n\n> [!TIP]\n> Track your daily habits. Mark each cell with a check when complete.\n\n---\n\n| Habit | Mon | Tue | Wed | Thu | Fri | Sat | Sun |\n|-------|-----|-----|-----|-----|-----|-----|-----|\n| Exercise | | | | | | | |\n| Reading | | | | | | | |\n| Meditation | | | | | | | |\n| Writing | | | | | | | |\n| | | | | | | | |\n\n---\n\n## Weekly Reflection\n\n{{cursor}}\n\n## Goals for Next Week\n\n- [ ] \n"),
    ("goal-tracker", "# Goals — {{date:YYYY}}\n\n> [!TIP]\n> Set SMART goals: Specific, Measurable, Achievable, Relevant, Time-bound.\n\n---\n\n## Annual Goals\n\n| Goal | Category | Target Date | Status |\n|------|----------|-------------|--------|\n| | Career | | Not Started |\n| | Health | | Not Started |\n| | Personal | | Not Started |\n| | Financial | | Not Started |\n\n---\n\n## Q1 Milestones\n\n- [ ] \n- [ ] \n\n## Q2 Milestones\n\n- [ ] \n- [ ] \n\n## Q3 Milestones\n\n- [ ] \n- [ ] \n\n## Q4 Milestones\n\n- [ ] \n- [ ] \n\n---\n\n## Progress Notes\n\n{{cursor}}\n"),
    ("eisenhower-matrix", "# Eisenhower Matrix — {{date:MMMM D, YYYY}}\n\n> [!NOTE]\n> Prioritize by urgency and importance. Focus on Quadrant 1 first, then schedule Quadrant 2.\n\n---\n\n## 1. Urgent & Important (Do First)\n\n- [ ] \n- [ ] \n\n## 2. Important, Not Urgent (Schedule)\n\n- [ ] \n- [ ] \n\n## 3. Urgent, Not Important (Delegate)\n\n- [ ] \n- [ ] \n\n## 4. Neither (Eliminate)\n\n- \n- \n\n---\n\n{{cursor}}\n"),
    ("one-on-one", "# 1:1 — {{date:MMMM D, YYYY}}\n\n**With:**  \n**Cadence:** Weekly\n\n---\n\n## Check-in\n\nHow are things going? Any blockers?\n\n{{cursor}}\n\n## Updates\n\n- \n\n## Discussion Topics\n\n- \n\n## Action Items\n\n- [ ] \n- [ ] \n\n---\n\n> [!NOTE]\n> Carry over unresolved items to next meeting.\n"),
    ("standup", "# Standup — {{date:MMMM D, YYYY}}\n\n---\n\n## Yesterday\n\n- \n\n## Today\n\n- \n\n## Blockers\n\n> [!WARNING]\n> List anything preventing progress.\n\n{{cursor}}\n"),
    ("travel-planner", "# Trip: {{title}}\n\n**Dates:**  →   \n**Destination:**  \n**Budget:** $\n\n---\n\n## Packing List\n\n- [ ] Passport / ID\n- [ ] Phone & chargers\n- [ ] Toiletries\n- [ ] \n\n## Itinerary\n\n| Day | Date | Activity | Location | Notes |\n|-----|------|----------|----------|-------|\n| 1 | | Arrival | | |\n| 2 | | | | |\n| 3 | | | | |\n| 4 | | Departure | | |\n\n---\n\n## Accommodation\n\n**Hotel/Airbnb:**  \n**Address:**  \n**Confirmation #:**  \n\n## Transportation\n\n- \n\n## Notes\n\n{{cursor}}\n"),
    ("book-tracker", "# Reading Log\n\n> [!TIP]\n> Track books you're reading, want to read, and have finished.\n\n---\n\n| Title | Author | Status | Rating | Finished |\n|-------|--------|--------|--------|----------|\n| | | Reading | | |\n| | | To Read | | |\n| | | To Read | | |\n| | | Done | ⭐⭐⭐⭐ | |\n\n---\n\n## Currently Reading\n\n**Title:**  \n**Author:**  \n\n### Notes\n\n{{cursor}}\n\n### Key Quotes\n\n> \n"),
    ("meal-planner", "# Meal Plan — {{date:MMMM D, YYYY}}\n\n---\n\n| | Breakfast | Lunch | Dinner |\n|-----------|-----------|-------|--------|\n| Monday | | | |\n| Tuesday | | | |\n| Wednesday | | | |\n| Thursday | | | |\n| Friday | | | |\n| Saturday | | | |\n| Sunday | | | |\n\n---\n\n## Grocery List\n\n- [ ] \n- [ ] \n- [ ] \n- [ ] \n\n## Recipes to Try\n\n{{cursor}}\n"),
    ("budget-tracker", "# Budget — {{date:MMMM YYYY}}\n\n---\n\n## Income\n\n| Source | Amount |\n|--------|--------|\n| Salary | $ |\n| | $ |\n| **Total** | **$** |\n\n## Fixed Expenses\n\n| Category | Amount |\n|----------|--------|\n| Rent/Mortgage | $ |\n| Utilities | $ |\n| Insurance | $ |\n| Subscriptions | $ |\n| **Total** | **$** |\n\n## Variable Expenses\n\n| Category | Budget | Actual |\n|----------|--------|--------|\n| Groceries | $ | $ |\n| Dining Out | $ | $ |\n| Transport | $ | $ |\n| Entertainment | $ | $ |\n| **Total** | **$** | **$** |\n\n---\n\n> [!IMPORTANT]\n> Review at month end. Adjust next month's budget based on actuals.\n\n## Notes\n\n{{cursor}}\n"),
    ("blog-post", "# {{title}}\n\n**Status:** Draft  \n**Date:** {{date:YYYY-MM-DD}}  \n**Tags:**  \n\n---\n\n## Hook\n\n{{cursor}}\n\n## Main Points\n\n### 1. \n\n\n\n### 2. \n\n\n\n### 3. \n\n\n\n---\n\n## Conclusion\n\n\n\n## Call to Action\n\n> [!TIP]\n> End with a clear next step for the reader.\n\n"),
    ("cornell-notes", "# {{title}}\n\n**Date:** {{date:YYYY-MM-DD}}  \n**Subject:**  \n\n---\n\n## Key Questions\n\n- \n- \n- \n\n## Notes\n\n{{cursor}}\n\n---\n\n## Summary\n\n> [!IMPORTANT]\n> Summarize the main ideas in your own words after reviewing.\n\n"),
    ("project-tracker", "# {{title}}\n\n**Owner:**  \n**Start:** {{date:YYYY-MM-DD}}  \n**Target:**  \n**Status:** Planning\n\n---\n\n> [!NOTE]\n> Track milestones and tasks for this project.\n\n## Milestones\n\n| Milestone | Due | Status | Notes |\n|-----------|-----|--------|-------|\n| Kickoff | {{date:YYYY-MM-DD}} | Done | |\n| | | Not Started | |\n| | | Not Started | |\n| Launch | | Not Started | |\n\n---\n\n## Tasks\n\n- [ ] \n- [ ] \n- [ ] \n\n## Risks\n\n> [!WARNING]\n> Identify risks early and plan mitigations.\n\n| Risk | Likelihood | Impact | Mitigation |\n|------|-----------|--------|------------|\n| | | | |\n\n## Notes\n\n{{cursor}}\n"),
    ("okr-tracker", "# OKRs — {{date:MMMM YYYY}}\n\n> [!TIP]\n> Each Objective should have 2–5 measurable Key Results.\n\n---\n\n## Objective 1:\n\n| Key Result | Target | Current | Progress |\n|------------|--------|---------|----------|\n| | | | 0% |\n| | | | 0% |\n| | | | 0% |\n\n## Objective 2:\n\n| Key Result | Target | Current | Progress |\n|------------|--------|---------|----------|\n| | | | 0% |\n| | | | 0% |\n\n---\n\n## Weekly Check-in\n\n{{cursor}}\n\n> [!IMPORTANT]\n> Review OKRs weekly. Update progress and adjust approach as needed.\n"),
    ("gratitude-journal", "# Gratitude — {{date:MMMM D, YYYY}}\n\n## Three Things I'm Grateful For\n\n1. \n2. \n3. \n\n---\n\n## Today's Wins\n\nWhat went well today, no matter how small?\n\n- \n\n## Positive Affirmation\n\n> {{cursor}}\n\n## Reflection\n\nWhat made today meaningful?\n\n"),
    ("workout-log", "# Workout — {{date:MMMM D, YYYY}}\n\n**Type:**  \n**Duration:**  \n**Energy Level:** /10\n\n---\n\n## Exercises\n\n| Exercise | Sets | Reps | Weight | Notes |\n|----------|------|------|--------|-------|\n| | | | | |\n| | | | | |\n| | | | | |\n| | | | | |\n| | | | | |\n\n---\n\n## Cardio\n\n| Activity | Duration | Distance | Pace |\n|----------|----------|----------|------|\n| | | | |\n\n## Notes\n\n{{cursor}}\n\n> [!TIP]\n> Track progressive overload: aim to increase weight, reps, or sets each week.\n"),
    ("recipe", "# {{title}}\n\n**Prep Time:**  \n**Cook Time:**  \n**Servings:**  \n**Difficulty:**  \n\n---\n\n## Ingredients\n\n- [ ] \n- [ ] \n- [ ] \n- [ ] \n- [ ] \n\n---\n\n## Instructions\n\n1. \n2. \n3. \n4. \n\n---\n\n## Notes\n\n{{cursor}}\n\n## Variations\n\n- \n"),
    ("subscription-tracker", "# Subscriptions — {{date:MMMM YYYY}}\n\n> [!NOTE]\n> Review monthly to cancel unused subscriptions.\n\n---\n\n| Service | Category | Cost | Billing | Renewal Date | Essential? |\n|---------|----------|------|---------|--------------|------------|\n| | Streaming | $/mo | Monthly | | Yes |\n| | Music | $/mo | Monthly | | |\n| | Cloud | $/mo | Monthly | | Yes |\n| | Software | $/yr | Annual | | |\n| | News | $/mo | Monthly | | |\n| | Fitness | $/mo | Monthly | | |\n| | | | | | |\n\n---\n\n## Monthly Total: $\n\n## Annual Total: $\n\n---\n\n> [!WARNING]\n> Check for price increases and free alternatives.\n\n## Notes\n\n{{cursor}}\n"),
    ("weekly-review", "# Weekly Review — {{date:MMMM D, YYYY}}\n\n> [!TIP]\n> Do this every Friday or Sunday. Clear your mind, plan the next week.\n\n---\n\n## Capture & Process\n\n- [ ] Empty all inboxes (email, messages, notes)\n- [ ] Review loose papers and files\n- [ ] Process all open browser tabs\n\n## Review\n\n- [ ] Review calendar (past week)\n- [ ] Review calendar (next week)\n- [ ] Review active projects\n- [ ] Review waiting-for items\n- [ ] Review someday/maybe list\n\n---\n\n## This Week's Wins\n\n- \n- \n\n## Lessons Learned\n\n- \n\n## Next Week's Priorities\n\n1. \n2. \n3. \n\n---\n\n## Open Loops\n\nAnything still on your mind?\n\n{{cursor}}\n"),
    ("zettelkasten", "# {{title}}\n\n**ID:** {{date:YYYYMMDDHHmm}}  \n**Tags:**  \n**Source:**  \n\n---\n\n## Idea\n\n{{cursor}}\n\n---\n\n## Connections\n\nHow does this relate to other ideas?\n\n- \n- \n\n## References\n\n- \n\n> [!NOTE]\n> Each note should contain one atomic idea. Link generously to other notes.\n"),
    ("swot-analysis", "# SWOT Analysis: {{title}}\n\n**Date:** {{date:YYYY-MM-DD}}\n\n---\n\n## Strengths (Internal)\n\nWhat advantages do we have?\n\n- \n- \n\n## Weaknesses (Internal)\n\nWhat could be improved?\n\n- \n- \n\n## Opportunities (External)\n\nWhat trends or changes can we leverage?\n\n- \n- \n\n## Threats (External)\n\nWhat obstacles do we face?\n\n- \n- \n\n---\n\n## Key Actions\n\n| Priority | Action | Owner | Due |\n|----------|--------|-------|-----|\n| | | | |\n| | | | |\n\n## Notes\n\n{{cursor}}\n"),
    ("content-calendar", "# Content Calendar — {{date:MMMM YYYY}}\n\n> [!TIP]\n> Plan content across all channels. Batch-create for efficiency.\n\n---\n\n| Date | Channel | Topic | Status | Notes |\n|------|---------|-------|--------|-------|\n| | Blog | | Draft | |\n| | Twitter | | Idea | |\n| | Newsletter | | Idea | |\n| | YouTube | | Idea | |\n| | LinkedIn | | Idea | |\n| | | | | |\n| | | | | |\n\n---\n\n## Content Ideas Backlog\n\n- \n- \n- \n\n## This Month's Theme\n\n{{cursor}}\n\n> [!NOTE]\n> Repurpose content: one blog post can become tweets, newsletter sections, and video scripts.\n"),
    ("interview-prep", "# Interview: {{title}}\n\n**Company:**  \n**Role:**  \n**Date:** {{date:YYYY-MM-DD}}  \n**Interviewer(s):**  \n\n---\n\n## Company Research\n\n- **What they do:** \n- **Recent news:** \n- **Why I want to work here:** \n\n## Role Understanding\n\n- **Key responsibilities:** \n- **Must-have skills:** \n\n---\n\n## Questions to Prepare\n\n> [!NOTE]\n> Use the STAR method: Situation, Task, Action, Result.\n\n| Question | My Answer (Key Points) |\n|----------|------------------------|\n| Tell me about yourself | |\n| Why this company? | |\n| Biggest challenge you've overcome? | |\n| Where do you see yourself in 5 years? | |\n| | |\n\n---\n\n## Questions to Ask Them\n\n- \n- \n- \n\n## Post-Interview Notes\n\n{{cursor}}\n"),
    ("faq-page", "---\nicon: \"❓\"\n---\n# {{title}}\n\n[toc]\n\n---\n\n<details><summary>What is this about?</summary></details>\n\n<details><summary>How do I get started?</summary></details>\n\n<details><summary>What are the requirements?</summary></details>\n\n<details><summary>Where can I learn more?</summary></details>\n\n<details><summary>Who do I contact for help?</summary></details>\n\n---\n\n{{cursor}}\n"),
    ("pros-cons", "---\nicon: \"⚖️\"\n---\n# {{title}}\n\n**Date:** {{date:YYYY-MM-DD}}\n\n---\n\n:::columns\n::: col\n## Pros\n\n- \n- \n- \n:::\n::: col\n## Cons\n\n- \n- \n- \n:::\n:::\n\n---\n\n## Verdict\n\n{{cursor}}\n\n> [!IMPORTANT]\n> Weigh the factors that matter most to your situation.\n"),
    ("research-note", "---\nicon: \"🔬\"\nwide: true\n---\n# {{title}}\n\n**Date:** {{date:YYYY-MM-DD}}  \n**Topic:**  \n**Sources:**  \n\n[toc]\n\n---\n\n## Key Findings\n\n{{cursor}}\n\n## Evidence\n\n> [!NOTE]\n> Link to related notes with [[wikilinks]] for a connected knowledge base.\n\n:::columns\n::: col\n### Supporting\n\n- \n- \n:::\n::: col\n### Contradicting\n\n- \n- \n:::\n:::\n\n---\n\n## Methodology\n\n\n\n## Data\n\n| Variable | Value | Source |\n|----------|-------|--------|\n| | | |\n\n## Related Notes\n\n- [[Related Topic A]]\n- [[Related Topic B]]\n\n---\n\n## Formulas & Equations\n\nInline: $E = mc^2$\n\nBlock:\n\n$$\n\\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}\n$$\n"),
    ("project-dashboard", "---\nicon: \"📊\"\nwide: true\n---\n# {{title}}\n\n**Owner:**  \n**Start:** {{date:YYYY-MM-DD}}  \n**Status:** Active\n\n[toc]\n\n---\n\n## Overview\n\n{{cursor}}\n\n## Task Database\n\n[database:project-tasks](view:table)\n\n> [!TIP]\n> Use the slash menu to create the database above if it doesn't exist yet. Add columns for Status, Priority, Assignee, and Due Date.\n\n---\n\n## Progress\n\n<details><summary>Completed Milestones</summary></details>\n\n<details><summary>Upcoming Milestones</summary></details>\n\n---\n\n:::columns\n::: col\n### This Week\n\n- [ ] \n- [ ] \n:::\n::: col\n### Blockers\n\n> [!WARNING]\n> List anything preventing progress.\n\n- \n:::\n:::\n\n---\n\n## Meeting Notes\n\n- [[Meeting 1]]\n\n## Decisions Log\n\n| Date | Decision | Rationale |\n|------|----------|-----------|\n| {{date:YYYY-MM-DD}} | | |\n"),
    ("study-guide", "---\nicon: \"📚\"\n---\n# {{title}}\n\n**Subject:**  \n**Exam Date:**  \n\n[toc]\n\n---\n\n## Key Concepts\n\n<details><summary>Concept 1</summary></details>\n\n<details><summary>Concept 2</summary></details>\n\n<details><summary>Concept 3</summary></details>\n\n---\n\n## Formulas\n\n$$\n\n$$\n\n## Summary Table\n\n| Term | Definition |\n|------|------------|\n| | |\n| | |\n\n---\n\n## Practice Questions\n\n1. \n2. \n3. \n\n## Related Notes\n\n- [[Topic A]]\n- [[Topic B]]\n\n{{cursor}}\n"),
    ("comparison", "---\nicon: \"🔄\"\nwide: true\n---\n# {{title}}\n\n**Date:** {{date:YYYY-MM-DD}}\n\n---\n\n:::columns\n::: col\n## Option A\n\n**Name:**  \n**Cost:**  \n\n### Strengths\n- \n- \n\n### Weaknesses\n- \n:::\n::: col\n## Option B\n\n**Name:**  \n**Cost:**  \n\n### Strengths\n- \n- \n\n### Weaknesses\n- \n:::\n:::\n\n---\n\n## Detailed Comparison\n\n| Criteria | Option A | Option B |\n|----------|----------|----------|\n| Price | | |\n| Quality | | |\n| Speed | | |\n| Support | | |\n\n---\n\n## Recommendation\n\n> [!IMPORTANT]\n> Final verdict and reasoning.\n\n{{cursor}}\n"),
    ("kitchen-sink", "---\nicon: \"🧪\"\nwide: true\n---\n# Scratch Feature Torture Test\n\n**Created:** {{date:MMMM D, YYYY}} at {{time}}  \n**Purpose:** Exercise every Scratch feature on a single page\n\n[toc]\n\n---\n\n## 1. Text Formatting\n\nThis paragraph has **bold**, *italic*, ***bold italic***, ~~strikethrough~~, and `inline code`. Here is a [hyperlink](https://example.com).\n\n## 2. Headings\n\n### Level 3 Heading\n\n#### Level 4 Heading\n\n## 3. Lists\n\n### Unordered\n\n- First item\n  - Nested item\n    - Deep nested\n- Second item\n\n### Ordered\n\n1. Step one\n2. Step two\n   1. Sub-step\n3. Step three\n\n### Task List\n\n- [x] Completed task\n- [ ] Pending task\n- [ ] Another pending task\n\n---\n\n## 4. Blockquotes\n\n> This is a blockquote. It can span multiple lines and contain **formatting**.\n>\n> — Someone wise\n\n---\n\n## 5. Callouts (All 5 Types)\n\n> [!NOTE]\n> This is a **note** callout for general information.\n\n> [!TIP]\n> This is a **tip** callout for helpful advice.\n\n> [!IMPORTANT]\n> This is an **important** callout for critical information.\n\n> [!WARNING]\n> This is a **warning** callout for potential issues.\n\n> [!CAUTION]\n> This is a **caution** callout for dangerous actions.\n\n---\n\n## 6. Code Block\n\n```rust\nfn main() {\n    let greeting = \"Hello from Scratch!\";\n    println!(\"{}\", greeting);\n}\n```\n\n```typescript\nconst sum = (a: number, b: number): number => a + b;\nconsole.log(sum(2, 3));\n```\n\n---\n\n## 7. Tables\n\n| Feature | Status | Priority | Owner |\n|---------|--------|----------|-------|\n| Editor blocks | Done | High | Claude |\n| Databases | Done | High | Claude |\n| Page decoration | Done | Medium | Claude |\n| Templates | Done | Low | Claude |\n\n---\n\n## 8. Divider\n\nContent above the divider.\n\n---\n\nContent below the divider.\n\n---\n\n## 9. Toggle Blocks\n\n<details><summary>Click to expand — Toggle Block 1</summary></details>\n\n<details><summary>Click to expand — Toggle Block 2</summary></details>\n\n<details><summary>Nested content toggle</summary></details>\n\n---\n\n## 10. Column Layout\n\n:::columns\n::: col\n### Left Column\n\nThis is the left column. It supports **full markdown** including:\n\n- Bullet points\n- **Bold text**\n- `Inline code`\n:::\n::: col\n### Right Column\n\nThis is the right column with different content:\n\n1. Numbered list\n2. More items\n3. Third item\n:::\n:::\n\n---\n\n## 11. Wikilinks\n\nLink to other notes: [[Daily Journal]] and [[Meeting Notes]].\n\nWikilinks create a connected knowledge graph between your notes.\n\n---\n\n## 12. Equations\n\n### Inline Math\n\nEuler's identity: $e^{i\\pi} + 1 = 0$. The area of a circle is $A = \\pi r^2$.\n\n### Block Math\n\n$$\n\\int_{-\\infty}^{\\infty} e^{-x^2} dx = \\sqrt{\\pi}\n$$\n\n$$\n\\sum_{n=1}^{\\infty} \\frac{1}{n^2} = \\frac{\\pi^2}{6}\n$$\n\n---\n\n## 13. Database — Table View\n\n[database:scratch-torture-db](view:table)\n\n---\n\n## 14. Database — Calendar View\n\n[database:scratch-torture-db](view:calendar)\n\n---\n\n## 15. Images\n\nUse the slash menu or drag-and-drop to insert images. They are stored in the `.assets/` folder.\n\n---\n\n## 16. Template Variables\n\nThese were substituted when this note was created:\n\n- **Date:** {{date:YYYY-MM-DD}}\n- **Long date:** {{date:MMMM D, YYYY}}\n- **Time:** {{time}}\n- **Title:** {{title}}\n- **Cursor landed here →** {{cursor}}\n\n---\n\n## 17. Page Decoration\n\nThis note uses frontmatter for:\n- `icon: \"🧪\"` — Page icon displayed in sidebar\n- `wide: true` — Full-width layout\n\n---\n\n*End of torture test. If everything above renders correctly, all Scratch features are working.*\n"),
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
    // Auto-create any referenced databases that don't exist yet
    ensure_template_databases(&folder_path, &content);

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

/// Scan template content for `[database:name](view:...)` references and create
/// sample databases with columns and rows so the template works out of the box.
fn ensure_template_databases(notes_folder: &std::path::Path, content: &str) {
    let re = regex::Regex::new(r"\[database:([^\]]+)\]\(view:(\w+)\)").unwrap();
    for cap in re.captures_iter(content) {
        let db_name = &cap[1];
        let slug = database::slugify(db_name);
        let db_folder = notes_folder.join(&slug);
        if db_folder.exists() {
            continue; // Already exists
        }
        // Create a sample database with useful columns and rows
        let columns = vec![
            database::ColumnDef {
                id: "title".to_string(),
                name: "Title".to_string(),
                col_type: database::ColumnType::Text,
                options: None,
                target: None,
            },
            database::ColumnDef {
                id: "status".to_string(),
                name: "Status".to_string(),
                col_type: database::ColumnType::Select,
                options: Some(vec![
                    "Backlog".to_string(),
                    "In Progress".to_string(),
                    "Done".to_string(),
                ]),
                target: None,
            },
            database::ColumnDef {
                id: "priority".to_string(),
                name: "Priority".to_string(),
                col_type: database::ColumnType::Select,
                options: Some(vec![
                    "High".to_string(),
                    "Medium".to_string(),
                    "Low".to_string(),
                ]),
                target: None,
            },
            database::ColumnDef {
                id: "due".to_string(),
                name: "Due Date".to_string(),
                col_type: database::ColumnType::Date,
                options: None,
                target: None,
            },
            database::ColumnDef {
                id: "done".to_string(),
                name: "Complete".to_string(),
                col_type: database::ColumnType::Checkbox,
                options: None,
                target: None,
            },
        ];

        match database::create_database(notes_folder, db_name, columns, None) {
            Ok(_) => {
                // Add sample rows
                // Generate dates relative to today
                let today = chrono::Local::now().date_naive();
                let fmt = |d: chrono::NaiveDate| d.format("%Y-%m-%d").to_string();
                let sample_rows: Vec<std::collections::HashMap<String, serde_json::Value>> = vec![
                    [
                        ("title".to_string(), serde_json::json!("Set up project structure")),
                        ("status".to_string(), serde_json::json!("Done")),
                        ("priority".to_string(), serde_json::json!("High")),
                        ("due".to_string(), serde_json::json!(fmt(today - chrono::Duration::days(3)))),
                        ("done".to_string(), serde_json::json!(true)),
                    ].into_iter().collect(),
                    [
                        ("title".to_string(), serde_json::json!("Design the UI mockups")),
                        ("status".to_string(), serde_json::json!("In Progress")),
                        ("priority".to_string(), serde_json::json!("High")),
                        ("due".to_string(), serde_json::json!(fmt(today))),
                        ("done".to_string(), serde_json::json!(false)),
                    ].into_iter().collect(),
                    [
                        ("title".to_string(), serde_json::json!("Write documentation")),
                        ("status".to_string(), serde_json::json!("Backlog")),
                        ("priority".to_string(), serde_json::json!("Medium")),
                        ("due".to_string(), serde_json::json!(fmt(today + chrono::Duration::days(5)))),
                        ("done".to_string(), serde_json::json!(false)),
                    ].into_iter().collect(),
                    [
                        ("title".to_string(), serde_json::json!("Add unit tests")),
                        ("status".to_string(), serde_json::json!("Backlog")),
                        ("priority".to_string(), serde_json::json!("Low")),
                        ("due".to_string(), serde_json::json!(fmt(today + chrono::Duration::days(10)))),
                        ("done".to_string(), serde_json::json!(false)),
                    ].into_iter().collect(),
                ];
                for fields in sample_rows {
                    let _ = database::create_row(notes_folder, &slug, fields, None);
                }
            }
            Err(e) => {
                eprintln!("Failed to create template database '{}': {}", db_name, e);
            }
        }
    }
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

// --- Database _impl functions ---

pub async fn db_list_impl(state: &AppState) -> Result<serde_json::Value, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config
            .notes_folder
            .clone()
            .ok_or("Notes folder not set")?
    };

    let databases = database::scan_databases(&PathBuf::from(&folder))?;
    Ok(serde_json::json!({ "databases": databases }))
}

pub async fn db_get_schema_impl(
    database_id: String,
    state: &AppState,
) -> Result<serde_json::Value, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config
            .notes_folder
            .clone()
            .ok_or("Notes folder not set")?
    };

    let db_folder = PathBuf::from(&folder).join(&database_id);
    if !database::is_database_folder(&db_folder) {
        return Err(format!("'{}' is not a database folder", database_id));
    }

    let schema = database::load_schema(&db_folder)?;
    Ok(serde_json::json!({
        "database_id": database_id,
        "schema": schema,
    }))
}

pub async fn db_query_impl(
    database_id: String,
    filters: Option<serde_json::Value>,
    sort: Option<serde_json::Value>,
    limit: usize,
    offset: usize,
    state: &AppState,
) -> Result<serde_json::Value, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config
            .notes_folder
            .clone()
            .ok_or("Notes folder not set")?
    };
    let notes_folder = PathBuf::from(&folder);

    let (schema, mut rows) = database::get_database(&notes_folder, &database_id)?;

    // Apply filters
    if let Some(filter_val) = filters {
        if let Some(filter_arr) = filter_val.as_array() {
            for filter in filter_arr {
                let field = filter
                    .get("field")
                    .and_then(|v| v.as_str())
                    .ok_or("Filter missing 'field'")?;
                let operator = filter
                    .get("operator")
                    .and_then(|v| v.as_str())
                    .ok_or("Filter missing 'operator'")?;
                let value = filter.get("value");

                rows.retain(|row| {
                    let row_val = row.fields.get(field);
                    match operator {
                        "eq" => match (row_val, value) {
                            (Some(rv), Some(fv)) => rv == fv,
                            (None, None) => true,
                            _ => false,
                        },
                        "neq" => match (row_val, value) {
                            (Some(rv), Some(fv)) => rv != fv,
                            (None, None) => false,
                            _ => true,
                        },
                        "gt" => compare_values(row_val, value, |a, b| a > b),
                        "gte" => compare_values(row_val, value, |a, b| a >= b),
                        "lt" => compare_values(row_val, value, |a, b| a < b),
                        "lte" => compare_values(row_val, value, |a, b| a <= b),
                        "contains" => {
                            let needle = value
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_lowercase();
                            match row_val {
                                Some(serde_json::Value::String(s)) => {
                                    s.to_lowercase().contains(&needle)
                                }
                                Some(serde_json::Value::Array(arr)) => arr.iter().any(|item| {
                                    item.as_str()
                                        .map(|s| s.to_lowercase() == needle)
                                        .unwrap_or(false)
                                }),
                                _ => false,
                            }
                        }
                        "not_contains" => {
                            let needle = value
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_lowercase();
                            match row_val {
                                Some(serde_json::Value::String(s)) => {
                                    !s.to_lowercase().contains(&needle)
                                }
                                Some(serde_json::Value::Array(arr)) => !arr.iter().any(|item| {
                                    item.as_str()
                                        .map(|s| s.to_lowercase() == needle)
                                        .unwrap_or(false)
                                }),
                                None => true,
                                _ => true,
                            }
                        }
                        "starts_with" => {
                            let prefix = value
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_lowercase();
                            row_val
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_lowercase().starts_with(&prefix))
                                .unwrap_or(false)
                        }
                        "ends_with" => {
                            let suffix = value
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_lowercase();
                            row_val
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_lowercase().ends_with(&suffix))
                                .unwrap_or(false)
                        }
                        "is_empty" => match row_val {
                            None => true,
                            Some(serde_json::Value::String(s)) => s.is_empty(),
                            Some(serde_json::Value::Array(a)) => a.is_empty(),
                            Some(serde_json::Value::Null) => true,
                            _ => false,
                        },
                        "is_not_empty" => match row_val {
                            None => false,
                            Some(serde_json::Value::String(s)) => !s.is_empty(),
                            Some(serde_json::Value::Array(a)) => !a.is_empty(),
                            Some(serde_json::Value::Null) => false,
                            _ => true,
                        },
                        _ => true, // Unknown operator: no filter
                    }
                });
            }
        }
    }

    // Apply sorting
    if let Some(sort_val) = sort {
        let sort_field = sort_val
            .get("field")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let sort_desc = sort_val
            .get("direction")
            .and_then(|v| v.as_str())
            .map(|d| d == "desc")
            .unwrap_or(false);

        if !sort_field.is_empty() {
            rows.sort_by(|a, b| {
                let va = a.fields.get(sort_field);
                let vb = b.fields.get(sort_field);
                let ord = compare_json_values(va, vb);
                if sort_desc {
                    ord.reverse()
                } else {
                    ord
                }
            });
        }
    }

    let total = rows.len();

    // Apply pagination
    let paginated: Vec<_> = rows.into_iter().skip(offset).take(limit).collect();

    // Compute etags for each row (for concurrency control on update)
    let rows_with_etag: Vec<serde_json::Value> = paginated
        .iter()
        .map(|row| {
            let row_path = std::path::Path::new(&row.path);
            let content = std::fs::read_to_string(row_path).unwrap_or_default();
            let etag = stories::compute_etag(&content);
            serde_json::json!({
                "id": row.id,
                "fields": row.fields,
                "body": row.body,
                "etag": etag,
                "modified": row.modified,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "database_id": database_id,
        "total": total,
        "offset": offset,
        "limit": limit,
        "rows": rows_with_etag,
        "columns": schema.columns.iter().map(|c| serde_json::json!({
            "id": c.id,
            "name": c.name,
            "type": c.col_type.as_str(),
        })).collect::<Vec<_>>(),
    }))
}

/// Compare two JSON values for filter operations (gt, gte, lt, lte).
fn compare_values(
    row_val: Option<&serde_json::Value>,
    filter_val: Option<&serde_json::Value>,
    cmp: fn(f64, f64) -> bool,
) -> bool {
    match (row_val, filter_val) {
        (Some(rv), Some(fv)) => {
            // Try numeric comparison first
            let rn = rv.as_f64();
            let fn_ = fv.as_f64();
            if let (Some(a), Some(b)) = (rn, fn_) {
                return cmp(a, b);
            }
            // Fall back to string comparison
            let rs = rv.as_str().unwrap_or("");
            let fs = fv.as_str().unwrap_or("");
            let ord = rs.cmp(fs);
            match ord {
                std::cmp::Ordering::Less => cmp(-1.0, 0.0),
                std::cmp::Ordering::Equal => cmp(0.0, 0.0),
                std::cmp::Ordering::Greater => cmp(1.0, 0.0),
            }
        }
        _ => false,
    }
}

/// Compare two optional JSON values for sorting.
fn compare_json_values(
    a: Option<&serde_json::Value>,
    b: Option<&serde_json::Value>,
) -> std::cmp::Ordering {
    match (a, b) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (Some(va), Some(vb)) => {
            // Numeric comparison
            if let (Some(na), Some(nb)) = (va.as_f64(), vb.as_f64()) {
                return na.partial_cmp(&nb).unwrap_or(std::cmp::Ordering::Equal);
            }
            // Boolean comparison
            if let (Some(ba), Some(bb)) = (va.as_bool(), vb.as_bool()) {
                return ba.cmp(&bb);
            }
            // String comparison (covers text, date, select, url)
            let sa = va.as_str().unwrap_or("");
            let sb = vb.as_str().unwrap_or("");
            sa.cmp(sb)
        }
    }
}

pub async fn db_insert_row_impl(
    database_id: String,
    fields_val: serde_json::Value,
    body: Option<String>,
    state: &AppState,
) -> Result<serde_json::Value, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config
            .notes_folder
            .clone()
            .ok_or("Notes folder not set")?
    };
    let notes_folder = PathBuf::from(&folder);

    // Convert JSON Value to HashMap<String, JsonValue>
    let fields: std::collections::HashMap<String, serde_json::Value> = fields_val
        .as_object()
        .ok_or("'fields' must be a JSON object")?
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let row = database::create_row(&notes_folder, &database_id, fields, body)?;

    Ok(serde_json::json!({
        "row": {
            "id": row.id,
            "fields": row.fields,
            "body": row.body,
            "path": row.path,
            "modified": row.modified,
        }
    }))
}

pub async fn db_update_row_impl(
    database_id: String,
    row_id: String,
    etag: String,
    fields_val: serde_json::Value,
    body: Option<String>,
    state: &AppState,
) -> Result<serde_json::Value, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config
            .notes_folder
            .clone()
            .ok_or("Notes folder not set")?
    };
    let notes_folder = PathBuf::from(&folder);

    // Check etag for concurrency control
    let db_folder = notes_folder.join(&database_id);
    let row_path = db_folder.join(format!("{}.md", row_id));
    if !row_path.exists() {
        return Err(format!(
            "Row '{}' not found in database '{}'",
            row_id, database_id
        ));
    }

    let current_content = std::fs::read_to_string(&row_path)
        .map_err(|e| format!("Failed to read row file: {}", e))?;
    let current_etag = stories::compute_etag(&current_content);

    if current_etag != etag {
        return Err(format!(
            "CONFLICT: etag mismatch. Expected '{}', got '{}'. Refetch the row via db_query to get the latest etag.",
            etag, current_etag
        ));
    }

    // Convert JSON Value to HashMap
    let fields: std::collections::HashMap<String, serde_json::Value> = fields_val
        .as_object()
        .ok_or("'fields' must be a JSON object")?
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let row = database::update_row(&notes_folder, &database_id, &row_id, fields, body)?;

    // Compute new etag
    let new_content = std::fs::read_to_string(&row.path)
        .map_err(|e| format!("Failed to read updated row: {}", e))?;
    let new_etag = stories::compute_etag(&new_content);

    Ok(serde_json::json!({
        "ok": true,
        "row": {
            "id": row.id,
            "fields": row.fields,
            "body": row.body,
            "etag": new_etag,
            "modified": row.modified,
        }
    }))
}

pub async fn db_delete_row_impl(
    database_id: String,
    row_id: String,
    state: &AppState,
) -> Result<serde_json::Value, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config
            .notes_folder
            .clone()
            .ok_or("Notes folder not set")?
    };
    let notes_folder = PathBuf::from(&folder);

    database::delete_row(&notes_folder, &database_id, &row_id)?;

    Ok(serde_json::json!({
        "ok": true,
        "deleted": { "database_id": database_id, "row_id": row_id }
    }))
}

pub async fn db_create_impl(
    name: String,
    columns_val: serde_json::Value,
    state: &AppState,
) -> Result<serde_json::Value, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config
            .notes_folder
            .clone()
            .ok_or("Notes folder not set")?
    };
    let notes_folder = PathBuf::from(&folder);

    // Parse column definitions from JSON
    let columns_arr = columns_val
        .as_array()
        .ok_or("'columns' must be a JSON array")?;

    let mut columns: Vec<database::ColumnDef> = Vec::new();
    for col_val in columns_arr {
        let id = col_val
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or("Column missing 'id'")?
            .to_string();
        let col_name = col_val
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or("Column missing 'name'")?
            .to_string();
        let col_type_str = col_val
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or("Column missing 'type'")?;
        let col_type = database::ColumnType::from_str(col_type_str)?;

        let options = col_val.get("options").and_then(|v| v.as_array()).map(|arr| {
            arr.iter()
                .filter_map(|item| item.as_str().map(String::from))
                .collect()
        });

        let target = col_val
            .get("target")
            .and_then(|v| v.as_str())
            .map(String::from);

        columns.push(database::ColumnDef {
            id,
            name: col_name,
            col_type,
            options,
            target,
        });
    }

    let db_info = database::create_database(&notes_folder, &name, columns, None)?;

    Ok(serde_json::json!({
        "database": db_info,
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

/// URL metadata returned by fetch_url_metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UrlMetadata {
    pub title: String,
    pub description: String,
    pub image: String,
    pub favicon: String,
    pub domain: String,
}

#[tauri::command]
async fn fetch_url_metadata(url: String) -> Result<UrlMetadata, String> {
    // Validate URL scheme
    let parsed = url::Url::parse(&url).map_err(|e| format!("Invalid URL: {}", e))?;
    match parsed.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(format!(
                "URL scheme '{}' is not allowed. Only http and https are permitted.",
                scheme
            ))
        }
    }

    let domain = parsed.host_str().unwrap_or("").to_string();

    // Build a favicon URL from the domain
    let favicon_default = format!(
        "https://www.google.com/s2/favicons?domain={}&sz=32",
        domain
    );

    // Fetch the HTML
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("Mozilla/5.0 (compatible; Scratch/1.0)")
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch URL: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    let html = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    // Parse HTML with scraper
    let document = scraper::Html::parse_document(&html);

    // Helper to get meta content by property or name
    let get_meta = |attr: &str, value: &str| -> Option<String> {
        let selector_str = format!("meta[{}=\"{}\"]", attr, value);
        let result = if let Ok(selector) = scraper::Selector::parse(&selector_str) {
            document
                .select(&selector)
                .next()
                .and_then(|el| el.value().attr("content"))
                .map(|s| s.to_string())
        } else {
            None
        };
        result
    };

    // Extract title: og:title > twitter:title > <title>
    let title = get_meta("property", "og:title")
        .or_else(|| get_meta("name", "twitter:title"))
        .or_else(|| {
            scraper::Selector::parse("title")
                .ok()
                .and_then(|sel| document.select(&sel).next())
                .map(|el| el.text().collect::<String>())
        })
        .unwrap_or_default()
        .trim()
        .to_string();

    // Extract description: og:description > twitter:description > meta description
    let description = get_meta("property", "og:description")
        .or_else(|| get_meta("name", "twitter:description"))
        .or_else(|| get_meta("name", "description"))
        .unwrap_or_default()
        .trim()
        .to_string();

    // Extract image: og:image > twitter:image
    let image = get_meta("property", "og:image")
        .or_else(|| get_meta("name", "twitter:image"))
        .unwrap_or_default()
        .trim()
        .to_string();

    // Extract favicon: link[rel~=icon] > default
    let favicon = scraper::Selector::parse("link[rel~=\"icon\"], link[rel=\"shortcut icon\"]")
        .ok()
        .and_then(|sel| {
            document
                .select(&sel)
                .next()
                .and_then(|el| el.value().attr("href"))
                .map(|href| {
                    // Resolve relative URLs
                    if href.starts_with("http") {
                        href.to_string()
                    } else if href.starts_with("//") {
                        format!("https:{}", href)
                    } else {
                        format!("{}://{}{}", parsed.scheme(), domain, if href.starts_with('/') { href.to_string() } else { format!("/{}", href) })
                    }
                })
        })
        .unwrap_or(favicon_default);

    Ok(UrlMetadata {
        title,
        description,
        image,
        favicon,
        domain,
    })
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

// ---- Import / Export Commands ----

/// Strip YAML frontmatter (---...---) from markdown content for clean export.
fn strip_frontmatter(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() || lines[0].trim() != "---" {
        return content.to_string();
    }
    for i in 1..lines.len() {
        if lines[i].trim() == "---" {
            // Return everything after the closing ---
            let rest = &lines[i + 1..];
            let result = rest.join("\n");
            return result.trim_start_matches('\n').to_string();
        }
    }
    content.to_string()
}

/// Convert markdown to a styled HTML document.
fn markdown_to_html_doc(title: &str, md_content: &str) -> String {
    // Simple markdown to HTML: use basic conversion
    // For proper rendering, we convert common patterns
    let mut html_body = String::new();
    let mut in_code_block = false;

    for line in md_content.lines() {
        if line.starts_with("```") {
            if in_code_block {
                html_body.push_str("</code></pre>\n");
                in_code_block = false;
            } else {
                html_body.push_str("<pre><code>");
                in_code_block = true;
            }
            continue;
        }
        if in_code_block {
            html_body.push_str(&line.replace('<', "&lt;").replace('>', "&gt;"));
            html_body.push('\n');
            continue;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            html_body.push_str("<br>\n");
        } else if trimmed.starts_with("# ") {
            html_body.push_str(&format!("<h1>{}</h1>\n", &trimmed[2..]));
        } else if trimmed.starts_with("## ") {
            html_body.push_str(&format!("<h2>{}</h2>\n", &trimmed[3..]));
        } else if trimmed.starts_with("### ") {
            html_body.push_str(&format!("<h3>{}</h3>\n", &trimmed[4..]));
        } else if trimmed.starts_with("- [ ] ") {
            html_body.push_str(&format!("<p><input type=\"checkbox\" disabled> {}</p>\n", &trimmed[6..]));
        } else if trimmed.starts_with("- [x] ") {
            html_body.push_str(&format!("<p><input type=\"checkbox\" checked disabled> {}</p>\n", &trimmed[6..]));
        } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            html_body.push_str(&format!("<li>{}</li>\n", &trimmed[2..]));
        } else if trimmed.starts_with("> ") {
            html_body.push_str(&format!("<blockquote>{}</blockquote>\n", &trimmed[2..]));
        } else if trimmed == "---" {
            html_body.push_str("<hr>\n");
        } else {
            html_body.push_str(&format!("<p>{}</p>\n", trimmed));
        }
    }
    if in_code_block {
        html_body.push_str("</code></pre>\n");
    }

    // Apply inline formatting: bold, italic, code, links
    let html_body = html_body
        .replace("**", "<strong>") // Simple toggle (imperfect but functional)
        .replace("__", "<strong>");

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>{title}</title>
<style>
  body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; max-width: 720px; margin: 2em auto; padding: 0 1em; line-height: 1.6; color: #333; }}
  h1, h2, h3 {{ margin-top: 1.5em; }}
  pre {{ background: #f5f5f5; padding: 1em; border-radius: 6px; overflow-x: auto; }}
  code {{ background: #f5f5f5; padding: 0.2em 0.4em; border-radius: 3px; font-size: 0.9em; }}
  pre code {{ background: none; padding: 0; }}
  blockquote {{ border-left: 3px solid #ddd; margin: 1em 0; padding-left: 1em; color: #666; }}
  hr {{ border: none; border-top: 1px solid #eee; margin: 2em 0; }}
  li {{ margin: 0.25em 0; }}
  img {{ max-width: 100%; }}
</style>
</head>
<body>
{html_body}
</body>
</html>"#,
        title = title.replace('<', "&lt;").replace('>', "&gt;"),
        html_body = html_body
    )
}

#[tauri::command]
async fn export_note_markdown(id: String, dest: String, include_frontmatter: bool, state: State<'_, AppState>) -> Result<(), String> {
    let note = read_note_impl(id, &state).await?;
    let content = if include_frontmatter {
        note.content.clone()
    } else {
        strip_frontmatter(&note.content)
    };
    std::fs::write(&dest, content).map_err(|e| format!("Failed to write file: {}", e))
}

#[tauri::command]
async fn export_note_html(id: String, dest: String, state: State<'_, AppState>) -> Result<(), String> {
    let note = read_note_impl(id, &state).await?;
    let clean = strip_frontmatter(&note.content);
    let title = extract_title(&note.content);
    let html = markdown_to_html_doc(&title, &clean);
    std::fs::write(&dest, html).map_err(|e| format!("Failed to write file: {}", e))
}

#[tauri::command]
async fn export_all_zip(dest: String, state: State<'_, AppState>) -> Result<usize, String> {
    let notes_folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone().ok_or("Notes folder not set")?
    };
    let base = PathBuf::from(&notes_folder);
    let files = walk_md_files_sync(&base, &base)?;

    let zip_file = std::fs::File::create(&dest).map_err(|e| format!("Failed to create zip: {}", e))?;
    let mut zip = zip::ZipWriter::new(zip_file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let mut count = 0;
    for file_path in &files {
        if let Ok(relative) = file_path.strip_prefix(&base) {
            let name = relative.to_string_lossy().replace('\\', "/");
            let content = std::fs::read_to_string(file_path).unwrap_or_default();
            zip.start_file(&name, options).map_err(|e| format!("Zip error: {}", e))?;
            use std::io::Write;
            zip.write_all(content.as_bytes()).map_err(|e| format!("Zip write error: {}", e))?;
            count += 1;
        }
    }
    zip.finish().map_err(|e| format!("Zip finish error: {}", e))?;
    Ok(count)
}

#[tauri::command]
async fn import_notes(paths: Vec<String>, state: State<'_, AppState>) -> Result<usize, String> {
    let notes_folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone().ok_or("Notes folder not set")?
    };
    let dest_dir = PathBuf::from(&notes_folder);
    let mut count = 0;

    for path_str in &paths {
        let src = PathBuf::from(path_str);
        let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("");

        match ext {
            "md" | "txt" => {
                // Copy directly, renaming .txt to .md
                let filename = src.file_stem().unwrap_or_default();
                let dest_name = format!("{}.md", filename.to_string_lossy());
                let dest_path = dest_dir.join(&dest_name);
                // Avoid overwriting - add suffix if exists
                let dest_path = unique_path(dest_path);
                std::fs::copy(&src, &dest_path).map_err(|e| format!("Copy failed: {}", e))?;
                count += 1;
            }
            "html" | "htm" => {
                // Basic HTML to markdown conversion
                let html_content = std::fs::read_to_string(&src).map_err(|e| format!("Read failed: {}", e))?;
                let md_content = html_to_markdown(&html_content);
                let filename = src.file_stem().unwrap_or_default();
                let dest_name = format!("{}.md", filename.to_string_lossy());
                let dest_path = dest_dir.join(&dest_name);
                let dest_path = unique_path(dest_path);
                std::fs::write(&dest_path, md_content).map_err(|e| format!("Write failed: {}", e))?;
                count += 1;
            }
            _ => {} // Skip unsupported formats
        }
    }
    Ok(count)
}

#[tauri::command]
async fn import_zip(path: String, state: State<'_, AppState>) -> Result<usize, String> {
    let notes_folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone().ok_or("Notes folder not set")?
    };
    let dest_dir = PathBuf::from(&notes_folder);

    let file = std::fs::File::open(&path).map_err(|e| format!("Failed to open zip: {}", e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Invalid zip: {}", e))?;
    let mut count = 0;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| format!("Zip entry error: {}", e))?;
        let name = entry.name().to_string();

        // Skip directories and non-markdown files, skip __MACOSX and hidden files
        if entry.is_dir() || name.starts_with("__MACOSX") || name.contains("/.")  {
            continue;
        }

        let ext = name.rsplit('.').next().unwrap_or("");
        if ext != "md" && ext != "txt" && ext != "html" && ext != "htm" {
            continue;
        }

        // Preserve folder structure from zip
        let relative = PathBuf::from(&name);
        let dest_path = dest_dir.join(&relative);

        // Create parent dirs
        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("Mkdir failed: {}", e))?;
        }

        let mut content = String::new();
        use std::io::Read;
        entry.read_to_string(&mut content).map_err(|e| format!("Read zip entry failed: {}", e))?;

        // Convert HTML to markdown if needed
        if ext == "html" || ext == "htm" {
            let md = html_to_markdown(&content);
            let md_path = dest_path.with_extension("md");
            let md_path = unique_path(md_path);
            std::fs::write(&md_path, md).map_err(|e| format!("Write failed: {}", e))?;
        } else {
            let dest_path = unique_path(dest_path);
            std::fs::write(&dest_path, content).map_err(|e| format!("Write failed: {}", e))?;
        }
        count += 1;
    }
    Ok(count)
}

/// Generate a unique file path by appending (1), (2) etc. if file exists.
fn unique_path(path: PathBuf) -> PathBuf {
    if !path.exists() {
        return path;
    }
    let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
    let ext = path.extension().unwrap_or_default().to_string_lossy().to_string();
    let parent = path.parent().unwrap_or(&path);
    for i in 1..1000 {
        let new_name = if ext.is_empty() {
            format!("{} ({})", stem, i)
        } else {
            format!("{} ({}).{}", stem, i, ext)
        };
        let new_path = parent.join(new_name);
        if !new_path.exists() {
            return new_path;
        }
    }
    path
}

/// Basic HTML to markdown converter using scraper.
fn html_to_markdown(html: &str) -> String {
    // Use scraper to extract text content with basic formatting
    let document = scraper::Html::parse_document(html);

    // Try to get body content, fall back to full document
    let body_sel = scraper::Selector::parse("body").unwrap();
    let root = document.select(&body_sel).next();

    let mut md = String::new();
    if let Some(body) = root {
        convert_node_to_md(&body, &mut md);
    } else {
        // Just extract all text
        md = document.root_element().text().collect::<Vec<_>>().join(" ");
    }
    md.trim().to_string()
}

fn convert_node_to_md(element: &scraper::ElementRef, out: &mut String) {
    for child in element.children() {
        match child.value() {
            scraper::node::Node::Text(text) => {
                let t = text.text.trim();
                if !t.is_empty() {
                    out.push_str(t);
                }
            }
            scraper::node::Node::Element(el) => {
                if let Some(child_ref) = scraper::ElementRef::wrap(child) {
                    match el.name() {
                        "h1" => {
                            out.push_str("\n# ");
                            convert_node_to_md(&child_ref, out);
                            out.push_str("\n\n");
                        }
                        "h2" => {
                            out.push_str("\n## ");
                            convert_node_to_md(&child_ref, out);
                            out.push_str("\n\n");
                        }
                        "h3" => {
                            out.push_str("\n### ");
                            convert_node_to_md(&child_ref, out);
                            out.push_str("\n\n");
                        }
                        "p" | "div" => {
                            convert_node_to_md(&child_ref, out);
                            out.push_str("\n\n");
                        }
                        "br" => out.push('\n'),
                        "strong" | "b" => {
                            out.push_str("**");
                            convert_node_to_md(&child_ref, out);
                            out.push_str("**");
                        }
                        "em" | "i" => {
                            out.push('*');
                            convert_node_to_md(&child_ref, out);
                            out.push('*');
                        }
                        "code" => {
                            out.push('`');
                            convert_node_to_md(&child_ref, out);
                            out.push('`');
                        }
                        "pre" => {
                            out.push_str("\n```\n");
                            convert_node_to_md(&child_ref, out);
                            out.push_str("\n```\n\n");
                        }
                        "a" => {
                            let href = el.attr("href").unwrap_or("#");
                            out.push('[');
                            convert_node_to_md(&child_ref, out);
                            out.push_str("](");
                            out.push_str(href);
                            out.push(')');
                        }
                        "li" => {
                            out.push_str("- ");
                            convert_node_to_md(&child_ref, out);
                            out.push('\n');
                        }
                        "blockquote" => {
                            out.push_str("> ");
                            convert_node_to_md(&child_ref, out);
                            out.push('\n');
                        }
                        "hr" => out.push_str("\n---\n\n"),
                        "img" => {
                            let src = el.attr("src").unwrap_or("");
                            let alt = el.attr("alt").unwrap_or("image");
                            out.push_str(&format!("![{}]({})", alt, src));
                        }
                        _ => convert_node_to_md(&child_ref, out),
                    }
                }
            }
            _ => {}
        }
    }
}

// ---- Trash Bin Commands ----

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrashMeta {
    original_path: String,
    deleted_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrashedNote {
    id: String,
    title: String,
    original_path: String,
    deleted_at: String,
    preview: String,
}

fn get_trash_dir(notes_folder: &str) -> PathBuf {
    PathBuf::from(notes_folder).join(".scratch").join("trash")
}

fn ensure_trash_dir(notes_folder: &str) -> Result<PathBuf, String> {
    let trash = get_trash_dir(notes_folder);
    std::fs::create_dir_all(&trash).map_err(|e| format!("Failed to create trash dir: {}", e))?;
    Ok(trash)
}

/// Auto-purge trashed notes older than 30 days.
fn purge_old_trash(notes_folder: &str) {
    let trash_dir = get_trash_dir(notes_folder);
    if !trash_dir.exists() {
        return;
    }
    let now = chrono::Utc::now();
    let cutoff = chrono::Duration::days(30);

    if let Ok(entries) = std::fs::read_dir(&trash_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") && path.to_string_lossy().ends_with(".meta.json") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(meta) = serde_json::from_str::<TrashMeta>(&content) {
                        if let Ok(deleted) = chrono::DateTime::parse_from_rfc3339(&meta.deleted_at) {
                            if now.signed_duration_since(deleted) > cutoff {
                                // Remove note file and meta
                                // note file path derived from meta path
                                // meta file name: foo.meta.json -> remove "foo.meta.json"
                                let _ = std::fs::remove_file(&path);
                                // The note file: strip ".meta.json" -> stem = "foo.meta", but we stored as "foo.md"
                                let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
                                let note_stem = stem.strip_suffix(".meta").unwrap_or(&stem);
                                let note_path = trash_dir.join(format!("{}.md", note_stem));
                                let _ = std::fs::remove_file(&note_path);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[tauri::command]
async fn trash_note(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let notes_folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone().ok_or("Notes folder not set")?
    };

    let file_path = resolve_note_path(&notes_folder, &id)?;
    if !file_path.exists() {
        return Err("Note not found".to_string());
    }

    let trash_dir = ensure_trash_dir(&notes_folder)?;

    // Compute a safe filename for trash (flatten path separators)
    let safe_name = id.replace('/', "__");
    let trash_file = trash_dir.join(format!("{}.md", &safe_name));
    let meta_file = trash_dir.join(format!("{}.meta.json", &safe_name));

    // Move file to trash
    std::fs::rename(&file_path, &trash_file)
        .or_else(|_| {
            // rename can fail across filesystems, fall back to copy + delete
            std::fs::copy(&file_path, &trash_file)?;
            std::fs::remove_file(&file_path)
        })
        .map_err(|e| format!("Failed to move to trash: {}", e))?;

    // Write metadata
    let meta = TrashMeta {
        original_path: format!("{}.md", id),
        deleted_at: chrono::Utc::now().to_rfc3339(),
    };
    let meta_json = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
    std::fs::write(&meta_file, meta_json).map_err(|e| format!("Failed to write trash meta: {}", e))?;

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

    // Remove backlinks
    {
        let mut bl_index = state.backlinks_index.write().expect("backlinks write lock");
        remove_backlinks_for_note(&mut bl_index, &id);
        let _ = save_backlinks_index(&notes_folder, &bl_index);
    }

    Ok(())
}

#[tauri::command]
async fn list_trash(state: State<'_, AppState>) -> Result<Vec<TrashedNote>, String> {
    let notes_folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone().ok_or("Notes folder not set")?
    };

    let trash_dir = get_trash_dir(&notes_folder);
    if !trash_dir.exists() {
        return Ok(Vec::new());
    }

    let mut result = Vec::new();
    for entry in std::fs::read_dir(&trash_dir).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        if !path.to_string_lossy().ends_with(".meta.json") {
            continue;
        }

        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let meta: TrashMeta = match serde_json::from_str(&content) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
        let note_stem = stem.strip_suffix(".meta").unwrap_or(&stem);
        let note_path = trash_dir.join(format!("{}.md", note_stem));

        if !note_path.exists() {
            continue;
        }

        let note_content = std::fs::read_to_string(&note_path).unwrap_or_default();
        let title = extract_title(&note_content);
        let preview = generate_preview(&note_content);

        result.push(TrashedNote {
            id: note_stem.to_string(),
            title,
            original_path: meta.original_path,
            deleted_at: meta.deleted_at,
            preview,
        });
    }

    // Sort by deletion date, newest first
    result.sort_by(|a, b| b.deleted_at.cmp(&a.deleted_at));
    Ok(result)
}

#[tauri::command]
async fn restore_note(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let notes_folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone().ok_or("Notes folder not set")?
    };

    let trash_dir = get_trash_dir(&notes_folder);
    let note_path = trash_dir.join(format!("{}.md", &id));
    let meta_path = trash_dir.join(format!("{}.meta.json", &id));

    if !note_path.exists() {
        return Err("Trashed note not found".to_string());
    }

    // Read meta to get original path
    let meta_content = std::fs::read_to_string(&meta_path).map_err(|e| e.to_string())?;
    let meta: TrashMeta = serde_json::from_str(&meta_content).map_err(|e| e.to_string())?;

    let dest = PathBuf::from(&notes_folder).join(&meta.original_path);
    let dest = unique_path(dest);

    // Ensure parent dir exists
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create dir: {}", e))?;
    }

    // Move back
    std::fs::rename(&note_path, &dest)
        .or_else(|_| {
            std::fs::copy(&note_path, &dest)?;
            std::fs::remove_file(&note_path)
        })
        .map_err(|e| format!("Failed to restore: {}", e))?;

    // Remove meta
    let _ = std::fs::remove_file(&meta_path);

    // Re-index the note
    let base = PathBuf::from(&notes_folder);
    if let Some(note_id) = path_to_note_id(&base, &dest) {
        let content = std::fs::read_to_string(&dest).unwrap_or_default();
        let index = state.search_index.lock().expect("search index mutex");
        if let Some(ref search_index) = *index {
            let title = extract_title(&content);
            let modified = dest.metadata()
                .and_then(|m| m.modified())
                .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64)
                .unwrap_or(0);
            let _ = search_index.index_note(&note_id, &title, &content, modified);
        }
    }

    Ok(())
}

#[tauri::command]
async fn delete_permanently(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let notes_folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone().ok_or("Notes folder not set")?
    };

    let trash_dir = get_trash_dir(&notes_folder);
    let note_path = trash_dir.join(format!("{}.md", &id));
    let meta_path = trash_dir.join(format!("{}.meta.json", &id));

    if note_path.exists() {
        std::fs::remove_file(&note_path).map_err(|e| format!("Failed to delete: {}", e))?;
    }
    if meta_path.exists() {
        std::fs::remove_file(&meta_path).map_err(|e| format!("Failed to delete meta: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
async fn empty_trash(state: State<'_, AppState>) -> Result<usize, String> {
    let notes_folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone().ok_or("Notes folder not set")?
    };

    let trash_dir = get_trash_dir(&notes_folder);
    if !trash_dir.exists() {
        return Ok(0);
    }

    let mut count = 0;
    for entry in std::fs::read_dir(&trash_dir).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        if path.is_file() {
            let _ = std::fs::remove_file(&path);
            if path.extension().is_some_and(|e| e == "md") {
                count += 1;
            }
        }
    }
    Ok(count)
}

// ---- Version History Commands ----

fn get_history_dir(notes_folder: &str) -> PathBuf {
    PathBuf::from(notes_folder).join(".scratch").join("history")
}

fn get_note_history_dir(notes_folder: &str, note_id: &str) -> PathBuf {
    let safe_name = note_id.replace('/', "__");
    get_history_dir(notes_folder).join(safe_name)
}

/// Create a snapshot of a note's content for version history.
/// Debounced: skips if last snapshot was less than 5 minutes ago.
fn maybe_snapshot_note(notes_folder: &str, note_id: &str, content: &str) {
    let history_dir = get_note_history_dir(notes_folder, note_id);

    // Check if last snapshot is < 5 minutes old
    if history_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&history_dir) {
            let mut latest: Option<std::time::SystemTime> = None;
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata() {
                    if let Ok(modified) = meta.modified() {
                        latest = Some(latest.map_or(modified, |l: std::time::SystemTime| l.max(modified)));
                    }
                }
            }
            if let Some(last) = latest {
                if let Ok(elapsed) = last.elapsed() {
                    if elapsed < Duration::from_secs(300) {
                        return; // Less than 5 minutes since last snapshot
                    }
                }
            }
        }
    }

    // Create snapshot
    let _ = std::fs::create_dir_all(&history_dir);
    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H-%M-%SZ").to_string();
    let snapshot_path = history_dir.join(format!("{}.md", timestamp));
    let _ = std::fs::write(&snapshot_path, content);

    // Enforce 50-version limit
    purge_note_history(&history_dir, 50);
}

fn purge_note_history(history_dir: &PathBuf, max_versions: usize) {
    if let Ok(entries) = std::fs::read_dir(history_dir) {
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
            .map(|e| e.path())
            .collect();
        // Sort by name (timestamp-based, so alphabetical = chronological)
        files.sort();
        // Remove oldest if over limit
        while files.len() > max_versions {
            if let Some(oldest) = files.first() {
                let _ = std::fs::remove_file(oldest);
                files.remove(0);
            }
        }
    }
}

/// Purge version history to stay under 100MB total.
fn purge_history_size(notes_folder: &str) {
    let history_dir = get_history_dir(notes_folder);
    if !history_dir.exists() {
        return;
    }

    let max_bytes: u64 = 100 * 1024 * 1024; // 100MB

    // Collect all history files with their sizes and modified times
    let mut all_files: Vec<(PathBuf, u64, std::time::SystemTime)> = Vec::new();
    if let Ok(note_dirs) = std::fs::read_dir(&history_dir) {
        for dir_entry in note_dirs.flatten() {
            if dir_entry.path().is_dir() {
                if let Ok(files) = std::fs::read_dir(dir_entry.path()) {
                    for file in files.flatten() {
                        let path = file.path();
                        if path.extension().is_some_and(|e| e == "md") {
                            if let Ok(meta) = path.metadata() {
                                let modified = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
                                all_files.push((path, meta.len(), modified));
                            }
                        }
                    }
                }
            }
        }
    }

    let total: u64 = all_files.iter().map(|(_, size, _)| size).sum();
    if total <= max_bytes {
        return;
    }

    // Sort by modified time, oldest first
    all_files.sort_by_key(|(_, _, t)| *t);

    let mut current_total = total;
    for (path, size, _) in &all_files {
        if current_total <= max_bytes {
            break;
        }
        let _ = std::fs::remove_file(path);
        current_total -= size;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VersionEntry {
    id: String,          // timestamp filename (without .md)
    timestamp: String,   // human-readable ISO timestamp
    size: u64,           // file size in bytes
}

#[tauri::command]
async fn list_versions(note_id: String, state: State<'_, AppState>) -> Result<Vec<VersionEntry>, String> {
    let notes_folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone().ok_or("Notes folder not set")?
    };

    let history_dir = get_note_history_dir(&notes_folder, &note_id);
    if !history_dir.exists() {
        return Ok(Vec::new());
    }

    let mut versions = Vec::new();
    for entry in std::fs::read_dir(&history_dir).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "md") {
            let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
            let size = path.metadata().map(|m| m.len()).unwrap_or(0);
            // Convert filename timestamp (2026-02-14T04-30-00Z) to ISO (2026-02-14T04:30:00Z)
            let parts: Vec<&str> = stem.splitn(2, 'T').collect();
            let timestamp = if parts.len() == 2 {
                format!("{}T{}", parts[0], parts[1].replace('-', ":"))
            } else {
                stem.clone()
            };

            versions.push(VersionEntry { id: stem, timestamp, size });
        }
    }

    // Sort newest first
    versions.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(versions)
}

#[tauri::command]
async fn read_version(note_id: String, version_id: String, state: State<'_, AppState>) -> Result<String, String> {
    let notes_folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone().ok_or("Notes folder not set")?
    };

    let history_dir = get_note_history_dir(&notes_folder, &note_id);
    let version_path = history_dir.join(format!("{}.md", version_id));

    if !version_path.exists() {
        return Err("Version not found".to_string());
    }

    std::fs::read_to_string(&version_path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn restore_version(note_id: String, version_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let notes_folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone().ok_or("Notes folder not set")?
    };

    // Read the version content
    let history_dir = get_note_history_dir(&notes_folder, &note_id);
    let version_path = history_dir.join(format!("{}.md", version_id));
    let version_content = std::fs::read_to_string(&version_path).map_err(|e| e.to_string())?;

    // Snapshot current content before restoring
    let current_path = resolve_note_path(&notes_folder, &note_id)?;
    if current_path.exists() {
        let current_content = std::fs::read_to_string(&current_path).unwrap_or_default();
        // Force snapshot regardless of time
        let _ = std::fs::create_dir_all(&history_dir);
        let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H-%M-%SZ").to_string();
        let snapshot_path = history_dir.join(format!("{}.md", timestamp));
        let _ = std::fs::write(&snapshot_path, &current_content);
    }

    // Write version content to the note file
    std::fs::write(&current_path, version_content).map_err(|e| format!("Failed to restore: {}", e))
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

/// Serializable row template info for the frontend.
#[derive(Serialize, Deserialize)]
struct RowTemplateInfo {
    id: String,
    name: String,
    title: Option<String>,
    fields: std::collections::HashMap<String, serde_json::Value>,
    body: Option<String>,
}

#[tauri::command]
fn db_list_templates(
    db_id: String,
    state: State<AppState>,
) -> Result<Vec<RowTemplateInfo>, String> {
    let folder = get_notes_folder_path(&state)?;
    let templates = database::list_row_templates(&folder, &db_id)?;
    Ok(templates.into_iter().map(|(id, t)| RowTemplateInfo {
        id,
        name: t.name,
        title: t.title,
        fields: t.fields,
        body: t.body,
    }).collect())
}

#[tauri::command]
fn db_create_row_from_template(
    db_id: String,
    template_name: String,
    variables: std::collections::HashMap<String, String>,
    state: State<AppState>,
) -> Result<database::DatabaseRow, String> {
    let folder = get_notes_folder_path(&state)?;
    database::create_row_from_template(&folder, &db_id, &template_name, variables)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_deep_link::init())
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

            // Auto-purge old trash and oversized version history on startup
            if let Some(ref folder) = app_config.notes_folder {
                purge_old_trash(folder);
                purge_history_size(folder);
            }

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

            // Create main window programmatically so we can attach on_navigation / on_new_window
            let app_handle = app.handle().clone();
            let mut builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                .title("Scratch")
                .inner_size(1080.0, 720.0)
                .min_inner_size(600.0, 400.0)
                .resizable(true)
                .decorations(true)
                .title_bar_style(tauri::TitleBarStyle::Overlay)
                .hidden_title(true)
                .on_navigation(|url| {
                    // Allow Tauri internal URLs and localhost dev server
                    let host = url.host_str().unwrap_or("");
                    host == "localhost" || host == "tauri.localhost" || host == "ipc.localhost" || host == "asset.localhost" || url.scheme() == "tauri"
                })
                .on_new_window(move |url, _features| {
                    // Intercept window.open calls (e.g. BlockNote "Open link" button)
                    let href = url.to_string();
                    let host = url.host_str().unwrap_or("").to_ascii_lowercase();
                    let is_local_host = host == "localhost" || host.ends_with(".localhost");
                    let is_internal_webview = is_local_host || url.scheme() == "tauri";

                    // Anchor links — ignore
                    if href.starts_with("#") {
                        return NewWindowResponse::Deny;
                    }

                    // Local markdown links should navigate within the app, not open externally.
                    if href.contains(".md") && is_internal_webview {
                        let _ = app_handle.emit("link-navigate", href);
                        return NewWindowResponse::Deny;
                    }

                    // Other localhost/tauri links are internal; suppress browser opens.
                    if is_internal_webview {
                        return NewWindowResponse::Deny;
                    }

                    // External URLs — open in default browser via opener plugin
                    if href.starts_with("http://") || href.starts_with("https://") || href.starts_with("mailto:") {
                        let _ = tauri_plugin_opener::open_url(&href, None::<&str>);
                    }

                    NewWindowResponse::Deny
                });

            #[cfg(target_os = "macos")]
            {
                builder = builder.traffic_light_position(tauri::LogicalPosition::new(16.0, 24.0));
            }

            builder.build()?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_notes_folder,
            set_notes_folder,
            list_notes,
            list_folders,
            list_notes_in_folder,
            create_folder,
            rename_folder,
            delete_folder,
            move_note,
            read_note,
            save_note,
            delete_note,
            create_note,
            create_note_in_folder,
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
            fetch_url_metadata,
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
            db_list_templates,
            db_create_row_from_template,
            list_templates,
            read_template,
            create_note_from_template,
            export_note_markdown,
            export_note_html,
            export_all_zip,
            import_notes,
            import_zip,
            trash_note,
            list_trash,
            restore_note,
            delete_permanently,
            empty_trash,
            list_versions,
            read_version,
            restore_version,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

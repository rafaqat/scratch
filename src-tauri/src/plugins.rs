use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::AppState;

// ── Plugin manifest types ──────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginManifest {
    /// Unique plugin name (also used as filename slug)
    pub name: String,
    /// Semantic version
    #[serde(default = "default_version")]
    pub version: String,
    /// Human-readable description
    #[serde(default)]
    pub description: String,
    /// Whether this plugin is enabled (defaults to true)
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// MCP tools provided by this plugin
    #[serde(default)]
    pub tools: Vec<PluginTool>,
    /// Webhook handlers provided by this plugin
    #[serde(default)]
    pub webhooks: Vec<PluginWebhook>,
    /// Permissions requested by this plugin
    #[serde(default)]
    pub permissions: Vec<String>,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginTool {
    /// Tool name (will be prefixed with plugin name: `{plugin}_{name}`)
    pub name: String,
    /// Tool description for MCP
    pub description: String,
    /// Parameter definitions
    #[serde(default)]
    pub params: HashMap<String, ParamDef>,
    /// Action to execute when the tool is called
    pub action: ToolAction,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ParamDef {
    /// Parameter type: "string", "number", "boolean"
    #[serde(rename = "type")]
    pub param_type: String,
    /// Whether this parameter is required
    #[serde(default)]
    pub required: bool,
    /// Default value if not provided
    #[serde(default)]
    pub default: Option<Value>,
    /// Description of the parameter
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolAction {
    /// Action type: "shell", "http", or "note_op"
    #[serde(rename = "type")]
    pub action_type: String,

    // Shell action fields
    /// Shell command to execute (supports {{param}} template substitution)
    #[serde(default)]
    pub command: Option<String>,

    // HTTP action fields
    /// HTTP method (GET, POST, etc.)
    #[serde(default)]
    pub method: Option<String>,
    /// URL to call (supports {{param}} template substitution)
    #[serde(default)]
    pub url: Option<String>,
    /// HTTP headers
    #[serde(default)]
    pub headers: Option<HashMap<String, String>>,
    /// Request body template
    #[serde(default)]
    pub body: Option<String>,

    // Note operation fields
    /// Note operation: "create", "read", "update", "append", "search", "list"
    #[serde(rename = "op", default)]
    pub note_op: Option<String>,
    /// Folder for note operations
    #[serde(default)]
    pub folder: Option<String>,
    /// Template for note content
    #[serde(default)]
    pub template: Option<String>,

    // Output handling
    /// What to do with the output: "text" (return raw), "create_notes", "json"
    #[serde(default = "default_output")]
    pub output: String,
}

fn default_output() -> String {
    "text".to_string()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginWebhook {
    /// URL path segment for the webhook
    pub path: String,
    /// Events this webhook handles
    #[serde(default)]
    pub events: Vec<String>,
    /// Secret for authentication
    #[serde(default)]
    pub secret: Option<String>,
}

// ── Validation ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Validate a plugin manifest against the schema and permission rules.
pub fn validate_manifest(manifest: &PluginManifest) -> ValidationResult {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Required: name
    if manifest.name.is_empty() {
        errors.push("Plugin name is required".to_string());
    } else if !manifest.name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        errors.push("Plugin name must be alphanumeric with hyphens/underscores only".to_string());
    }

    // Version format check
    if !manifest.version.is_empty() {
        let parts: Vec<&str> = manifest.version.split('.').collect();
        if parts.len() != 3 || !parts.iter().all(|p| p.parse::<u32>().is_ok()) {
            warnings.push(format!(
                "Version '{}' is not valid semver (expected X.Y.Z)",
                manifest.version
            ));
        }
    }

    // Validate tools
    for (i, tool) in manifest.tools.iter().enumerate() {
        if tool.name.is_empty() {
            errors.push(format!("Tool {} has no name", i));
        }
        if tool.description.is_empty() {
            warnings.push(format!("Tool '{}' has no description", tool.name));
        }

        // Validate action
        match tool.action.action_type.as_str() {
            "shell" => {
                if tool.action.command.is_none() {
                    errors.push(format!(
                        "Tool '{}': shell action requires a 'command' field",
                        tool.name
                    ));
                }
            }
            "http" => {
                if tool.action.url.is_none() {
                    errors.push(format!(
                        "Tool '{}': http action requires a 'url' field",
                        tool.name
                    ));
                }
                if tool.action.method.is_none() {
                    warnings.push(format!(
                        "Tool '{}': http action has no 'method', defaulting to GET",
                        tool.name
                    ));
                }
            }
            "note_op" => {
                if tool.action.note_op.is_none() {
                    errors.push(format!(
                        "Tool '{}': note_op action requires an 'op' field",
                        tool.name
                    ));
                } else {
                    let valid_ops = ["create", "read", "update", "append", "search", "list"];
                    let op = tool.action.note_op.as_ref().unwrap();
                    if !valid_ops.contains(&op.as_str()) {
                        errors.push(format!(
                            "Tool '{}': unknown note operation '{}'. Valid: {:?}",
                            tool.name, op, valid_ops
                        ));
                    }
                }
            }
            other => {
                errors.push(format!(
                    "Tool '{}': unknown action type '{}'. Valid: shell, http, note_op",
                    tool.name, other
                ));
            }
        }

        // Validate param types
        for (param_name, param_def) in &tool.params {
            let valid_types = ["string", "number", "boolean", "integer"];
            if !valid_types.contains(&param_def.param_type.as_str()) {
                warnings.push(format!(
                    "Tool '{}' param '{}': unknown type '{}'. Valid: {:?}",
                    tool.name, param_name, param_def.param_type, valid_types
                ));
            }
        }
    }

    // Permission validation
    let valid_permissions = [
        "notes:read",
        "notes:write",
        "notes:delete",
        "shell:execute",
        "http:request",
        "folders:read",
        "folders:write",
        "search",
    ];

    for perm in &manifest.permissions {
        if !valid_permissions.contains(&perm.as_str()) {
            warnings.push(format!(
                "Unknown permission '{}'. Valid: {:?}",
                perm, valid_permissions
            ));
        }
    }

    // Check that declared permissions match tool actions
    for tool in &manifest.tools {
        match tool.action.action_type.as_str() {
            "shell" => {
                if !manifest.permissions.contains(&"shell:execute".to_string()) {
                    warnings.push(format!(
                        "Tool '{}' uses shell action but plugin doesn't declare 'shell:execute' permission",
                        tool.name
                    ));
                }
            }
            "http" => {
                if !manifest.permissions.contains(&"http:request".to_string()) {
                    warnings.push(format!(
                        "Tool '{}' uses http action but plugin doesn't declare 'http:request' permission",
                        tool.name
                    ));
                }
            }
            "note_op" => {
                if let Some(ref op) = tool.action.note_op {
                    match op.as_str() {
                        "read" | "search" | "list" => {
                            if !manifest.permissions.contains(&"notes:read".to_string()) {
                                warnings.push(format!(
                                    "Tool '{}' uses note_op '{}' but plugin doesn't declare 'notes:read' permission",
                                    tool.name, op
                                ));
                            }
                        }
                        "create" | "update" | "append" => {
                            if !manifest.permissions.contains(&"notes:write".to_string()) {
                                warnings.push(format!(
                                    "Tool '{}' uses note_op '{}' but plugin doesn't declare 'notes:write' permission",
                                    tool.name, op
                                ));
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    ValidationResult {
        valid: errors.is_empty(),
        errors,
        warnings,
    }
}

// ── Plugin loading ─────────────────────────────────────────────────────────

/// Load all plugin manifests from `.scratch/plugins/` directory.
/// Returns only successfully parsed and validated manifests.
pub fn load_plugins(notes_folder: &str) -> Vec<PluginManifest> {
    let plugins_dir = PathBuf::from(notes_folder).join(".scratch").join("plugins");
    if !plugins_dir.exists() {
        return Vec::new();
    }

    let mut plugins = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&plugins_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "yaml" && ext != "yml" {
                continue;
            }

            if let Ok(content) = std::fs::read_to_string(&path) {
                // Try to parse as a plugin manifest (top-level has `name` field)
                match serde_yaml::from_str::<PluginManifest>(&content) {
                    Ok(manifest) => {
                        let validation = validate_manifest(&manifest);
                        if validation.valid {
                            plugins.push(manifest);
                        } else {
                            eprintln!(
                                "Warning: Plugin manifest {} has validation errors: {:?}",
                                path.display(),
                                validation.errors
                            );
                        }
                    }
                    Err(_) => {
                        // Not a plugin manifest (could be a webhook-only config)
                        // Silently skip — webhooks.rs handles these
                    }
                }
            }
        }
    }

    plugins
}

/// Load enabled plugins only.
pub fn load_enabled_plugins(notes_folder: &str) -> Vec<PluginManifest> {
    load_plugins(notes_folder)
        .into_iter()
        .filter(|p| p.enabled)
        .collect()
}

// ── MCP tool generation ────────────────────────────────────────────────────

/// Generate MCP tool definitions from loaded plugins.
/// Tool names are prefixed with `plugin_{plugin_name}_` to avoid conflicts.
pub fn generate_plugin_tools(plugins: &[PluginManifest]) -> Vec<Value> {
    let mut tools = Vec::new();

    for plugin in plugins {
        if !plugin.enabled {
            continue;
        }

        for tool in &plugin.tools {
            let full_name = format!("plugin_{}_{}", plugin.name.replace('-', "_"), tool.name);

            // Build input schema from param definitions
            let mut properties = serde_json::Map::new();
            let mut required = Vec::new();

            for (param_name, param_def) in &tool.params {
                let json_type = match param_def.param_type.as_str() {
                    "string" => "string",
                    "number" | "integer" => "number",
                    "boolean" => "boolean",
                    _ => "string",
                };

                let mut prop = json!({ "type": json_type });
                if let Some(ref desc) = param_def.description {
                    prop["description"] = json!(desc);
                }
                if let Some(ref default) = param_def.default {
                    prop["default"] = default.clone();
                }

                properties.insert(param_name.clone(), prop);

                if param_def.required {
                    required.push(json!(param_name));
                }
            }

            let tool_def = json!({
                "name": full_name,
                "description": format!("[{}] {}", plugin.name, tool.description),
                "inputSchema": {
                    "type": "object",
                    "properties": properties,
                    "required": required,
                }
            });

            tools.push(tool_def);
        }
    }

    tools
}

// ── Tool execution ─────────────────────────────────────────────────────────

/// Execute a plugin tool action with the given arguments.
pub async fn execute_plugin_tool(
    plugin: &PluginManifest,
    tool: &PluginTool,
    arguments: &Value,
    state: &AppState,
) -> Result<String, String> {
    match tool.action.action_type.as_str() {
        "shell" => execute_shell_action(tool, arguments).await,
        "http" => execute_http_action(tool, arguments).await,
        "note_op" => execute_note_op_action(plugin, tool, arguments, state).await,
        other => Err(format!("Unknown action type: {}", other)),
    }
}

/// Render a template string by replacing `{{key}}` with argument values.
fn render_args_template(template: &str, arguments: &Value) -> String {
    let mut result = String::with_capacity(template.len());
    let mut remaining = template;

    while let Some(start) = remaining.find("{{") {
        result.push_str(&remaining[..start]);
        let after_open = &remaining[start + 2..];

        if let Some(end) = after_open.find("}}") {
            let key = after_open[..end].trim();
            let value = match arguments.get(key) {
                Some(Value::String(s)) => s.clone(),
                Some(Value::Null) => String::new(),
                Some(v) => v.to_string(),
                None => String::new(),
            };
            result.push_str(&value);
            remaining = &after_open[end + 2..];
        } else {
            result.push_str("{{");
            remaining = after_open;
        }
    }
    result.push_str(remaining);
    result
}

async fn execute_shell_action(tool: &PluginTool, arguments: &Value) -> Result<String, String> {
    let command_template = tool
        .action
        .command
        .as_ref()
        .ok_or("Shell action missing command")?;

    let command = render_args_template(command_template, arguments);

    // Execute shell command
    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .await
        .map_err(|e| format!("Failed to execute command: {}", e))?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Err(format!(
            "Command failed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            stderr
        ))
    }
}

async fn execute_http_action(tool: &PluginTool, arguments: &Value) -> Result<String, String> {
    let url_template = tool
        .action
        .url
        .as_ref()
        .ok_or("HTTP action missing url")?;

    let url = render_args_template(url_template, arguments);
    let method = tool
        .action
        .method
        .as_deref()
        .unwrap_or("GET")
        .to_uppercase();

    // Build a simple HTTP request using tokio TCP (no reqwest dependency)
    // Parse URL to extract host, port, path
    let parsed_url: url::Url = url.parse().map_err(|e| format!("Invalid URL: {}", e))?;
    let host = parsed_url
        .host_str()
        .ok_or("URL has no host")?
        .to_string();
    let port = parsed_url.port_or_known_default().unwrap_or(80);
    let path = if parsed_url.query().is_some() {
        format!("{}?{}", parsed_url.path(), parsed_url.query().unwrap())
    } else {
        parsed_url.path().to_string()
    };

    // Render body if present
    let body = tool
        .action
        .body
        .as_ref()
        .map(|b| render_args_template(b, arguments));

    // Build raw HTTP request
    let content_length = body.as_ref().map(|b| b.len()).unwrap_or(0);
    let mut request = format!("{} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n", method, path, host);

    // Add custom headers
    if let Some(ref headers) = tool.action.headers {
        for (key, value) in headers {
            let rendered_value = render_args_template(value, arguments);
            request.push_str(&format!("{}: {}\r\n", key, rendered_value));
        }
    }

    if body.is_some() {
        request.push_str(&format!("Content-Length: {}\r\n", content_length));
        request.push_str("Content-Type: application/json\r\n");
    }

    request.push_str("\r\n");
    if let Some(ref b) = body {
        request.push_str(b);
    }

    // Connect and send
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let addr = format!("{}:{}", host, port);
    let mut stream = tokio::net::TcpStream::connect(&addr)
        .await
        .map_err(|e| format!("Failed to connect to {}: {}", addr, e))?;

    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|e| format!("Failed to send request: {}", e))?;

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    let response_str = String::from_utf8_lossy(&response).to_string();

    // Extract body from HTTP response (after blank line)
    if let Some(body_start) = response_str.find("\r\n\r\n") {
        Ok(response_str[body_start + 4..].to_string())
    } else {
        Ok(response_str)
    }
}

async fn execute_note_op_action(
    _plugin: &PluginManifest,
    tool: &PluginTool,
    arguments: &Value,
    state: &AppState,
) -> Result<String, String> {
    let op = tool
        .action
        .note_op
        .as_ref()
        .ok_or("note_op action missing 'op' field")?;

    match op.as_str() {
        "create" => {
            let content = if let Some(ref template) = tool.action.template {
                render_args_template(template, arguments)
            } else {
                arguments
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("# Untitled\n\n")
                    .to_string()
            };

            let folder = tool
                .action
                .folder
                .as_ref()
                .map(|f| render_args_template(f, arguments));

            // Ensure folder exists
            if let Some(ref subfolder) = folder {
                let notes_folder = {
                    let app_config = state.app_config.read().expect("app_config read lock");
                    app_config.notes_folder.clone().ok_or("Notes folder not set")?
                };
                let target_dir = PathBuf::from(&notes_folder).join(subfolder);
                if !target_dir.exists() {
                    tokio::fs::create_dir_all(&target_dir)
                        .await
                        .map_err(|e| format!("Failed to create folder: {}", e))?;
                }
            }

            let note = crate::save_note_impl(None, content, state).await?;

            // Move to subfolder if specified
            if let Some(ref subfolder) = folder {
                let moved = crate::move_note_impl(note.id.clone(), subfolder.clone(), state).await?;
                return serde_json::to_string_pretty(&moved).map_err(|e| e.to_string());
            }

            serde_json::to_string_pretty(&note).map_err(|e| e.to_string())
        }
        "read" => {
            let id = arguments
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or("note_op 'read' requires 'id' parameter")?
                .to_string();
            let note = crate::read_note_impl(id, state).await?;
            Ok(note.content)
        }
        "update" => {
            let id = arguments
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or("note_op 'update' requires 'id' parameter")?
                .to_string();

            let content = if let Some(ref template) = tool.action.template {
                render_args_template(template, arguments)
            } else {
                arguments
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or("note_op 'update' requires 'content' parameter or action template")?
                    .to_string()
            };

            let note = crate::save_note_impl(Some(id), content, state).await?;
            serde_json::to_string_pretty(&note).map_err(|e| e.to_string())
        }
        "append" => {
            let id = arguments
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or("note_op 'append' requires 'id' parameter")?
                .to_string();

            let content = if let Some(ref template) = tool.action.template {
                render_args_template(template, arguments)
            } else {
                arguments
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or("note_op 'append' requires 'content' parameter or action template")?
                    .to_string()
            };

            let existing = crate::read_note_impl(id.clone(), state).await?;
            let new_content = format!("{}\n{}", existing.content, content);
            let note = crate::save_note_impl(Some(id), new_content, state).await?;
            serde_json::to_string_pretty(&note).map_err(|e| e.to_string())
        }
        "search" => {
            let query = arguments
                .get("query")
                .and_then(|v| v.as_str())
                .ok_or("note_op 'search' requires 'query' parameter")?
                .to_string();
            let results = crate::search_notes_impl(query, state).await?;
            serde_json::to_string_pretty(&results).map_err(|e| e.to_string())
        }
        "list" => {
            let folder = arguments
                .get("folder")
                .and_then(|v| v.as_str())
                .or(tool.action.folder.as_deref());
            let notes = crate::list_notes_impl(state, folder, false).await?;
            serde_json::to_string_pretty(&notes).map_err(|e| e.to_string())
        }
        other => Err(format!("Unknown note operation: {}", other)),
    }
}

// ── Plugin state management ────────────────────────────────────────────────

/// Toggle a plugin's enabled state by rewriting its manifest file.
pub fn toggle_plugin(notes_folder: &str, plugin_name: &str, enabled: bool) -> Result<(), String> {
    let plugins_dir = PathBuf::from(notes_folder).join(".scratch").join("plugins");

    // Find the plugin file
    let plugin_path = find_plugin_file(&plugins_dir, plugin_name)?;

    // Read the content
    let content = std::fs::read_to_string(&plugin_path).map_err(|e| e.to_string())?;

    // Parse as YAML Value to preserve formatting as much as possible
    let mut manifest: PluginManifest =
        serde_yaml::from_str(&content).map_err(|e| format!("Failed to parse manifest: {}", e))?;

    manifest.enabled = enabled;

    // Write back
    let yaml = serde_yaml::to_string(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(&plugin_path, yaml).map_err(|e| e.to_string())?;

    Ok(())
}

fn find_plugin_file(plugins_dir: &PathBuf, plugin_name: &str) -> Result<PathBuf, String> {
    if !plugins_dir.exists() {
        return Err(format!(
            "Plugins directory does not exist: {}",
            plugins_dir.display()
        ));
    }

    // Try common extensions
    for ext in &["yaml", "yml"] {
        let path = plugins_dir.join(format!("{}.{}", plugin_name, ext));
        if path.exists() {
            return Ok(path);
        }
    }

    // Search all files for matching plugin name
    if let Ok(entries) = std::fs::read_dir(plugins_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "yaml" && ext != "yml" {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(manifest) = serde_yaml::from_str::<PluginManifest>(&content) {
                    if manifest.name == plugin_name {
                        return Ok(path);
                    }
                }
            }
        }
    }

    Err(format!("Plugin '{}' not found", plugin_name))
}

// ── Plugin info for frontend ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub enabled: bool,
    pub tool_count: usize,
    pub webhook_count: usize,
    pub permissions: Vec<String>,
    pub validation: ValidationResult,
}

/// Get summary info for all installed plugins (for settings UI).
pub fn get_plugin_info(notes_folder: &str) -> Vec<PluginInfo> {
    let plugins_dir = PathBuf::from(notes_folder).join(".scratch").join("plugins");
    if !plugins_dir.exists() {
        return Vec::new();
    }

    let mut infos = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&plugins_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "yaml" && ext != "yml" {
                continue;
            }

            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(manifest) = serde_yaml::from_str::<PluginManifest>(&content) {
                    let validation = validate_manifest(&manifest);
                    infos.push(PluginInfo {
                        name: manifest.name.clone(),
                        version: manifest.version.clone(),
                        description: manifest.description.clone(),
                        enabled: manifest.enabled,
                        tool_count: manifest.tools.len(),
                        webhook_count: manifest.webhooks.len(),
                        permissions: manifest.permissions.clone(),
                        validation,
                    });
                }
            }
        }
    }

    infos
}

// ── Dispatch plugin tools from MCP ─────────────────────────────────────────

/// Attempt to handle a tool call as a plugin tool. Returns None if the tool
/// name doesn't match any plugin tool (prefix `plugin_`).
pub async fn try_dispatch_plugin_tool(
    tool_name: &str,
    arguments: &Value,
    state: &AppState,
) -> Option<Result<String, String>> {
    if !tool_name.starts_with("plugin_") {
        return None;
    }

    let notes_folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone()?
    };

    let plugins = load_enabled_plugins(&notes_folder);

    for plugin in &plugins {
        let prefix = format!("plugin_{}_", plugin.name.replace('-', "_"));
        if let Some(short_name) = tool_name.strip_prefix(&prefix) {
            if let Some(tool) = plugin.tools.iter().find(|t| t.name == short_name) {
                return Some(execute_plugin_tool(plugin, tool, arguments, state).await);
            }
        }
    }

    Some(Err(format!("Plugin tool '{}' not found", tool_name)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_valid_manifest() {
        let manifest = PluginManifest {
            name: "test-plugin".to_string(),
            version: "1.0.0".to_string(),
            description: "A test plugin".to_string(),
            enabled: true,
            tools: vec![PluginTool {
                name: "test_tool".to_string(),
                description: "Does a thing".to_string(),
                params: HashMap::new(),
                action: ToolAction {
                    action_type: "shell".to_string(),
                    command: Some("echo hello".to_string()),
                    method: None,
                    url: None,
                    headers: None,
                    body: None,
                    note_op: None,
                    folder: None,
                    template: None,
                    output: "text".to_string(),
                },
            }],
            webhooks: vec![],
            permissions: vec!["shell:execute".to_string()],
        };

        let result = validate_manifest(&manifest);
        assert!(result.valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_validate_empty_name() {
        let manifest = PluginManifest {
            name: "".to_string(),
            version: "1.0.0".to_string(),
            description: "".to_string(),
            enabled: true,
            tools: vec![],
            webhooks: vec![],
            permissions: vec![],
        };

        let result = validate_manifest(&manifest);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("name is required")));
    }

    #[test]
    fn test_validate_missing_permission_warning() {
        let manifest = PluginManifest {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            description: "".to_string(),
            enabled: true,
            tools: vec![PluginTool {
                name: "t".to_string(),
                description: "d".to_string(),
                params: HashMap::new(),
                action: ToolAction {
                    action_type: "shell".to_string(),
                    command: Some("ls".to_string()),
                    method: None,
                    url: None,
                    headers: None,
                    body: None,
                    note_op: None,
                    folder: None,
                    template: None,
                    output: "text".to_string(),
                },
            }],
            webhooks: vec![],
            permissions: vec![], // Missing shell:execute
        };

        let result = validate_manifest(&manifest);
        assert!(result.valid); // Warnings don't invalidate
        assert!(result.warnings.iter().any(|w| w.contains("shell:execute")));
    }

    #[test]
    fn test_render_args_template() {
        let args = json!({ "repo": "user/project", "count": 10 });
        let template = "gh issue list --repo {{repo}} --limit {{count}}";
        let result = render_args_template(template, &args);
        assert_eq!(result, "gh issue list --repo user/project --limit 10");
    }

    #[test]
    fn test_generate_plugin_tools() {
        let plugins = vec![PluginManifest {
            name: "github-sync".to_string(),
            version: "1.0.0".to_string(),
            description: "GitHub sync".to_string(),
            enabled: true,
            tools: vec![PluginTool {
                name: "list_issues".to_string(),
                description: "List GitHub issues".to_string(),
                params: {
                    let mut m = HashMap::new();
                    m.insert(
                        "repo".to_string(),
                        ParamDef {
                            param_type: "string".to_string(),
                            required: true,
                            default: None,
                            description: Some("Repository name".to_string()),
                        },
                    );
                    m
                },
                action: ToolAction {
                    action_type: "shell".to_string(),
                    command: Some("gh issue list --repo {{repo}}".to_string()),
                    method: None,
                    url: None,
                    headers: None,
                    body: None,
                    note_op: None,
                    folder: None,
                    template: None,
                    output: "text".to_string(),
                },
            }],
            webhooks: vec![],
            permissions: vec!["shell:execute".to_string()],
        }];

        let tools = generate_plugin_tools(&plugins);
        assert_eq!(tools.len(), 1);
        assert_eq!(
            tools[0]["name"].as_str().unwrap(),
            "plugin_github_sync_list_issues"
        );
        assert!(tools[0]["description"]
            .as_str()
            .unwrap()
            .starts_with("[github-sync]"));
    }
}

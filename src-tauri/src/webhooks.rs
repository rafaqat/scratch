use axum::{
    body::Bytes,
    extract::{Path, State as AxumState},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::AppState;

// ── Webhook handler config (YAML) ──────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebhookConfig {
    pub webhook: WebhookDefinition,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebhookDefinition {
    /// URL path segment: POST /webhooks/{path}
    pub path: String,
    /// Shared secret for HMAC-SHA256 verification. Supports ${ENV_VAR} expansion.
    #[serde(default)]
    pub secret: Option<String>,
    /// Event→action mapping
    pub on: HashMap<String, WebhookAction>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebhookAction {
    /// Action type: "create_note" or "update_note"
    pub action: String,
    /// Subfolder to create the note in (created if missing)
    #[serde(default)]
    pub folder: Option<String>,
    /// Handlebars-style template for the note content
    pub template: String,
    /// Optional: note ID pattern for update_note (e.g., "{{payload.issue.number}}")
    #[serde(default)]
    pub note_id: Option<String>,
}

// ── Activity log entry ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookLogEntry {
    pub timestamp: String,
    pub plugin: String,
    pub event: String,
    pub action: String,
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note_id: Option<String>,
}

// ── Template rendering ─────────────────────────────────────────────────────

/// Render a template string by replacing `{{path.to.field}}` with values from
/// the JSON payload. Supports nested dotted paths like `{{payload.issue.title}}`.
fn render_template(template: &str, context: &Value) -> String {
    let mut result = String::with_capacity(template.len());
    let mut remaining = template;

    while let Some(start) = remaining.find("{{") {
        result.push_str(&remaining[..start]);
        let after_open = &remaining[start + 2..];

        if let Some(end) = after_open.find("}}") {
            let key = after_open[..end].trim();
            let value = resolve_json_path(context, key);
            result.push_str(&value);
            remaining = &after_open[end + 2..];
        } else {
            // No closing }}, push the {{ and continue
            result.push_str("{{");
            remaining = after_open;
        }
    }
    result.push_str(remaining);
    result
}

/// Walk a dotted path through a JSON Value, e.g. "payload.issue.title"
fn resolve_json_path(value: &Value, path: &str) -> String {
    let mut current = value;
    for segment in path.split('.') {
        match current {
            Value::Object(map) => {
                if let Some(v) = map.get(segment) {
                    current = v;
                } else {
                    return String::new();
                }
            }
            Value::Array(arr) => {
                if let Ok(idx) = segment.parse::<usize>() {
                    if let Some(v) = arr.get(idx) {
                        current = v;
                    } else {
                        return String::new();
                    }
                } else {
                    return String::new();
                }
            }
            _ => return String::new(),
        }
    }

    match current {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

// ── HMAC-SHA256 verification ───────────────────────────────────────────────

/// Verify a webhook signature using HMAC-SHA256.
/// Supports GitHub-style `sha256=<hex>` and plain hex signatures.
fn verify_signature(secret: &str, body: &[u8], signature_header: &str) -> bool {
    // Expand environment variables in the secret
    let expanded_secret = expand_env_vars(secret);

    // Compute HMAC-SHA256 manually using a simple implementation
    // For production, you'd use the `hmac` + `sha2` crates.
    // Here we use a basic approach that works without extra deps.
    let expected = hmac_sha256(expanded_secret.as_bytes(), body);
    let expected_hex = hex_encode(&expected);

    // Strip "sha256=" prefix if present (GitHub-style)
    let provided = signature_header
        .strip_prefix("sha256=")
        .unwrap_or(signature_header);

    // Constant-time comparison
    if expected_hex.len() != provided.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in expected_hex.bytes().zip(provided.bytes()) {
        diff |= a ^ b;
    }
    diff == 0
}

/// Minimal HMAC-SHA256 using standard library (no extra crate needed).
/// Uses the two-pass HMAC construction: H((K' xor opad) || H((K' xor ipad) || message))
fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    use std::io::Write;

    // SHA-256 block size is 64 bytes
    const BLOCK_SIZE: usize = 64;

    // If key > block size, hash it first
    let key_block = if key.len() > BLOCK_SIZE {
        let hash = sha256(key);
        let mut block = [0u8; BLOCK_SIZE];
        block[..32].copy_from_slice(&hash);
        block
    } else {
        let mut block = [0u8; BLOCK_SIZE];
        block[..key.len()].copy_from_slice(key);
        block
    };

    // Inner padding
    let mut ipad = [0x36u8; BLOCK_SIZE];
    for i in 0..BLOCK_SIZE {
        ipad[i] ^= key_block[i];
    }

    // Outer padding
    let mut opad = [0x5cu8; BLOCK_SIZE];
    for i in 0..BLOCK_SIZE {
        opad[i] ^= key_block[i];
    }

    // Inner hash: H(ipad || message)
    let mut inner_data = Vec::with_capacity(BLOCK_SIZE + message.len());
    inner_data.write_all(&ipad).unwrap();
    inner_data.write_all(message).unwrap();
    let inner_hash = sha256(&inner_data);

    // Outer hash: H(opad || inner_hash)
    let mut outer_data = Vec::with_capacity(BLOCK_SIZE + 32);
    outer_data.write_all(&opad).unwrap();
    outer_data.write_all(&inner_hash).unwrap();
    sha256(&outer_data)
}

/// Minimal SHA-256 implementation. In production code, prefer the `sha2` crate.
/// This uses Rust's standard library — we can piggyback off the `ring` or `sha2`
/// crate if available. For now, we use a raw implementation.
fn sha256(data: &[u8]) -> [u8; 32] {
    // SHA-256 constants
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
        0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
        0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
        0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
        0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
        0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
        0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
        0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];

    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];

    // Pre-processing: padding
    let bit_len = (data.len() as u64) * 8;
    let mut padded = data.to_vec();
    padded.push(0x80);
    while (padded.len() % 64) != 56 {
        padded.push(0x00);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    // Process each 512-bit block
    for chunk in padded.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut result = [0u8; 32];
    for (i, &val) in h.iter().enumerate() {
        result[i * 4..i * 4 + 4].copy_from_slice(&val.to_be_bytes());
    }
    result
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Expand `${ENV_VAR}` patterns in a string.
fn expand_env_vars(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut remaining = s;

    while let Some(start) = remaining.find("${") {
        result.push_str(&remaining[..start]);
        let after_open = &remaining[start + 2..];
        if let Some(end) = after_open.find('}') {
            let var_name = &after_open[..end];
            let value = std::env::var(var_name).unwrap_or_default();
            result.push_str(&value);
            remaining = &after_open[end + 1..];
        } else {
            result.push_str("${");
            remaining = after_open;
        }
    }
    result.push_str(remaining);
    result
}

// ── Plugin config loading ──────────────────────────────────────────────────

/// Load all webhook configs from `.scratch/plugins/*.yaml` files.
pub fn load_webhook_configs(notes_folder: &str) -> Vec<WebhookConfig> {
    let plugins_dir = PathBuf::from(notes_folder).join(".scratch").join("plugins");
    if !plugins_dir.exists() {
        return Vec::new();
    }

    let mut configs = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&plugins_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext == "yaml" || ext == "yml" {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    match serde_yaml::from_str::<WebhookConfig>(&content) {
                        Ok(config) => configs.push(config),
                        Err(e) => {
                            eprintln!(
                                "Warning: Failed to parse webhook config {}: {}",
                                path.display(),
                                e
                            );
                        }
                    }
                }
            }
        }
    }

    configs
}

// ── Activity log ───────────────────────────────────────────────────────────

const MAX_LOG_ENTRIES: usize = 200;

fn log_path(notes_folder: &str) -> PathBuf {
    PathBuf::from(notes_folder)
        .join(".scratch")
        .join("plugins")
        .join("webhook-log.json")
}

fn read_log(notes_folder: &str) -> Vec<WebhookLogEntry> {
    let path = log_path(notes_folder);
    if !path.exists() {
        return Vec::new();
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

fn append_log(notes_folder: &str, entry: WebhookLogEntry) {
    let path = log_path(notes_folder);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let mut entries = read_log(notes_folder);
    entries.push(entry);

    // Trim to max entries (keep most recent)
    if entries.len() > MAX_LOG_ENTRIES {
        entries = entries.split_off(entries.len() - MAX_LOG_ENTRIES);
    }

    if let Ok(json) = serde_json::to_string_pretty(&entries) {
        let _ = std::fs::write(&path, json);
    }
}

/// Determine the event key from the payload. Supports GitHub-style
/// `X-GitHub-Event` header + `action` field → "issues.opened" etc.
fn determine_event(headers: &HeaderMap, payload: &Value) -> String {
    // Check for GitHub-style event header
    if let Some(event_type) = headers
        .get("x-github-event")
        .and_then(|v| v.to_str().ok())
    {
        // Combine with action field if present
        if let Some(action) = payload.get("action").and_then(|v| v.as_str()) {
            return format!("{}.{}", event_type, action);
        }
        return event_type.to_string();
    }

    // Check for generic event field in payload
    if let Some(event) = payload.get("event").and_then(|v| v.as_str()) {
        return event.to_string();
    }

    // Check for type field
    if let Some(event_type) = payload.get("type").and_then(|v| v.as_str()) {
        return event_type.to_string();
    }

    "default".to_string()
}

// ── Axum handler ───────────────────────────────────────────────────────────

pub async fn handle_webhook(
    AxumState(state): AxumState<AppState>,
    Path(plugin_name): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> (StatusCode, Json<Value>) {
    let notes_folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        match app_config.notes_folder.clone() {
            Some(f) => f,
            None => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({ "error": "Notes folder not configured" })),
                );
            }
        }
    };

    // Load configs and find matching plugin
    let configs = load_webhook_configs(&notes_folder);
    let config = match configs.iter().find(|c| c.webhook.path == plugin_name) {
        Some(c) => c.clone(),
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("No webhook handler for '{}'", plugin_name) })),
            );
        }
    };

    // Parse the JSON body
    let payload: Value = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("Invalid JSON: {}", e) })),
            );
        }
    };

    // Verify secret if configured
    if let Some(ref secret) = config.webhook.secret {
        let expanded = expand_env_vars(secret);
        if !expanded.is_empty() {
            // Look for signature in common header locations
            let signature = headers
                .get("x-hub-signature-256")
                .or_else(|| headers.get("x-webhook-signature"))
                .or_else(|| headers.get("x-signature"))
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");

            if signature.is_empty() {
                append_log(
                    &notes_folder,
                    WebhookLogEntry {
                        timestamp: chrono_now(),
                        plugin: plugin_name.clone(),
                        event: "auth_failed".to_string(),
                        action: "verify".to_string(),
                        success: false,
                        error: Some("Missing signature header".to_string()),
                        note_id: None,
                    },
                );

                return (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "error": "Missing signature" })),
                );
            }

            if !verify_signature(&expanded, &body, signature) {
                append_log(
                    &notes_folder,
                    WebhookLogEntry {
                        timestamp: chrono_now(),
                        plugin: plugin_name.clone(),
                        event: "auth_failed".to_string(),
                        action: "verify".to_string(),
                        success: false,
                        error: Some("Invalid signature".to_string()),
                        note_id: None,
                    },
                );

                return (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "error": "Invalid signature" })),
                );
            }
        }
    }

    // Determine the event key
    let event_key = determine_event(&headers, &payload);

    // Find matching handler
    let handler = config
        .webhook
        .on
        .get(&event_key)
        .or_else(|| config.webhook.on.get("default"));

    let handler = match handler {
        Some(h) => h.clone(),
        None => {
            // No handler for this event — log it and return 200 (accepted but ignored)
            append_log(
                &notes_folder,
                WebhookLogEntry {
                    timestamp: chrono_now(),
                    plugin: plugin_name.clone(),
                    event: event_key.clone(),
                    action: "ignored".to_string(),
                    success: true,
                    error: None,
                    note_id: None,
                },
            );

            return (
                StatusCode::OK,
                Json(json!({ "status": "ignored", "event": event_key })),
            );
        }
    };

    // Build template context with payload accessible
    let context = json!({ "payload": payload });

    // Render the note content from template
    let rendered_content = render_template(&handler.template, &context);

    // Execute the action
    let result = match handler.action.as_str() {
        "create_note" => {
            execute_create_note(&state, &handler, &rendered_content, &context).await
        }
        "update_note" => {
            execute_update_note(&state, &handler, &rendered_content, &context).await
        }
        other => Err(format!("Unknown action: {}", other)),
    };

    match result {
        Ok(note_id) => {
            append_log(
                &notes_folder,
                WebhookLogEntry {
                    timestamp: chrono_now(),
                    plugin: plugin_name.clone(),
                    event: event_key.clone(),
                    action: handler.action.clone(),
                    success: true,
                    error: None,
                    note_id: Some(note_id.clone()),
                },
            );

            (
                StatusCode::OK,
                Json(json!({
                    "status": "ok",
                    "event": event_key,
                    "action": handler.action,
                    "note_id": note_id,
                })),
            )
        }
        Err(e) => {
            append_log(
                &notes_folder,
                WebhookLogEntry {
                    timestamp: chrono_now(),
                    plugin: plugin_name.clone(),
                    event: event_key.clone(),
                    action: handler.action.clone(),
                    success: false,
                    error: Some(e.clone()),
                    note_id: None,
                },
            );

            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "status": "error",
                    "event": event_key,
                    "error": e,
                })),
            )
        }
    }
}

async fn execute_create_note(
    state: &AppState,
    handler: &WebhookAction,
    content: &str,
    _context: &Value,
) -> Result<String, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config
            .notes_folder
            .clone()
            .ok_or("Notes folder not set")?
    };

    // Ensure subfolder exists
    if let Some(ref subfolder) = handler.folder {
        let target_dir = PathBuf::from(&folder).join(subfolder);
        if !target_dir.exists() {
            tokio::fs::create_dir_all(&target_dir)
                .await
                .map_err(|e| format!("Failed to create folder '{}': {}", subfolder, e))?;
        }
    }

    // Save the note with content (title derived from first # heading)
    let note = crate::save_note_impl(None, content.to_string(), state).await?;

    // If we need to move it to a subfolder, do so
    if let Some(ref subfolder) = handler.folder {
        let moved = crate::move_note_impl(note.id.clone(), subfolder.clone(), state).await?;
        return Ok(moved.id);
    }

    Ok(note.id)
}

async fn execute_update_note(
    state: &AppState,
    handler: &WebhookAction,
    content: &str,
    context: &Value,
) -> Result<String, String> {
    // Determine note ID from template or folder
    let note_id = if let Some(ref id_template) = handler.note_id {
        render_template(id_template, context)
    } else {
        return Err("update_note requires a note_id template".to_string());
    };

    if note_id.is_empty() {
        return Err("Rendered note_id is empty".to_string());
    }

    // Prepend folder prefix if specified
    let full_id = if let Some(ref subfolder) = handler.folder {
        format!("{}/{}", subfolder, note_id)
    } else {
        note_id
    };

    // Try to read existing note; if it doesn't exist, create it
    match crate::read_note_impl(full_id.clone(), state).await {
        Ok(_existing) => {
            // Update existing note
            let note = crate::save_note_impl(Some(full_id.clone()), content.to_string(), state).await?;
            Ok(note.id)
        }
        Err(_) => {
            // Note doesn't exist — create it
            // Ensure subfolder exists
            if let Some(ref subfolder) = handler.folder {
                let folder = {
                    let app_config = state.app_config.read().expect("app_config read lock");
                    app_config.notes_folder.clone().ok_or("Notes folder not set")?
                };
                let target_dir = PathBuf::from(&folder).join(subfolder);
                if !target_dir.exists() {
                    tokio::fs::create_dir_all(&target_dir)
                        .await
                        .map_err(|e| format!("Failed to create folder: {}", e))?;
                }
            }
            let note = crate::save_note_impl(Some(full_id.clone()), content.to_string(), state).await?;
            Ok(note.id)
        }
    }
}

/// Get ISO-8601 timestamp without external chrono dependency.
fn chrono_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();

    // Convert Unix timestamp to ISO 8601 (simplified — no chrono crate needed)
    // Calculate date components
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Days since 1970-01-01 to date
    let (year, month, day) = days_to_date(days as i64);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

/// Convert days since Unix epoch to (year, month, day).
fn days_to_date(mut days: i64) -> (i64, u32, u32) {
    // Shift to March-based year for easier leap year handling
    days += 719468; // days from 0000-03-01 to 1970-01-01

    let era = if days >= 0 { days } else { days - 146096 } / 146097;
    let doe = (days - era * 146097) as u32; // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };

    (year, m, d)
}

// ── Tauri commands for the activity log ────────────────────────────────────

pub fn get_webhook_log(notes_folder: &str) -> Vec<WebhookLogEntry> {
    read_log(notes_folder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_template_simple() {
        let ctx = json!({ "payload": { "issue": { "title": "Bug fix", "body": "Details here" } } });
        let template = "# {{payload.issue.title}}\n{{payload.issue.body}}";
        let result = render_template(template, &ctx);
        assert_eq!(result, "# Bug fix\nDetails here");
    }

    #[test]
    fn test_render_template_missing_field() {
        let ctx = json!({ "payload": {} });
        let result = render_template("Title: {{payload.missing}}", &ctx);
        assert_eq!(result, "Title: ");
    }

    #[test]
    fn test_render_template_nested() {
        let ctx = json!({ "payload": { "a": { "b": { "c": "deep" } } } });
        let result = render_template("{{payload.a.b.c}}", &ctx);
        assert_eq!(result, "deep");
    }

    #[test]
    fn test_expand_env_vars() {
        std::env::set_var("TEST_WEBHOOK_SECRET", "mysecret");
        assert_eq!(expand_env_vars("${TEST_WEBHOOK_SECRET}"), "mysecret");
        assert_eq!(expand_env_vars("prefix_${TEST_WEBHOOK_SECRET}_suffix"), "prefix_mysecret_suffix");
        assert_eq!(expand_env_vars("no_vars_here"), "no_vars_here");
        std::env::remove_var("TEST_WEBHOOK_SECRET");
    }

    #[test]
    fn test_hmac_sha256_known_vector() {
        // Test against a known HMAC-SHA256 vector
        let key = b"key";
        let message = b"The quick brown fox jumps over the lazy dog";
        let result = hmac_sha256(key, message);
        let hex = hex_encode(&result);
        assert_eq!(hex, "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8");
    }

    #[test]
    fn test_verify_signature_github_style() {
        let secret = "mysecret";
        let body = b"hello world";
        let hmac = hmac_sha256(secret.as_bytes(), body);
        let sig = format!("sha256={}", hex_encode(&hmac));
        assert!(verify_signature(secret, body, &sig));
        assert!(!verify_signature(secret, body, "sha256=0000000000000000000000000000000000000000000000000000000000000000"));
    }

    #[test]
    fn test_chrono_now_format() {
        let ts = chrono_now();
        // Should be ISO 8601 format
        assert!(ts.contains('T'));
        assert!(ts.ends_with('Z'));
        assert_eq!(ts.len(), 20);
    }

    #[test]
    fn test_determine_event_github() {
        let mut headers = HeaderMap::new();
        headers.insert("x-github-event", "issues".parse().unwrap());
        let payload = json!({ "action": "opened" });
        assert_eq!(determine_event(&headers, &payload), "issues.opened");
    }

    #[test]
    fn test_determine_event_generic() {
        let headers = HeaderMap::new();
        let payload = json!({ "event": "user.created" });
        assert_eq!(determine_event(&headers, &payload), "user.created");
    }
}

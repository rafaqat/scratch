use axum::{
    extract::State as AxumState,
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tower_http::cors::CorsLayer;

use crate::AppState;

// JSON-RPC request/response types
#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

impl JsonRpcResponse {
    fn success(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Value, code: i64, message: String) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data: None,
            }),
        }
    }
}

// MCP protocol constants
const MCP_PROTOCOL_VERSION: &str = "2025-03-26";
const SERVER_NAME: &str = "scratch-notes";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

// Tool definitions
fn get_tools() -> Value {
    json!([
        {
            "name": "scratch_list_notes",
            "description": "List notes in Scratch. Returns title, id, preview text, and last modified timestamp for each note. Supports folder filtering and recursive listing.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "folder": {
                        "type": "string",
                        "description": "Optional folder path to list notes from (e.g. 'projects' or 'work/drafts'). Omit to list root-level notes."
                    },
                    "recursive": {
                        "type": "boolean",
                        "description": "If true, list notes from all subfolders recursively. Defaults to false."
                    }
                },
                "required": []
            }
        },
        {
            "name": "scratch_read_note",
            "description": "Read the full markdown content of a note by its ID.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "The note ID. Can be a simple name (e.g. 'my-note') for root notes or a path (e.g. 'projects/todo') for notes in subfolders."
                    }
                },
                "required": ["id"]
            }
        },
        {
            "name": "scratch_create_note",
            "description": "Create a new empty note in Scratch. Returns the created note with its generated ID.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "folder": {
                        "type": "string",
                        "description": "Optional folder to create the note in (e.g. 'projects'). The folder must already exist. Omit to create in the root notes folder."
                    }
                },
                "required": []
            }
        },
        {
            "name": "scratch_update_note",
            "description": "Update an existing note's content. The note title is derived from the first # heading in the content. If the title changes, the note may be renamed (within its folder).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "The note ID to update. Use path-based IDs for subfolder notes (e.g. 'projects/todo')."
                    },
                    "content": {
                        "type": "string",
                        "description": "The full markdown content for the note"
                    }
                },
                "required": ["id", "content"]
            }
        },
        {
            "name": "scratch_delete_note",
            "description": "Delete a note by its ID. This permanently removes the markdown file.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "The note ID to delete. Use path-based IDs for subfolder notes (e.g. 'projects/todo')."
                    }
                },
                "required": ["id"]
            }
        },
        {
            "name": "scratch_search_notes",
            "description": "Full-text search across all notes (including subfolders) using Tantivy search engine. Returns matching notes with relevance scores.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query string"
                    }
                },
                "required": ["query"]
            }
        },
        {
            "name": "scratch_append_to_note",
            "description": "Append content to the end of an existing note.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "The note ID to append to. Use path-based IDs for subfolder notes (e.g. 'projects/todo')."
                    },
                    "content": {
                        "type": "string",
                        "description": "The markdown content to append"
                    }
                },
                "required": ["id", "content"]
            }
        },
        {
            "name": "scratch_get_info",
            "description": "Get information about the Scratch notes setup: notes folder path, total note count, and current settings.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "required": []
            }
        },
        {
            "name": "scratch_list_folders",
            "description": "List subfolders in the notes directory. Returns folder names relative to the notes root.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "parent": {
                        "type": "string",
                        "description": "Optional parent folder to list subfolders of (e.g. 'projects'). Omit to list top-level folders."
                    }
                },
                "required": []
            }
        },
        {
            "name": "scratch_create_folder",
            "description": "Create a new folder (or nested folders) in the notes directory.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The folder path to create, relative to the notes root (e.g. 'projects' or 'work/drafts'). Nested paths will create all intermediate directories."
                    }
                },
                "required": ["path"]
            }
        },
        {
            "name": "scratch_move_note",
            "description": "Move a note to a different folder. Use '.' as the destination to move a note to the root folder.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "The note ID to move (e.g. 'my-note' or 'projects/todo')."
                    },
                    "destination": {
                        "type": "string",
                        "description": "The destination folder (e.g. 'projects', 'work/drafts', or '.' for root)."
                    }
                },
                "required": ["id", "destination"]
            }
        },
        {
            "name": "scratch_list_directory",
            "description": "List all files and subdirectories in the notes folder (or a subfolder). Returns file names, sizes, modification times, and subdirectory names.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Optional subdirectory path relative to notes root (e.g. 'projects'). Omit to list the root notes folder."
                    }
                },
                "required": []
            }
        },
        {
            "name": "scratch_read_file",
            "description": "Read the raw contents of any file in the notes folder. Works with any file type, not just .md notes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path relative to the notes root (e.g. 'README.md', 'assets/data.json', 'projects/notes.txt')."
                    }
                },
                "required": ["path"]
            }
        },
        {
            "name": "scratch_find",
            "description": "Powerful search within and across notes. Supports exact substring matching, fuzzy matching (Levenshtein edit distance with similarity scoring), and regex patterns. Returns matches with line numbers, matched text, context lines, and similarity scores.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query: a substring (exact mode), approximate text (fuzzy mode), or regex pattern (regex mode)."
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["exact", "fuzzy", "regex"],
                        "description": "Search mode. 'exact': substring match. 'fuzzy': edit-distance match with similarity scoring. 'regex': regular expression. Defaults to 'exact'."
                    },
                    "note_id": {
                        "type": "string",
                        "description": "Optional note ID to search within a single note. Omit to search across all notes."
                    },
                    "case_sensitive": {
                        "type": "boolean",
                        "description": "Whether the search is case-sensitive. Defaults to false."
                    },
                    "context_lines": {
                        "type": "integer",
                        "description": "Number of lines of context to include before and after each match. Defaults to 2."
                    },
                    "max_distance": {
                        "type": "integer",
                        "description": "Maximum edit distance for fuzzy mode. Defaults to ~30% of query length (minimum 2)."
                    }
                },
                "required": ["query"]
            }
        },
        {
            "name": "scratch_replace_in_note",
            "description": "Find and replace text within a note. Supports replacing the first occurrence, all occurrences, or regex-based replacement. The note is saved automatically after replacement.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "The note ID to perform replacement in."
                    },
                    "find": {
                        "type": "string",
                        "description": "The text or regex pattern to find."
                    },
                    "replace": {
                        "type": "string",
                        "description": "The replacement text. For regex mode, supports backreferences ($1, $2, etc.)."
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["first", "all", "regex"],
                        "description": "Replace mode. 'first': replace first occurrence. 'all': replace all occurrences. 'regex': regex-based replacement. Defaults to 'all'."
                    },
                    "case_sensitive": {
                        "type": "boolean",
                        "description": "Whether the search is case-sensitive. Defaults to true."
                    }
                },
                "required": ["id", "find", "replace"]
            }
        },
        // --- Stories / Kanban tools ---
        {
            "name": "stories_epics_list",
            "description": "List epics (E-####-slug folders) under a given base directory in the notes folder.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "basePath": {
                        "type": "string",
                        "description": "Subdirectory to search for epics (e.g., 'product'). Defaults to notes root."
                    }
                }
            }
        },
        {
            "name": "stories_boards_get",
            "description": "Get a kanban board for one epic — returns lanes grouped by status (Backlog, Ready, In Progress, In Review, Done, Blocked) with story cards.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "epicId": {
                        "type": "string",
                        "description": "Epic identifier, e.g. 'E-0123'"
                    }
                },
                "required": ["epicId"]
            }
        },
        {
            "name": "stories_list",
            "description": "List stories (cards) with optional filters by epic, status, tag, owner, or text search.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "epicId": {
                        "type": "string",
                        "description": "Filter to stories in this epic (e.g. 'E-0123')"
                    },
                    "status": {
                        "type": "string",
                        "description": "Filter by status: Backlog, Ready, In Progress, In Review, Done, Blocked"
                    },
                    "tag": {
                        "type": "string",
                        "description": "Filter by tag"
                    },
                    "owner": {
                        "type": "string",
                        "description": "Filter by owner"
                    },
                    "text": {
                        "type": "string",
                        "description": "Text search in title and body"
                    }
                }
            }
        },
        {
            "name": "stories_get",
            "description": "Get one story including YAML frontmatter, markdown body, path, and etag for concurrency control.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Story ID, e.g. 'S-0123-02'"
                    }
                },
                "required": ["id"]
            }
        },
        {
            "name": "stories_create",
            "description": "Create a new story under an epic with server-generated ID and filename. Returns the new story ID and path.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "epicId": {
                        "type": "string",
                        "description": "Epic to create the story in, e.g. 'E-0123'"
                    },
                    "title": {
                        "type": "string",
                        "description": "Story title"
                    },
                    "status": {
                        "type": "string",
                        "description": "Initial status. Defaults to 'Backlog'. One of: Backlog, Ready, In Progress, In Review, Done, Blocked"
                    },
                    "owner": {
                        "type": "string",
                        "description": "Story owner"
                    },
                    "estimatePoints": {
                        "type": "number",
                        "description": "Story point estimate"
                    },
                    "tags": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Tags for the story"
                    }
                },
                "required": ["epicId", "title"]
            }
        },
        {
            "name": "stories_update",
            "description": "Update story metadata and/or body. Supports optimistic concurrency via etag. Returns CONFLICT error if etag mismatch.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Story ID, e.g. 'S-0123-02'"
                    },
                    "etag": {
                        "type": "string",
                        "description": "ETag from stories_get for optimistic concurrency"
                    },
                    "patch": {
                        "type": "object",
                        "description": "Object with fields to update: title, status, owner, estimate_points, tags, links"
                    },
                    "markdownBody": {
                        "type": "string",
                        "description": "New markdown body (replaces entire body)"
                    }
                },
                "required": ["id", "etag"]
            }
        },
        {
            "name": "stories_move",
            "description": "Move story between kanban lanes by updating status. Supports optimistic concurrency via etag.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Story ID, e.g. 'S-0123-02'"
                    },
                    "etag": {
                        "type": "string",
                        "description": "ETag from stories_get for optimistic concurrency"
                    },
                    "status": {
                        "type": "string",
                        "description": "New status: Backlog, Ready, In Progress, In Review, Done, Blocked"
                    }
                },
                "required": ["id", "etag", "status"]
            }
        },
        {
            "name": "stories_search",
            "description": "Search across stories by text, tag, owner, status. Returns snippets around matches.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": {
                        "type": "string",
                        "description": "Text to search in title and body"
                    },
                    "epicId": {
                        "type": "string",
                        "description": "Limit search to this epic"
                    },
                    "tag": {
                        "type": "string",
                        "description": "Filter by tag"
                    },
                    "owner": {
                        "type": "string",
                        "description": "Filter by owner"
                    },
                    "status": {
                        "type": "string",
                        "description": "Filter by status"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Max results to return. Defaults to 20."
                    }
                }
            }
        },
        {
            "name": "stories_validate",
            "description": "Validate a story file against the schema and conventions. Returns errors (schema violations) and warnings (missing recommended sections).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Story ID to validate, e.g. 'S-0123-02'"
                    }
                },
                "required": ["id"]
            }
        }
    ])
}

// Start the MCP HTTP server on a tokio task
pub fn start_mcp_server(state: AppState, port: u16) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        let app = Router::new()
            .route("/mcp", post(handle_mcp))
            .route("/health", get(handle_health))
            .layer(CorsLayer::permissive())
            .with_state(state);

        let addr = format!("127.0.0.1:{}", port);
        let listener = match tokio::net::TcpListener::bind(&addr).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("MCP server failed to bind to {}: {}", addr, e);
                return;
            }
        };

        eprintln!("MCP server listening on http://{}", addr);

        if let Err(e) = axum::serve(listener, app).await {
            eprintln!("MCP server error: {}", e);
        }
    })
}

// Health check endpoint
async fn handle_health(AxumState(state): AxumState<AppState>) -> Json<Value> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone()
    };

    let note_count = {
        let cache = state.notes_cache.read().expect("cache read lock");
        cache.len()
    };

    Json(json!({
        "status": "ok",
        "server": SERVER_NAME,
        "version": SERVER_VERSION,
        "notes_folder": folder,
        "note_count": note_count,
    }))
}

// Main MCP JSON-RPC handler
async fn handle_mcp(
    AxumState(state): AxumState<AppState>,
    Json(request): Json<JsonRpcRequest>,
) -> (StatusCode, Json<JsonRpcResponse>) {
    let id = request.id.unwrap_or(Value::Null);

    if request.jsonrpc != "2.0" {
        return (
            StatusCode::OK,
            Json(JsonRpcResponse::error(
                id,
                -32600,
                "Invalid JSON-RPC version".to_string(),
            )),
        );
    }

    let response = match request.method.as_str() {
        "initialize" => handle_initialize(id),
        "notifications/initialized" => {
            // Client acknowledgement, no response needed but we return success
            JsonRpcResponse::success(id, json!({}))
        }
        "tools/list" => handle_tools_list(id),
        "tools/call" => Box::pin(handle_tools_call(id, request.params, &state)).await,
        "resources/list" => handle_resources_list(id),
        "resources/read" => handle_resources_read(id, request.params),
        "prompts/list" => JsonRpcResponse::success(id, json!({ "prompts": [] })),
        "ping" => JsonRpcResponse::success(id, json!({})),
        _ => JsonRpcResponse::error(id, -32601, format!("Method not found: {}", request.method)),
    };

    (StatusCode::OK, Json(response))
}

// MCP initialize
fn handle_initialize(id: Value) -> JsonRpcResponse {
    JsonRpcResponse::success(
        id,
        json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {
                "tools": { "listChanged": false },
                "resources": { "listChanged": false },
            },
            "serverInfo": {
                "name": SERVER_NAME,
                "version": SERVER_VERSION,
            }
        }),
    )
}

// MCP tools/list
fn handle_tools_list(id: Value) -> JsonRpcResponse {
    JsonRpcResponse::success(id, json!({ "tools": get_tools() }))
}

// MCP resources/list
fn handle_resources_list(id: Value) -> JsonRpcResponse {
    JsonRpcResponse::success(id, json!({ "resources": [] }))
}

// MCP resources/read
fn handle_resources_read(id: Value, _params: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse::error(id, -32602, "Resource not found".to_string())
}

// MCP tools/call dispatcher
async fn handle_tools_call(
    id: Value,
    params: Option<Value>,
    state: &AppState,
) -> JsonRpcResponse {
    let params = match params {
        Some(p) => p,
        None => {
            return JsonRpcResponse::error(id, -32602, "Missing params".to_string());
        }
    };

    let tool_name = params
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let result = match tool_name {
        "scratch_list_notes" => tool_list_notes(state, &arguments).await,
        "scratch_read_note" => tool_read_note(state, &arguments).await,
        "scratch_create_note" => tool_create_note(state, &arguments).await,
        "scratch_update_note" => tool_update_note(state, &arguments).await,
        "scratch_delete_note" => tool_delete_note(state, &arguments).await,
        "scratch_search_notes" => tool_search_notes(state, &arguments).await,
        "scratch_append_to_note" => tool_append_to_note(state, &arguments).await,
        "scratch_get_info" => tool_get_info(state).await,
        "scratch_list_folders" => tool_list_folders(state, &arguments).await,
        "scratch_create_folder" => tool_create_folder(state, &arguments).await,
        "scratch_move_note" => tool_move_note(state, &arguments).await,
        "scratch_list_directory" => tool_list_directory(state, &arguments).await,
        "scratch_read_file" => tool_read_file(state, &arguments).await,
        "scratch_find" => tool_find(state, &arguments).await,
        "scratch_replace_in_note" => tool_replace_in_note(state, &arguments).await,
        // Stories / Kanban tools
        "stories_epics_list" => tool_epics_list(state, &arguments).await,
        "stories_boards_get" => tool_boards_get(state, &arguments).await,
        "stories_list" => tool_stories_list(state, &arguments).await,
        "stories_get" => tool_stories_get(state, &arguments).await,
        "stories_create" => tool_stories_create(state, &arguments).await,
        "stories_update" => tool_stories_update(state, &arguments).await,
        "stories_move" => tool_stories_move(state, &arguments).await,
        "stories_search" => tool_stories_search(state, &arguments).await,
        "stories_validate" => tool_stories_validate(state, &arguments).await,
        _ => Err(format!("Unknown tool: {}", tool_name)),
    };

    match result {
        Ok(content) => JsonRpcResponse::success(
            id,
            json!({
                "content": [{
                    "type": "text",
                    "text": content
                }]
            }),
        ),
        Err(e) => JsonRpcResponse::success(
            id,
            json!({
                "content": [{
                    "type": "text",
                    "text": format!("Error: {}", e)
                }],
                "isError": true
            }),
        ),
    }
}

// Tool implementations

async fn tool_list_notes(state: &AppState, args: &Value) -> Result<String, String> {
    let folder = args.get("folder").and_then(|v| v.as_str()).map(|s| s.to_string());
    let recursive = args.get("recursive").and_then(|v| v.as_bool()).unwrap_or(false);

    let notes = crate::list_notes_impl(state, folder.as_deref(), recursive).await?;
    serde_json::to_string_pretty(&notes).map_err(|e| e.to_string())
}

async fn tool_read_note(state: &AppState, args: &Value) -> Result<String, String> {
    let id = args
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: id")?
        .to_string();

    let note = crate::read_note_impl(id, state).await?;
    Ok(note.content)
}

async fn tool_create_note(state: &AppState, args: &Value) -> Result<String, String> {
    let folder = args.get("folder").and_then(|v| v.as_str()).map(|s| s.to_string());

    let note = crate::create_note_impl(folder, state).await?;
    serde_json::to_string_pretty(&note).map_err(|e| e.to_string())
}

async fn tool_update_note(state: &AppState, args: &Value) -> Result<String, String> {
    let id = args
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: id")?
        .to_string();

    let content = args
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: content")?
        .to_string();

    let note = crate::save_note_impl(Some(id), content, state).await?;
    serde_json::to_string_pretty(&note).map_err(|e| e.to_string())
}

async fn tool_delete_note(state: &AppState, args: &Value) -> Result<String, String> {
    let id = args
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: id")?
        .to_string();

    crate::delete_note_impl(id, state).await?;
    Ok("Note deleted successfully".to_string())
}

async fn tool_search_notes(state: &AppState, args: &Value) -> Result<String, String> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: query")?
        .to_string();

    let results = crate::search_notes_impl(query, state).await?;
    serde_json::to_string_pretty(&results).map_err(|e| e.to_string())
}

async fn tool_append_to_note(state: &AppState, args: &Value) -> Result<String, String> {
    let id = args
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: id")?
        .to_string();

    let append_content = args
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: content")?;

    // Read existing note
    let existing = crate::read_note_impl(id.clone(), state).await?;

    // Append content
    let new_content = format!("{}\n{}", existing.content, append_content);

    let note = crate::save_note_impl(Some(id), new_content, state).await?;
    serde_json::to_string_pretty(&note).map_err(|e| e.to_string())
}

async fn tool_get_info(state: &AppState) -> Result<String, String> {
    let folder = {
        let app_config = state.app_config.read().expect("app_config read lock");
        app_config.notes_folder.clone()
    };

    let note_count = {
        let cache = state.notes_cache.read().expect("cache read lock");
        cache.len()
    };

    let settings = crate::get_settings_impl(state);

    let info = json!({
        "notes_folder": folder,
        "note_count": note_count,
        "mcp_enabled": settings.mcp_enabled.unwrap_or(false),
        "mcp_port": settings.mcp_port.unwrap_or(3921),
        "git_enabled": settings.git_enabled.unwrap_or(false),
    });

    serde_json::to_string_pretty(&info).map_err(|e| e.to_string())
}

async fn tool_list_folders(state: &AppState, args: &Value) -> Result<String, String> {
    let parent = args.get("parent").and_then(|v| v.as_str()).map(|s| s.to_string());

    let folders = crate::list_folders_impl(parent, state).await?;
    serde_json::to_string_pretty(&folders).map_err(|e| e.to_string())
}

async fn tool_create_folder(state: &AppState, args: &Value) -> Result<String, String> {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: path")?
        .to_string();

    let created = crate::create_folder_impl(path, state).await?;
    Ok(format!("Folder created: {}", created))
}

async fn tool_move_note(state: &AppState, args: &Value) -> Result<String, String> {
    let id = args
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: id")?
        .to_string();

    let destination = args
        .get("destination")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: destination")?
        .to_string();

    let note = crate::move_note_impl(id, destination, state).await?;
    serde_json::to_string_pretty(&note).map_err(|e| e.to_string())
}

async fn tool_list_directory(state: &AppState, args: &Value) -> Result<String, String> {
    let path = args.get("path").and_then(|v| v.as_str()).map(|s| s.to_string());

    let result = crate::list_directory_impl(path, state).await?;
    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

async fn tool_read_file(state: &AppState, args: &Value) -> Result<String, String> {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: path")?
        .to_string();

    crate::read_file_impl(path, state).await
}

async fn tool_find(state: &AppState, args: &Value) -> Result<String, String> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: query")?
        .to_string();

    let mode = args
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("exact")
        .to_string();

    let note_id = args.get("note_id").and_then(|v| v.as_str()).map(|s| s.to_string());

    let case_sensitive = args
        .get("case_sensitive")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let context_lines = args
        .get("context_lines")
        .and_then(|v| v.as_u64())
        .unwrap_or(2) as usize;

    let max_distance = args
        .get("max_distance")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);

    let result = crate::find_in_notes_impl(
        query,
        mode,
        note_id,
        case_sensitive,
        context_lines,
        max_distance,
        state,
    )
    .await?;

    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

async fn tool_replace_in_note(state: &AppState, args: &Value) -> Result<String, String> {
    let id = args
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: id")?
        .to_string();

    let find = args
        .get("find")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: find")?
        .to_string();

    let replace = args
        .get("replace")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: replace")?
        .to_string();

    let mode = args
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("all")
        .to_string();

    let case_sensitive = args
        .get("case_sensitive")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let result = crate::replace_in_note_impl(id, find, replace, mode, case_sensitive, state).await?;
    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

// --- Stories / Kanban tool handlers ---

async fn tool_epics_list(state: &AppState, args: &Value) -> Result<String, String> {
    let base_path = args.get("basePath").and_then(|v| v.as_str()).map(String::from);
    let result = crate::epics_list_impl(base_path, state).await?;
    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

async fn tool_boards_get(state: &AppState, args: &Value) -> Result<String, String> {
    let epic_id = args
        .get("epicId")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: epicId")?
        .to_string();

    let result = crate::boards_get_impl(epic_id, state).await?;
    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

async fn tool_stories_list(state: &AppState, args: &Value) -> Result<String, String> {
    let epic_id = args.get("epicId").and_then(|v| v.as_str()).map(String::from);
    let status = args.get("status").and_then(|v| v.as_str()).map(String::from);
    let tag = args.get("tag").and_then(|v| v.as_str()).map(String::from);
    let owner = args.get("owner").and_then(|v| v.as_str()).map(String::from);
    let text = args.get("text").and_then(|v| v.as_str()).map(String::from);

    let result = crate::stories_list_impl(epic_id, status, tag, owner, text, state).await?;
    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

async fn tool_stories_get(state: &AppState, args: &Value) -> Result<String, String> {
    let id = args
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: id")?
        .to_string();

    let result = crate::stories_get_impl(id, state).await?;
    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

async fn tool_stories_create(state: &AppState, args: &Value) -> Result<String, String> {
    let epic_id = args
        .get("epicId")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: epicId")?
        .to_string();

    let title = args
        .get("title")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: title")?
        .to_string();

    let status = args.get("status").and_then(|v| v.as_str()).map(String::from);
    let owner = args.get("owner").and_then(|v| v.as_str()).map(String::from);
    let estimate_points = args.get("estimatePoints").and_then(|v| v.as_f64());
    let tags = args.get("tags").and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|t| t.as_str().map(String::from))
            .collect()
    });

    let result =
        crate::stories_create_impl(epic_id, title, status, owner, estimate_points, tags, state)
            .await?;
    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

async fn tool_stories_update(state: &AppState, args: &Value) -> Result<String, String> {
    let id = args
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: id")?
        .to_string();

    let etag = args
        .get("etag")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: etag")?
        .to_string();

    let patch = args.get("patch").cloned();
    let markdown_body = args
        .get("markdownBody")
        .and_then(|v| v.as_str())
        .map(String::from);

    let result = crate::stories_update_impl(id, etag, patch, markdown_body, state).await?;
    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

async fn tool_stories_move(state: &AppState, args: &Value) -> Result<String, String> {
    let id = args
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: id")?
        .to_string();

    let etag = args
        .get("etag")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: etag")?
        .to_string();

    let status = args
        .get("status")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: status")?
        .to_string();

    let result = crate::stories_move_impl(id, etag, status, state).await?;
    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

async fn tool_stories_search(state: &AppState, args: &Value) -> Result<String, String> {
    let text = args.get("text").and_then(|v| v.as_str()).map(String::from);
    let epic_id = args.get("epicId").and_then(|v| v.as_str()).map(String::from);
    let tag = args.get("tag").and_then(|v| v.as_str()).map(String::from);
    let owner = args.get("owner").and_then(|v| v.as_str()).map(String::from);
    let status = args.get("status").and_then(|v| v.as_str()).map(String::from);
    let limit = args.get("limit").and_then(|v| v.as_u64()).map(|v| v as usize);

    let result =
        crate::search_stories_impl(text, epic_id, tag, owner, status, limit, state).await?;
    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

async fn tool_stories_validate(state: &AppState, args: &Value) -> Result<String, String> {
    let id = args
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: id")?
        .to_string();

    let result = crate::validate_story_impl(id, state).await?;
    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

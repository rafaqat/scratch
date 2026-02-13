# Scratch MCP Server

Scratch is a local markdown note-taking app. This MCP server gives you full read/write access to the user's notes, folders, and kanban stories. All data is stored as plain files on disk in the user's chosen notes folder.

## Quick Start

```
scratch_get_info          → see notes folder path, note count, settings
scratch_list_notes        → browse all notes
scratch_read_note id="my-note"  → read a note
scratch_search_notes query="meeting"  → full-text search
```

## Concepts

- **Notes** are markdown files. The note ID is the filename without `.md`. Notes in subfolders use path-based IDs like `projects/todo`.
- **Title** is derived from the first `# Heading` in the note content. Changing the heading renames the file.
- **Folders** map to directories inside the notes folder.
- **Stories** are markdown files with YAML frontmatter, organized in epic folders under a `product/` directory.

## Tools Reference

### Notes — CRUD

**scratch_list_notes** — List all notes with title, ID, preview, and last modified timestamp.
```json
{"folder": "projects"}          // list notes in projects/
{"recursive": true}             // list all notes in all subfolders
{"folder": "work", "recursive": true}  // all notes under work/
{}                              // list root-level notes
```

**scratch_read_note** — Read full markdown content.
```json
{"id": "meeting-notes"}         // root note
{"id": "projects/roadmap"}      // note in subfolder
```

**scratch_create_note** — Create a new empty note. Returns the generated ID.
```json
{}                              // create in root
{"folder": "projects"}          // create in projects/
```

**scratch_update_note** — Replace a note's content. Title auto-updates from first heading.
```json
{"id": "my-note", "content": "# Updated Title\n\nNew content here."}
```

**scratch_delete_note** — Permanently delete a note.
```json
{"id": "old-note"}
```

**scratch_append_to_note** — Append content to the end of a note.
```json
{"id": "journal", "content": "\n## Feb 13\n\nToday I worked on..."}
```

### Search

**scratch_search_notes** — Full-text search powered by Tantivy. Returns top 20 results with relevance scores.
```json
{"query": "authentication"}
```

**scratch_find** — Advanced search with three modes. Works within a single note or across all notes.
```json
{"query": "TODO", "mode": "exact"}                    // substring match
{"query": "authenticate", "mode": "fuzzy"}             // typo-tolerant (~30% edit distance)
{"query": "fn\\s+\\w+", "mode": "regex"}               // regex pattern
{"query": "bug", "note_id": "projects/tracker"}        // search in one note
{"query": "config", "context_lines": 5}                // more context around matches
{"query": "password", "case_sensitive": true}           // case-sensitive
```

**scratch_replace_in_note** — Find and replace within a note.
```json
{"id": "readme", "find": "v1.0", "replace": "v2.0"}                    // replace all
{"id": "readme", "find": "v1.0", "replace": "v2.0", "mode": "first"}   // first only
{"id": "code", "find": "let (\\w+)", "replace": "const $1", "mode": "regex"}  // regex
```

### Folders & Files

**scratch_list_folders** — List subfolders.
```json
{}                              // top-level folders
{"parent": "projects"}          // subfolders of projects/
```

**scratch_create_folder** — Create folders (nested paths auto-create intermediates).
```json
{"path": "projects"}
{"path": "work/2024/q1"}       // creates work/, work/2024/, work/2024/q1/
```

**scratch_move_note** — Move a note between folders.
```json
{"id": "my-note", "destination": "archive"}     // move to archive/
{"id": "archive/old", "destination": "."}       // move back to root
```

**scratch_list_directory** — List all files and subdirectories with sizes and modification times.
```json
{}                              // root
{"path": "projects"}            // specific directory
```

**scratch_read_file** — Read any file (not just .md). Useful for config, JSON, etc.
```json
{"path": ".scratch/settings.json"}
{"path": "assets/data.csv"}
```

### Info

**scratch_get_info** — Get the notes folder path, total note count, and current settings.
```json
{}
```

## Kanban Stories

Stories are markdown files with YAML frontmatter organized in epic folders:

```
product/
  E-0001-payments/
    stories/
      S-0001-01-user-onboarding.md
      S-0001-02-kyc-webhook.md
```

### Story File Format

```yaml
---
id: S-0001-01
epic: E-0001
title: User onboarding flow
status: In Progress
owner: sara
estimate_points: 3.0
tags:
- onboarding
- web
timestamps:
  created_at: 2026-02-13T10:42:49Z
  updated_at: 2026-02-13T10:44:30Z
---
## Problem

Users need a smooth onboarding experience.

## Acceptance Criteria

- User can sign up
- User receives welcome email
```

### Statuses

Backlog → Ready → In Progress → In Review → Done

Also: Blocked (can be set at any time)

### Concurrency Control

All mutations (update, move) require an `etag` obtained from `stories_get`. If the file changed since you read it, you get a CONFLICT error with the current etag. Re-read and retry.

### Story Tools

**stories_epics_list** — List all epics.
```json
{"basePath": "product"}         // search in product/ directory
{}                              // search in notes root
```

**stories_boards_get** — Get a full kanban board for an epic with 6 lanes.
```json
{"epicId": "E-0001"}
```
Returns: `{ epicId, lanes: [{ status: "Backlog", cards: [...] }, ...], generatedAt }`

**stories_list** — List stories with optional filters. All filters are combinable.
```json
{"epicId": "E-0001"}                    // all stories in epic
{"status": "In Progress"}               // by status
{"owner": "sara"}                        // by owner
{"tag": "mvp"}                           // by tag
{"text": "onboarding"}                   // text search in title+body
{"epicId": "E-0001", "status": "Done", "owner": "alex"}  // combined
```

**stories_get** — Get a single story with full frontmatter, markdown body, path, and etag.
```json
{"id": "S-0001-01"}
```
Returns: `{ story: { frontmatter: {...}, markdownBody: "...", path: "...", etag: "abc123" } }`

**stories_create** — Create a new story. ID is auto-generated.
```json
{
  "epicId": "E-0001",
  "title": "Add payment processing",
  "status": "Backlog",
  "owner": "alex",
  "estimatePoints": 5,
  "tags": ["payments", "api"]
}
```
Only `epicId` and `title` are required. Status defaults to "Backlog".

**stories_update** — Update story metadata and/or body. Requires etag.
```json
{
  "id": "S-0001-01",
  "etag": "abc123def456",
  "patch": {
    "owner": "rafa",
    "tags": ["payments", "api", "urgent"],
    "estimate_points": 8
  }
}
```
Patchable fields: `title`, `status`, `owner`, `estimate_points`, `tags`, `links`.

To update the markdown body:
```json
{
  "id": "S-0001-01",
  "etag": "abc123def456",
  "markdownBody": "## Problem\n\nUpdated description.\n\n## Acceptance Criteria\n\n- New criteria"
}
```

**stories_move** — Change story status (move between kanban lanes). Requires etag.
```json
{"id": "S-0001-01", "etag": "abc123def456", "status": "In Review"}
```

**stories_search** — Search across all stories.
```json
{"text": "payment"}                     // text search
{"tag": "mvp", "status": "Backlog"}     // filter search
{"owner": "sara", "limit": 5}           // limit results
```

**stories_validate** — Check a story against schema and conventions.
```json
{"id": "S-0001-01"}
```
Returns: `{ valid: true/false, errors: [...], warnings: [...] }`

### Common Workflows

**Example: Create a new project from scratch**

Step 1 — Create the epic folder structure:
```json
scratch_create_folder {"path": "product/E-0003-auth/stories"}
```

Step 2 — Create stories with full metadata:
```json
stories_create {
  "epicId": "E-0003",
  "title": "Login page with email and password",
  "status": "Ready",
  "owner": "alex",
  "estimatePoints": 3,
  "tags": ["auth", "web", "mvp"]
}
```
→ Returns `{ story: { id: "S-0003-01", path: "..." } }`

```json
stories_create {
  "epicId": "E-0003",
  "title": "OAuth integration with Google and GitHub",
  "status": "Backlog",
  "owner": "sara",
  "estimatePoints": 8,
  "tags": ["auth", "api", "oauth"]
}
```
→ Returns `{ story: { id: "S-0003-02", path: "..." } }`

```json
stories_create {
  "epicId": "E-0003",
  "title": "Password reset flow",
  "status": "Backlog",
  "estimatePoints": 5,
  "tags": ["auth", "email"]
}
```
→ Returns `{ story: { id: "S-0003-03", path: "..." } }`

Step 3 — Fill in story details:
```json
stories_get {"id": "S-0003-01"}
```
→ Returns story with etag "a1b2c3d4e5f6g7h8"

```json
stories_update {
  "id": "S-0003-01",
  "etag": "a1b2c3d4e5f6g7h8",
  "markdownBody": "## Problem\n\nUsers need a secure way to log into the application.\n\n## Acceptance Criteria\n\n- Email + password login form\n- Form validation with error messages\n- Rate limiting after 5 failed attempts\n- \"Remember me\" checkbox\n- Redirect to dashboard after login\n\n## UX Notes\n\n- Center form on page, max-width 400px\n- Show/hide password toggle\n- Loading spinner on submit\n\n## API / Data\n\nPOST /api/auth/login { email, password, rememberMe }\nResponse: { token, expiresAt, user }\n\n## Test Notes\n\n- Test with valid and invalid credentials\n- Test rate limiting threshold\n- Test session persistence with \"remember me\""
}
```

Step 4 — View the board:
```json
stories_boards_get {"epicId": "E-0003"}
```
→ Returns lanes with cards grouped by status

**Example: Move stories through the board (sprint workflow)**

```json
// Developer picks up a story
stories_get {"id": "S-0003-01"}
// → etag: "abc123"

stories_move {"id": "S-0003-01", "etag": "abc123", "status": "In Progress"}
// → new etag: "def456"

// Work is done, submit for review
stories_move {"id": "S-0003-01", "etag": "def456", "status": "In Review"}
// → new etag: "ghi789"

// Review passed
stories_move {"id": "S-0003-01", "etag": "ghi789", "status": "Done"}
```

**Example: Update story metadata (reassign, re-estimate, retag)**

```json
stories_get {"id": "S-0003-02"}
// → etag: "xyz999"

stories_update {
  "id": "S-0003-02",
  "etag": "xyz999",
  "patch": {
    "owner": "rafa",
    "estimate_points": 13,
    "tags": ["auth", "api", "oauth", "blocked-by-google"],
    "title": "OAuth integration with Google, GitHub, and Apple"
  }
}
```

**Example: Sprint planning — find and prioritize work**

```json
// See the full board
stories_boards_get {"epicId": "E-0003"}

// Find all unassigned backlog stories
stories_list {"status": "Backlog", "epicId": "E-0003"}

// Find everything tagged as MVP across all epics
stories_list {"tag": "mvp"}

// Find all of sara's in-progress work across all epics
stories_list {"owner": "sara", "status": "In Progress"}

// Search for anything mentioning "security"
stories_search {"text": "security"}

// Search within a specific epic
stories_search {"text": "password", "epicId": "E-0003"}
```

**Example: Handle etag conflict (concurrent edit)**

```json
stories_update {
  "id": "S-0003-01",
  "etag": "stale_etag_value",
  "patch": {"owner": "sara"}
}
// → Error: "CONFLICT: File has been modified. Current etag: fresh_etag_123"

// Re-read to get fresh data and etag
stories_get {"id": "S-0003-01"}
// → etag: "fresh_etag_123"

// Retry with correct etag
stories_update {
  "id": "S-0003-01",
  "etag": "fresh_etag_123",
  "patch": {"owner": "sara"}
}
// → Success
```

**Example: Validate stories before a release**

```json
stories_validate {"id": "S-0003-01"}
// → { valid: true, errors: [], warnings: [] }

stories_validate {"id": "S-0003-03"}
// → { valid: true, errors: [], warnings: [
//     { field: "body", message: "Missing recommended heading: Problem" },
//     { field: "body", message: "Missing recommended heading: Acceptance Criteria" }
//   ]}
```

**Example: Read audit trail**

```json
scratch_read_file {"path": ".scratch/audit/events.jsonl"}
```
Each line is a JSON event:
```json
{"action":"stories.create","actor":"mcp","id":"S-0003-01","before":null,"after":{"epic":"E-0003","title":"Login page"},"ts":"2026-02-13T12:00:00Z"}
{"action":"stories.move","actor":"mcp","id":"S-0003-01","before":{"status":"Ready"},"after":{"status":"In Progress"},"ts":"2026-02-13T12:05:00Z"}
{"action":"stories.update","actor":"mcp","id":"S-0003-01","before":{...},"after":{...},"ts":"2026-02-13T12:10:00Z"}
```

### Naming Conventions

- **Epic IDs**: `E-NNNN` (e.g., E-0001, E-0042)
- **Epic folders**: `E-NNNN-slug` (e.g., E-0001-payments-kyc)
- **Story IDs**: `S-NNNN-NN` (e.g., S-0001-01, S-0001-12) — auto-generated
- **Story files**: `S-NNNN-NN-slug.md` (e.g., S-0001-01-user-onboarding.md) — auto-generated
- **Base path**: Stories live under a directory like `product/` in the notes folder

### Recommended Story Body Sections

Stories should include these markdown headings for consistency:
- `## Problem` — What user problem does this solve?
- `## Acceptance Criteria` — Bullet list of requirements
- `## UX Notes` — Design guidance
- `## API / Data` — Endpoints, schemas, data model
- `## Test Notes` — Testing guidance

### Audit Log

All story create/update/move operations are logged to `.scratch/audit/events.jsonl` in the notes folder. Each line is a JSON object with: action, actor, id, before, after, timestamp.

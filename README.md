# Scratch

<img src="docs/app-icon.png" alt="Scratch" width="128" height="128" style="border-radius: 22px; margin-bottom: 8px;">

A minimalist, offline-first markdown note-taking app for macOS and Windows.

![macOS](https://img.shields.io/badge/platform-macOS-lightgrey) ![Windows](https://img.shields.io/badge/platform-Windows-blue)

[Website](https://www.ericli.io/scratch) · [Releases](https://github.com/erictli/scratch/releases)

## Features

- **Offline-first** - No cloud, no account, no internet required
- **Markdown-based** - Notes stored as plain `.md` files you own
- **WYSIWYG editing** - Rich text editing that saves as markdown
- **Edit with Claude Code** - Use your local Claude Code CLI to edit notes
- **Works with other AI agents** - Detects external file changes
- **Keyboard optimized** - Lots of shortcuts and a command palette
- **Customizable** - Theme and editor typography settings
- **Git integration** - Optional version control for your notes
- **MCP server** - Built-in Model Context Protocol server for AI agents
- **Kanban stories** - Manage epics and stories as markdown with YAML frontmatter
- **Lightweight** - Less than 10% the size of Obsidian or Notion

## Screenshot

![Screenshot](docs/screenshot.png)

## Installation

### macOS

**Homebrew (Recommended)**

```bash
brew tap erictli/tap
brew install --cask erictli/tap/scratch
```

**Manual Download**

1. Download the latest `.dmg` from [Releases](https://github.com/erictli/scratch/releases)
2. Open the DMG and drag Scratch to Applications
3. Open Scratch from Applications

### Windows

Pre-built Windows binaries are not yet available. To run on Windows, build from source (see below).

### From Source

**Prerequisites:** Node.js 18+, Rust 1.70+

**macOS:** Xcode Command Line Tools · **Windows:** WebView2 Runtime (pre-installed on Windows 11)

```bash
git clone https://github.com/erictli/scratch.git
cd scratch
npm install
npm run tauri dev      # Development
npm run tauri build    # Production build
```

## Keyboard Shortcuts

Scratch is designed to be usable without a mouse. Here are the essentials to get started:

| Shortcut      | Action              |
| ------------- | ------------------- |
| `Cmd+N`       | New note            |
| `Cmd+P`       | Command palette     |
| `Cmd+Shift+C` | Copy as...          |
| `Cmd+R`       | Reload current note |
| `Cmd+,`       | Open settings       |
| `Cmd+\`       | Toggle sidebar      |
| `Cmd+B/I`     | Bold/Italic         |
| `↑/↓`         | Navigate notes      |

**Note:** On Windows, use `Ctrl` instead of `Cmd` for all shortcuts.

Many more shortcuts and features are available in the app—explore via the command palette (`Cmd+P` / `Ctrl+P`) or view the full reference in Settings → Shortcuts.

## MCP Server

Scratch includes a built-in [Model Context Protocol](https://modelcontextprotocol.io/) server that lets AI agents read, create, search, and manage your notes programmatically. The server starts automatically with the app on `http://localhost:3921/mcp`.

### Connecting Claude Code

Add to `~/.claude/claude_code_config.json`:

```json
{
  "mcpServers": {
    "scratch": {
      "type": "url",
      "url": "http://localhost:3921/mcp"
    }
  }
}
```

Claude Code will auto-discover all available tools on next launch.

### Available Tools (24)

**Notes**

| Tool | Description |
|------|-------------|
| `scratch_list_notes` | List notes with folder filtering and recursive listing |
| `scratch_read_note` | Read full markdown content by note ID |
| `scratch_create_note` | Create a new empty note |
| `scratch_update_note` | Update note content (title derived from first heading) |
| `scratch_delete_note` | Delete a note permanently |
| `scratch_append_to_note` | Append content to an existing note |
| `scratch_search_notes` | Full-text search across all notes (Tantivy) |
| `scratch_get_info` | Get notes folder path, count, and settings |

**Folders & Files**

| Tool | Description |
|------|-------------|
| `scratch_list_folders` | List subfolders in the notes directory |
| `scratch_create_folder` | Create folders (supports nested paths) |
| `scratch_move_note` | Move a note between folders |
| `scratch_list_directory` | List files and subdirectories with metadata |
| `scratch_read_file` | Read raw file contents (any file type) |

**Power Search & Replace**

| Tool | Description |
|------|-------------|
| `scratch_find` | Exact, fuzzy (Levenshtein), or regex search with context lines |
| `scratch_replace_in_note` | Find and replace with first/all/regex modes |

**Kanban Stories**

| Tool | Description |
|------|-------------|
| `stories_epics_list` | List epics (E-####-slug folders) |
| `stories_boards_get` | Get kanban board with 6 lanes per epic |
| `stories_list` | List stories with filters (epic, status, tag, owner, text) |
| `stories_get` | Get story with frontmatter, body, and etag |
| `stories_create` | Create a story with auto-generated ID |
| `stories_update` | Update story metadata/body with optimistic locking |
| `stories_move` | Move story between kanban lanes |
| `stories_search` | Search across stories with snippet extraction |
| `stories_validate` | Validate story against schema and conventions |

### Kanban Stories

Scratch can manage epics and stories as a Git-backed markdown kanban board. Stories are markdown files with YAML frontmatter stored in a folder structure:

```
notes/product/
  E-0001-payments/
    stories/
      S-0001-01-user-onboarding.md
      S-0001-02-kyc-webhook.md
  E-0002-todo-app/
    stories/
      S-0002-01-task-crud.md
```

Each story file has YAML frontmatter for metadata and markdown body for details:

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

**Statuses:** Backlog, Ready, In Progress, In Review, Done, Blocked

**Concurrency:** All mutations use etag-based optimistic locking. Get the etag from `stories_get`, pass it to `stories_update` or `stories_move`. If the file changed since you read it, you get a `CONFLICT` error with the current etag.

**Audit log:** All story mutations are logged to `.scratch/audit/events.jsonl` in the notes folder.

**Story card rendering:** When you open a story file in Scratch, the YAML frontmatter renders as a formatted card with status dropdown, colored tags, avatar, and story point indicator — the markdown body edits normally below.

## Built With

[Tauri](https://tauri.app/) · [React](https://react.dev/) · [BlockNote](https://www.blocknotejs.org/) · [Tailwind CSS](https://tailwindcss.com/) · [Tantivy](https://github.com/quickwit-oss/tantivy)

## License

MIT

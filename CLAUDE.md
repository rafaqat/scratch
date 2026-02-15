# Scratch - Development Guide

## Story & Roadmap Tracking (MANDATORY)

**CRITICAL:** When implementing stories from `product/E-*/stories/`, you MUST use the `/story` skill to track progress. Never implement story work without updating the story files.

### Workflow for Every Story

1. **Before coding:** Run `/story S-XXXX-XX` to claim the story (sets status to "In Progress", logs started_at)
2. **During implementation:** Check off acceptance criteria `- [x]` in the story file as you complete them
3. **After commits:** Log commit hashes to the story frontmatter and Activity Log
4. **When done:** Mark the story "Done" with completed_at date and Results section
5. **Always:** Update the epic README.md changelog and feature status tables

### When Implementing Multiple Stories

Even when doing bulk implementation, claim and complete stories ONE AT A TIME through the `/story` skill. Do not use `TaskCreate` as a substitute — that's ephemeral and session-only. The `product/` folder is the source of truth.

### Quick Reference

- `/roadmap` — Show board with story statuses
- `/roadmap next` — Find highest-priority unblocked story
- `/story S-XXXX-XX` — Claim and work on a specific story
- `/roadmap progress` — Show completion stats

## Project Overview

Scratch is a cross-platform markdown note-taking app for macOS and Windows, built with Tauri v2 (Rust backend) + React/TypeScript/Tailwind (frontend) + BlockNote (block-based editor) + Tantivy (full-text search).

## Tech Stack

- **Backend**: Tauri v2, Rust
- **Frontend**: React 19, TypeScript, Tailwind CSS v4
- **Editor**: BlockNote (Notion-style block editor with markdown support)
- **Search**: Tantivy full-text search engine
- **File watching**: notify crate with custom debouncing

## Commands

```bash
npm run dev          # Start Vite dev server only
npm run build        # Build frontend (tsc + vite)
npm run tauri dev    # Run full app in development mode
npm run tauri build  # Build production app
```

## Building for Release

**Before building:** Bump version in `package.json` and `src-tauri/tauri.conf.json`

### macOS Build (Universal Binary)

Builds a universal binary supporting both Intel and Apple Silicon Macs.

**Prerequisites:**
```bash
rustup target add x86_64-apple-darwin
rustup target add aarch64-apple-darwin
```

**Build Steps:**

1. Set up environment (credentials in `.env.build`):
   ```bash
   source .env.build
   PATH="/usr/bin:$PATH"  # Ensure system xattr is used, not Python's
   ```

2. Clean previous build and build universal binary:
   ```bash
   rm -rf src-tauri/target/release/bundle
   npm run tauri build -- --target universal-apple-darwin
   ```

3. Submit for notarization:
   ```bash
   xcrun notarytool submit src-tauri/target/release/bundle/macos/Scratch.zip \
     --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID" --wait
   ```

4. Staple the notarization ticket:
   ```bash
   xcrun stapler staple src-tauri/target/release/bundle/macos/Scratch.app
   ```

5. Create DMG with Applications symlink:
   ```bash
   mkdir -p /tmp/scratch-dmg
   cp -R src-tauri/target/release/bundle/macos/Scratch.app /tmp/scratch-dmg/
   ln -sf /Applications /tmp/scratch-dmg/Applications
   hdiutil create -volname "Scratch" -srcfolder /tmp/scratch-dmg -ov -format UDZO \
     src-tauri/target/release/bundle/dmg/Scratch_VERSION_universal.dmg
   rm -rf /tmp/scratch-dmg
   ```

6. Get checksum and upload: `shasum -a 256 <dmg_path>`

7. The build also generates an updater manifest at:
   ```
   src-tauri/target/release/bundle/macos/Scratch.app.tar.gz.sig
   src-tauri/target/release/bundle/macos/Scratch.app.tar.gz
   ```
   These are used by the auto-updater (see [Publishing a Release](#publishing-a-release) below).

### Windows Build

Builds `.msi` installer and `.exe` setup for Windows.

**Prerequisites:**
- Windows machine or VM
- Rust toolchain installed (`rustup`)
- WebView2 Runtime (usually pre-installed on Windows 11)

**Build Steps:**

1. Clean previous build:
   ```powershell
   Remove-Item -Recurse -Force src-tauri\target\release\bundle
   ```

2. Build installers:
   ```powershell
   npm run tauri build
   ```

3. Outputs will be in `src-tauri/target/release/bundle/`:
   - `msi/Scratch_VERSION_x64_en-US.msi` - MSI installer
   - `nsis/Scratch_VERSION_x64-setup.exe` - NSIS setup (recommended for distribution)

4. Get checksum and upload:
   ```powershell
   Get-FileHash .\src-tauri\target\release\bundle\nsis\Scratch_VERSION_x64-setup.exe -Algorithm SHA256
   ```

**Notes:**
- The NSIS setup (`.exe`) automatically downloads WebView2 if needed
- For x86 support, add `--target i686-pc-windows-msvc` (requires `rustup target add i686-pc-windows-msvc`)

### Publishing a Release

The app checks for updates via the Tauri updater plugin, which fetches `latest.json` from GitHub releases.

**How it works:**
- On startup (after 3s delay) and manually via Settings → General → "Check for Updates", the app fetches:
  `https://github.com/erictli/scratch/releases/latest/download/latest.json`
- The updater compares the version in `latest.json` to the running app version
- If newer, a toast appears with an "Update Now" button that downloads and installs the update

**Creating `latest.json`:**

After building for each platform, create a `latest.json` file with this structure:

```json
{
  "version": "VERSION",
  "notes": "Release notes here",
  "pub_date": "2025-01-01T00:00:00Z",
  "platforms": {
    "darwin-universal": {
      "signature": "CONTENTS_OF .app.tar.gz.sig FILE",
      "url": "https://github.com/erictli/scratch/releases/download/vVERSION/Scratch.app.tar.gz"
    },
    "windows-x86_64": {
      "signature": "CONTENTS_OF .nsis.zip.sig FILE",
      "url": "https://github.com/erictli/scratch/releases/download/vVERSION/Scratch_VERSION_x64-setup.nsis.zip"
    }
  }
}
```

- The `signature` values come from the `.sig` files generated alongside the build artifacts
- The `url` values should point to the release assets on GitHub

**Upload to GitHub release:**

1. Create a GitHub release tagged `vVERSION` (e.g., `v0.4.0`)
2. Upload these assets:
   - `latest.json` (the updater manifest)
   - macOS: `Scratch_VERSION_universal.dmg` (for manual download) and `Scratch.app.tar.gz` (for auto-update)
   - Windows: `Scratch_VERSION_x64-setup.exe` (for manual download) and the NSIS `.zip` (for auto-update)
3. The updater endpoint resolves to the **latest** release's `latest.json` automatically via GitHub's `/releases/latest/download/` URL pattern

**Updater config** is in `src-tauri/tauri.conf.json` under `plugins.updater`, including the public key and endpoint URL.

## Project Structure

```
scratch/
├── src/                            # React frontend
│   ├── components/
│   │   ├── editor/                 # BlockNote editor
│   │   │   └── Editor.tsx          # Main editor with auto-save, copy-as, block editing
│   │   ├── layout/                 # Sidebar, main layout
│   │   │   ├── Sidebar.tsx         # Note list, search, git status
│   │   │   └── FolderPicker.tsx    # Initial folder selection dialog
│   │   ├── notes/
│   │   │   └── NoteList.tsx        # Scrollable note list with context menu
│   │   ├── command-palette/
│   │   │   └── CommandPalette.tsx  # Cmd+P for notes & commands
│   │   ├── settings/               # Settings page
│   │   │   ├── SettingsPage.tsx    # Tabbed settings interface
│   │   │   ├── GeneralSettingsSection.tsx       # Notes folder picker
│   │   │   ├── AppearanceSettingsSection.tsx    # Theme & typography
│   │   │   ├── GitSettingsSection.tsx           # Git config & remote
│   │   │   └── ShortcutsSettingsSection.tsx     # Keyboard shortcuts reference
│   │   ├── ai/                     # AI editing components
│   │   │   ├── AiEditModal.tsx     # AI prompt input modal
│   │   │   └── AiResponseToast.tsx # AI response display with undo
│   │   ├── git/
│   │   │   └── GitStatus.tsx       # Floating git status with commit UI
│   │   ├── ui/                     # Shared UI components
│   │   │   ├── Button.tsx          # Button variants (default, ghost, outline, etc.)
│   │   │   ├── Input.tsx           # Form input
│   │   │   ├── Tooltip.tsx         # Radix UI tooltip wrapper
│   │   │   └── index.tsx           # ListItem, CommandItem, IconButton exports
│   │   └── icons/                  # SVG icon components (30+ icons)
│   │       └── index.tsx
│   ├── context/                    # React context providers
│   │   ├── NotesContext.tsx        # Note CRUD, search, file watching
│   │   ├── GitContext.tsx          # Git operations wrapper
│   │   └── ThemeContext.tsx        # Theme mode & typography settings
│   ├── lib/                        # Utility functions
│   │   └── utils.ts                # cn() for className merging
│   ├── services/                   # Tauri command wrappers
│   │   ├── notes.ts                # Note management commands
│   │   ├── git.ts                  # Git commands
│   │   └── ai.ts                   # AI/Claude Code CLI commands
│   ├── types/
│   │   └── note.ts                 # TypeScript types
│   ├── App.tsx                     # Main app component
│   └── main.tsx                    # React root & providers
├── src-tauri/                      # Rust backend
│   ├── src/
│   │   ├── lib.rs                  # Tauri commands, state, file watcher, search
│   │   ├── mcp.rs                  # MCP server (JSON-RPC, tool definitions, handlers)
│   │   ├── mcp_instructions.md     # MCP instructions sent to clients (compiled in)
│   │   ├── git.rs                  # Git CLI wrapper (8 commands)
│   │   ├── stories.rs              # Stories/kanban engine
│   │   ├── database.rs             # Database engine (schema, query, CRUD)
│   │   ├── plugins.rs              # Plugin manifest loading & tool dispatch
│   │   └── webhooks.rs             # Webhook receiver for plugins
│   ├── capabilities/default.json   # Tauri permissions config
│   └── Cargo.toml                  # Rust dependencies
└── package.json                    # Node dependencies & scripts
```

## Key Patterns

### Tauri Commands

All backend operations go through Tauri commands defined in `src-tauri/src/lib.rs`. Frontend calls them via `invoke()` from `@tauri-apps/api/core`.

### State Management

- `NotesContext` manages all note state, CRUD operations, and search
- `ThemeContext` handles light/dark/system theme and editor typography settings

### Settings

- **App config** (notes folder path): `{APP_DATA}/config.json`
- **Per-folder settings**: `{NOTES_FOLDER}/.scratch/settings.json`

The settings page provides UI for:

- Theme mode (light/dark/system)
- Editor typography (font family, size, line height, bold weight)
- Git integration (optional)
- Keyboard shortcuts reference

Power users can edit the settings JSON directly to customize colors.

### Editor

BlockNote block-based editor with Notion-style editing:

**Built-in Features (via BlockNote):**
- Slash menu for block insertion
- Drag-and-drop block reordering
- Formatting toolbar
- Link, image, table, task list blocks
- Markdown paste detection and parsing

**Custom Features:**
- Auto-save with 500ms debounce
- Copy-as menu (Markdown/Plain Text/HTML) via `Cmd+Shift+C`
- Image insertion from disk (copies to `.assets/` folder)
- External file change detection with auto-reload
- "Last saved" status indicator
- Unsaved changes spinner
- AI editing with Claude Code CLI integration

**Note:** `dragDropEnabled` is set to `false` in `tauri.conf.json` because Tauri v2 intercepts HTML5 drag events by default, which prevents BlockNote's block drag-and-drop from working.

### Component Architecture

**Context Providers:**
- `NotesContext` - Dual context pattern (data/actions separated for performance)
  - Data: notes, selectedNoteId, currentNote, searchResults, etc.
  - Actions: selectNote, createNote, saveNote, deleteNote, search, etc.
  - Race condition protection during note switches
  - Recently saved note tracking to ignore own file watcher events
- `GitContext` - Git operations with loading states and error handling
  - Auto-refresh status on file changes (1000ms debounce)
- `ThemeContext` - Theme mode and typography with CSS variable application

**Key Components:**
- `Editor` - Main BlockNote editor with auto-save, copy-as, image insertion
- `CommandPalette` - Cmd+P for quick actions and note search
- `GitStatus` - Floating commit UI in sidebar
- `NoteList` - Scrollable list with context menu and smart date formatting
- `SettingsPage` - Tabbed settings (General, Appearance, Git, Shortcuts)
- `AiEditModal` - AI prompt input for Claude Code CLI integration
- `AiResponseToast` - AI response display with markdown parsing and undo button

### Tauri Commands

**Note Management:** `list_notes`, `read_note`, `save_note`, `delete_note`, `create_note`

**Configuration:** `get_notes_folder`, `set_notes_folder`, `get_settings`, `update_settings`

**Search:** `search_notes`, `rebuild_search_index` (Tantivy full-text with prefix fallback)

**File Watching:** `start_file_watcher` (notify crate with 500ms debounce per file)

**Git:** `git_is_available`, `git_get_status`, `git_init_repo`, `git_commit`, `git_push`, `git_add_remote`, `git_push_with_upstream`

**AI/Claude Code:** `ai_check_claude_cli`, `ai_execute_claude` (shell execution with Claude Code CLI)

**Utilities:** `copy_to_clipboard`, `copy_image_to_assets`, `save_clipboard_image`

**UI Helpers:** `open_folder_dialog`, `reveal_in_file_manager`, `open_url_safe` (URL scheme validated)

### Search Implementation

The app uses **Tantivy** (Rust full-text search engine) with:
- Schema: id (string), title (text), content (text), modified (i64)
- Full-text search with prefix query fallback (query*)
- Returns top 20 results with scoring
- Fallback to cache-based search (title/preview matching) if Tantivy fails

### File Watching

Uses `notify` crate with custom debouncing:
- 500ms debounce per file to batch rapid changes
- Emits "file-change" events to frontend
- Frontend filters events for currently edited note to prevent conflicts
- Debounce map cleanup (5 second retention)

### Permissions

Tauri v2 uses capability-based permissions. Add new permissions to `src-tauri/capabilities/default.json`. Core permissions use `core:` prefix (e.g., `core:menu:default`).

Current capabilities include:
- File system read/write for notes folder
- Dialog (folder picker)
- Clipboard
- Shell (for git commands)
- Window management

## Keyboard Shortcuts

- `Cmd+N` - New note
- `Cmd+P` - Command palette
- `Cmd+Shift+C` - Copy as (Markdown/Plain Text/HTML)
- `Cmd+R` - Reload current note (pull external changes)
- `Cmd+,` - Open settings
- `Cmd+1/2/3` - Switch settings tabs (General/Appearance/Shortcuts)
- `Cmd+\` - Toggle sidebar
- `Cmd+B/I` - Bold/Italic
- Arrow keys - Navigate note list (when focused)

**Note:** On Windows and Linux, use `Ctrl` instead of `Cmd` for all shortcuts. Full reference available in Settings → Shortcuts tab.

## Notes Storage

Notes are stored as markdown files in a user-selected folder. Filenames are derived from the note title (sanitized for filesystem safety). The first `# Heading` in the content becomes the note title displayed in the sidebar.

### File Watching

The app watches the notes folder for external changes (e.g., from AI agents or other editors). When a file changes externally, the sidebar updates automatically and the editor reloads the content if the current note was modified.

## Development Philosophy

### Code Quality
- Clean, minimal codebase with low technical debt
- Proper React patterns (contexts, hooks, memoization)
- Type-safe with TypeScript throughout
- No commented-out code or TODOs in production code

### Performance Optimizations
- Auto-save debouncing (300ms)
- Search debouncing (150ms in sidebar)
- File watcher debouncing (500ms per file)
- Git status refresh debouncing (1000ms)
- React.memo for expensive components (NoteList items)
- useCallback/useMemo for performance-critical paths

### User Experience
- Native macOS feel with drag region
- Keyboard-first navigation
- Smart date formatting (Today, Yesterday, X days ago)
- Inline editing (commits)
- Non-blocking operations (async everything)
- Error handling with user-friendly messages

## MCP Server

Scratch includes a built-in MCP (Model Context Protocol) server that exposes notes, databases, and stories to AI agents like Claude Code.

### Configuration

- **Default port**: 3921 (configurable in settings)
- **Enable/disable**: Settings → General → MCP Server toggle
- **Protocol**: HTTP JSON-RPC 2.0 at `POST http://127.0.0.1:{port}/mcp`
- **Health check**: `GET http://127.0.0.1:{port}/health`
- **Implementation**: `src-tauri/src/mcp.rs`
- **Instructions file**: `src-tauri/src/mcp_instructions.md` (compiled into the binary via `include_str!`, sent to clients on `initialize`)

### Tool Categories (30+ tools)

**Notes CRUD (7):** `scratch_list_notes`, `scratch_read_note`, `scratch_create_note`, `scratch_update_note`, `scratch_delete_note`, `scratch_append_to_note`, `scratch_get_info`

**Search & Replace (3):** `scratch_search_notes` (Tantivy full-text), `scratch_find` (exact/fuzzy/regex), `scratch_replace_in_note`

**Folders & Files (5):** `scratch_list_folders`, `scratch_create_folder`, `scratch_move_note`, `scratch_list_directory`, `scratch_read_file`

**Stories/Kanban (9):** `stories_epics_list`, `stories_boards_get`, `stories_list`, `stories_get`, `stories_create`, `stories_update`, `stories_move`, `stories_search`, `stories_validate`

**Databases (7):** `db_list`, `db_get_schema`, `db_query`, `db_insert_row`, `db_update_row`, `db_delete_row`, `db_create`

**Plugin tools**: Dynamically loaded from `.scratch/plugins/` manifests, prefixed with `plugin_`

### Key Patterns

- **Note IDs**: Filename without `.md`. Path-based for subfolders: `projects/todo`
- **Database IDs**: Folder path relative to notes root: `my-tasks`
- **Concurrency control**: Stories and database rows use etag-based optimistic concurrency. Read → get etag → pass etag on update/move. CONFLICT error on mismatch.
- **Audit log**: Story operations logged to `.scratch/audit/events.jsonl`

### Adding New MCP Tools

1. Add tool definition JSON to `get_tools()` in `mcp.rs`
2. Add `async fn tool_xxx()` implementation in `mcp.rs`
3. Add match arm in `handle_tools_call()` dispatcher
4. Document in `mcp_instructions.md` (this gets sent to MCP clients)
5. Rebuild (`npm run tauri dev` will auto-recompile)

## Recent Development

Recent commits show continuous improvement:
- AI editing with Claude Code CLI integration (invoke Claude to edit notes)
- Table editing support with context menu operations
- Keyboard shortcuts reference page in settings
- Migrated editor from TipTap to BlockNote (Notion-style block editing)
- Yellow selection highlight and UI polish
- Git integration with push/remote management
- Settings UI simplification
- Copy-as feature (Markdown/Plain/HTML)
- Task list styling improvements
- Cross-platform keyboard support (Ctrl on non-Mac)

# Quick Capture Plugin

Append-only tool for fast note capture from CLI and AI agents. Follows the "inbox" pattern -- everything goes into a single `inbox.md` note for later processing.

## Setup

1. Copy the plugin to your notes folder:
   ```bash
   cp plugins/quick-capture.yaml /path/to/notes/.scratch/plugins/
   ```

2. Enable the plugin in Scratch Settings > MCP > Plugins.

3. Set the `SCRATCH_NOTES_FOLDER` environment variable to your notes folder path, or the plugin will use the current working directory as fallback.

## Tools

### `capture`
Append a timestamped entry to the inbox note. Creates the inbox if it does not exist.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `text` | string | yes | Text to capture |
| `tag` | string | no | Optional tag to categorize the entry |

**Examples (via MCP):**
```json
{ "text": "Look into Tantivy fuzzy search support" }
{ "text": "Bug: sidebar flickers on theme change", "tag": "bug" }
{ "text": "Add export to PDF feature", "tag": "idea" }
```

Produces in `inbox.md`:
```markdown
# Inbox

Quick captures from CLI and agents.

- 2026-02-13 14:30 Look into Tantivy fuzzy search support
- 2026-02-13 14:31 [bug] Bug: sidebar flickers on theme change
- 2026-02-13 14:32 [idea] Add export to PDF feature
```

### `view_inbox`
Read the current inbox contents. No parameters required.

### `clear_inbox`
Reset the inbox to its header only, clearing all captured entries. No parameters required.

## Permissions

- `shell:execute` — Runs shell commands for timestamp generation and file I/O
- `notes:write` — Creates and appends to the inbox note
- `notes:read` — Reads the inbox note

## Workflow

The inbox pattern works well with AI agents:

1. **Capture** -- agents and CLI tools dump thoughts, links, and tasks into the inbox throughout the day
2. **Review** -- periodically open `inbox.md` in Scratch and sort items into proper notes
3. **Clear** -- once processed, clear the inbox to start fresh

This keeps the friction of capturing ideas as low as possible while maintaining a single collection point.

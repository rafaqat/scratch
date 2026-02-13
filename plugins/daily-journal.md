# Daily Journal Plugin

Automatically create and append to daily journal notes in a `journal/` subfolder.

## Setup

1. Copy the plugin to your notes folder:
   ```bash
   cp plugins/daily-journal.yaml /path/to/notes/.scratch/plugins/
   ```

2. Enable the plugin in Scratch Settings > MCP > Plugins.

3. Set the `SCRATCH_NOTES_FOLDER` environment variable to your notes folder path, or the plugin will use the current working directory as fallback.

## Tools

### `append`
Append a timestamped entry to today's journal. Creates the journal note if it does not exist.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `text` | string | yes | Text to append |

Entries are formatted as `- HH:MM your text` and appended to `journal/YYYY-MM-DD.md`.

**Example (via MCP):**
```json
{ "text": "Finished refactoring the search module" }
```

Produces in `journal/2026-02-13.md`:
```markdown
# Journal 2026-02-13

- 14:30 Finished refactoring the search module
```

### `read_today`
Read the current day's journal note. No parameters required.

### `read_date`
Read a journal note for a specific date.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `date` | string | yes | Date in `YYYY-MM-DD` format |

## Permissions

- `shell:execute` — Runs shell commands for date generation and file I/O
- `notes:write` — Creates journal notes
- `notes:read` — Reads journal notes
- `folders:write` — Creates the `journal/` subfolder

## File Structure

```
notes/
  journal/
    2026-02-10.md
    2026-02-11.md
    2026-02-12.md
    2026-02-13.md
```

Each journal note has a `# Journal YYYY-MM-DD` heading and timestamped bullet entries.

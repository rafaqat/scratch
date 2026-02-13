# GitHub Issue Sync Plugin

Fetch and sync GitHub issues as Scratch notes using the `gh` CLI.

## Setup

1. Install the [GitHub CLI](https://cli.github.com/) and authenticate:
   ```bash
   brew install gh
   gh auth login
   ```

2. Copy the plugin to your notes folder:
   ```bash
   cp plugins/github-issue-sync.yaml /path/to/notes/.scratch/plugins/
   ```

3. Enable the plugin in Scratch Settings > MCP > Plugins.

## Tools

### `list_issues`
List open issues from a repo as a one-line-per-issue summary.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `repo` | string | yes | Owner/repo (e.g. `user/project`) |
| `limit` | integer | no | Max issues to return (default: 30) |

**Example (via MCP):**
```json
{ "repo": "erictli/scratch", "limit": 10 }
```

### `sync_issues`
Fetch open issues and produce a formatted markdown summary suitable for saving as a note. Includes issue bodies, labels, and assignees.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `repo` | string | yes | Owner/repo |
| `limit` | integer | no | Max issues to fetch (default: 30) |

### `fetch_issue`
Fetch a single issue with full details including comments.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `repo` | string | yes | Owner/repo |
| `number` | integer | yes | Issue number |

## Permissions

- `shell:execute` — Runs `gh` CLI commands
- `notes:write` — Creates/updates issue notes

## How It Works

All tools shell out to the `gh` CLI with JSON output and `jq` formatting. The `sync_issues` tool produces a single markdown document with all issues, suitable for saving as a note via the Scratch note API.

To create a note from the sync output, pipe the result through a `note_op` create tool or use an AI agent to save it.

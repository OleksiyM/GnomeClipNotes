# GnomeClipNotes Companion for IDEs

A lightweight, local-first companion IDE extension for **GnomeClipNotes** (compatible with VSCodium, Antigravity IDE, VS Code, and Open VSX).

## Features

- **Folder & Note Browsing**: Sidebar view container (`Notes & Folders`) showing all GnomeClipNotes collections and items.
- **RAM Projection (`file://` `.md`)**: Opens notes as ordinary local Markdown files in high-speed RAM session directories (`$XDG_RUNTIME_DIR/gnome-clip-notes-ide/<pid>/` or fallback `/tmp/gnome-clip-notes-ide-<uid>/<pid>/`) with strict permissions (`0700` directories, `0600` files).
- **First-Class AI Agent & Tooling Support**: Because notes are projected onto the filesystem as standard `.md` files, external CLI tools (`ripgrep`), IDE spellcheckers, Markdown previews, and IDE AI agents have full direct read and write access.
- **Dual Save Triggers & Echo-Loop Prevention**: Monitors both editor buffer saves (`onDidSaveTextDocument`) and direct disk filesystem writes (`createFileSystemWatcher`), with per-file debouncing (default 250ms) and self-write content-hash tracking to suppress echo loops.
- **Safety Net & Conflict Resolution**:
  - `CONFLICT`: Revision token mismatch triggers side-by-side diff view between the DB version and local RAM file. Local RAM files are never silently overwritten.
  - `CONTENT_TOO_LARGE`: Files exceeding 1 MiB display an error alert and keep the RAM file intact so edits can be trimmed and saved.
  - `INVALID_CONTENT`: Empty files or null bytes are blocked with an alert while preserving the local draft.
- **Safe Session Lifecycle & Orphan Cleanup**:
  - Closing an individual editor tab **never** deletes the RAM file, keeping AI agent context intact across panes.
  - On activation, orphaned session directories from dead PIDs are safely identified (`kill(pid, 0)`) and removed.
  - On deactivation, the current session directory is cleaned up and the bridge process is cleanly terminated.

## Protocol Architecture

The extension communicates with the native GnomeClipNotes backend exclusively over standard input/output using line-delimited JSON (JSONL):

```
IDE Extension (TypeScript)  <--- JSONL stdin/stdout --->  gnome-clip-notes --bridge (Rust)
```

No native SQLite, C bindings, or D-Bus libraries are bundled into the extension.

## Configuration

| Setting | Type | Default | Description |
|---|---|---|---|
| `gnomeClipNotes.bridgePath` | `string` | `"gnome-clip-notes"` | Path to the `gnome-clip-notes` binary. |
| `gnomeClipNotes.debounceMs` | `number` | `250` | Debounce delay in milliseconds before sending edits to the bridge. |

## Commands

- `GnomeClipNotes: Refresh Notes` (`gnomeClipNotes.refresh`)
- `GnomeClipNotes: Open Note` (`gnomeClipNotes.openNote`)
- `GnomeClipNotes: Clear Notes Cache` (`gnomeClipNotes.clearCache`)

## Development

```bash
# Install dependencies
npm install

# Compile TypeScript
npm run compile

# Run test suite
npm test

# Package VSIX
npm run package
```

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

## Installation

### Requirements

- **GnomeClipNotes `1.6.0` or newer** installed locally (providing `gnome-clip-notes --bridge`), or a built `gnome-clip-notes` binary reachable via `gnomeClipNotes.bridgePath`.
- A compatible IDE: **Antigravity IDE**, **VSCodium**, or **VS Code** (`>= 1.85.0`).

### Install from GitHub Releases (`.vsix`)

1. Download `gnome-clip-notes-ide-<VERSION>.vsix` (and optionally `SHA256SUMS` / `gnome-clip-notes-ide-<VERSION>.vsix.sigstore.json`) from [GitHub Releases](https://github.com/OleksiyM/GnomeClipNotes/releases).
2. Verify the checksum if desired:

   ```sh
   sha256sum --check --ignore-missing SHA256SUMS
   ```

3. Install the `.vsix` in your IDE:
   - **GUI:** Open the **Extensions** view (`Ctrl+Shift+X`), open the `...` menu, choose **Install from VSIX...**, and select the downloaded `.vsix` file.
   - **CLI:**

     ```sh
     # Antigravity IDE
     agy --install-extension gnome-clip-notes-ide-<VERSION>.vsix
     # VSCodium
     codium --install-extension gnome-clip-notes-ide-<VERSION>.vsix
     # VS Code
     code --install-extension gnome-clip-notes-ide-<VERSION>.vsix
     ```

4. If `gnome-clip-notes` is not in your `PATH` (for example, when installed under `~/Applications/GnomeClipNotes/gnome-clip-notes` or running a local Cargo build), set **`gnomeClipNotes.bridgePath`** in your IDE settings to the full binary path.

### Build and install from source

```bash
cd companion-ide
npm ci
npm test
npm run package
```

Then install the generated `.vsix` file using the GUI or CLI steps above.

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

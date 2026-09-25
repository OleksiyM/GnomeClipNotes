# GnomeClipNotes — project map

Start here, then read only the linked contract for the area being changed.
Keep this file compact: decisions and reasons here, implementation detail in
[docs/](docs/README.md), session history in optional `.private/`.
If `.private/README.md` exists, consult its index; for release work also read and
update `.private/release-plan.md`. A public checkout remains self-contained.

## Product and engineering direction

**Capture → Curate → Organize → Export → Develop → Reuse.** GNOME is the
environment, Clip the source/tool, Notes the retained result. History is temporary;
Notes and folders are deliberate collections. Export continues the workflow in
other tools, not an all-in-one knowledge platform. [Product rationale](docs/product.md).

Local-first plain text/Markdown, no account/cloud sync; capture filtering is not
a password vault. Public docs and the 1.0 UI are English. Prefer small, complete
improvements. Simple installation with a safety net, not another product inside
the product. Do not turn a local bug fix into a rewrite or infrastructure project.

## Architecture and why it is split

- **Rust GTK4/libadwaita app:** normal windows, settings and service commands.
  `src/lib.rs` composes `State`, CLI, D-Bus and refresh notifications.
  Library is a separate window, not the clipboard overlay.
- **GNOME Shell extension:** Wayland clipboard, global keys, paste/focus,
  indicator and overlay. D-Bus only, never direct SQLite access. Shell-specific
  code stays here; the overlay is **St/Clutter + Shell CSS**, not HTML/WebKit.
- **Shared Store:** SQLite operations/migrations in `src/store.rs`. Both daemon
  and Full editors open connections (WAL, busy timeout); the daemon is not the
  only writer. Full saves send `EditorSaved`; the service emits `Changed` so
  Library/overlay refresh. Preserve this propagation when adding mutations.
- **Two previews, intentionally:** Native uses bounded GTK blocks in the daemon;
  Full uses pulldown-cmark → filtered HTML → WebKitGTK in a separate editor.
  A per-editor systemd *service* cleans up the entire cgroup, including WebKit
  children. One lazy WebView survives tab switches; closing the window releases
  it. This bounds persistent memory without losing unsaved buffers on tab changes.
  Do not embed WebKit in the daemon or grow Native into a browser engine.

Details: [architecture](docs/architecture.md), [preview decisions](docs/preview.md).

## Change routing

| Task | Entry points | Read first |
| --- | --- | --- |
| Capture/paste/overlay/keys | `extension/extension.js`, `extension/clipboardFormats.js`, `extension/datePicker.js`; D-Bus in `src/lib.rs` | [Privacy](docs/privacy.md), [interface](docs/interface.md) |
| Data/retention/sources/settings | `src/store.rs`, `src/model.rs`, `src/preferences.rs` | [Architecture](docs/architecture.md), [privacy](docs/privacy.md) |
| Library/Native editor/selection | `src/ui.rs`, `src/library_selection.rs`, `src/item_shortcuts.rs`, `src/date_picker.rs`, `src/style.css` | [Interface](docs/interface.md) |
| Full editor/rendering | `src/editor_process.rs`, `src/bin/gnome-clip-notes-editor.rs`, `src/preview_{native,html,webkit}.rs` | [Preview](docs/preview.md) |
| Export/cleanup | `src/export.rs`, `src/export_ui.rs`, revision triggers in `src/store.rs` | [Export](docs/export.md) |
| Languages/update check | `src/i18n.rs`, `po/`, `scripts/translations.py`; `src/release_check.rs` | [Translations](docs/translations.md), [privacy](docs/privacy.md) |
| Install/package/release/site | Standard `install.sh` / `uninstall.sh`; Guided `guided-install.sh`, `scripts/install-release.py`; `scripts/package-release.py`, `.github/workflows/`, `site/` | [Distribution](docs/distribution.md), [installation](docs/installation.md), [site](site/README.md) |

Use the existing command/store path rather than implementing shared behavior
independently in both UIs. Delegate bounded work when useful; retain integration review.

## Invariants worth preserving

- **Retained data wins.** Group 0 is History, 1 Notes, custom IDs ≥2. Deduplication
  and expiry affect History only; expiry uses `copied_at`, not item origin.
  Normal folder deletion moves contents to Notes. Create-folder-and-move is
  atomic. Sources are DISTINCT existing item sources, not a registry to maintain.
- **Edits and selection survive refresh.** View opens the editor on Preview;
  reopening presents the existing window without replacing tab/buffer/selection.
  Dirty marker and Save share the saved-content comparison. Dirty close needs
  confirmation; crash/draft recovery is not implemented. Cards track item ID,
  not widget position/focus ring; shortcuts must respect focused text fields.
- **Export is not backup.** Export complete selected Notes/custom folders, never
  History or just the filtered page. Cleanup requires durable files, explicit
  counts/consent, file rechecks and transactional revision checks, preserving
  open/changed/new items.
  Random revision triggers catch edit-and-revert and reused IDs; timestamps alone
  cannot. Back up via SQLite API, never by copying a live WAL database file.
- **Content stays inert.** Item content is bounded to 1 MiB. Clipboard fallback keeps
  exact advertised MIME names and owner/generation/privacy checks; never combine
  partial transfers. Previews execute no author HTML/JS and load no external
  resources; clicked web links open externally. Unsupported Markdown stays readable.
- **Guided update exclusion spans processes.** Count unnamed/unsaved editors and pending
  Full launches. `src/update.rs` provides editor-aware shutdown; `update_lock.rs`
  supplies shared lifetime locks and installer exclusion. An acknowledgment or
  zero editor count does not prove process exit. Never replace the lock inode,
  use process-name guessing, or substitute generic `--quit` for this protocol.
  Rollback covers owned program files, not databases or system packages.
  Standard deliberately relies on users saving/closing editors and stopping the app;
  no forced process killing, dependency automation or rollback. Do not silently
  mix install methods; uninstall the previous one while retaining data first.
  Shell Service Start/Stop/Restart and CLI `--quit` respect editor lifetimes.
  Shell restart waits for the inspected bus owner's PID to exit; the indicator
  remains available. This is not an extension reload or installer lock protocol.
- **Identity is not storage.** App ID `io.github.OleksiyM.GnomeClipNotes`, extension
  `gnome-clip-notes@oleksiym.github.io`; XDG storage remains `gnome-clip-notes`.
  Renaming integration must not reset data/settings/consent. Migrations preserve
  data; older binaries need not understand newer schemas.
- **Native UI, whole messages.** Follow GNOME HIG/libadwaita, symbolic colors,
  readable light/dark and narrow layouts. Consult current official widget docs;
  native minimums remain GTK 4.12 / libadwaita 1.5 regardless of Rust crate version.
  Translate whole messages with named placeholders/plurals, never user content or
  Shell's global locale. No background update requests or automatic self-updater.

## Verify the boundary you changed

Never use the user's clipboard/database/session as a test fixture or restart their
app/extension without approval. Existing harnesses use isolated profiles; some
need desktop/systemd permission. [Test details](docs/manual-testing.md).

- Rust: `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`,
  `cargo test --locked`. UI: `cargo build --locked`, then `bash scripts/test-session.sh`
  (add `--export` for export); Shell/input: `bash scripts/test-shell.sh`.
- Process lifetime: `bash scripts/test-editor-process.sh` (also `--view`);
  update coordination: `bash scripts/test-update.sh`.
- Messages: `scripts/translations.py update` then `check`; locale behavior:
  `bash scripts/test-i18n-runtime.sh` / `bash scripts/test-i18n-ui.sh`.
- Packages: `bash scripts/build-package.sh`; installer changes also need
  `scripts/test-packaged-lifecycle.py` with real archives. Mocks/build success
  do not prove public downloads or Wayland compatibility.
- Docs-only: links, examples and `git diff --check`, not a full rebuild.
  Report what actually passed and which relevant checks remain unperformed.

## Release path

Repo: `OleksiyM/GnomeClipNotes`; [site](https://oleksiym.github.io/GnomeClipNotes/).
First public release: v1.0.0; earlier 0.x builds were private. Small reviewed
changes may go to `main` with CI; branches are optional. Stage explicit files,
never private notes. Use meaningful commit subjects for generated release notes.

1. Patch for fixes, minor for features, major for incompatibilities; no bump per
   dev commit. Update Cargo.toml, its Cargo.lock entry, extension `version-name`,
   gettext template and Changelog. Shell's integer `version` is not product SemVer.
2. Check CI and get approval for the exact tag/target. **Pushing `vX.Y.Z`
   publishes**, not drafts: checks → Fedora 44 x86_64/ARM64 archives → checksum/provenance
   → Release → Pages from that tag. Main alone publishes neither.
3. Verify released download/provenance, isolated install/launch and site. Do not
   rewrite published tags/assets. If only Pages fails, rerun Pages at that tag;
   [site instructions](site/README.md) cover setup/recovery.

Each `gnome-clip-notes-VERSION-ARCH.tar.gz` includes both binaries and extension;
`release.json` records exact platform. No separate extension ZIP or universal Linux
binary claim. SHA256 is mandatory; provenance uses capable `gh` automatically.
Standard explicitly skips missing/old gh; Guided refuses old gh and skips missing gh.
`--require-provenance` makes verification mandatory in either path;
failed verification never downgrades. Discuss changes to install/update behavior first.

## Where to go next — directions, not commitments

Useful increments: observed capture/paste/UX bugs, accessibility and keyboard
consistency, translations through the existing contract, measured performance
fixes. Languages need layout/font/locale checks; platforms need actual build and
runtime evidence. ARM64 is a native CI target since 1.1.0; desktop support remains
experimental. The original 1.0.0 release contains only an x86_64 archive.

Do not casually add a custom browser renderer, background updater, package-manager
framework, sync/import system or sweeping rewrite: these need a concrete problem
and a separate product decision. The archived WebKit prototype is historical
[reference](docs/preview.md), not a build dependency. When a decision changes,
update this map and the linked rationale; do not require rediscovery from old chats.

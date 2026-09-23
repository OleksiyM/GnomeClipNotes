# Note preview

The preview setting is `preview_mode` (`webkit` or `native`), defaulting to
`webkit` for both new and older configuration files. It applies when opening a
new editor, not to an already open document. Theme changes still propagate to
open editors, including workers, without changing their source or dirty state.

Card menus in the clipboard overlay and library expose **View** before **Edit**.
View opens the existing editor directly on Preview using the configured renderer;
viewing alone does not save, move, or modify the item. Edit opens new windows on
Editor. If the item already has an open window, either action presents that window
without switching tabs or replacing its buffer/selection, and shows a short
"Already open" toast. There is no confirmation dialog or duplicate editor.
Closed editor windows leave the registry during accepted close, not only when
their GTK objects are eventually finalized; retained references must not cause
the next View/Edit request to resurrect a destroyed window.

## Native

`src/preview_native.rs` converts pulldown-cmark events into small GTK display
blocks. Symbolic libadwaita colours provide light/dark styling. Supported scope:
heading hierarchy, emphasis/strikethrough, inline and fenced code, quotes, safe
HTTP(S) links, simple lists, task markers and separators. Unsupported table or
complex layout is shown as readable text, never a reason to discard source.
No syntax highlighter, HTML engine, image loader or custom Markdown grammar.
Selection/copy operates within a block. More than 512 blocks uses a complete
read-only plain-text view to bound widget overhead.

## Full

`src/preview_html.rs` filters pulldown-cmark events before its HTML writer.
`src/preview_webkit.rs`, referenced only by the helper binary, displays the HTML.
Raw HTML is escaped; images become placeholders; scripts/media/file access are
disabled; restrictive CSP and an ephemeral session block external resources.
The fail-closed custom proxy guard also blocks speculative preconnect. It is a
tested application guard, not an OS-level guarantee against engine vulnerabilities;
keep WebKit updated and rerun security regression checks on engine changes.
User-clicked HTTP(S) links go through GtkUriLauncher, not in-view navigation.

The service launches one editor via `systemd-run --user` as a transient **service**,
not a scope: Type=exec, KillMode=control-group, TimeoutStopSec=3s. This ensures that
when the main editor process exits, remaining processes in that service are
terminated. WebKit's own sandbox remains enabled. Only named, per-editor groups
are managed; there is no global process-name killing. This uses the
[systemd service lifecycle](https://github.com/systemd/systemd/blob/main/man/systemd.service.xml)
and [control-group kill policy](https://github.com/systemd/systemd/blob/main/man/systemd.kill.xml).

One WebView is created lazily per editor and reused across Editor/Preview switches.
The whole editor, rather than its text buffer alone, is isolated to preserve GTK
window integration and unsaved edits. The heavy renderer stays resident while
that window remains open, even on the Editor tab. It leaves when the window closes.
This avoids repeated sandbox-helper creation on each tab switch. Missing helper,
runtime or user systemd is reported; Native remains an explicit alternative.

Workers reuse normal save/naming/discard behavior and the shared SQLite store.
After a save, `EditorSaved` notifies the persistent service; the service emits
`Changed`. Existing workers are tracked so reopening an item presents its current
editor, even if the preview preference changed meanwhile. A GDK activation token
is passed when available. Settings are not rewritten by the worker.

## Verification

```sh
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
bash scripts/test-session.sh
bash scripts/test-editor-process.sh
bash scripts/test-editor-process.sh --view
```

The first GUI test covers Native styling and persistent radio choices. The second
uses private XDG/D-Bus/Wayland state but the actual user systemd manager for one
uniquely named temporary editor service. It verifies ten switches without losing
unsaved content, first save, visibility of the saved note from the daemon, Cancel/
Discard behavior, removal of the service cgroup and absence of sampled surviving
PIDs while the daemon and private session are still alive.
The `--view` variant opens a captured History item on Preview. Both worker cases
also exercise daemon-to-worker reactivation with an unsaved edit and selection.
The worker driver's selection test temporarily detaches the buffer from PRIMARY:
programmatic selection in its headless Wayland session can otherwise collapse
before activation. It tests buffer preservation, not PRIMARY clipboard ownership.

The earlier experiment under `prototypes/webkit-preview` remains as historical
measurements. Its intermediate lifecycle is not the production implementation.

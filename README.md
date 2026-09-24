# GnomeClipNotes

GnomeClipNotes is a native clipboard history and Markdown notes application for
GNOME on Wayland. The core is a Rust GTK4/libadwaita application backed by
SQLite; a small GNOME Shell extension handles Shell-only integration.

**Capture → Curate → Organize → Export → Develop → Reuse.**

Keep clipboard fragments that matter, collect them into Notes and folders, then
export readable Markdown to develop and reuse in your preferred tools. History
can expire automatically; deliberately saved material stays until you remove it.
No account, cloud storage or synchronization service is required.

This is preparation for the first public release, **1.0.0**, not a published
release. Fedora 44 x86_64 is the release build target; development use and
isolated app/Shell tests run on GNOME 50.4. The public release download-to-install
path has not yet been verified. Ubuntu has no prebuilt release target, and its
desktop compatibility is unverified.

## Build

Install a recent stable Rust toolchain with [rustup](https://rustup.rs/), plus
GTK 4.12 or newer, libadwaita 1.5 or newer, libsoup 3 and WebKitGTK 6.0 development files,
and gettext. Distribution Rust
packages may be older than the toolchain required by the current lockfile.

Fedora:

```sh
sudo dnf install gtk4-devel libadwaita-devel libsoup3-devel webkitgtk6.0-devel gettext desktop-file-utils
cargo build --release --locked
```

Ubuntu (source build only; desktop use unverified):

```sh
sudo apt install libgtk-4-dev libadwaita-1-dev libsoup-3.0-dev libwebkitgtk-6.0-dev gettext desktop-file-utils
cargo build --release --locked
```

## Install

**1.0.0 has not been published.** For prebuilt archives, see the
[manual installation steps and runtime packages](docs/installation.md#manual-installation-from-an-archive).
No Rust toolchain is needed for that path. Fedora 44 x86_64 is the planned
release target; a public release archive is not available yet.

The one-command installer and signed release workflow are prepared locally, not
yet verified against a public release. For a trusted source checkout:

```sh
./scripts/install.sh
```

It installs `gnome-clip-notes` and its `gnome-clip-notes-editor` helper under
`~/Applications/GnomeClipNotes`, desktop integration under
the user XDG directories, and the Shell extension under
`$XDG_DATA_HOME/gnome-shell/extensions`. Log out and back in, then enable the
extension in Extensions. No root access is needed for application installation.
The installer refuses private legacy-ID 0.2.0 installations. Automatic legacy
migration is outside the 1.0.0 scope; keep that installation until a separately
backed-up manual transition.

Managed uninstall uses the installed helper with `--uninstall` (or
`bash scripts/uninstall.sh` from a trusted checkout), preserving notes and settings.
See [Installation](docs/installation.md) for custom paths,
dependencies and package creation, and [Artifact verification](docs/artifact-verification.md)
for SHA256 integrity and optional signed-provenance verification.

## Use

Running `gnome-clip-notes` opens the bottom clipboard panel through the GNOME
extension. The library is a separate browsing window, available from the panel
menu, **Super+B**, or `--library`. Settings → Shortcuts → Open Library lets you
choose **Ctrl+Alt+B** instead, or disable the binding. Without the extension, the app falls back to the library
and explains how to enable the panel. Other entry points are:

```text
--daemon        run the background service
--library       open the library window
--settings      open Settings
--new-note      create a note
--about         open About
--backup PATH   write a consistent SQLite snapshot to PATH
--quit          ask the running service to quit
--version       print the application version
```

Clipboard and notes contain plain text. Notes may contain Markdown source;
HTML, images, and files are outside the current scope. Captured content stays
on this computer and is not synchronized or sent over the network.

**About** shows the installed version. **Check for Updates**
manually queries GitHub for a public stable release; an available update offers
**View Release** in your browser. No background checks, note uploads or in-app
installation. No published release and a failed check are reported separately
from “Up to date”. The linked project pages are not yet verified public endpoints.

Settings → General → **Note preview** offers two modes, applied to newly opened
editors:

- **Full · WebKit** (default): richer Markdown layout, including tables. Requires
  the WebKitGTK 6.0 runtime and a working systemd user manager. The editor runs in
  a short-lived service; WebKit is created on first Preview and retained until the
  editor window closes. Switching back to Editor keeps the unsaved text and the
  renderer. Closing the window cleans up the whole service control group.
- **Lightweight · Native**: native light/dark GTK styling for headings, emphasis,
  code, quotes, links, simple lists and tasks. No table layout or syntax highlighting.
  Very large documents with over 512 display blocks fall back to complete plain
  text to avoid creating thousands of widgets. Text selection is per display block.

Both modes keep Markdown source unchanged. Raw author HTML is inert, external
resources are not loaded, and clicked HTTP(S) links open in the external browser.
The clipboard service does not load WebKit. See [preview implementation](docs/preview.md).

Use **View** in a card's menu (clipboard panel or library) to open directly on
Preview. If the item is already open, its window is brought forward with a brief
"Already open" message; its current tab and unsaved edits stay untouched.

Card shortcuts in both the library and overlay: **F3** View, **F4** Edit,
**Alt+Enter** Info, **F2** Rename, **Delete** or **F8** Delete (with confirmation).
In the library, click a card or focus it with the keyboard to select it; arrows
move through the grid and across pages. Selection stays visible and follows item
identity across refreshes; it clears if that item leaves the visible results.
Commands require focus in the grid, not in search or a dialog. In the overlay,
commands target the highlighted card; arrows also move keyboard focus to it.
Delete edits nonempty search text while search has focus; with empty search it
acts on the highlighted card. Menu hints and Settings → Shortcuts document the bindings.

Settings → Data → **Export…** saves Notes and selected custom folders as separate
human-readable Markdown files. History and current filters are excluded. Choose
a destination each time; the last successful folder is suggested. Existing files
are never overwritten. Basic dates are included; extra metadata is optional.

Optional cleanup is off by default and requires a second confirmation with exact
counts after every file has been saved. Changed/moved items, open editors and
nonempty folders are protected. Files are checked again before deletion; a missing
or modified export prevents cleanup of its collection. The final report explains
anything kept. This exports saved contents, not unsaved editor drafts; it is not
a database backup. See [export details](docs/export.md).

The first release ships in English. Settings → General → Language offers
System default and English; unsupported system languages fall back to English.
Log out and back in to apply a language change. Library, editors, export, and the
Shell overlay share the app's
language choice; the GNOME Shell language is never changed.
See [translation infrastructure](docs/translations.md) for catalogs and tests.
The panel shows up to nine cards, adapting the count to keep previews readable.
Windows and dialogs follow native libadwaita layout, theme, and typography;
see the [interface design notes](docs/interface.md).
See [architecture](docs/architecture.md), [privacy and limitations](docs/privacy.md),
and the [manual test checklist](docs/manual-testing.md). The
[documentation index](docs/README.md) and [changelog](CHANGELOG.md) collect the
user and contributor references.

## Development and releases

Small reviewed changes may go directly to `main`, with CI checks; branches are
available when a change benefits from isolation. Release notes include commit
subjects, so use clear messages such as `fix: keep source filter on screen` or
`feat: export selected folders as Markdown`. Do not put personal notes in commits.
Public releases start at 1.0.0: fixes normally use patch versions, new features
minor versions. Pushing a version tag is intended to run checks, build and attest
the archive, publish the GitHub Release, and deploy the website. This workflow
has not yet run for a public release. Before the first tag, complete the one-time
[Pages setup](site/README.md); main pushes run checks without publishing.

The public checkout is self-contained. Maintainers may attach an optional private
context repository at the ignored `.private/` directory; it is not required for
building, testing or contributing. Never include that directory in public commits
or website assets. See [project instructions](AGENTS.md).

## License

MIT. See [LICENSE](LICENSE).

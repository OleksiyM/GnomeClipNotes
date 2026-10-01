# Architecture

The `gnome-clip-notes` executable contains the application service, GTK4/
libadwaita windows, and SQLite storage. The persistent clipboard process
exports `io.github.OleksiyM.GnomeClipNotes.Service` at
`/io/github/OleksiyM/GnomeClipNotes`. Starting another CLI or UI action activates that
service. Full note editors are short-lived helper processes with their own SQLite
connections (WAL and a busy timeout); they notify the service after saves so the
library and Shell panel refresh. Native editors remain inside the main process.

SQLite is the durable source of truth. Schema migrations are versioned and run
when the store opens. The application uses XDG user directories for data and
configuration. `--backup PATH` asks SQLite to create a consistent snapshot, so
the caller does not need to copy a live database and its WAL files.

The `extension/` directory contains the GNOME Shell side. GNOME Shell owns global
shortcuts, clipboard observation under Wayland, active-window interaction, and
the panel indicator. The extension sends plain-text events and commands over
the session D-Bus interface. It does not open SQLite directly. This boundary
keeps GNOME-version-specific JavaScript out of the storage and UI core.
The overlay uses St/Clutter widgets and Shell CSS, not HTML or WebKit. Library
and the overlay share service/store behavior, not one widget implementation.

Storage rules live in `Store`, including for Full editor connections. Group 0 is
History, group 1 is Notes, and custom folders start at 2. Retention and clipboard
deduplication affect only History. Normal folder deletion moves its contents to
Notes; create-folder-and-move is one transaction. Source filters query distinct
sources of existing items, so there is no independent source registry to prune.
Schema migrations and export revision triggers are part of the data contract;
see [export cleanup](export.md) before changing mutation or deletion paths.

Optional Library classification is stored by `classification.rs` (schema 3),
with native UI in `classification_preferences.rs`, `classification_ui.rs` and
`classification_filters.rs`.
Categories and their optional children are global, independent of folders.
`item_classifications` stores at most one assignment and a free-text comment per
item. Removing a category clears its assignments/children, not comments or notes;
removing a child preserves the parent. Deleting an item cascades its annotation.
Dictionary IDs are not reused, so stale menu choices cannot target a new category.
The independent `classification_enabled` and `comments_enabled` preferences
default to false; hiding either feature must never delete annotations.
Classification alone does not retain History.

Assignment/comment changes and dictionary renames/removals invalidate the same
item revision tokens used by bulk actions and export cleanup, including edits
made by another connection. They do not change the note body or capture time.
Reordering the dictionary does not invalidate note content. Query and Select all
share category/child/comment predicates; ordinary text search still searches only
title/body. `Query::same_filter` includes these fields for selection invalidation.
UI callers emit the existing `State::changed` notification after mutations.
Comment dialogs save against the item revision they opened with; a concurrent
change keeps the dialog/text open with an error instead of overwriting new data.
Export freezes category/child names and comments alongside items in its read
transaction, so rendering and file verification never look up current names.
Schema 3 preserves existing rows and revision tokens, but released schema-2-only
binaries cannot reopen a database after this migration. Do not test dev builds
against a working profile without approval and a SQLite backup.

Clipboard transfers are bounded to 1 MiB of UTF-8 text. The extension prefers
the exact advertised `text/plain;charset=utf-8` format, then other advertised
UTF-8 spellings, plain text and `UTF8_STRING`. A provider may advertise a format
it cannot serve; on a transfer error, the next permitted text format gets a new
stream, with at most eight candidate formats per owner change. Partial failed
transfers are never combined. Owner changes cancel the
old operation; retries recheck lifecycle, capture settings and privacy. Oversized
or invalid text is rejected, not replaced by an HTML or image representation.
Bridge diagnostics expose attempted MIME names and status, not clipboard text.

Localization follows the same boundary: the Rust process owns the selected
language and gettext catalogs. Metadata-only queries include its resolved Shell
message map; the extension never changes the desktop process locale. Full editor
helpers inherit the daemon's active language, and preference changes take effect
after restarting the app. See [localization contract](translations.md).

GTK requires version 4.12 or newer and libadwaita requires 1.5 or newer. The
Rust bindings currently track `gtk4` 0.11 and `libadwaita` 0.9. Only the
`gnome-clip-notes-editor` executable references WebKit. See [preview lifecycle](preview.md).

The native About dialog displays update status inline. `release_check.rs` uses
libsoup 3 asynchronously only after an explicit check request, with a fixed
GitHub latest-release endpoint and no database access. HTTP redirects are refused;
the streamed response is capped at 512 KiB and the complete operation at 20 seconds
(with a 15-second session I/O timeout). Closing the dialog drops the task and
aborts its dedicated session. Strict stable `vMAJOR.MINOR.PATCH` tags are compared
numerically; release links are constructed from the canonical repository, never
accepted as arbitrary remote URLs. This is availability information, not an
updater. Tests inject local outcomes into the UI, not alternate production URLs.

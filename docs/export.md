# Markdown export

Settings → Data → Export… opens a native dialog. Select Notes and/or custom
folders, then choose a local folder or mounted drive in the system folder picker.
The picker starts at the last successful destination, otherwise Documents.
Cancelling the picker does not export anything.

Each selected collection becomes a separate `.md` file. Notes always uses
`Notes.md`; folder names are sanitized and length-limited without splitting UTF-8.
An existing name receives a numeric suffix, such as `Notes (2).md`. Creation is
exclusive, including against existing directories and symlinks; files are private
to the current user (mode 0600). Empty collections are visible but unavailable for
selection. Collections that become empty before the export snapshot are skipped
without a file or folder deletion. If all selected collections become empty, the
dialog reports Nothing to export; no files are written and no cleanup is offered.

Items are ordered by creation time, then ID. Documents contain collection and
item headings, local dates with explicit UTC offsets, original Markdown content
and horizontal separators. The system timezone is captured at export time and
reused for file verification; each record uses the offset applicable to its date
(including daylight saving time). Existing export files are not rewritten.
Unnamed items receive document-local labels such as `Note 1`. Optional metadata
adds source, origin, identifiers and last-copied time. Notes may themselves contain
arbitrary Markdown, including incomplete fragments; export preserves their body
verbatim and does not repair or normalize author markup.

History is never exported. Library filters do not affect the export. Clipboard
items saved into custom folders are included. Only the saved database state is
exported, not unsaved editor contents.

## Optional cleanup

Both cleanup options start off every time. Folder removal requires item removal,
applies only to selected custom folders and never removes the Notes collection.

1. Capture the complete selected collections in a consistent SQLite snapshot.
2. Write every Markdown file and sync files and the destination directory. Any
   failure disables all cleanup, even if other collection files succeeded.
3. Check export files and item revisions, then ask for confirmation with eligible
   item/folder totals and per-collection counts. Keep in App is the default.
4. After confirmation, recheck files and, inside an immediate SQLite transaction,
   recheck revisions and delete only the confirmed subset. New, changed, moved or
   editor-open items stay; folders with remaining contents stay. A database error
   rolls the entire cleanup back. Exported files remain on disk.
5. Display a persistent report including every skipped item/folder category and
   why it was kept, plus an Open Export Folder action.

Open Export Folder uses GTK's file launcher for the destination directory.
Only one launch request can be pending. A failed launch leaves a retryable inline
message in the export report instead of stacking another modal error dialog;
cancelling a system chooser is not treated as an error. Exported files are unchanged.

Schema version 2 adds internal random revision tokens maintained by SQLite
triggers. These detect same-second edits, edit-and-revert, and row ID reuse across
processes. Tokens are not included in the Markdown. The migration preserves
existing items and settings; older schema-1-only application builds cannot reopen
the upgraded database.

Files remain ordinary user-managed files: they are verified just before cleanup,
not locked against subsequent modification or deletion by other applications.
The feature is for readable documents, not a restorable application backup.

## Verification

`cargo test --offline` covers ordering, filenames, collisions, revision checks,
ID reuse, retention boundaries, partial failure and schema migration.
`bash scripts/test-session.sh --export` runs isolated GTK scenarios against a
temporary database and saves light/dark screenshots. No real session is restarted.
It also registers a private test folder handler to exercise the actual launch
API, injects failure/cancellation into the completion handler, and checks retry,
without opening the user's file manager or changing real file associations.
`cargo run --example check-folder-launch` is a separate
manual probe that opens a new empty temporary folder in the real file manager.
Manually verify the system folder picker (cancel, Documents fallback, remembered
destination, mounted drive), long folder names, many collections and a narrow
window. Choose Open Export Folder and verify the file manager opens the result.

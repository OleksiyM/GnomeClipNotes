# Manual test checklist

Run this checklist in real GNOME Wayland sessions. Record the distribution,
GNOME version, and whether the session is Wayland for each run.

## Current test focus

Headless-session checks passed on Fedora 44 with GNOME 50.4: text capture,
clipboard ownership without recapture, active-window paste, sensitive MIME
rejection, pause/resume, adaptive card overlay, and on-demand service activation.
The isolated GTK smoke test also passed editor saving and rendered the library,
Markdown preview, Settings, and About, plus dark styling, a 420-pixel library,
and narrow adaptive Settings. These do not replace the real-session
checks below. The maintainer reports application use on Ubuntu 26.04.1;
the revised installer and new service controls still need testing there.

- Build from a clean checkout with the documented packages.
- Run `cargo test --locked` and `cargo clippy --all-targets -- -D warnings`.
- Install as a regular user; log out and in; enable the extension.
- Open the app from the launcher with the daemon stopped, running without a
  Library window, and with Library already open. Each launch must present one
  Library, not the overlay; the launch spinner must settle normally. Check both
  D-Bus activation and the desktop file's `--library` Exec fallback. Super+V and
  the indicator must still toggle the overlay; `--daemon` opens no window.
- Confirm the panel menu opens the app, creates a note, pauses capture, opens
  Settings, and opens About.
- Uninstall and confirm application data remains.

## Menus

- In the indicator's Service submenu, check Start / Stop / Restart, separator,
  then status. Running enables Stop/Restart only; stopped enables Start only.
  During an operation all three are disabled. Start/Restart open no app windows.
  Stop closes Library/Settings but keeps the indicator; opening its menu alone
  must not restart capture. Explicit Open Library/Clipboard may start the service.
- With a new unsaved Native note, a saved note editor and a Full editor, Stop,
  Restart and CLI `--quit` must refuse without closing buffers. After closing
  editors, stop/restart succeeds; CLI prints to the invoking terminal and returns
  nonzero on refusal. Rapid repeated clicks must not launch duplicate operations.
- After CLI shutdown, the menu must show Stopped, not a late Running status.
  Service Restart is not a Shell extension reload; updates still need logout/login.
- Check the library main menu and each card's More Actions: left-aligned native
  rows, keyboard navigation, Escape dismissal and activation. Cancel Rename
  and Delete confirmations to verify they do not change data.
- Change the global New Note/Open Clipboard bindings in Settings and verify
  main-menu hints update without reopening the library. No made-up shortcuts
  should appear beside card actions.
- Check overlay card menus, all three filter menus, More folders, and the tray
  including Pause Capture. Verify selection marks, section separators and
  hover/keyboard contrast in light and dark appearances.
- Check built-in text menus in search, editor/preview, and dialog fields.
  Filter forms and preference dropdowns should retain native controls.
- Hover the overlay's New Folder, New Note and Settings icons, then focus them
  with the keyboard: readable hints should appear without shifting the toolbar.
  Move away, open a menu, or close the overlay before/after the hint appears;
  no hint should remain floating over the desktop.
- Open overlay date filter → Custom: enter From/To as YYYY-MM-DD without
  leaving the overlay. Verify live results, full inclusion of the last date,
  partial/invalid dates preserving the previous filter, and remembered values
  after reopening. Enter moves between fields, then dismisses a valid form;
  Escape/outside click dismiss the menu without pasting. Any time clears the
  date filter. Check the form in light and dark appearances.
- For each From/To field in Library and overlay, open its calendar with empty,
  invalid and valid text. Opening and month navigation must leave text/results
  untouched; day selection (including the already highlighted day) and Today
  fill only that field. Escape/outside dismissal must not commit a date or
  activate a card. Check calendar header/day contrast in both appearances.
- In Library Filters, choose a type, date and source: results update immediately
  and the button counts active filters. After using each nested dropdown, click
  elsewhere inside the window, then repeat with a desktop click and Escape:
  the panel closes and choices stay applied. A dismissing click must not open
  the card underneath. Clear filters resets all filters but keeps search text.
  Partial dates and reversed custom ranges keep the last valid date filter and
  show an inline hint instead of emptying the results.

## Settings layout

- Language initially offers only System default and English. Choose English,
  leave an editor open, and confirm the selection is saved without closing the
  editor. Restart the app when drafts are safe; the choice should remain.
  An unsupported system language must fall back to English. GNOME Settings,
  Shell menus and the desktop language must not change.
- Run `scripts/translations.py check`, `bash scripts/test-i18n-runtime.sh` and
  `bash scripts/test-i18n-ui.sh`. Review the elongated-label screenshots; use an
  installed `en_US.UTF-8` locale for the pseudolocale runs.

- Visit General, History, Privacy, Shortcuts, Folders and Data using the sidebar.
  Check title and selection match. Repeat at 420 px: use Back to return to the
  section list, then open another section without widening the window.
- Check each section in light and dark themes, including long folder names and
  application identifiers. Scroll through the entire shortcut reference.
- Retention's selected period and Apply state must update together. Cancel a
  shorter period and verify the currently applied value remains unchanged.
- Rename/reorder/add/remove a custom folder and add/remove an ignored app;
  stay in the current Settings section after the list refresh. The first
  folder cannot move up, and the last cannot move down.

## Clipboard, paste, and persistence

- Copy ordinary text and a URL from two applications; verify source/type labels,
  ordering, search, and deduplication.
- Confirm images, file lists, and rich HTML are not captured as non-text data.
- Select items with the keyboard; test Enter, Shift+Enter, and quick paste 1–9.
- Press Ctrl+C on different selected overlay cards with US, Russian, and
  Ukrainian layouts: copy the full card content, keep the overlay open, and
  do not inject text into the previous application.
- Paste into GTK, browser, terminal, and sandboxed applications; record failures.
- Repeat Enter, card clicks, and context-menu Paste with US, Russian, and
  Ukrainian keyboard layouts, including Cyrillic text. Confirm the active
  layout is unchanged and no literal shortcut characters appear in terminals.
- Use arrows across page boundaries in both directions and hold an arrow to
  traverse history. At the final card, confirm navigation does not open an
  empty page. Repeat with a filtered history and Notes.
- Pause capture for a timed interval and verify automatic resume.
- Restart the daemon, then log out and back in; verify history and notes persist
  and only one service instance owns the database.

## Privacy and Notes

- Copy from an ignored application and a password field; confirm no item appears.
- Verify adding and removing an ignored application takes effect immediately.
- Lock the session while the overlay is open. Confirm it closes and shortcuts
  cannot reopen it or paste content on the lock screen.
- After unlocking, confirm capture resumes. Notes have no separate unlock or
  encryption feature; their protection is exclusion from automatic retention.
- Run `--backup PATH` while capture is active, open the resulting SQLite file,
  and verify recent history and notes are consistent.

## Retention and recovery

- Move the retention slider without applying: confirm neither the saved period
  nor stored items change. Apply a shorter period, cancel, then apply and confirm.
  Check the count includes only expired History entries; Notes and custom folders
  (including captured items moved there) survive. Forever disables cleanup.
- Kill the daemon during normal use, restart it, and verify database recovery and
  continued capture without duplicate entries.

## Local test scripts

- `scripts/test-session.sh` runs the debug UI smoke test in an isolated D-Bus
  session under Xvfb.
- `scripts/test-shell.sh` runs the GNOME Shell integration checks in a separate
  headless session (currently invoking GNOME Shell 50). It verifies initial
  card population and the modal grab, then injects real keyboard and pointer
  events for Enter, card clicks, and context-menu Paste into a GTK entry,
  on US, Russian, and Ukrainian layouts, plus Move to Notes through the context
  menu. It also checks arrow navigation across pages and the final boundary.
  Bridge-only paste tests are not sufficient to validate overlay interaction.
  Library filter checks use real pointer events through nested GTK dropdowns,
  outside clicks inside/outside the app, Escape, Clear filters preserving
  search text, and partial/invalid dates retaining the last valid range.
  Use `scripts/test-shell.sh --filters` to run only the filter regression and
  service lifecycle checks in that private session.
  Service checks exercise start/stop/restart, fixed menu order, sensitivity while
  busy, editor refusal, and a screenshot of the native submenu.

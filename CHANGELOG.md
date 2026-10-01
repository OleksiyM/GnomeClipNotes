# Changelog

Release availability and publication dates are recorded in
[GitHub Releases](https://github.com/OleksiyM/GnomeClipNotes/releases).

## 1.3.0 — 2026-10-01

### Library

- Optional categories and subcategories, independent of folders. Manage names
  and ordering in Settings; assign or remove a category from a card's menu.
  Subcategories can be labels or saved questions.
- Independent per-item comments with a card shortcut for editing existing
  comments. Comments and categories have separate default-off switches in
  Settings → Comments & Categories; disabling them hides controls, not data.
- Filter by category, subcategory or comment text, including across All Items
  and bulk selection. Search within category dropdowns narrows choices live.
- Collection and category tags share a subtle, noninteractive style. Long names
  use the available card width without widening the grid; full names have tooltips.

### Export and data

- Both collection and selected-item Markdown exports include categories,
  subcategories and comments, even when technical metadata is disabled.
- Removing categories preserves notes and comments; moving items preserves
  their annotations. Combine creates a new unclassified note. History retention
  is unchanged by classification.
- Database schema upgrades automatically to version 3. Earlier app versions
  cannot open the upgraded database; keep a pre-upgrade database backup if a
  downgrade may be needed. Export is not a database backup.

### Known issue

- Previously observed transient Library selection/paging inconsistencies remain
  unresolved. Narrow pages can also require scrolling. This release does not
  claim to fix these observations; cancel selection if its count looks wrong.

## 1.2.0 — 2026-09-27

### Library

- Show collection labels in All Items and per-collection counts before bulk move
  or deletion. Block stale bulk actions during search updates, recover empty
  later pages after mutations, and remove obsolete Library source filters.
- Explicit multi-selection with checkboxes, cross-page Select all and bulk
  Combine, Copy, Export, Move and confirmed Delete. Narrow layouts keep Combine
  visible and move Export/Delete into More Actions.
- All Items provides search and filtering across collections. Combine creates a
  new retained note in chronological order and separately offers to delete originals.

### Known issue

- During exploratory Library use, a temporarily inconsistent selection count
  and underfilled pages were observed. A reliable reproduction is not yet known;
  reopening Library restored normal behavior. If the selected count looks wrong,
  cancel selection before performing a bulk action. These observations remain
  unresolved, not claimed as fixed by the search/paging improvements.

## 1.1.1 — 2026-09-26

### Fixed

- Guided interactive install/uninstall and dependency consent now use
  non-seeking terminal streams, fixing the misleading `/dev/tty` failure.
  Interactive sudo authorization uses an unbuffered terminal descriptor.

### Documentation

- Website command blocks have visible copy controls with hover/press feedback,
  inline success states and local manual-copy instructions when access is denied.
- Version-independent website and README requirements; exact runtime, Shell
  and verified distribution combinations remain in the installation guide.
- Record the maintainer's Ubuntu Standard and noninteractive Guided checks.

## 1.1.0 — 2026-09-25

### Service controls

- Indicator Service submenu with Start, Stop and Restart, status and state-aware
  availability. Stopping or restarting refuses open note editors; the Shell
  indicator remains available. Restart waits for process exit and opens no windows.
- CLI `--quit` respects the same editor guard and reports its result; About and
  stop descriptions in `--help` clarify what the commands do.
- Discard late service replies after owner changes to avoid stale Running status.

### Installation and presentation

- Standalone Standard install/update and uninstall scripts, with readable stage
  messages, optional provenance verification and preserved notes by default.
  Existing transactional setup remains available as Guided.
- Guided compatibility mapping for Ubuntu 26.04 x86_64 using the Fedora 44 build;
  dependency offers use Ubuntu packages. Live verification of the revised path
  remains pending.
- Native ARM64 CI/release build configuration alongside x86_64; ARM desktop
  support remains experimental until tested.
- README screenshots and an eight-image website gallery, plus Standard/Guided
  installation choices and copy buttons.

### Fixed

- Launching from the applications menu opens or presents Library instead of
  toggling the Shell overlay. The desktop file's Exec fallback also opens Library;
  Super+V, the indicator and the no-argument CLI keep their overlay behavior.

## 1.0.0 — 2026-09-24

First public version. Earlier 0.x builds were private development builds,
not public releases.

### Capture and collect

- Plain-text clipboard history with a bottom GNOME Shell panel, search, source,
  type and date filters, and keyboard navigation across pages.
- Notes and custom folders for material worth keeping; quick moves and creation
  of a destination folder directly from a card menu.
- History-only retention, capture pause, ignored applications and sensitive-format
  exclusions. These are safeguards, not a guarantee of detecting every password.

### Write and reuse

- Markdown editing with draft preservation and clear unsaved-change indication.
- Full WebKit and lightweight Native previews, following the desktop theme.
- Human-readable Markdown export of Notes and selected folders, with optional
  metadata and separately confirmed cleanup.
- Local SQLite storage, consistent database backups and no account requirement.

### Desktop integration

- Native GTK4/libadwaita Library, Settings and About; GNOME Shell clipboard and
  shortcut integration on Wayland.
- Configurable Library shortcut: Super+B by default, with Ctrl+Alt+B as an
  alternative. Card deletion uses Delete or F8 with confirmation; overlay arrow
  navigation keeps keyboard focus aligned with the highlighted card.
- Explicit release availability checks, opening downloads in the system browser.
- English UI with System default / English selection and localization groundwork.
- A Fedora 44 x86_64 archive containing the application and matching Shell
  extension, with per-user installation/update tooling.

### Platform and distribution

- Fedora 44 x86_64 build, with the matching Shell extension inside one archive.
- SHA256 checksums and GitHub build-provenance attestations.
- Manual archive installation and one entry point for installation and updates.
- Ubuntu desktop compatibility and ARM64 builds are unverified; neither is a
  prebuilt release target. See [installation requirements](docs/installation.md).

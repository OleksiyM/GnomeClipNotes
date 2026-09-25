# Changelog

Release availability and publication dates are recorded in
[GitHub Releases](https://github.com/OleksiyM/GnomeClipNotes/releases).

## Unreleased

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

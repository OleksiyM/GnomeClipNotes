# Changelog

## 1.0.0 — unreleased

First public release, in preparation. Earlier 0.x builds were private development
builds, not public releases.

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

### Release status

No public 1.0.0 artifacts or verified download endpoints yet. Fedora 44 x86_64
is the only planned release build; the actual public download-to-install path is
not verified. Ubuntu has no prebuilt release target; desktop compatibility is
unverified. See [installation requirements and verification limits](docs/installation.md).
Pushing a version tag is intended to run checks, build and attest the archive,
publish the GitHub Release, and deploy the website. This workflow has not yet
run for a public release. Subsequent release notes include commit subjects
since the previous stable tag, including changes committed directly to `main`.

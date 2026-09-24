# GnomeClipNotes project instructions

## Resume ongoing work

Read the relevant public documentation before changing implementation. Maintainer
workspaces may also contain an ignored `.private/README.md`; when present, read
its index and `.private/release-plan.md` before resuming release work and update
the checkpoint after meaningful work. The optional private context is never a
build, test or contributor requirement. Do not treat planned features as implemented.

## Product and ownership

GnomeClipNotes: GNOME is the environment, Clip is the source/tool, Notes is the
result. The product direction is Capture → Curate → Organize → Export → Develop
→ Reuse. See `docs/product.md`; do not promise that all stages happen inside the
application. Public documentation and the first release UI are English.

Keep architecture, coordination and final integration review with the lead agent.
Delegate bounded independent work when useful; review the integrated result.

## Architecture and safety

- Rust GTK4/libadwaita service owns SQLite, settings and normal application UI.
  GNOME Shell extension owns Wayland clipboard/global-shortcut integration.
- Full/WebKit editors are separate short-lived processes; Native stays in the
  service. Preserve unsaved buffers, focus, selection and process cleanup.
- Keep existing `gnome-clip-notes` XDG data/config paths during the application-ID
  transition. Never reset notes, folders, settings or shortcut consent to rename
  desktop integration. Do not run two capture services during migration.
- Do not restart the user's application, disable their active extension, log out,
  close editors or touch real clipboard contents as part of an automated test.
  Use the existing isolated test harnesses and temporary profiles.
- Preserve user-authored documents. Local plans/conversations are not public
  release content. Do not delete them or stage the whole workspace blindly.
- Use native GNOME/libadwaita patterns and current official documentation.
- Follow `docs/translations.md`: whole messages, named placeholders, gettext
  plural selection, no translation of user content or global Shell locale edits.

## Checks

For relevant changes run `cargo fmt --check`, `cargo clippy --locked --all-targets
-- -D warnings`, and `cargo test --locked`. Update/check gettext catalogs when
messages change. Run affected isolated GTK, Shell, export, i18n and editor-process
tests; scripts requiring a desktop/systemd manager may need explicit permission.
Build success does not establish GNOME-version/distribution compatibility.

## Release and deployment

- First public release: 1.0.0; first release tag: v1.0.0. Do not invent earlier
  public release history or mark a release published before it exists.
- Canonical repository: `OleksiyM/GnomeClipNotes`; website:
  `https://oleksiym.github.io/GnomeClipNotes/`.
- Desktop ID: `io.github.OleksiyM.GnomeClipNotes`; extension UUID:
  `gnome-clip-notes@oleksiym.github.io`. See `docs/architecture.md` and
  `docs/distribution.md` before changing active IDs, installers, autostart or D-Bus endpoints.
- Discuss installation/update behavior with the user before implementing that
  workflow. No background self-updater is planned for 1.0.
- Pushes to main run CI. An approved stable version tag runs checks, builds the
  Fedora 44 x86_64 archive, and publishes a GitHub Release and website. The archive
  name includes version and architecture; exact platform metadata stays inside.
  Sending the tag is the publication decision, not a request to prepare a draft.
- Do not publish, push tags, create a public repository, or enable Pages merely
  because local preparation is complete; confirm readiness and exact targets.
- Archive the WebKit prototype in the verified archive branch before removing
  its main-branch copy. No synthetic claim of preserving unavailable Git history.
- Small reviewed changes may go directly to `main`; branches are optional for
  larger work. Keep CI checks. Use meaningful commit subjects for generated
  release notes; no private conversations or secrets in commits. After 1.0.0,
  fixes normally increment patch (1.0.1), features minor (1.1.0), incompatible
  changes major. Do not bump versions for every development commit.

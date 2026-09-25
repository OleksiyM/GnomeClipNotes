# Documentation

## Using GnomeClipNotes

- [Installation, updates and removal](installation.md): runtime packages, manual
  archive installation, activation and platform limits.
- [Privacy and limitations](privacy.md): local storage and capture boundaries.
- [Export](export.md): readable Markdown and optional cleanup.
- [Note previews](preview.md): Native and Full behavior and resource lifetime.
- [Artifact verification](artifact-verification.md): checksums and optional provenance.

## Understanding and developing the application

- Start with the [project map](../AGENTS.md): architecture boundaries, reasons,
  invariants, change entry points, verification and release sequence. Follow its
  links for the relevant area rather than reading the entire documentation first.
- [Product direction](product.md): Capture → Curate → Organize → Export → Develop → Reuse.
- [Architecture](architecture.md): Rust service, SQLite, Shell and editor processes.
- [Interface design](interface.md): native patterns and interaction rules.
- [Translations](translations.md): message and locale contract.
- [Distribution](distribution.md): installer boundaries and update protocol.
- [Manual tests](manual-testing.md): real-desktop checks and isolated harnesses.
- [Changelog](../CHANGELOG.md).

`site/` contains the static website source. No private maintainer notes are needed
to build, test or understand the public project.

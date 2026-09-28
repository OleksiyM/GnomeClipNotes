# Security

## Reporting a vulnerability

Use GitHub's [private vulnerability reporting form](https://github.com/OleksiyM/GnomeClipNotes/security/advisories/new).
Do not publish exploit details in an issue or pull request before coordinated
disclosure. Ordinary bugs and installation questions belong in
[Issues](https://github.com/OleksiyM/GnomeClipNotes/issues).

Include the affected version, environment, expected impact and the smallest safe
reproduction you can provide. Use synthetic text instead of real clipboard
contents. Do not upload credentials, personal notes or your clipboard database,
even to a private report.

This is an independently maintained project. Reports are reviewed as time
permits; there is no guaranteed response time or bug bounty program.

## Versions and scope

Security fixes target the latest stable release. Older releases have no separate
maintenance or backport guarantee. Check the
[latest release](https://github.com/OleksiyM/GnomeClipNotes/releases/latest)
and mention your exact version when reporting; do not delay a report just
because you cannot retest on the latest version.

GnomeClipNotes stores clipboard text and notes locally. Sensitive-content
exclusions are best-effort filtering, not a password vault or encryption feature.
See [privacy and limitations](docs/privacy.md) and
[release artifact verification](docs/artifact-verification.md).

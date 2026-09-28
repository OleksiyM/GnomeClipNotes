# Contributing

Bug reports, usability feedback, documentation fixes, translations and focused
code changes are welcome. This is a small independently maintained project;
review and support happen as time permits.

## Reports and suggestions

Search [existing issues](https://github.com/OleksiyM/GnomeClipNotes/issues) before
opening a new one. Use the bug or suggestion form, or a blank issue for questions.
An intermittent bug is still worth reporting: distinguish what you observed from
what you suspect, and include steps only when you know them.

Include the app version, distribution/GNOME version and installation method when
relevant. For paste problems, name the source and destination applications.
**Never post your clipboard database, credentials or private notes.** Redact logs
and screenshots, and use synthetic sample text wherever possible.
Security vulnerabilities have a [private reporting route](SECURITY.md).

Be considerate, discuss the behavior or proposed change rather than the person,
and respect other people's privacy. Disagreement about a design is normal;
harassment and personal attacks are not welcome.
See the [Code of Conduct](CODE_OF_CONDUCT.md) for expectations and private reporting.

## Before a larger change

Discuss substantial features or architectural changes in an issue first, so you
do not spend time building something outside the project's scope. GnomeClipNotes
is local-first text/Markdown capture and curation, not a cloud service or a
general-purpose knowledge platform. See the [product direction](docs/product.md).
Small fixes do not need a proposal document.

## Working on the code

1. Fork and clone the public repository; create a focused branch for your PR.
   No maintainer-private files or services are required.
2. Follow [build instructions](README.md#build-from-source). Read the relevant
   area of the [project map](AGENTS.md) rather than rediscovering the architecture.
3. Keep the change scoped. Update relevant documentation; route UI messages
   through the existing [translation workflow](docs/translations.md).
4. Verify the boundary you changed and describe actual results in the PR.

For Rust changes, the normal checks are:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

UI, Shell, installer and process-lifetime changes need their relevant checks too;
see [AGENTS.md](AGENTS.md#verify-the-boundary-you-changed) and the
[manual checklist](docs/manual-testing.md). Use isolated test profiles, never
someone's real clipboard or database as a fixture. Report unperformed checks
honestly; successful compilation is not desktop compatibility evidence.

For documentation-only changes, check links/examples and `git diff --check`;
a full rebuild is unnecessary. Keep private workspace files and generated
artifacts out of commits. Do not change the release version for every PR: the
maintainer coordinates version bumps and release tags.

Use a descriptive PR title and explain why the change is useful. AI-assisted
contributions are welcome under the same expectations: understand the submitted
code, review it, and state what was actually verified.

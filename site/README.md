# Project website

Plain HTML/CSS with a small optional JavaScript enhancement for install tabs and
copy buttons; no build system, external fonts or analytics. Without JavaScript,
both installation commands remain visible and selectable.
Open `index.html` locally or serve this directory with a static HTTP server.

Publish **only this directory**, never the workspace root. GitHub Pages uses
`.github/workflows/pages.yml`, called by the release workflow after a stable
tag's GitHub Release is published. The site is checked out from that same tag.
The first deployment accompanied v1.0.0 on 2026-09-24:
https://oleksiym.github.io/GnomeClipNotes/

Before the first tag, configure repository Settings → Pages → Source as **GitHub
Actions** and allow `v*` tags in the `github-pages` environment's deployment
rules. These are one-time repository settings, not additional release actions.
The workflow does not enable Pages or change environment rules itself.

If only website deployment fails, rerun that failed job, or dispatch this workflow
at the published tag (`gh workflow run pages.yml --ref v1.0.0`). Do not recreate
the release or move its tag. Manual runs on main do not deploy.

Runtime package commands and installation details stay authoritative in
`docs/installation.md`; the website links to them rather than maintaining a
second copy. The static page describes the released product; it is deployed only
after a published release exists. No separate website build system is needed.

## Screenshots

The gallery uses eight unmodified screenshots supplied by the maintainer from
their Ubuntu 26.04.1 application test on 2026-09-24. They show example commands
and Markdown, not generated mockups. This reports application use, not support
for every installer path on Ubuntu. The icon is the project's desktop asset.

| Asset | View |
| --- | --- |
| `overlay-dark.png` | Full desktop and dark Shell overlay |
| `library-dark.png`, `library-light.png` | Library in both themes |
| `editor-dark.png`, `full-preview.png` | Markdown source and Full preview |
| `settings-dark.png`, `about-light.png` | Preferences and project information |
| `overlay-light.png` | Light overlay with source filter open |

README reuses three of these assets. The site keeps every frame uncropped and
links directly to the full-resolution image; smaller windows use a single column.
Original source captures remain private and are not needed to build the site.
The older `overlay.png` and `library.png` remain isolated-test reference images,
but are no longer displayed. Review visible content before adding screenshots;
never copy the whole private directory into public assets.

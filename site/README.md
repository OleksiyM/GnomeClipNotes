# Project website

Plain HTML/CSS; no JavaScript, build system, external fonts or analytics.
Open `index.html` locally or serve this directory with a static HTTP server.

Publish **only this directory**, never the workspace root. GitHub Pages uses
`.github/workflows/pages.yml`, triggered manually after repository/Pages setup
and publication approval. This checkout has not deployed a public site.

Runtime package commands and installation details stay authoritative in
`docs/installation.md`; the website links to them rather than maintaining a
second copy. Before launch, remove the clearly marked unpublished notices only
after the actual release/download links have been verified.

Screenshots are unmodified output from isolated development UI tests on GNOME,
with synthetic data. `overlay.png` shows the Shell test desktop; `library.png`
shows the GTK smoke fixture. No personal clipboard contents are used. The icon
is copied from the project's desktop asset. Refresh screenshots when the visible
interface changes; they are not generated mockups.

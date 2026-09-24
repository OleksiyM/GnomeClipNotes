# Project website

Plain HTML/CSS; no JavaScript, build system, external fonts or analytics.
Open `index.html` locally or serve this directory with a static HTTP server.

Publish **only this directory**, never the workspace root. GitHub Pages uses
`.github/workflows/pages.yml`, called by the release workflow after a stable
tag's GitHub Release is published. The site is checked out from that same tag.
This checkout has not deployed a public site yet.

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

Screenshots are unmodified output from isolated development UI tests on GNOME,
with synthetic data. `overlay.png` shows the Shell test desktop; `library.png`
shows the GTK smoke fixture. No personal clipboard contents are used. The icon
is copied from the project's desktop asset. Refresh screenshots when the visible
interface changes; they are not generated mockups.

# Installation

## Choose your path

Both paths install the same application, Full preview helper and GNOME Shell
extension. These are installation methods, not different editions of the app.

| | Standard — recommended | Guided |
| --- | --- | --- |
| Entry point | `install.sh` | `guided-install.sh` |
| Best for | You manage dependencies and close the app yourself | You want help with dependencies and coordinated updates |
| Implementation | Standalone Bash; no Python | Bash bootstrap and packaged Python helper |
| System packages | Never installed by the script | Offered with separate consent on listed systems |
| Closing editors/service | Your responsibility | Refuses open editors, then requests safe service shutdown |
| Recovery of program files | None; fix the error and rerun | One previous owned-file snapshot |
| Uninstall | Standalone `uninstall.sh` | Installed `install-release.py --uninstall` |

Use the same method for updates. To switch, uninstall with the previous method
first, **without purging data**. Standard refuses an existing Guided receipt rather
than leaving its file inventory stale; Guided refuses an unowned Standard install.
Neither method automatically logs you out.

These installation paths are available with 1.1.0. Published 1.0.0 contains the
earlier Guided helper; its platform check still accepts Fedora 44 x86_64 only.

## Requirements and verification

Use GNOME on **Wayland**, with a working systemd user session. Install the full
runtime, including WebKitGTK 6.0, even if you prefer Native preview. The clipboard
service itself does not load WebKit. Rust is not needed for prebuilt archives.

The intended environment is a recent GNOME distribution, such as Fedora or
Ubuntu. Distribution names (including Arch and derivatives) alone do not prove
binary compatibility. Check the runtime, architecture and Shell version below;
Guided accepts only explicitly listed host platforms. The website keeps this
summary version-independent; the table records specific testing evidence.

Source-level minimums are GTK 4.12 and libadwaita 1.5, plus libsoup 3 and
WebKitGTK 6.0. Prebuilt archives also need compatible native libraries/ABI,
including glibc; those minimums alone do not guarantee a binary will run.
The bundled extension currently declares GNOME Shell 46–50; a newer Shell
release needs compatibility review, not merely newer GTK packages.

| System | Evidence / scope |
| --- | --- |
| Fedora 44 x86_64, GNOME 50.4 | Development use, live Standard install/uninstall and update to 1.1.0 verified; extension activation after login confirmed |
| Ubuntu 26.04.1 x86_64 | Maintainer reports working app and Standard installation/update/removal; 1.1.0 logs confirm Guided install/uninstall with `--yes` and provenance verification during install. Interactive confirmation in published 1.1.0 fails; see below |
| Fedora 44 ARM64 | Native 1.1.0 build/tests and public archive provenance verified; desktop validation remains pending |
| Arch, derivatives, older Ubuntu, other distributions | Not verified for the published binary; do not assume compatibility |

Archives are built on Fedora 44. Standard selects by CPU architecture, not
distribution: this makes it usable on compatible systems, **not a universal Linux
binary**. Native library/ABI requirements still apply. Ubuntu 22.04's standard
desktop stack is below the application's GTK 4.12 / libadwaita 1.5 minimums.
Use only archives actually listed in [Releases](https://github.com/OleksiyM/GnomeClipNotes/releases);
1.0.0 has no ARM64 archive.

The 1.1.0 Guided helper explicitly allows Fedora 44 x86_64/ARM64 and Ubuntu
26.04 x86_64 (including 26.04.1, which reports `VERSION_ID=26.04`).
It uses the Fedora build without relabelling it, and chooses dependency packages
for the **host OS**. An allowlist is not proof of desktop testing on every target.

### Runtime packages

Fedora 44 GNOME desktop:

```sh
sudo dnf install gtk4 libadwaita libsoup3 webkitgtk6.0 glib2 glibc-common systemd gnome-shell curl ca-certificates tar gzip coreutils gawk sed
```

Ubuntu 26.04 GNOME desktop:

```sh
sudo apt install libgtk-4-1 libadwaita-1-0 libsoup-3.0-0 libwebkitgtk-6.0-4 libglib2.0-bin libglib2.0-0t64 libc-bin systemd gnome-shell curl ca-certificates tar gzip coreutils gawk sed
```

Guided additionally needs Python 3. GitHub CLI (`gh`) and the Extensions GUI
are optional. Installing `gnome-shell` on another desktop does not by itself
make that desktop supported. Older distributions may require building from source
and a newer desktop stack, not just these package names.

## Standard installation and updates

**Before running:** install dependencies, save and close every note editor, disable
the extension, and quit the app. Closing Library does not stop the background service:

```sh
gnome-extensions disable gnome-clip-notes@oleksiym.github.io
"$HOME/Applications/GnomeClipNotes/gnome-clip-notes" --quit
```

On a first installation there is nothing to stop. `--quit` does not save open
editors: save and close them first. Then run as your normal user, never with sudo:

In builds with the indicator's **Service** submenu, you can instead save/close
editors, choose **Service → Stop**, then disable the extension. The indicator
remaining visible does not mean the service is running; check its submenu status.

```sh
curl -fsSL https://raw.githubusercontent.com/OleksiyM/GnomeClipNotes/main/install.sh | bash
```

The same command updates an existing Standard installation. It downloads a stable
release, verifies SHA256, optionally verifies provenance, stages the archive, and
copies binaries, translations and desktop integration. It does not install
dependencies, stop processes, make database backups or provide rollback.
A failure after replacement starts may leave a partial installation: resolve the
reported cause and rerun. Notes and settings are not installation targets.

If `gh` is missing **or lacks the required flags**, Standard explicitly skips
provenance. To require it:

```sh
curl -fsSL https://raw.githubusercontent.com/OleksiyM/GnomeClipNotes/main/install.sh | bash -s -- --require-provenance
```

Once verification is attempted, a missing bundle or failed verification stops
installation. Select a specific stable release with `--version v1.1.0`.
An unavailable architecture asset fails before application files are replaced.

For inspect-first use, download the script, read it, then run `bash install.sh`.
HTTPS/repository delivery of the bootstrap is trusted; checking archive integrity
does not authenticate the script itself. See [artifact verification](artifact-verification.md).

### Standard uninstall

Save/close editors and quit the service as above, then:

```sh
curl -fsSL https://raw.githubusercontent.com/OleksiyM/GnomeClipNotes/main/uninstall.sh | bash
```

This is standalone: no clone, Python helper or installation receipt is needed.
It disables the extension and removes known application files; notes, folders,
History, settings and backups remain. Unrelated files in the app directory remain.

`--purge` is deliberately **not** part of the normal command. Explicitly supplying
it also deletes all contents of the app's data/config directories, including
notes, History, folders and backups stored there. There is no extra confirmation
or undo. Keep any backup you need outside those directories.

## Guided installation and updates

```sh
curl -fsSL https://raw.githubusercontent.com/OleksiyM/GnomeClipNotes/main/guided-install.sh | bash
```

Save and close all editors first. Guided verifies the archive, checks runtime
dependencies, proposes missing packages and asks before replacing application
files. All consent precedes mutation. Package installation requires separate
consent and sudo authorization; the application itself is installed per user.
No repositories, compiler toolchains or GitHub CLI are added.

Guided uses exact editor counts and the application's shared/exclusive update lock,
not process-name guessing. It keeps one recoverable previous set of owned program
files, not a database backup or system-package rollback. An incomplete transaction
reports the saved recovery command. Same-version installation is a no-op; implicit
downgrades and private legacy-ID installations are refused.

Guided verifies provenance automatically when gh is present; an installed but
incompatible gh stops installation. Without gh it reports skipped provenance.
`--require-provenance` makes verification mandatory. For unattended use, `--yes`
accepts app changes only; `--install-deps` additionally authorizes missing system
packages and needs pre-authorized noninteractive sudo. See [distribution](distribution.md).

**Published 1.1.0 limitation:** interactive confirmation fails with a misleading
`confirmation requires /dev/tty` message, including for uninstall. `--yes`
bypasses that prompt only if you explicitly accept the operation; it does not
authorize system package changes. Version 1.1.1 fixes terminal confirmation.
Updating the bootstrap alone does not fix
the helper inside an already published archive or installation.

Guided uninstall works offline from the installed helper:

```sh
python3 "$HOME/Applications/GnomeClipNotes/install-release.py" --uninstall
```

The source-checkout wrapper `scripts/uninstall.sh` is **not** a curl-pipe installer.
It requires the repository; normal users should use the installed helper above.
Notes and settings are preserved.

## After installation — enable clipboard capture

1. **Log out and back in** to load new Shell extension code.
2. Open the **Extensions** application and enable **GnomeClipNotes**. If the global
   user-extension switch is off, enable it too.
3. Without the Extensions GUI, run this after signing back in:

   ```sh
   gnome-extensions enable gnome-clip-notes@oleksiym.github.io
   ```

You can return to this guide after login. Opening Library alone does not activate
the Shell extension. Neither installer claims capture is active immediately after
replacement or enables cached extension code in the old session.

## Manual installation from an archive

Download the matching archive, `SHA256SUMS` and optional attestation bundle from
the same release into an otherwise empty directory. For example:

```sh
archive=gnome-clip-notes-1.1.0-x86_64.tar.gz
sha256sum --check --ignore-missing SHA256SUMS
```

The selected archive must report **OK**. Stop if its entry is missing or any check
fails. Verify provenance as described in [artifact verification](artifact-verification.md)
if desired. Then:

```sh
tar -xzf "$archive" --no-same-owner
cd "${archive%.tar.gz}"
python3 scripts/install-release.py --package-dir .
```

This runs the packaged **Guided** helper without a network bootstrap. Its platform
rules belong to that release, not to a newer script from main. Then follow the
logout/login and extension-enable steps above.

## Locations, source builds and shortcuts

Default binaries live in `~/Applications/GnomeClipNotes`; both methods keep
`gnome-clip-notes` XDG data/config paths and the same desktop ID/extension UUID.
Standard accepts `GNOME_CLIP_NOTES_APP_DIR`; Guided accepts `--app-dir PATH`.
Both honor `XDG_DATA_HOME` and `XDG_CONFIG_HOME`; repeat the same overrides for
updates and removal.

A source checkout can run `bash scripts/install.sh` to build an archive and use
the Guided helper. `scripts/build-package.sh` creates local archives; they have
no GitHub provenance attestation.

If Super+V conflicts with GNOME's calendar, the app offers to free just that
binding with consent. Settings → Shortcuts also offers Resolve. Super+M and other
calendar bindings remain. The installer never changes GNOME shortcuts; an accepted
calendar change persists after uninstall.

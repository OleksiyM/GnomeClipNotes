# Installation

**1.0.0 is not published yet.** Release filenames below describe the planned
first release, not downloads that are available today. Local archives use their
actual version and platform label instead.

## Requirements

Use a GNOME **Wayland** desktop with a working systemd user session. The Shell
extension is required for clipboard capture and the bottom panel. Prebuilt
archives are distribution-specific; do not use a Fedora archive on Ubuntu.

| Platform | Current verification |
| --- | --- |
| Fedora 44, x86_64, GNOME 50.4 | Development use and isolated app/Shell tests; local archive install/update/restore tests with session commands mocked |
| Ubuntu 24.04, x86_64, GNOME 46 | Candidate build target; not yet tested on the target system |
| Other distributions, architectures or GNOME versions | No verified prebuilt support; building from source does not establish compatibility |

### Runtime packages

On an existing Fedora 44 GNOME desktop:

```sh
sudo dnf install gtk4 libadwaita libsoup3 webkitgtk6.0 glib2 glibc-common systemd gnome-shell python3 curl ca-certificates tar coreutils
```

Ubuntu 24.04 candidate (not yet installation-tested):

```sh
sudo apt update
sudo apt install libgtk-4-1 libadwaita-1-0 libsoup-3.0-0 libwebkitgtk-6.0-4 libglib2.0-bin libglib2.0-0t64 libc-bin systemd gnome-shell python3 curl ca-certificates tar coreutils
```

These are runtime packages, not compilers or development headers. The package
manager resolves their dependencies. Full preview uses WebKitGTK 6.0, named
[`webkitgtk6.0` on Fedora](https://packages.fedoraproject.org/pkgs/webkitgtk/webkitgtk6.0/)
and [`libwebkitgtk-6.0-4` on Ubuntu](https://packages.ubuntu.com/noble/libwebkitgtk-6.0-4).
The packaged installer currently checks the complete application, including the
Full helper, even if you intend to use Native preview. A non-GNOME desktop does
not become supported merely by installing `gnome-shell`.

GitHub CLI (`gh`) is **optional**, for signed provenance verification. The GNOME
Extensions application is also optional; the activation command below works
without it. Rust, Cargo and gettext tools are needed only for a source build,
not for installing a release archive.

## Manual installation from an archive

1. Install the runtime packages above. Download the matching `.tar.gz` archive
   and `SHA256SUMS` from the **same** [GitHub release](https://github.com/OleksiyM/GnomeClipNotes/releases).
   Put them in an otherwise empty working directory. If using `gh` verification,
   also download the archive's `.sigstore.json` bundle.
2. Check the download before extracting. For example, a future Fedora 44 release:

   ```sh
   archive=gnome-clip-notes-1.0.0-fedora-44-x86_64.tar.gz
   sha256sum --check --ignore-missing SHA256SUMS
   ```

   The selected archive must be reported as **OK**. Stop on any failure, missing
   match or different filename. A checksum detects corruption; it does not prove
   authenticity if both files were replaced. Optional stronger verification is
   described in [Artifact verification](artifact-verification.md).
3. Extract the verified archive, enter its directory and run its local helper:

   ```sh
   tar -xzf "$archive" --no-same-owner &&
   cd "${archive%.tar.gz}" &&
   python3 scripts/install-release.py --package-dir .
   ```

   Run this as your normal user, **not with sudo**. This route uses no network
   bootstrap and builds nothing. With dependencies installed, the helper installs
   only the application and desktop integration in your user directories. Read
   the proposed locations and confirm once; close note editors before an update.
4. Log out and back in, then enable the extension:

   ```sh
   gnome-extensions enable gnome-clip-notes@oleksiym.github.io
   ```

   Open GnomeClipNotes from the application launcher. If GNOME's global user
   extension switch is off, turn it on in Extensions. The installer does not log
   you out or activate cached extension code in the old session.

To update, repeat these steps with a newer matching archive. Notes and settings
are retained; installing the same version again is a no-op. Missing libraries or
an unsupported platform should be resolved before retrying, not bypassed with a
different distribution's binary. Private **legacy-ID** 0.2.0 installations are
not automatically migrated; this is distinct from a managed dev build that also
reports version 0.2.0.

## One-command installation and updates

The intended install/update entry point is:

```sh
curl -fsSL https://raw.githubusercontent.com/OleksiyM/GnomeClipNotes/main/install.sh | bash
```

The bootstrap downloads a platform-matched stable release, checks SHA256
integrity **before extraction**, additionally verifies signed build provenance
when GitHub CLI is present, and runs that release's installation
helper. See [Artifact verification](artifact-verification.md) for the verification
policy and download/inspect-first alternative. Running a script from `main`
still trusts the repository and HTTPS delivery.

The bootstrap needs `curl` and Python 3 already installed from trusted package
sources. Missing application/desktop runtime packages are offered separately
after the release is verified. GitHub CLI
is optional: without it the installer explicitly reports skipped provenance
verification. SHA256 alone does not authenticate a jointly replaced archive and
manifest. No GitHub login is needed for the local attestation bundle.
On candidate Fedora 44 and Ubuntu 24.04, the installer lists missing runtime
packages and prints equivalent manual commands. With separate consent, it uses
the system's configured package repositories via DNF or APT. It never adds a
repository or installs development toolchains or GitHub CLI. APT refreshes its
package index first. Dependencies may be added or updated by the package manager;
these system changes are not part of ClipNotes rollback or uninstall.

Both package consent and final application confirmation, followed by sudo
authorization, precede package installation. Application files remain untouched
until packages and runtime checks succeed. Refusal prints the manual route and
stops; package failure leaves ClipNotes files unchanged but may leave system
package changes. Commands after authorization use noninteractive sudo; expired
credentials cause failure rather than a late password prompt. System package
hooks remain the distribution's responsibility, not a transactional part of our
installer.

For unattended use, `--yes` alone does not authorize system changes:
`--yes --install-deps` explicitly permits missing runtime packages and requires
pre-authorized noninteractive sudo. Normal interactive use needs no extra flags.
An installed but broken/incompatible runtime is reported for manual repair;
the installer does not perform a general system upgrade to try to fix it.
The check lists missing commands and missing/incompatible libraries for both
binaries, using a stable child-process locale without changing the session
language. `systemd-run` must be present for Full editors; command availability
alone does not prove the user systemd manager is operational.

Fedora 44 and Ubuntu 24.04 x86_64 are **candidate build targets**, not a verified
support matrix. Remote builds and real artifact installation are release gates;
an older distribution-provided `gh` may lack the required verification flags.

The per-user installer uses these locations by default:

| Component | Location |
| --- | --- |
| Executable | `~/Applications/GnomeClipNotes/gnome-clip-notes` |
| Full preview editor | `~/Applications/GnomeClipNotes/gnome-clip-notes-editor` |
| Launcher and icon | `$XDG_DATA_HOME` (normally `~/.local/share`) |
| D-Bus activation | `$XDG_DATA_HOME/dbus-1/services` |
| Login startup | `$XDG_CONFIG_HOME/autostart` |
| Shell extension | `$XDG_DATA_HOME/gnome-shell/extensions/gnome-clip-notes@oleksiym.github.io` |

Use `--app-dir PATH`, `XDG_DATA_HOME`, or `XDG_CONFIG_HOME` to override these
roots. Repeat the same overrides when updating. Paths containing spaces are
supported; shell/desktop-sensitive characters are rejected. The bootstrap accepts
`--version v1.0.0` to select a stable tag, `--install-deps` for explicit runtime
package consent, `--require-provenance` to require
GitHub CLI verification, and `--yes` for explicit noninteractive acceptance.
Otherwise, the final confirmation reads `/dev/tty`, not the piped
script's standard input. All questions precede replacement.

Full preview requires the WebKitGTK 6.0 runtime (`webkitgtk6.0` on Fedora,
`libwebkitgtk-6.0-4` on Ubuntu) and a working systemd user manager. Keep the editor
helper alongside the main executable. The clipboard service and Native preview
do not load WebKit. See [Note preview](preview.md) for process lifetime and limits.
The main application also links libsoup 3 (`libsoup3` on Fedora,
`libsoup-3.0-0` on Ubuntu) for the optional manual update check. It is included
in the installer's explicit runtime-package list; GitHub CLI is not needed for
this check.

Save and close all Native and Full editors before updating. New builds expose
exact editor counts over D-Bus; the installer refuses open editors and unknown
protocols, requests safe service shutdown, then takes the application's exclusive
update lock before replacing owned files. It never uses unconditional `--quit`.
An installation receipt identifies owned program/integration paths; unrelated
files, notes and preferences are preserved. Same-version installation is a no-op;
implicit downgrades and unowned existing installations are refused.

The current implementation leaves the extension disabled after replacement to
avoid running cached Shell code against a different application version. Log out
and back in, then enable GnomeClipNotes in Extensions. This remaining manual
activation step is explicit; the installer never logs you out or claims capture
is active immediately after replacement.

After logging back in, enable the extension in Extensions or run:

```sh
gnome-extensions enable gnome-clip-notes@oleksiym.github.io
```

If GNOME has globally disabled user extensions, first turn on the main Extensions
switch. The installer does not change this global preference. Enabling cached
code in the old session is not a substitute for loading changed code after login;
see [GNOME's extension development guide](https://gjs.guide/extensions/development/creating.html).

One previous owned-file snapshot is retained, not a version history or database
backup. Ordinary replacement errors attempt restoration. An incomplete
transaction blocks another update and reports how to run the saved recovery
helper; recovery is also guarded by the runtime lock. Forced termination or power
loss cannot be made equivalent to a normal completed operation.

## Super+V on a new device

Launch the installed app once. If both ClipNotes and GNOME's calendar use
Super+V, the app offers to free that shortcut with explicit confirmation.
Accepting removes only Super+V from the calendar; Super+M and any other assigned
calendar shortcuts remain unchanged. Background daemon startup never prompts
or changes system shortcuts. Declining is remembered.

Settings → Shortcuts also shows a Resolve action while this conflict exists,
including after a previous refusal. Alternatively, choose another Activate
shortcut there. The installer itself does not change GNOME shortcuts.
The approved calendar change persists after uninstall; the clock remains
clickable, and calendar shortcuts can be changed in GNOME Keyboard Settings.

Create a platform-labelled release archive from a trusted checkout with:

```sh
./scripts/build-package.sh
```

The archive is written below `dist/` as
`gnome-clip-notes-VERSION-OS-OSVERSION-ARCH.tar.gz`. It includes both binaries,
the matching extension, desktop integration, selected public documentation,
`release.json`, and the versioned installer. A local build has no GitHub
attestation; do not mistake local packaging for authenticated release provenance.
It is not a universal Linux binary.

For installation from your trusted source checkout, `bash scripts/install.sh`
builds this package and uses the same local helper as the manual archive path.

## Uninstall without deleting notes

The installer saves its helper alongside the binaries, so removal works offline
without the original archive or source checkout:

```sh
python3 "$HOME/Applications/GnomeClipNotes/install-release.py" --uninstall
```

For a custom location, pass the same `--app-dir PATH` and XDG overrides as at
installation. A trusted checkout also provides `bash scripts/uninstall.sh`.
Confirmation uses `/dev/tty`; `--yes` is explicit noninteractive acceptance.

Uninstall checks the receipt and exact recorded file inventory, refuses open
editors, suspends only this extension, requests safe shutdown and takes the
exclusive runtime lock. It removes recorded program/integration files only;
notes, preferences, shortcut consent and unrecorded files are retained. Empty
extension/locale directories may be removed; directories containing unrecorded
files remain. Missing or incomplete ownership records cause refusal rather
than guessed deletion. Private legacy installations are not managed by this tool.

One recovery snapshot remains in the application's XDG data directory. A failed
uninstall attempts automatic rollback. After a hard interruption, the reported
saved helper supports `--restore`; uninstall recovery restores recorded files
individually, preserving unrecorded files added after the interruption. If the
interruption happened while stopping capture but before file replacement, recovery
leaves program files intact. Restore reports the logout/login and extension-enable
step explicitly; it does not promise automatic capture resumption. No user data
is purged and no automatic logout is performed.

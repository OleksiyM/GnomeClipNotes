# Installation and update design

## Two deliberately different paths

`install.sh` / `uninstall.sh` are standalone **Standard** shell scripts, based on
the maintainer's tested Ubuntu scripts. Users provide dependencies, save/close
editors, disable capture and stop the service beforehand. No Python, process
killing, package installation, update-lock protocol or automatic rollback is
promised for Standard. SHA256 precedes extraction; supported gh verifies provenance,
missing/old gh is explicitly skipped unless required. A failed verification stops.
Installation/removal leave user data alone unless uninstall explicitly receives
`--purge`. A Guided receipt blocks Standard to avoid corrupting ownership records.

The remaining transaction/consent/recovery contract below belongs to **Guided**:
`guided-install.sh` and the packaged `scripts/install-release.py`. Do not impose
all of it on Standard or let the two methods silently overwrite one another.
Switch by uninstalling the previous method without purging data.

Builds target Fedora 44 x86_64 and aarch64 (new ARM CI still needs validation).
Guided additionally allows the Fedora x86_64 archive on Ubuntu 26.04, where the
maintainer tested the application. Runtime package planning uses host OS, not
archive build OS; release metadata stays truthful. The older 1.0.0 helper does
not include this compatibility mapping. See [Installation](installation.md)
for user-facing instructions and verification limits, and
[GitHub Releases](https://github.com/OleksiyM/GnomeClipNotes/releases) for published
versions. A source version or local archive is not proof of a published release.

Safety contract: all questions and consents must be completed before changing
the installed application, desktop integration or system dependencies. Read-only
checks and private download/staging files are preparation, not installation.
After the user confirms the complete proposed operation, the
installer does not ask for additional prompts; it either completes, reports a
failure, or presents the defined recovery path. This cannot promise resistance
to a forced process kill, power loss, or user tampering.

The Guided command for published releases is:

```sh
curl -fsSL https://raw.githubusercontent.com/OleksiyM/GnomeClipNotes/main/guided-install.sh | bash
```

One entry point handles first installation and later updates. The user does not
have to locate and unpack an archive. Keep an inspect/download-first alternative
documented: piping into Bash executes code obtained from the repository and trusts
that repository and HTTPS delivery. Do not describe this as risk-free.

## Boundaries

1. Check the OS, CPU architecture, supported desktop/runtime dependencies and
   installation state. Match a tested release asset; never silently substitute a
   binary built for a different distribution or architecture.
2. Resolve one stable release and pin all subsequent downloads to that release.
   Download into a private temporary directory with failure/timeout handling.
3. Always check the archive against release `SHA256SUMS` before extraction.
   If `gh` is installed, additionally verify signed GitHub build provenance for
   the exact repository, release workflow and tag. Without `gh`, explicitly
   report that provenance verification was skipped; do not require installing
   GitHub CLI just for this application. `--require-provenance` makes that check
   mandatory. A present but incompatible verifier or failed provenance check
   stops installation: no silent downgrade after failure. Checksums alone do
   not authenticate an archive and manifest replaced together. Neither mechanism
   protects against compromise of the trusted repository/workflow itself. Reject
   unsafe archive paths/links. See `artifact-verification.md`.
4. Install the main binary, editor helper, extension and integration files as a
   version-matched set. Do not fetch extension files independently from `main`.
5. Preserve user data, settings and installation location. Same-version reruns
   should be safe. Refuse implicit downgrades. Keep a recoverable previous program
   installation until the coordinated replacement succeeds.
6. Clearly separate files installed on disk from code currently running in the
   app or GNOME Shell. Do not forcibly close editors or claim Shell has reloaded
  when a new login is required. Any future legacy-ID upgrade needs explicit
  handling to prevent two capture services from running; it is not part of the
  public 1.0.0 installation promise.

Application installation is per-user, without running the full downloaded script
as root. System dependency changes, if offered, require a clear separate consent
step. A piped script must not read its own source stream as answers: any interactive
prompt needs a terminal, and unattended behavior must be explicit and safe.

The public `guided-install.sh` acts as a bootstrap around versioned package
installation logic; the user still has one command, and local/offline installation
can share that logic. Never execute downloaded release metadata as shell code.

Before the mutation boundary, newer application/editor builds must report their
app-owned editor counts through the versioned D-Bus update-status protocol,
including new unsaved editors. Any open editor blocks automatic shutdown, even
if currently clean; the installer never closes editors itself. Do not infer this from process
names or an empty registry. A legacy build that lacks the protocol cannot establish
safety automatically: the installer refuses it rather than attempting an unverified
migration. When real legacy-upgrade testing is performed separately, require the
user to save, close and exit it manually; a confirmation alone is not proof of exit.
Unknown status is not safe status.

## In-use update contract

Managed same-ID transactions are implemented
locally, including the separately consented runtime package offer; real
target-system package transactions and release installation remain open gates.
Migration from the private legacy 0.2.0 installation is refused by
the installer; an optional backed-up manual transition is not a public 1.0.0
release gate. Genuine future managed two-version upgrades remain required tests.

- Download and validate the version-matched package before changing the active
  installation. Show the current/target version and any required session restart.
- Before any mutation, collect every required answer and consent, including
  dependency installation, editor closure, service/integration suspension and
  recovery implications. Never send an unconditional `--quit` or kill an editor.
  If safe shutdown cannot be established, leave installed files unchanged and
  explain how to retry the same command. After confirmation, do not prompt again.
- Once the user has confirmed and the exact D-Bus status says it is safe,
  suspend this application's capture integration, stop only its idle background
  service, and replace the owned
  program/integration files with a recoverable previous set. Preserve notes,
  folders, preferences and unrelated GNOME settings. Detect application/editor
  launches during this boundary; a one-time process check is not a sufficient
  interlock.
- Distinguish a completed installation from activation of new Shell code. When
  the extension changed, tell the user that activation needs a new login; never
  log them out automatically. Do not resume incompatible old extension/new app
  combinations or claim capture is active during a required restart boundary.
- Rollback is deliberately tiny: retain one prior installer-owned program and
  integration file set. On an ordinary installation failure, restore that set
  when possible. After a hard interruption, detect the incomplete transaction on
  the next invocation and offer an explicit, understandable recovery path before
  another update. This is not version history, a dependency solver, general
  downgrade support, or database rollback.
- Missing system dependencies are reported separately and installed only with
  explicit consent; the installer itself runs as the ordinary user. Unsupported
  systems receive a clear explanation rather than an untested substitute binary.

The CLI `--quit` and Shell's Service Stop/Restart use the editor-lifetime guard:
open editors block shutdown, regardless of whether their buffers are dirty.
The CLI reports acceptance/refusal to its invoking terminal. It is still not an
updater shutdown API: acceptance does not establish process exit or acquire the
exclusive installer lock. The installer uses the dedicated protocol below;
legacy builds without that protocol cannot be stopped automatically by this route.

GNOME caches loaded extension modules; on Wayland a Shell restart requires a new
login. See the GNOME JavaScript guides on
[loading changed code](https://gjs.guide/extensions/development/creating.html) and
[restarting Shell](https://gjs.guide/extensions/development/debugging.html).

## Required tests before publication

- First install, clean restore, repeat same version, same-ID update, attempted
  downgrade. An optional private legacy 0.2.0 transition is not a public 1.0.0
  migration requirement; same-version installation is not a two-version upgrade.
- Existing custom installation path, spaces in paths, retained settings/data.
- Unsupported system, missing dependency, unavailable/no public release.
- Network failure, truncated script/archive, wrong checksum, unsafe archive.
- Concurrent installation attempt, interrupted replacement and recovery.
- Running app/editors and old/new extension/service identity compatibility.
- Piped invocation vs downloaded-script invocation, including no terminal.
- Uninstall removes owned program files only and retains user data.

Verify restart/recovery behavior on a target desktop before claiming support.
Current installation leaves extension activation as an explicit logout/login
and manual enable step. Missing runtime packages are offered with separate
up-front consent; `--yes` alone does not approve system changes. This
document records the full contract, not a claim that every release gate passed.

## Application-side update protocol (version 1)

The application exposes two methods on its Service D-Bus interface. Downloading,
file replacement and rollback belong to the installer, not these methods.

- `GetUpdateStatus() → s`: JSON with `protocol: 1`, `native_editors`,
  `full_editors`, and `quitting`. Counts follow actual lifetimes, including new
  unsaved windows and pending Full launches, not stored item IDs or process names.
  No document titles, text or dirty-buffer contents are returned.
- `QuitForUpdate() → b`: rechecks those lifetimes on the GTK main thread. `false`
  leaves the application and editors untouched. `true` gates new activations and
  editor creation, flushes the reply to D-Bus and exits the daemon. It does not
  mean the process has already exited or that file replacement is allowed.

The main application, Full helper and backup operation hold a shared nonblocking
`flock` on `$XDG_DATA_HOME/gnome-clip-notes/update.lock` for their lifetimes. The
installer must acquire an exclusive lock on that **same file and profile** after
shutdown, and hold it through replacement/recovery. A newly launched app or helper
then fails before opening its database. Do not delete or replace the lock file:
all participants must keep using the same inode. This is an advisory lock for
cooperating versions, not protection against deliberate account-owner tampering.

Editor counts describe lifetimes managed by the current daemon. For example, a
Full helper left alive after a daemon crash still holds its own shared lock even
if a replacement daemon has no registry entry for it. Zero counts never override
lock contention. Release checks must cover that distinction and legacy versions,
which have neither this protocol nor the runtime lock.

The installer inspects existing bus owners without auto-starting the
application during preflight, addresses the inspected unique owner, and treats a
missing/unknown protocol or a changed owner as an unresolved prerequisite. It
must not use the old unrestricted `--quit` path as a fallback.

Local verification commands:

```sh
cargo test --locked
bash scripts/test-update.sh
bash scripts/test-editor-process.sh
bash scripts/test-editor-process.sh --view
```

These run against temporary profiles, not the user's data. Local archive install,
backup/restore and two-version update checks are also available via
`scripts/test-packaged-lifecycle.py ARCHIVE --upgrade-archive NEWER_ARCHIVE`.
That test uses real binaries but mocked GNOME/D-Bus calls; it is not certification
of a public signed release or a real target desktop session.

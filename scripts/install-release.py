#!/usr/bin/env python3
"""Transactional per-user installer for a verified GnomeClipNotes package.

This helper deliberately does no downloading or attestation verification.  It is
executed after the bootstrap has checked integrity (and provenance when enabled)
and unpacked a release, or by a user who trusts a local package directory.
"""

from __future__ import annotations

import argparse
import ast
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import stat
import subprocess
import sys
import tempfile
import time
from typing import Callable


APP_ID = "io.github.OleksiyM.GnomeClipNotes"
EXTENSION_UUID = "gnome-clip-notes@oleksiym.github.io"
LEGACY_APP_ID = "org.gnome.GnomeClipNotes"
LEGACY_UUID = "gnome-clip-notes@local"
BUS_PATH = "/io/github/OleksiyM/GnomeClipNotes"
BUS_IFACE = APP_ID + ".Service"
RECEIPT = ".install-receipt.json"
SEMVER = re.compile(r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$")
UNSAFE_EXEC = re.compile(r"[\x00-\x1f\x7f\"'`$\\%]")


class InstallError(RuntimeError):
    pass


# Runtime packages only. No repositories, compiler toolchains or gh are added.
RUNTIME_PACKAGES = {
    ('fedora', '44'): ('gtk4', 'libadwaita', 'libsoup3', 'webkitgtk6.0', 'glib2', 'glibc-common', 'systemd', 'gnome-shell'),
    ('ubuntu', '26.04'): ('libgtk-4-1', 'libadwaita-1-0', 'libsoup-3.0-0', 'libwebkitgtk-6.0-4',
                         'libglib2.0-bin', 'libglib2.0-0t64', 'libc-bin', 'systemd', 'gnome-shell'),
}


def host_platform() -> dict[str, str]:
    try:
        os_release = {}
        for line in Path('/etc/os-release').read_text().splitlines():
            if '=' in line:
                key, value = line.split('=', 1)
                os_release[key] = value.strip().strip('"')
    except OSError as error:
        raise InstallError('cannot identify this operating system') from error
    return {'id': os_release.get('ID', ''),
            'version_id': os_release.get('VERSION_ID', ''),
            'arch': platform.machine()}


def release_platform_for(host: dict[str, str]) -> dict[str, str] | None:
    supported = {
        ('fedora', '44', 'x86_64'): ('fedora', '44', 'x86_64'),
        ('fedora', '44', 'aarch64'): ('fedora', '44', 'aarch64'),
        ('ubuntu', '26.04', 'x86_64'): ('fedora', '44', 'x86_64'),
    }
    release = supported.get((host['id'], host['version_id'], host['arch']))
    return dict(zip(('id', 'version_id', 'arch'), release)) if release else None


def dependency_commands(os_id: str, packages: list[str]) -> list[list[str]]:
    allowed = {name for (candidate, _), names in RUNTIME_PACKAGES.items()
               if candidate == os_id for name in names}
    if not packages or any(name not in allowed for name in packages):
        raise InstallError('Invalid system package plan')
    if os_id == 'fedora':
        return [['/usr/bin/dnf', '-y', '--setopt=install_weak_deps=False', 'install', *packages]]
    return [['/usr/bin/apt-get', 'update'],
            ['/usr/bin/apt-get', '-y', '--no-remove', '--no-install-recommends',
             '-o', 'Dpkg::Options::=--force-confdef', '-o', 'Dpkg::Options::=--force-confold',
             'install', *packages]]


def _version(value: object) -> tuple[int, int, int]:
    if not isinstance(value, str) or not (match := SEMVER.fullmatch(value)):
        raise InstallError(f"invalid stable version: {value!r}")
    return tuple(map(int, match.groups()))


def _private_dir(path: Path) -> None:
    path.mkdir(parents=True, exist_ok=True, mode=0o700)
    path.chmod(0o700)


def _regular_owned(path: Path) -> None:
    info = path.lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != os.geteuid() or info.st_nlink != 1:
        raise InstallError(f"unsafe lock file: {path}")


class Commands:
    """Small mockable boundary around session commands."""

    required = ("gdbus", "gsettings", "gnome-extensions", "glib-compile-schemas", "ldd", "systemd-run")

    @staticmethod
    def _missing_tools(missing: list[str]) -> None:
        if missing:
            raise InstallError("Missing required commands: " + ", ".join(missing) +
                               ". Install these tools from your distribution's trusted packages, "
                               "then rerun the installer. No program files were changed.")

    def check_dependencies(self) -> None:
        missing = [name for name in self.required if shutil.which(name) is None]
        self._missing_tools(missing)

    def check_uninstall_dependencies(self) -> None:
        missing = [name for name in ("gdbus", "gsettings", "gnome-extensions")
                   if shutil.which(name) is None]
        self._missing_tools(missing)

    def run(self, argv: list[str], *, check: bool = True) -> subprocess.CompletedProcess[str]:
        # Parse tool output in a known locale without changing the user's session.
        env = dict(os.environ, LC_ALL='C', LANGUAGE='C')
        return subprocess.run(argv, text=True, capture_output=True, check=check,
                              timeout=15, env=env)

    def check_runtime(self, package: Path) -> None:
        problems = []
        for name in ("gnome-clip-notes", "gnome-clip-notes-editor"):
            result = self.run(["ldd", str(package / "target/release" / name)], check=False)
            output = result.stdout + '\n' + result.stderr
            failures = [line.strip() for line in output.splitlines() if 'not found' in line]
            if failures:
                problems.append(name + ':\n  ' + '\n  '.join(dict.fromkeys(failures)))
            elif result.returncode:
                detail = output.strip()[:2000] or 'no diagnostic output'
                problems.append(f'{name}: runtime check failed (exit {result.returncode}): {detail}')
        if problems:
            raise InstallError('Runtime dependencies are missing or incompatible:\n' +
                               '\n'.join(problems) +
                               '\nUse runtime packages for this OS release (GTK4, libadwaita, '
                               'WebKitGTK 6.0 and their dependencies), then rerun the installer. '
                               'No program files were changed.')

    def dependency_plan(self, package: Path, platform_info: dict) -> tuple[str, list[str]] | None:
        try:
            self.check_dependencies()
            self.check_runtime(package)
            return None
        except InstallError as error:
            diagnosis = str(error)
        key = (platform_info['id'], platform_info['version_id'])
        candidates = RUNTIME_PACKAGES.get(key)
        if candidates is None:
            raise InstallError(diagnosis + '\nNo automatic package plan for this OS release.')
        missing = []
        for name in candidates:
            if key[0] == 'fedora':
                result = self.run(['/usr/bin/rpm', '-q', name], check=False)
                installed = result.returncode == 0
            else:
                result = self.run(['/usr/bin/dpkg-query', '-W', '-f=${Status}', name], check=False)
                installed = result.returncode == 0 and result.stdout.strip() == 'install ok installed'
            if result.returncode not in (0, 1):
                raise InstallError('Cannot inspect the system package database; no packages were installed.')
            if not installed:
                missing.append(name)
        if not missing:
            raise InstallError(diagnosis + '\nRuntime packages are already installed. '
                               'Repair the system/runtime mismatch manually; no broad upgrade will be attempted.')
        print(diagnosis)
        return key[0], missing

    def authorize_dependencies(self, *, noninteractive: bool) -> None:
        try:
            if noninteractive:
                subprocess.run(['/usr/bin/sudo', '-n', '-v'], check=True, stdin=subprocess.DEVNULL, timeout=120)
            else:
                with open('/dev/tty', 'r+') as tty:
                    subprocess.run(['/usr/bin/sudo', '-v'], check=True, stdin=tty, stdout=tty, stderr=tty, timeout=120)
        except (OSError, subprocess.SubprocessError) as error:
            raise InstallError('Could not authorize system package installation. No packages or '
                               'ClipNotes files were changed. Use the manual commands above or retry interactively.') from error

    def install_dependencies(self, plan: tuple[str, list[str]]) -> None:
        try:
            for command in dependency_commands(*plan):
                subprocess.run(['/usr/bin/sudo', '-n', '/usr/bin/env', 'LC_ALL=C',
                                'DEBIAN_FRONTEND=noninteractive', 'NEEDRESTART_MODE=l', *command],
                               check=True, stdin=subprocess.DEVNULL, timeout=1800)
        except (OSError, subprocess.SubprocessError) as error:
            raise InstallError('System package installation did not complete. ClipNotes program files '
                               'were not changed. System package changes may remain; inspect the package '
                               'manager output before retrying. No automatic system-package rollback.') from error

    def bus_owner(self) -> str | None:
        base = ["gdbus", "call", "--session", "--dest", "org.freedesktop.DBus",
                "--object-path", "/org/freedesktop/DBus", "--method"]
        result = self.run(base + ["org.freedesktop.DBus.NameHasOwner", APP_ID]).stdout.strip()
        if result != "(true,)":
            if result == "(false,)":
                return None
            raise InstallError("could not understand D-Bus ownership response")
        output = self.run(base + ["org.freedesktop.DBus.GetNameOwner", APP_ID]).stdout.strip()
        try:
            owner = ast.literal_eval(output)[0]
        except (ValueError, SyntaxError, TypeError, IndexError) as error:
            raise InstallError("could not determine the application's unique D-Bus owner") from error
        if not isinstance(owner, str) or not owner.startswith(":"):
            raise InstallError("application returned an invalid unique D-Bus owner")
        return owner

    def update_status(self, owner: str) -> dict[str, object]:
        output = self.run(["gdbus", "call", "--session", "--dest", owner,
                           "--object-path", BUS_PATH, "--method",
                           BUS_IFACE + ".GetUpdateStatus"]).stdout.strip()
        try:
            raw = ast.literal_eval(output)[0]
            value = json.loads(raw)
        except (ValueError, SyntaxError, TypeError, IndexError, json.JSONDecodeError) as error:
            raise InstallError("application update status is invalid") from error
        if not isinstance(value, dict):
            raise InstallError("application update status is invalid")
        return value

    def quit_for_update(self, owner: str) -> bool:
        output = self.run(["gdbus", "call", "--session", "--dest", owner,
                           "--object-path", BUS_PATH, "--method",
                           BUS_IFACE + ".QuitForUpdate"]).stdout.strip()
        if output not in ("(true,)", "(false,)"):
            raise InstallError("application returned an invalid shutdown response")
        return output == "(true,)"

    def extension_enabled(self) -> bool:
        output = self.run(["gsettings", "get", "org.gnome.shell",
                           "enabled-extensions"]).stdout.strip()
        try:
            enabled = ast.literal_eval(output.removeprefix('@as '))
        except (ValueError, SyntaxError) as error:
            raise InstallError("cannot understand enabled extension list") from error
        if not isinstance(enabled, list) or any(not isinstance(item, str) for item in enabled):
            raise InstallError("cannot understand enabled extension list")
        return EXTENSION_UUID in enabled

    def disable_extension(self) -> None:
        self.run(["gnome-extensions", "disable", EXTENSION_UUID])

    def enable_extension(self) -> None:
        self.run(["gnome-extensions", "enable", EXTENSION_UUID])


class Installer:
    def __init__(self, package: Path, app_dir: Path, *, yes: bool = False,
                 install_deps: bool = False,
                 commands: Commands | None = None, home: Path | None = None,
                 data_home: Path | None = None, config_home: Path | None = None,
                 mutation_hook: Callable[[int], None] | None = None):
        self.package = package.resolve()
        self.app_dir = Path(os.path.abspath(app_dir.expanduser()))
        self.yes = yes
        self.install_deps = install_deps
        self.commands = commands or Commands()
        self.home = home or Path.home()
        self.data_home = Path(os.path.abspath(data_home or os.environ.get(
            "XDG_DATA_HOME", self.home / ".local/share")))
        self.config_home = Path(os.path.abspath(config_home or os.environ.get(
            "XDG_CONFIG_HOME", self.home / ".config")))
        digest = hashlib.sha256(os.fsencode(str(self.app_dir))).hexdigest()[:24]
        self.state_dir = self.data_home / "gnome-clip-notes/installer" / digest
        self.snapshot_dir = self.state_dir / "snapshot"
        self.marker = self.state_dir / "incomplete.json"
        self.install_lock_path = self.state_dir / "installer.lock"
        self.runtime_lock_path = self.data_home / "gnome-clip-notes/update.lock"
        self.mutation_hook = mutation_hook
        self.manifest: dict[str, object] = {}
        self.host_platform: dict[str, str] = {}
        self.targets: dict[Path, tuple[str, Path | bytes]] = {}

    def _package_file(self, relative: str) -> Path:
        path = self.package / relative
        if not path.is_file() or path.is_symlink():
            raise InstallError(f"package is missing regular file: {relative}")
        return path

    def validate_package(self) -> None:
        if not self.package.is_dir() or self.package.is_symlink():
            raise InstallError("package directory is missing or unsafe")
        for root, dirs, files in os.walk(self.package, followlinks=False):
            for name in dirs + files:
                path = Path(root, name)
                mode = path.lstat().st_mode
                if stat.S_ISLNK(mode) or not (stat.S_ISDIR(mode) or stat.S_ISREG(mode)):
                    raise InstallError(f"package contains a link or special file: {path}")
        try:
            self.manifest = json.loads(self._package_file("release.json").read_text())
        except (OSError, json.JSONDecodeError) as error:
            raise InstallError("release.json is unreadable or invalid") from error
        expected = {"format": 1, "app_id": APP_ID, "extension_uuid": EXTENSION_UUID,
                    "update_protocol": 1}
        for key, value in expected.items():
            if self.manifest.get(key) != value:
                raise InstallError(f"unsupported release metadata: {key}")
        _version(self.manifest.get("version"))
        release_platform = self.manifest.get("platform")
        if not isinstance(release_platform, dict):
            raise InstallError("release platform metadata is missing")
        self.host_platform = host_platform()
        if release_platform != release_platform_for(self.host_platform):
            raise InstallError("release package does not match a verified OS version and architecture")
        for path in ("target/release/gnome-clip-notes",
                     "target/release/gnome-clip-notes-editor",
                     f"data/{APP_ID}.svg", f"data/{APP_ID}.desktop.in",
                     f"data/{APP_ID}.service.in", f"data/{APP_ID}-autostart.desktop.in",
                     "extension/metadata.json", "scripts/install-release.py"):
            self._package_file(path)
        locales = self.package / "target/locales"
        if not locales.is_dir() or locales.is_symlink():
            raise InstallError("package is missing safe target/locales directory")
        metadata = json.loads(self._package_file("extension/metadata.json").read_text())
        if metadata.get("uuid") != EXTENSION_UUID:
            raise InstallError("extension metadata has the wrong UUID")
        if UNSAFE_EXEC.search(str(self.app_dir / "gnome-clip-notes")):
            raise InstallError("installation path contains characters unsafe in desktop Exec fields")
        self._build_targets()
        for target in self.targets:
            self._reject_symlink_ancestors(target)

    def _reject_symlink_ancestors(self, target: Path) -> None:
        target = Path(os.path.abspath(target))
        roots = [Path(os.path.abspath(root)) for root in
                 (self.app_dir, self.data_home, self.config_home)]
        root = next((item for item in roots if target == item or item in target.parents), None)
        if root is None:
            raise InstallError(f"installation target escapes configured roots: {target}")
        current = Path(target.anchor)
        for part in target.parent.parts[1:]:
            current /= part
            if current.is_symlink():
                candidate = current
                raise InstallError(f"installation target has a symlinked directory: {candidate}")

    def _render(self, template: Path) -> bytes:
        text = template.read_text()
        if text.count("@BINARY@") != 1:
            raise InstallError(f"invalid integration template: {template.name}")
        return text.replace("@BINARY@", str(self.app_dir / "gnome-clip-notes")).encode()

    def _build_targets(self) -> None:
        p, d, c = self.package, self.data_home, self.config_home
        self.targets = {
            self.app_dir / "gnome-clip-notes": ("file", p / "target/release/gnome-clip-notes"),
            self.app_dir / "gnome-clip-notes-editor": ("file", p / "target/release/gnome-clip-notes-editor"),
            self.app_dir / "install-release.py": ("file", p / "scripts/install-release.py"),
            self.app_dir / "share/locale": ("tree", p / "target/locales"),
            d / f"gnome-shell/extensions/{EXTENSION_UUID}": ("tree", p / "extension"),
            d / f"applications/{APP_ID}.desktop": ("bytes", self._render(p / f"data/{APP_ID}.desktop.in")),
            d / f"dbus-1/services/{APP_ID}.service": ("bytes", self._render(p / f"data/{APP_ID}.service.in")),
            d / f"icons/hicolor/scalable/apps/{APP_ID}.svg": ("file", p / f"data/{APP_ID}.svg"),
        }
        settings = self.config_home / "gnome-clip-notes/settings.json"
        open_at_login = True
        if settings.is_file() and not settings.is_symlink():
            try:
                open_at_login = json.loads(settings.read_text()).get("open_at_login", True) is not False
            except (OSError, json.JSONDecodeError):
                pass
        if open_at_login:
            self.targets[c / f"autostart/{APP_ID}.desktop"] = (
                "bytes", self._render(p / f"data/{APP_ID}-autostart.desktop.in"))

    def _legacy_paths(self) -> list[Path]:
        return [self.data_home / f"applications/{LEGACY_APP_ID}.desktop",
                self.data_home / f"dbus-1/services/{LEGACY_APP_ID}.service",
                self.data_home / f"icons/hicolor/scalable/apps/{LEGACY_APP_ID}.svg",
                self.data_home / f"gnome-shell/extensions/{LEGACY_UUID}",
                self.config_home / f"autostart/{LEGACY_APP_ID}.desktop"]

    def _receipt(self) -> dict[str, object] | None:
        path = self.app_dir / RECEIPT
        if not path.exists():
            return None
        if path.is_symlink() or not path.is_file():
            raise InstallError("installation receipt is unsafe")
        try:
            value = json.loads(path.read_text())
        except (OSError, json.JSONDecodeError) as error:
            raise InstallError("installation receipt is invalid") from error
        if not isinstance(value, dict) or value.get("schema") != 1 or not isinstance(value.get("owned_paths"), list):
            raise InstallError("installation receipt is invalid")
        _version(value.get("version"))
        allowed = {str(path) for path in self._allowed_targets() - {self.app_dir / RECEIPT}}
        if any(not isinstance(item, str) or item not in allowed for item in value["owned_paths"]):
            raise InstallError("installation receipt claims an unexpected path")
        if len(set(value['owned_paths'])) != len(value['owned_paths']):
            raise InstallError("installation receipt contains duplicate paths")
        required = allowed - {str(self.config_home / f'autostart/{APP_ID}.desktop')}
        if not required.issubset(set(value['owned_paths'])):
            raise InstallError("installation receipt is missing mandatory owned paths")
        return value

    def _allowed_targets(self) -> set[Path]:
        return {
            self.app_dir / "gnome-clip-notes",
            self.app_dir / "gnome-clip-notes-editor",
            self.app_dir / "install-release.py",
            self.app_dir / "share/locale",
            self.data_home / f"gnome-shell/extensions/{EXTENSION_UUID}",
            self.data_home / f"applications/{APP_ID}.desktop",
            self.data_home / f"dbus-1/services/{APP_ID}.service",
            self.data_home / f"icons/hicolor/scalable/apps/{APP_ID}.svg",
            self.config_home / f"autostart/{APP_ID}.desktop",
            self.app_dir / RECEIPT,
        }

    def _receipt_file_inventory(self, receipt: dict[str, object]) -> tuple[list[str], set[Path]]:
        files = receipt.get('owned_files')
        if not isinstance(files, list) or any(not isinstance(item, str) for item in files):
            raise InstallError("receipt lacks a file inventory; automatic update or uninstall is not supported")
        owned = {Path(item) for item in receipt['owned_paths']}
        trees = {self.app_dir / 'share/locale',
                 self.data_home / f'gnome-shell/extensions/{EXTENSION_UUID}'} & owned
        if len(set(files)) != len(files):
            raise InstallError("receipt contains duplicate file inventory entries")
        if not (owned - trees).issubset({Path(item) for item in files}):
            raise InstallError("receipt file inventory is missing mandatory program files")
        for name in files:
            path = Path(name)
            if (str(Path(os.path.abspath(path))) != name or
                    not (path in owned - trees or any(root in path.parents for root in trees))):
                raise InstallError("receipt file inventory claims an unexpected path")
            self._reject_symlink_ancestors(path)
            if path.exists() or path.is_symlink():
                if not stat.S_ISREG(path.lstat().st_mode):
                    raise InstallError(f"recorded program file is not regular: {path}")
        return files, trees

    def _reject_unrecorded_tree_files(self, receipt: dict[str, object]) -> None:
        files, trees = self._receipt_file_inventory(receipt)
        recorded = {Path(item) for item in files}
        for tree in sorted(trees, key=str):
            if not tree.is_dir():
                continue
            for path in sorted(tree.rglob('*'), key=str):
                if path.is_file() and path not in recorded:
                    raise InstallError(
                        f"installed program tree contains an unrecorded file: {path}. "
                        "Move it aside before updating")

    def preflight_installation(self, *, skip_session: bool = False) -> tuple[dict[str, object] | None, str | None]:
        if any(path.exists() or path.is_symlink() for path in self._legacy_paths()):
            raise InstallError("legacy migration not supported; keep the existing installation until a separately backed-up manual transition")
        receipt = self._receipt()
        modern = set(self.targets) | {self.config_home / f"autostart/{APP_ID}.desktop"}
        if receipt is None and any(path.exists() or path.is_symlink() for path in modern):
            raise InstallError("existing modern installation is not owned by this installer (receipt missing)")
        if receipt:
            owned = {Path(item) for item in receipt["owned_paths"]}
            unowned = [path for path in modern if path not in owned and
                       (path.exists() or path.is_symlink())]
            if unowned:
                raise InstallError(f"existing modern target is not installer-owned: {unowned[0]}")
            current, target = _version(receipt["version"]), _version(self.manifest["version"])
            if target < current:
                raise InstallError("downgrades are not supported")
            for path in owned:
                self._reject_symlink_ancestors(path)
                if path.is_dir():
                    self._validate_tree(path, "installed owned tree")
            self._reject_unrecorded_tree_files(receipt)
        if self.marker.exists() or self.marker.is_symlink():
            raise InstallError("an interrupted installation needs recovery; run " +
                               self._recovery_command())
        owner = None if skip_session else self.commands.bus_owner()
        self._validate_status(owner)
        return receipt, owner

    def _validate_status(self, owner: str | None) -> None:
        if owner is None:
            return
        status = self.commands.update_status(owner)
        if status.get("protocol") != 1:
            raise InstallError("running application uses an unknown update protocol")
        for field in ("native_editors", "full_editors"):
            value = status.get(field)
            if not isinstance(value, int) or isinstance(value, bool) or value < 0:
                raise InstallError("running application returned invalid editor counts")
            if value:
                raise InstallError("close all Native and Full editors before changing the installation")
        if status.get("quitting") is not False:
            raise InstallError("application is already shutting down or returned invalid status")

    def _confirm(self, receipt: dict[str, object] | None, *, uninstall: bool = False) -> None:
        if uninstall:
            print("Save and close all editors. Uninstall stops GnomeClipNotes capture and "
                  "removes only recorded program files. Notes, settings and unrecorded files "
                  "are kept. One recovery snapshot is retained. No automatic logout.")
        else:
            print("All Native and Full editors must be saved and closed. Installation pauses "
                  "GnomeClipNotes capture; load the installed Shell extension only after "
                  "logging out and back in, then enabling it in Extensions.")
        if self.yes:
            return
        action = "uninstall" if uninstall else ("install" if receipt is None else "update")
        version = receipt['version'] if uninstall else self.manifest['version']
        prompt = f"{action.capitalize()} GnomeClipNotes {version} in {self.app_dir}? [y/N] "
        try:
            with open("/dev/tty", "r+", encoding="utf-8") as tty:
                tty.write(prompt)
                tty.flush()
                answer = tty.readline().strip().lower()
        except OSError as error:
            raise InstallError("confirmation requires /dev/tty; use --yes for explicit noninteractive acceptance") from error
        if answer not in ("y", "yes"):
            raise InstallError("installation cancelled")

    def _lock(self, path: Path, *, timeout: float = 0) -> object:
        self._reject_symlink_ancestors(path)
        _private_dir(path.parent)
        deadline = time.monotonic() + timeout
        while True:
            flags = os.O_RDWR | os.O_CREAT | os.O_NONBLOCK | getattr(os, "O_NOFOLLOW", 0)
            try:
                fd = os.open(path, flags, 0o600)
            except OSError as error:
                raise InstallError(f"cannot open update lock safely: {path}") from error
            handle = None
            try:
                _regular_owned(path)
                opened, named = os.fstat(fd), path.lstat()
                if (opened.st_dev, opened.st_ino) != (named.st_dev, named.st_ino):
                    raise InstallError(f"lock path changed while opening it: {path}")
                handle = os.fdopen(fd, "r+")
                fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
                return handle
            except BlockingIOError as error:
                assert handle is not None
                handle.close()
                if time.monotonic() >= deadline:
                    raise InstallError(f"cannot acquire exclusive update lock: {path}") from error
                time.sleep(0.05)
            except (OSError, InstallError) as error:
                if handle is None:
                    os.close(fd)
                else:
                    handle.close()
                raise InstallError(f"cannot acquire exclusive update lock: {path}") from error

    @staticmethod
    def _validate_tree(path: Path, description: str) -> None:
        for root, dirs, files in os.walk(path, followlinks=False):
            for name in dirs + files:
                item = Path(root, name)
                mode = item.lstat().st_mode
                if stat.S_ISLNK(mode) or not (stat.S_ISDIR(mode) or stat.S_ISREG(mode)):
                    raise InstallError(f"{description} contains a link or special file: {item}")

    def _snapshot(self, owned: list[Path], *, filewise: bool = False) -> None:
        if self.snapshot_dir.exists():
            if self.snapshot_dir.is_symlink() or self.snapshot_dir.parent != self.state_dir:
                raise InstallError("unsafe installer snapshot path")
            shutil.rmtree(self.snapshot_dir)
        _private_dir(self.snapshot_dir)
        entries = []
        candidates = list(dict.fromkeys(owned + [self.app_dir / RECEIPT]))
        for index, source in enumerate(candidates):
            if not source.exists() and not source.is_symlink():
                continue
            if source.is_symlink():
                raise InstallError(f"installed owned path is a symlink: {source}")
            saved = self.snapshot_dir / str(index)
            if source.is_dir():
                self._validate_tree(source, "installed owned tree")
                shutil.copytree(source, saved)
                kind = "tree"
            elif source.is_file():
                shutil.copy2(source, saved)
                kind = "file"
            else:
                raise InstallError(f"installed owned path is special: {source}")
            entries.append({"path": str(source), "saved": str(index), "kind": kind})
        shutil.copy2(Path(__file__), self.snapshot_dir / "install-release.py")
        transaction_targets = list(dict.fromkeys(owned + ([] if filewise else list(self.targets)) +
                                                  [self.app_dir / RECEIPT]))
        (self.snapshot_dir / "manifest.json").write_text(json.dumps({
            "entries": entries,
            "filewise": filewise,
            "transaction_targets": [str(path) for path in transaction_targets],
        }) + "\n")

    def _recovery_command(self) -> str:
        helper = self.state_dir / 'recovery.py'
        if not helper.is_file():
            helper = self.snapshot_dir / "install-release.py"
        return "python3 {} --restore --app-dir {}".format(
            shlex.quote(str(helper)), shlex.quote(str(self.app_dir)))

    def _write_marker(self, phase: str, uninstall: bool) -> None:
        fd, name = tempfile.mkstemp(prefix='.transaction-', dir=self.state_dir)
        try:
            with os.fdopen(fd, 'w') as stream:
                json.dump({'phase': phase, 'operation': 'uninstall' if uninstall else 'install'}, stream)
                stream.flush()
                os.fsync(stream.fileno())
            os.replace(name, self.marker)
        finally:
            Path(name).unlink(missing_ok=True)

    @staticmethod
    def _remove_owned(path: Path) -> None:
        if path.is_symlink() or path.is_file():
            path.unlink()
        elif path.is_dir():
            shutil.rmtree(path)

    def _restore_snapshot(self) -> None:
        manifest_path = self.snapshot_dir / "manifest.json"
        if not manifest_path.is_file() or manifest_path.is_symlink():
            raise InstallError("recovery snapshot is missing or unsafe")
        try:
            data = json.loads(manifest_path.read_text())
        except (OSError, json.JSONDecodeError) as error:
            raise InstallError("recovery snapshot manifest is unreadable or invalid") from error
        if not isinstance(data, dict):
            raise InstallError("recovery snapshot manifest is invalid")
        entries = data.get("entries")
        transaction_targets = data.get("transaction_targets")
        if not isinstance(entries, list) or not isinstance(transaction_targets, list):
            raise InstallError("recovery snapshot manifest is invalid")
        allowed = self._allowed_targets()
        filewise = data.get('filewise', False)
        if not isinstance(filewise, bool):
            raise InstallError("recovery snapshot mode is invalid")
        trees = {self.app_dir / 'share/locale',
                 self.data_home / f'gnome-shell/extensions/{EXTENSION_UUID}'}
        def permitted(path):
            if not filewise:
                return path in allowed
            return (str(Path(os.path.abspath(path))) == str(path) and
                    (path in allowed - trees or any(tree in path.parents for tree in trees)))
        validated_entries = []
        entry_targets = set()
        for entry in entries:
            if not isinstance(entry, dict):
                raise InstallError("recovery snapshot entry is invalid")
            target = Path(entry.get("path", ""))
            saved_name = entry.get("saved")
            if not isinstance(saved_name, str) or not saved_name.isdecimal():
                raise InstallError("recovery snapshot entry is invalid")
            saved = self.snapshot_dir / saved_name
            kind = entry.get("kind")
            if (not permitted(target) or target in entry_targets or saved.is_symlink() or
                    (filewise and kind != 'file')):
                raise InstallError("recovery snapshot claims an unexpected path")
            entry_targets.add(target)
            if kind == "tree" and saved.is_dir():
                self._validate_tree(saved, "recovery snapshot")
            elif kind != "file" or not saved.is_file():
                raise InstallError("recovery snapshot entry is invalid")
            validated_entries.append((target, saved, kind))
        targets = [Path(item) for item in transaction_targets
                   if isinstance(item, str) and permitted(Path(item))]
        if len(targets) != len(transaction_targets) or len(set(targets)) != len(targets):
            raise InstallError("recovery snapshot target list is invalid")
        if not entry_targets.issubset(set(targets)):
            raise InstallError("recovery snapshot entry is outside its transaction")
        for target in targets:
            self._reject_symlink_ancestors(target)
            if filewise and target.exists() and not target.is_file():
                raise InstallError(f"recovery file target is not a regular file: {target}")
        for target in targets:
            if target.exists() or target.is_symlink():
                self._remove_owned(target)
        for target, saved, kind in validated_entries:
            target.parent.mkdir(parents=True, exist_ok=True)
            if kind == "tree":
                shutil.copytree(saved, target)
            else:
                shutil.copy2(saved, target)

    def _write_target(self, target: Path, kind: str, source: Path | bytes) -> None:
        target.parent.mkdir(parents=True, exist_ok=True)
        if target.exists() or target.is_symlink():
            self._remove_owned(target)
        if kind == "tree":
            assert isinstance(source, Path)
            shutil.copytree(source, target)
        else:
            fd, temporary = tempfile.mkstemp(prefix=".gcn-install-", dir=target.parent)
            try:
                with os.fdopen(fd, "wb") as output:
                    if kind == "bytes":
                        assert isinstance(source, bytes)
                        output.write(source)
                    else:
                        assert isinstance(source, Path)
                        with source.open("rb") as input_file:
                            shutil.copyfileobj(input_file, output)
                if kind == "file" and Path(source).stat().st_mode & 0o111:
                    os.chmod(temporary, 0o755)
                else:
                    os.chmod(temporary, 0o644)
                os.replace(temporary, target)
            finally:
                if os.path.exists(temporary):
                    os.unlink(temporary)

    def install(self) -> str:
        if os.geteuid() == 0:
            raise InstallError("refusing to install as root")
        self.validate_package()
        # Reject ownership/legacy problems before offering system changes.
        receipt, owner = self.preflight_installation(skip_session=shutil.which('gdbus') is None)
        plan = self.commands.dependency_plan(self.package, self.host_platform)
        if receipt and receipt["version"] == self.manifest["version"] and plan is None:
            return "already installed"
        if plan:
            self._confirm_dependencies(plan)
        self._confirm(receipt)
        if plan:
            self.commands.authorize_dependencies(noninteractive=self.yes)
            self.commands.install_dependencies(plan)
            print('System runtime packages installed. They remain installed if a later ClipNotes check fails.')
            # No questions after system mutations. Revalidate before stopping capture.
            self.commands.check_dependencies()
            self.commands.check_runtime(self.package)
            current_receipt, owner = self.preflight_installation()
            if current_receipt != receipt:
                raise InstallError('Installation changed during dependency setup; retry. System packages remain installed.')
            if receipt and receipt['version'] == self.manifest['version']:
                return 'already installed; runtime dependencies repaired'
        self._transact(receipt, owner)
        return (f"installed {self.manifest['version']} in {self.app_dir}.\n"
                "Program files are installed; clipboard capture has not been activated by the installer.\n"
                "1. Log out and back in to load the installed Shell code.\n"
                "2. Enable GnomeClipNotes in Extensions, or run after logging back in:\n"
                f"   gnome-extensions enable {EXTENSION_UUID}\n"
                "If GNOME has disabled all user extensions, turn on the main Extensions switch first.")

    def _confirm_dependencies(self, plan: tuple[str, list[str]]) -> None:
        manual = '\n'.join(shlex.join(['sudo', *command]) for command in dependency_commands(*plan))
        print('Missing runtime packages: ' + ', '.join(plan[1]))
        print('The system package manager may add/update their dependencies from configured repositories. '
              'These system changes are not rolled back if ClipNotes installation fails. '
              'The application itself remains a per-user installation.\nManual commands:\n' + manual)
        if self.install_deps:
            return
        if self.yes:
            raise InstallError('System package consent is separate: use --install-deps with --yes, '
                               'or run the manual commands above. Nothing was installed.')
        try:
            with open('/dev/tty', 'r+') as tty:
                tty.write('Allow installation of these system packages? [y/N] ')
                tty.flush()
                accepted = tty.readline().strip().lower() in ('y', 'yes')
        except OSError as error:
            raise InstallError('Package consent needs /dev/tty; use the manual commands above.') from error
        if not accepted:
            raise InstallError('System package installation declined. Run the manual commands above and retry.')

    def _installed_files(self) -> list[str]:
        files = []
        for target, (kind, _) in self.targets.items():
            if kind == 'tree':
                files.extend(str(path) for path in target.rglob('*') if path.is_file())
            else:
                files.append(str(target))
        return sorted(files)

    def _uninstall_preflight(self) -> tuple[dict[str, object], str | None]:
        self._reject_symlink_ancestors(self.app_dir / RECEIPT)
        if self.marker.exists() or self.marker.is_symlink():
            raise InstallError("an interrupted installation needs recovery; run " + self._recovery_command())
        receipt = self._receipt()
        if receipt is None:
            raise InstallError("no managed installation receipt; nothing will be removed")
        owned = {Path(item) for item in receipt['owned_paths']}
        self._receipt_file_inventory(receipt)
        for path in owned:
            self._reject_symlink_ancestors(path)
            if path.is_symlink():
                raise InstallError(f"installed owned path is a symlink: {path}")
            if path.is_dir():
                self._validate_tree(path, 'installed owned tree')
        owner = self.commands.bus_owner()
        self._validate_status(owner)
        return receipt, owner

    def uninstall(self) -> str:
        if os.geteuid() == 0:
            raise InstallError("refusing to uninstall as root")
        self.commands.check_uninstall_dependencies()
        receipt, owner = self._uninstall_preflight()
        self._confirm(receipt, uninstall=True)
        self._transact(receipt, owner, uninstall=True)
        return ("Removed recorded GnomeClipNotes program files. Notes, settings and "
                "unrecorded files were kept, together with one recovery snapshot. "
                "Log out/in to unload cached Shell code.")

    def _transact(self, receipt: dict[str, object] | None, owner: str | None,
                  *, uninstall: bool = False) -> None:
        installer_lock = self._lock(self.install_lock_path)
        extension_was_enabled = False
        files_started = False
        stopping_marker = False
        runtime_lock = None
        try:
            current_receipt, locked_owner = (self._uninstall_preflight() if uninstall
                                             else self.preflight_installation())
            if current_receipt != receipt or locked_owner != owner:
                raise InstallError("installation state changed after confirmation; retry")
            current_owner = self.commands.bus_owner()
            if current_owner != owner:
                raise InstallError("application D-Bus owner changed after confirmation; retry")
            self._validate_status(current_owner)
            extension_was_enabled = self.commands.extension_enabled()
            # Record the restart boundary before disabling capture. If interrupted
            # here, files are unchanged; recovery must not use an older snapshot.
            recovery = self.state_dir / 'recovery.py'
            if recovery.is_symlink():
                raise InstallError('unsafe saved recovery helper')
            shutil.copy2(Path(__file__), recovery)
            self._write_marker('stopping', uninstall)
            stopping_marker = True
            if extension_was_enabled:
                self.commands.disable_extension()
            if current_owner is not None and not self.commands.quit_for_update(current_owner):
                raise InstallError("application refused safe shutdown")
            runtime_lock = self._lock(self.runtime_lock_path, timeout=5.0)
            prior = [Path(item) for item in receipt["owned_paths"]] if receipt else []
            self._snapshot([Path(item) for item in receipt['owned_files']] if uninstall else prior,
                           filewise=uninstall)
            self._write_marker('files', uninstall)
            files_started = True
            try:
                if uninstall:
                    assert receipt is not None
                    for index, name in enumerate(receipt['owned_files']):
                        Path(name).unlink(missing_ok=True)
                        if self.mutation_hook:
                            self.mutation_hook(index)
                    (self.app_dir / RECEIPT).unlink()
                    # Remove only empty directories; never recursively delete
                    # the contents of an owned tree during uninstall.
                    trees = [Path(item) for item in receipt['owned_paths'] if Path(item).is_dir()]
                    for tree in trees:
                        for directory in sorted(tree.rglob('*'), key=lambda p: len(p.parts), reverse=True):
                            if directory.is_dir():
                                try:
                                    directory.rmdir()
                                except OSError:
                                    pass
                        try:
                            tree.rmdir()
                        except OSError:
                            pass
                    self.marker.unlink()
                    return
                for obsolete in set(prior) - set(self.targets):
                    if obsolete.exists() or obsolete.is_symlink():
                        self._remove_owned(obsolete)
                for index, (target, (kind, source)) in enumerate(self.targets.items()):
                    self._write_target(target, kind, source)
                    if self.mutation_hook:
                        self.mutation_hook(index)
                schemas = self.data_home / f"gnome-shell/extensions/{EXTENSION_UUID}/schemas"
                self.commands.run(["glib-compile-schemas", str(schemas)])
                receipt_data = {"schema": 1, "version": self.manifest["version"],
                                "owned_paths": [str(path) for path in self.targets],
                                "owned_files": self._installed_files()}
                self._write_target(self.app_dir / RECEIPT, "bytes",
                                   (json.dumps(receipt_data, indent=2) + "\n").encode())
                self.marker.unlink()
            except Exception:
                try:
                    self._restore_snapshot()
                    self.marker.unlink(missing_ok=True)
                except Exception as rollback_error:
                    raise InstallError("automatic rollback failed; leave the extension disabled "
                                       "and recover with " + self._recovery_command()) from rollback_error
                raise
        except Exception:
            if stopping_marker and not files_started:
                self.marker.unlink(missing_ok=True)
            if extension_was_enabled and not self.marker.exists():
                try:
                    self.commands.enable_extension()
                except Exception:
                    pass
            raise
        finally:
            if runtime_lock:
                runtime_lock.close()
            installer_lock.close()

    def restore(self) -> str:
        if os.geteuid() == 0:
            raise InstallError("refusing to restore as root")
        if not self.marker.is_file() or self.marker.is_symlink():
            raise InstallError("there is no interrupted installation to restore")
        installer_lock = self._lock(self.install_lock_path)
        runtime_lock = None
        try:
            try:
                marker = json.loads(self.marker.read_text())
            except (OSError, json.JSONDecodeError) as error:
                raise InstallError('recovery marker is unreadable or invalid') from error
            if not isinstance(marker, dict):
                raise InstallError('recovery marker is invalid')
            if marker.get('phase') == 'stopping':
                self.marker.unlink()
                return ('Interruption occurred before program files changed. Files left intact. '
                        'Log out/in and enable GnomeClipNotes in Extensions to resume capture if needed.')
            if marker.get('phase') not in (None, 'files'):
                raise InstallError('unknown recovery transaction phase')
            runtime_lock = self._lock(self.runtime_lock_path)
            self._restore_snapshot()
            self.marker.unlink()
        finally:
            if runtime_lock:
                runtime_lock.close()
            installer_lock.close()
        return ("restored the previous installer-owned files. Log out/in and enable "
                "GnomeClipNotes in Extensions to resume capture if needed.")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--package-dir", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--app-dir", type=Path, default=Path.home() / "Applications/GnomeClipNotes")
    parser.add_argument("--yes", action="store_true")
    parser.add_argument("--install-deps", action="store_true", help="Explicit consent to install missing system runtime packages")
    operation = parser.add_mutually_exclusive_group()
    operation.add_argument("--restore", action="store_true")
    operation.add_argument("--uninstall", action="store_true")
    args = parser.parse_args(argv)
    installer = Installer(args.package_dir, args.app_dir, yes=args.yes, install_deps=args.install_deps)
    try:
        message = installer.restore() if args.restore else (
            installer.uninstall() if args.uninstall else installer.install())
    except (InstallError, OSError, subprocess.SubprocessError) as error:
        print(f"GnomeClipNotes installer: {error}", file=sys.stderr)
        return 1
    print(message)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

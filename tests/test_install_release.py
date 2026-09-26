import importlib.util
import json
import os
from pathlib import Path
import platform
import pty
import re
import select
import signal
import subprocess
import sys
import tempfile
import time
import traceback
import unittest
from unittest import mock


SCRIPT = Path(__file__).parents[1] / "scripts/install-release.py"
SPEC = importlib.util.spec_from_file_location("install_release", SCRIPT)
install_release = importlib.util.module_from_spec(SPEC)
assert SPEC.loader
SPEC.loader.exec_module(install_release)


class FakeCommands:
    def __init__(self):
        self.owner = None
        self.status = {"protocol": 1, "native_editors": 0, "full_editors": 0,
                       "quitting": False}
        self.enabled = False
        self.disabled = 0
        self.enabled_again = 0
        self.quit_result = True

    def check_dependencies(self):
        pass

    def check_runtime(self, package):
        pass

    def dependency_plan(self, package, platform_info):
        self.check_dependencies()
        self.check_runtime(package)
        return None

    def authorize_dependencies(self, *, noninteractive):
        raise AssertionError('Unexpected privilege request')

    def install_dependencies(self, plan):
        raise AssertionError('Unexpected system installation')

    def check_uninstall_dependencies(self):
        pass

    def bus_owner(self):
        return self.owner

    def update_status(self, owner):
        return dict(self.status)

    def quit_for_update(self, owner):
        return self.quit_result

    def extension_enabled(self):
        return self.enabled

    def disable_extension(self):
        self.disabled += 1

    def enable_extension(self):
        self.enabled_again += 1

    def run(self, argv, check=True):
        return None


def make_package(root: Path, version="1.0.0") -> Path:
    package = root / ("package-" + version)
    files = {
        "target/release/gnome-clip-notes": f"app {version}\n",
        "target/release/gnome-clip-notes-editor": f"editor {version}\n",
        "target/locales/fr/LC_MESSAGES/gnome-clip-notes.mo": f"locale {version}\n",
        "data/io.github.OleksiyM.GnomeClipNotes.svg": "<svg/>\n",
        "data/io.github.OleksiyM.GnomeClipNotes.desktop.in":
            '[Desktop Entry]\nExec="@BINARY@"\n',
        "data/io.github.OleksiyM.GnomeClipNotes.service.in":
            '[D-BUS Service]\nExec="@BINARY@" --daemon\n',
        "data/io.github.OleksiyM.GnomeClipNotes-autostart.desktop.in":
            '[Desktop Entry]\nExec="@BINARY@" --daemon\n',
        "extension/metadata.json": json.dumps({"uuid": install_release.EXTENSION_UUID}),
        "extension/extension.js": f"// {version}\n",
        "extension/schemas/example.gschema.xml": "<schemalist/>\n",
        "scripts/install-release.py": "# saved recovery helper\n",
    }
    for relative, contents in files.items():
        path = package / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(contents)
    for name in ("gnome-clip-notes", "gnome-clip-notes-editor"):
        (package / "target/release" / name).chmod(0o755)
    os_release = {}
    for line in Path("/etc/os-release").read_text().splitlines():
        if "=" in line:
            key, value = line.split("=", 1)
            os_release[key] = value.strip().strip('"')
    manifest = {"format": 1, "version": version,
                "platform": {"id": os_release["ID"],
                             "version_id": os_release["VERSION_ID"],
                             "arch": platform.machine()},
                "app_id": install_release.APP_ID,
                "extension_uuid": install_release.EXTENSION_UUID,
                "update_protocol": 1}
    (package / "release.json").write_text(json.dumps(manifest))
    return package


class InstallerTest(unittest.TestCase):
    def setUp(self):
        # Exercise real ownership, not a mocked UID that disagrees with the
        # files created by CI. Container jobs may start as root; run fixtures
        # unprivileged there and restore credentials only after cleanup.
        original_euid = os.geteuid()
        if original_euid == 0:
            os.seteuid(65534)
            self.addCleanup(os.seteuid, original_euid)
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.home = self.root / "home"
        self.data = self.root / "data"
        self.config = self.root / "config"
        self.app = self.home / "Applications/GnomeClipNotes"
        self.commands = FakeCommands()

    def installer(self, package, app=None, hook=None):
        return install_release.Installer(
            package, app or self.app, yes=True, commands=self.commands,
            home=self.home, data_home=self.data, config_home=self.config,
            mutation_hook=hook)

    def terminal_exchange(self, action, answer):
        # A real controlling terminal, but piped stdin as with curl | bash.
        pipe_read, pipe_write = os.pipe()
        sys.stdout.flush()
        sys.stderr.flush()
        pid, terminal = pty.fork()
        if pid == 0:
            os.close(pipe_write)
            os.dup2(pipe_read, 0)
            os.close(pipe_read)
            try:
                action()
            except BaseException:
                traceback.print_exc()
                sys.stderr.flush()
                os._exit(1)
            sys.stdout.flush()
            os._exit(0)
        os.close(pipe_read)
        os.close(pipe_write)
        output = b''
        sent = False
        status = None
        deadline = time.monotonic() + 5
        try:
            while time.monotonic() < deadline:
                if select.select([terminal], [], [], 0.05)[0]:
                    try:
                        chunk = os.read(terminal, 4096)
                    except OSError:
                        chunk = b''  # Linux reports EIO after the slave closes.
                    output += chunk
                    if not sent and b'[y/N]' in output:
                        os.write(terminal, answer.encode() + b'\n')
                        sent = True
                child, status_value = os.waitpid(pid, os.WNOHANG)
                if child:
                    status = status_value
                    break
            self.assertIsNotNone(status, output.decode(errors='replace'))
            self.assertEqual(os.waitstatus_to_exitcode(status), 0,
                             output.decode(errors='replace'))
        finally:
            if status is None:
                os.kill(pid, signal.SIGKILL)
                os.waitpid(pid, 0)
            os.close(terminal)

    def test_interactive_consents_with_real_terminal_and_piped_stdin(self):
        installer = self.installer(make_package(self.root))
        installer.yes = False
        installer.manifest = {'version': '1.0.0'}
        cases = (
            lambda: installer._confirm(None),
            lambda: installer._confirm({'version': '1.0.0'}, uninstall=True),
            lambda: installer._confirm_dependencies(('fedora', ['webkitgtk6.0'])),
        )
        for index, confirm in enumerate(cases):
            for answer in ('yes', 'no'):
                with self.subTest(prompt=index, answer=answer):
                    def action():
                        if answer == 'yes':
                            confirm()
                        else:
                            with self.assertRaisesRegex(install_release.InstallError, 'cancelled|declined'):
                                confirm()
                    self.terminal_exchange(action, answer)

    def test_interactive_sudo_receives_real_terminal_without_running_sudo(self):
        def action():
            def inspect(argv, **kwargs):
                self.assertEqual(argv, ['/usr/bin/sudo', '-v'])
                for stream in ('stdin', 'stdout', 'stderr'):
                    self.assertTrue(os.isatty(kwargs[stream].fileno()))
            with mock.patch.object(install_release.subprocess, 'run', side_effect=inspect):
                install_release.Commands().authorize_dependencies(noninteractive=False)
        self.terminal_exchange(action, '')

    def test_fresh_install_and_path_with_spaces(self):
        app = self.home / "Applications/My Clip Notes"
        result = self.installer(make_package(self.root), app).install()
        self.assertIn("installed 1.0.0", result)
        self.assertIn('clipboard capture has not been activated', result)
        self.assertIn('after logging back in', result)
        self.assertIn(f'gnome-extensions enable {install_release.EXTENSION_UUID}', result)
        self.assertEqual((app / "gnome-clip-notes").read_text(), "app 1.0.0\n")
        desktop = self.data / f"applications/{install_release.APP_ID}.desktop"
        self.assertIn(f'Exec="{app}/gnome-clip-notes"', desktop.read_text())
        receipt = json.loads((app / install_release.RECEIPT).read_text())
        self.assertEqual(receipt["version"], "1.0.0")

    def test_ubuntu_2604_accepts_fedora_release_and_uses_apt_platform(self):
        package = make_package(self.root)
        release = json.loads((package / 'release.json').read_text())
        release['platform'] = {'id': 'fedora', 'version_id': '44', 'arch': 'x86_64'}
        (package / 'release.json').write_text(json.dumps(release))
        host = {'id': 'ubuntu', 'version_id': '26.04', 'arch': 'x86_64'}
        with mock.patch.object(install_release, 'host_platform', return_value=host), \
             mock.patch.object(self.commands, 'dependency_plan', wraps=self.commands.dependency_plan) as plan:
            self.assertIn('installed 1.0.0', self.installer(package).install())
        self.assertEqual(plan.call_args.args[1], host)

    def test_other_ubuntu_releases_reject_fedora_binary(self):
        package = make_package(self.root)
        release = json.loads((package / 'release.json').read_text())
        release['platform'] = {'id': 'fedora', 'version_id': '44', 'arch': 'x86_64'}
        (package / 'release.json').write_text(json.dumps(release))
        for version in ('22.04', '24.04', '26.04.1'):
            with self.subTest(version=version), \
                 mock.patch.object(install_release, 'host_platform', return_value={
                     'id': 'ubuntu', 'version_id': version, 'arch': 'x86_64'}):
                with self.assertRaisesRegex(install_release.InstallError, 'verified OS version'):
                    self.installer(package).validate_package()

    def test_ubuntu_manifest_must_still_identify_fedora_build(self):
        package = make_package(self.root)
        release = json.loads((package / 'release.json').read_text())
        release['platform'] = {'id': 'ubuntu', 'version_id': '26.04', 'arch': 'x86_64'}
        (package / 'release.json').write_text(json.dumps(release))
        with mock.patch.object(install_release, 'host_platform', return_value={
                'id': 'ubuntu', 'version_id': '26.04', 'arch': 'x86_64'}):
            with self.assertRaisesRegex(install_release.InstallError, 'verified OS version'):
                self.installer(package).validate_package()

    def test_fedora_aarch64_accepts_matching_release(self):
        package = make_package(self.root)
        release = json.loads((package / 'release.json').read_text())
        release['platform'] = {'id': 'fedora', 'version_id': '44', 'arch': 'aarch64'}
        (package / 'release.json').write_text(json.dumps(release))
        with mock.patch.object(install_release, 'host_platform', return_value={
                'id': 'fedora', 'version_id': '44', 'arch': 'aarch64'}):
            self.installer(package).validate_package()

    def test_preserves_unrelated_app_and_user_data(self):
        self.app.mkdir(parents=True)
        (self.app / "mine.txt").write_text("keep")
        user_data = self.data / "gnome-clip-notes/data.db"
        user_data.parent.mkdir(parents=True)
        user_data.write_text("notes")
        settings = self.config / "gnome-clip-notes/settings.json"
        settings.parent.mkdir(parents=True)
        settings.write_text('{"open_at_login": false}')
        self.installer(make_package(self.root)).install()
        self.assertEqual((self.app / "mine.txt").read_text(), "keep")
        self.assertEqual(user_data.read_text(), "notes")
        self.assertEqual(settings.read_text(), '{"open_at_login": false}')
        self.assertFalse((self.config / f"autostart/{install_release.APP_ID}.desktop").exists())

    def test_upgrade_repeat_and_downgrade(self):
        self.installer(make_package(self.root, "1.0.0")).install()
        self.installer(make_package(self.root, "1.1.0")).install()
        self.assertEqual((self.app / "gnome-clip-notes").read_text(), "app 1.1.0\n")
        self.assertEqual(self.installer(make_package(self.root, "1.1.0")).install(),
                         "already installed")
        with self.assertRaisesRegex(install_release.InstallError, "downgrades"):
            self.installer(make_package(self.root, "1.0.1")).install()

    def test_upgrade_refuses_unrecorded_file_in_replaceable_tree_before_mutation(self):
        self.installer(make_package(self.root, "1.0.0")).install()
        extension = self.data / f"gnome-shell/extensions/{install_release.EXTENSION_UUID}"
        foreign = extension / "personal.txt"
        foreign.write_text("keep")
        self.commands.enabled = True

        with self.assertRaisesRegex(
                install_release.InstallError,
                rf"unrecorded file: {re.escape(str(foreign))}.*Move it aside"):
            self.installer(make_package(self.root, "1.1.0")).install()

        self.assertEqual(foreign.read_text(), "keep")
        self.assertEqual((self.app / "gnome-clip-notes").read_text(), "app 1.0.0\n")
        self.assertEqual(
            json.loads((self.app / install_release.RECEIPT).read_text())["version"],
            "1.0.0")
        self.assertEqual(self.commands.disabled, 0)

    def test_refuses_legacy_installation(self):
        legacy = self.data / f"gnome-shell/extensions/{install_release.LEGACY_UUID}"
        legacy.mkdir(parents=True)
        with self.assertRaisesRegex(install_release.InstallError, "legacy migration not supported"):
            self.installer(make_package(self.root)).install()
        self.assertFalse(self.app.exists())

    def test_refuses_unowned_modern_target(self):
        target = self.data / f"applications/{install_release.APP_ID}.desktop"
        target.parent.mkdir(parents=True)
        target.write_text("unowned")
        with self.assertRaisesRegex(install_release.InstallError, "not owned"):
            self.installer(make_package(self.root)).install()
        self.assertEqual(target.read_text(), "unowned")

    def test_refuses_open_editors_and_unknown_protocol(self):
        package = make_package(self.root)
        self.commands.owner = ":1.42"
        self.commands.status["native_editors"] = 1
        with self.assertRaisesRegex(install_release.InstallError, "close all"):
            self.installer(package).install()
        self.commands.status["native_editors"] = 0
        self.commands.status["protocol"] = 2
        with self.assertRaisesRegex(install_release.InstallError, "unknown update protocol"):
            self.installer(package).install()

    def test_injected_failure_rolls_back_and_restores_extension_state(self):
        self.installer(make_package(self.root, "1.0.0")).install()
        self.commands.enabled = True

        def fail(index):
            if index == 2:
                raise RuntimeError("injected")

        with self.assertRaisesRegex(RuntimeError, "injected"):
            self.installer(make_package(self.root, "1.1.0"), hook=fail).install()
        self.assertEqual((self.app / "gnome-clip-notes").read_text(), "app 1.0.0\n")
        self.assertEqual(json.loads((self.app / install_release.RECEIPT).read_text())["version"],
                         "1.0.0")
        self.assertEqual(self.commands.disabled, 1)
        self.assertEqual(self.commands.enabled_again, 1)

    def test_interruption_marker_blocks_then_restore_recovers(self):
        self.installer(make_package(self.root, "1.0.0")).install()

        def interrupt(index):
            if index == 0:
                raise KeyboardInterrupt()

        update = self.installer(make_package(self.root, "1.1.0"), hook=interrupt)
        with self.assertRaises(KeyboardInterrupt):
            update.install()
        self.assertTrue(update.marker.exists())
        with self.assertRaisesRegex(install_release.InstallError, "needs recovery"):
            self.installer(make_package(self.root, "1.1.0")).install()
        restored = self.installer(make_package(self.root, "1.1.0")).restore()
        self.assertIn("restored", restored)
        self.assertEqual((self.app / "gnome-clip-notes").read_text(), "app 1.0.0\n")
        self.assertFalse(update.marker.exists())
        self.assertTrue((update.snapshot_dir / "install-release.py").is_file())

    def test_failed_rollback_leaves_capture_disabled_and_reports_saved_helper(self):
        self.installer(make_package(self.root)).install()
        self.commands.enabled = True
        update = self.installer(make_package(self.root, '1.1.0'),
                                hook=lambda _: (_ for _ in ()).throw(RuntimeError('write failure')))
        with mock.patch.object(update, '_restore_snapshot', side_effect=OSError('restore failure')):
            with self.assertRaisesRegex(install_release.InstallError, 'automatic rollback failed') as error:
                update.install()
        self.assertTrue(update.marker.exists())
        self.assertIn(str(update.state_dir / 'recovery.py'), str(error.exception))
        self.assertEqual(self.commands.enabled_again, 0)

    def test_missing_runtime_does_not_stop_capture_or_install_files(self):
        with mock.patch.object(self.commands, 'check_runtime',
                               side_effect=install_release.InstallError('missing runtime library')):
            with self.assertRaisesRegex(install_release.InstallError, 'missing runtime'):
                self.installer(make_package(self.root)).install()
        self.assertEqual(self.commands.disabled, 0)
        self.assertFalse(self.app.exists())

    def test_recovery_rejects_symlink_parent_before_deleting_anything(self):
        installer = self.installer(make_package(self.root))
        installer.install()
        installer.marker.write_text('{}')
        outside = self.root / 'moved applications'
        (self.data / 'applications').rename(outside)
        (self.data / 'applications').symlink_to(outside, target_is_directory=True)
        with self.assertRaisesRegex(install_release.InstallError, 'symlinked directory'):
            installer.restore()
        self.assertEqual((self.app / 'gnome-clip-notes').read_text(), 'app 1.0.0\n')
        self.assertTrue((outside / f'{install_release.APP_ID}.desktop').exists())

    def test_rejects_package_symlink(self):
        package = make_package(self.root)
        (package / "extension/bad").symlink_to("extension.js")
        with self.assertRaisesRegex(install_release.InstallError, "link or special"):
            self.installer(package).install()

    def test_rejects_symlinked_destination_directory(self):
        outside = self.root / "outside"
        outside.mkdir()
        self.data.mkdir()
        (self.data / "applications").symlink_to(outside, target_is_directory=True)
        with self.assertRaisesRegex(install_release.InstallError, "symlinked directory"):
            self.installer(make_package(self.root)).install()
        self.assertEqual(list(outside.iterdir()), [])

    def test_rejects_unsafe_exec_path(self):
        with self.assertRaisesRegex(install_release.InstallError, "unsafe in desktop Exec"):
            self.installer(make_package(self.root), self.home / "Bad$Path").install()
        with self.assertRaisesRegex(install_release.InstallError, "unsafe in desktop Exec"):
            self.installer(make_package(self.root), self.home / "Bad%Path").install()

    def test_rejects_development_version(self):
        with self.assertRaisesRegex(install_release.InstallError, "invalid stable version"):
            self.installer(make_package(self.root, "dev0.2.0")).install()

    def test_rejects_nested_link_in_installed_owned_tree_before_mutation(self):
        self.installer(make_package(self.root, "1.0.0")).install()
        extension = self.data / f"gnome-shell/extensions/{install_release.EXTENSION_UUID}"
        (extension / "unsafe-link").symlink_to("extension.js")
        with self.assertRaisesRegex(install_release.InstallError, "installed owned tree"):
            self.installer(make_package(self.root, "1.1.0")).install()
        self.assertEqual((self.app / "gnome-clip-notes").read_text(), "app 1.0.0\n")

    def test_invalid_snapshot_is_checked_before_any_restore_deletion(self):
        installer = self.installer(make_package(self.root))
        installer.install()
        installer.marker.write_text('{}')
        manifest = installer.snapshot_dir / "manifest.json"
        value = json.loads(manifest.read_text())
        value["entries"].append({"path": "/unexpected", "saved": "999", "kind": "file"})
        manifest.write_text(json.dumps(value))
        with self.assertRaisesRegex(install_release.InstallError, "unexpected path"):
            installer.restore()
        self.assertEqual((self.app / "gnome-clip-notes").read_text(), "app 1.0.0\n")

    def test_fifo_lock_is_rejected_without_blocking(self):
        installer = self.installer(make_package(self.root))
        installer.state_dir.mkdir(parents=True)
        os.mkfifo(installer.install_lock_path)
        with self.assertRaisesRegex(install_release.InstallError, "exclusive update lock"):
            installer._lock(installer.install_lock_path)


    def test_uninstall_preserves_data_settings_and_unrecorded_tree_files(self):
        self.installer(make_package(self.root)).install()
        data = self.data / 'gnome-clip-notes/data.db'
        data.write_text('precious notes')
        settings = self.config / 'gnome-clip-notes/settings.json'
        settings.parent.mkdir(parents=True, exist_ok=True)
        settings.write_text('{"open_at_login": false}')
        unknown = self.data / f'gnome-shell/extensions/{install_release.EXTENSION_UUID}/personal.txt'
        unknown.write_text('mine')
        (self.app / 'mine.txt').write_text('keep')
        self.commands.enabled = True
        result = self.installer(self.root / 'no-package-needed').uninstall()
        self.assertIn('Removed recorded', result)
        self.assertEqual(data.read_text(), 'precious notes')
        self.assertEqual(settings.read_text(), '{"open_at_login": false}')
        self.assertEqual(unknown.read_text(), 'mine')
        self.assertEqual((self.app / 'mine.txt').read_text(), 'keep')
        self.assertFalse((self.app / 'gnome-clip-notes').exists())
        self.assertFalse((self.app / 'install-release.py').exists())
        self.assertFalse((self.app / install_release.RECEIPT).exists())
        self.assertEqual(self.commands.disabled, 1)
        self.assertEqual(self.commands.enabled_again, 0)

    def test_uninstall_refuses_missing_receipt_or_inventory(self):
        remover = self.installer(self.root / 'unused')
        with self.assertRaisesRegex(install_release.InstallError, 'no managed installation'):
            remover.uninstall()
        self.installer(make_package(self.root)).install()
        receipt_path = self.app / install_release.RECEIPT
        receipt = json.loads(receipt_path.read_text())
        receipt.pop('owned_files')
        receipt_path.write_text(json.dumps(receipt))
        with self.assertRaisesRegex(install_release.InstallError, 'lacks a file inventory'):
            remover.uninstall()
        self.assertTrue((self.app / 'gnome-clip-notes').exists())

    def test_uninstall_refuses_injected_outside_inventory_path(self):
        self.installer(make_package(self.root)).install()
        sentinel = self.root / 'not-ours'
        sentinel.write_text('keep')
        receipt_path = self.app / install_release.RECEIPT
        receipt = json.loads(receipt_path.read_text())
        receipt['owned_files'].append(str(sentinel))
        receipt_path.write_text(json.dumps(receipt))
        with self.assertRaisesRegex(install_release.InstallError, 'unexpected path'):
            self.installer(self.root).uninstall()
        self.assertEqual(sentinel.read_text(), 'keep')
        self.assertTrue((self.app / 'gnome-clip-notes').exists())

    def test_uninstall_blocks_open_editors_and_changed_owner(self):
        self.installer(make_package(self.root)).install()
        self.commands.owner = ':1.55'
        self.commands.status['full_editors'] = 1
        with self.assertRaisesRegex(install_release.InstallError, 'close all Native and Full'):
            self.installer(self.root).uninstall()
        self.commands.status['full_editors'] = 0
        remover = self.installer(self.root)
        with mock.patch.object(remover, '_confirm', side_effect=lambda *a, **kw: setattr(self.commands, 'owner', ':1.56')):
            with self.assertRaisesRegex(install_release.InstallError, 'changed after confirmation'):
                remover.uninstall()
        self.assertEqual(self.commands.disabled, 0)
        self.assertTrue((self.app / 'gnome-clip-notes').exists())

    def test_uninstall_failure_rolls_back(self):
        self.installer(make_package(self.root)).install()
        self.commands.enabled = True
        def fail(index):
            if index == 1:
                raise OSError('injected removal failure')
        remover = self.installer(self.root, hook=fail)
        with self.assertRaisesRegex(OSError, 'injected removal failure'):
            remover.uninstall()
        self.assertEqual((self.app / 'gnome-clip-notes').read_text(), 'app 1.0.0\n')
        self.assertTrue((self.app / install_release.RECEIPT).exists())
        self.assertFalse(remover.marker.exists())
        self.assertEqual(self.commands.enabled_again, 1)

    def test_interrupted_uninstall_recovers_without_original_package(self):
        self.installer(make_package(self.root)).install()
        def interrupt(index):
            if index == 1:
                raise KeyboardInterrupt()
        remover = self.installer(self.root / 'missing', hook=interrupt)
        with self.assertRaises(KeyboardInterrupt):
            remover.uninstall()
        self.assertTrue(remover.marker.exists())
        with self.assertRaisesRegex(install_release.InstallError, 'needs recovery'):
            self.installer(self.root).uninstall()
        self.installer(self.root / 'missing').restore()
        self.assertTrue((self.app / 'gnome-clip-notes').exists())
        self.assertTrue((self.app / 'install-release.py').exists())
        self.assertFalse(remover.marker.exists())

    def test_fresh_install_after_uninstall_reuses_preserved_data(self):
        package = make_package(self.root)
        self.installer(package).install()
        data = self.data / 'gnome-clip-notes/data.db'
        data.write_text('notes')
        self.installer(self.root).uninstall()
        self.installer(package).install()
        self.assertEqual(data.read_text(), 'notes')

    def test_uninstall_refuses_incomplete_receipt_before_removal(self):
        self.installer(make_package(self.root)).install()
        path = self.app / install_release.RECEIPT
        original = json.loads(path.read_text())
        for key in ('owned_paths', 'owned_files'):
            receipt = dict(original)
            receipt[key] = []
            path.write_text(json.dumps(receipt))
            with self.assertRaisesRegex(install_release.InstallError, 'missing mandatory'):
                self.installer(self.root).uninstall()
            self.assertTrue((self.app / 'gnome-clip-notes').exists())
            self.assertTrue(path.exists())

    def test_uninstall_recovery_preserves_file_added_after_interruption(self):
        self.installer(make_package(self.root)).install()
        def interrupt(index):
            if index == 1:
                raise KeyboardInterrupt()
        remover = self.installer(self.root, hook=interrupt)
        with self.assertRaises(KeyboardInterrupt):
            remover.uninstall()
        unknown = self.data / f'gnome-shell/extensions/{install_release.EXTENSION_UUID}/new-file'
        unknown.parent.mkdir(parents=True, exist_ok=True)
        unknown.write_text('created after interruption')
        remover.restore()
        self.assertEqual(unknown.read_text(), 'created after interruption')
        self.assertTrue((self.app / 'gnome-clip-notes').exists())

    def test_interruption_while_disabling_capture_has_explicit_recovery(self):
        self.installer(make_package(self.root)).install()
        self.commands.enabled = True
        remover = self.installer(self.root)
        with mock.patch.object(self.commands, 'disable_extension', side_effect=KeyboardInterrupt):
            with self.assertRaises(KeyboardInterrupt):
                remover.uninstall()
        self.assertEqual(json.loads(remover.marker.read_text())['phase'], 'stopping')
        self.assertTrue((remover.state_dir / 'recovery.py').is_file())
        result = remover.restore()
        self.assertIn('before program files changed', result)
        self.assertIn('enable GnomeClipNotes', result)
        self.assertTrue((self.app / 'gnome-clip-notes').exists())
        self.assertTrue((self.app / install_release.RECEIPT).exists())


    def test_dependency_consent_authentication_and_install_order(self):
        installer = self.installer(make_package(self.root))
        installer.install_deps = True
        events = []
        with mock.patch.object(self.commands, 'dependency_plan', return_value=('fedora', ['webkitgtk6.0'])), \
             mock.patch.object(installer, '_confirm', side_effect=lambda *a: events.append('confirm-app')), \
             mock.patch.object(self.commands, 'authorize_dependencies', side_effect=lambda **kw: events.append('auth')), \
             mock.patch.object(self.commands, 'install_dependencies', side_effect=lambda p: events.append('system-install')), \
             mock.patch.object(installer, '_transact', side_effect=lambda *a: events.append('app-install')):
            installer.install()
        self.assertEqual(events, ['confirm-app', 'auth', 'system-install', 'app-install'])

    def test_yes_alone_does_not_authorize_system_changes(self):
        installer = self.installer(make_package(self.root))
        with mock.patch.object(self.commands, 'dependency_plan', return_value=('fedora', ['webkitgtk6.0'])):
            with self.assertRaisesRegex(install_release.InstallError, 'consent is separate'):
                installer.install()
        self.assertFalse(self.app.exists())

    def test_dependency_refusal_and_auth_failure_leave_application_untouched(self):
        installer = self.installer(make_package(self.root))
        with mock.patch.object(self.commands, 'dependency_plan', return_value=('fedora', ['webkitgtk6.0'])):
            installer.yes = False
            with mock.patch('builtins.open', mock.mock_open(read_data='no\n')):
                with self.assertRaisesRegex(install_release.InstallError, 'declined'):
                    installer._confirm_dependencies(('fedora', ['webkitgtk6.0']))
            installer.yes = True
            installer.install_deps = True
            with mock.patch.object(self.commands, 'authorize_dependencies', side_effect=OSError('sudo refused')):
                with self.assertRaisesRegex(OSError, 'sudo refused'):
                    installer.install()
        self.assertFalse(self.app.exists())

    def test_package_failure_never_stops_capture_or_replaces_app(self):
        installer = self.installer(make_package(self.root))
        installer.install_deps = True
        with mock.patch.object(self.commands, 'dependency_plan', return_value=('ubuntu', ['libwebkitgtk-6.0-4'])), \
             mock.patch.object(self.commands, 'authorize_dependencies'), \
             mock.patch.object(self.commands, 'install_dependencies', side_effect=install_release.InstallError('package failure')):
            with self.assertRaisesRegex(install_release.InstallError, 'package failure'):
                installer.install()
        self.assertEqual(self.commands.disabled, 0)
        self.assertFalse(self.app.exists())

    def test_final_decline_precedes_auth_and_system_install(self):
        installer = self.installer(make_package(self.root))
        installer.install_deps = True
        with mock.patch.object(self.commands, 'dependency_plan', return_value=('fedora', ['webkitgtk6.0'])), \
             mock.patch.object(installer, '_confirm', side_effect=install_release.InstallError('cancelled')):
            with self.assertRaisesRegex(install_release.InstallError, 'cancelled'):
                installer.install()
        self.assertFalse(self.app.exists())

    def test_runtime_recheck_failure_after_packages_does_not_stop_capture(self):
        installer = self.installer(make_package(self.root))
        installer.install_deps = True
        with mock.patch.object(self.commands, 'dependency_plan', return_value=('fedora', ['webkitgtk6.0'])), \
             mock.patch.object(self.commands, 'authorize_dependencies'), \
             mock.patch.object(self.commands, 'install_dependencies'), \
             mock.patch.object(self.commands, 'check_runtime', side_effect=install_release.InstallError('still missing')):
            with self.assertRaisesRegex(install_release.InstallError, 'still missing'):
                installer.install()
        self.assertEqual(self.commands.disabled, 0)
        self.assertFalse(self.app.exists())


class RuntimeDiagnosticsTest(unittest.TestCase):
    def test_ubuntu_partial_package_status_is_missing(self):
        commands = install_release.Commands()
        def query(argv, **kw):
            status = 'install ok unpacked' if argv[-1] == 'libwebkitgtk-6.0-4' else 'install ok installed'
            return subprocess.CompletedProcess(argv, 0, status, '')
        with mock.patch.object(commands, 'check_dependencies', side_effect=install_release.InstallError('missing')), \
             mock.patch.object(commands, 'run', side_effect=query):
            self.assertEqual(commands.dependency_plan(Path('/unused'), {'id': 'ubuntu', 'version_id': '26.04'}),
                             ('ubuntu', ['libwebkitgtk-6.0-4']))

    def test_unsupported_system_and_unknown_packages_are_not_installed(self):
        commands = install_release.Commands()
        with mock.patch.object(commands, 'check_dependencies', side_effect=install_release.InstallError('missing')):
            with self.assertRaisesRegex(install_release.InstallError, 'No automatic package plan'):
                commands.dependency_plan(Path('/unused'), {'id': 'other', 'version_id': '1'})
        with self.assertRaisesRegex(install_release.InstallError, 'Invalid system package plan'):
            install_release.dependency_commands('fedora', ['--nogpgcheck'])

    def test_noninteractive_auth_never_prompts(self):
        with mock.patch.object(install_release.subprocess, 'run', side_effect=subprocess.CalledProcessError(1, 'sudo')) as run:
            with self.assertRaisesRegex(install_release.InstallError, 'Could not authorize'):
                install_release.Commands().authorize_dependencies(noninteractive=True)
        self.assertEqual(run.call_args.args[0], ['/usr/bin/sudo', '-n', '-v'])

    def test_system_package_failure_reports_possible_retained_changes(self):
        with mock.patch.object(install_release.subprocess, 'run', side_effect=subprocess.CalledProcessError(1, 'apt')):
            with self.assertRaisesRegex(install_release.InstallError, 'System package changes may remain'):
                install_release.Commands().install_dependencies(('ubuntu', ['libwebkitgtk-6.0-4']))
    def test_package_plan_uses_installed_database_not_arbitrary_library_names(self):
        commands = install_release.Commands()
        def query(argv, **kw):
            return subprocess.CompletedProcess(argv, 1 if argv[-1] == 'webkitgtk6.0' else 0, '', '')
        with mock.patch.object(commands, 'check_dependencies', side_effect=install_release.InstallError('missing tools')), \
             mock.patch.object(commands, 'run', side_effect=query):
            self.assertEqual(commands.dependency_plan(Path('/unused'), {'id': 'fedora', 'version_id': '44'}),
                             ('fedora', ['webkitgtk6.0']))

    def test_installed_but_broken_runtime_is_not_a_system_upgrade(self):
        commands = install_release.Commands()
        with mock.patch.object(commands, 'check_dependencies', side_effect=install_release.InstallError('ABI failure')), \
             mock.patch.object(commands, 'run', return_value=subprocess.CompletedProcess([], 0, '', '')):
            with self.assertRaisesRegex(install_release.InstallError, 'no broad upgrade'):
                commands.dependency_plan(Path('/unused'), {'id': 'fedora', 'version_id': '44'})

    def test_package_execution_is_noninteractive_and_does_not_add_repositories(self):
        with mock.patch.object(install_release.subprocess, 'run') as run:
            install_release.Commands().install_dependencies(('ubuntu', ['libwebkitgtk-6.0-4']))
        self.assertEqual(run.call_count, 2)
        for call in run.call_args_list:
            self.assertEqual(call.args[0][:2], ['/usr/bin/sudo', '-n'])
            self.assertEqual(call.kwargs['stdin'], subprocess.DEVNULL)
        argv = run.call_args.args[0]
        self.assertIn('--no-remove', argv)
        self.assertNotIn('--allow-unauthenticated', argv)

    def test_runtime_reports_missing_libraries_for_both_binaries(self):
        commands = install_release.Commands()
        results = [subprocess.CompletedProcess([], 0, 'libadwaita-1.so.0 => not found\n', ''),
                   subprocess.CompletedProcess([], 0, 'libwebkitgtk-6.0.so.4 => not found\n', '')]
        with mock.patch.object(commands, 'run', side_effect=results) as run:
            with self.assertRaises(install_release.InstallError) as error:
                commands.check_runtime(Path('/unused'))
        message = str(error.exception)
        self.assertIn('libadwaita-1.so.0', message)
        self.assertIn('libwebkitgtk-6.0.so.4', message)
        self.assertIn('No program files were changed', message)
        self.assertEqual(run.call_count, 2)

    def test_runtime_rejects_abi_failure_and_unreadable_binary(self):
        commands = install_release.Commands()
        results = [subprocess.CompletedProcess([], 1, '', "version `GLIBC_99' not found"),
                   subprocess.CompletedProcess([], 1, '', 'not a dynamic executable')]
        with mock.patch.object(commands, 'run', side_effect=results):
            with self.assertRaises(install_release.InstallError) as error:
                commands.check_runtime(Path('/unused'))
        self.assertIn('GLIBC_99', str(error.exception))
        self.assertIn('not a dynamic executable', str(error.exception))

    def test_command_locale_is_child_only(self):
        with mock.patch.dict(os.environ, {'LC_ALL': 'ru_RU.UTF-8', 'LANGUAGE': 'ru'}):
            with mock.patch.object(install_release.subprocess, 'run') as run:
                install_release.Commands().run(['ldd', '/unused'])
            self.assertEqual(run.call_args.kwargs['env']['LC_ALL'], 'C')
            self.assertEqual(run.call_args.kwargs['env']['LANGUAGE'], 'C')
            self.assertEqual(os.environ['LC_ALL'], 'ru_RU.UTF-8')
            self.assertEqual(os.environ['LANGUAGE'], 'ru')

    def test_missing_tools_explains_retry_and_uninstall_does_not_need_ldd(self):
        with mock.patch.object(install_release.shutil, 'which',
                               side_effect=lambda name: None if name == 'ldd' else '/mock/' + name):
            commands = install_release.Commands()
            with self.assertRaisesRegex(install_release.InstallError, 'Missing required commands: ldd'):
                commands.check_dependencies()
            commands.check_uninstall_dependencies()

    def test_missing_systemd_run_is_reported_before_install_not_uninstall(self):
        with mock.patch.object(install_release.shutil, 'which',
                               side_effect=lambda name: None if name == 'systemd-run' else '/mock/' + name):
            commands = install_release.Commands()
            with self.assertRaisesRegex(install_release.InstallError, 'Missing required commands: systemd-run'):
                commands.check_dependencies()
            commands.check_uninstall_dependencies()


if __name__ == "__main__":
    unittest.main()

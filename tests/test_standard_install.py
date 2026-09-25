"""Standard lifecycle in temporary paths; no real network, desktop or user data."""
import hashlib
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


@unittest.skipIf(os.geteuid() == 0, 'Standard deliberately refuses root; run this test as a regular user')
class StandardTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='gcn-standard-test-')
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.bin = self.base / 'bin'
        self.bin.mkdir()
        self.app = self.base / 'Applications with spaces' / 'GnomeClipNotes'
        self.data = self.base / 'data'
        self.config = self.base / 'config'
        self.env = dict(os.environ, PATH=str(self.bin), GNOME_CLIP_NOTES_APP_DIR=str(self.app),
                        XDG_DATA_HOME=str(self.data), XDG_CONFIG_HOME=str(self.config),
                        STANDARD_FIXTURE=str(self.base))
        # Deliberately no gh, gdbus, pkill or actual desktop-cache tools in PATH.
        for name in ('curl', 'tar', 'gzip', 'sha256sum', 'awk', 'sed', 'install', 'mktemp',
                     'uname', 'glib-compile-schemas', 'rm', 'cp', 'chmod', 'rmdir'):
            if name != 'curl':
                (self.bin / name).symlink_to(shutil.which(name))
        for name in ('gnome-extensions', 'update-desktop-database', 'gtk-update-icon-cache'):
            self.executable(name, '#!/bin/sh\nprintf "%s\\n" "$0 $*" >> "$STANDARD_FIXTURE/desktop.log"\n')
        arch = platform.machine()
        self.package = f'gnome-clip-notes-1.0.0-{arch}'
        stage = self.base / self.package
        (stage / 'target/release').mkdir(parents=True)
        for binary in ('gnome-clip-notes', 'gnome-clip-notes-editor'):
            (stage / 'target/release' / binary).write_text('#!/bin/sh\nexit 0\n')
        (stage / 'target/locales').mkdir()
        shutil.copytree(ROOT / 'data', stage / 'data')
        shutil.copytree(ROOT / 'extension', stage / 'extension')
        self.archive = self.base / f'{self.package}.tar.gz'
        with tarfile.open(self.archive, 'w:gz') as tar:
            tar.add(stage, arcname=self.package)
        self.hash = hashlib.sha256(self.archive.read_bytes()).hexdigest()
        (self.base / 'SHA256SUMS').write_text(f'{self.hash}  {self.archive.name}\n')
        self.executable('curl', f'''#!{sys.executable}
import os, pathlib, shutil, sys
args=sys.argv[1:]; root=pathlib.Path(os.environ['STANDARD_FIXTURE'])
if '--head' in args:
    print('https://github.com/OleksiyM/GnomeClipNotes/releases/tag/v1.0.0', end='')
else:
    dest=pathlib.Path(args[args.index('--output')+1])
    if dest.name.endswith('.sigstore.json'): dest.write_text('{{}}')
    else: shutil.copyfile(root/dest.name, dest)
''')
        for directory in (self.data, self.config):
            (directory / 'gnome-clip-notes').mkdir(parents=True)
            (directory / 'gnome-clip-notes/keep').write_text('private fixture')

    def executable(self, name, content):
        path = self.bin / name
        path.write_text(content)
        path.chmod(0o755)

    def run_script(self, name, *args):
        # Exactly the piped-script input path, with arguments passed after -s --.
        return subprocess.run(['/bin/bash', '-s', '--', *args],
                              input=(ROOT / name).read_text(), text=True,
                              capture_output=True, env=self.env, timeout=30)

    def test_install_repeat_uninstall_keeps_data_and_refreshes_caches(self):
        for _ in range(2):
            result = self.run_script('install.sh')
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn('Provenance skipped', result.stdout)
            self.assertIn('Installed successfully', result.stdout)
        self.assertTrue((self.app / 'gnome-clip-notes-editor').is_file())
        self.assertIn('--library', (self.data / 'applications/io.github.OleksiyM.GnomeClipNotes.desktop').read_text())
        (self.app / 'unrelated').write_text('keep')
        result = self.run_script('uninstall.sh')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse((self.app / 'gnome-clip-notes').exists())
        self.assertTrue((self.app / 'unrelated').exists())
        self.assertTrue((self.data / 'gnome-clip-notes/keep').exists())
        self.assertTrue((self.config / 'gnome-clip-notes/keep').exists())
        self.assertIn('gtk-update-icon-cache', (self.base / 'desktop.log').read_text())

    def test_old_gh_optional_or_required(self):
        self.executable('gh', '#!/bin/sh\necho "old gh"\nexit 1\n')
        result = self.run_script('install.sh', '--require-provenance')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.app.exists())
        self.assertEqual(self.run_script('install.sh').returncode, 0)

    def test_provenance_success_and_failure(self):
        flags = '--bundle --repo --signer-workflow --source-ref --cert-oidc-issuer --deny-self-hosted-runners --predicate-type'
        for status in (1, 0):
            self.executable('gh', f'#!/bin/sh\nif [ "$3" = --help ]; then echo "{flags}"; exit 0; fi\nexit {status}\n')
            result = self.run_script('install.sh', '--require-provenance')
            self.assertEqual(result.returncode, status, result.stderr)
            self.assertEqual(self.app.exists(), status == 0)

    def test_bad_checksum_leaves_install_untouched(self):
        (self.base / 'SHA256SUMS').write_text(f'{"0" * 64}  {self.archive.name}\n')
        result = self.run_script('install.sh')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.app.exists())

    def test_guided_guard_and_explicit_purge(self):
        self.app.mkdir(parents=True)
        receipt = self.app / '.install-receipt.json'
        receipt.write_text('{}')
        for name in ('install.sh', 'uninstall.sh'):
            self.assertNotEqual(self.run_script(name).returncode, 0)
        receipt.unlink()
        self.assertEqual(self.run_script('uninstall.sh', '--purge').returncode, 0)
        self.assertFalse((self.data / 'gnome-clip-notes').exists())
        self.assertFalse((self.config / 'gnome-clip-notes').exists())


if __name__ == '__main__':
    unittest.main()

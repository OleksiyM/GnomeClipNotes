#!/usr/bin/env python3
import io
import hashlib
import json
import os
import platform
from pathlib import Path
import stat
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
BOOTSTRAP = ROOT / "guided-install.sh"
VERSION = "1.0.0"
TAG = "v" + VERSION
OS_RELEASE = platform.freedesktop_os_release()
PLATFORM = f"{OS_RELEASE['ID']}-{OS_RELEASE['VERSION_ID']}-{platform.machine()}"
PACKAGE = f"gnome-clip-notes-{VERSION}-{platform.machine()}"
ARCHIVE = PACKAGE + ".tar.gz"


def manifest():
    return {
        "format": 1,
        "version": VERSION,
        "platform": {"id": OS_RELEASE['ID'], "version_id": OS_RELEASE['VERSION_ID'], "arch": platform.machine()},
        "app_id": "io.github.OleksiyM.GnomeClipNotes",
        "extension_uuid": "gnome-clip-notes@oleksiym.github.io",
        "update_protocol": 1,
    }


def regular(name, contents=b"fixture", mode=0o644):
    info = tarfile.TarInfo(name)
    info.size = len(contents)
    info.mode = mode
    return info, contents


def write_archive(path, entries=None, release=None):
    if entries is None:
        helper = b"""#!/usr/bin/env python3
import json, os, pathlib, sys
pathlib.Path(os.environ['BOOTSTRAP_MARKER']).write_text(json.dumps(sys.argv[1:]))
"""
        entries = [
            regular(PACKAGE + "/release.json", json.dumps(release or manifest()).encode()),
            regular(PACKAGE + "/scripts/install-release.py", helper, 0o755),
        ]
    with tarfile.open(path, "w:gz") as stream:
        for info, contents in entries:
            stream.addfile(info, io.BytesIO(contents) if info.isfile() else None)


class BootstrapTests(unittest.TestCase):
    def setUp(self):
        if PLATFORM != 'fedora-44-x86_64':
            self.skipTest("bootstrap integration fixture requires Fedora 44 x86_64")
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.bin = self.root / "mock bin"
        self.bin.mkdir()
        self.archive = self.root / ARCHIVE
        self.bundle = self.root / (ARCHIVE + ".sigstore.json")
        self.bundle.write_text("{}")
        self.marker = self.root / "helper marker.json"
        self.gh_log = self.root / "gh args.jsonl"
        self.curl_log = self.root / "curl args.jsonl"
        (self.bin / 'mktemp').symlink_to('/usr/bin/mktemp')
        self._write_executable("python3", f"#!/bin/sh\nexec {sys.executable!s} \"$@\"\n")
        self._write_executable("curl", """#!/usr/bin/env python3
import hashlib, json, os, pathlib, shutil, sys
args = sys.argv[1:]
with pathlib.Path(os.environ['CURL_LOG']).open('a') as log:
    log.write(json.dumps(args) + '\\n')
if os.environ.get('CURL_FAIL') == '1':
    raise SystemExit(22)
output = args[args.index('--output') + 1]
if output.endswith('/SHA256SUMS'):
    archive = pathlib.Path(os.environ['FIXTURE_ARCHIVE'])
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    if os.environ.get('BAD_HASH') == '1':
        digest = '0' * 64
    line = digest + '  ' + archive.name + '\\n'
    if os.environ.get('DUPLICATE_HASH') == '1':
        line += line
    if os.environ.get('MISSING_HASH') == '1':
        line = digest + '  other.tar.gz\\n'
    pathlib.Path(output).write_text(line)
    raise SystemExit(0)
source = os.environ['FIXTURE_BUNDLE'] if output.endswith('.sigstore.json') else os.environ['FIXTURE_ARCHIVE']
shutil.copyfile(source, output)
""")
        self._write_executable("gh", """#!/usr/bin/env python3
import json, os, pathlib, sys
args = sys.argv[1:]
if args == ['attestation', 'verify', '--help']:
    print('--bundle --repo --signer-workflow --source-ref --deny-self-hosted-runners --cert-oidc-issuer --predicate-type')
    raise SystemExit(0)
with pathlib.Path(os.environ['GH_LOG']).open('a') as log:
    log.write(json.dumps(args) + '\\n')
if os.environ.get('GH_FAIL') == '1':
    raise SystemExit(1)
""")

    def tearDown(self):
        self.temp.cleanup()

    def _write_executable(self, name, contents):
        path = self.bin / name
        path.write_text(contents)
        path.chmod(path.stat().st_mode | stat.S_IXUSR)

    def run_bootstrap(self, *extra, env=None):
        values = {
            "PATH": str(self.bin),
            "FIXTURE_ARCHIVE": str(self.archive),
            "FIXTURE_BUNDLE": str(self.bundle),
            "BOOTSTRAP_MARKER": str(self.marker),
            "GH_LOG": str(self.gh_log),
            "CURL_LOG": str(self.curl_log),
        }
        if env:
            values.update(env)
        return subprocess.run(
            ["/bin/bash", str(BOOTSTRAP), "--version", TAG, "--yes", *extra],
            text=True, capture_output=True, env=values, check=False)

    def gh_calls(self):
        if not self.gh_log.exists():
            return []
        return [json.loads(line) for line in self.gh_log.read_text().splitlines()]

    def test_happy_path_preserves_arguments_and_enforces_attestation_policy(self):
        write_archive(self.archive)
        app_dir = self.root / "Applications with spaces" / "Clip Notes"
        result = self.run_bootstrap("--app-dir", str(app_dir))
        self.assertEqual(result.returncode, 0, result.stderr)
        helper_args = json.loads(self.marker.read_text())
        self.assertEqual(helper_args[0], "--package-dir")
        self.assertTrue(helper_args[1].endswith("/" + PACKAGE))
        self.assertEqual(helper_args[2:], ["--yes", "--app-dir", str(app_dir)])
        verify, = self.gh_calls()
        self.assertEqual(verify[:3], ["attestation", "verify", verify[2]])
        expected = {
            "--bundle": None,
            "--repo": "OleksiyM/GnomeClipNotes",
            "--signer-workflow": "OleksiyM/GnomeClipNotes/.github/workflows/release.yml",
            "--source-ref": "refs/tags/v1.0.0",
            "--cert-oidc-issuer": "https://token.actions.githubusercontent.com",
            "--predicate-type": "https://slsa.dev/provenance/v1",
        }
        for flag, value in expected.items():
            self.assertIn(flag, verify)
            if value is not None:
                self.assertEqual(verify[verify.index(flag) + 1], value)
        self.assertIn("--deny-self-hosted-runners", verify)
        self.assertTrue(verify[2].endswith("/" + ARCHIVE))
        self.assertTrue(verify[verify.index("--bundle") + 1].endswith("/" + ARCHIVE + ".sigstore.json"))

    def test_signature_failure_never_extracts_or_runs_helper(self):
        write_archive(self.archive)
        result = self.run_bootstrap(env={"GH_FAIL": "1"})
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.marker.exists())
        self.assertEqual(len(self.gh_calls()), 1)

    def test_explicit_package_consent_is_forwarded_to_verified_helper(self):
        write_archive(self.archive)
        result = self.run_bootstrap('--install-deps')
        self.assertEqual(result.returncode, 0, result.stderr)
        args = json.loads(self.marker.read_text())
        self.assertIn('--install-deps', args)
        self.assertIn('--yes', args)

    def test_download_failure_never_verifies_or_runs_helper(self):
        write_archive(self.archive)
        result = self.run_bootstrap(env={"CURL_FAIL": "1"})
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.marker.exists())
        self.assertEqual(self.gh_calls(), [])

    def test_no_gh_checks_integrity_and_explains_skipped_provenance(self):
        (self.bin / 'gh').unlink()
        write_archive(self.archive)
        result = self.run_bootstrap()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(self.marker.exists())
        self.assertIn('SHA256 integrity verified', result.stdout)
        self.assertIn('provenance verification is skipped', result.stdout)
        self.assertEqual(self.gh_calls(), [])
        self.assertNotIn('.sigstore.json', self.curl_log.read_text())

    def test_required_provenance_without_gh_stops_before_download(self):
        (self.bin / 'gh').unlink()
        result = self.run_bootstrap('--require-provenance')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('gh is missing', result.stderr)
        self.assertFalse(self.curl_log.exists())
        self.assertFalse(self.marker.exists())

    def test_fedora_aarch64_uses_matching_release(self):
        (self.root / 'sitecustomize.py').write_text(
            'import platform\nplatform.machine = lambda: "aarch64"\n', encoding='utf-8')
        arm_package = f'gnome-clip-notes-{VERSION}-aarch64'
        self.archive = self.root / (arm_package + '.tar.gz')
        self.bundle = self.root / (self.archive.name + '.sigstore.json')
        self.bundle.write_text('{}')
        release = manifest()
        release['platform']['arch'] = 'aarch64'
        with mock.patch.dict(globals(), PACKAGE=arm_package):
            write_archive(self.archive, release=release)
        result = self.run_bootstrap(env={'PYTHONPATH': str(self.root)})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(self.marker.exists())

    def test_ubuntu_2604_uses_fedora_release_metadata(self):
        (self.root / 'sitecustomize.py').write_text(
            'from pathlib import Path\n'
            '_read_text = Path.read_text\n'
            'def read_text(self, *args, **kwargs):\n'
            '    if str(self) == "/etc/os-release":\n'
            '        return "ID=ubuntu\\nVERSION_ID=26.04\\n"\n'
            '    return _read_text(self, *args, **kwargs)\n'
            'Path.read_text = read_text\n', encoding='utf-8')
        release = manifest()
        release['platform'] = {'id': 'fedora', 'version_id': '44', 'arch': 'x86_64'}
        write_archive(self.archive, release=release)
        result = self.run_bootstrap(env={'PYTHONPATH': str(self.root)})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('fedora-44-x86_64', result.stdout)
        self.assertTrue(self.marker.exists())

    def test_ubuntu_2404_stops_before_download(self):
        (self.root / 'sitecustomize.py').write_text(
            'from pathlib import Path\n'
            '_read_text = Path.read_text\n'
            'def read_text(self, *args, **kwargs):\n'
            '    if str(self) == "/etc/os-release":\n'
            '        return "ID=ubuntu\\nVERSION_ID=24.04\\n"\n'
            '    return _read_text(self, *args, **kwargs)\n'
            'Path.read_text = read_text\n', encoding='utf-8')
        result = self.run_bootstrap(env={'PYTHONPATH': str(self.root)})
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('No release package configured for this platform: ubuntu-24.04-x86_64', result.stderr)
        self.assertFalse(self.curl_log.exists())

    def test_checksum_errors_never_verify_or_run_helper(self):
        write_archive(self.archive)
        for variable in ('BAD_HASH', 'DUPLICATE_HASH', 'MISSING_HASH'):
            with self.subTest(variable=variable):
                result = self.run_bootstrap(env={variable: '1'})
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(self.marker.exists())
                self.assertEqual(self.gh_calls(), [])

    def test_old_gh_does_not_silently_downgrade_to_integrity(self):
        self._write_executable('gh', '#!/bin/sh\necho "old gh"\n')
        result = self.run_bootstrap()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('lacks --bundle', result.stderr)
        self.assertFalse(self.curl_log.exists())

    def test_rejects_unsafe_paths_links_duplicates_and_oversized_metadata(self):
        valid_helper = regular(PACKAGE + "/scripts/install-release.py", b"raise SystemExit(99)\n")
        cases = {}
        cases["parent path"] = [regular(PACKAGE + "/../escape", b"bad")]
        cases["absolute path"] = [regular('/tmp/gcn-escape', b'bad')]
        link = tarfile.TarInfo(PACKAGE + "/link")
        link.type = tarfile.SYMTYPE
        link.linkname = "/tmp/escape"
        cases["symbolic link"] = [(link, b"")]
        hardlink = tarfile.TarInfo(PACKAGE + '/hardlink')
        hardlink.type = tarfile.LNKTYPE
        hardlink.linkname = PACKAGE + '/release.json'
        cases['hard link'] = [(hardlink, b'')]
        duplicate = regular(PACKAGE + "/duplicate", b"one")
        cases["duplicate"] = [duplicate, regular(PACKAGE + "/duplicate", b"two")]
        large_manifest = dict(manifest())
        large_manifest["padding"] = "x" * 1048576
        cases["oversized metadata"] = [
            regular(PACKAGE + "/release.json", json.dumps(large_manifest).encode()),
            valid_helper,
        ]
        for name, entries in cases.items():
            with self.subTest(name=name):
                if name != "oversized metadata":
                    entries = [regular(PACKAGE + "/release.json", json.dumps(manifest()).encode()),
                               valid_helper, *entries]
                write_archive(self.archive, entries)
                self.marker.unlink(missing_ok=True)
                result = self.run_bootstrap()
                self.assertNotEqual(result.returncode, 0, name)
                self.assertFalse(self.marker.exists(), name)

    def test_rejects_wrong_release_manifest(self):
        wrong = manifest()
        wrong["platform"]["version_id"] = "43"
        write_archive(self.archive, release=wrong)
        result = self.run_bootstrap()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Release metadata does not match", result.stderr)
        self.assertFalse(self.marker.exists())


if __name__ == "__main__":
    unittest.main()

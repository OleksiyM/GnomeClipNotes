#!/usr/bin/env python3
import json
import os
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest import mock

import importlib.util

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("package_release", ROOT / "scripts/package-release.py")
PACKAGE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PACKAGE)


class PackageReleaseTests(unittest.TestCase):
    def setUp(self):
        # Host fixtures must not inherit release-job platform assertions.
        environment = mock.patch.dict(os.environ)
        environment.start()
        self.addCleanup(environment.stop)
        for key in ("GCN_PACKAGE_OS_ID", "GCN_PACKAGE_OS_VERSION", "GCN_PACKAGE_ARCH"):
            os.environ.pop(key, None)

    def fixture(self):
        temp = tempfile.TemporaryDirectory()
        root = Path(temp.name)
        (root / "Cargo.toml").write_text('[package]\nname = "gnome-clip-notes"\nversion = "1.0.0"\n', encoding="utf-8")
        for path in ("LICENSE", "README.md", "install.sh", "uninstall.sh", "guided-install.sh", "docs/installation.md", "docs/privacy.md", "docs/preview.md", "docs/translations.md", "docs/artifact-verification.md", "scripts/install-release.py", "data/io.github.OleksiyM.GnomeClipNotes.desktop.in", "data/io.github.OleksiyM.GnomeClipNotes-autostart.desktop.in", "data/io.github.OleksiyM.GnomeClipNotes.service.in", "data/io.github.OleksiyM.GnomeClipNotes.svg", "extension/metadata.json", "extension/stylesheet.css", "extension/extension.js", "extension/clipboardFormats.js", "extension/schemas/org.gnome.shell.extensions.gnome-clip-notes.gschema.xml"):
            target = root / path
            target.parent.mkdir(parents=True, exist_ok=True)
            if path.endswith("metadata.json"):
                content = '{"uuid":"gnome-clip-notes@oleksiym.github.io"}\n'
            elif path.endswith(".gschema.xml"):
                content = '<schemalist><schema id="org.example.fixture" path="/org/example/fixture/"><key name="enabled" type="b"><default>true</default></key></schema></schemalist>\n'
            else:
                content = "fixture\n"
            target.write_text(content, encoding="utf-8")
        for path in ("target/release/gnome-clip-notes", "target/release/gnome-clip-notes-editor"):
            target = root / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(b"binary")
        (root / "target/locales/en").mkdir(parents=True)
        (root / "target/locales/en/LC_MESSAGES.mo").write_bytes(b"locale")
        return temp, root

    @mock.patch.object(PACKAGE, "os_release", return_value={"ID": "fedora", "VERSION_ID": "44"})
    @mock.patch.object(PACKAGE.platform, "machine", return_value="x86_64")
    def test_manifest_inventory_and_deterministic_names(self, _machine, _release):
        temp, root = self.fixture()
        with temp, mock.patch.dict(os.environ, {}, clear=False):
            output = root / "dist"
            archive = PACKAGE.package(root, output)
            self.assertEqual(archive.name, "gnome-clip-notes-1.0.0-x86_64.tar.gz")
            self.assertEqual(sorted(path.name for path in output.iterdir()), [archive.name])
            with tarfile.open(archive, "r:gz") as tar:
                names = tar.getnames()
                release = json.loads(tar.extractfile("gnome-clip-notes-1.0.0-x86_64/release.json").read())
                for script in ("install.sh", "uninstall.sh", "guided-install.sh"):
                    self.assertEqual(tar.getmember(f"gnome-clip-notes-1.0.0-x86_64/{script}").mode, 0o755)
            self.assertNotIn("docs/private", " ".join(names))
            self.assertIn("extension/schemas/gschemas.compiled", release["files"])
            self.assertIn("install.sh", release["files"])
            self.assertIn("uninstall.sh", release["files"])
            self.assertEqual(release["extension_uuid"], "gnome-clip-notes@oleksiym.github.io")
            self.assertEqual(release["platform"], {"id": "fedora", "version_id": "44", "arch": "x86_64"})
            self.assertIn("gnome-clip-notes-1.0.0-x86_64/extension/schemas/gschemas.compiled", names)

    @mock.patch.object(PACKAGE, "os_release", return_value={"ID": "fedora", "VERSION_ID": "42"})
    @mock.patch.object(PACKAGE.platform, "machine", return_value="aarch64")
    def test_override_must_match_host(self, _machine, _release):
        with mock.patch.dict(os.environ, {"GCN_PACKAGE_ARCH": "x86_64"}, clear=False):
            with self.assertRaisesRegex(ValueError, "does not match"):
                PACKAGE.platform_labels()

    @mock.patch.object(PACKAGE, "os_release", return_value={"ID": "fedora", "VERSION_ID": "44"})
    @mock.patch.object(PACKAGE.platform, "machine", return_value="aarch64")
    def test_arm_archive_records_native_architecture(self, _machine, _release):
        temp, root = self.fixture()
        with temp, mock.patch.dict(os.environ, {"GCN_PACKAGE_ARCH": "aarch64"}, clear=False):
            archive = PACKAGE.package(root, root / "dist")
            self.assertEqual(archive.name, "gnome-clip-notes-1.0.0-aarch64.tar.gz")
            with tarfile.open(archive, "r:gz") as tar:
                release = json.loads(tar.extractfile("gnome-clip-notes-1.0.0-aarch64/release.json").read())
            self.assertEqual(release["platform"], {"id": "fedora", "version_id": "44", "arch": "aarch64"})

    @mock.patch.object(PACKAGE, "os_release", return_value={"ID": "ubuntu", "VERSION_ID": "24.04"})
    @mock.patch.object(PACKAGE.platform, "machine", return_value="x86_64")
    def test_local_archive_records_other_build_host(self, _machine, _release):
        temp, root = self.fixture()
        with temp:
            archive = PACKAGE.package(root, root / "dist")
            self.assertEqual(archive.name, "gnome-clip-notes-1.0.0-x86_64.tar.gz")
            with tarfile.open(archive, "r:gz") as tar:
                release = json.loads(tar.extractfile("gnome-clip-notes-1.0.0-x86_64/release.json").read())
            self.assertEqual(release["platform"], {"id": "ubuntu", "version_id": "24.04", "arch": "x86_64"})

    @mock.patch.object(PACKAGE, "os_release", return_value={"ID": "fedora", "VERSION_ID": "44"})
    @mock.patch.object(PACKAGE.platform, "machine", return_value="x86_64")
    def test_symlink_binary_rejected(self, _machine, _release):
        temp, root = self.fixture()
        with temp:
            (root / "target/release/gnome-clip-notes").unlink()
            (root / "target/release/gnome-clip-notes").symlink_to("gnome-clip-notes-editor")
            with self.assertRaisesRegex(ValueError, "regular file"):
                PACKAGE.package(root, root / "dist")


if __name__ == "__main__":
    unittest.main()

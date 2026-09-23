#!/usr/bin/env python3
"""Build deterministic, platform-labelled release archives from ready binaries."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
import zipfile

APP_ID = "io.github.OleksiyM.GnomeClipNotes"
EXTENSION_UUID = "gnome-clip-notes@oleksiym.github.io"
ROOT_FILES = ("LICENSE", "README.md")
DOC_FILES = ("installation.md", "privacy.md", "preview.md", "translations.md", "artifact-verification.md")
IDENTITY_DATA = (
    "io.github.OleksiyM.GnomeClipNotes.desktop.in",
    "io.github.OleksiyM.GnomeClipNotes-autostart.desktop.in",
    "io.github.OleksiyM.GnomeClipNotes.service.in",
    "io.github.OleksiyM.GnomeClipNotes.svg",
)
NAME = re.compile(r"^[A-Za-z0-9._-]+$")


def os_release(path: Path = Path("/etc/os-release")) -> dict[str, str]:
    values: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        if "=" not in line or line.startswith(("#", " ")):
            continue
        key, value = line.split("=", 1)
        value = value.strip()
        if len(value) >= 2 and value[0] == value[-1] and value[0] in "\"'":
            value = value[1:-1]
        values[key] = value
    return values


def checked_label(value: str, label: str) -> str:
    if not value or not NAME.fullmatch(value):
        raise ValueError(f"invalid {label}: {value!r}")
    return value


def platform_labels() -> tuple[str, str, str]:
    release = os_release()
    actual = (release.get("ID", ""), release.get("VERSION_ID", ""), platform.machine())
    requested = tuple(os.environ.get(name, actual[index]) for index, name in enumerate(
        ("GCN_PACKAGE_OS_ID", "GCN_PACKAGE_OS_VERSION", "GCN_PACKAGE_ARCH")
    ))
    labels = (checked_label(requested[0], "OS ID"), checked_label(requested[1], "OS version"), checked_label(requested[2], "architecture"))
    for label, wanted, host in zip(("OS ID", "OS version", "architecture"), labels, actual):
        if wanted != host:
            raise ValueError(f"{label} override {wanted!r} does not match build host {host!r}")
    return labels


def safe_copy(source: Path, destination: Path, mode: int | None = None) -> None:
    if source.is_symlink() or not source.is_file():
        raise ValueError(f"required package file is not a regular file: {source}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    os.chmod(destination, mode if mode is not None else 0o644)


def reject_special_tree(root: Path) -> None:
    for path in root.rglob("*"):
        if path.is_symlink() or not (path.is_file() or path.is_dir()):
            raise ValueError(f"unsupported package input: {path}")


def package(project: Path, output: Path) -> tuple[Path, Path]:
    with (project / "Cargo.toml").open("rb") as stream:
        version = tomllib.load(stream)["package"]["version"]
    os_id, version_id, arch = platform_labels()
    package_name = f"gnome-clip-notes-{version}-{os_id}-{version_id}-{arch}"
    required = [project / "target/release/gnome-clip-notes", project / "target/release/gnome-clip-notes-editor"]
    required += [project / "target/locales"]
    required += [project / path for path in ROOT_FILES]
    required += [project / "docs" / path for path in DOC_FILES]
    required += [project / "scripts/install-release.py"]
    required += [project / "data" / path for path in IDENTITY_DATA]
    required += [project / "extension" / path for path in ("metadata.json", "stylesheet.css")]
    required += list((project / "extension").glob("*.js"))
    missing = [str(path.relative_to(project)) for path in required if not path.exists()]
    if missing:
        raise ValueError("missing package inputs: " + ", ".join(missing))
    if (project / "target/locales").is_symlink():
        raise ValueError("target/locales must not be a symlink")
    reject_special_tree(project / "target/locales")
    reject_special_tree(project / "extension/schemas")
    metadata = json.loads((project / "extension/metadata.json").read_text(encoding="utf-8"))
    if metadata.get("uuid") != EXTENSION_UUID:
        raise ValueError("extension metadata UUID does not match release identity")
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="gcn-release-") as raw:
        stage = Path(raw) / package_name
        for binary in required[:2]:
            safe_copy(binary, stage / binary.relative_to(project), 0o755)
        for root_file in ROOT_FILES:
            safe_copy(project / root_file, stage / root_file)
        for doc in DOC_FILES:
            safe_copy(project / "docs" / doc, stage / "docs" / doc)
        for data_file in IDENTITY_DATA:
            safe_copy(project / "data" / data_file, stage / "data" / data_file)
        safe_copy(project / "scripts/install-release.py", stage / "scripts/install-release.py", 0o755)
        for ext in ("metadata.json", "stylesheet.css"):
            safe_copy(project / "extension" / ext, stage / "extension" / ext)
        for source in sorted((project / "extension").glob("*.js")):
            safe_copy(source, stage / "extension" / source.name)
        locales = project / "target/locales"
        shutil.copytree(locales, stage / "target/locales", symlinks=False)
        schema_source = project / "extension/schemas"
        shutil.copytree(schema_source, stage / "extension/schemas", symlinks=False)
        if shutil.which("glib-compile-schemas") is None:
            raise ValueError("glib-compile-schemas is required to package the extension")
        subprocess.run(["glib-compile-schemas", str(stage / "extension/schemas")], check=True)
        manifest = {"format": 1, "version": version, "platform": {"id": os_id, "version_id": version_id, "arch": arch}, "app_id": APP_ID, "extension_uuid": EXTENSION_UUID, "update_protocol": 1}
        manifest["files"] = sorted([*(path.relative_to(stage).as_posix() for path in stage.rglob("*") if path.is_file()), "release.json"])
        (stage / "release.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        archive = output / f"{package_name}.tar.gz"
        with archive.open("wb") as raw, gzip.GzipFile(fileobj=raw, mode="wb", mtime=0, compresslevel=9) as compressed:
            with tarfile.open(fileobj=compressed, mode="w", format=tarfile.PAX_FORMAT) as tar:
                for path in sorted(stage.rglob("*")):
                    relative = path.relative_to(stage.parent).as_posix()
                    info = tar.gettarinfo(path, arcname=relative)
                    if info.issym() or info.islnk() or not (info.isdir() or info.isfile()):
                        raise ValueError(f"unsupported package entry: {relative}")
                    info.uid = info.gid = 0
                    info.uname = info.gname = ""
                    info.mtime = 0
                    info.mode = 0o755 if info.isdir() or os.access(path, os.X_OK) else 0o644
                    with path.open("rb") if info.isfile() else tempfile.TemporaryFile() as stream:
                        tar.addfile(info, stream if info.isfile() else None)
        shell_zip = output / f"gnome-clip-notes-{version}.shell-extension.zip"
        with zipfile.ZipFile(shell_zip, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive_zip:
            for path in sorted((stage / "extension").rglob("*")):
                if path.is_file():
                    name = path.relative_to(stage / "extension").as_posix()
                    info = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
                    info.compress_type = zipfile.ZIP_DEFLATED
                    info.external_attr = (0o755 if path.suffix == ".js" else 0o644) << 16
                    archive_zip.writestr(info, path.read_bytes())
    return archive, shell_zip


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--project-dir", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--output-dir", type=Path, default=None)
    args = parser.parse_args()
    output = args.output_dir or args.project_dir / "dist"
    try:
        archive, shell_zip = package(args.project_dir.resolve(), output.resolve())
    except (OSError, ValueError, json.JSONDecodeError, tomllib.TOMLDecodeError) as error:
        parser.error(str(error))
    print(f"Created {archive}")
    print(f"Created {shell_zip}")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Build two genuinely versioned LOCAL test archives, without changing the checkout.

Versions are disposable test inputs, not public release history. Keep the private
build directory for diagnostics; nothing is installed or published by this script.
"""
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import tomllib


def main():
    project = Path(__file__).resolve().parent.parent
    root = Path(tempfile.mkdtemp(prefix='gcn-lifecycle-fixtures-'))
    source = root / 'source'
    source.mkdir()
    print(f'Private fixture build: {root}', flush=True)
    for name in ('Cargo.toml', 'Cargo.lock', 'build.rs', 'LICENSE', 'README.md'):
        shutil.copy2(project / name, source / name)
    for name in ('src', 'data', 'po', 'extension', 'scripts', 'docs', 'tests/fixtures'):
        shutil.copytree(project / name, source / name,
                        ignore=shutil.ignore_patterns('__pycache__', '*.pyc'))
    manifest = source / 'Cargo.toml'
    original = tomllib.loads(manifest.read_text())['package']['version']
    major, minor, patch = map(int, original.split('.'))
    versions = [original, f'{major}.{minor}.{patch + 1}']
    spec = importlib.util.spec_from_file_location('packager', source / 'scripts/package-release.py')
    packager = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(packager)
    subprocess.run(['python3', str(source / 'scripts/translations.py'), 'build',
                    '--output', str(source / 'target/locales')], check=True)
    env = dict(os.environ)
    # Do not allow an inherited target setting to replace the working binaries.
    env['CARGO_TARGET_DIR'] = str(source / 'target')
    archives = []
    for index, version in enumerate(versions):
        if index:
            text, count = re.subn(r'(?m)^version = "' + re.escape(original) + r'"$',
                                 f'version = "{version}"', manifest.read_text(), count=1)
            assert count == 1
            manifest.write_text(text)
            lock = source / 'Cargo.lock'
            old = f'name = "gnome-clip-notes"\nversion = "{original}"'
            assert lock.read_text().count(old) == 1
            lock.write_text(lock.read_text().replace(old, f'name = "gnome-clip-notes"\nversion = "{version}"'))
            metadata_path = source / 'extension/metadata.json'
            metadata = json.loads(metadata_path.read_text())
            metadata['version'] += 1
            metadata['version-name'] = version
            metadata_path.write_text(json.dumps(metadata, indent=2) + '\n')
        subprocess.run(['cargo', 'build', '--release', '--locked', '--offline', '-j', '2'],
                       cwd=source, env=env, check=True)
        archive = packager.package(source, root / 'archives')
        archives.append(str(archive))
        print(f'Built test-only {version}: {archive}', flush=True)
    (root / 'fixtures.json').write_text(json.dumps({
        'purpose': 'Unsigned disposable lifecycle fixtures; not public releases',
        'versions': versions, 'archives': archives,
    }, indent=2) + '\n')
    print(f'Run: python3 scripts/test-packaged-lifecycle.py {archives[0]} --upgrade-archive {archives[1]}', flush=True)


if __name__ == '__main__':
    main()

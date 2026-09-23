#!/usr/bin/env python3
"""Real local archives and SQLite backup/restore, with GNOME session calls mocked.

No live app/session, network, sudo or system-package installation. --upgrade-archive
must contain a genuinely newer compiled binary, not a relabelled manifest.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import sqlite3
import subprocess
import tarfile
import tempfile

APP_ID = 'io.github.OleksiyM.GnomeClipNotes'
UUID = 'gnome-clip-notes@oleksiym.github.io'


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def execute(argv, env, *, failure=None):
    result = subprocess.run([str(arg) for arg in argv], env=env, text=True,
                            capture_output=True, timeout=45)
    output = result.stdout + result.stderr
    if failure is not None:
        assert result.returncode != 0 and failure in output, output
    elif result.returncode:
        raise RuntimeError(output)
    return output


def unpack(archive, destination):
    destination.mkdir()
    with tarfile.open(archive, 'r:gz') as bundle:
        bundle.extractall(destination, filter='data')
    package, = destination.iterdir()
    assert package.is_dir()
    return package


def database_contents(path):
    with sqlite3.connect(f'{path.as_uri()}?mode=ro', uri=True) as db:
        assert db.execute('PRAGMA integrity_check').fetchone() == ('ok',)
        assert db.execute('PRAGMA foreign_key_check').fetchall() == []
        return tuple(db.execute(f'SELECT * FROM {table} ORDER BY id').fetchall()
                     for table in ('groups', 'items'))


def seed_database(path):
    """Historical schema-1 fixture; writable only inside this temporary profile."""
    path.parent.mkdir(parents=True, exist_ok=True)
    db = sqlite3.connect(path)
    db.executescript('''
        PRAGMA journal_mode=WAL;
        PRAGMA foreign_keys=ON;
        CREATE TABLE groups(id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, position INTEGER NOT NULL);
        INSERT INTO groups VALUES(0,'History',0),(1,'Notes',1),(2,'Research — чернетки',2);
        CREATE TABLE items(id INTEGER PRIMARY KEY, title TEXT NOT NULL DEFAULT '', content TEXT NOT NULL,
            kind TEXT NOT NULL CHECK(kind IN ('text','link')), origin TEXT NOT NULL CHECK(origin IN ('clipboard','manual')),
            source TEXT NOT NULL DEFAULT '', source_id TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL, copied_at INTEGER NOT NULL, group_id INTEGER NOT NULL DEFAULT 0 REFERENCES groups(id));
        CREATE INDEX items_group_copied ON items(group_id,copied_at DESC);
        CREATE INDEX items_created ON items(created_at);
        PRAGMA user_version=1;
    ''')
    db.executemany('INSERT INTO items VALUES(?,?,?,?,?,?,?,?,?,?,?)', [
        (1, '', 'clipboard\nline two', 'text', 'clipboard', 'Fixture', 'fixture.desktop', 123, 124, 125, 0),
        (2, 'Нотатка', '# Markdown\n\n**Hello** — Привіт 🌿\n', 'text', 'manual', '', '', 126, 127, 128, 1),
        (3, 'Reference', 'https://example.com/?a=1&b=2', 'link', 'manual', '', '', 129, 130, 131, 2),
    ])
    db.commit()
    return db  # Keep WAL alive while the real application makes its backup.


def managed_files(app):
    receipt_path = app / '.install-receipt.json'
    receipt = json.loads(receipt_path.read_text())
    paths = [Path(path) for path in receipt['owned_files']] + [receipt_path]
    return {str(path): (digest(path), path.stat().st_mode & 0o777) for path in paths}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('archive', type=Path, help='Trusted, locally built archive')
    parser.add_argument('--upgrade-archive', type=Path, help='Newer independently compiled local archive')
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='gcn-packaged-lifecycle-') as temporary:
        root = Path(temporary)
        package = unpack(args.archive, root / 'unpacked-old')
        newer = unpack(args.upgrade_archive, root / 'unpacked-new') if args.upgrade_archive else None
        home = root / 'home'
        home.mkdir()
        data = home / '.local/share'
        config = home / '.config'
        app = home / 'Applications with spaces/Clip Notes'
        mocks = root / 'mock-bin'
        mocks.mkdir()
        compiler = shutil.which('glib-compile-schemas')
        assert compiler, 'glib-compile-schemas is required'
        fail_compiler = root / 'fail-compiler'
        commands = {
            'gdbus': '#!/bin/sh\ncase "$*" in *NameHasOwner*) printf "(false,)\\n";; *) exit 98;; esac\n',
            'gsettings': '#!/bin/sh\nprintf "@as []\\n"\n',
            'gnome-extensions': '#!/bin/sh\nexit 99\n',
            'glib-compile-schemas': f'#!/bin/sh\nif [ -f {shlex.quote(str(fail_compiler))} ]; then echo FIXTURE_COMPILER_FAILURE >&2; exit 97; fi\nexec {shlex.quote(compiler)} "$@"\n',
        }
        for name, contents in commands.items():
            path = mocks / name
            path.write_text(contents)
            path.chmod(0o755)
        runtime = root / 'runtime'
        runtime.mkdir(mode=0o700)
        env = dict(os.environ, HOME=str(home), XDG_DATA_HOME=str(data),
                   XDG_CONFIG_HOME=str(config), XDG_CACHE_HOME=str(root / 'cache'),
                   XDG_RUNTIME_DIR=str(runtime), XDG_STATE_HOME=str(root / 'state'),
                   GSETTINGS_BACKEND='memory', LANGUAGE='en', LANG='C.UTF-8',
                   DBUS_SESSION_BUS_ADDRESS=f'unix:path={root}/no-live-bus',
                   PATH=str(mocks) + os.pathsep + os.environ['PATH'])
        env.pop('DISPLAY', None)
        env.pop('WAYLAND_DISPLAY', None)

        def version(path):
            return execute([path / 'gnome-clip-notes', '--version'], env).strip().split()[-1]

        old_version = json.loads((package / 'release.json').read_text())['version']
        assert version(package / 'target/release') == old_version
        if newer:
            new_version = json.loads((newer / 'release.json').read_text())['version']
            assert tuple(map(int, new_version.split('.'))) > tuple(map(int, old_version.split('.')))
            assert version(newer / 'target/release') == new_version
            for binary in ('gnome-clip-notes', 'gnome-clip-notes-editor'):
                assert digest(package / 'target/release' / binary) != digest(newer / 'target/release' / binary)

        def install(script, *options, failure=None):
            return execute(['python3', script, '--app-dir', app, '--yes', *options], env, failure=failure)

        helper = package / 'scripts/install-release.py'
        database = data / 'gnome-clip-notes/data.db'
        settings = config / 'gnome-clip-notes/settings.json'
        settings.parent.mkdir(parents=True)
        settings.write_text('{"open_at_login": false, "retention_days": 0, "preview_mode": "native", "language": "en", "fixture": "Привіт"}\n')
        settings_bytes = settings.read_bytes()
        autostart = config / f'autostart/{APP_ID}.desktop'
        assert not database.exists()
        assert 'installed' in install(helper)
        assert not database.exists(), 'Installer must not create or open the notes database'
        assert version(app) == old_version and not autostart.exists()
        before_repeat = managed_files(app)
        assert 'already installed' in install(helper)
        assert managed_files(app) == before_repeat

        seed = seed_database(database)
        expected = database_contents(database)
        backup = root / 'backup with spaces.db'
        assert Path(str(database) + '-wal').stat().st_size > 0
        execute([app / 'gnome-clip-notes', '--backup', backup], env)
        backup_hash = digest(backup)
        assert database_contents(backup) == expected
        assert backup.stat().st_mode & 0o777 == 0o600
        execute([app / 'gnome-clip-notes', '--backup', backup], env, failure='already exists')
        assert digest(backup) == backup_hash
        seed.close()

        def verify_data():
            assert database_contents(database) == expected
            assert settings.read_bytes() == settings_bytes
            assert digest(backup) == backup_hash
            assert not autostart.exists()

        if newer:
            new_helper = newer / 'scripts/install-release.py'
            old_files = managed_files(app)
            foreign = data / f'gnome-shell/extensions/{UUID}/personal-before-update.txt'
            foreign.write_text('must not disappear during replacement')
            install(new_helper, failure=str(foreign))
            assert foreign.read_text() == 'must not disappear during replacement'
            assert managed_files(app) == old_files
            foreign.rename(root / 'preserved-personal-file.txt')
            fail_compiler.touch()  # Failure after program targets were replaced.
            install(new_helper, failure='returned non-zero exit status 97')
            fail_compiler.unlink()
            assert managed_files(app) == old_files
            assert version(app) == old_version
            assert not list((data / 'gnome-clip-notes/installer').glob('*/incomplete.json'))
            verify_data()
            assert 'installed' in install(new_helper)
            assert version(app) == new_version
            for binary in ('gnome-clip-notes', 'gnome-clip-notes-editor'):
                assert digest(app / binary) == digest(newer / 'target/release' / binary)
            installed_ext = data / f'gnome-shell/extensions/{UUID}'
            for path in (newer / 'extension').rglob('*'):
                if path.is_file():
                    assert digest(installed_ext / path.relative_to(newer / 'extension')) == digest(path)
            assert digest(app / 'install-release.py') == digest(new_helper)
            upgraded = managed_files(app)
            assert json.loads((app / '.install-receipt.json').read_text())['version'] == new_version
            install(helper, failure='downgrades are not supported')
            assert managed_files(app) == upgraded
            assert 'already installed' in install(new_helper)
            assert managed_files(app) == upgraded
            verify_data()
            print(f'PASS actual compiled {old_version} → {new_version}, full-file rollback, downgrade refusal and no-op repeat')

        receipt = json.loads((app / '.install-receipt.json').read_text())
        unrelated = app / 'my-file.txt'
        unrelated.write_text('keep')
        foreign_ext = data / f'gnome-shell/extensions/{UUID}/personal.txt'
        foreign_ext.write_text('keep extension-side file')
        shutil.rmtree(package)
        if newer:
            shutil.rmtree(newer)
        assert 'Removed recorded' in install(app / 'install-release.py', '--uninstall')
        assert all(not Path(path).exists() for path in receipt['owned_files'])
        assert not (app / '.install-receipt.json').exists()
        assert unrelated.read_text() == 'keep'
        assert foreign_ext.read_text() == 'keep extension-side file'
        verify_data()

        # A clean profile receives only the real backup and the saved settings.
        restored_home = root / 'restored home'
        restored_data = restored_home / '.local/share'
        restored_config = restored_home / '.config'
        restored_db = restored_data / 'gnome-clip-notes/data.db'
        restored_settings = restored_config / 'gnome-clip-notes/settings.json'
        restored_db.parent.mkdir(parents=True)
        restored_settings.parent.mkdir(parents=True)
        shutil.copy2(backup, restored_db)
        shutil.copy2(settings, restored_settings)
        restored_env = dict(env, HOME=str(restored_home), XDG_DATA_HOME=str(restored_data),
                            XDG_CONFIG_HOME=str(restored_config))
        latest = unpack(args.upgrade_archive or args.archive, root / 'unpacked-restore')
        restored_app = restored_home / 'Applications with spaces/Clip Notes'
        execute(['python3', latest / 'scripts/install-release.py', '--app-dir', restored_app, '--yes'], restored_env)
        restored_backup = root / 'restored snapshot.db'
        execute([restored_app / 'gnome-clip-notes', '--backup', restored_backup], restored_env)
        assert database_contents(restored_db) == database_contents(restored_backup) == expected
        assert restored_settings.read_bytes() == settings_bytes
        assert not (restored_config / f'autostart/{APP_ID}.desktop').exists()
        print('PASS clean install, real WAL-aware backup, overwrite refusal, offline uninstall and clean-profile restore')
        print('PACKAGED_LIFECYCLE_OK: local unsigned binaries; GNOME/D-Bus mocked, not target-session certification')


if __name__ == '__main__':
    main()

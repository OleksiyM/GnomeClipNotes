#!/usr/bin/env bash
# Public bootstrap. Keep execution inside the complete function for curl | bash.
gcn_install() (
    set -euo pipefail
    local repo=OleksiyM/GnomeClipNotes tag='' app_dir='' accept=no require_provenance=no verify_provenance=no install_deps=no
    local version platform_label archive_name package_name temporary latest help_text
    while (($#)); do
        case "$1" in
            --yes) accept=yes; shift ;;
            --install-deps) install_deps=yes; shift ;;
            --require-provenance) require_provenance=yes; shift ;;
            --version|--app-dir)
                [[ $# -ge 2 && -n "$2" ]] || { printf 'Missing value for %s\n' "$1" >&2; exit 1; }
                if [[ "$1" == --version ]]; then tag=$2; else app_dir=$2; fi
                shift 2 ;;
            --help)
                printf '%s\n' 'GnomeClipNotes install/update' \
                    'Usage: bash install.sh [--version v1.0.0] [--app-dir PATH] [--yes] [--install-deps] [--require-provenance]' \
                    '--install-deps explicitly permits missing runtime package installation; --yes alone does not.' \
                    'Requires curl and Python 3. SHA256 integrity is always checked.' \
                    'If GitHub CLI is installed, signed provenance is also checked; failures stop installation.' \
                    'No GitHub login required. Without --yes, confirmation uses /dev/tty.'
                exit 0 ;;
            *) printf 'Unknown option: %s\n' "$1" >&2; exit 1 ;;
        esac
    done
    for command_name in curl python3; do
        command -v "$command_name" >/dev/null || {
            printf 'Missing %s. Install it from a trusted package source, then retry.\n' "$command_name" >&2
            exit 1
        }
    done
    if command -v gh >/dev/null; then
        verify_provenance=yes
        help_text=$(gh attestation verify --help) || {
            printf 'Installed GitHub CLI cannot verify provenance. Update gh from a trusted package source.\n' >&2; exit 1;
        }
        for option in --bundle --repo --signer-workflow --source-ref --deny-self-hosted-runners --cert-oidc-issuer --predicate-type; do
            [[ "$help_text" == *"$option"* ]] || {
                printf 'Your GitHub CLI lacks %s. Update gh from a trusted package source.\n' "$option" >&2
                exit 1
            }
        done
    elif [[ "$require_provenance" == yes ]]; then
        printf 'Provenance verification was required, but gh is missing. Install GitHub CLI from a trusted package source.\n' >&2
        exit 1
    else
        printf '%s\n' 'GitHub CLI is not installed: SHA256 integrity will be checked; signed provenance verification is skipped.' \
            'For provenance verification, install gh from a trusted package source and rerun with --require-provenance.'
    fi
    platform_label=$(python3 - <<'PY'
import platform
from pathlib import Path
values = {}
for line in Path('/etc/os-release').read_text().splitlines():
    if '=' in line and not line.startswith('#'):
        key, value = line.split('=', 1)
        values[key] = value.strip().strip('\"\'')
label = '-'.join((values.get('ID', ''), values.get('VERSION_ID', ''), platform.machine()))
if label not in ('fedora-44-x86_64', 'ubuntu-24.04-x86_64'):
    raise SystemExit('No release package configured for this platform: ' + label)
print(label)
PY
    )
    local -a download=(curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' --tlsv1.2 --connect-timeout 15 --max-time 300 --retry 2)
    if [[ -z "$tag" ]]; then
        latest=$("${download[@]}" --head --output /dev/null --write-out '%{url_effective}' "https://github.com/$repo/releases/latest")
        [[ "$latest" == "https://github.com/$repo/releases/tag/"* ]] || {
            printf 'No stable GnomeClipNotes release found.\n' >&2; exit 1;
        }
        tag=${latest#"https://github.com/$repo/releases/tag/"}
    fi
    [[ "$tag" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] || {
        printf 'Expected a stable release tag, for example v1.0.0.\n' >&2; exit 1;
    }
    version=${tag#v}
    package_name="gnome-clip-notes-$version-$platform_label"
    archive_name="$package_name.tar.gz"
    temporary=$(mktemp -d /tmp/gnome-clip-notes-download.XXXXXXXX)
    # Only our exact, freshly-created private download directory is cleaned up.
    trap 'python3 -c '\''import shutil,sys; shutil.rmtree(sys.argv[1])'\'' "$temporary"' EXIT
    printf 'Downloading GnomeClipNotes %s (%s)…\n' "$version" "$platform_label"
    "${download[@]}" --max-filesize 268435456 --output "$temporary/$archive_name" "https://github.com/$repo/releases/download/$tag/$archive_name"
    "${download[@]}" --max-filesize 1048576 --output "$temporary/SHA256SUMS" "https://github.com/$repo/releases/download/$tag/SHA256SUMS"
    python3 - "$temporary/$archive_name" "$temporary/SHA256SUMS" "$archive_name" <<'PY'
import hashlib
import re
import sys
from pathlib import Path
archive, manifest, expected_name = sys.argv[1:]
data = Path(manifest).read_bytes()
if len(data) > 1048576:
    raise SystemExit('Checksum manifest is too large')
matches = []
for line in data.decode('ascii').splitlines():
    parsed = re.fullmatch(r'([0-9a-fA-F]{64}) [ *]([^\r\n]+)', line)
    if not parsed:
        raise SystemExit('Invalid SHA256SUMS format')
    if parsed[2] == expected_name:
        matches.append(parsed[1].lower())
if len(matches) != 1:
    raise SystemExit('SHA256SUMS must contain exactly one entry for the requested archive')
digest = hashlib.sha256()
with open(archive, 'rb') as stream:
    for chunk in iter(lambda: stream.read(1024 * 1024), b''):
        digest.update(chunk)
if digest.hexdigest() != matches[0]:
    raise SystemExit('Archive SHA256 mismatch; installation stopped')
print('SHA256 integrity verified.')
PY
    if [[ "$verify_provenance" == yes ]]; then
        "${download[@]}" --max-filesize 16777216 --output "$temporary/$archive_name.sigstore.json" "https://github.com/$repo/releases/download/$tag/$archive_name.sigstore.json"
        printf 'Verifying signed build provenance…\n'
        gh attestation verify "$temporary/$archive_name" \
        --bundle "$temporary/$archive_name.sigstore.json" \
        --repo "$repo" \
        --signer-workflow "$repo/.github/workflows/release.yml" \
        --source-ref "refs/tags/$tag" \
        --deny-self-hosted-runners \
        --cert-oidc-issuer https://token.actions.githubusercontent.com \
        --predicate-type https://slsa.dev/provenance/v1
    fi
    # Integrity (and provenance when enabled) passed. Validate paths, types and
    # sizes before extraction; do not execute a release-provided extractor.
    python3 - "$temporary/$archive_name" "$temporary" "$package_name" "$version" "$platform_label" <<'PY'
import json
import shutil
import sys
import tarfile
from pathlib import Path, PurePosixPath
archive, destination, root, version, platform_label = sys.argv[1:]
with tarfile.open(archive, 'r:gz') as stream:
    members, names, total = [], {}, 0
    for member in stream:
        name = member.name.rstrip('/')
        parts = PurePosixPath(name).parts
        if (not parts or parts[0] != root or name.startswith('/') or
                any(part in ('', '.', '..') for part in name.split('/')) or
                any(ord(char) < 32 or ord(char) == 127 for char in name) or
                '\\' in name or name in names or
                not (member.isdir() or member.isfile()) or member.issparse()):
            raise SystemExit('Unsafe or duplicate archive entry: ' + repr(name))
        total += member.size
        if len(members) >= 10000 or member.size < 0 or member.size > 268435456 or total > 536870912:
            raise SystemExit('Release archive exceeds extraction limits')
        names[name] = member.isdir()
        members.append(member)
    for name in names:
        for parent in PurePosixPath(name).parents:
            if str(parent) in names and not names[str(parent)]:
                raise SystemExit('Archive has a file/directory conflict')
    manifest_name = root + '/release.json'
    if manifest_name not in names or names[manifest_name]:
        raise SystemExit('Missing release.json')
    manifest_member = next(item for item in members if item.name == manifest_name)
    if manifest_member.size > 1048576:
        raise SystemExit('Release metadata is too large')
    manifest = json.load(stream.extractfile(manifest_member))
    os_id, os_version, arch = platform_label.split('-')
    expected = dict(format=1, version=version,
                    platform=dict(id=os_id, version_id=os_version, arch=arch),
                    app_id='io.github.OleksiyM.GnomeClipNotes',
                    extension_uuid='gnome-clip-notes@oleksiym.github.io', update_protocol=1)
    if not isinstance(manifest, dict) or any(manifest.get(key) != value for key, value in expected.items()):
        raise SystemExit('Release metadata does not match the requested release')
    helper = root + '/scripts/install-release.py'
    if helper not in names or names[helper]:
        raise SystemExit('Missing release installer')
    for member in members:
        target = Path(destination) / member.name
        if member.isdir():
            target.mkdir(parents=True, exist_ok=True)
            target.chmod(0o755)
        else:
            target.parent.mkdir(parents=True, exist_ok=True)
            with stream.extractfile(member) as source, target.open('xb') as output:
                shutil.copyfileobj(source, output)
            executable = member.name in (root + '/target/release/gnome-clip-notes',
                                         root + '/target/release/gnome-clip-notes-editor')
            target.chmod(0o755 if executable else 0o644)
PY
    local -a arguments=(--package-dir "$temporary/$package_name")
    [[ "$accept" != yes ]] || arguments+=(--yes)
    [[ "$install_deps" != yes ]] || arguments+=(--install-deps)
    [[ -z "$app_dir" ]] || arguments+=(--app-dir "$app_dir")
    python3 "$temporary/$package_name/scripts/install-release.py" "${arguments[@]}"
)
gcn_install "$@"

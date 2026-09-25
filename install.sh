#!/usr/bin/env bash
# Standard per-user install/update. Save notes and stop the app before running.
# No Python, package manager, process killing or rollback. See docs/installation.md.
# Keep the entry point inside a function so curl | bash reads it before execution.
gcn_standard_install() (
    set -euo pipefail
    fail() { printf 'Install failed: %s\n' "$*" >&2; exit 1; }
    trap 'printf "Installation did not complete. See the error above; fix it and rerun.\n" >&2' ERR

    repo=OleksiyM/GnomeClipNotes
    app_id=io.github.OleksiyM.GnomeClipNotes
    uuid=gnome-clip-notes@oleksiym.github.io
    app_dir=${GNOME_CLIP_NOTES_APP_DIR:-"$HOME/Applications/GnomeClipNotes"}
    data_dir=${XDG_DATA_HOME:-"$HOME/.local/share"}
    config_dir=${XDG_CONFIG_HOME:-"$HOME/.config"}
    tag=''
    require_provenance=no
    while (($#)); do
        case "$1" in
            --version)
                [[ $# -ge 2 ]] || fail '--version needs a tag such as v1.0.0'
                tag=$2; shift 2 ;;
            --require-provenance) require_provenance=yes; shift ;;
            -h|--help)
                printf '%s\n' 'Standard install/update: bash install.sh [--version v1.0.0] [--require-provenance]' \
                    'First install runtime dependencies, save/close editors, disable the extension and quit the app.' \
                    'Installs in ~/Applications/GnomeClipNotes; GNOME_CLIP_NOTES_APP_DIR overrides that path.' \
                    'No dependency installation, process termination or automatic rollback.'
                exit 0 ;;
            *) fail "Unknown option: $1" ;;
        esac
    done
    printf '\n%s\n\n' '💡 Before continuing: save/close editors, disable the extension and quit GnomeClipNotes. Standard does not stop it for you.'
    [[ $EUID -ne 0 ]] || fail 'Run as your desktop user, not with sudo.'
    for directory in "$app_dir" "$data_dir" "$config_dir"; do
        [[ $directory == /* && $directory != / ]] || fail 'Installation and XDG paths must be absolute, non-root paths.'
    done
    case "$app_dir" in
        *['"$`%\']*|*'|'*|*'&'*|*$'\n'*|*$'\r'*) fail 'Installation path contains characters unsupported in desktop launchers.' ;;
    esac
    [[ ! -f "$app_dir/.install-receipt.json" ]] || fail 'This is a Guided installation. Uninstall it with its installed Python helper (notes are kept) before switching to Standard.'
    for tool in curl tar gzip sha256sum awk sed install mktemp uname glib-compile-schemas; do
        command -v "$tool" >/dev/null || fail "Missing $tool. Install dependencies first; see docs/installation.md."
    done
    case "$(uname -m)" in
        x86_64) arch=x86_64 ;;
        aarch64|arm64) arch=aarch64 ;;
        *) fail 'Only x86_64 and ARM64 release archives are supported.' ;;
    esac

    download=(curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' --connect-timeout 15 --max-time 300)
    if [[ -z "$tag" ]]; then
        latest=$("${download[@]}" --head --output /dev/null --write-out '%{url_effective}' "https://github.com/$repo/releases/latest")
        [[ $latest == "https://github.com/$repo/releases/tag/"* ]] || fail 'No stable release found.'
        tag=${latest##*/}
    fi
    [[ $tag =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] || fail 'Expected a stable tag such as v1.0.0.'
    package="gnome-clip-notes-${tag#v}-$arch"
    archive="$package.tar.gz"
    url="https://github.com/$repo/releases/download/$tag"
    temporary=$(mktemp -d /tmp/gnome-clip-notes-standard.XXXXXXXX)
    trap 'rm -rf -- "$temporary"' EXIT
    printf '\n📦 GnomeClipNotes %s · %s\n\n⬇️  Downloading release…\n' "$tag" "$arch"
    "${download[@]}" --output "$temporary/$archive" "$url/$archive"
    "${download[@]}" --output "$temporary/SHA256SUMS" "$url/SHA256SUMS"
    printf '\n🔒 Verifying SHA-256…\n'
    # Select the exact archive, not a substring or other paths in the manifest.
    hash=$(awk -v name="$archive" '$2 == name || $2 == "*" name { print $1 }' "$temporary/SHA256SUMS")
    [[ $hash =~ ^[0-9a-fA-F]{64}$ ]] || fail 'Missing or duplicate archive checksum.'
    (cd "$temporary"; printf '%s  %s\n' "$hash" "$archive" | sha256sum --check --status -) || fail 'SHA-256 mismatch. Nothing installed.'
    printf '✅ SHA-256 verified.\n'

    # Probe capabilities, not gh's version string. Old gh is optional too.
    verify=no
    if command -v gh >/dev/null && help=$(gh attestation verify --help 2>/dev/null); then
        verify=yes
        for option in --bundle --repo --signer-workflow --source-ref --cert-oidc-issuer --deny-self-hosted-runners --predicate-type; do
            [[ $help == *"$option"* ]] || verify=no
        done
    fi
    if [[ $verify == yes ]]; then
        printf '\n🛡️  Verifying release provenance…\n'
        "${download[@]}" --output "$temporary/$archive.sigstore.json" "$url/$archive.sigstore.json"
        gh attestation verify "$temporary/$archive" \
            --bundle "$temporary/$archive.sigstore.json" --repo "$repo" \
            --signer-workflow "$repo/.github/workflows/release.yml" --source-ref "refs/tags/$tag" \
            --cert-oidc-issuer https://token.actions.githubusercontent.com \
            --deny-self-hosted-runners --predicate-type https://slsa.dev/provenance/v1
        printf '✅ Provenance verified.\n'
    elif [[ $require_provenance == yes ]]; then
        fail '--require-provenance needs gh with the required attestation flags. Install or update gh from a trusted source.'
    else
        printf '%s\n' '' 'ℹ️  Provenance skipped: gh is missing or too old. SHA-256 integrity passed.' \
            'For signed verification, install/update gh and rerun with --require-provenance.'
    fi

    # Extract the verified release into staging, never into the live installation.
    tar -xzf "$temporary/$archive" --no-same-owner --no-same-permissions -C "$temporary"
    source_dir="$temporary/$package"
    for file in target/release/gnome-clip-notes target/release/gnome-clip-notes-editor \
        "data/$app_id.svg" "data/$app_id.desktop.in" "data/$app_id.service.in" \
        "data/$app_id-autostart.desktop.in" extension/metadata.json; do
        [[ -f "$source_dir/$file" ]] || fail "Archive is missing $file. Nothing installed."
    done
    [[ -d "$source_dir/target/locales" && -d "$source_dir/extension/schemas" ]] || fail 'Archive is missing locales or extension schemas.'
    glib-compile-schemas "$source_dir/extension/schemas"

    printf '\n🚀 Installing to %s…\n' "$app_dir"
    install -d "$app_dir/share" "$data_dir/applications" "$data_dir/dbus-1/services" \
        "$data_dir/icons/hicolor/scalable/apps" "$config_dir/autostart" "$data_dir/gnome-shell/extensions"
    install -m 0755 "$source_dir/target/release/gnome-clip-notes" "$app_dir/gnome-clip-notes"
    install -m 0755 "$source_dir/target/release/gnome-clip-notes-editor" "$app_dir/gnome-clip-notes-editor"
    # Replace only these application-owned trees; no data/config directories.
    rm -rf -- "$app_dir/share/locale" "$data_dir/gnome-shell/extensions/$uuid"
    cp -R "$source_dir/target/locales" "$app_dir/share/locale"
    cp -R "$source_dir/extension" "$data_dir/gnome-shell/extensions/$uuid"
    install -m 0644 "$source_dir/data/$app_id.svg" "$data_dir/icons/hicolor/scalable/apps/$app_id.svg"
    sed "s|@BINARY@|$app_dir/gnome-clip-notes|g" "$source_dir/data/$app_id.desktop.in" > "$data_dir/applications/$app_id.desktop"
    sed "s|@BINARY@|$app_dir/gnome-clip-notes|g" "$source_dir/data/$app_id.service.in" > "$data_dir/dbus-1/services/$app_id.service"
    sed "s|@BINARY@|$app_dir/gnome-clip-notes|g" "$source_dir/data/$app_id-autostart.desktop.in" > "$config_dir/autostart/$app_id.desktop"
    chmod 0644 "$data_dir/applications/$app_id.desktop" "$data_dir/dbus-1/services/$app_id.service" "$config_dir/autostart/$app_id.desktop"
    if command -v update-desktop-database >/dev/null; then update-desktop-database "$data_dir/applications" || true; fi
    if command -v gtk-update-icon-cache >/dev/null; then gtk-update-icon-cache -f -t "$data_dir/icons/hicolor" 2>/dev/null || true; fi
    printf '%s\n' '✅ Installed successfully.' '📝 Notes and settings were not changed.'
    printf '%s\n' '' '💡 ONE MORE STEP — clipboard capture needs the extension:' \
        '   1. Log out and back in.' \
        '   2. Open Extensions and enable GnomeClipNotes.' \
        '      Or run this command after logging back in:' "      gnome-extensions enable $uuid" \
        '   These steps are also in the website installation guide — no need to keep this terminal open.'
)
gcn_standard_install "$@"

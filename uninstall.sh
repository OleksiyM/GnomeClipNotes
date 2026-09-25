#!/usr/bin/env bash
# Standalone Standard removal. Save/close editors and quit the app first.
gcn_standard_uninstall() (
    set -euo pipefail
    fail() { printf 'Uninstall failed: %s\n' "$*" >&2; exit 1; }
    app_id=io.github.OleksiyM.GnomeClipNotes
    uuid=gnome-clip-notes@oleksiym.github.io
    app_dir=${GNOME_CLIP_NOTES_APP_DIR:-"$HOME/Applications/GnomeClipNotes"}
    data_dir=${XDG_DATA_HOME:-"$HOME/.local/share"}
    config_dir=${XDG_CONFIG_HOME:-"$HOME/.config"}
    purge=no
    for arg in "$@"; do
        case "$arg" in
            --purge) purge=yes ;;
            -h|--help)
                printf '%s\n' 'Standard uninstall: bash uninstall.sh [--purge]' \
                    'Save/close editors and quit GnomeClipNotes before running.' \
                    'Notes and settings are kept unless --purge is explicitly supplied.' \
                    'WARNING: --purge also deletes History, folders and backups in the app data directory.'
                exit 0 ;;
            *) fail "Unknown option: $arg" ;;
        esac
    done
    printf '%s\n' '' '📦 GnomeClipNotes · Standard uninstall' '' \
        '💡 Save/close editors and quit GnomeClipNotes before continuing. This script does not stop it for you.' ''
    [[ $EUID -ne 0 ]] || fail 'Run as your desktop user, not with sudo.'
    for directory in "$app_dir" "$data_dir" "$config_dir"; do
        [[ $directory == /* && $directory != / ]] || fail 'Installation and XDG paths must be absolute, non-root paths.'
    done
    [[ ! -f "$app_dir/.install-receipt.json" ]] || fail 'This is a Guided installation. Use its installed install-release.py --uninstall helper instead.'
    printf '🧩 Disabling the Shell extension…\n'
    if command -v gnome-extensions >/dev/null; then gnome-extensions disable "$uuid" || true; fi
    printf '🗑️  Removing application and desktop integration…\n'
    rm -rf -- "$data_dir/gnome-shell/extensions/$uuid" "$app_dir/share/locale"
    rm -f -- "$data_dir/applications/$app_id.desktop" "$config_dir/autostart/$app_id.desktop" \
        "$data_dir/dbus-1/services/$app_id.service" "$data_dir/icons/hicolor/scalable/apps/$app_id.svg" \
        "$app_dir/gnome-clip-notes" "$app_dir/gnome-clip-notes-editor"
    # Leave unrelated files in the installation directory alone.
    rmdir -- "$app_dir/share" "$app_dir" 2>/dev/null || true
    if [[ $purge == yes ]]; then
        printf '%s\n' '⚠️  Removing ALL notes, History, folders, settings and backups in the application data directory.'
        rm -rf -- "$config_dir/gnome-clip-notes" "$data_dir/gnome-clip-notes"
    else
        printf '%s\n' '📝 Notes, History, folders, settings and backups were kept.'
    fi
    if command -v update-desktop-database >/dev/null; then update-desktop-database "$data_dir/applications" || true; fi
    if command -v gtk-update-icon-cache >/dev/null; then gtk-update-icon-cache -f -t "$data_dir/icons/hicolor" 2>/dev/null || true; fi
    printf '%s\n' '' '✅ GnomeClipNotes uninstalled.' \
        '💡 Log out and back in to unload cached Shell extension code.'
)
gcn_standard_uninstall "$@"

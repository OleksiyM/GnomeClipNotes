#!/usr/bin/env bash
set -euo pipefail

# Offline, receipt-aware removal; never guess ownership of a legacy install.
project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
app_dir=${GNOME_CLIP_NOTES_APP_DIR:-"${HOME}/Applications/GnomeClipNotes"}
exec python3 "${project_dir}/scripts/install-release.py" --uninstall --app-dir "$app_dir" "$@"

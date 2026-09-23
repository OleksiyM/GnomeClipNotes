#!/usr/bin/env bash
set -euo pipefail
project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
test_root=$(mktemp -d /tmp/gnome-clip-notes-i18n.XXXXXX)
export GCN_I18N_TEST_DIR="${test_root}"
export XDG_CONFIG_HOME="${test_root}/config"
export XDG_DATA_HOME="${test_root}/data"
export XDG_CACHE_HOME="${test_root}/cache"
export XDG_RUNTIME_DIR="${test_root}/runtime"
export LANGUAGE=en_XA LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8
export GSETTINGS_BACKEND=memory GSK_RENDERER=cairo GTK_A11Y=none GDK_BACKEND=x11
unset WAYLAND_DISPLAY
mkdir -m 700 -p "${XDG_RUNTIME_DIR}" "${XDG_CONFIG_HOME}"
"${project_dir}/scripts/translations.py" pseudo --output "${test_root}/locale"
printf 'Isolated pseudolocale UI test: %s\n' "${test_root}"
timeout 90s xvfb-run -a -s '-screen 0 1600x1000x24' dbus-run-session -- \
    "${project_dir}/target/debug/gnome-clip-notes" --test-i18n

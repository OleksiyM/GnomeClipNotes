#!/usr/bin/env bash
set -euo pipefail
project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
test_dir=$(mktemp -d /tmp/gnome-clip-notes-test.XXXXXX)
export GCN_SMOKE_DIR="${test_dir}"
export XDG_DATA_HOME="${test_dir}/data"
export XDG_CONFIG_HOME="${test_dir}/config"
export XDG_CACHE_HOME="${test_dir}/cache"
export XDG_RUNTIME_DIR="${test_dir}/runtime"
export GSETTINGS_BACKEND=memory
export GSK_RENDERER=cairo
export GTK_A11Y=none
export GDK_BACKEND=x11
export LANGUAGE=en LANG=C.UTF-8
unset WAYLAND_DISPLAY
mkdir -m 700 -p "${XDG_RUNTIME_DIR}"
test_schemas="${XDG_DATA_HOME}/gnome-shell/extensions/gnome-clip-notes@oleksiym.github.io/schemas"
mkdir -p "${test_schemas}"
install -m 0644 "${project_dir}/extension/schemas/org.gnome.shell.extensions.gnome-clip-notes.gschema.xml" "${test_schemas}/"
glib-compile-schemas "${test_schemas}"
printf 'Isolated UI test: %s\n' "${test_dir}"
test_argument=--smoke-test
if [[ ${1:-} == --export ]]; then test_argument=--test-export; fi
timeout 90s xvfb-run -a -s '-screen 0 1600x1000x24' dbus-run-session -- "${project_dir}/target/debug/gnome-clip-notes" "${test_argument}"

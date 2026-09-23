#!/usr/bin/env bash
set -euo pipefail
# Run only the isolated prototype; never attach to the user's session or notes.
prototype_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
if [[ "${1:-}" != --inside ]]; then
    test_dir=$(mktemp -d /tmp/gcn-webkit-prototype.XXXXXX)
    export GCN_PROTOTYPE_OUTPUT="${test_dir}"
    export GCN_PROTOTYPE_DIR="${prototype_dir}"
    export XDG_DATA_HOME="${test_dir}/data" XDG_CONFIG_HOME="${test_dir}/config"
    export XDG_CACHE_HOME="${test_dir}/cache" XDG_RUNTIME_DIR="${test_dir}/runtime"
    mkdir -m 700 -p "${XDG_RUNTIME_DIR}" "${XDG_DATA_HOME}" "${XDG_CONFIG_HOME}" "${XDG_CACHE_HOME}"
    export GSETTINGS_BACKEND=memory GTK_A11Y=none GDK_BACKEND=wayland
    export WAYLAND_DISPLAY=gcn-webkit-test
    unset DISPLAY
    printf 'Prototype results: %s\n' "${test_dir}"
    dbus-run-session -- bash "$0" --inside "$@" 2>"${test_dir}/session.log"
    exit
fi
shift
shell_pid=''
cleanup() {
    if [[ -n "${shell_pid}" ]]; then
        kill "${shell_pid}" 2>/dev/null || true
        wait "${shell_pid}" 2>/dev/null || true
    fi
}
trap cleanup EXIT
gnome-shell --headless --wayland --no-x11 --virtual-monitor 1440x900 \
    --wayland-display "${WAYLAND_DISPLAY}" >"${GCN_PROTOTYPE_OUTPUT}/shell.log" 2>&1 &
shell_pid=$!
for _ in {1..100}; do
    [[ -S "${XDG_RUNTIME_DIR}/${WAYLAND_DISPLAY}" ]] && break
    kill -0 "${shell_pid}"
    sleep .1
done
[[ -S "${XDG_RUNTIME_DIR}/${WAYLAND_DISPLAY}" ]]
# Wait until Shell/portals settle; otherwise their first startup dominates GTK.
gdbus call --session --dest org.freedesktop.portal.Desktop \
    --object-path /org/freedesktop/portal/desktop --method org.freedesktop.DBus.Peer.Ping >/dev/null
sleep 2
export GCN_START_NS
GCN_START_NS=$(date +%s%N)
"${GCN_PROTOTYPE_BINARY:-${GCN_PROTOTYPE_DIR}/target/release/gcn-webkit-preview-prototype}" --auto "$@" \
    >"${GCN_PROTOTYPE_OUTPUT}/metrics.log" 2>"${GCN_PROTOTYPE_OUTPUT}/app.log"
printf 'Completed: %s\n' "${GCN_PROTOTYPE_OUTPUT}"

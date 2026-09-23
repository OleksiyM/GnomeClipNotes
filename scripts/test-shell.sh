#!/usr/bin/env bash
set -euo pipefail

project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)

if [[ "${1:-}" != "--inside" ]]; then
    if [[ "${1:-}" == "--filters" ]]; then export GCN_FILTER_ONLY=1; fi
    test_dir=$(mktemp -d /tmp/gnome-clip-notes-shell.XXXXXX)
    export GCN_PROJECT_DIR="${project_dir}"
    export GCN_SHELL_TEST_DIR="${test_dir}"
    export XDG_CONFIG_HOME="${test_dir}/config"
    export XDG_DATA_HOME="${test_dir}/data"
    export XDG_CACHE_HOME="${test_dir}/cache"
    export XDG_RUNTIME_DIR="${test_dir}/runtime"
    export GSETTINGS_BACKEND=dconf
    export XDG_SESSION_TYPE=wayland
    export WAYLAND_DISPLAY=gcn-test
    export GTK_A11Y=none
    export GDK_BACKEND=wayland
    export LANGUAGE=en LANG=C.UTF-8
    mkdir -m 700 -p "${XDG_CONFIG_HOME}" "${XDG_DATA_HOME}" "${XDG_CACHE_HOME}" "${XDG_RUNTIME_DIR}"
    printf 'GNOME Shell test logs: %s\n' "${test_dir}"
    if timeout --foreground 90s dbus-run-session -- bash "${BASH_SOURCE[0]}" --inside 2>"${test_dir}/session.log"; then
        printf 'PASS: isolated GNOME Shell integration\nLogs: %s\n' "${test_dir}"
    else
        status=$?
        printf 'FAIL: isolated GNOME Shell integration (status %s)\nLogs: %s\n' "${status}" "${test_dir}" >&2
        exit "${status}"
    fi
    exit 0
fi

shell_pid=''
app_pid=''
stage='setup'

cleanup() {
    status=$?
    trap - EXIT INT TERM
    if [[ -n "${app_pid}" ]] && kill -0 "${app_pid}" 2>/dev/null; then
        kill "${app_pid}" 2>/dev/null || true
        wait "${app_pid}" 2>/dev/null || true
    fi
    if [[ -n "${shell_pid}" ]] && kill -0 "${shell_pid}" 2>/dev/null; then
        kill "${shell_pid}" 2>/dev/null || true
        wait "${shell_pid}" 2>/dev/null || true
    fi
    if (( status != 0 )); then
        printf 'Failed stage: %s\nLogs: %s\n' "${stage}" "${GCN_SHELL_TEST_DIR}" >&2
    fi
    exit "${status}"
}
trap cleanup EXIT INT TERM

step() {
    stage=$1
    printf '[%s] %s\n' "$(date +%H:%M:%S)" "${stage}"
}

wait_for() {
    local description=$1
    shift
    local attempt
    for attempt in $(seq 1 200); do
        if "$@"; then return 0; fi
        sleep 0.1
    done
    printf 'Timed out waiting for %s\n' "${description}" >&2
    return 1
}

step 'preflight'
test -x "${GCN_PROJECT_DIR}/target/debug/gnome-clip-notes"
command -v gnome-shell >/dev/null
command -v gjs >/dev/null
gjs -m "${GCN_PROJECT_DIR}/tests/date-range.js"
TZ=America/New_York gjs -m "${GCN_PROJECT_DIR}/tests/date-range.js"
gjs -m "${GCN_PROJECT_DIR}/tests/i18n.js"
gjs -m "${GCN_PROJECT_DIR}/tests/clipboard-formats.js"

extension_dir="${XDG_DATA_HOME}/gnome-shell/extensions/gnome-clip-notes@oleksiym.github.io"
mkdir -p "${extension_dir}" "${XDG_DATA_HOME}/applications" "${XDG_DATA_HOME}/dbus-1/services"
printf '[D-BUS Service]\nName=io.github.OleksiyM.GnomeClipNotes\nExec="%s" --daemon\n' "${GCN_PROJECT_DIR}/target/debug/gnome-clip-notes" >"${XDG_DATA_HOME}/dbus-1/services/io.github.OleksiyM.GnomeClipNotes.service"
cp -a "${GCN_PROJECT_DIR}/extension/." "${extension_dir}/"
mkdir -p "${XDG_DATA_HOME}/gnome-shell/extensions/gcn-test-driver@local"
cp -a "${GCN_PROJECT_DIR}/tests/driver/." "${XDG_DATA_HOME}/gnome-shell/extensions/gcn-test-driver@local/"
glib-compile-schemas "${extension_dir}/schemas"
printf '%s\n' \
    '[Desktop Entry]' \
    'Type=Application' \
    'Name=Shell Integration Test' \
    'Exec=/bin/false' \
    'NoDisplay=true' \
    >"${XDG_DATA_HOME}/applications/org.example.ClipNotesShellTest.desktop"
gsettings set org.gnome.shell enabled-extensions "['gnome-clip-notes@oleksiym.github.io', 'gcn-test-driver@local']"
# This test session has already opted to reserve Super+V for ClipNotes.
# Consent and preservation of other calendar bindings are tested by GTK smoke.
gsettings set org.gnome.shell.keybindings toggle-message-tray "['<Super>m']"

gsettings set org.gnome.desktop.input-sources sources "[('xkb', 'us'), ('xkb', 'ru'), ('xkb', 'ua')]"

step 'start GNOME Shell 50 headless compositor'
gnome-shell --headless --wayland --no-x11 \
    --virtual-monitor 1440x900 --wayland-display "${WAYLAND_DISPLAY}" \
    >"${GCN_SHELL_TEST_DIR}/gnome-shell.log" 2>&1 &
shell_pid=$!

socket_ready() { [[ -S "${XDG_RUNTIME_DIR}/${WAYLAND_DISPLAY}" ]] && kill -0 "${shell_pid}" 2>/dev/null; }
shell_bus_ready() {
    kill -0 "${shell_pid}" 2>/dev/null &&
        gdbus call --session --dest org.gnome.Shell --object-path /org/gnome/Shell \
            --method org.freedesktop.DBus.Peer.Ping >/dev/null 2>&1
}
wait_for 'Wayland socket' socket_ready
wait_for 'org.gnome.Shell D-Bus name' shell_bus_ready

bridge_ready() {
    gdbus call --session --dest org.gnome.Shell \
        --object-path /io/github/OleksiyM/GnomeClipNotes/Bridge \
        --method io.github.OleksiyM.GnomeClipNotes.Bridge.GetStatus >/dev/null 2>&1
}
wait_for 'extension loaded without starting daemon' bridge_ready
owner=$(gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus --method org.freedesktop.DBus.NameHasOwner io.github.OleksiyM.GnomeClipNotes)
[[ "${owner}" == '(false,)' ]] || { printf 'Extension unexpectedly started daemon\n' >&2; exit 1; }
printf 'PASS: extension respects disabled autostart\n'

step 'start GnomeClipNotes service'
"${GCN_PROJECT_DIR}/target/debug/gnome-clip-notes" --daemon \
    >"${GCN_SHELL_TEST_DIR}/service.log" 2>&1 &
app_pid=$!
service_ready() {
    # A readiness probe must not race the explicit process by auto-activating
    # a second daemon before the first has acquired its bus name.
    local owner
    owner=$(gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus \
        --method org.freedesktop.DBus.NameHasOwner io.github.OleksiyM.GnomeClipNotes) || return 1
    [[ "${owner}" == '(true,)' ]] || return 1
    kill -0 "${app_pid}" 2>/dev/null &&
        gdbus call --session --dest io.github.OleksiyM.GnomeClipNotes \
            --object-path /io/github/OleksiyM/GnomeClipNotes --method io.github.OleksiyM.GnomeClipNotes.Service.Query '{"metadata_only":true}' >/dev/null 2>&1
}
wait_for 'io.github.OleksiyM.GnomeClipNotes D-Bus name' service_ready

step 'wait for extension bridge and privacy settings cache'
bridge_ready() {
    gdbus call --session --dest org.gnome.Shell \
        --object-path /io/github/OleksiyM/GnomeClipNotes/Bridge \
        --method io.github.OleksiyM.GnomeClipNotes.Bridge.GetStatus >/dev/null 2>&1
}
wait_for 'GnomeClipNotes Shell bridge' bridge_ready
sleep 0.3

if [[ "${GCN_FILTER_ONLY:-}" != 1 ]]; then
step 'run clipboard, overlay and active-paste assertions'
gjs -m "${GCN_PROJECT_DIR}/tests/shell-client.js" \
    >"${GCN_SHELL_TEST_DIR}/client.log" 2>&1 || {
        tail -80 "${GCN_SHELL_TEST_DIR}/client.log" >&2
        exit 1
    }

step 'default launcher opens clipboard overlay'
"${GCN_PROJECT_DIR}/target/debug/gnome-clip-notes"
overlay_open() {
    gdbus call --session --dest org.gnome.Shell --object-path /io/github/OleksiyM/GnomeClipNotes/Bridge \
        --method io.github.OleksiyM.GnomeClipNotes.Bridge.GetStatus | grep -q '"overlay_visible":true'
}
wait_for 'default launcher overlay' overlay_open
gdbus call --session --dest org.gnome.Shell --object-path /io/github/OleksiyM/GnomeClipNotes/Bridge \
    --method io.github.OleksiyM.GnomeClipNotes.Bridge.Toggle >/dev/null
printf 'PASS: default launcher opens clipboard overlay\n'
fi

step 'library live filters and outside click'
"${GCN_PROJECT_DIR}/target/debug/gnome-clip-notes" --test-library-filters
filters_finished() { [[ -f "${GCN_SHELL_TEST_DIR}/filters-passed" || -f "${GCN_SHELL_TEST_DIR}/filters-failed" ]]; }
wait_for 'library filter regression' filters_finished
if [[ -f "${GCN_SHELL_TEST_DIR}/filters-failed" ]]; then
    cat "${GCN_SHELL_TEST_DIR}/filters-failed" >&2
    exit 1
fi
printf 'PASS: library filter regression\n'

step 'check child processes remained healthy'
kill -0 "${shell_pid}"
kill -0 "${app_pid}"
step 'stop daemon and restart through explicit overlay action'
"${GCN_PROJECT_DIR}/target/debug/gnome-clip-notes" --quit
wait "${app_pid}"
app_pid=''
sleep 0.3
gdbus call --session --dest org.gnome.Shell --object-path /io/github/OleksiyM/GnomeClipNotes/Bridge --method io.github.OleksiyM.GnomeClipNotes.Bridge.Toggle >/dev/null
restarted() { [[ $(gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus --method org.freedesktop.DBus.NameHasOwner io.github.OleksiyM.GnomeClipNotes) == '(true,)' ]]; }
wait_for 'explicit D-Bus activation' restarted
"${GCN_PROJECT_DIR}/target/debug/gnome-clip-notes" --quit
printf 'PASS: explicit action restarts daemon\n'

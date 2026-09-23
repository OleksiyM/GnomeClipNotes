#!/usr/bin/env bash
set -euo pipefail
project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
if [[ "${1:-}" != --inside ]]; then
    test_dir=$(mktemp -d /tmp/gnome-clip-notes-editor-test.XXXXXX)
    export GCN_EDITOR_TEST_DIR="${test_dir}" GCN_EDITOR_PROJECT="${project_dir}"
    export GCN_TEST_SYSTEMD_BUS="${DBUS_SESSION_BUS_ADDRESS:-unix:path=${XDG_RUNTIME_DIR}/bus}"
    export GCN_TEST_SYSTEMD_RUNTIME="${XDG_RUNTIME_DIR}"
    export XDG_DATA_HOME="${test_dir}/data" XDG_CONFIG_HOME="${test_dir}/config"
    export XDG_CACHE_HOME="${test_dir}/cache" XDG_RUNTIME_DIR="${test_dir}/runtime"
    mkdir -m 700 -p "${XDG_RUNTIME_DIR}" "${XDG_DATA_HOME}" "${XDG_CONFIG_HOME}" "${XDG_CACHE_HOME}"
    export GSETTINGS_BACKEND=memory GTK_A11Y=none GDK_BACKEND=wayland WAYLAND_DISPLAY=gcn-editor-test
    export LANGUAGE=en LANG=C.UTF-8
    unset DISPLAY
    printf 'Editor process test: %s\n' "${test_dir}"
    timeout --foreground 90s dbus-run-session -- bash "$0" --inside "${1:-}" 2>"${test_dir}/session.log"
    exit
fi
control() {
    DBUS_SESSION_BUS_ADDRESS="${GCN_TEST_SYSTEMD_BUS}" XDG_RUNTIME_DIR="${GCN_TEST_SYSTEMD_RUNTIME}" systemctl --user "$@"
}
app_pid=''
shell_pid=''
unit=''
cleanup() {
    [[ -z "${unit}" ]] || control stop "${unit}" 2>/dev/null || true
    [[ -z "${app_pid}" ]] || kill "${app_pid}" 2>/dev/null || true
    [[ -z "${shell_pid}" ]] || kill "${shell_pid}" 2>/dev/null || true
    wait 2>/dev/null || true
}
trap cleanup EXIT
gnome-shell --headless --wayland --no-x11 --virtual-monitor 1440x900 \
    --wayland-display "${WAYLAND_DISPLAY}" >"${GCN_EDITOR_TEST_DIR}/shell.log" 2>&1 &
shell_pid=$!
for _ in {1..100}; do
    [[ -S "${XDG_RUNTIME_DIR}/${WAYLAND_DISPLAY}" ]] && break
    sleep .1
done
[[ -S "${XDG_RUNTIME_DIR}/${WAYLAND_DISPLAY}" ]]
gdbus call --session --dest org.freedesktop.portal.Desktop --object-path /org/freedesktop/portal/desktop \
    --method org.freedesktop.DBus.Peer.Ping >/dev/null
"${GCN_EDITOR_PROJECT}/target/debug/gnome-clip-notes" --daemon >"${GCN_EDITOR_TEST_DIR}/app.log" 2>&1 &
app_pid=$!
for _ in {1..100}; do
    if gdbus call --session --dest io.github.OleksiyM.GnomeClipNotes --object-path /io/github/OleksiyM/GnomeClipNotes \
        --method io.github.OleksiyM.GnomeClipNotes.Service.Query '{}' >/dev/null 2>&1; then break; fi
    sleep .1
done
unit="gnome-clip-notes-editor-${app_pid}-1.service"
if [[ "${2:-}" == --view ]]; then
    gdbus call --session --dest io.github.OleksiyM.GnomeClipNotes --object-path /io/github/OleksiyM/GnomeClipNotes \
        --method io.github.OleksiyM.GnomeClipNotes.Service.Capture '# View fixture' 'Isolated Full preview test' '' false >/dev/null
    gdbus call --session --dest io.github.OleksiyM.GnomeClipNotes --object-path /io/github/OleksiyM/GnomeClipNotes \
        --method io.github.OleksiyM.GnomeClipNotes.Service.GetItem 1 | rg -q 'View fixture'
    gdbus call --session --dest io.github.OleksiyM.GnomeClipNotes --object-path /io/github/OleksiyM/GnomeClipNotes \
        --method io.github.OleksiyM.GnomeClipNotes.Service.Activate view 1 >/dev/null
else
gdbus call --session --dest io.github.OleksiyM.GnomeClipNotes --object-path /io/github/OleksiyM/GnomeClipNotes \
    --method io.github.OleksiyM.GnomeClipNotes.Service.Activate new-note 0 >/dev/null
fi
group=''
for _ in {1..100}; do
    group=$(control show "${unit}" -p ControlGroup --value 2>/dev/null || true)
    [[ -n "${group}" && -f "/sys/fs/cgroup${group}/cgroup.procs" ]] && break
    sleep .1
done
[[ "${group}" == /user.slice/*"${unit}" ]]
control show "${unit}" -p Type -p KillMode -p TimeoutStopUSec -p ControlGroup >"${GCN_EDITOR_TEST_DIR}/unit.txt"
for _ in {1..120}; do
    if [[ -f "/sys/fs/cgroup${group}/cgroup.procs" ]]; then
        cat "/sys/fs/cgroup${group}/cgroup.procs" >>"${GCN_EDITOR_TEST_DIR}/pids"
    else
        break
    fi
    sleep .25
done
if [[ ! -f "${GCN_EDITOR_TEST_DIR}/passed" ]]; then
    [[ ! -f "${GCN_EDITOR_TEST_DIR}/failed" ]] || cat "${GCN_EDITOR_TEST_DIR}/failed" >&2
    exit 1
fi
[[ ! -d "/sys/fs/cgroup${group}" ]]
for _ in {1..100}; do
    update_status=$(gdbus call --session --dest io.github.OleksiyM.GnomeClipNotes \
        --object-path /io/github/OleksiyM/GnomeClipNotes \
        --method io.github.OleksiyM.GnomeClipNotes.Service.GetUpdateStatus)
    [[ "${update_status}" == *'"full_editors":0'* ]] && break
    sleep .1
done
[[ "${update_status}" == *'"full_editors":0'* ]]
while read -r pid; do
    if kill -0 "${pid}" 2>/dev/null; then
        printf 'Process %s survived editor shutdown\n' "${pid}" >&2
        exit 1
    fi
done < <(sort -nu "${GCN_EDITOR_TEST_DIR}/pids")
item_id=$(<"${GCN_EDITOR_TEST_DIR}/saved-id")
gdbus call --session --dest io.github.OleksiyM.GnomeClipNotes --object-path /io/github/OleksiyM/GnomeClipNotes \
    --method io.github.OleksiyM.GnomeClipNotes.Service.GetItem "${item_id}" >"${GCN_EDITOR_TEST_DIR}/saved-item.txt"
rg -q 'Isolated Full preview test' "${GCN_EDITOR_TEST_DIR}/saved-item.txt"
kill -0 "${app_pid}"
printf 'EDITOR_PROCESS_OK: saved note, unchanged draft across toggles, discard confirmation, no surviving cgroup processes\n'

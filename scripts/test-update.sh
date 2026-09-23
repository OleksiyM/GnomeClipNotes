#!/usr/bin/env bash
set -euo pipefail

project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
if [[ "${1:-}" != --inside ]]; then
test_dir=$(mktemp -d /tmp/gnome-clip-notes-update.XXXXXX)
export GCN_UPDATE_TEST_DIR="${test_dir}"
export XDG_DATA_HOME="${test_dir}/data" XDG_CONFIG_HOME="${test_dir}/config"
export XDG_CACHE_HOME="${test_dir}/cache" XDG_RUNTIME_DIR="${test_dir}/runtime"
mkdir -m 700 -p "${XDG_DATA_HOME}" "${XDG_CONFIG_HOME}" "${XDG_CACHE_HOME}" "${XDG_RUNTIME_DIR}"
export GSETTINGS_BACKEND=memory GTK_A11Y=none LANGUAGE=en LANG=C.UTF-8
export GDK_BACKEND=x11 GSK_RENDERER=cairo
unset WAYLAND_DISPLAY

printf 'Update protocol test: %s\n' "${test_dir}"
timeout --foreground 90s xvfb-run -a -s '-screen 0 1600x1000x24' \
    dbus-run-session -- bash "${BASH_SOURCE[0]}" --inside \
    2>"${test_dir}/session.log"
exit
fi

test_dir=${GCN_UPDATE_TEST_DIR:?Private update test profile required}
[[ "${test_dir}" == /tmp/gnome-clip-notes-update.* ]]
[[ "${XDG_DATA_HOME}" == "${test_dir}/data" ]]
[[ "${XDG_CONFIG_HOME}" == "${test_dir}/config" ]]
[[ "${XDG_RUNTIME_DIR}" == "${test_dir}/runtime" ]]

app_pid=''
cleanup() {
    if [[ -n "${app_pid}" ]]; then
        kill "${app_pid}" 2>/dev/null || true
        wait "${app_pid}" 2>/dev/null || true
    fi
    exec 9>&- || true
}
trap cleanup EXIT

"${project_dir}/target/debug/gnome-clip-notes" --test-update >"${test_dir}/app.log" 2>&1 &
app_pid=$!
for _ in {1..100}; do
    [[ ! -f "${test_dir}/ready" ]] || break
    kill -0 "${app_pid}"
    sleep .1
done
[[ -f "${test_dir}/ready" ]]
[[ ! -f "${test_dir}/failed" ]] || { cat "${test_dir}/failed" >&2; exit 1; }
lock_path="${XDG_DATA_HOME}/gnome-clip-notes/update.lock"
if flock -n -x "${lock_path}" true; then
    printf 'Exclusive lock acquired while the daemon was still running\n' >&2
    exit 1
fi
reply=$(gdbus call --session --dest io.github.OleksiyM.GnomeClipNotes \
    --object-path /io/github/OleksiyM/GnomeClipNotes \
    --method io.github.OleksiyM.GnomeClipNotes.Service.QuitForUpdate)
[[ "${reply}" == '(true,)' ]]
wait "${app_pid}"
app_pid=''

exec 9<>"${lock_path}"
flock -n -x 9
database="${XDG_DATA_HOME}/gnome-clip-notes/data.db"
before=$(sha256sum "${database}")

if timeout 5s "${project_dir}/target/debug/gnome-clip-notes" --daemon >"${test_dir}/locked-main.log" 2>&1; then
    printf 'Main application started while the exclusive update lock was held\n' >&2
    exit 1
fi
rg -q 'Cannot start GnomeClipNotes. An update may be in progress.' "${test_dir}/locked-main.log"
if timeout 5s "${project_dir}/target/debug/gnome-clip-notes-editor" \
    --instance io.github.OleksiyM.GnomeClipNotes.Editor.UpdateLockTest \
    >"${test_dir}/locked-editor.log" 2>&1; then
    printf 'Editor helper started while the exclusive update lock was held\n' >&2
    exit 1
fi
rg -q 'Cannot open the editor. An update may be in progress.' "${test_dir}/locked-editor.log"
backup="${test_dir}/locked-backup.db"
if timeout 5s "${project_dir}/target/debug/gnome-clip-notes" --backup "${backup}" \
    >"${test_dir}/locked-backup.log" 2>&1; then
    printf 'Backup started while the exclusive update lock was held\n' >&2
    exit 1
fi
rg -q 'operation would block' "${test_dir}/locked-backup.log"
[[ ! -e "${backup}" ]]
after=$(sha256sum "${database}")
[[ "${before}" == "${after}" ]]

printf 'UPDATE_OK: editor-aware quit protocol and exclusive launch lock verified\n'

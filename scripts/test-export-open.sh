#!/usr/bin/env bash
# Private file-manager substitute for the export GTK regression test.
set -euo pipefail
case ${GCN_SMOKE_DIR:-} in /tmp/gnome-clip-notes-test.*) ;; *) exit 64 ;; esac
[[ ${XDG_DATA_HOME:-} == "${GCN_SMOKE_DIR}/data" ]] || exit 64
printf '%s\n' "$1" > "${GCN_SMOKE_DIR}/folder-opened-uri"

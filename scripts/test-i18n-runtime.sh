#!/usr/bin/env bash
set -euo pipefail

project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
test_root=$(mktemp -d /tmp/gnome-clip-notes-i18n.XXXXXX)
config_dir="${test_root}/config/gnome-clip-notes"
binary="${project_dir}/target/debug/gnome-clip-notes"
mkdir -m 700 -p -- "${config_dir}" "${test_root}/locale"

printf 'i18n runtime test: %s\n' "${test_root}"
"${project_dir}/scripts/translations.py" pseudo --output "${test_root}/locale"
cargo build --manifest-path "${project_dir}/Cargo.toml" --locked

write_language() {
    local language=$1
    printf '{"language":"%s"}\n' "${language}" > "${config_dir}/settings.json"
}

clear_settings() {
    : > "${config_dir}/settings.json"
}

probe() {
    local name=$1 language_mode=$2
    shift 2
    if [[ "${language_mode}" == unset ]]; then
        env -u LANGUAGE GCN_I18N_TEST_DIR="${test_root}" XDG_CONFIG_HOME="${test_root}/config" \
            LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8 "$@" "${binary}" --i18n-probe > "${test_root}/${name}.json"
    else
        env GCN_I18N_TEST_DIR="${test_root}" XDG_CONFIG_HOME="${test_root}/config" \
            LANGUAGE="${language_mode}" LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8 "$@" "${binary}" --i18n-probe > "${test_root}/${name}.json"
    fi
}

clear_settings
probe system-pseudo en_XA

write_language en
probe explicit-english en_XA

write_language unknown-language
probe unknown-english en_XA

clear_settings
probe system-unsupported unset LANG=zz_ZZ.UTF-8 LC_ALL=zz_ZZ.UTF-8
probe system-c en_XA LC_ALL=C.UTF-8

python3 - "${test_root}" <<'PY'
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])

def load(name):
    with (root / f"{name}.json").open(encoding="utf-8") as stream:
        value = json.load(stream)
    required = {"language", "simple", "plural_one", "plural_many", "shell"}
    if set(value) != required:
        raise SystemExit(f"{name}: unexpected probe fields: {sorted(value)}")
    shell = value["shell"]
    if shell.get("language") != value["language"]:
        raise SystemExit(f"{name}: Shell and Rust language policies differ")
    messages = shell.get("messages")
    if not isinstance(messages, dict) or messages.get("New Note") != value["simple"]:
        raise SystemExit(f"{name}: Shell and Rust do not share the same translation")
    return value

def english(name, language):
    value = load(name)
    expected = (language, "New Note", "1 item", "2 items")
    actual = (value["language"], value["simple"], value["plural_one"], value["plural_many"])
    if actual != expected:
        raise SystemExit(f"{name}: expected English {expected!r}, got {actual!r}")
    if any(translated != source for source, translated in value["shell"]["messages"].items()):
        raise SystemExit(f"{name}: Shell catalog is not English")

pseudo = load("system-pseudo")
if pseudo["language"] != "system":
    raise SystemExit(f"system-pseudo: unexpected language {pseudo['language']!r}")
if (pseudo["simple"], pseudo["plural_one"], pseudo["plural_many"]) != (
        "[!! Neew Nootee !!]", "[!! 1 iiteem !!]", "[!! 2 iiteems !!]"):
    raise SystemExit(f"system-pseudo: pseudolocale or plural selection failed: {pseudo!r}")
if not all(text.startswith("[!! ") and text.endswith(" !!]")
           for text in pseudo["shell"]["messages"].values()):
    raise SystemExit("system-pseudo: Shell catalog contains untranslated messages")

english("explicit-english", "en")
english("unknown-english", "en")
english("system-unsupported", "system")
english("system-c", "system")
PY

printf 'PASS: isolated runtime language policy\nDiagnostics retained: %s\n' "${test_root}"

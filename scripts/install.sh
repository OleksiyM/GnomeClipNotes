#!/usr/bin/env bash
set -euo pipefail

# Trusted-local-source wrapper. Network/bootstrap policy belongs to the
# release installer; this entry point only builds and installs this checkout.
project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
dist_dir="${project_dir}/dist"
if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    exec python3 "${project_dir}/scripts/install-release.py" --help
fi

stage_dir=$(mktemp -d)
trap 'rm -rf -- "${stage_dir}"' EXIT

"${project_dir}/scripts/build-package.sh"

archive=$(PROJECT_DIR="${project_dir}" DIST_DIR="${dist_dir}" python3 - <<'PY'
import importlib.util
import os
from pathlib import Path
import tomllib

project = Path(os.environ["PROJECT_DIR"])
spec = importlib.util.spec_from_file_location("package_release", project / "scripts/package-release.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
with (project / "Cargo.toml").open("rb") as stream:
    version = tomllib.load(stream)["package"]["version"]
_, _, arch = module.platform_labels()
print(Path(os.environ["DIST_DIR"]) / f"gnome-clip-notes-{version}-{arch}.tar.gz")
PY
)
[[ -f "${archive}" ]] || { printf 'Local package was not produced: %s\n' "${archive}" >&2; exit 1; }

tar -xzf "${archive}" --no-same-owner -C "${stage_dir}"
package_dir=$(find "${stage_dir}" -mindepth 1 -maxdepth 1 -type d -print -quit)
[[ -n "${package_dir}" ]] || { printf 'Local package has no top-level directory\n' >&2; exit 1; }
python3 "${package_dir}/scripts/install-release.py" --package-dir "${package_dir}" "$@"

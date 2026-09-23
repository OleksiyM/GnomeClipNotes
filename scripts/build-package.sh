#!/usr/bin/env bash
set -euo pipefail

project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
dist_dir="${project_dir}/dist"
python3 -m unittest discover -s "${project_dir}/tests" -p 'test_*release.py'
python3 -m unittest discover -s "${project_dir}/tests" -p 'test_bootstrap.py'
cargo test --manifest-path "${project_dir}/Cargo.toml" --locked
cargo build --manifest-path "${project_dir}/Cargo.toml" --release --locked
"${project_dir}/scripts/translations.py" check
"${project_dir}/scripts/translations.py" build --output "${project_dir}/target/locales"
python3 "${project_dir}/scripts/package-release.py" --project-dir "${project_dir}" --output-dir "${dist_dir}"

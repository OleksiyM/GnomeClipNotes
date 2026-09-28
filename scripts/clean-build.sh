#!/usr/bin/env bash
# Local build outputs only. Run between builds, never alongside Cargo or packaging.
set -euo pipefail

usage() {
    printf '%s\n' \
        'Usage: bash scripts/clean-build.sh [--full] [--apply]' \
        'Default: show debug incremental cache and archived prototype build outputs.' \
        '--full:  also remove all project target/ outputs and dist/ packages.' \
        '--apply: delete the listed directories; without it, only report sizes.' \
        'Stop builds/tests first. Deleted outputs must be regenerated.' \
        'Source, .private, Git, installed apps and shared Cargo caches are untouched.'
}

full=no
apply=no
for argument in "$@"; do
    case "$argument" in
        --full) full=yes ;;
        --apply) apply=yes ;;
        -h|--help) usage; exit 0 ;;
        *) printf 'Unknown option: %s\n' "$argument" >&2; usage >&2; exit 2 ;;
    esac
done

project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
if [[ ! -f "$project_dir/Cargo.toml" ]] ||
   [[ "$(git -C "$project_dir" rev-parse --show-toplevel)" != "$project_dir" ]]; then
    printf 'Run this script from its GnomeClipNotes Git checkout.\n' >&2
    exit 1
fi

targets=(target/debug/incremental prototypes/webkit-preview/target)
if [[ "$full" == yes ]]; then
    targets=(target dist prototypes/webkit-preview/target)
fi

# Validate every target before deleting anything. Refuse redirected parent paths
# and tracked files; never derive deletion targets from user input or globs.
for relative in "${targets[@]}"; do
    path="$project_dir"
    IFS=/ read -r -a components <<< "$relative"
    for component in "${components[@]}"; do
        path="$path/$component"
        if [[ -L "$path" || ( -e "$path" && ! -d "$path" ) ]]; then
            printf 'Refusing non-directory or symlink: %s\n' "$path" >&2
            exit 1
        fi
    done
    tracked_files=$(git -C "$project_dir" ls-files -- "$relative")
    if [[ -n "$tracked_files" ]]; then
        printf 'Refusing directory containing tracked files: %s\n' "$path" >&2
        exit 1
    fi
done

printf 'Build cleanup in %s\n' "$project_dir"
for relative in "${targets[@]}"; do
    path="$project_dir/$relative"
    if [[ -d "$path" ]]; then
        du -sh -- "$path"
    fi
done
if [[ "$apply" != yes ]]; then
    printf 'Preview only. Add --apply to delete these outputs; stop builds/tests first.\n'
    exit 0
fi

for relative in "${targets[@]}"; do
    path="$project_dir/$relative"
    if [[ -d "$path" ]]; then
        rm -r -- "$path"
        printf 'Removed: %s\n' "$relative"
    fi
done
printf 'Done. Outputs are not backed up; rebuild to regenerate them.\n'

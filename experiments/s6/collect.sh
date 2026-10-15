#!/usr/bin/env bash
set -euo pipefail
root="${1:?fixture root required}"
destination="${2:?new evidence directory required}"
[[ ! -e "$destination" ]] || { echo 'Use a new evidence directory.' >&2; exit 1; }
mkdir -p "$destination"
# Deliberately only top-level harness logs; never traverse caches or fixtures.
for source in "$root" "$root.build"; do
  [[ -d "$source" ]] || continue
  mkdir -p "$destination/$(basename "$source")"
  for file in "$source"/command-*.log "$source"/*-result.log "$source"/*-constraint.log "$source"/provenance.log "$source"/sources.sha256 "$source"/format.log "$source"/compile-unit.log "$source"/unit.log "$source"/compile.log "$source"/run.log "$source"/tool-setup.log; do
    [[ -f "$file" ]] || continue
    cp "$file" "$destination/$(basename "$source")/"
  done
done

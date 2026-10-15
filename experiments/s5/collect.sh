#!/usr/bin/env bash
# Explicit allowlist: never recursively collect logs from synthetic Git repositories.
set -euo pipefail
root="${1:?fixture root required}"
destination="${2:?new evidence directory required}"
[[ ! -e "$destination" ]] || { echo 'Evidence destination must be new.' >&2; exit 1; }
mkdir -p "$destination"
for name in provenance.log sources.sha256 format.log check.log compile-unit.log unit.log compile.log run.log; do
  [[ -f "$root.build/$name" && ! -L "$root.build/$name" ]] || continue
  mkdir -p "$destination/build"
  cp "$root.build/$name" "$destination/build/$name"
done
if [[ -f "$root/commands.log" && ! -L "$root/commands.log" ]]; then
  cp "$root/commands.log" "$destination/commands.log"
fi

#!/usr/bin/env bash
# Explicit allowlist: never upload repositories, SQLite stores, caches or binaries.
set -euo pipefail
root="${1:?fixture root required}"
destination="${2:?new evidence directory required}"
[[ ! -e "$destination" ]] || { echo 'Choose a new evidence directory' >&2; exit 1; }
mkdir -p "$destination"
for name in provenance.log sources.sha256 format.log unit.log clippy.log compile.log run.log; do
  [[ ! -f "$root.build/$name" || -L "$root.build/$name" || -L "$root.build" ]] || cp "$root.build/$name" "$destination/$name"
done
[[ ! -f "$root/summary.log" || -L "$root/summary.log" || -L "$root" ]] || cp "$root/summary.log" "$destination/summary.log"
for case in "$root"/*; do
  [[ -d "$case" && ! -L "$case" && ! -L "$root" ]] || continue
  for name in commands.log worker.log retry.log; do
    [[ -f "$case/$name" && ! -L "$case/$name" ]] || continue
    mkdir -p "$destination/$(basename "$case")"
    cp "$case/$name" "$destination/$(basename "$case")/$name"
  done
done

#!/usr/bin/env bash
# Exact generated-path allowlist: no source checkouts or arbitrary recursive logs.
set -euo pipefail
root="${1:?fixture root required}"
destination="${2:?new evidence directory required}"
if [[ -e "$destination" ]]; then
  echo 'Choose a new evidence directory; existing evidence is never overwritten.' >&2
  exit 1
fi
mkdir -p "$destination"
copy_file() {
  [[ -f "$1" && ! -L "$1" ]] || return 0
  mkdir -p "$destination/$2"
  cp "$1" "$destination/$2/$(basename "$1")"
}
for name in provenance.log sources.sha256 format.log compile-unit.log unit.log compile.log run.log workload.log; do
  copy_file "$root.build/$name" build
done
for name in metrics.log recovery.log input.log policy.log; do
  copy_file "$root/$name" results
done
shopt -s nullglob
for file in "$root"/s[0-9][0-9][0-9].log; do copy_file "$file" results; done

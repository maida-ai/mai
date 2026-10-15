#!/usr/bin/env bash
set -euo pipefail
root="${1:?fixture root required}"
destination="${2:?new evidence directory required}"
[[ ! -e "$destination" ]] || { echo 'Choose a new evidence directory.' >&2; exit 1; }
mkdir -p "$destination"
for name in provenance.log sources.sha256 format.log compile-unit.log unit.log compile.log run.log; do
  [[ ! -f "$root.build/$name" ]] || cp "$root.build/$name" "$destination/$name"
done
[[ ! -f "$root/scenarios.log" ]] || cp "$root/scenarios.log" "$destination/scenarios.log"

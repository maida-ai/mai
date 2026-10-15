#!/usr/bin/env bash
# Copy only top-level harness logs/samples, never fixture or dependency logs.
set -euo pipefail
root="${1:?fixture root required}"
destination="${2:?new evidence directory required}"
[[ ! -e "$destination" ]] || { echo 'Choose a new evidence directory.' >&2; exit 1; }
mkdir -p "$destination"
for source in "$root" "$root.build"; do
  [[ -d "$source" ]] || continue
  mkdir -p "$destination/$(basename "$source")"
  while IFS= read -r -d '' file; do
    cp "$file" "$destination/$(basename "$source")/$(basename "$file")"
  done < <(find "$source" -maxdepth 1 -type f \( -name '*.log' -o -name '*-samples.tsv' -o -name sources.sha256 \) -print0)
done

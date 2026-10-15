#!/usr/bin/env bash
# Copy only generated logs and source hashes, including after a failed run.
set -euo pipefail
root="${1:?fixture root required}"
destination="${2:?new evidence directory required}"
if [[ -e "$destination" ]]; then
  echo 'Choose a new evidence directory; existing evidence is never overwritten.' >&2
  exit 1
fi
mkdir -p "$destination"
for source in "$root" "$root.build"; do
  [[ -d "$source" ]] || continue
  while IFS= read -r -d '' file; do
    relative="${file#"$source"/}"
    target="$destination/$(basename "$source")/$relative"
    mkdir -p "$(dirname "$target")"
    cp "$file" "$target"
  done < <(find "$source" -type f \( -name '*.log' -o -name 'sources.sha256' \) -print0)
done

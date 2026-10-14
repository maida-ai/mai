#!/usr/bin/env bash
# Bounded local matrix; set FULL=1 to include 1024 workspaces. Override WORKERS
# to test actual simultaneous fanout instead of the default bounded worker pool.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${1:-$ROOT/target/benchmark-results}"
mkdir -p "$OUT"
cargo build --manifest-path "$ROOT/Cargo.toml" --release --locked --bins
status=0
counts=(1 16 64 256)
if [[ "${FULL:-0}" == 1 ]]; then counts+=(1024); fi
for n in "${counts[@]}"; do
  workers="${WORKERS:-16}"
  if (( workers > n )); then workers="$n"; fi
  for mode in disjoint contended; do
    hotspot=0
    if [[ "$mode" == contended ]]; then hotspot="$n"; fi
    args=("$ROOT/target/release/mai-bench" --agents "$n" --workers "$workers" --hotspot-agents "$hotspot" --cli "$ROOT/target/release/mai")
    if [[ -x /usr/bin/time ]] && /usr/bin/time --version >/dev/null 2>&1; then
      /usr/bin/time -v -o "$OUT/$n-$mode.resources" "${args[@]}" > "$OUT/$n-$mode.json" || status=1
    else
      "${args[@]}" > "$OUT/$n-$mode.json" || status=1
    fi
  done
done

exit "$status"

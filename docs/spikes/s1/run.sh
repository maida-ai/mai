#!/usr/bin/env bash
# Run from the mai repository root. Both paths must be new; nothing is cleaned.
set -euo pipefail
root="${1:-.throwaway/s1-$(date -u +%Y%m%dT%H%M%SZ)}"
build="${root}.build"
if [[ -e "$root" || -e "$build" ]]; then
  echo 'Choose a new output path; existing fixtures/builds are never overwritten.' >&2
  exit 1
fi
mkdir -p "$build"
rustfmt --edition 2024 --check docs/spikes/s1/main.rs docs/spikes/s1/scenarios.rs docs/spikes/s1/transport.rs
rustc --edition 2024 -D warnings --test docs/spikes/s1/main.rs -o "$build/unit"
"$build/unit" 2>&1 | tee "$build/unit.log"
rustc --edition 2024 -D warnings docs/spikes/s1/main.rs -o "$build/s1"
"$build/s1" run "$root" 2>&1 | tee "$build/run.log"

#!/usr/bin/env bash
# Deterministic live demonstration; requires a built Rust executable and Git 2.36+.
set -euo pipefail
HERE="$(cd "$(dirname "$0")/.." && pwd)"
MAI="${MAI_BIN:-$HERE/target/release/mai}"
if [[ ! -x "$MAI" ]]; then
  cargo build --manifest-path "$HERE/Cargo.toml" --release --locked --bin mai
fi
PROJECT="$(mktemp -d)/mai-demo"
# The disposable demo directory is retained for inspection.
"$MAI" init "$PROJECT"
"$MAI" workspace "$PROJECT" frontend
"$MAI" workspace "$PROJECT" backend
"$MAI" lease "$PROJECT" frontend web --ttl 120
"$MAI" lease "$PROJECT" backend api --ttl 120
mkdir -p "$PROJECT/workspaces/frontend/web" "$PROJECT/workspaces/backend/api"
printf 'index page\n' > "$PROJECT/workspaces/frontend/web/index.txt"
printf 'api handler\n' > "$PROJECT/workspaces/backend/api/server.txt"
"$MAI" --json submit "$PROJECT" frontend -m 'Add UI'
"$MAI" --json submit "$PROJECT" backend -m 'Add API'
"$MAI" history "$PROJECT"

printf "Demo retained at %s\n" "$PROJECT"

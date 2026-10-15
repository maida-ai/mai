#!/usr/bin/env bash
set -euo pipefail
root="${1:?new fixture root required}"
build="$root.build"
[[ ! -e "$root" && ! -e "$build" ]] || { echo 'Use new fixture and build paths.' >&2; exit 1; }
case "$root" in .throwaway/*) ;; *) echo 'Run from repository root using .throwaway/ paths.' >&2; exit 1;; esac
mkdir -p "$build"
suffix=''
case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) suffix='.exe';; esac
{
  date -u +%Y-%m-%dT%H:%M:%SZ
  git rev-parse HEAD
  git status --short -- experiments/s6 docs/spikes/s6 .github/workflows/experiment-s6.yml
  rustc -vV
  uname -srm
  printf 'runner_os=%s\nrunner_arch=%s\nimage_os=%s\nimage_version=%s\n' "${RUNNER_OS:-local}" "${RUNNER_ARCH:-unknown}" "${ImageOS:-unknown}" "${ImageVersion:-unknown}"
} > "$build/provenance.log" 2>&1
if command -v sha256sum >/dev/null; then sha256sum experiments/s6/*.rs experiments/s6/*.py experiments/s6/*.sh > "$build/sources.sha256"; else shasum -a 256 experiments/s6/*.rs experiments/s6/*.py experiments/s6/*.sh > "$build/sources.sha256"; fi
# A pinned uv bootstrap is isolated from the user's Python environment.
if ! command -v uv >/dev/null; then
  python -m venv "$build/tool-venv"
  if [[ -n "$suffix" ]]; then toolbin="$build/tool-venv/Scripts"; else toolbin="$build/tool-venv/bin"; fi
  "$toolbin/python" -m pip install --cache-dir "$build/pip-cache" 'uv==0.12.18' > "$build/tool-setup.log" 2>&1
  export PATH="$(cd "$toolbin" && pwd):$PATH"
fi
rustfmt --edition 2024 --check experiments/s6/main.rs 2>&1 | tee "$build/format.log"
rustc --edition 2024 -D warnings --test experiments/s6/main.rs -o "$build/unit$suffix" 2>&1 | tee "$build/compile-unit.log"
"$build/unit$suffix" 2>&1 | tee "$build/unit.log"
rustc --edition 2024 -D warnings experiments/s6/main.rs -o "$build/s6$suffix" 2>&1 | tee "$build/compile.log"
"$build/s6$suffix" "$root" 2>&1 | tee "$build/run.log"

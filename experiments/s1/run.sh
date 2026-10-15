#!/usr/bin/env bash
# Run from the repository root with Bash (Git Bash on native Windows).
set -euo pipefail
root="${1:-.throwaway/s1-$(date -u +%Y%m%dT%H%M%SZ)}"
build="${root}.build"
if [[ -e "$root" || -e "$build" ]]; then
  echo 'Choose a new output path; existing fixtures/builds are never overwritten.' >&2
  exit 1
fi
mkdir -p "$build"
suffix=''
case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) suffix='.exe';; esac
{
  date -u +%Y-%m-%dT%H:%M:%SZ
  git rev-parse HEAD
  git status --short -- experiments/s1 .github/workflows/experiment-s1.yml .github/workflows/evidence-experiment-runner.yml
  git --version
  rustc -vV
  uname -srm
  printf 'runner_os=%s\nrunner_arch=%s\nimage_os=%s\nimage_version=%s\nrun_id=%s\nrun_attempt=%s\n' \
    "${RUNNER_OS:-local}" "${RUNNER_ARCH:-unknown}" "${ImageOS:-unknown}" \
    "${ImageVersion:-unknown}" "${GITHUB_RUN_ID:-local}" "${GITHUB_RUN_ATTEMPT:-local}"
} > "$build/provenance.log" 2>&1
if command -v sha256sum >/dev/null; then
  sha256sum experiments/s1/*.rs experiments/s1/*.sh > "$build/sources.sha256"
else
  shasum -a 256 experiments/s1/*.rs experiments/s1/*.sh > "$build/sources.sha256"
fi
rustfmt --edition 2024 --check experiments/s1/main.rs experiments/s1/scenarios.rs experiments/s1/transport.rs 2>&1 | tee "$build/format.log"
rustc --edition 2024 -D warnings --test experiments/s1/main.rs -o "$build/unit$suffix" 2>&1 | tee "$build/compile-unit.log"
"$build/unit$suffix" 2>&1 | tee "$build/unit.log"
rustc --edition 2024 -D warnings experiments/s1/main.rs -o "$build/s1$suffix" 2>&1 | tee "$build/compile.log"
"$build/s1$suffix" run "$root" 2>&1 | tee "$build/run.log"

#!/usr/bin/env bash
# Run from the repository root with Bash (Git Bash on native Windows).
set -euo pipefail
root="${1:-.throwaway/s5-$(date -u +%Y%m%dT%H%M%SZ)}"
case "$root" in .throwaway/*) ;; *) echo 'Run outputs must be under .throwaway/.' >&2; exit 1;; esac
case "/$root/" in */../*) echo 'Parent traversal is not allowed.' >&2; exit 1;; esac
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
  printf 'conflict_owner=%s\n' "${S5_CONFLICT_OWNER:-fixture-upstream-owner}"
  git rev-parse HEAD
  git status --short -- experiments/s5 .github/workflows/experiment-s5.yml .github/workflows/evidence-experiment-runner.yml
  git --version
  rustc -vV
  uname -srm
  printf 'runner_os=%s\nrunner_arch=%s\nimage_os=%s\nimage_version=%s\nrun_id=%s\nrun_attempt=%s\n' \
    "${RUNNER_OS:-local}" "${RUNNER_ARCH:-unknown}" "${ImageOS:-unknown}" \
    "${ImageVersion:-unknown}" "${GITHUB_RUN_ID:-local}" "${GITHUB_RUN_ATTEMPT:-local}"
} > "$build/provenance.log" 2>&1
if command -v sha256sum >/dev/null; then
  sha256sum experiments/s5/*.rs experiments/s5/*.sh .github/workflows/experiment-s5.yml .github/workflows/evidence-experiment-runner.yml > "$build/sources.sha256"
else
  shasum -a 256 experiments/s5/*.rs experiments/s5/*.sh .github/workflows/experiment-s5.yml .github/workflows/evidence-experiment-runner.yml > "$build/sources.sha256"
fi
bash -n experiments/s5/run.sh experiments/s5/collect.sh experiments/s5/check.sh
bash experiments/s5/check.sh "$build/collection-check" 2>&1 | tee "$build/check.log"
rustfmt --edition 2024 --check experiments/s5/main.rs 2>&1 | tee "$build/format.log"
rustc --edition 2024 -D warnings --test experiments/s5/main.rs -o "$build/unit$suffix" 2>&1 | tee "$build/compile-unit.log"
"$build/unit$suffix" 2>&1 | tee "$build/unit.log"
rustc --edition 2024 -D warnings experiments/s5/main.rs -o "$build/s5$suffix" 2>&1 | tee "$build/compile.log"
"$build/s5$suffix" run "$root" 2>&1 | tee "$build/run.log"

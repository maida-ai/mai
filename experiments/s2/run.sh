#!/usr/bin/env bash
# Run from the repository root with Bash (Git Bash on native Windows).
set -euo pipefail
root="${1:-.throwaway/s2-$(date -u +%Y%m%dT%H%M%SZ)}"
build="${root}.build"
mode="${S2_MODE:-run}"
[[ "$mode" == run || "$mode" == scenarios ]] || { echo "Unknown S2_MODE" >&2; exit 1; }
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
  git status --short -- experiments/s2 .github/workflows/experiment-s2.yml .github/workflows/evidence-experiment-runner.yml
  printf "mode=%s\n" "$mode"
  git --version
  rustc -vV
  uname -srm
  if command -v lscpu >/dev/null; then lscpu; elif command -v sysctl >/dev/null; then sysctl -n machdep.cpu.brand_string hw.memsize; else printf "processor=%s\ncores=%s\n" "${PROCESSOR_IDENTIFIER:-unknown}" "${NUMBER_OF_PROCESSORS:-unknown}"; fi
  printf 'runner_os=%s\nrunner_arch=%s\nimage_os=%s\nimage_version=%s\nrun_id=%s\nrun_attempt=%s\n' \
    "${RUNNER_OS:-local}" "${RUNNER_ARCH:-unknown}" "${ImageOS:-unknown}" \
    "${ImageVersion:-unknown}" "${GITHUB_RUN_ID:-local}" "${GITHUB_RUN_ATTEMPT:-local}"
} > "$build/provenance.log" 2>&1
if command -v sha256sum >/dev/null; then
  sha256sum experiments/s2/*.rs experiments/s2/*.sh > "$build/sources.sha256"
else
  shasum -a 256 experiments/s2/*.rs experiments/s2/*.sh > "$build/sources.sha256"
fi
rustfmt --edition 2024 --check experiments/s2/main.rs experiments/s2/manifest.rs 2>&1 | tee "$build/format.log"
rustc --edition 2024 -D warnings --test experiments/s2/main.rs -o "$build/unit$suffix" 2>&1 | tee "$build/compile-unit.log"
"$build/unit$suffix" 2>&1 | tee "$build/unit.log"
rustc --edition 2024 -D warnings experiments/s2/main.rs -o "$build/s2$suffix" 2>&1 | tee "$build/compile.log"
"$build/s2$suffix" "$mode" "$root" 2>&1 | tee "$build/run.log"

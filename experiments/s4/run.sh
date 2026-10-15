#!/usr/bin/env bash
# Run from the repository root with Bash (Git Bash on native Windows).
set -euo pipefail
root="${1:-.throwaway/s4-$(date -u +%Y%m%dT%H%M%SZ)}"
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
  git status --short -- experiments/s4 .github/workflows/experiment-s4.yml .github/workflows/evidence-experiment-runner.yml
  git --version
  rustc -vV
  uname -srm
  printf 'runner_os=%s\nrunner_arch=%s\nimage_os=%s\nimage_version=%s\nrun_id=%s\nrun_attempt=%s\n' \
    "${RUNNER_OS:-local}" "${RUNNER_ARCH:-unknown}" "${ImageOS:-unknown}" \
    "${ImageVersion:-unknown}" "${GITHUB_RUN_ID:-local}" "${GITHUB_RUN_ATTEMPT:-local}"
} > "$build/provenance.log" 2>&1
if command -v sha256sum >/dev/null; then
  sha256sum experiments/s4/*.rs experiments/s4/*.sh experiments/s4/*.csv experiments/s4/Cargo.toml experiments/s4/Cargo.lock > "$build/sources.sha256"
else
  shasum -a 256 experiments/s4/*.rs experiments/s4/*.sh experiments/s4/*.csv experiments/s4/Cargo.toml experiments/s4/Cargo.lock > "$build/sources.sha256"
fi
rustfmt --edition 2024 --check experiments/s4/main.rs experiments/s4/recovery.rs experiments/s4/input.rs experiments/s4/policy.rs 2>&1 | tee "$build/format.log"
rustc --edition 2024 -D warnings --test experiments/s4/main.rs -o "$build/unit$suffix" 2>&1 | tee "$build/compile-unit.log"
"$build/unit$suffix" 2>&1 | tee "$build/unit.log"
rustc --edition 2024 -D warnings -O experiments/s4/main.rs -o "$build/s4$suffix" 2>&1 | tee "$build/compile.log"
args=(run "$root")
if [[ -n "${S4_WORKLOAD:-}" ]]; then
  cp "$S4_WORKLOAD" "$build/workload.log"
  if command -v sha256sum >/dev/null; then
    sha256sum "$build/workload.log" >> "$build/sources.sha256"
  else
    shasum -a 256 "$build/workload.log" >> "$build/sources.sha256"
  fi
  args+=("$build/workload.log")
fi
printf 'command=%s run %s; workload=%s\n' "$build/s4$suffix" "$root" "${S4_WORKLOAD:-built-in-grid}" >> "$build/provenance.log"
"$build/s4$suffix" "${args[@]}" 2>&1 | tee "$build/run.log"

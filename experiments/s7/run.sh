#!/usr/bin/env bash
# Run from the repository root with Bash (Git Bash on native Windows).
set -euo pipefail
root="${1:-.throwaway/s7-$(date -u +%Y%m%dT%H%M%SZ)}"
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
  git status --short -- experiments/s7 .github/workflows/experiment-s7.yml .github/workflows/evidence-experiment-runner.yml
  git --version
  rustc -vV
  uname -srm
  printf 'runner_os=%s\nrunner_arch=%s\nimage_os=%s\nimage_version=%s\nrun_id=%s\nrun_attempt=%s\n' \
    "${RUNNER_OS:-local}" "${RUNNER_ARCH:-unknown}" "${ImageOS:-unknown}" \
    "${ImageVersion:-unknown}" "${GITHUB_RUN_ID:-local}" "${GITHUB_RUN_ATTEMPT:-local}"
} > "$build/provenance.log" 2>&1
if command -v sha256sum >/dev/null; then
  sha256sum experiments/s7/src/*.rs experiments/s7/*.sh experiments/s7/Cargo.toml experiments/s7/Cargo.lock > "$build/sources.sha256"
else
  shasum -a 256 experiments/s7/src/*.rs experiments/s7/*.sh experiments/s7/Cargo.toml experiments/s7/Cargo.lock > "$build/sources.sha256"
fi
build="$(cd "$build" && pwd)"
export CARGO_HOME="$build/cargo-home"
export CARGO_TARGET_DIR="$build/target"
cargo fmt --manifest-path experiments/s7/Cargo.toml --check 2>&1 | tee "$build/format.log"
cargo test --manifest-path experiments/s7/Cargo.toml --locked 2>&1 | tee "$build/unit.log"
cargo clippy --manifest-path experiments/s7/Cargo.toml --all-targets --locked -- -D warnings 2>&1 | tee "$build/clippy.log"
cargo build --manifest-path experiments/s7/Cargo.toml --locked 2>&1 | tee "$build/compile.log"
"$CARGO_TARGET_DIR/debug/s7-crash-experiment$suffix" run "$root" 2>&1 | tee "$build/run.log"

#!/usr/bin/env bash
# Run from the repository root with Bash (Git Bash on native Windows).
set -euo pipefail
root="${1:-.throwaway/s3-$(date -u +%Y%m%dT%H%M%SZ)}"
build="${root}.build"
mode="${S3_MODE:-run}"
[[ "$mode" == run || "$mode" == functional ]] || { echo "Unknown S3_MODE" >&2; exit 1; }
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
  git status --short -- experiments/s3 .github/workflows/experiment-s3.yml .github/workflows/evidence-experiment-runner.yml
  printf 'mode=%s\n' "$mode"
  git --version
  rustc -vV
  uname -srm
  printf 'runner_os=%s\nrunner_arch=%s\nimage_os=%s\nimage_version=%s\nrun_id=%s\nrun_attempt=%s\n' \
    "${RUNNER_OS:-local}" "${RUNNER_ARCH:-unknown}" "${ImageOS:-unknown}" \
    "${ImageVersion:-unknown}" "${GITHUB_RUN_ID:-local}" "${GITHUB_RUN_ATTEMPT:-local}"
} > "$build/provenance.log" 2>&1
if command -v sha256sum >/dev/null; then
  sha256sum experiments/s3/*.rs experiments/s3/*.sh > "$build/sources.sha256"
else
  shasum -a 256 experiments/s3/*.rs experiments/s3/*.sh > "$build/sources.sha256"
fi
# Host uv is recorded; missing runners receive a pinned experiment-only wheel.
if ! command -v uv >/dev/null; then
  python_command=python3
  case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) python_command=python;; esac
  "$python_command" -m pip install --disable-pip-version-check --cache-dir "$build/pip-cache" --no-deps --target "$build/uv-tool" uv==0.6.17 > "$build/setup-uv.log" 2>&1
  tool_root="$(cd "$build" && pwd)/uv-tool"
  export PATH="$tool_root/bin:$tool_root/Scripts:$PATH"
fi
rustfmt --edition 2024 --check experiments/s3/main.rs 2>&1 | tee "$build/format.log"
rustc --edition 2024 -D warnings --test experiments/s3/main.rs -o "$build/unit$suffix" 2>&1 | tee "$build/compile-unit.log"
"$build/unit$suffix" 2>&1 | tee "$build/unit.log"
rustc --edition 2024 -D warnings experiments/s3/main.rs -o "$build/s3$suffix" 2>&1 | tee "$build/compile.log"
"$build/s3$suffix" "$mode" "$root" 2>&1 | tee "$build/run.log"

#!/usr/bin/env bash
# Small failure-path checks; caller supplies a fresh directory beneath .throwaway/.
set -euo pipefail
root="${1:?new test root required}"
[[ ! -e "$root" ]] || { echo 'Test root must be new.' >&2; exit 1; }
mkdir -p "$root/input.build" "$root/input/repository"
printf 'allowed\n' > "$root/input/commands.log"
printf 'allowed\n' > "$root/input.build/unit.log"
printf 'never collect\n' > "$root/input/repository/private.log"
printf 'never collect\n' > "$root/input.build/unknown.log"
bash experiments/s5/collect.sh "$root/input" "$root/output"
[[ -f "$root/output/commands.log" && -f "$root/output/build/unit.log" ]]
[[ "$(find "$root/output" -type f | wc -l | tr -d ' ')" == 2 ]]
if bash experiments/s5/collect.sh "$root/input" "$root/output" 2>/dev/null; then
  echo 'FAIL collection overwrote existing evidence' >&2; exit 1
fi
if bash experiments/s5/run.sh "$root/input" >/dev/null 2>&1; then
  echo 'FAIL runner reused an existing fixture' >&2; exit 1
fi
if bash experiments/s5/run.sh ../s5-escape >/dev/null 2>&1; then
  echo 'FAIL runner accepted output outside .throwaway' >&2; exit 1
fi
printf 'PASS collector allowlist, overwrite rejection, fixture reuse rejection, output boundary\n'

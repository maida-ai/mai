# See a failed gate and submit a correction

Run [getting started](getting-started.md) first and keep the same shell, including `MAI_BIN` and `DEMO_ROOT`. This optional exercise uses its retained disposable authority. It proves a small boundary: a successful push can still fail validation, while accepted trunk stays unchanged. It is not a general correctness or performance qualification.

## 1. Submit work that violates the gate

The fresh workspace contains accepted r2. Change the value that the trusted gate checks and add a feature file.

```sh
"$MAI_BIN" status "$DEMO_ROOT/project" --json
git -C "$DEMO_ROOT/fresh" config user.name 'Demo agent'
git -C "$DEMO_ROOT/fresh" config user.email 'agent@example.invalid'
printf 'goodbye\n' > "$DEMO_ROOT/fresh/message.txt"
printf 'new feature\n' > "$DEMO_ROOT/fresh/feature.txt"
git -C "$DEMO_ROOT/fresh" add message.txt feature.txt
git -C "$DEMO_ROOT/fresh" commit -m 'Submit a deliberately failing change'
BAD=$(git -C "$DEMO_ROOT/fresh" rev-parse HEAD)
BEFORE=$(git --git-dir="$DEMO_ROOT/project/authority.git" rev-parse refs/heads/trunk)
git -C "$DEMO_ROOT/fresh" push -o agent=fresh mai HEAD:refs/for/main
```

Expected: push succeeds with immutable identity `fresh-<BAD OID>`, generation 1. This acknowledges receipt only.

## 2. Observe rejection and inspect the record

Integration should fail because `message.txt` no longer contains `hello`. The shell condition below treats an unexpected success as an error.

```sh
if "$MAI_BIN" integrate "$DEMO_ROOT/project" "fresh-$BAD" --mode textual --wait-seconds 60 --json; then
  printf 'Unexpected acceptance: stop and inspect the demo\n' >&2
  exit 1
else
  RESULT=$?
  test "$RESULT" -eq 3 || { printf 'Expected rejection exit 3, got %s\n' "$RESULT" >&2; exit 1; }
fi
"$MAI_BIN" show "$DEMO_ROOT/project" "fresh-$BAD" --generation 1 --json
test "$(git --git-dir="$DEMO_ROOT/project/authority.git" rev-parse refs/heads/trunk)" = "$BEFORE"
```

Expected: integration returns rejected exit code 3 with `reason: check_failed`; `show` returns a successful query with a terminal rejection and gate diagnostics. Trunk remains r2. Inspect the actual outcome: an infrastructure error is not evidence that this intended gate failure occurred. The gate prints no output, so an empty log tail is expected.

## 3. Correct the work from accepted trunk

A rejected submission remains rejected. Use a new workspace from accepted trunk to submit the corrected feature independently; the failed workspace remains available for diagnosis. This avoids making the correction depend on rejected ancestry.

```sh
"$MAI_BIN" workspace "$DEMO_ROOT/project" correction "$DEMO_ROOT/correction" --json
git -C "$DEMO_ROOT/correction" config user.name 'Demo agent'
git -C "$DEMO_ROOT/correction" config user.email 'agent@example.invalid'
printf 'new feature\n' > "$DEMO_ROOT/correction/feature.txt"
git -C "$DEMO_ROOT/correction" add feature.txt
git -C "$DEMO_ROOT/correction" commit -m 'Add feature while preserving the checked value'
FIX=$(git -C "$DEMO_ROOT/correction" rev-parse HEAD)
git -C "$DEMO_ROOT/correction" push -o agent=correction mai HEAD:refs/for/main
"$MAI_BIN" integrate "$DEMO_ROOT/project" "correction-$FIX" --mode textual --wait-seconds 60 --json
"$MAI_BIN" show "$DEMO_ROOT/project" "correction-$FIX" --generation 1 --json
git --git-dir="$DEMO_ROOT/project/authority.git" show refs/heads/trunk:message.txt
git --git-dir="$DEMO_ROOT/project/authority.git" show refs/heads/trunk:feature.txt
test "$(git -C "$DEMO_ROOT/fresh" rev-parse HEAD)" = "$BAD"
"$MAI_BIN" stop "$DEMO_ROOT/project" --json
```

Expected: the corrected submission lands as r3. Accepted contents are `hello` and `new feature`; the original failed workspace still has its submitted commit. No command silently rewrites the rejected change or its working tree. Keep the demo for inspection, or remove only its disposable directory when finished.

For actual conflicting changes and continuing later commits, follow the [two-agent workflow](two-agent-workflow.md). For uncertain responses, inspect the same generation before retrying; a client timeout does not cancel integration.

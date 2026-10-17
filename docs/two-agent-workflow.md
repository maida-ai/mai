# Two agents: continue, conflict and correct

This disposable POSIX-shell walkthrough demonstrates explicit submission and integration, continuation after squash landing, and ordinary editing to correct conflicting intent. It uses a deliberately trivial gate; it is not a recommended policy or a performance demonstration. Build mai first and run from its checkout. Run every shell block in order in the same shell session because later steps use variables from earlier ones. Keep all created directories for inspection.

## 1. Prepare a trusted gate and source repository

This step creates the seed repository and the small gate that later candidates must pass. Expected: the source contains one committed baseline and policy.

```sh
set -eu
MAI_BIN="$PWD/target/debug/mai"
DEMO=$(mktemp -d /var/tmp/mai-two.XXXXXX)
mkdir "$DEMO/seed"
git -C "$DEMO/seed" init -b main
git -C "$DEMO/seed" config user.name 'Demo maintainer'
git -C "$DEMO/seed" config user.email 'demo@example.invalid'
printf 'base\n' > "$DEMO/seed/shared.txt"
printf 'baseline\n' > "$DEMO/seed/parent.txt"
cat > "$DEMO/seed/gate.sh" <<'GATE'
test -f "$MAI_CANDIDATE_ROOT/shared.txt"
GATE
cat > "$DEMO/seed/mai.toml" <<'POLICY'
version = 1
[check]
program = "sh"
script = "gate.sh"
timeout_ms = 5000
output_tail_bytes = 1024
[validators]
slots = 1
port_base = 43000
ports_per_slot = 10
POLICY
git -C "$DEMO/seed" add .
git -C "$DEMO/seed" commit -m 'Seed disposable collaboration'
```

## 2. Create the authority and enable receive

This step imports the trusted baseline, starts the local engine, enables bounded receive, and creates Alice's independent workspace. Expected: the authority is ready to receive; creating the workspace does not submit or land work.

```sh
"$MAI_BIN" init "$DEMO/project" --from "$DEMO/seed" --json
"$MAI_BIN" status "$DEMO/project" --json
"$MAI_BIN" receive enable "$DEMO/project" --max-input-bytes 1048576 --max-file-bytes 65536 --max-changed-files 32 --json
"$MAI_BIN" workspace "$DEMO/project" alice "$DEMO/alice" --json
git -C "$DEMO/alice" config user.name 'Alice'
git -C "$DEMO/alice" config user.email 'alice@example.invalid'
```

## 3. Submit and explicitly integrate Alice's parent

Alice submits a change to the parent file, then an operator explicitly selects that generation for serial integration. Expected: the parent lands as revision r1.

```sh
printf 'accepted Alice parent\n' > "$DEMO/alice/parent.txt"
git -C "$DEMO/alice" add parent.txt
git -C "$DEMO/alice" commit -m 'Alice parent'
A=$(git -C "$DEMO/alice" rev-parse HEAD)
git -C "$DEMO/alice" push -o agent=alice mai HEAD:refs/for/main
"$MAI_BIN" integrate "$DEMO/project" "alice-$A" --generation 1 --mode textual --json
```

## 4. Keep Alice's later work local while Bob submits

Alice makes another local commit but does not submit it yet. Bob starts from the accepted parent, changes that file, adds another file, and submits his change. Expected: Bob's generation lands as r2 while Alice's later commit remains local.

```sh
# Alice continues locally without submitting this later commit yet.
printf 'Alice intent\n' > "$DEMO/alice/shared.txt"
git -C "$DEMO/alice" add shared.txt
git -C "$DEMO/alice" commit -m 'Alice later local work'
LOCAL=$(git -C "$DEMO/alice" rev-parse HEAD)
# Bob starts from the accepted parent and changes its content.
"$MAI_BIN" workspace "$DEMO/project" bob "$DEMO/bob" --json
git -C "$DEMO/bob" config user.name 'Bob'
git -C "$DEMO/bob" config user.email 'bob@example.invalid'
printf 'Bob updated accepted parent\n' > "$DEMO/bob/parent.txt"
printf 'Bob additional work\n' > "$DEMO/bob/bob.txt"
git -C "$DEMO/bob" add .
git -C "$DEMO/bob" commit -m 'Bob additional accepted work'
B=$(git -C "$DEMO/bob" rev-parse HEAD)
git -C "$DEMO/bob" push -o agent=bob mai HEAD:refs/for/main
"$MAI_BIN" integrate "$DEMO/project" "bob-$B" --generation 1 --mode textual --json
```

## 5. Continue Alice after the accepted squash

Continuation copies accepted trunk and replays only Alice's unsubmitted local commit into a new workspace. Expected: the new workspace has Bob's accepted files plus Alice's later edit, while the original clone remains at `$LOCAL`.

```sh
"$MAI_BIN" continue "$DEMO/project" "$DEMO/alice" "$DEMO/continued" --change "alice-$A" --generation 1 --json
test "$(cat "$DEMO/continued/parent.txt")" = 'Bob updated accepted parent'
test "$(cat "$DEMO/continued/shared.txt")" = 'Alice intent'
test -f "$DEMO/continued/bob.txt"
test "$(git -C "$DEMO/alice" rev-parse HEAD)" = "$LOCAL"
```

The continued workspace includes Bob's accepted changes and only Alice's later local commit. Replaying Alice's already accepted parent would overwrite or conflict with Bob's updated parent; that parent must be excluded. Alice's original clone is still available. Repeating the command with the same destination refuses without replacing it.

## 6. Create and inspect a true conflict

Bob now changes the same content as Alice's pending local work. His change lands first; Alice's continued submission is rejected as a true conflict. Replaying the original local commit is also expected to fail with exit 5 and preserve an incomplete marker.

```sh
printf 'Bob overlapping intent\n' > "$DEMO/bob/shared.txt"
git -C "$DEMO/bob" add shared.txt
git -C "$DEMO/bob" commit -m 'Bob overlapping edit'
B2=$(git -C "$DEMO/bob" rev-parse HEAD)
git -C "$DEMO/bob" push -o agent=bob mai HEAD:refs/for/main
"$MAI_BIN" integrate "$DEMO/project" "bob-$B2" --generation 1 --mode textual --json
C=$(git -C "$DEMO/continued" rev-parse HEAD)
git -C "$DEMO/continued" push -o agent=alice mai HEAD:refs/for/main
# Expected exit 3 with a rejected conflict outcome:
RESULT=0
"$MAI_BIN" integrate "$DEMO/project" "alice-$C" --generation 1 --mode textual --json || RESULT=$?
test "$RESULT" -eq 3
"$MAI_BIN" show "$DEMO/project" "alice-$C" --generation 1 --json
# Also expected to fail: replaying the original local commit now conflicts.
# The original clone and this incomplete destination remain preserved.
RESULT=0
"$MAI_BIN" continue "$DEMO/project" "$DEMO/alice" "$DEMO/conflicted" --change "alice-$A" --generation 1 --json || RESULT=$?
test "$RESULT" -eq 5
test -f "$DEMO/conflicted/.mai-continuation-incomplete"
```

Neither retrying the rejected generation nor deleting an incomplete marker resolves the conflicting intent. Make the correction in another fresh workspace using ordinary editing. The example deliberately combines both intents; mai does not choose that content for you.

## 7. Correct the conflict in a fresh workspace

Alice combines the intended edits in a new workspace, submits the corrected change, and explicitly selects it for integration. Expected: the correction lands as r4, after Bob's overlapping edit at r3; the rejected generation did not advance trunk.

```sh
"$MAI_BIN" workspace "$DEMO/project" alice "$DEMO/correction" --json
git -C "$DEMO/correction" config user.name 'Alice'
git -C "$DEMO/correction" config user.email 'alice@example.invalid'
printf 'Bob overlapping intent + Alice intent\n' > "$DEMO/correction/shared.txt"
git -C "$DEMO/correction" add shared.txt
git -C "$DEMO/correction" commit -m 'Reconcile both intended edits'
FIX=$(git -C "$DEMO/correction" rev-parse HEAD)
git -C "$DEMO/correction" push -o agent=alice mai HEAD:refs/for/main
"$MAI_BIN" integrate "$DEMO/project" "alice-$FIX" --generation 1 --mode textual --json
"$MAI_BIN" stop "$DEMO/project" --json
printf 'Disposable work and evidence retained at %s\n' "$DEMO"
```

Expected accepted revisions are Alice's parent r1, Bob's additional work r2, Bob's overlapping edit r3 and the correction r4. The rejected change does not advance trunk. The [reproducible harness](../experiments/continuation/README.md) adds exact source-byte preservation and repeated-destination checks. See [continuation](continuation.md) for unsupported histories and [validator recovery](validator-recovery.md) for the separate covered engine-crash procedure.

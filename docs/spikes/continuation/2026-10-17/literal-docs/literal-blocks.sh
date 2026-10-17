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
"$MAI_BIN" init "$DEMO/project" --from "$DEMO/seed" --json
"$MAI_BIN" status "$DEMO/project" --json
"$MAI_BIN" receive enable "$DEMO/project" --max-input-bytes 1048576 --max-file-bytes 65536 --max-changed-files 32 --json
"$MAI_BIN" workspace "$DEMO/project" alice "$DEMO/alice" --json
git -C "$DEMO/alice" config user.name 'Alice'
git -C "$DEMO/alice" config user.email 'alice@example.invalid'
printf 'accepted Alice parent\n' > "$DEMO/alice/parent.txt"
git -C "$DEMO/alice" add parent.txt
git -C "$DEMO/alice" commit -m 'Alice parent'
A=$(git -C "$DEMO/alice" rev-parse HEAD)
git -C "$DEMO/alice" push -o agent=alice mai HEAD:refs/for/main
"$MAI_BIN" integrate "$DEMO/project" "alice-$A" --generation 1 --mode textual --json
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
"$MAI_BIN" continue "$DEMO/project" "$DEMO/alice" "$DEMO/continued" --change "alice-$A" --generation 1 --json
test "$(cat "$DEMO/continued/parent.txt")" = 'Bob updated accepted parent'
test "$(cat "$DEMO/continued/shared.txt")" = 'Alice intent'
test -f "$DEMO/continued/bob.txt"
test "$(git -C "$DEMO/alice" rev-parse HEAD)" = "$LOCAL"


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

printf "\nMAI_DOC_FIXTURE=%s\n" "$DEMO"
printf "MAI_DOC_LOCAL=%s\n" "$LOCAL"
printf "MAI_DOC_FINAL=%s\n" "$FIX"

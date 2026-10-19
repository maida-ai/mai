# Try two dependent changes through a trusted gate

The old worktree prototype has been retired after scoped design acceptance. The foundation implements a versioned change lifecycle, SQLite persistence, repository validation, authority initialization, local engine serving, independent workspaces and manual trusted validation. Receive supports explicit named-generation integration by default and opt-in integration on receipt. Use disposable evaluation repositories; see [current delivery and limits](release-status.md).

## Build and verify

Install Rust 1.85+, Git 2.40+ and a C compiler for bundled SQLite. On native Windows, use Git for Windows and the MSVC build tools. No Python runtime or standalone SQLite installation is required.

```sh
git clone https://github.com/maida-ai/mai.git
cd mai
cargo build --locked --bin mai
cargo test --all-targets --locked
cargo run --locked -- --help
cargo run --locked -- --version
```

The version remains `mai 0.0.1`; authority and ordinary lease formats are now 2, with no migration from older disposable state. The current binary exposes initialization, engine administration, workspaces, manual validation, receive with explicit or opt-in automatic integration, submission inspection, ordinary leases, copy-based continuation and explicit Linux validator recovery; see the [CLI reference](cli.md) for the available commands. Retired commands such as `refresh`, `submit` and `github-sync` reject; there are no legacy aliases or data migrations. Old demos and benchmarks have been removed with the old engine, while experiment harnesses and reviewed evidence remain available.

## Run the example

This POSIX-shell example uses a disposable directory and a deliberately small gate. Run it from the built mai checkout. Git Bash can supply the shell on Windows; native platform qualification remains separate. All identities and settings below are local example choices, not production policy defaults.

Validators require a temporary directory outside any Git repository. If your system temporary root has a `.git` ancestor, set the service’s `TMPDIR`, `TMP` and `TEMP` to an external directory before starting; `/var/tmp` is one Linux example. An error saying “validator temporary directory must be outside a repository” indicates this environment constraint. See [validator slots](validator-slots.md).

### 1. Prepare a disposable source repository

The gate below accepts only `hello` in `message.txt`. Keeping that gate in the initial source makes it trusted policy for later submissions. Run all steps in the same shell.

```sh
MAI_BIN="$PWD/target/debug/mai"
DEMO_ROOT=$(mktemp -d /tmp/mai-demo.XXXXXX)
mkdir "$DEMO_ROOT/source"
git -C "$DEMO_ROOT/source" init -b main
git -C "$DEMO_ROOT/source" config user.name 'Demo maintainer'
git -C "$DEMO_ROOT/source" config user.email 'demo@example.invalid'
cat > "$DEMO_ROOT/source/gate.sh" <<'GATE'
set -eu
test "$(cat "$MAI_CANDIDATE_ROOT/message.txt")" = 'hello'
GATE
cat > "$DEMO_ROOT/source/mai.toml" <<'POLICY'
version = 1
[check]
program = "sh"
script = "gate.sh"
timeout_ms = 5000
output_tail_bytes = 1024
[validators]
slots = 1
port_base = 31000
ports_per_slot = 10
POLICY
printf 'hello\n' > "$DEMO_ROOT/source/message.txt"
git -C "$DEMO_ROOT/source" add .
git -C "$DEMO_ROOT/source" commit -m 'Add explicit demo gate'
```

Expected: the source has one commit containing `mai.toml`, `gate.sh` and `message.txt`. No authority or agent process exists yet.

### 2. Create the authority and validate its baseline

An authority owns accepted history; an agent works in an independent clone. First check that the small trusted gate actually runs.

```sh
"$MAI_BIN" init "$DEMO_ROOT/project" --from "$DEMO_ROOT/source" --json
"$MAI_BIN" status "$DEMO_ROOT/project" --json
"$MAI_BIN" workspace "$DEMO_ROOT/project" alice "$DEMO_ROOT/alice" --json
BASE=$(git --git-dir="$DEMO_ROOT/project/authority.git" rev-parse refs/heads/trunk)
"$MAI_BIN" validator run "$DEMO_ROOT/project" "$BASE" --json
```

Expected: initialization succeeds, status reports `registration_available: false`, and baseline validation succeeds. Workspace creation does not submit or land work.

### 3. Enable receipt and submit a parent

These limits are explicit example choices. This walkthrough uses the default explicit integration mode: push acknowledges durable receipt, not acceptance. Add `--auto-integrate` to the enable command to attempt gated textual integration during each receipt; in that mode, inspect each outcome instead of expecting the `received` states below.

```sh
"$MAI_BIN" receive enable "$DEMO_ROOT/project" --max-input-bytes 10485760 --max-file-bytes 1048576 --max-changed-files 100 --json
git -C "$DEMO_ROOT/alice" config user.name 'Demo Alice'
git -C "$DEMO_ROOT/alice" config user.email 'alice@example.invalid'
printf 'parent work\n' > "$DEMO_ROOT/alice/parent.txt"
git -C "$DEMO_ROOT/alice" add parent.txt
git -C "$DEMO_ROOT/alice" commit -m 'Parent change'
PARENT=$(git -C "$DEMO_ROOT/alice" rev-parse HEAD)
git -C "$DEMO_ROOT/alice" push -o agent=alice mai HEAD:refs/for/main
```

Expected: push reports immutable identity `alice-<PARENT OID>`, generation 1. Keep that identity for inspection and retries.

### 4. Submit a dependent child

Continue from the parent commit without waiting for it to land. The engine records the submitted ancestry.

```sh
printf 'dependent work\n' > "$DEMO_ROOT/alice/child.txt"
git -C "$DEMO_ROOT/alice" add child.txt
git -C "$DEMO_ROOT/alice" commit -m 'Dependent child change'
CHILD=$(git -C "$DEMO_ROOT/alice" rev-parse HEAD)
git -C "$DEMO_ROOT/alice" push -o agent=alice mai HEAD:refs/for/main
"$MAI_BIN" show "$DEMO_ROOT/project" "alice-$PARENT" --generation 1 --json
"$MAI_BIN" show "$DEMO_ROOT/project" "alice-$CHILD" --generation 1 --json
```

Expected: both records are `received`; the child names the parent generation in `dependencies`. A successful `show` means the query succeeded. Neither change has landed.

### 5. Integrate parent, then child

The default mode used here requires an operator to select each generation explicitly. The gate checks the exact candidate before landing; the child maps onto the parent’s accepted squash.

```sh
"$MAI_BIN" integrate "$DEMO_ROOT/project" "alice-$PARENT" --mode textual --wait-seconds 60 --json
"$MAI_BIN" integrate "$DEMO_ROOT/project" "alice-$CHILD" --mode textual --wait-seconds 60 --json
"$MAI_BIN" show "$DEMO_ROOT/project" "alice-$PARENT" --generation 1 --json
"$MAI_BIN" show "$DEMO_ROOT/project" "alice-$CHILD" --generation 1 --json
```

Expected: parent lands as r1 and child as r2; both final records have terminal outcome `landed`.

### 6. Verify accepted work and preserve the original clone

A fresh clone sees both accepted changes. Alice’s submitted history remains untouched.

```sh
"$MAI_BIN" workspace "$DEMO_ROOT/project" fresh "$DEMO_ROOT/fresh" --json
test -f "$DEMO_ROOT/fresh/parent.txt"
test -f "$DEMO_ROOT/fresh/child.txt"
test "$(git -C "$DEMO_ROOT/alice" rev-parse HEAD)" = "$CHILD"
"$MAI_BIN" stop "$DEMO_ROOT/project" --json
printf 'Disposable demo retained at %s\n' "$DEMO_ROOT"
```

Expected: all three assertions succeed and the demo prints its retained directory. No bootstrap runs because this example declares none.

This example selects textual integration explicitly and uses a tiny gate, not a recommended project policy. For later clean unregistered commits, [copy-based continuation](continuation.md) replays only the suffix after an explicitly verified landed submission into a new workspace. Preserve the old clone. Try the [failed-gate and correction exercise](check-failure.md), or follow the [two-agent continuation/conflict/correction walkthrough](two-agent-workflow.md) to exercise the next part of the user loop. `stop` acknowledges a shutdown request; process exit can follow shortly afterward. Keep the disposable directory for inspection, then remove only that directory when finished. On an uncertain response, inspect the generation after the operation completes; timeout does not cancel integration. For opted-in Linux supervision, [explicit validator recovery](validator-recovery.md) can reclaim a covered interrupted slot from verified cleanup proof. Missing proof and worker death still require preserved-state diagnosis. See [receive and integration](receive.md), [manual validation](manual-validator.md) and [workspace recovery](workspaces.md).

## What is qualified here

Lifecycle tests cover immutable submission generations, retry deduplication, dependency blocking and rejection, protected-change approval states, explicit validation attempts, terminal outcomes and later annotations. A serialized snapshot round trip tests the model's restart contract; it does not prove durable receive acknowledgment or Git landing. Persistence and startup reconciliation now have interruption tests; explicit receive/integration have additional enabled-path tests; these finite tests do not qualify an automatic queue or power-loss durability.

CI runs Rust formatting, Clippy, tests and release builds on Linux, macOS and Windows. A separate bounded Linux job builds the checksum-pinned Git 2.40.0 release and uses Rust 1.85.0 with the locked dependency graph. This minimum combination is not a Windows or macOS qualification. A local Linux pass does not establish hosted native results. See [platform qualification](platform-qualification.md) for capability probes, fallback semantics and evidence limits. See [CLI reference](cli.md), [architecture](architecture.md) and [current delivery and limits](release-status.md) for current behavior and remaining limits.

[All docs →](README.md)

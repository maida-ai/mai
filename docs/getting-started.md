# Build the MVP foundation

The old worktree prototype has been retired after scoped design acceptance. The foundation implements a versioned change lifecycle, SQLite persistence, repository validation, authority initialization, local engine serving, independent workspaces and manual trusted validation. Explicit receive and named-generation integration are available; this is not the full automatic integration queue. Use disposable evaluation repositories throughout MVP development.

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

The version remains `mai 0.0.1`; authority and ordinary lease formats are now 2, with no migration from older disposable state. The current binary exposes initialization, engine administration, workspaces, manual validation, explicit receive/integration, submission inspection, ordinary leases, copy-based continuation and explicit Linux validator recovery; see the [CLI reference](cli.md) for the available commands. Retired commands such as `refresh`, `submit` and `github-sync` reject; there are no legacy aliases or data migrations. Old demos and benchmarks have been removed with the old engine, while experiment harnesses and reviewed evidence remain available.

## Try the current workflow

This POSIX-shell example uses a disposable directory and a deliberately small gate. Run it from the built mai checkout. Git Bash can supply the shell on Windows; native platform qualification remains separate. All identities and settings below are local example choices, not production policy defaults.

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
"$MAI_BIN" init "$DEMO_ROOT/project" --from "$DEMO_ROOT/source" --json
"$MAI_BIN" status "$DEMO_ROOT/project" --json
"$MAI_BIN" workspace "$DEMO_ROOT/project" alice "$DEMO_ROOT/alice" --json
"$MAI_BIN" workspace "$DEMO_ROOT/project" bob "$DEMO_ROOT/bob" --json
BASE=$(git --git-dir="$DEMO_ROOT/project/authority.git" rev-parse refs/heads/trunk)
"$MAI_BIN" validator run "$DEMO_ROOT/project" "$BASE" --json
# Enable durable receipt with explicit example limits.
"$MAI_BIN" receive enable "$DEMO_ROOT/project" --max-input-bytes 10485760 --max-file-bytes 1048576 --max-changed-files 100 --json
git -C "$DEMO_ROOT/alice" config user.name 'Demo Alice'
git -C "$DEMO_ROOT/alice" config user.email 'alice@example.invalid'
printf 'parent work\n' > "$DEMO_ROOT/alice/parent.txt"
git -C "$DEMO_ROOT/alice" add parent.txt
git -C "$DEMO_ROOT/alice" commit -m 'Parent change'
PARENT=$(git -C "$DEMO_ROOT/alice" rev-parse HEAD)
git -C "$DEMO_ROOT/alice" push -o agent=alice mai HEAD:refs/for/main
printf 'dependent work\n' > "$DEMO_ROOT/alice/child.txt"
git -C "$DEMO_ROOT/alice" add child.txt
git -C "$DEMO_ROOT/alice" commit -m 'Dependent child change'
CHILD=$(git -C "$DEMO_ROOT/alice" rev-parse HEAD)
git -C "$DEMO_ROOT/alice" push -o agent=alice mai HEAD:refs/for/main
"$MAI_BIN" show "$DEMO_ROOT/project" "alice-$PARENT" --generation 1 --json
"$MAI_BIN" show "$DEMO_ROOT/project" "alice-$CHILD" --generation 1 --json
"$MAI_BIN" integrate "$DEMO_ROOT/project" "alice-$PARENT" --mode textual --wait-seconds 60 --json
"$MAI_BIN" integrate "$DEMO_ROOT/project" "alice-$CHILD" --mode textual --wait-seconds 60 --json
"$MAI_BIN" show "$DEMO_ROOT/project" "alice-$PARENT" --generation 1 --json
"$MAI_BIN" show "$DEMO_ROOT/project" "alice-$CHILD" --generation 1 --json
"$MAI_BIN" workspace "$DEMO_ROOT/project" fresh "$DEMO_ROOT/fresh" --json
test -f "$DEMO_ROOT/fresh/parent.txt"
test -f "$DEMO_ROOT/fresh/child.txt"
test "$(git -C "$DEMO_ROOT/alice" rev-parse HEAD)" = "$CHILD"
"$MAI_BIN" stop "$DEMO_ROOT/project" --json
printf 'Disposable demo retained at %s\n' "$DEMO_ROOT"
```

Expected results: initialization succeeds, status initially reports `registration_available: false`, and manual baseline validation succeeds. Explicit enablement allows each push to report its immutable `alice-<OID>` generation 1 identity. Before integration, both records have state `received`; the child records the parent generation in `dependencies`. Explicitly integrate the parent first: it lands as r1, satisfying the child dependency. Integrating the child lands r2 with both files, mapped onto the parent's accepted squash. Final `show` records are terminal `landed`; a successful query alone is not acceptance. A fresh workspace contains both accepted files, while Alice's original HEAD remains at her submitted child. No bootstrap runs because this example declares none.

This example selects textual integration explicitly and uses a tiny gate, not a recommended project policy. There is no automatic scheduler or in-place sync. For later clean unregistered commits, [copy-based continuation](continuation.md) replays only the suffix after an explicitly verified landed submission into a new workspace. Preserve the old clone. Follow the [two-agent continuation/conflict/correction walkthrough](two-agent-workflow.md) to exercise the next part of the user loop. `stop` acknowledges a shutdown request; process exit can follow shortly afterward. Keep the disposable directory for inspection, then remove only that directory when finished. On an uncertain response, inspect the generation after the operation completes; timeout does not cancel integration. For opted-in Linux supervision, [explicit validator recovery](validator-recovery.md) can reclaim a covered interrupted slot from verified cleanup proof. Missing proof and worker death still require preserved-state diagnosis. See [receive and integration](receive.md), [manual validation](manual-validator.md) and [workspace recovery](workspaces.md).

## What is qualified here

Lifecycle tests cover immutable submission generations, retry deduplication, dependency blocking and rejection, protected-change approval states, explicit validation attempts, terminal outcomes and later annotations. A serialized snapshot round trip tests the model's restart contract; it does not prove durable receive acknowledgment or Git landing. Persistence and startup reconciliation now have interruption tests; explicit receive/integration have additional enabled-path tests; these finite tests do not qualify an automatic queue or power-loss durability.

CI runs Rust formatting, Clippy, tests and release builds on Linux, macOS and Windows. A separate bounded Linux job builds the checksum-pinned Git 2.40.0 release and uses Rust 1.85.0 with the locked dependency graph. This minimum combination is not a Windows or macOS qualification. A local Linux pass does not establish hosted native results. See [platform qualification](platform-qualification.md) for capability probes, fallback semantics and evidence limits. See [CLI reference](cli.md), [architecture](architecture.md) and [roadmap](project.md) for the staged delivery plan.

[All docs →](README.md)

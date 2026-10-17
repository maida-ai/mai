# Build the MVP foundation

The old worktree prototype has been retired after scoped design acceptance. The foundation implements a versioned change lifecycle, SQLite persistence, repository validation, authority initialization, local engine serving, independent workspaces and manual trusted validation. Receive and landing remain subsequent work; this is not yet a usable integration queue. Use disposable evaluation repositories throughout MVP development.

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

The version remains `mai 0.0.1`. The current binary exposes initialization, engine administration, workspaces and manual validation; see the [engine guide](engine.md) for `init`, `serve`, `status` and `stop`. Retired commands such as `refresh`, `submit` and `github-sync` reject; there are no legacy aliases or data migrations. Old demos and benchmarks have been removed with the old engine, while experiment harnesses and reviewed evidence remain available.

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
"$MAI_BIN" stop "$DEMO_ROOT/project" --json
printf 'Disposable demo retained at %s\n' "$DEMO_ROOT"
```

Expected results: initialization succeeds, status reports `ready` with `registration_available: false`, Alice and Bob receive independent clones with a `mai` remote, and validation reports `outcome: success`. The imported commit is the unchanged r0 baseline; validation never advances trunk. No bootstrap runs because this example declares none. `stop` acknowledges a shutdown request; process exit can follow shortly afterward. Keep the demo directory for inspection, then remove only that disposable directory when finished.

This proves a small manual foundation workflow, not submission, queue ordering or landing. Git pushes into this authority are rejected. A real gate should check the project's actual tests; [manual validation](manual-validator.md) explains executable trust, approval bindings and bounded failure results. [Workspace guidance](workspaces.md) explains incomplete creations and bootstrap recovery. If a command fails, preserve its JSON and the authority directory; inspect `.mai/engine.log` or use foreground serving rather than deleting durable state.

## What is qualified here

Lifecycle tests cover immutable submission generations, retry deduplication, dependency blocking and rejection, protected-change approval states, explicit validation attempts, terminal outcomes and later annotations. A serialized snapshot round trip tests the model's restart contract; it does not prove durable receive acknowledgment or Git landing. Persistence and startup reconciliation now have interruption tests; receive and landing still require their own integration tests before enablement.

CI runs Rust formatting, Clippy, tests and release builds on Linux, macOS and Windows. A separate bounded Linux job builds the checksum-pinned Git 2.40.0 release and uses Rust 1.85.0 with the locked dependency graph. This minimum combination is not a Windows or macOS qualification. A local Linux pass does not establish hosted native results. See [platform qualification](platform-qualification.md) for capability probes, fallback semantics and evidence limits. See [CLI reference](cli.md), [architecture](architecture.md) and [roadmap](project.md) for the staged delivery plan.

[All docs →](README.md)

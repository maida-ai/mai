# mai

**SVN-inspired revision semantics, agent-native concurrency, Rust implementation, Git-backed storage.** `mai` belongs to the maida-ai family; the proposed upstream is `maida-ai/mai`. The project is Apache-2.0 licensed, entirely open source, self-hostable, and usable independently of Maida or GitHub.

Autonomous agents submit frequent small changes, need deterministic conflicts and attribution, and may create disposable workspaces in large numbers. `mai` exposes an ordered authority (`r0`, `r1`, …) rather than asking each agent to manage branches and rebases. SVN inspires the revision model; this is neither SVN storage nor its wire protocol. Scalability concerns motivating systems such as Scion inform our goals, without claiming to reproduce an undocumented implementation.

## Versioning

`mai` starts at **0.0.1** as an MVP. MVP releases remain in the `0.0.x` series; leaving MVP promotes the project to `0.1.0` (the `0.1` line), and demonstrated stability promotes it to `1.0.0` (the `1.0` line). These are maturity milestones, not automatic dates or feature-count thresholds.

## Implemented in 0.0.1 (MVP)

A native Rust CLI replaces the Python prototype. It supports isolated detached workspaces, path leases, conservative conflict rejection, transplantation of disjoint stale edits, per-agent provenance, Git HEAD import, revision inspection, and optional fast-forward Git remote mirroring. Immutable submission preparation runs outside the SQLite publication gate; publication revalidates the head and leases and uses Git compare-and-swap. Acceptance recovery uses authoritative Git history and auxiliary receipts.

Rust enables native deployment, low-overhead coordination, explicit ownership, and a coherent foundation for future persistent services. It does not eliminate Git subprocess costs, checkout scans, or publication contention. Thousands of concurrent agents are an architectural goal, not demonstrated capacity; large disjoint bursts currently encounter bounded contention rejections. See [ARCHITECTURE.md](ARCHITECTURE.md) and [PERFORMANCE.md](PERFORMANCE.md).

## Requirements and installation

Use Rust 1.85+ with Cargo to build and Git 2.36+ at runtime on a trusted local host (Linux, macOS, or Windows). Bundled SQLite requires a C compiler during the build, but no separate SQLite installation. Python is not required to build, test, benchmark, install, or run this version. CI covers Linux, macOS, and Windows; Windows remains experimental until native CI and filesystem behavior are qualified. Windows builds need Git for Windows and C build tools (MSVC for the native MSVC target).

```sh
cargo build --release --locked --bin mai
cargo install --path . --locked --bin mai
mai --help
bash examples/demo.sh
# On Windows PowerShell:
# ./examples/demo.ps1
```

Keep the authority and SQLite metadata on one local filesystem with working native file locks. Do not place it on NFS/SMB or expose it to untrusted tenants.

## Quick start

```sh
mai init /tmp/mai-project
# Or import an existing Git HEAD snapshot and its history:
# mai init /tmp/mai-project --from /path/to/git-repository
mai workspace /tmp/mai-project frontend
mai workspace /tmp/mai-project backend
mai lease /tmp/mai-project frontend web/ --ttl 600
mkdir -p /tmp/mai-project/workspaces/frontend/web
printf 'hello UI\n' > /tmp/mai-project/workspaces/frontend/web/index.txt
printf 'hello API\n' > /tmp/mai-project/workspaces/backend/api.txt
mai --json submit /tmp/mai-project frontend -m 'Add UI'
mai --json submit /tmp/mai-project backend -m 'Add API' --check 'test -f api.txt'
mai history /tmp/mai-project
mai show /tmp/mai-project r1 web/index.txt
```

The backend remains based on r0 but its independent path can become r2 without manual rebasing. `--check` runs an explicitly supplied, trusted local shell command before preparation; it does not validate the eventual integrated trunk. Checks use `sh -c` on Unix and `cmd.exe /D /S /C` on Windows, so supply platform-appropriate shell syntax.

## CLI reference

- `init PROJECT [--from GIT_REPO]`: create an empty authority or import Git HEAD as r0.
- `workspace PROJECT AGENT`: create an agent's detached full Git worktree.
- `refresh PROJECT AGENT [--discard]`: update a clean workspace. Explicit `--discard` destroys edits and nonignored untracked files.
- `lease PROJECT AGENT PATH... [--ttl SEC]`: reserve file/directory paths, default 900 seconds, allowed range 1–86400.
- `release PROJECT AGENT [PATH...]`: release exact reservations or all owned leases.
- `status PROJECT`: inspect head, revision, registered workspaces, and active leases.
- `diff PROJECT AGENT`: stage all changes and print a unified diff.
- `submit PROJECT AGENT -m TEXT [--check CMD]`: prepare changes, reject overlap/foreign leases, publish one revision.
- `history PROJECT [-n COUNT]`: newest revisions, including at most the import snapshot r0.
- `show PROJECT rN PATH`: print exact file bytes; `HEAD` is also supported.
- `github-link PROJECT URL`: configure a Git-compatible downstream remote named `github`.
- `github-sync PROJECT`: explicitly push a snapshot of accepted main, without force. Remote divergence rejects safely.
- `--json`: return `{"ok":true,"result":...}` or `{"ok":false,"error":"..."}`; errors exit 2. `show` returns base64 bytes. `--version` prints `mai 0.0.1`.

Successful submission fields retain `revision`, `commit`, `agent`, `files`, and `rebased`, adding `recovered`, `workspace_updated`, `retries`, `lock_wait_ms`, and `lock_hold_ms`. A recovered receipt means acceptance already occurred, not that a new revision was created. Contention-budget errors mean that attempt did not publish; retry the same submission after backoff. Clients must check `workspace_updated` before continuing edits from that workspace. See the recovery procedure in [ARCHITECTURE.md](ARCHITECTURE.md).

## Compatibility

Existing Python v0.1.1 `.mai/metadata.sqlite3`, `.mai/repo.git`, workspaces, baseline refs, and history open in place. Early `.agenttrunk` stores and `refs/agenttrunk/base` also remain supported. No metadata schema conversion occurs. New writes add `locks/` files and Git `refs/mai/receipts/AGENT`; these are compatible additions. Stop old clients while changing binaries; mixed Python/Rust operations against the same workspace are unsupported because Python does not honor the new per-agent lock.

Repository paths and tracked filenames must be UTF-8. Unsupported filenames fail before publication; raw file contents remain arbitrary bytes. The Python implementation's surrogate-escaped non-UTF-8 filename behavior is not retained. `~` shell expansion works normally, but literal quoted tilde paths are not expanded by `mai`.

## Limits and direction

Leases coordinate edits and block foreign publication while active; they are not filesystem permissions. Worktrees isolate normal working copies, not arbitrary code. Identity is declared metadata, not authenticated identity. Trusted users can bypass policy by editing Git or SQLite directly. Do not edit a workspace while it is being submitted or refreshed.

There is no daemon, network server, authentication, distributed locking, sparse materialization, automatic lease renewal, or durable downstream job queue. Future `mai daemon` and `mai server` modes will share the core engine; no placeholder commands ship. GitHub remains optional, synchronous on explicit request, and downstream of local acceptance. No Maida production assets were migrated. [MAIDA_MIGRATION.md](MAIDA_MIGRATION.md) defines staged adoption gates.

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
cargo run --release --locked --bin mai-bench -- --agents 16 --workers 8
```

See [CONTRIBUTING.md](CONTRIBUTING.md), [AGENTS.md](AGENTS.md), and [ADRs](docs/adr/0001-foundation.md).

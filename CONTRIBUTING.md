# Contributing

`mai` is an Apache-2.0 Rust project in the maida-ai family, independent of Maida infrastructure and proprietary services. Start with [Architecture](docs/architecture.md) and [AGENTS.md](AGENTS.md). Keep changes focused on agent version control, not agent orchestration.

## Local workflow

Install Rust 1.85+, a C compiler for bundled SQLite, and Git 2.36+ on Linux/macOS/Windows (Git for Windows and MSVC C build tools for native Windows builds). No Python runtime is required.

```sh
cargo build --locked --bin mai
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
cargo build --release --locked --bins
bash examples/demo.sh
# Windows PowerShell: ./examples/demo.ps1
```

CI runs formatting, Clippy, tests, release builds, a native-platform demo, and a CLI benchmark smoke test on Linux, macOS, and Windows. Unix executable-mode/process-control/symlink tests run on Unix; Linux additionally tests raw invalid filename bytes, which APFS cannot create. A filesystem-independent unit test checks invalid Git path bytes on every platform. Common conflict, lease, binary/rename, recovery-state, hook suppression, and separate-process publication coverage runs on all platforms.

Real-Git tests use disposable repositories and local bare remotes, never credentials or customer repositories. Preserve compatibility fixtures and add success/error regressions for semantics changes. Concurrent changes need disjoint, contended, same-workspace, and separate-process coverage as appropriate. Durability changes require interruption-state and recovery tests; state simulation alone does not certify power-loss safety.

## Design and performance contributions

Use [capability IDs C01–C12](docs/agent-native-capabilities.md) to connect source-service work to concrete completion gates. Read [ADR 0002](docs/adr/0002-changesets-and-publication.md) before adding checkpoint or remote APIs. A network wrapper around today's CLI does not complete checkout-free writes, indexed queries, sparse clients, or cache topology.

Document behavioral changes in README and authoritative architecture; record significant alternatives in ADRs. Distinguish implemented capabilities, measured observations, targets, and plans. Preserve revision ordering, r0, conservative component overlap, active foreign lease enforcement, agent attribution, and Git CAS. Do not weaken correctness to increase throughput or build a new object database without evidence.

Run the native workload in [Performance](docs/performance.md) for performance changes; record hardware, filesystem, build profile, Git/Rust versions, worker/fanout settings, cold/warm conditions, rejection rates, and raw output. Compare equivalent modes; Git subprocesses and disk work often dominate pure Rust computation. Add production dependencies only for concrete uses; review current upstream documentation and licensing, pin reproducibly with Cargo.lock, and avoid unnecessary runtime services.

## Versioning

The current MVP starts at `0.0.1`; increment `0.0.x` during MVP development. Promote to `0.1.0` when leaving MVP and to `1.0.0` when stable. Keep Cargo package metadata, CLI output tests, and current documentation consistent; benchmark labels derive from Cargo metadata. Preserve original version labels in historical benchmark records and compatibility fixtures. See [version policy](docs/project.md#versioning).

## Security and release boundaries

Do not deploy this trusted-local milestone to hostile tenants. Never claim worktrees are sandboxes, leases are OS permissions, or declared agent names are authenticated identities. Do not run repository hooks or checks automatically, expose secrets, force-push customer remotes, migrate Maida assets, deploy, or publish packages without explicit authorization. Report security concerns privately to maintainers when a public contact is established. External contributors must receive the same open core as Maida users.

# Contributing

`mai` is an Apache-2.0 Rust project in the maida-ai family, independent of Maida infrastructure and proprietary services. Start with [Architecture](docs/architecture.md) and [AGENTS.md](AGENTS.md). Focus on coordination for 3–30 local coding agents sharing one repository.

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

Real-Git tests use disposable repositories and local bare remotes, never credentials or customer repositories. Add success/error regressions for current semantics. Concurrent changes need disjoint, contended, same-workspace, and separate-process coverage as appropriate. Durability changes require interruption-state and recovery tests; state simulation alone does not certify power-loss safety.

## Design and performance contributions

Start with the local workflow in [project direction](docs/project.md): ordinary Git use, dirty updates, integrated validation, truthful recovery and environment setup. [ADR 0002](docs/adr/0002-changesets-and-publication.md) permits Git-native checkpoints and serialized candidate assembly. C01–C12 are deferred ideas, not a checklist every contribution must satisfy.

Document behavioral changes in README and authoritative architecture; record significant alternatives in ADRs. Distinguish implemented capabilities, measured observations, targets, and plans. Preserve revision ordering, r0, conservative component overlap, active foreign lease enforcement, agent attribution, and Git CAS. Do not weaken correctness to increase throughput or build a new object database without evidence.

Run the native workload in [Performance](docs/performance.md) for performance changes; record hardware, filesystem, build profile, Git/Rust versions, worker/fanout settings, cold/warm conditions, rejection rates, and raw output. Compare equivalent modes; Git subprocesses and disk work often dominate pure Rust computation. Add production dependencies only for concrete uses; review current upstream documentation and licensing, pin reproducibly with Cargo.lock, and avoid unnecessary runtime services.

## Development policy

This project has no existing users and no backward-compatibility obligations. Repository formats, metadata, CLI syntax, and JSON/protocol schemas may change freely during MVP development. Do not add legacy readers, deprecated aliases, compatibility fixtures, format converters, or downgrade paths. Recreate disposable test repositories when the current format changes; document the change and update current tests/examples. This policy does not relax data integrity, acknowledged-write recovery, or the requirement to preserve uncommitted user changes.

## Versioning

The current MVP starts at `0.0.1`; increment `0.0.x` during MVP development. Promote to `0.1.0` when leaving MVP and to `1.0.0` when stable. Keep Cargo package metadata, CLI output tests, and current documentation consistent; benchmark labels derive from Cargo metadata. Preserve original version labels in historical benchmark records. See [version policy](docs/project.md#versioning).

## Security and release boundaries

Do not deploy this trusted-local milestone to hostile tenants. Never claim worktrees are sandboxes, leases are OS permissions, or declared agent names are authenticated identities. Do not run repository hooks or checks automatically, expose secrets, force-push customer remotes, migrate Maida assets, deploy, or publish packages without explicit authorization. Report vulnerabilities through [GitHub private reporting](https://github.com/maida-ai/mai/security/advisories/new). External contributors must receive the same open core as Maida users.

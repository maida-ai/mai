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

## Rewrite entry conditions

The current 0.0.1 engine remains buildable during M0. Do not start replacement product code until S1–S7 pass their go criteria or failed assumptions are redesigned in an accepted ADR. The [project plan](docs/project.md) and [architecture](docs/architecture.md) distinguish decided direction, spike-pending mechanisms and maintainer decisions. When M1 starts, replace the old source, tests, benchmarks and examples; preserve licensing, assets, contribution guidance and the CI skeleton. Do not preemptively delete executable code during documentation work.

The target is a plain-Git receive protocol, single-writer engine and integrated validation queue for 3–30 agents. Keep Git CLI merge/ref correctness, exact pipeline evidence, durable registration, typed terminal outcomes, protected trunk policy, dependent changes and safe explicit sync central. Batch-tip evidence must not be misrepresented as a separate pass on each intermediate revision. Do not carry over linked-worktree base refs, publication gates or group-commit optimization into the rewrite.

M0 scripts and short qualification reports go under `docs/spikes/` when executed. Record platform/tool versions, repository corpus, warm/cold state and reproducible raw results. Use the recovery and time-to-land metrics in [project direction](docs/project.md#success-measurements); no unmeasured performance claim qualifies an architecture. Add dependencies only for concrete uses with maintainer approval, license review and reproducible locking. gitoxide reads require profiling evidence; overlay materialization requires checkout-cost evidence.

Document significant alternatives in ADRs. Treat all new commands, configuration and harness snippets as planned until shipped. Models may add conflicts or escalate, never accept, merge or approve; uncertainty does not pass a gate. Preserve current checks until the replacement test harness proves its invariants. Do not start M2 before external-user qualification without explicit override.

## Development policy

This project has no existing users and no backward-compatibility obligations. Repository formats, metadata, CLI syntax, and JSON/protocol schemas may change freely during MVP development. Do not add legacy readers, deprecated aliases, compatibility fixtures, format converters, or downgrade paths. Recreate disposable test repositories when the current format changes; document the change and update current tests/examples. This policy does not relax data integrity, acknowledged-write recovery, or the requirement to preserve uncommitted user changes.

## Versioning

The current MVP starts at `0.0.1`; increment `0.0.x` during MVP development. Promote to `0.1.0` when leaving MVP and to `1.0.0` when stable. Keep Cargo package metadata, CLI output tests, and current documentation consistent; benchmark labels derive from Cargo metadata. Preserve original version labels in historical benchmark records. See [version policy](docs/project.md#versioning).

## Security and release boundaries

Do not deploy this trusted-local milestone to hostile tenants. Never claim worktrees are sandboxes, leases are OS permissions, or declared agent names are authenticated identities. Do not run repository hooks or checks automatically, expose secrets, force-push customer remotes, migrate Maida assets, deploy, or publish packages without explicit authorization. Report vulnerabilities through [GitHub private reporting](https://github.com/maida-ai/mai/security/advisories/new). External contributors must receive the same open core as Maida users.

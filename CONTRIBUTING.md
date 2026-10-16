# Contributing

`mai` is an Apache-2.0 Rust project in the maida-ai family, independent of Maida infrastructure and proprietary services. Start with [Architecture](docs/architecture.md) and [AGENTS.md](AGENTS.md). Focus on coordination for 3–30 local coding agents sharing one repository.

## Local workflow

Install Rust 1.85+, a C compiler for bundled SQLite, and Git 2.40+ on Linux/macOS/Windows (Git for Windows and MSVC C build tools for native Windows builds). No Python runtime is required.

```sh
cargo build --locked --bin mai
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
cargo build --release --locked --bins
```

CI runs formatting, Clippy, tests and release builds on Linux, macOS and Windows. The old engine, demonstrations and benchmark executable have been retired; replacement lifecycle regressions cover the enabled foundation. Each later persistence, initialization, receive and landing path needs its own native-platform and interruption tests before enablement.

Real-Git tests use disposable repositories and local bare remotes, never credentials or customer repositories. Add success/error regressions for current semantics. Concurrent changes need disjoint, contended, same-workspace, and separate-process coverage as appropriate. Durability changes require interruption-state and recovery tests; state simulation alone does not certify power-loss safety.

## Rewrite entry conditions

The current 0.0.1 engine remains buildable during M0. Follow the accepted [ADR 0010 staging amendment](docs/adr/0010-staged-mvp-qualification.md): fresh native S3 functional evidence and the scoped MVP design are accepted in ADRs 0003/0004; the maintainer-approved implementation plan may begin replacement product code. S4 realistic performance and future S7 coverage are not blanket blockers; actual feature-specific correctness tests pass before enablement. The [project plan](docs/project.md) and [architecture](docs/architecture.md) distinguish decided direction, spike-pending mechanisms and maintainer decisions. When M1 starts, replace the old source, tests, benchmarks and examples; preserve licensing, assets, contribution guidance and the CI skeleton. Do not preemptively delete executable code during documentation work.

The target is a plain-Git receive protocol, single-writer engine and integrated validation queue for 3–30 agents. Keep Git CLI merge/ref correctness, exact pipeline evidence, durable registration, typed terminal outcomes, protected trunk policy, dependent changes and safe explicit sync central. M1 adds file-unit manifests; differentiated priority policy awaits RFC #93. M2 refines objects rather than trusting agent-declared scope. Before any later priority feature is enabled, validate grant expiry, inheritance, aging, preemption grace and preservation of lower-priority work. Bounded fix-forward requires normal gate plus full confirmation; a validated revert remains the backstop. Do not implement a narrower severe gate without the explicit maintainer decision. Batch-tip evidence must not be misrepresented as a separate pass on each intermediate revision. Do not carry over linked-worktree base refs, publication gates or group-commit optimization into the rewrite.

M0 harnesses and experiment setups go under `experiments/`; short qualification reports and selected evidence go under `docs/spikes/`. Experiments run only on explicit local invocation or manual CI dispatch, never automatically on pushes, PRs or schedules. Record platform/tool versions, repository corpus, warm/cold state and reproducible raw results. Use the recovery and time-to-land metrics in [project direction](docs/project.md#success-measurements); no unmeasured performance claim qualifies an architecture. Add dependencies only for concrete uses with maintainer approval, license review and reproducible locking. gitoxide reads require profiling evidence; overlay materialization requires checkout-cost evidence.

Document significant alternatives in ADRs. Treat all new commands, configuration and harness snippets as planned until shipped. Models may add conflicts or escalate, never accept, merge or approve; uncertainty does not pass a gate. Preserve current checks until the replacement test harness proves its invariants. Do not start M2 before external-user qualification without explicit override.

## Development policy

This project has no existing users and no backward-compatibility obligations. Repository formats, metadata, CLI syntax, and JSON/protocol schemas may change freely during MVP development. Do not add legacy readers, deprecated aliases, compatibility fixtures, format converters, or downgrade paths. Recreate disposable test repositories when the current format changes; document the change and update current tests/examples. This policy does not relax data integrity, acknowledged-write recovery, or the requirement to preserve uncommitted user changes.

## Versioning

The current MVP starts at `0.0.1`; increment `0.0.x` during MVP development. Promote to `0.1.0` when leaving MVP and to `1.0.0` when stable. Keep Cargo package metadata, CLI output tests, and current documentation consistent; historical experiment records retain their original version labels. See [version policy](docs/project.md#versioning).

## Security and release boundaries

Do not deploy this trusted-local milestone to hostile tenants. Never claim worktrees are sandboxes, leases are OS permissions, or declared agent names are authenticated identities. Do not run repository hooks or checks automatically, expose secrets, force-push customer remotes, migrate Maida assets, deploy, or publish packages without explicit authorization. Report vulnerabilities through [GitHub private reporting](https://github.com/maida-ai/mai/security/advisories/new). External contributors must receive the same open core as Maida users.

Discourage premature optimization. Use a simple dependency-aware single-class serial reference path first, then opt-in local dogfood observations. Require a measured practical problem before adding speculative concurrency, batching or materialization complexity; do not claim realistic performance or starvation guarantees from synthetic fixtures.

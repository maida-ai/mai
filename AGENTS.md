# Agent instructions for contributing to `mai` (maida-ai family)

This repository is an open-source Rust CLI for a local centralized Git-backed source control authority. Keep the design **small, deterministic, and scriptable**. [ARCHITECTURE.md](ARCHITECTURE.md) is authoritative; [docs/adr/0001-foundation.md](docs/adr/0001-foundation.md) records decisions and [PERFORMANCE.md](PERFORMANCE.md) defines measurement.

* Follow the version policy in README.md and CONTRIBUTING.md: MVP begins at `0.0.1` and uses `0.0.x`; promote to `0.1.0` after MVP and `1.0.0` at stability. Preserve historical fixture/measurement labels.
* Use Rust first; do not restore Python runtime/packaging. Read source, Cargo.toml, Cargo.lock, nearby tests, and all relevant documentation before editing. Preserve uncommitted user changes.
* Do not introduce network services, dependencies, databases, or LLM calls without a concrete use case and tests. Do not build a custom storage engine or elaborate plugin system without measured need.
* Never force-push to a customer's GitHub remote. Treat the bare authority's `main` branch as the only accepted writer. `r0` is always the original baseline snapshot.
* Maintain deterministic `--json` success/failure output for any new CLI action.
* Preserve non-overlapping submission semantics, path-component overlap checks, Git compare-and-swap, and per-agent attribution. Avoid anything that silently overwrites other agents' work.
* Add regression tests covering success and failure for significant changes. Prefer a failing regression before fixing bugs. Run `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, and `cargo test --all-targets --locked` before finalizing.
* Publication changes require separate-process disjoint and contended tests, same-workspace coordination tests, and relevant interrupted-state recovery tests. Preserve Git main as authoritative; SQLite and Git are not one transaction. Keep preparation and checkout reset outside the publication gate.
* Preserve `.mai` and `.agenttrunk` compatibility and Python-generated fixtures; never silently convert existing metadata. Reject unsupported input before publication. Document any changed compatibility expectation.
* Never imply a local Git worktree is an OS sandbox or that an advisory lease enforces filesystem permissions.
* Optimize for high-fanout agent clients, but do not present unmeasured throughput as established. Run reproducible native benchmarks for performance changes and explain bottlenecks beyond language overhead.
* Keep the protocol and working implementation entirely open source and self-hostable; GitHub remains optional. Daemon/server modes are planned, not implemented; do not publish nonfunctional commands.
* Never migrate production Maida assets implicitly; follow MAIDA_MIGRATION.md.
* Update relevant Markdown and ADRs when architecture, behavior, installation, or limitations change. Write each prose paragraph on one source line; preserve meaningful Markdown structure and unrelated content.

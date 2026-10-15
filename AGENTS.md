# Agent instructions for contributing to `mai` (maida-ai family)

This repository is an open-source Rust CLI for a local centralized Git-backed source control authority. Keep the design **small, deterministic, and scriptable**. [Architecture](docs/architecture.md) is authoritative; [project direction](docs/project.md) prioritizes 3–30 local agents; [agent-native direction](docs/agent-native-capabilities.md) defines planned work and the external-user gate; fleet-scale futures are not completion gates; [docs/adr/0001-foundation.md](docs/adr/0001-foundation.md) records decisions and [Performance](docs/performance.md) defines measurement.

* Follow the version policy in docs/project.md and CONTRIBUTING.md: MVP begins at `0.0.1` and uses `0.0.x`; promote to `0.1.0` after MVP and `1.0.0` at stability. Preserve historical measurement labels.
* Use Rust first; do not restore Python runtime/packaging. Read source, Cargo.toml, Cargo.lock, nearby tests, and all relevant documentation before editing. Preserve uncommitted user changes.
* Do not introduce network services, dependencies or databases without a concrete use case and tests. Do not call models in core or build a custom storage engine/plugin system without measured need. Follow the planned design rules below; none implies a feature is implemented.
* Never force-push to a customer's GitHub remote. Treat the bare authority's `main` branch as the only accepted writer. `r0` is always the original baseline snapshot.
* Maintain deterministic `--json` success/failure output for any new CLI action.
* Preserve Git compare-and-swap, live lease enforcement, per-agent attribution and work preservation. Conservative path/component conflict rejection remains the default; tested opt-in textual/structural policies may use finer units. Never silently overwrite accepted work.
* Add regression tests covering success and failure for significant changes. Prefer a failing regression before fixing bugs. Run `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, and `cargo test --all-targets --locked` before finalizing.
* Publication changes require separate-process disjoint and contended tests, same-workspace coordination tests, and relevant interrupted-state recovery tests. Preserve Git main as authoritative; SQLite and Git are not one transaction. Keep staging/hashing, long checks, checkout reset and environment bootstrap outside the publication gate. Candidate assembly may move inside it to avoid wasted retries; required integrated checks must still bind to the exact accepted tree and parent.
* This new MVP has no backward-compatibility requirements for repository formats, metadata, CLI syntax, or JSON/protocol schemas. Prefer the simplest current design; do not add legacy readers, deprecated aliases, migration shims, or compatibility fixtures. Document breaking changes. Reject unsupported input before publication.
* Never imply a local Git worktree is an OS sandbox or that an advisory lease enforces filesystem permissions.
* Only accepted publication allocates `rN`. Prefer Git commits/refs for private checkpoints and explicit workspace bases; prove retention and recovery before promising durability. Follow revised ADR 0002; no custom changeset/query protocol is prerequisite for local workflow fixes.
* Optimize for 3–30 agents on one machine and one repository. Measure integrated correctness, environment setup, useful throughput and tails; use large fanout as stress testing, not a capacity promise. Run reproducible native benchmarks for performance changes.
* Keep the protocol and working implementation entirely open source and self-hostable; GitHub remains optional. Daemon/server modes are deferred possibilities, not implemented; do not publish nonfunctional commands.
* Never migrate production Maida assets implicitly; follow docs/maida-migration.md.
* Update relevant Markdown and ADRs when architecture, behavior, installation, or limitations change. Write each prose paragraph on one source line; preserve meaningful Markdown structure and unrelated content.

## Planned agent-work-unit rules

* Use the same unit for leases, conflicts and merging: symbol, structured-config key path, or path. Report unsupported/generated/parse-failure fallback explicitly; fallback must not weaken broader leases.
* Models may only add conflicts, require review or return `UNCERTAIN`. Never ship a model-backed path that accepts, merges, declares equivalence or approves; `UNCERTAIN` never passes a gate. External checks are opt-in and outside core.
* Lease/intent requests return promptly with holder, intent and expiry rather than waiting for another agent. Enforcement occurs at publication; this does not promise zero processing/lock latency.
* Share bounded structured intent and deltas as untrusted input, never raw agent context through awareness/conflict APIs. [ADR 0008](docs/adr/0008-context-checkpoints.md) proposes opt-in harness context; access/handoff, retention and size defaults remain undecided.
* Assign exactly one resolver deterministically: later lander by default, valid symbol-lease holder over non-holder. No negotiation protocol or automatic agent supervision.
* mai stores, binds, validates and publishes. It does not run/schedule agents, plan tasks, call models in core, or compress/summarize/redact context; harnesses own context preparation.
* Validate the exact integrated tree and parent that lands. A proposed parent-only policy cannot satisfy required integrated validation. Keep current workspace-only `--check` limitations explicit until fixed.
* Support local commits as private checkpoints through explicit base refs and authority guards, following ADR 0003. Current agent prompts must retain Git-use precautions until those fixes ship.
* Do not start M2 before 5–10 external users exercise real repositories and the maintainer accepts the users gate, unless the maintainer explicitly overrides it. Do not choose pending tagline, untracked-file, parser-language or context defaults on the maintainer's behalf.

# mai documentation

The current 0.0.1 code is the replacement MVP foundation. The old worktree engine is retired. Scoped M0 acceptance is complete under ADR 0010, with explicit trusted-local receive and serial integration of a named generation now available. This is a scoped workflow, not the full automatic integration queue; each enabled path requires its own correctness tests.

- [Current delivery and next decisions](release-status.md): implemented user loop, qualification limits and unfinished milestones.

## Evaluate the current binary

- [Getting started](getting-started.md): build and land two dependent changes in short steps.
- [Failed gate and correction](check-failure.md): distinguish a successful push from acceptance and preserve rejected work.
- [Benchmark protocol](benchmarking.md): reproducible comparisons, recovery metrics and evidence limits.
- [Receive and integration](receive.md): durable receipt, named-generation integration, limits and recovery.
- [Two-agent workflow](two-agent-workflow.md): continue later work, diagnose overlap and submit an explicit correction.
- [Continue after landing](continuation.md): replay a clean local suffix into a new workspace while preserving the original clone.
- [Validator recovery](validator-recovery.md): opt-in Linux supervision, explicit proof-backed reclaim and refusal cases.
- [CLI reference](cli.md): current command behavior, safety limitations and a separate proposed interface outline.
- [Agent integration](agents.md): current precautions followed by clearly labeled planned harness instructions.

## Design and contribute

- [Philosophy](philosophy.md): fast recovery with a protected trunk boundary.
- [Architecture](architecture.md): proposed Git receive path, single writer, integrated validation and forge topology.
- [Project direction](project.md): M0 through M6, qualification, open decisions and metrics.
- [Configuration](configuration.md): proposed `mai.toml`, bounded recovery, priority settings and engine-local boundaries.
- [Operations](operations.md): proposed lifecycle, bounded fix-forward, manifest reconciliation, priority operations, backup and retention.
- [ADRs](adr/): architecture decisions and explicitly superseded worktree-era assumptions.
- [Staged MVP qualification](adr/0010-staged-mvp-qualification.md): correctness gates, deferred performance research and priority policy.
- [Contributing](../CONTRIBUTING.md): current checks and rewrite entry conditions.
- [Branding](branding.md): symbol and visual assets.
- [Maida adoption](maida-migration.md): separate production adoption boundary; no automatic migration.

[Back to the project](../README.md)

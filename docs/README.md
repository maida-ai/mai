# mai documentation

Start with one working submission, then add the parts your workflow needs.

## Use mai

- [Getting started](getting-started.md): install, create a workspace, submit, and inspect your first revision. Includes Windows instructions and Git import/export.
- [CLI reference](cli.md): commands, JSON responses, checks, leases, conflict handling, and recovery.
- [Agent integration prompt](../examples/agent-prompt.md): a starting contract for your agent harness.
- [Development policy](project.md#development-policy): a new MVP with no backward-compatibility obligations.

## Understand and contribute

- [Agent-native direction](agent-native-capabilities.md): symbols, intent, context and draft plans, user gate, unresolved decisions and a fleet-scale futures appendix.
- [Project status and roadmap](project.md): local-agent priorities, current limits, and versioning.
- [Architecture](architecture.md): authoritative revision, concurrency, storage, security, and durability decisions.
- [Performance](performance.md): methodology, recorded findings, and remaining bottlenecks.
- Architecture decisions: [Rust/Git foundation](adr/0001-foundation.md) and [private work versus publication](adr/0002-changesets-and-publication.md).
- [Contributing](../CONTRIBUTING.md): local builds, tests, platform coverage, and contribution expectations.
- [Branding](branding.md): the curly-brace symbol, palette, provenance, and usage.
- [Maida adoption plan](maida-migration.md): staged evaluation and production migration gates.

[Back to the project →](../README.md)

## Architecture decisions

ADRs 0003–0009 are proposed and unimplemented; open maintainer choices remain explicit.

- [0003: Workspace bases, authority guard and local checkpoints](adr/0003-workspace-base-and-ref-guard.md)
- [0004: Exact integrated validation and proposed parent policy](adr/0004-integrated-candidate-validation.md)
- [0005: Serialized construction and performance batching](adr/0005-serialized-publication-and-batching.md)
- [0006: Symbols, key paths, fallback and model trust](adr/0006-units-of-change.md)
- [0007: Intent, activity and deterministic resolver ownership](adr/0007-intent-and-conflict-ownership.md)
- [0008: Opt-in context, retention and unresolved handoff access](adr/0008-context-checkpoints.md)
- [0009: Draft DAG and atomic group landing](adr/0009-draft-dag-and-group-landing.md)

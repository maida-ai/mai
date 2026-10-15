# mai documentation

The current code is the experimental 0.0.1 worktree engine. The rewrite design is unimplemented and must pass M0 qualification before product code begins.

## Evaluate the current binary

- [Getting started](getting-started.md): disposable-repository installation and current commands.
- [CLI reference](cli.md): current command behavior, safety limitations and a separate proposed interface outline.
- [Agent integration](agents.md): current precautions followed by clearly labeled planned harness instructions.

## Design and contribute

- [Philosophy](philosophy.md): fast recovery with a protected trunk boundary.
- [Architecture](architecture.md): proposed Git receive path, single writer, integrated validation and forge topology.
- [Project direction](project.md): M0 through M6, qualification, open decisions and metrics.
- [Configuration](configuration.md): proposed `mai.toml`, bounded recovery, priority settings and engine-local boundaries.
- [Operations](operations.md): proposed lifecycle, bounded fix-forward, manifest reconciliation, priority operations, backup and retention.
- [ADRs](adr/): architecture decisions and explicitly superseded worktree-era assumptions.
- [Contributing](../CONTRIBUTING.md): current checks and rewrite entry conditions.
- [Branding](branding.md): symbol and visual assets.
- [Maida adoption](maida-migration.md): separate production adoption boundary; no automatic migration.

[Back to the project](../README.md)

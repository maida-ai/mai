# Possible futures

**Status: deferred exploration, not a capability contract or release gate.** The earlier C01–C12 plan targeted a fleet-scale source service. Current delivery focuses on reliable coordination for 3–30 agents on one machine and one repository. [Architecture](architecture.md) describes today's implementation; [project direction](project.md#roadmap) describes the immediate work.

The original reference was Anthropic's [Redesigning source control at Anthropic](https://www.anthropic.com/engineering/redesigning-source-control). mai is independently designed and Apache-2.0; this reference does not establish implementation parity, affiliation, or a requirement to reproduce that system.

## Ideas retained for a demonstrated need

- **C01/C03 — Checkout-free operations and source queries:** consider a bounded API when a real client cannot use the local filesystem or Git. Do not require it for integrated validation.
- **C02 — Private checkpoints:** support ordinary local Git commits first. Define retention through reset, workspace retirement and garbage collection before promising durable checkpoints. A custom changeset encoding is not required for that workflow.
- **C04 — Derived indexes:** batch Git queries and measure history costs before adding persistent indexes. Any index must be rebuildable from authoritative source.
- **C05 — Selective materialization:** measure dependency bootstrap and warmed workspaces first. Sparse/lazy files need a workload that justifies their deletion, hydration and platform complexity.
- **C06/C07 — Daemons and cache tiers:** require measured repeated work and a bounded cache/lifecycle design. Direct local operation remains useful without a service.
- **C08 — Queued publication:** start with serialized candidate construction. Evaluate bounded group commit separately, with explicit request identity and Git/SQLite crash reconciliation.
- **C09 — Remote authorization:** only becomes a product requirement if a shared remote service is chosen. It must cover source, metadata and caches; local worktrees are not a substitute for isolation.
- **C10 — Review/revert/bisect integrations:** exact integrated-tree validation is immediate local work. Rich review APIs and remote attestations are optional extensions.
- **C11 — Operational qualification:** qualify recovery for the actual local workflow now. Distributed fault simulation follows a deployed distributed design, not the other way around.
- **C12 — Storage beyond one host:** consider partitioning, replication or another backend only after measurements establish a need and export/restore behavior is proven.

These IDs preserve context for older design discussions. They are not a required implementation sequence, a promise of future services, or completion criteria for the local MVP. Git objects, refs and worktrees remain the starting point; no proprietary service or core Maida integration is required.

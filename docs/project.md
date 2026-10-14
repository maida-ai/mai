# Project status and direction

`mai` is an Apache-2.0, agent-native version control system in the maida-ai family. Its upstream is `maida-ai/mai`. Any individual or organization can use it without Maida infrastructure, a GitHub account, or proprietary services. Maida is an early adopter, not a runtime dependency; no production assets migrate implicitly.

## Working today

The Rust CLI supports detached agent workspaces, ordered revisions, conservative conflicts, disjoint stale submission, time-limited path leases, per-agent provenance, Git HEAD import, revision inspection, JSON responses, recovery receipts, and optional fast-forward Git mirroring. Preparation runs outside the publication gate; head/lease revalidation and Git compare-and-swap protect publication.

SVN inspires the revision model: `r0` is the original snapshot, then accepted changes become `r1`, `r2`, and onward. mai does not implement SVN storage or wire protocols. Git provides proven content storage and interoperability, while agents interact through workspaces and submissions.

The [agent-native capability contract](agent-native-capabilities.md) defines our Apache-2.0 answer to agent-scale source control: checkout-free operations, immutable checkpoints, indexed source queries, sparse workspaces, reusable caches, authenticated publication, and open integration contracts. Public reference categories are recorded there; implementation and operational parity are not claimed. Rust supports native deployment, resource ownership, and future persistent services. It does not itself remove Git subprocesses, filesystem scans, full checkout costs, or lock contention. See [architecture](architecture.md) and [measurements](performance.md).

## Current limits

This is an MVP for trusted local workflows on one machine and a local filesystem. Workspaces are not sandboxes. Declared agent IDs are not authenticated identities. Leases block foreign publication through supported operations but do not enforce OS permissions. Administrators with filesystem access can bypass Git/SQLite policy.

There is no daemon, server, authentication, distributed locking, sparse materialization, automatic lease renewal, or durable downstream job queue. Windows CI coverage is experimental pending native qualification. Do not expose the authority to untrusted tenants or use NFS/SMB. Quiesce a workspace while submitting or refreshing it.

Large independent bursts can exhaust the bounded contention budget; rejected attempts need client backoff/retry. Accepted history remained intact in recorded samples, but thousand-agent capacity is not established. Crash-state/process-kill tests exist; power-loss, disk-full, interrupted initialization, and backup restoration still need qualification. `--check` tests the agent's workspace, not the eventual integrated trunk. Details and operator recovery are in [architecture](architecture.md#durability-and-recovery).

## Roadmap

Capabilities below refer to [C01–C12](agent-native-capabilities.md). Each milestone needs running code, success/error regressions, recovery/resource-limit evidence, and documentation before completion. Milestones do not determine version promotion automatically.

1. **Reliable local foundation (C08/C11/C12):** qualify acknowledgement/crash/restore/GC behavior, define fair bounded admission, and measure full completion/rejection under contention. Retain source export and prove r0 and publication correctness in the current format.
2. **Source/query contracts (C01/C03/C04):** extract typed engine operations for file/tree reads, revision diffs, ancestry, merge bases, exact revision lookup, and bounded results. Demonstrate metadata-only jobs without worktrees using the Git adapter before choosing remote transport.
3. **Durable changesets and API-native writes (C01/C02/C08):** add immutable candidate recording, scoped request-key outcomes, put/delete/mode requests, and controlled landing. Checkpoints must leave main unchanged and remain protected from GC. Choose the simplest tested format; no prior format needs an upgrade path. Conservative conflicts remain the default.
4. **Selective clients and shared local state (C05/C06/C07):** add sparse ordinary-file workspaces, lifecycle descriptors, incremental detection with fallback, exact object requests, and a bounded optional daemon cache. Test daemon/direct equivalence; qualify lazy files separately per OS.
5. **Secure single-node shared service (C08/C09/C11):** expose those contracts through authenticated bounded transport; enforce repository/directory authorization, lease fencing, audit outcomes, and durable retries. Existing local mode stays available. Do not expose a shell-execution endpoint.
6. **Indexed queries and ecosystem workflows (C03/C04/C10):** build versioned history/blame/search indexes and review/test evidence, compensating reverts, bisect query support, and retryable integration events. Keep review UI, build/test execution, deployment, and agent runners external.
7. **Measured fleet deployment (C07/C11/C12):** qualify optional cache tiers, independent read/index workers, network failure budgets, and migration/restore. Only then evaluate authority failover or another storage adapter under explicit conformance and operational requirements.

No network, checkpoint, query, sparse, daemon, or service API in that sequence ships merely because it appears here. GitHub stays optional and downstream. Git remains the storage foundation until evidence justifies an alternative; distributed consensus and a custom content database are not immediate deliverables.

## Development policy

This is a brand-new project with no users and no backward-compatibility requirements. Repository formats, metadata, CLI syntax, and JSON/protocol schemas may change freely. There are no legacy readers, compatibility fixtures, migration shims, downgrade routes, or promises that an older store/client will work. Recreate disposable evaluation repositories when formats change. Document breaking changes and keep tests/examples aligned with the current implementation.

Data integrity and recovery within the current implementation remain correctness requirements. This policy does not authorize deleting user work or migrating Maida production assets. Git import/export and self-hosting are product features, not historical-format support.

## Versioning

Start at **0.0.1** for the MVP. MVP releases use `0.0.x`; leaving MVP promotes to `0.1.0` (the `0.1` line), and demonstrated stability promotes to `1.0.0` (the `1.0` line). Maturity milestones are not automatic dates or feature-count thresholds. Historical measurement labels remain as originally recorded.

For Maida adoption, follow [the staged migration plan](maida-migration.md). For implementation choices and alternatives, see [ADR 0001](adr/0001-foundation.md).

[All docs →](README.md)

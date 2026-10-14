# Project status and direction

`mai` is an Apache-2.0, agent-native version control system in the maida-ai family. Its upstream is `maida-ai/mai`. Any individual or organization can use it without Maida infrastructure, a GitHub account, or proprietary services. Maida is an early adopter, not a runtime dependency; no production assets migrate implicitly.

## Working today

The Rust CLI supports detached agent workspaces, ordered revisions, conservative conflicts, disjoint stale submission, time-limited path leases, per-agent provenance, Git HEAD import, revision inspection, JSON responses, recovery receipts, and optional fast-forward Git mirroring. Preparation runs outside the publication gate; head/lease revalidation and Git compare-and-swap protect publication.

SVN inspires the revision model: `r0` is the original snapshot, then accepted changes become `r1`, `r2`, and onward. mai does not implement SVN storage or wire protocols. Git provides proven content storage and interoperability, while agents interact through workspaces and submissions.

Scalability concerns in agent systems such as Scion motivate the goals; this project does not claim to recreate an undocumented internal system. Rust supports native deployment, resource ownership, and future persistent services. It does not itself remove Git subprocesses, filesystem scans, full checkout costs, or lock contention. See [architecture](architecture.md) and [measurements](performance.md).

## Current limits

This is an MVP for trusted local workflows on one machine and a local filesystem. Workspaces are not sandboxes. Declared agent IDs are not authenticated identities. Leases block foreign publication through supported operations but do not enforce OS permissions. Administrators with filesystem access can bypass Git/SQLite policy.

There is no daemon, server, authentication, distributed locking, sparse materialization, automatic lease renewal, or durable downstream job queue. Windows CI coverage is experimental pending native qualification. Do not expose the authority to untrusted tenants or use NFS/SMB. Quiesce a workspace while submitting or refreshing it.

Large independent bursts can exhaust the bounded contention budget; rejected attempts need client backoff/retry. Accepted history remained intact in recorded samples, but thousand-agent capacity is not established. Crash-state/process-kill tests exist; power-loss, disk-full, interrupted initialization, and backup restoration still need qualification. `--check` tests the agent's workspace, not the eventual integrated trunk. Details and operator recovery are in [architecture](architecture.md#durability-and-recovery).

## Roadmap

1. Qualify interruption, backup/restore, and filesystem failure recovery.
2. Improve fair admission, backpressure, observability, and retry amplification with measured workloads.
3. Define a versioned agent protocol and add a tested optional local daemon for persistent caches and incremental tracking.
4. Add an authenticated, bounded single-node shared server for multi-machine agents, keeping the engine independent of transport.
5. Optimize workspace materialization/native Git operations where profiles justify it; add a durable downstream outbox.

These are planned milestones, not commands or guarantees in the current executable. GitHub stays optional and downstream. A custom object database, distributed consensus system, or agent orchestration framework is not an initial goal.

## Versioning

Start at **0.0.1** for the MVP. MVP releases use `0.0.x`; leaving MVP promotes to `0.1.0` (the `0.1` line), and demonstrated stability promotes to `1.0.0` (the `1.0` line). Maturity milestones are not automatic dates or feature-count thresholds. Historical fixture/measurement labels remain as originally recorded.

For Maida adoption, follow [the staged migration plan](maida-migration.md). For implementation choices and alternatives, see [ADR 0001](adr/0001-foundation.md).

[All docs →](README.md)

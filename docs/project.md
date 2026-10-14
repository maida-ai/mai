# Project status and direction

`mai` is an Apache-2.0 coordination tool for local coding agents in the maida-ai family. Its upstream is `maida-ai/mai`. Any individual or organization can use it without Maida infrastructure, a GitHub account, or proprietary services. Maida is an early adopter, not a runtime dependency; no production assets migrate implicitly.

## Working today

The Rust CLI supports detached agent workspaces, ordered revisions, conservative conflicts, disjoint stale submission, time-limited path leases, per-agent provenance, Git HEAD import, revision inspection, JSON responses, recovery receipts, and optional fast-forward Git mirroring. Preparation runs outside the publication gate; head/lease revalidation and Git compare-and-swap protect publication.

SVN inspires the revision model: `r0` is the original snapshot, then accepted changes become `r1`, `r2`, and onward. mai does not implement SVN storage or wire protocols. Git provides proven content storage and interoperability, while agents interact through workspaces and submissions.

The immediate audience is 3–30 agents working on one machine and one repository. The value is coordination before conflict through leases, an ordered acceptance history, attribution and recoverable outcomes. Exact integrated validation and normal local Git use are priorities, not shipped guarantees. See [architecture](architecture.md) and [measurements](performance.md).

## Current limits

This is an MVP for trusted local workflows on one machine and a local filesystem. Workspaces are not sandboxes. Declared agent IDs are not authenticated identities. Leases block foreign publication through supported operations but do not enforce OS permissions. Ordinary Git commits after checking out main in a linked workspace can bypass publication policy; no accidental-write guard is installed yet. Local commits also move HEAD away from the submit base. Avoid both until fixed.

There is no daemon, server, authentication, distributed locking, sparse materialization, automatic lease renewal, or durable downstream job queue. Windows CI coverage is experimental pending native qualification. Do not expose the authority to untrusted tenants or use NFS/SMB. Quiesce a workspace while submitting or refreshing it.

Large independent bursts can exhaust the bounded contention budget; rejected attempts need client backoff/retry. Accepted history remained intact in recorded samples, but thousand-agent capacity is not established. Crash-state/process-kill tests exist; power-loss, disk-full, interrupted initialization, and backup restoration still need qualification. `--check` tests the agent's workspace, not the eventual integrated trunk. Details and operator recovery are in [architecture](architecture.md#durability-and-recovery).

## Roadmap

Ship small local workflows with success/error regressions and recovery evidence. This order expresses product priorities; independent fixes can proceed in parallel.

1. **Ordinary Git use:** separate the workspace base from HEAD, guard authority refs and preserve private checkpoints through workspace updates.
2. **Dirty updates:** merge trunk into existing work without discard, with explicit conflict resolution and continuation/abort behavior.
3. **Integrated validation:** check the exact candidate tree outside the publication gate; revalidate or stop if its parent changes. No checkout-free API or custom changeset format is needed.
4. **Efficient publication:** assemble candidates under the gate for unchecked submissions, remove worktree creation from that gate, then measure bounded group commit separately.
5. **Truthful recovery:** distinguish accepted retries from new edits and return actionable machine outcomes. This correctness fix need not wait for performance work.
6. **Safe evaluation and distribution:** preserve ignored/untracked work, support explicit path capture, qualify object formats and bounded checks, then prepare verified native archives.
7. **Useful environments:** support explicit post-create bootstrap outside publication locks; measure dependency setup before investing in sparse or lazy materialization.

Opt-in same-file textual merges follow integrated validation. Remote services, custom changesets, tiered caches and partitioned storage are [possible futures](agent-native-capabilities.md), not local MVP requirements. No dates or automatic version promotions follow from this sequence.

## Development policy

This is a brand-new project with no users and no backward-compatibility requirements. Repository formats, metadata, CLI syntax, and JSON/protocol schemas may change freely. There are no legacy readers, compatibility fixtures, migration shims, downgrade routes, or promises that an older store/client will work. Recreate disposable evaluation repositories when formats change. Document breaking changes and keep tests/examples aligned with the current implementation.

Data integrity and recovery within the current implementation remain correctness requirements. This policy does not authorize deleting user work or migrating Maida production assets. Git import/export and self-hosting are product features, not historical-format support.

## Versioning

Start at **0.0.1** for the MVP. MVP releases use `0.0.x`; leaving MVP promotes to `0.1.0` (the `0.1` line), and demonstrated stability promotes to `1.0.0` (the `1.0` line). Maturity milestones are not automatic dates or feature-count thresholds. Historical measurement labels remain as originally recorded.

For Maida adoption, follow [the staged migration plan](maida-migration.md). For implementation choices and alternatives, see [ADR 0001](adr/0001-foundation.md).

[All docs →](README.md)

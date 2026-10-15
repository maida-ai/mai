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

Deliver reliable integration for 3–30 local agents first; then earn symbol-aware coordination, live intent, tree-bound context and draft composition through real use. These are plans, not shipped capabilities. [Agent-native direction](agent-native-capabilities.md) defines planned scope, design principles and unresolved decisions; the [GitHub roadmap](https://github.com/maida-ai/mai/issues/1) tracks implementation.

| Stage | Outcome |
| --- | --- |
| M1 — Usable local integration | Explicit workspace bases and accidental-write guards; dirty updates; truthful recovery; integrated-candidate validation; scoped capture; efficient publication; environment bootstrap and local safety fixes |
| Gate — External users | 5–10 external users on real repositories, usable releases/client examples, and failures recorded as issues |
| M2 — Symbol-aware integration | Symbol/key-path extraction, leases, structural merge and deterministic definition/reference interaction checks |
| M3 — Live awareness | Structured intent, activity and deterministic conflict ownership |
| M4 — Resumable, explainable work | Opt-in context checkpoints, orphan recovery and attribution queries |
| M5 — Draft DAG | Anonymous branching, draft composition and atomic group landing |
| M6 — Policy guards | Optional external checks on instruction and policy changes |

Do not start M2 before the users gate without an explicit maintainer override. Milestones have no invented deadlines or automatic version promotions. Integrated validation is P0 and unblocked; no checkout-free protocol or custom changeset format is a prerequisite. In-gate construction and bounded group commit are separate measured steps; long checks, hashing and environment setup stay outside the gate.

Exact validation requires the accepted candidate's tree and parent to match the checked candidate. The proposed weaker “validated on parent” alternative needs an explicit decision and cannot silently weaken that invariant. Context storage is planned as opt-in and harness-owned; cross-agent context handoff, retention and size limits remain unresolved. Local commits, update/resume commands and context storage must not be advertised as usable until their implementations ship.

Final tagline, untracked-file policy default, initial parser languages/config formats and context limits remain maintainer decisions. Fleet-scale services, caches and distributed storage are deferred until after M4 and demonstrated user demand. Measure late-conflict waste, false-conflict rejection, post-land failures, resume success and runnable environment readiness alongside publication performance; see [measurement definitions](performance.md#differentiation-metrics-planned).

## Development policy

This is a brand-new project with no users and no backward-compatibility requirements. Repository formats, metadata, CLI syntax, and JSON/protocol schemas may change freely. There are no legacy readers, compatibility fixtures, migration shims, downgrade routes, or promises that an older store/client will work. Recreate disposable evaluation repositories when formats change. Document breaking changes and keep tests/examples aligned with the current implementation.

Data integrity and recovery within the current implementation remain correctness requirements. This policy does not authorize deleting user work or migrating Maida production assets. Git import/export and self-hosting are product features, not historical-format support.

## Versioning

Start at **0.0.1** for the MVP. MVP releases use `0.0.x`; leaving MVP promotes to `0.1.0` (the `0.1` line), and demonstrated stability promotes to `1.0.0` (the `1.0` line). Maturity milestones are not automatic dates or feature-count thresholds. Historical measurement labels remain as originally recorded.

For Maida adoption, follow [the staged migration plan](maida-migration.md). For implementation choices and alternatives, see [ADR 0001](adr/0001-foundation.md).

[All docs →](README.md)

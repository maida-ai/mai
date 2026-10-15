# ADR 0005: Serialized candidate construction and bounded batching

Status: proposed, 2026-10-14. In-gate construction and queued group commit described here are not implemented; deliver and measure them separately in M1.

## Proposal

Move candidate tree and commit assembly into the existing serialized publication gate to avoid losing optimistic construction work to competing publishers. Keep staging, hashing, workspace creation/reset, environment bootstrap and long checks outside it. Shorten workspace-registration transactions so `git worktree add` never holds the publication gate. Measure build waste and lock tails before claiming improvement; building inside the gate alone does not eliminate validation retries or semantic rejections.

With required integrated checks, release the gate after candidate construction, check outside it, and reacquire for exact-parent CAS publication under [ADR 0004](0004-integrated-candidate-validation.md). If main advanced, rebuild and revalidate or return a typed non-published outcome. Holding the gate through a slow check is not an allowed shortcut.

Only after measurement, consider a bounded SQLite queue of immutable captured deltas with request identities and a current metadata format version. A gate holder drains requests in a deterministic order, checks each against the accumulating batch, chains accepted commits, and publishes the chain with one ref transaction. Each rejected request receives its own outcome. Checks of captured deltas are not checks of batch candidates: required validation must match the exact candidate each publication promises, including its accumulated parent. Specify fairness, cancellation, limits and acknowledgement semantics before implementation.

## Durability and verification

Git main is the acceptance ledger; SQLite is not atomically committed with it. Reconcile queue state and receipts using accepted ancestry after every interruption boundary. Retain request objects until reconciliation establishes their outcome. A single ref transaction does not prove one fsync, power-loss atomicity across stores, or durable acknowledgement. Measure actual persistence behavior rather than promising a single-fsync batch.

Metadata versioning is scoped to the chosen queue/configuration, with unsupported formats rejected; do not introduce legacy readers or restore the deferred custom changeset protocol. Revision trailers may replace history counting in the current format, but no legacy counting fallback is adopted here; define baseline `r0`, malformed trailers and validation explicitly.

Test separate-process disjoint and overlapping requests, same-workspace coordination, crashes around Git publication and queue acknowledgement, and stalled/cancelled requests. Record useful throughput, retries by class, build waste, and p50/p95/p99 including rejected requests. Batching independent submissions is distinct from the all-or-none dependent landing in [ADR 0009](0009-draft-dag-and-group-landing.md).

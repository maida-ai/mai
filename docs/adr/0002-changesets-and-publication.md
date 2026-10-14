# ADR 0002: Agent work and authoritative publication are distinct operations

Status: accepted architectural direction; not implemented in the 0.0.1 protocol or repository format. This extends [ADR 0001](0001-foundation.md) while retaining accepted main semantics, r0, and Git-backed storage. [Capability contract](../agent-native-capabilities.md) defines the product requirements.

## Context

The current CLI combines workspace preparation and main publication in `submit`. It prepares immutable Git objects outside the gate, but private candidates have no durable public identity. It uses per-agent receipt refs for acceptance recovery and traverses Git history for several queries. This remains useful locally, while forcing checkpoints, graph queries, and metadata-only jobs into the same worktree-and-submit model would constrain the intended agent workload.

The roadmap needs a source authority, not only a larger merge queue. Frequent private saves and automated read/write jobs should not require global accepted revision assignment. Deployment must stay self-hostable and manageable for small adopters.

## Decision

Introduce three separate future domain operations: **record an immutable changeset**, **query/materialize source**, and **publish onto main**. Only successful publication allocates the next accepted `rN`. A draft graph may have multiple parents and need no per-draft Git branch; main's accepted commits remain one single-parent chain. A checkpoint is durable agent work, never an accepted revision.

A proposed changeset descriptor includes explicit parent/base identities, normalized put/delete/mode entries, immutable content references, minimal actor/task provenance, and a canonical descriptor identity. Authenticated principal and authorization are request context, not a freely trusted descriptor field. Rebase/merge creates new content and a new identity; it does not mutate a prior checkpoint. Exactly-once retry behavior requires scoped request keys and a durable outcome record, not content hashes alone.

Use Git blobs/trees/commits initially. The protocol describes domain identities and file operations rather than making clients manipulate refs or packs. Draft reachability, retention, durable recording, and GC protection must be implemented and tested before exposing checkpoints; creating unreachable Git objects is insufficient. Do not add one mutable ref for every agent checkpoint as a shortcut.

Keep the authority-side publication gate narrowly focused on head, lease/policy revalidation, revision assignment, and durable acceptance. Candidate preparation and source queries must not acquire that gate. A single-node authority is the next deployment target; replication/HA and a different accepted-history ledger require separate evidence and design. A future multi-node read fleet does not make publication lock-free or globally distributed automatically.

Current Git main remains the sole acceptance ledger. If a new metadata journal is introduced, explicitly define intent, object durability, ref update, acknowledgement, and reconciliation order. Git and SQLite are not one atomic transaction. No older metadata format or client needs support; choose the current format directly and document breaking changes. Backward compatibility is not a reason to duplicate authority records or add conversion machinery.

## Alternatives

- Assign `rN` to every checkpoint: rejected because it confuses exploration with accepted history and serializes unnecessary work.
- Keep one heavyweight Git branch/worktree per saved draft: rejected as the required public model; temporary Git internals remain acceptable until a better measured materializer exists.
- Add an HTTP wrapper around current `submit` and call it agent-native parity: insufficient for checkout-free writes, stable queries, bounded client data, indexed metadata, or shared caching.
- Replace Git with a custom distributed database now: rejected without measured need, storage conformance, recovery, and operational qualification.
- Require every operation to use a daemon: rejected; direct local access remains valuable and uses the same domain invariants.

## Consequences and implementation gates

Separate source/query/publication ports should be extracted when their first tested consumers require them, not as empty crates. Prototype metadata-only operations before remote transport. Then qualify durable candidate/outcome recording and checkpoint-free submission. Sparse workspaces and shared bounded object caches follow the stable source contract; an authenticated single-node service follows authorization and request-limit tests.

Required tests cover checkpoint saves with unchanged main, disjoint/conflicting landing, repeated request keys, mismatched retry payloads, interruption after object writes, missing outcome/ref records, reachable-draft retention, publication policy/lease races, unauthorized reads through caches/indexes, and current-format integrity. Safe conservative conflict rejection is retained unless a separate tested merge policy is enabled. No LLM, GitHub, or proprietary infrastructure is essential.


## Decisions still required before implementation

- Canonical changeset encoding and typed identifier/hash policy, including how Git object identities are exposed or mapped.
- Current metadata layout, durable request-key retention, source export, and the journal/Git reconciliation state machine. No format converters or downgrade routes are required.
- Reachable private-source roots, retention/quotas, and garbage collection that survives interrupted maintenance without relying on one ref per draft.
- Authentication adapters, directory-policy semantics, cache revocation, and how authorized filtered views expose graph/history metadata.
- Attestation dependency/scope rules, the first optional deterministic merge policy, and the boundary between pre-land checks and post-land/pre-deploy observation.
- Lazy materialization mechanisms per OS and the profiles/restore evidence required to evaluate partitioned source storage or authority failover.

These are follow-up ADRs and tested vertical slices, not unspecified behavior that implementations may leave untested. There are no backward-compatibility obligations.

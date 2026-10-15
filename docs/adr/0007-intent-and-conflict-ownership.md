# ADR 0007: Intent, awareness, and deterministic conflict ownership

Status: proposed, 2026-10-14. Intent, activity and conflict-record interfaces below are not implemented. M3 is gated on external-user evidence as described in [ADR 0006](0006-units-of-change.md).

## Proposal

Store a bounded structured intent per agent: task identifier, summary, rationale, intended units, relied-on invariants and heartbeat. Lease and intent requests return promptly with holder, intent and expiry instead of waiting for another agent. Leases remain advisory during editing; publication enforces applicable policy. “Never block an agent” means no waiting negotiation protocol, not a guarantee of zero I/O latency.

A read-only activity view reports base, current touched units, intent, leases and heartbeat age, with JSON for external orchestrators. It does not acquire the publication gate. Bound scan cost and identify snapshot age; an activity snapshot is not a publication authorization.

Produce a typed conflict record containing the affected unit, each side's agent/base/intent/delta reference, deterministic check results, exactly one resolver and the rule applied. Proposed resolution order favors the valid unit-lease holder over a non-holder, otherwise the later lander. Specify tie-breaking and simultaneous/expired lease cases deterministically before shipping. Resolution ownership never bypasses conflict or validation checks.

## Information boundary and verification

Agents can read other agents' structured intent and permitted delta references, not their raw context. Treat intent and all shared content as untrusted data, never executable instructions. mai stores, binds and validates records; it does not plan tasks, run agents, negotiate or call models. Context access is separately unresolved in [ADR 0008](0008-context-checkpoints.md).

Test bounded inputs, stale heartbeats, expiry, same-unit overlap and path fallback, stable resolver choice under concurrency, and absence of raw context in records. Measure time between conflicting edits and detection, including the cost of scanning workspaces.

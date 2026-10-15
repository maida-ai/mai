# ADR 0008: Opt-in context checkpoints and explicit access

Status: proposed, 2026-10-14; access and retention decisions unresolved. Context storage, resume and why interfaces below are not implemented. This proposes revising the earlier no-prompt-storage stance only for explicit harness-supplied opt-in storage; it does not authorize automatic context sharing. M4 remains behind external-user evidence.

## Proposal

A future checkpoint operation creates a private commit, retains the latest checkpoint at `refs/mai/workspaces/<agent>/checkpoint`, and attaches a context envelope through `refs/notes/mai-context`. Bind the envelope to exact commit/tree, base revision, agent/task, harness, model and creation time. Include schema version, payload media type, object identity, byte length and digest, with a harness-written summary and redaction declaration.

mai validates structure, binding and configured limits, stores payloads opaquely, and never transmits, compresses, summarizes or redacts them. Harnesses own content preparation; a `redacted` flag is an assertion, not proof that secrets are absent. Context storage is explicit opt-in. Default retention period and size cap require maintainer decisions. Define reachability, older-checkpoint retention, notes/blob cleanup, export and retirement behavior before promising durability or erasure.

Heartbeat and lease expiry can mark a workspace potentially orphaned; expiry alone does not authorize takeover, transfer leases or grant access to context. A proposed resume operation can reconstruct code from a permitted checkpoint and report trunk changes since its base, using symbols where available. Handoff must be explicit and cannot steal active ownership or silently expose payloads.

## Unresolved access boundary

Returning a dead agent's envelope to a new agent requires an explicit exception to the default no-sharing rule. No automatic cross-agent context return is adopted. Define identity, ownership and explicit authorized handoff semantics before implementing resume context retrieval; enforce the no-sharing rule unless a maintainer-approved access policy expressly permits that handoff. Local same-user Git access is not a security sandbox, so document the actual boundary and storage exposure.

A future `why` query may expose accepted attribution and permitted intent rationale. Context summaries require a separately safe, authorized projection; a summary is not automatically public merely because it is not the payload. Neither conflict records nor activity views include raw context. See [ADR 0007](0007-intent-and-conflict-ownership.md).

## Verification

Test exact-state binding, malformed and oversized envelopes, unauthorized retrieval and handoff, payload leakage through summaries/errors, interrupted checkpoint creation, GC/retirement, and orphan false positives. Measure resumed task completion without redoing accepted work; do not claim resume success from envelope storage alone.

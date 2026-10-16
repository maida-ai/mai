# CLI reference

**MVP foundation layer.** The previous worktree engine has been retired. This binary currently supports `mai --help` and `mai --version` only; the version is `0.0.1`. Unsupported commands exit 2. Initialization and local serving will be added by dependent implementation layers. The planned Git-push receive/queue interface is not enabled.

## Versioned lifecycle contract

The Rust `lifecycle` module defines schema version 1. A submission is identified by an immutable Change-Id and positive generation number, with immutable agent identity, lowercase nonzero SHA-1 commit and exact parent-generation dependencies. Change/agent identifiers contain 1–128 ASCII letters, digits, underscores or hyphens. Identical commit retries with identical metadata return the existing generation, including after snapshot restoration; conflicting metadata rejects.

States are received, queued, dependency-blocked, awaiting-approval, validating and terminal. A queued update can be superseded; a validating generation finishes before a replacement proceeds. Dependencies never silently retarget. A failed, superseded or cancelled dependency rejects descendants; all parents must land before a dependent attempt begins. Approval and dependency blocking are distinct conditions.

Each validation attempt has its own increasing number. Interrupting an attempt requeues the same generation without assigning a terminal outcome. Completion must identify the currently running attempt, so stale workers cannot complete a later attempt. Each generation has one immutable terminal outcome: landed, rejected, superseded or cancelled. Later validation, revert or recovery annotations append without rewriting that outcome.

Rejected outcomes carry stable reasons: `invalid_submission`, `dependency_failed`, `conflict`, `check_failed`, `policy_denied` or `infrastructure`. Optional conflict, check, log-tail, candidate-ref and next-action diagnostics are each bounded to 4096 UTF-8 bytes. Oversized values reject rather than silently truncate. Candidate refs must be under `refs/mai/candidates/` and use safe ASCII components. Unknown schema versions, unknown fields and inconsistent snapshots refuse decoding; there are no migrations or compatibility readers.

Stable process categories for upcoming commands are success 0, invalid input 2, terminal rejection/supersession/cancellation 3, unavailable transport 4, infrastructure failure 5 and pending 6. This layer supplies the types; no submit/wait command or JSON transport is exposed yet. Pure model transitions do not certify validation evidence, durable acknowledgment or Git CAS publication. Persistence and authority layers must enforce those separately.

## Planned interface — unavailable

M1 will support ordinary clones and `git push mai HEAD:refs/for/main`; push success acknowledges durable registration, not landing. Planned commands include `init`, `serve`, `workspace`, `submit`, `wait`, `sync`, `status`, `queue`, `lease` and `approve`. Exact flags and response schemas must be implemented and tested before use. Explicit sync must preserve unsubmitted local work after squash landing. See [agent protocol](agents.md), [configuration](configuration.md) and [operations](operations.md).

Differentiated priority and automated incident recovery remain deferred under [ADR 0010](adr/0010-staged-mvp-qualification.md) and RFC #93. They are not represented as enabled features or speculative fields in the current schema. The initial MVP uses a single-class dependency-aware queue.

[Getting started →](getting-started.md) · [All docs →](README.md)

# ADR 0002: Private work and accepted publication

Status: revised for local-agent scope on 2026-10-14. This supersedes the earlier unimplemented requirement for custom changeset/query protocols, a remote source service, and fleet-scale completion gates. It retains the distinction between private work and acceptance from [ADR 0001](0001-foundation.md).

## Decision

Only publication onto authoritative main allocates an accepted `rN`. Local Git commits should serve as private checkpoints without publishing or requiring a custom changeset format. This is a target behavior: today's submit still derives its base from HEAD and does not safely support normal local commits.

Introduce an explicit workspace base ref independent of HEAD and guard authority-owned refs against accidental writes from ordinary Git. The guard is not protection against a same-user process that deliberately bypasses hooks. mai's own hook-bypassing operations must not reset an attached main branch. Define checkpoint reachability and retention through submit/reset and workspace removal; a base ref alone does not retain every private commit.

Keep staging/hashing, workspace checkout, environment bootstrap and long-running checks outside the publication gate. Candidate assembly may move inside the gate to avoid optimistic rebuilds. Integrated checks run against an immutable candidate outside the gate; if main changes, rebuild/revalidate or return a non-published outcome. A stale-parent check is not validation of the eventual accepted tree.

Git main remains the sole acceptance ledger. A future bounded group-commit queue needs immutable request identity and explicit acknowledgement/reconciliation around Git publication and SQLite state. One ref transaction does not establish one fsync or cross-system power-loss atomicity. No queue, remote API or custom storage engine is prerequisite for the base-ref, update or integrated-validation fixes.

## Consequences

Preserve single-parent accepted history, r0, ancestry-verified receipts, lease checks, CAS and recoverable work. Test local commits/amend/rebase, accidental main writes, dirty update conflicts, post-acceptance edits and interrupted transitions before calling these workflows supported. Existing conservative overlap remains the default; opt-in textual merging is a separate feature with exact integrated validation.

No backward compatibility is required during this new MVP. Choose the simplest current format and document changes; do not add legacy readers or migration shims. Remote clients, custom descriptors, indexed source services and cache tiers are fleet-scale futures. [Agent-native direction](../agent-native-capabilities.md) adds staged symbol/intent/context/draft work; proposed ADRs 0003–0009 refine those plans without claiming implementation.

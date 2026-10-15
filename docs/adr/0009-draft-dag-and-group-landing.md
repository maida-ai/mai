# ADR 0009: Draft DAG and atomic group landing

Status: proposed, 2026-10-14. Fork, fuse and group landing described here are not implemented. M5 follows external-user evidence and the prerequisite local workflow, validation and checkpoint work; none is required to deliver M1.

## Proposal

A future fork creates private work based on another permitted checkpoint. Fuse performs a three-way integration of another private delta and records both private parents. These are anonymous branches: the product discourages long-lived named branches and manual rebase chores, not Git's underlying branching model. Define draft-base identity separately from an accepted base; never treat an unpublished draft as an accepted `rN`.

A group landing publishes an explicitly ordered set of dependent contributions, or none. A draft-dependent workspace cannot land before its parent unless both are in that group. Reject cycles, missing parents and invalid order before publication. Preserve single-parent accepted main history, original `r0`, attribution and ancestry-verified receipts; the private DAG does not redefine the acceptance ledger.

Build the final integrated group tree and run required checks outside the gate, bound to exact final tree and accepted parent under [ADR 0004](0004-integrated-candidate-validation.md). Preserve each contribution's deterministic overlap/lease requirements. Publish the accepted chain with one CAS ref transaction only after the entire group qualifies. Define whether checks cover every intermediate accepted revision or only the final group tree, and how receipts state that scope, before implementation; do not claim individually validated revisions from a final-tree-only check.

## Recovery and verification

Retain all private source objects and checkpoints until publication/reconciliation completes. Git main establishes all-or-none acceptance; reconcile SQLite and workspace updates after interruption without discarding private work. This explicit atomic group is distinct from performance batching of independent requests in [ADR 0005](0005-serialized-publication-and-batching.md).

Test dependent order, invalid/cyclic groups, fuse conflicts, concurrent main movement, final-check failure, attribution, and crashes before/after the ref transaction. Context remains subject to [ADR 0008](0008-context-checkpoints.md); forking code never implicitly grants access to another agent's raw context.

# ADR 0003: Workspace bases, guarded refs, and private checkpoints

Status: proposed, 2026-10-14. None of the behavior introduced here is implemented. This refines [ADR 0002](0002-changesets-and-publication.md) for M1.

## Proposal

Store the accepted workspace base at `refs/mai/workspaces/<agent>/base`, independently of HEAD. Compute submission, diff and status deltas against that base and the captured working tree, including committed private changes. Local commits, amends and rebases become private checkpoints; acceptance still squashes the submitted delta into one attributed first-parent main commit. Only acceptance allocates `rN`.

Install a `reference-transaction` hook at initialization and verify or repair it on open. Reject ordinary Git writes to `refs/heads/main` and `refs/mai/*` at the prepared phase. mai's controlled Git calls bypass hooks; therefore they must separately enforce authority checks and never reset an attached main branch. The guard prevents accidents, not deliberate hook bypass or same-user filesystem writes. Verify behavior on Git for Windows.

`mai update` would checkpoint dirty work, merge a captured trunk commit into the private workspace, and advance the base only after a clean or explicitly resolved merge. Conflicts retain standard markers and record `refs/mai/workspaces/<agent>/pending`. Continue rejects unresolved index entries and unresolved conflict markers; abort restores the pre-update checkpoint. Preserve staged, unstaged and untracked work through interruption and abort; checkpoint creation must follow the chosen capture policy, not silently include secrets or discard excluded files.

Recovery must reconcile the accepted commit and the captured submitted delta before advancing the base. Preserve concurrent or subsequent edits; a tree comparison alone must not mistake changes introduced by integration for new agent work. Return `accepted_with_pending_changes`, its verified receipt and a distinct exit status when new work remains. Never report an old receipt as plain success for new work.

## Boundaries and verification

Missing or unsupported base metadata is rejected with an actionable recreation/recovery instruction. The MVP has no legacy HEAD-derivation migration. A base ref does not retain all private commits: define checkpoint roots and retention across submit, reset, abort and retirement before claiming durability; opt-in context retention is separate in [ADR 0008](0008-context-checkpoints.md).

Test multiple local commits, amend, rebase, accidental main writes, dirty updates, resolution, abort, same-workspace concurrency, and process death before and after acceptance. Assert authority refs, source repository refs and all user work remain correct. Publication and validation obey [ADR 0004](0004-integrated-candidate-validation.md).

# S2 go/no-go assessment

**Overall: NO-GO for declaring #52 complete on the available evidence. GO for the tested Git merge mechanism and performance criterion.** This assessment is complete; its no-go conclusion identifies the remaining object-manifest coverage rather than leaving the decision pending. Maintainer acceptance remains separate from this recommendation.

## Evidence supporting go

[Run 38694778050, attempt 1](https://github.com/maida-ai/mai/actions/runs/38694778050), source `ff55f478f861d85c1a75347f685bfa5bc691d5c3`, passed Linux, macOS and native Windows jobs. All nine unit tests and ten differential scenarios passed per platform. Exact clean trees and conflicted index entries agree with equivalent cherry-picks. Stale and squash-dependent base handling, zero/multiple-base rejection and the tested custom-driver trust boundary are exercised. Median integration times were 1.415511, 6.393542 and 15.302000 ms respectively, each below the unchanged 100 ms limit on exactly 100,000 tracked files. The [qualification report](README.md) maps individual issue criteria and the [snapshot](evidence/2026-10-15-run-38694778050-attempt-1/README.md) retains their provenance.

## Blocking manifest finding

In the executed [manifest fixture](../../../experiments/s2/manifest.rs), the base contains `config database.host` and the submission replaces it with `config database.port`. The emitted receive and final manifests include `config:source.units:database.port` but omit `config:source.units:database.host`. They label the enclosing file delta `M`, rather than asserting added/deleted kinds for the changed config units. The extractor also emits unchanged module/symbol identities from the after snapshot. The code asserts file-diff counts and fallback behavior, but only prints these manifest contents; it does not compare both complete views against independent expected unit-level results.

This is observable in all three platform logs under `MANIFEST view=receive` and `MANIFEST view=final-post-regeneration`; [review.json](evidence/2026-10-15-run-38694778050-attempt-1/review.json) retains those rows for comparison. The final view correctly includes `cache.generated` with explicit file fallback, so file-level regeneration widening is demonstrated. It does not establish preservation of removed objects or correct unit-level change kinds. A green harness run cannot satisfy that unasserted portion of #52's object-manifest extension or [ADR 0004's added/modified/deleted manifest contract](../../adr/0004-objects-manifests-and-priority.md#d11-engine-computed-manifests).

This finding is about the restricted fixture's evidence, not a shipped product defect: the rewrite is not implemented. The plan intentionally permits synthetic grammar and conservative fallback. The blocker can be addressed within that grammar; introducing production parsers or choosing M2 defaults is unnecessary.

## Harness correction and remaining evidence

The follow-up harness now compares both snapshots, asserts complete receive/final manifests and deletion/rename unit changes, and preserves conservative file fallback for uncertainty on either side, mode changes and content outside the identity-only grammar. Regression checks cover removed-unit retention, unchanged-unit exclusion, generated addition/deletion and required fallback. Local non-timed Linux verification passes; no local evidence snapshot is committed. These changes address the implementation gap described above in the previously executed source, but do not retroactively change its artifacts or qualify the new source on macOS/Windows.

## Conditions to change the overall decision to go

1. Implemented in the follow-up harness; native CI verification pending: derive synthetic unit changes from both before and after snapshots. Assert the removed `database.host` unit and added `database.port` unit, with explicit change kinds; preserve conservative fallback where identity is uncertain.
2. Implemented in the follow-up harness; native CI verification pending: independently assert the complete receive and final post-regeneration manifests, including generated-file widening, instead of asserting only path counts and printing the units. Include a negative regression that fails if a removed unit or required fallback is dropped.
3. Implemented in the follow-up harness; native CI verification pending: cover object deletion and rename delete-plus-add through those manifest assertions, while keeping containment and module/fallback limitations explicit. Manifest precision must never authorize semantic independence or bypass the gate.
4. Rerun the updated S2 workflow on the native platforms and review the new source hashes and artifacts. Preserve this passing merge/timing evidence and its documented gap unchanged.

No threshold change or ADR redesign is warranted by this run. If follow-up evidence exposes a failed merge assumption or performance criterion, record that failure and resolve it or obtain an accepted redesign before M1. Exact Git 2.40, cold-cache/large-content performance, arbitrary custom drivers and real-language parsers remain unqualified beyond this spike's tested scope; they are not additional invented gates for the narrow manifest follow-up.

## Disposition

Open an evidence PR with `Refs #52`, not a closing reference. Keep the issue open and do not change ADR acceptance, product gates or maintainer defaults. Merging this report preserves useful evidence; it does not complete S2 or M0.

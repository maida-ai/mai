# S2 Git merge qualification

**Decision: GO for the tested checkout-free Git merge approach; NO-GO for full S2/#52 acceptance until object-manifest assertions are completed.** All three native CI jobs passed, including the unchanged median <100 ms criterion. The green run does not establish the missing manifest semantics. See the completed [go/no-go assessment](go-no-go.md) for the blocking finding and acceptance conditions. No issue closure, ADR acceptance or M0/product implementation is implied.

## Reviewed CI evidence

[Workflow run 38694778050, attempt 1](https://github.com/maida-ai/mai/actions/runs/38694778050) ran on 2026-10-15 UTC from `main` at `ff55f478f861d85c1a75347f685bfa5bc691d5c3`, the merged S2 harness. The [reviewed snapshot](evidence/2026-10-15-run-38694778050-attempt-1/README.md) retains all three platforms, source hashes, provenance, command/conflict/manifest logs, all timing samples and GitHub job/artifact metadata. Each artifact's four harness SHA-256 entries match the exact source revision that ran. No locally generated experiment evidence is included.

| Native platform | Git | Host recorded by runner | Median, 31 samples | Criterion |
| --- | --- | --- | --- | --- |
| Linux x86_64 | 2.55.0 | AMD EPYC 9V45, 4 visible CPUs; Ubuntu 24 image | 1.415511 ms | PASS: <100 ms |
| macOS arm64 | 2.55.0 | Apple M2 Pro (Virtual), 14 GiB; macOS 26 image | 6.393542 ms | PASS: <100 ms |
| Windows x86_64 | 2.55.0.windows.5 | AMD64 Family 26 Model 2, 4 cores; Windows 2025 image | 15.302000 ms | PASS: <100 ms |

Each platform passed nine unit checks and ten differential cases. Review independently counted exactly 100,000 unique tracked paths in each original log, verified sample IDs 0–30 and recomputed the median from every retained sample. The benchmark is a warm-object-store fixture with shared small blobs, 100 directories and two endpoint edits; it measures Git process startup plus evidence logging as well as merge work. It does not establish cold-cache, large-content, arbitrary workload or minimum-Git-version performance. No threshold was relaxed and no sample was dropped.

## Issue #52 criterion assessment

| Criterion | Assessment | Supporting evidence and limit |
| --- | --- | --- |
| Disjoint same-file hunks, same-hunk conflict, rename/edit, delete/edit, executable mode, binary and file/directory cases versus cherry-pick | PASS for the corpus on all three platforms | `PASS case=` records; clean trees match independent expected trees and the oracle; conflicted paths, index stages, modes and blobs match the oracle. |
| Stale and squash-dependent submissions use intended bases | PASS for fixture semantics | Stale and dependent-squash cases pass; the child applies its submitted-parent delta after the parent's squash and a subsequent trunk edit. The deliberately wrong root base conflicts. Dependency mapping lifecycle remains separate S7/product work. |
| Missing/multiple merge-base policy | PASS for explicit experiment policy | Unrelated histories and criss-cross graph are exercised. Reject zero/multiple bases unless a separately verified dependency mapping supplies the intended base; never guess. This does not select pending shipping defaults. |
| Median <100 ms on 100k tracked files with reproducible provenance | PASS on all recorded hosts | Every platform has exactly 100,000 paths, 31 raw samples, unchanged strict threshold, Git/compiler/image/hardware provenance and matching source hashes. |
| Attributes/custom drivers and checkout-free integration | PASS for tested behavior and trust boundary | Integration repository is bare. Approved `info/attributes` selects a configured constant-output driver. A malicious candidate-tracked script is never checked out; invoking that relative script fails. Approved `-merge` policy forces a conflict. Candidate-attribute conflict behavior is compared with cherry-pick, not generalized to arbitrary drivers. |
| File/symbol/config/module fixtures, containment, generated/ambiguous fallback and rename delete-plus-add | PARTIAL | Restricted `.units` grammar and fallback/containment checks pass; Git reports file rename as delete-plus-add. This is not a real-language parser or proof of semantic independence. Object-level change kinds and removed objects are not asserted. |
| Receive versus final post-regeneration manifests | PARTIAL — blocks full acceptance | File-level diffs distinguish one changed receive path from two final paths and include generated-file fallback. Unit extraction reads only the after snapshot; the removed `database.host` key is absent from both printed views, and the expected full manifests are not asserted. See the go/no-go report. |
| Reproducible fixture and go/no-go report | COMPLETE for this evidence review | Harness remains under `experiments/s2/` per repository conventions; this report, decision and selected CI evidence are under `docs/spikes/s2/`. |

## Interpretation and follow-up

The merge and timing results support retaining Git CLI explicit-base, checkout-free integration for the tested scope. Attributes and configured drivers are trusted executable policy, not a sandbox. Approved policy must remain outside candidate control; these fixtures do not qualify arbitrary hooks/drivers or the full production policy closure. The cherry-pick oracle deliberately uses its own checkout, while integration does not.

Complete the narrow manifest follow-up described in [go-no-go.md](go-no-go.md), then collect fresh CI evidence for the changed harness. Real parser qualification belongs to later object work; it is not necessary to replace the restricted grammar merely to assert correct synthetic object deltas. Real Cargo/npm/uv regeneration remains S6 coverage. Keep #52 and M0 open pending the missing assertions and maintainer review. No ADR redesign is indicated by the passing merge/timing criteria; the current blocker is a qualification gap, not evidence that the Git approach fails.

## Review verification

The review checked every run/job conclusion, artifact availability, source hash against the executed commit, unit summary, ten differential outcomes, unique file count, complete timing sequence, recomputed median and manifest output. Snapshot normalization is documented explicitly. Documentation links, shell/workflow structure and whitespace checks passed. `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings` and `cargo test --all-targets --locked` passed during review (34 product tests); no experiment was rerun or workflow dispatched during evidence review.

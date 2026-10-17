# S5 forge-cycle qualification

**GO for #55's faithful local topology qualification.** Linux, macOS and native Windows each passed five unit tests, collector checks and 122 scenario assertions across six cycles: three squash and three single-commit rebase cycles. The issue explicitly permits a faithful local topology with missing real-forge qualification recorded; that boundary is preserved here.

## Criterion assessment

| #55 criterion | Observed evidence | Assessment |
| --- | --- | --- |
| Three landing/export/squash/sync cycles plus rebase behavior and concurrent human edits | Six `CYCLE` records and two `MODE PASS` records per platform, actual Git subprocesses | PASS |
| Zero spurious conflicts, expected contents/diffs and ancestry | Exact tree oracle and export/sync delta checks; nine contiguous post-import revisions per mode; landing trailers/parents and sync second-parent provenance asserted | PASS |
| Genuine conflict and configured owner | `shared.txt` conflict and `fixture-upstream-owner` recorded; Git rejects real conflicting integration | PASS |
| Continue non-conflicting local work on old upstream base | Independent landings during failed validation and pending true conflict; last-synced/forge heads checked | PASS for fixture |
| No force-push or automatic human-main mutation | Authority only exports named branches; forge human/merger roles advance main; failed upstream gate does not advance authority or allocate revision | PASS |
| Local topology with limits, reproducible script/report | Hashed source, all commands, six complete cycles on each native platform | PASS |

## Limits and disposition

Exports are synthesized single commits parented on the last synced upstream and carry the trunk tree. This qualifies that strategy, not raw-trunk export, reused PR branches or arbitrary multi-commit exports. The exact-content gate is synthetic. GitHub APIs, required checks, review rules, protected branches, credentials, webhooks and remote concurrency races remain pending real-forge qualification; no live forge settings were changed. Large real repositories and arbitrary attributes/drivers remain outside this small fixture. These limits do not block the explicitly allowed local-topology issue; recommend closing #55 on merge while retaining them for later implementation/integration work. No ADR acceptance or M0 completion is implied.

## Reviewed evidence and verification

[Run 38701977194, attempt 1](https://github.com/maida-ai/mai/actions/runs/38701977194) executed `4a58e63e77a5604a1454da85dd26603770195716` on 2026-10-15. The [snapshot](evidence/2026-10-15-run-38701977194-attempt-1/README.md) retains every collected artifact file with original and retained SHA-256s, job conclusions, source hashes and independently checked summaries. All recorded harness source hashes match the executed revision. See the [go/no-go decision](go-no-go.md) and [reproduction instructions](../../../experiments/s5/README.md).

This evidence-only review checked source identity, artifact completeness, reported outcomes against raw logs and the executed assertions, documentation links and whitespace. No experiment was rerun or workflow dispatched; no product code, thresholds, defaults or ADR status changed.

Baseline product checks passed during review: `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings` and `cargo test --all-targets --locked` (34 tests). Disposable builds stayed under `.throwaway/`. No new local experiment evidence was generated.

# S7 receive and landing crash-recovery assessment

**NO-GO for full #57 qualification; GO for the implemented process-kill fixture subset.** Linux, macOS and native Windows jobs pass. Each passes three unit tests, all 26 kill checkpoints with two recovery retries, active/stale lock handling, note/index rebuilding, stale-pass refusal and six independent corruption probes. Process kills do not establish power-loss durability or a shared Git/SQLite transaction.

## Criterion assessment

| #57 criterion | Observed evidence | Assessment |
| --- | --- | --- |
| Registration, refs, queue, candidate, before/after CAS, note/index and terminal windows | 26 named checkpoint PASS rows per platform, including two uncommitted SQLite WAL windows | PASS for implemented windows |
| No acknowledged loss, duplicate landing, eventual one terminal | Independent inspector checks retained refs/objects, first-parent ledger, one landing/compensation and unique terminal records after retries | PASS for one-generation fixture |
| Exact identity and verified trunk ancestry | Older generation on trunk and unrelated exact-identity decoy do not count as the submitted landing | PASS |
| Duplicate/stale engine lock and lost reply | Live lock rejects second worker; killed worker's stale file does not prevent OS-lock reacquisition; ack window retries | PASS on all native platforms |
| Incomplete notes, manifests, draft objects and invented pass | Git-diff rebuild and corruption probes reject identity/evidence/manifest/deadline/compensation/lost-ref damage | PASS for file fallback and exact-tree gate |
| Dependency rejection/requeue | Durable synthetic records survive checkpoints | PARTIAL: no real multi-submission candidate DAG rebuild/rejection |
| Grant/revoke/expiry/grace/sev attempt preemption | Durable event records survive kills | PARTIAL: no operational scheduler, lease transfer, contention or cancellation semantics |
| Persisted fix-window/full-confirmation/revert races | Original deadline survives changed restart clocks; failed confirmation leads to gated compensation; before/after revert kills recover once | PARTIAL: competing successful fix versus deadline/failure and failed/conflicting revert freeze/notify not exercised |
| Reproducible scripts/report | Exact source/provenance, worker/retry/command logs and checkpoint summary retained | PASS for reporting |

## Required follow-up

Extend the throwaway protocol to real dependent submissions and operational priority/lease/preemption transitions. Inject crashes around competing successful-fix versus timeout/failed-confirmation decisions and failed/conflicting revert freeze/notify paths, then independently inspect exact identities, one landing/compensation, original deadlines and genuine evidence after sufficient recovery. Native platform handling is now exercised for the current fixture; these additional behaviors still require native runs. Keep #57 and M0 open. No power-loss, disk-full, corrupt-WAL or production durability claim is made; no shipping priority/recovery defaults are selected.

## Reviewed evidence and verification

[Run 38702095710, attempt 1](https://github.com/maida-ai/mai/actions/runs/38702095710) executed `6d3859020a1658f267adb9f9e623a598d78e046f` on 2026-10-15. The [snapshot](evidence/2026-10-15-run-38702095710-attempt-1/README.md) retains every collected artifact file with original and retained SHA-256s, job conclusions, source hashes and independently checked summaries. All recorded harness source hashes match the executed revision. See the [go/no-go decision](go-no-go.md) and [reproduction instructions](../../../experiments/s7/README.md).

This evidence-only review checked source identity, artifact completeness, reported outcomes against raw logs and the executed assertions, documentation links and whitespace. No experiment was rerun or workflow dispatched; no product code, thresholds, defaults or ADR status changed.

Baseline product checks passed during review: `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings` and `cargo test --all-targets --locked` (34 tests). Disposable builds stayed under `.throwaway/`. No new local experiment evidence was generated.

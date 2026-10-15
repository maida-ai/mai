# S7 receive and landing crash recovery

**Status: partial harness; NO-GO for S7 qualification.** Native CI evidence remains pending. Process-kill results do not establish power-loss durability, cross-store atomicity, or production readiness. Refs #57; no issue or M0 gate is completed by this work.

The [harness](../../../experiments/s7/README.md) is separate from product code and uses real Git CAS, SQLite WAL, filesystem locks and separate worker processes. The parent kills and reaps each stopped worker, then retries twice; the final inspector is independent of the worker recovery decisions. The synthetic fixture has one submitted generation, an earlier generation on trunk ancestry, an unrelated branch with the current exact submission identity, one exact-tree content gate and one gated compensation. Full production gates and policy defaults are outside this experiment.

| Issue criterion | Harness coverage / remaining work |
| --- | --- |
| Registration, ref creation, queue persistence, candidate, before/after CAS, note/index and terminal writes | Native process kills at every named checkpoint, including two uncommitted WAL transaction windows; all recoveries inspected. |
| No acknowledged loss, duplicate landing, eventual single terminal | Retained change/draft refs and object existence inspected; first-parent chain has baseline, one landing and one compensation; unique terminal outcomes checked after two retries. Ack checkpoint represents lost reply; transport quarantine remains S1 coverage. |
| Exact submitted identity and verified ancestry | Current generation/object trailer must occur on trunk; unrelated exact-identity decoy cannot count, and older generation on actual ancestry cannot count. |
| Duplicate/stale lock | Separate worker rejected while lock is live; stale file survives kill and next process acquires OS lock. Native Windows execution pending. |
| Incomplete notes and manifest index | Deleted note/index rebuilt from actual Git diff; inspector rejects invented index; file fallback explicitly labeled. |
| Dependency rejection/requeue | Crash checkpoints persist rejected generation and explicitly requeued next-generation records; real multi-submission Git candidate DAG rebuild/rejection remains pending. |
| Priority grant/revoke/expiry, grace, sev attempt preemption | Durable synthetic event records survive crashes; operational priority, leases, cancellation and scheduler contention are not implemented or qualified. |
| Fix-window/full-confirmation/revert | Persisted original incident and deadline survive changing restart clocks; failed confirmation leads to exact-tree-gated CAS compensation, including before/after publication kills. Concurrent successful-fix versus timeout/failed-confirmation arbitration, failed/conflicting revert freeze/notify remain pending. |
| No invented pass, manifest or reset deadline | Independent negative corruption checks reject stale/wrong identity, pass, manifest, deadline, compensation and lost acknowledgment ref; stale evidence refuses initial landing. |
| Platforms and durability | Linux/macOS/Windows manual CI evidence pending. No power-loss, disk-full or corrupted-WAL qualification. |

## Evidence collection

Qualification evidence will be collected from manual CI workflow runs after this PR is merged. This preparation includes no local evidence snapshots or measured-result claims. Review the resulting artifacts separately before accepting the spike; no workflow is dispatched by this change.

The harness exercises 26 kill checkpoints, two recovery passes per checkpoint, active/stale OS-lock exclusion, manifest reconstruction, stale-pass refusal and independent corruption probes. Use the harness README reproduction commands and collect the generated logs, source hashes and provenance after each CI run. Process-kill results cannot establish power-loss durability.

Pending acceptance requires operational scheduler/dependency/recovery-race extensions, additional crash positions as those mechanisms develop, and native macOS/Windows evidence. Failures in the eventual qualification protocol require ADR redesign before M1. This limited harness does not select shipping priority, lease, recovery-window or gate defaults.

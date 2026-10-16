# Operations — proposed rewrite

[ADR 0010](adr/0010-staged-mvp-qualification.md) controls MVP scope. Priority, speculative/batch tuning and automated fix-forward examples below remain deferred proposals, not required or enabled initial MVP behavior. The initial queue is single-class and dependency-aware; core validation, ownership and recovery correctness remain mandatory. Priority RFC #93 and feature-specific tests precede future enablement.

**Target operational contract.** The old worktree engine is retired. This MVP foundation layer provides lifecycle types, persistence, initialization and local serving. Validation slots and the full receive/landing queue are not available. See the [current CLI reference](cli.md) for the implemented surface; this page describes mechanisms to implement and qualify, not a claim of operational readiness.

## Lifecycle and trust

One `mai serve PROJECT --foreground` engine owns each authority, enforced by an exclusive OS lock before database opening and startup reconciliation. `mai status PROJECT` can autostart it; `mai stop PROJECT` requests graceful shutdown. Clients use a Unix socket or native Windows named pipe; native platform CI must qualify behavior. Stale PID metadata never establishes ownership, and the persistent lock file is never deleted. The initialization hook currently rejects every receive because registration is unavailable, regardless of engine readiness. See the [engine guide](engine.md) for the implemented surface and bounded startup behavior.

Authority qualification targets Linux and macOS with Git ≥ 2.40; S1 records Windows behavior and a receive fallback. Verify the installed Git rather than assuming a system-supplied version. Initial object format is SHA-1; imports with SHA-256, submodules or LFS reject clearly. Local alternates depend on authority object retention. Loopback tokens are a local transport boundary, not a multi-tenant service. Never mount the authority writable into agent containers.

## Recovery

Git's first-parent trunk is the acceptance ledger; SQLite WAL state is queue metadata. Before acknowledging receive, retain the submitted commit in `refs/mai/changes/<n>`. Registration must be idempotent. On restart, reconcile a `landing` row against verified authoritative-trunk ancestry and exact submitted generation, not `Change-Id` alone: mark an actual landing complete or requeue absent work. Rebuild candidates and slot state without accepting lost evidence or duplicating a landing. S7 must interrupt each registration and landing boundary to qualify these rules.

Pipeline candidates invalidated by an earlier rejection are discarded and rebuilt; stale passes cannot authorize a different landed tree. Timeouts terminate the process group and retain a bounded diagnostic tail. Retry a failure on the identical candidate and record flakiness; no pass means no landing.

The optional M1b full tier confirms a nonflaky culprit by rerunning before and after it, then opens an owned conflict record with logs, the culprit manifest and a temporary `sev` grant. Recommended recovery gives that agent a bounded fix-forward window: the `mai.fixes=<change>` submission runs the normal gate, lands at severe priority and requires full confirmation inside the same recovery deadline; a gate pass alone never establishes green trunk. Expiry or the configured failed-gate attempt budget triggers a compensating revert through the same gate, with evidence and affected pending dependency re-evaluation. Already-landed dependents require an explicit compensation plan and must never be blindly replayed or duplicated. A failed full confirmation is not recovery and does not reset the deadline. If the revert conflicts or fails, freeze and notify the configured owner. Immediate `revert`, `freeze` and `continue` remain explicit alternatives; default policy and S4 window selection are pending. History is never rewritten. An upstream-sync conflict has a configured resolver; unrelated queue work can continue against the last synced base.

## Backup and restore

Back up `authority.git`, `state.db` and required engine configuration as a coordinated recovery point. A safe procedure must quiesce intake and the writer, checkpoint or consistently copy SQLite including WAL state, preserve refs and objects, and protect token material separately from shareable archives. A live directory copy is not a proven consistent backup. The backup/restore issue must qualify restore into an isolated location, engine reconciliation, pending changes, drafts and validation evidence before claiming recoverability. No implemented backup command is promised yet.

## Retention and capacity

Change refs remain for a configured interval after terminal outcomes; drafts remain while registered and for an interval afterward. Concrete retention durations are not chosen. Do not prune objects still needed by queues, dependency chains, candidates, drafts, evidence or alternates-backed clones. Maintenance runs during safe idle periods. Workspace removal requires explicit handling of dirty files and pending/draft work rather than unconditional deletion.

Bound validator count, output, receive sizes and changed-path counts. Warm slots re-bootstrap when configured inputs change. Each gets a private temporary directory, `MAI_SLOT` and `MAI_PORT_BASE`; suites must use these for ports and databases because mai does not sandbox tests. Measure bootstrap separately from checkout and gate runtime before adding materialization machinery.

## Metrics and escalation

The proposed `mai status --metrics` reports p50/p95 recovery time from rejection to relanding and breakage detection to green; death-to-resumed-landing starts in M4. Track full-tier failures per landed change and revert rate as trunk guardrails. Split time to land into queue wait and validation; report gate duration, slot utilization, discarded speculation, outcome reasons, flaky rate and human interventions including priority grants. Report time to land per class, lease preemptions, aging promotions, and in-window fix-forward success versus reverts. Add scope-drift rate from M2 and late-conflict waste in M3 and resume success in M4. These metrics are currently unimplemented and unmeasured.

Capture reproduction refs, configuration hash, tree identity, attempt results and a bounded log tail for diagnosis. Do not expose tokens or raw agent context in diagnostics. Human approval is required for protected policy changes; models may escalate conflicts but never approve or accept them. Qualification evidence and current failures belong with implementation work, not a claim that the design has already achieved its targets.

## Manifests and priority operations

M1 computes file-unit manifests against the submitted base at receive and against the actual parent at landing. Keep `refs/notes/mai-objects` and the `state.db` unit index consistent with accepted history and include them in backup/reconciliation tests. `Mai-Objects:` lists up to 20 units, otherwise records a count and note pointer. Parser versions and explicit fallback reasons accompany later M2 object units. Agent-declared `mai.touches` is intent, not authoritative evidence. Manifests help diagnosis and affected-test selection; they never independently approve correctness.

Use the proposed `mai priority grant <agent|change> <class> --reason ... --ttl ...` and `mai priority revoke` interfaces only after implemented. Grant actors and revoke selectors still need qualification; do not assume a token has grant authority. Audit expiry, inheritance, severe in-flight caps and aging. Transfer path leases in M1 and object leases in M2 only after notice and grace, then reject stale-owner publication. Priority conflict winners determine which side resolves; they do not silently replace or discard the other side's work. Only severe work may cancel lower-class speculative candidates, whose submissions remain queued for rebuild. `mai log --object <unit>` is the planned history lookup.

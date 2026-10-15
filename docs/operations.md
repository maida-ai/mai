# Operations — proposed rewrite

**Not implemented.** The existing 0.0.1 binary has no daemon or validation slots. Its current recovery limitations are documented in [CLI reference](cli.md#submission-results-and-recovery). This page specifies the operational contract to implement and qualify after M0, not commands to run against an existing authority.

## Lifecycle and trust

One `mai serve` engine owns each authority, enforced by an exclusive lock. The CLI auto-starts it; `mai serve --foreground` supports an external supervisor. Clients communicate through a Unix socket, with a named-pipe implementation planned for best-effort Windows support. If the engine is unavailable, the receive hook rejects with an actionable message instead of acknowledging unregistered work. A stale lock is diagnosed through ownership/liveness checks, not blindly deleted while another writer may run.

Authority qualification targets Linux and macOS with Git ≥ 2.40; S1 records Windows behavior and a receive fallback. Verify the installed Git rather than assuming a system-supplied version. Initial object format is SHA-1; imports with SHA-256, submodules or LFS reject clearly. Local alternates depend on authority object retention. Loopback tokens are a local transport boundary, not a multi-tenant service. Never mount the authority writable into agent containers.

## Recovery

Git's first-parent trunk is the acceptance ledger; SQLite WAL state is queue metadata. Before acknowledging receive, retain the submitted commit in `refs/mai/changes/<n>`. Registration must be idempotent. On restart, reconcile a `landing` row against verified authoritative-trunk ancestry and exact submitted generation, not `Change-Id` alone: mark an actual landing complete or requeue absent work. Rebuild candidates and slot state without accepting lost evidence or duplicating a landing. S7 must interrupt each registration and landing boundary to qualify these rules.

Pipeline candidates invalidated by an earlier rejection are discarded and rebuilt; stale passes cannot authorize a different landed tree. Timeouts terminate the process group and retain a bounded diagnostic tail. Retry a failure on the identical candidate and record flakiness; no pass means no landing.

The optional M1b full tier confirms a nonflaky culprit by rerunning before and after it. The recommended recovery is a compensating revert that passes the same gate, records evidence and reevaluates affected pending dependents. Already-landed dependents must not be blindly replayed: recovery needs an explicit compensating/replay plan that cannot duplicate accepted content; indeterminate recovery freezes acceptance with an assigned owner. If the revert conflicts or fails, freeze and notify the configured owner. Explicit `freeze` and `continue` policies exist in the design, but their default remains a maintainer decision. History is never rewritten. An upstream-sync conflict has a configured resolver; unrelated queue work can continue against the last synced base.

## Backup and restore

Back up `authority.git`, `state.db` and required engine configuration as a coordinated recovery point. A safe procedure must quiesce intake and the writer, checkpoint or consistently copy SQLite including WAL state, preserve refs and objects, and protect token material separately from shareable archives. A live directory copy is not a proven consistent backup. The backup/restore issue must qualify restore into an isolated location, engine reconciliation, pending changes, drafts and validation evidence before claiming recoverability. No implemented backup command is promised yet.

## Retention and capacity

Change refs remain for a configured interval after terminal outcomes; drafts remain while registered and for an interval afterward. Concrete retention durations are not chosen. Do not prune objects still needed by queues, dependency chains, candidates, drafts, evidence or alternates-backed clones. Maintenance runs during safe idle periods. Workspace removal requires explicit handling of dirty files and pending/draft work rather than unconditional deletion.

Bound validator count, output, receive sizes and changed-path counts. Warm slots re-bootstrap when configured inputs change. Each gets a private temporary directory, `MAI_SLOT` and `MAI_PORT_BASE`; suites must use these for ports and databases because mai does not sandbox tests. Measure bootstrap separately from checkout and gate runtime before adding materialization machinery.

## Metrics and escalation

The proposed `mai status --metrics` reports p50/p95 recovery time from rejection to relanding and breakage detection to green; death-to-resumed-landing starts in M4. Track full-tier failures per landed change and revert rate as trunk guardrails. Split time to land into queue wait and validation; report gate duration, slot utilization, discarded speculation, outcome reasons, flaky rate and human interventions. Add late-conflict waste in M3 and resume success in M4. These metrics are currently unimplemented and unmeasured.

Capture reproduction refs, configuration hash, tree identity, attempt results and a bounded log tail for diagnosis. Do not expose tokens or raw agent context in diagnostics. Human approval is required for protected policy changes; models may escalate conflicts but never approve or accept them. Qualification evidence and current failures belong with implementation work, not a claim that the design has already achieved its targets.

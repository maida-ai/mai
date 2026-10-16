# Gradual Maida asset adoption plan

`mai` belongs to the **maida-ai** family, but Maida is an early adopter, not a runtime dependency. The Rust rewrite does not authorize moving production assets. No assets are migrated by installation, initialization, or this milestone.

## Promotion sequence

1. Disposable test repositories: synthetic workloads, real-Git correctness tests, conflict cases, import/export, and interruption-state recovery. Rollback by discarding only explicitly disposable data.
2. Representative noncritical workloads: realistic file sizes/history and agent behavior; establish measured resource/latency targets and operational limits.
3. Shadow copies of Maida assets: non-sensitive read-only copies; existing Git remains authoritative. Validate matching snapshots and declared provenance.
4. Dual-system validation: reconcile hashes, revision exports, backup restoration, and divergence handling. Specify one accepted writer per repository to prevent split brain.
5. Recoverability and performance qualification: crash/power-loss drills, disk-full cases, access policy review, realistic fanout, documented soak period, and rollback playbook.
6. Incremental migration of selected assets: owner-approved low-risk canaries, explicit cutover and freeze points, recoverable Git export, monitored resource use.
7. Broader adoption: opt in repository by repository only after demonstrated reliability, security, and recoverability. Maintain a documented source export and recovery procedure.

## Gates before production writes

The 0.0.1 MVP foundation has replaced the experimental worktree engine after [scoped M0 acceptance](project.md#m0-qualification). The complete Git-native queue and its operational safeguards remain under development. Neither model tests nor passing throwaway spikes qualify production adoption; each implemented correctness path and the production adoption gates below still need evidence.

Qualify acknowledged-push object retention, exact pipeline validation, first-parent revision provenance, crash reconciliation and authority/state.db backup restoration. Exercise dependent squash submissions, interrupted sync, protected-path approval and upstream export cycles. Process-kill evidence does not certify power-loss durability. Local clones that borrow objects through alternates require proven retention or dissociation before authority maintenance or removal.

Keep the forge's main authoritative for human work and export mai's integration line to a separate branch or PR. Upstream syncs must pass the same gate; mai never silently force-pushes or replaces human main. Test confirmed-culprit bounded fix-forward with a temporary severe grant and full confirmation, window/attempt expiry into a validated revert, and freeze only if that revert fails. Qualify file/object manifest reconstruction, priority grant audits and expiry, dependency inheritance, aging and lease preemption without discarded work or weaker gate evidence. A narrower severe gate remains unapproved. M1b defaults and its ordering relative to external users remain open maintainer decisions.

Qualify per-agent tokens and container reachability for the supported trusted-local scope; neither clones, receive hooks nor validator slots are sandboxes. Multi-tenant authorization and hostile remote use are outside the MVP. Agent drafts contain only successfully pushed committed work; no promise covers uncommitted or unpushed changes.

Measure the [recovery and validation metrics](project.md#success-measurements) on representative 3–30-agent repositories against a worktree/PR baseline. Set workload-specific targets from evidence, including human intervention and broken-trunk rate. [Possible futures](project.md#possible-futures) do not create current adoption prerequisites.

Publish the migration/rollback playbook and obtain explicit sign-off from every affected repository owner. External adopters receive the same open interfaces and capabilities as Maida.

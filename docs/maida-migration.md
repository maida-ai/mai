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

Accepted revisions must survive process termination and qualified filesystem failure without lost or duplicated publication. The Rust milestone tests intermediate recovery states but does not certify power-loss or incomplete initialization recovery. Workspace reset and registry reconciliation need operator procedures.

Qualify authenticated agents, least-privilege access, secrets handling, and execution isolation before hostile multi-machine use. Current worktrees are not sandboxes; current identities are declarations. Keep GitHub optional and test Git export/restore with all relevant revisions, binary files, modes, renames, and provenance.

Measure p50/p95/p99 submission latency, accepted revisions per second, workspace/cold-start latency, contention/retry rates, lock hold time, CPU, RSS, and storage growth for 1/16/64/256 agents and 1,024 where hardware permits. Set workload-specific targets from evidence. Never promote solely because unit tests or a synthetic throughput number pass.

For adoption of future services, additionally qualify durable draft/request outcomes, source export, reachable-object retention, sparse views, directory authorization through caches/indexes, and attestation/revert behavior from [the capability contract](agent-native-capabilities.md). A local full-worktree test is not evidence for those capabilities.

Publish the migration/rollback playbook and obtain explicit sign-off from every affected repository owner. External adopters receive the same open interfaces and capabilities as Maida.

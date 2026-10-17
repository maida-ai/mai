# Benchmark and recovery protocol

mai aims to reduce recovery effort while preserving an exact gate at trunk. A faster acknowledgment is not a faster landing, and passing a synthetic workload is not evidence of realistic agent throughput. This protocol defines comparable measurements; it reports no measured advantage. Performance tuning, the 15% warm-overhead target and differentiated priority remain deferred under [ADR 0010](adr/0010-staged-mvp-qualification.md).

## Match the comparison

Use Git worktrees plus the team's normal PR workflow as the primary baseline. Pin the same starting repository commit, submitted patches and dependency graph, gate commands and executable policy, machine resources, tool versions and concurrency. Use ordinary independent clones where that is the actual baseline, and label the difference. Keep required reviews and checks equivalent; record hosted CI/network/review wait separately from local execution. Do not compare a local mai validation with a full hosted PR cycle and call the difference engine overhead.

Record the exact mai commit, build profile, lockfile, OS/filesystem, Git/Rust versions, CPU/RAM, repository tracked-file count and byte size, gate revision, worker count, cache policy and run commands. Report each side's setup/materialization time separately, then cold and warm steady-state runs. Disclose retained cache directories and invalidation triggers. Alternate baseline/mai run order, avoid competing machine workloads, use the same fixed workload seed and retain every failed run. Repeat enough times to show variability; publish sample counts and raw observations rather than an unsupported percentile from a tiny sample.

## Exercise recovery as well as the happy path

| Workload | Required observations |
| --- | --- |
| Disjoint changes | Same final content and gate coverage; acknowledgment, queue wait, validation and landing times separately |
| Overlapping edits | Same intended edits; conflict reason, preserved submitted objects, resolution effort and eventual result |
| Dependent changes | Parent and child identities; rejection/supersession handling; no child accepted without its prerequisite |
| Deterministic gate failure | Rejection diagnostics and time from corrected submission to a passing landing |
| Interrupted engine/client | Exact interruption point; acknowledged objects retained; retry identity; restart time and terminal outcome uniqueness |
| Bootstrap/cache change | Cold/warm distinction, invalidated inputs and successful gate on the exact candidate |
| Dirty agent workspace | Dirty/untracked contents before and after explicit sync; no hidden work loss |

Only run scenarios supported by the tested implementation. The current manual validator can measure gate/bootstrap execution and slot behavior, but cannot establish receive-to-land latency or safe sync. Process-kill experiments establish process-interruption behavior, not power-loss durability. Protected policy changes must still require explicit approval, and uncertain checks must never be counted as passes. Keep unsupported cases in the report as unavailable rather than assigning zero time or success.

## Report outcomes, not just speed

Measure elapsed wall time with a monotonic clock and retain individual events. For completed workflows, report acknowledgment-to-terminal outcome and submission-to-land, decomposed into queue wait, candidate preparation and gate time. Recovery measurements are rejection-to-successful-reland and confirmed-breakage-to-green, only when those features exist. Report active human minutes/interventions separately from elapsed waiting; record scripted agent actions separately from human actions. Include median and tail latency when sample size permits, plus failures, timeouts, rejected changes and unfinished work so survivor bias cannot hide slow failures.

Correctness guardrails accompany every timing: lost acknowledged submissions, duplicate landings, gate/tree mismatch, lost local work and broken trunk. A nonzero guardrail failure invalidates a speed claim for that workflow. Count full gate invocations and discarded work to expose compute cost. A benchmark run that never creates or lands a change has no broken-trunk-rate denominator. Do not extrapolate a local run to hundreds or thousands of agents or distributed operation.

## Make evidence reproducible

Keep harnesses, fixed fixtures and workload manifests under `experiments/`, separate from product tests. A run bundle should include the command transcript, environment manifest, per-event CSV/JSON, bounded diagnostic logs, final Git identities and assertions, plus a report linking the exact source revision. Use opaque local identities and synthetic/consented repositories; do not export source contents, prompts, credentials or raw agent context. Review any export explicitly. Place reviewed summaries under `docs/spikes/`; link raw artifacts and include unsuccessful runs, limitations and reproduction commands. Any experiment CI must be manual-only, explicitly select platforms and use bounded artifact retention.

Start with a small deterministic local smoke workload. Its purpose is to make comparison and recovery failures reproducible, not to qualify M1 throughput or satisfy the external-user gate. Practical workload claims require dogfooding and consented real-repository observations; [project measurements](project.md#success-measurements) define that later evidence and the 5–10-user gate.

# S4 synthetic queue and recovery assessment

Historical status — 2026-10-17: This report preserves the 2026-10-15 synthetic assessment against the original criteria. [historical ADR 0010](https://github.com/maida-ai/mai/blob/9ee6acc844acc3e8272ebd24d8301c6b4af6870c/docs/adr/0010-staged-mvp-qualification.md) subsequently completed scoped M0 and deferred realistic S4 performance qualification until useful MVP observations. The original measurements, NO-GO wording and model limitations below remain historical evidence; S4 completion is not a blanket implementation gate.

**NO-GO for full #54 acceptance; GO only for collecting and reviewing this synthetic sensitivity grid.** Native jobs and all 16 harness units pass on all three platforms. Metrics are simulated seconds, not measured production gate performance. Realistic workloads and shipping recommendations remain pending as agreed with the maintainer.

## Results

Each platform produced 96 scenarios (24 grid inputs × four modes), 384 per-class metric rows and 3,840 recovery-overlay rows. Seeds are 7/42; per-agent intervals are 600/1,800/3,600 seconds; permanent failure inputs are 0/15%; each uses ordinary/severe priority mixes and 30 agents. Per-class p50/p95, utilization, waste, preemption, inheritance, aging, cap reconciliation, grant cost and backlog are retained in `results/metrics.log`; complete traces are losslessly compressed.

Review found all 384 metric rows identical across platforms; the elapsed wall-time footer differs as expected. Pipeline-2 has a shorter final drain time than sequential in 16 of 24 paired inputs, with eight ties. This comparison is a finite synthetic result, not the required benefit at agreed realistic failure rates. It does not substitute drain time for per-class latency.

| Mode | Low p95 range s | Normal p95 range s | High p95 range s | Sev p95 range s |
| --- | --- | --- | --- | --- |
| Sequential | 152–12,744 | 142–12,664 | 129–12,592 | 96–8,732 |
| Pipeline-2 | 95–4,868 | 93–4,918 | 91–4,845 | 94–3,725 |
| Pipeline-4 | 93–1,374 | 92–1,194 | 91–1,250 | 94–907 |
| Batch-4 | 126–12,834 | 125–12,754 | 125–12,682 | 96–8,822 |

Ranges are minima/maxima of scenario-level class p95 values, not pooled percentiles. Nominal single-slot load is 3.0/1.0/0.5 at the three intervals; overload/backlog is explicit. Each platform's recovery rows contain 1,728 Reverted, 768 FreezeNotify, 768 AwaitEvidence and 576 Fixed outcomes. These are counts over deliberately constructed incidents, not empirical probabilities or recovery reliability estimates.

## Criterion assessment

| #54 criterion | Evidence / assessment | Remaining work |
| --- | --- | --- |
| Seeded sequential/pipeline/batch comparison and pipeline-2 benefit | PASS for synthetic replay; realistic acceptance pending | Collect labeled arrivals/gate/failure/flake/conflict inputs and replay via existing CSV interface |
| Immediate and 5/15/30-minute windows, attempts and confirmation | Recovery overlay and failure/timeout checks pass | Couple recovery service and full-tier interference to queue capacity, fix submissions and dependent compensation |
| Per-class/resource/recovery metrics and recommendations | All metrics retained; grant cost assumes 60 seconds | Measure grant effort and operational costs before recommending mode, slots or fix window |
| No starvation under admissible sustained load | Finite draining and conditional argument only | Establish bounded service/residual capacity with sustained arrivals, repeated sev demand, leases and dependency chains |
| Grants/expiry/cap/grace/attempt-only preemption | Policy units and queue audits pass | Integrate real lease/object graph; lease transfer currently separate from conflict-delay model; cap reconciliation policy needs acceptance |
| Exact pipeline/weaker batch evidence, same-candidate retries, attribution | Synthetic identity and negative checks pass | Real Git/full-tier binding outside numeric model; batch uses singleton fallback after failure |
| Reproducible simulator/report | PASS | No product claim or default selection |

## Follow-up

Retain the editable grid and external workload schema in the [harness README](../../../experiments/s4/README.md). Collect realistic inputs later as agreed. Extend the coupled model before interpreting recovery-window economics. No threshold or shipping default is selected from this run.

## Conditional starvation argument and its limits

A useful sufficient condition is a bounded service model: arrivals have finite bursts, dependency depth and resolution delays are bounded, each attempt has bounded duration, retries and invalidations have bounded total cost, and admitted sev work leaves strictly positive residual service for non-sev work. Dependencies must be acyclic and their predecessors scheduled inside the sev cap. Expiry/revocation must not manufacture grants. In this model a non-sev change reaches high after at most two aging intervals, and FIFO arrival order within high prevents newly arriving high changes from overtaking it. Its earlier arrived prerequisites and older high changes form a finite predecessor set. A strictly positive lower bound on residual service therefore clears their bounded work and then the change's own bounded work; induction over bounded dependency depth extends the argument to dependent changes.

This is a conditional reasoning argument, not a claim that this harness or proposed production engine has established those service assumptions. A sev cap alone bounds concurrency, not incoming sev demand. Unbounded retries, cascading invalidations, unresolved leases or saturated sev arrivals can destroy the residual-service bound even when an average nominal arrival rate looks small. The finite grid intentionally includes overload, but does not prove a sustainable-rate bound or justify a universal starvation claim. Realistic-load acceptance and a formal service bound remain open.


## Reviewed evidence and verification

[Run 38701913394, attempt 1](https://github.com/maida-ai/mai/actions/runs/38701913394) executed `38e1f8073b939a28cea2a887307994620549bdc5` on 2026-10-15. The [snapshot](evidence/2026-10-15-run-38701913394-attempt-1/README.md) retains every collected artifact file with original and retained SHA-256s, job conclusions, source hashes and independently checked summaries. All recorded harness source hashes match the executed revision. See the [go/no-go decision](go-no-go.md) and [reproduction instructions](../../../experiments/s4/README.md).

This evidence-only review checked source identity, artifact completeness, reported outcomes against raw logs and the executed assertions, documentation links and whitespace. No experiment was rerun or workflow dispatched; no product code, thresholds, defaults or ADR status changed.

Baseline product checks passed during review: `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings` and `cargo test --all-targets --locked` (34 tests). Disposable builds stayed under `.throwaway/`. No new local experiment evidence was generated.

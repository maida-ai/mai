# S4 queue qualification: synthetic preparation, not accepted

**Go/no-go: no qualification claim; keep #54 and M0 open.** The simulator and native manual workflow are prepared, with qualification pending CI evidence. This is a useful finite sensitivity experiment, not realistic workload acceptance, a proof of the complete production scheduler, or a choice of mode, slot count or recovery default. Current 0.0.1 still uses the old worktree engine.

## Reproduction and evidence

The [harness README](../../../experiments/s4/README.md) documents the model, editable [grid](../../../experiments/s4/grid.csv), external workload CSV schema, commands and limitations.

Qualification evidence will be collected from manual CI workflow runs after this PR is merged. This preparation includes no local evidence snapshots or measured-result claims. Review the resulting artifacts separately before accepting the spike; no workflow is dispatched by this change.

## Issue criteria mapping

| #54 criterion | Harness coverage | Remaining acceptance work |
| --- | --- | --- |
| Seeded sequential/pipeline-K/batch-M and pipeline-2 benefit | 96 scenarios, immutable input traces, seeds and per-class metrics; comparative results pending CI | Calibrate inputs from workload data; agree realistic failure/flake/conflict rates; evaluate operational costs and maintainer recommendations |
| Immediate/5/15/30-minute recovery, attempts and mandatory confirmation | 3840 overlay rows; fixed incident deadline; bounded attempts; confirmation failure/timeout and revert failures unit-tested | Couple recovery service, fix submissions, full-tier interference and dependent compensation back into queue scheduling; assess representative incidents |
| Class p50/p95, utilization, waste, preemption, inheritance/aging, grant cost | Full CSV metrics; actual simulated service and explicit 60-second grant-cost assumption | Measure grant effort, tune sensitivity independently and assess platform/production behavior |
| No starvation under admissible sustained traffic | Repeated 30-agent finite arrivals; class outcomes and overload/backlog reported; conditional argument below | Establish service bounds and admissible load for real gates, cancellation, dependency/lease delays and sustained arrivals; finite draining is insufficient |
| Grants/expiry/cap/grace/preemption | Separate policy tests plus queue grant audits, effective running-priority reconciliation, inherited cap, retained submissions | Integrate the actual lease/object graph and define cap-reconciliation policy with maintainer approval; native hosted verification |
| Exact pipeline/weaker batch evidence, same-candidate retries, confirmed compensation | Separate stored pass identity, parent/config/tree negative checks, labeled batch scope, retry identity trace, inconclusive-attribution negative checks | Real Git evidence binding and full-tier attribution are outside this numeric model; batch diagnosis currently remains singleton after first failure |
| Throwaway harness/report, no product change | `experiments/s4/`, this report, manual workflow only | Qualification remains open; no defaults, issue closure or product replacement authorized |

## Conditional starvation argument and its limits

A useful sufficient condition is a bounded service model: arrivals have finite bursts, dependency depth and resolution delays are bounded, each attempt has bounded duration, retries and invalidations have bounded total cost, and admitted sev work leaves strictly positive residual service for non-sev work. Dependencies must be acyclic and their predecessors scheduled inside the sev cap. Expiry/revocation must not manufacture grants. In this model a non-sev change reaches high after at most two aging intervals, and FIFO arrival order within high prevents newly arriving high changes from overtaking it. Its earlier arrived prerequisites and older high changes form a finite predecessor set. A strictly positive lower bound on residual service therefore clears their bounded work and then the change's own bounded work; induction over bounded dependency depth extends the argument to dependent changes.

This is a conditional reasoning argument, not a claim that this harness or proposed production engine has established those service assumptions. A sev cap alone bounds concurrency, not incoming sev demand. Unbounded retries, cascading invalidations, unresolved leases or saturated sev arrivals can destroy the residual-service bound even when an average nominal arrival rate looks small. The finite grid intentionally includes overload, but does not prove a sustainable-rate bound or justify a universal starvation claim. Realistic-load acceptance and a formal service bound remain open.

## Review and follow-up

Review the independent recovery overlay, the synthetic cap-reconciliation cancellation policy, singleton batch fallback, grant-cost assumption and conflict-delay abstraction before using numbers for decisions. The harness can immediately replay measured workload CSVs and edit its grid; no production dependencies or product implementation were added. Register and manually run the workflow on all native platforms, review artifacts separately, then collect realistic arrivals/gates/incidents and extend the coupled model before any S4 acceptance or shipping recommendation.

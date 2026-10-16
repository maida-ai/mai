# ADR 0010: Staged MVP qualification and evidence-led optimization

Status: accepted scope and sequencing decision by the maintainer on 2026-10-16; implementation is not shipped. This amends the blanket M0 qualification requirement in ADRs 0003/0004. It does not accept their remaining MVP design details or close acceptance tasks #58/#70.

## Context

mai is a non-commercial MVP. Requiring representative scheduling and performance evidence before a usable implementation creates a circular dependency. Synthetic experiments establish limited mechanisms and reveal real failures, but cannot supply realistic demand, urgency or human-workflow distributions. Correctness requirements, performance targets and unresolved priority design need different gates.

## Decision

M0 now gates implementation on review of the scoped mechanism evidence, review of fresh native CI for S3's corrected functional paths and explicit acceptance of the MVP portions of ADRs 0003/0004 through [#58](https://github.com/maida-ai/mai/issues/58) and [#70](https://github.com/maida-ai/mai/issues/70). S4 completion and the full future S7 matrix are not blanket prerequisites. This decision changes sequencing, not the truth of existing reports: failed measurements remain failures against their original criteria, missing evidence remains missing, and historical artifacts are unchanged.

The initial MVP uses a simple single-class queue respecting dependency order, with arrival order among eligible changes. Start with a serial reference path; speculative concurrency and batching are optional later work justified by practical need, not preconditions for collecting MVP data. No throughput, starvation, universal latency or realistic workload guarantee is claimed. Provisional settings are labeled development assumptions and can change without a compatibility layer. This does not select final performance-tuned defaults.

Differentiated priorities, grants, inheritance/aging, priority preemption and priority-driven conflict ownership are deferred to [RFC #93](https://github.com/maida-ai/mai/issues/93). Existing proposals remain available for discussion, not implementation obligations. [#72](https://github.com/maida-ai/mai/issues/72), [#73](https://github.com/maida-ai/mai/issues/73) and priority-dependent automatic recovery in [#68](https://github.com/maida-ai/mai/issues/68) await that decision and their own correctness evidence. Ordinary lease ownership checks, protected approval and safe handling of failures are not deferred. Unqualified automatic recovery remains unavailable; do not enable a sev lane or narrower gate implicitly.

## Gates by kind of work

| Work | Required before implementation or enablement | Deferred evidence |
| --- | --- | --- |
| S3 validator readiness | Review the Rust temp/source-path and Windows native-path corrections on fresh native CI; demonstrate truthful gates, invalidation, isolation, stale-cache rejection and cleanup for supported paths | The <15% warm-overhead target and representative workload performance move to later measurement; preserve raw timing and original failure results |
| S4 scheduling economics | No blanket implementation gate; retain synthetic grid and explicit assumptions | Realistic arrival/failure/flake/conflict inputs, recovery capacity coupling, comparative mode/slot/window recommendations and admissible-load/starvation arguments after useful MVP observations |
| S7 crash correctness | Deterministic separate-process crash/retry tests accompany each implemented transition and pass before that feature is enabled; actual engine regressions tracked in #69 | Tests for unimplemented priority/recovery mechanisms wait for those mechanisms; they do not block unrelated implementation |
| Priority policy | Reviewed RFC/ADR decision and feature-specific correctness tests before enablement | Need, roles, fairness policy and performance claims informed by opt-in dogfooding |
| Optimization | A measured practical usability problem, a profile or baseline, a small justified change and regression checks | Broad tuning, COW/overlay/lazy work, speculative concurrency and batching absent demonstrated need |

Non-negotiable correctness includes retaining acknowledged objects, exact submitted identity and terminal outcomes, exact final-tree/parent/trusted-policy evidence, CAS landing, no duplicate landing or compensation, no invented pass/manifest, work-preserving cancellation/sync, and original recovery deadlines whenever recovery is implemented. Keep success and negative cases. No untested crash safety, power-loss durability or universal correctness guarantee is inferred from finite fixtures. A failing implemented correctness path is not reclassified as a deferred performance question.

## S3 implementation and evidence status

The lower S3 fix layer gives the pinned ripgrep suite private non-repository temporary directories, corrects its warm/poison source path to `crates/core/main.rs`, validates tracked fixture paths and replaces separator-sensitive slot assertions with native path comparisons. The real upstream gate remains unchanged. Local Linux functional checks pass; native Windows/macOS confirmation remains pending manual CI and does not follow from local success.

S3 now records functional and performance outcomes separately. This amendment selects functional mode by default for the wrapper/manual workflow; `S3_MODE=run` still enforces the original strict <15% threshold. Both modes run the same samples and correctness checks and retain failed performance results. Functional failures always exit nonzero. This is an explicit acceptance-policy change, not a new performance pass. Existing evidence and original NO-GO reports are unchanged; #53 remains open for fresh native evidence. Harmless manifest-byte invalidation and unimplemented Windows COW probing remain documented limits, not newly qualified behaviors.

## Data collection after an MVP exists

Use [#11](https://github.com/maida-ai/mai/issues/11) for opt-in, local, bounded event recording: submission/attempt timestamps, dependency relationships, gate durations/outcomes, retries, conflicts, cancellations and attributable manual interventions. Use opaque identifiers; exclude source contents, prompts, credentials and raw agent context. Document retention and explicit export/review before sharing. Keep unavailable fields explicit and distinguish synthetic, dogfood and volunteer inputs.

Dogfood on this repository, then use consented volunteer repositories through [#37](https://github.com/maida-ai/mai/issues/37). Replay labeled traces through S4's existing editable grid/workload CSV interface. Preserve repository/toolchain/hardware and warm/cold context; finite observations do not establish universal guarantees. Existing CI histories can optionally supply rough gate-duration/failure distributions, but not representative submission, conflict or priority demand; collecting them is not an MVP prerequisite. Record the original 15% validator target in [#54](https://github.com/maida-ai/mai/issues/54) for later evaluation rather than tuning the MVP to synthetic timings.

## Consequences and tracking

Keep #53 open until fresh native CI verifies the functional corrections, #54 open as post-MVP empirical research and #57 open as incremental crash qualification. Their prior NO-GO reports remain valid assessments of their original broader criteria. Remove S4/S7 completion dependencies from #58/#70; retain review of their evidence and this accepted redesign. Do not require priority implementation to complete baseline validation or its regression harness. M1 no longer requires five-agent zero-intervention/zero-broken-trunk performance demonstrations before it can supply data; use bounded dogfood observations and report failures honestly. The external-user gate for M2 and later remains unchanged.

Premature optimization is discouraged. Prefer a small, maintainable, correct MVP and collect measurements while using it. This decision authorizes the staged plan, not automatic issue closure, an accepted production release, replacement of source before scoped design acceptance.

# Project status and direction

mai is an Apache-2.0 Rust project for 3–30 coding agents working on one repository, independent of core Maida. The proposed rewrite is a Git-native integration queue between agents and their existing forge. It does not run agents, call models in core, replace review or CI, or sandbox tests.

## Working today

The 0.0.1 MVP foundation replaces the old worktree engine with a versioned lifecycle model: immutable generations, exact dependency identities, explicit attempts and terminal outcomes. SQLite persistence, authority initialization/reconciliation, local single-writer serving, independent workspace creation/bootstrap and manual exact-candidate validation are implemented. Manual validation uses trusted trunk policy, bounded validator slots and explicit protected-change approval; it never lands a change. Explicit receive retains immutable submissions; `mai integrate` validates and squash-lands one selected generation through the engine, respecting registered parent dependencies, file manifests and ordinary leases. An automatic queue scheduler, safe sync, drafts and regeneration remain implementation work. [Getting started](getting-started.md) and [CLI reference](cli.md) document the implemented surface.

The long-term direction is coordination across hundreds or thousands of overlapping agents with less recovery effort. The accepted local MVP scope remains 3–30 agents; neither that workload nor distributed scale is performance-qualified. The immediate test is whether an operator can reproduce a failure, preserve work and recover without specialist intervention while keeping the trunk gate intact.

## Current limits

Use only disposable trusted repositories during MVP development. Persistence, startup reconciliation and manual validation have dedicated Git/process tests, with finite explicit receive/integration paths covered separately; model transitions and synthetic landing fixtures alone do not establish an operational automatic queue. Each enabled receive and publication path needs its own interruption and dependency tests. No complete integration queue, authentication or remote multi-tenant service is available. Consult the [current CLI surface](cli.md) before using planned commands.

## Rewrite sequence

The maintainer accepted [ADR 0010](adr/0010-staged-mvp-qualification.md) on 2026-10-16: build a small correct MVP before realistic performance qualification or differentiated priority policy. Scoped MVP design acceptance and fresh native verification of S3 functional corrections are recorded in ADRs 0003/0004 and the S3 functional review. Qualification is not established by a prototype or a report from one platform.

| Stage | Exit criteria |
| --- | --- |
| M0 — Scoped design lock | Completed: fresh native S3 functional evidence, mechanism review and MVP acceptance in ADRs 0003/0004 through #58/#70. S4 and future S7 coverage are not blanket blockers |
| M1 — Local MVP | Single-writer engine, durable registration, typed outcomes, dependent squash landing, serial exact-candidate validation, protected approval, ordinary leases, clones/drafts/sync/bootstrap and file manifests. Feature-specific crash tests and opt-in local metrics; no differentiated priority or realistic performance guarantee |
| M1b — Containers and forge | Loopback smart HTTP/tokens and validated upstream sync/export. Priority-driven automatic recovery awaits RFC #93 and feature-specific crash qualification; it does not block the initial MVP |
| Gate — External users | 5–10 users on real repositories; compare worktrees plus PRs with mai on human minutes, time to land and broken trunk |
| M2 — Object-aware integration | Parsers add symbol, config-key and module-scope units; object leases use containment/preemption; manifests gain object precision and [scope-drift outcomes](https://github.com/maida-ai/mai/issues/74) |
| M3 — Live awareness | #42–#44: intent, draft-derived activity and deterministic conflict ownership |
| M4 — Resumable, explainable work | #45–#47: opt-in context, recovery from authority drafts and attribution queries |
| M5 — Cross-agent groups | #49: atomic topic landing across agents; fork/fuse uses native Git and documentation |
| M6 — Policy guards | #50: external policy checks using the protected-path approval boundary |

M1b before the users gate is recommended, not settled. The maintainer has authorized technically independent implementation work to proceed before external-user feedback when it does not depend on that evidence. This exception permits development; it does not waive external-user exit criteria, select unresolved product defaults or qualify later milestones. The immediate implementation focus remains the coherent M1 workflow. There are no invented dates or automatic version promotions. Track implementation through [issues and subissues](https://github.com/maida-ai/mai/issues/1), [milestones](https://github.com/maida-ai/mai/milestones) and the [Project](https://github.com/orgs/maida-ai/projects/9).

Architecture decisions and their rationale live in versioned ADRs. Discussions can host unresolved debate; acceptance tasks close once the reviewed decision and evidence are committed. Implementation issues, subissues, milestones and the Project track delivery.

## Delivery order and readiness

Separate implementation readiness from acceptance evidence. Durable receive and idempotent retry can be developed alongside file manifests, documentation and independent crash tests. Candidate construction depends on received identities and dependency mapping; landing additionally depends on exact trusted validation, policy/lease enforcement and recovery reconciliation. Integrate those contracts before exposing a receive-to-land workflow. Safe sync and actionable outcome commands complete the agent-facing loop. Later technically independent foundations can proceed under the maintainer exception above, but cannot stand in for missing M1 correctness or real-user feedback.

A merged component is implemented, not automatically operationally qualified. Record which commands are available, which interruption paths and native platforms have passed, and which comparisons remain unmeasured. Keep incomplete issue acceptance criteria open and report partial delivery as foundations until the complete path works. External-user validation still requires 5–10 users and equivalent baseline comparisons; no amount of synthetic testing completes that gate.

## M0 qualification

| Spike | Required evidence |
| --- | --- |
| [S1](https://github.com/maida-ai/mai/issues/51) Transport | Host and container receive, options, status, draft policy and fallback on Linux/macOS Git ≥ 2.40; record Windows result |
| [S2](https://github.com/maida-ai/mai/issues/52) Merge | Differential corpus matching cherry-pick for disjoint/same hunks, rename/edit, delete/edit, modes, binary and file/directory; median below 100 ms on 100k files |
| [S3](https://github.com/maida-ai/mai/issues/53) Validators | Functional real-repository gates, isolation, invalidation, cache/residue and cancellation correctness remain required; preserve timings, defer the <15% target |
| [S4](https://github.com/maida-ai/mai/issues/54) Queue model | Post-MVP research: labeled real traces, mode/recovery economics and explicitly bounded claims. Synthetic grid retained; not an M0 completion dependency |
| [S5](https://github.com/maida-ai/mai/issues/55) Upstream loop | Three export/squash/sync cycles including concurrent human changes, without spurious conflicts |
| [S6](https://github.com/maida-ai/mai/issues/56) Lockfiles | Concurrent Cargo, npm and uv dependency additions regenerate and pass the gate |
| [S7](https://github.com/maida-ai/mai/issues/57) Crash windows | Incremental tests before each feature is enabled: no lost acknowledgments, duplicate landing/compensation, invented evidence or reset deadlines. Unimplemented mechanisms do not block unrelated work |

Experiment harnesses belong under `experiments/`; short results and selected evidence belong under `docs/spikes/` when performed. Experiment CI runs only on manual dispatch. Historical reports retain their original decisions. ADR 0010 records the accepted staging redesign without changing measurements or calling missing evidence a pass.

## Open maintainer decisions

1. Final tagline; the proposed wording is “The local integration queue for coding agents. Push with Git; mai validates, lands, and keeps every agent in sync.”
2. Default merge mode with a configured gate; `textual` is recommended. Without a gate, use conservative path rejection.
3. Initial implementation uses a simple serial single-class queue respecting dependencies. Final mode/slot tuning and speculative concurrency await measured need; no S4 performance guarantee is required to build the MVP.
4. Automated full-tier recovery remains deferred; priority RFC and feature-specific correctness tests precede enablement. No fix window is selected now.
5. Whether M1b precedes the external-users gate; recommended, not an accepted sequencing constraint yet.
6. Initial merge-rule example ecosystems; Cargo, npm and uv are recommended.
7. Who can grant `sev` and `high`: humans only, or also a designated orchestrator token; both with audit is recommended, not authorized yet.
8. Whether to allow an explicitly configured narrower `check.sev_gate` for severe fixes, pinned to trunk and recorded in evidence with mandatory full confirmation. This exception is not accepted: the normal gate remains required.

Priority questions 7–8 belong to RFC #93 after preliminary observations and do not block scoped MVP acceptance. Nonessential tagline, examples and tuning choices are not implementation gates. These decisions do not authorize weakened validation evidence. Batch tips and intermediate revisions must not be reported as equivalent evidence. M4 raw-context access/handoff and concrete retention limits still require a design before implementation; sharing intent does not authorize sharing raw context.

## Success measurements

Use #11 for opt-in local bounded event recording, opaque identifiers, documented retention and explicit export review; exclude source contents, prompts, secrets and raw agent context. Collect from M1 and expose through the proposed `mai status --metrics`; no rewrite measurements exist yet. Record p50/p95 rejection-to-reland, detected-breakage-to-green, and (from M4) agent-death-to-resumed-landing. Track full-tier failures per landed change and revert rate as trunk guardrails. Split time to land into queue wait and validation; include gate duration, slot utilization, discarded speculation, reason-code mix and flaky rate. Only after the corresponding features are accepted and enabled, report time to land per priority class, human interventions including grants, lease preemptions, aging promotions and fix-forward success within the window versus reverts. Add scope-drift rate from M2, late-conflict waste from M3 and resume success from M4. Compare equivalent repository workloads, hardware, suite versions and warm/cold conditions against a worktree/PR baseline.

Realistic workload experiments follow dogfooding and consented volunteers (#37), then replay through S4’s existing CSV interface. Existing CI histories may supply rough duration/failure distributions, not representative urgency/conflict demand; collection is optional. Record failures and unavailable metrics honestly. Discourage premature optimization: require a measured practical problem before tuning, additional concurrency, batching or materialization complexity.

## Possible futures

Profiled in-process Git reads may justify gitoxide. Overlay/lazy materialization requires evidence that checkout dominates bootstrap and tests. Remote agents can extend Git transport; multi-tenant authorization, cache tiers and distributed storage remain outside the MVP. Affected-test selection stays a user-owned hook rather than an inferred build graph.

## Development policy

There are no backward-compatibility obligations. Do not build legacy readers, schema migrations, converters, deprecated aliases or downgrade paths. A mismatched rewrite state schema refuses startup; disposable evaluation repositories are recreated. This policy does not relax acknowledged-write recovery or authorize destroying user work. The first M1 scaffold retired the old source, tests, benchmarks and examples after scoped M0 acceptance, while preserving licensing, assets, contribution guidance, experiment evidence and the CI skeleton.

## Versioning

The MVP foundation remains 0.0.1. MVP releases use 0.0.x; leaving MVP uses 0.1.0 and demonstrated stability uses 1.0.0. Milestone completion alone does not promote a version. Keep package metadata, CLI tests and implemented documentation consistent.

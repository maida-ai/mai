# Project status and direction

mai is an Apache-2.0 Rust project for 3–30 coding agents working on one repository, independent of core Maida. The proposed rewrite is a Git-native integration queue between agents and their existing forge. It does not run agents, call models in core, replace review or CI, or sandbox tests.

## Working today

The existing 0.0.1 engine uses linked worktrees, path leases, conservative path conflicts, numbered revisions, workspace checks and optional downstream mirroring. It is retained during design qualification and will be replaced entirely when M1 begins. The proposed daemon, clone protocol, integrated queue and inbound forge synchronization are not implemented. [Getting started](getting-started.md) and the current section of [CLI reference](cli.md) document the existing binary.

## Current limits

Use only disposable trusted repositories. Linked worktrees share authority refs; committing on main can bypass publication checks, and local commits break the submission base assumption. Checks run in the agent workspace, not the integrated candidate. Recovery receipts can refer to earlier accepted work while new edits remain pending. There is no daemon, authentication, automatic dirty update or remote multi-tenant support. Preserve drafts before recovery and follow the [current CLI precautions](cli.md#current-workspace-precautions).

## Rewrite sequence

Design choices are proposed for implementation, M0 mechanisms are spike pending, and six defaults/order choices below remain open. Qualification is not established by a prototype or a report from one platform.

| Stage | Exit criteria |
| --- | --- |
| M0 — Design lock | Accept [ADR 0003](https://github.com/maida-ai/mai/issues/58); complete S1–S7 with go evidence or redesign failed assumptions before any product code |
| M1 — Agent integration queue, local | Single-writer engine; durable receive registration; typed outcomes; dependent squash landing; pipeline checks, protected paths and approval; merge rules and limits; clones, drafts, sync, bootstrap, leases and metrics; three harness snippets; five agents on a real repository with zero manual integration and zero broken trunk |
| M1b — Containers and forge | Loopback smart HTTP and per-agent tokens; upstream sync revisions and export branch/PR; full-tier culprit confirmation, validated compensating revert and freeze fallback |
| Gate — External users | 5–10 users on real repositories; compare worktrees plus PRs with mai on human minutes, time to land and broken trunk |
| M2 — Symbol-aware integration | Rebase #38–#41 onto pushed commits and candidate trees |
| M3 — Live awareness | #42–#44: intent, draft-derived activity and deterministic conflict ownership |
| M4 — Resumable, explainable work | #45–#47: opt-in context, recovery from authority drafts and attribution queries |
| M5 — Cross-agent groups | #49: atomic topic landing across agents; fork/fuse uses native Git and documentation |
| M6 — Policy guards | #50: external policy checks using the protected-path approval boundary |

M1b before the users gate is recommended, not settled. Do not begin M2 before the external-users gate without an explicit maintainer override. There are no invented dates or automatic version promotions. Track implementation through [issues and subissues](https://github.com/maida-ai/mai/issues/1), [milestones](https://github.com/maida-ai/mai/milestones) and the [Project](https://github.com/orgs/maida-ai/projects/9).

## M0 qualification

| Spike | Required evidence |
| --- | --- |
| [S1](https://github.com/maida-ai/mai/issues/51) Transport | Host and container receive, options, status, draft policy and fallback on Linux/macOS Git ≥ 2.40; record Windows result |
| [S2](https://github.com/maida-ai/mai/issues/52) Merge | Differential corpus matching cherry-pick for disjoint/same hunks, rename/edit, delete/edit, modes, binary and file/directory; median below 100 ms on 100k files |
| [S3](https://github.com/maida-ai/mai/issues/53) Validators | Rust, Python and TypeScript real repositories; two-slot warm-switch overhead below 15% of gate time; documented resource isolation |
| [S4](https://github.com/maida-ai/mai/issues/54) Queue model | Realistic 30-agent simulation of sequential, pipeline and batch policies; default selection and time-to-land table; pipeline-2 beats sequential at realistic failure rates |
| [S5](https://github.com/maida-ai/mai/issues/55) Upstream loop | Three export/squash/sync cycles including concurrent human changes, without spurious conflicts |
| [S6](https://github.com/maida-ai/mai/issues/56) Lockfiles | Concurrent Cargo, npm and uv dependency additions regenerate and pass the gate |
| [S7](https://github.com/maida-ai/mai/issues/57) Crash windows | Interrupt registration and every landing step; no lost or duplicate landing; terminal outcomes after restart |

Throwaway scripts and short results belong under `docs/spikes/` when performed. No results are asserted here. Failed go criteria require an ADR design correction before M1.

## Open maintainer decisions

1. Final tagline; the proposed wording is “The local integration queue for coding agents. Push with Git; mai validates, lands, and keeps every agent in sync.”
2. Default merge mode with a configured gate; `textual` is recommended. Without a gate, use conservative path rejection.
3. Default validation mode and slot count; pipeline with two slots is recommended, pending S4.
4. Default full-tier failure policy; validated `revert` with freeze fallback is recommended.
5. Whether M1b precedes the external-users gate; recommended, not an accepted sequencing constraint yet.
6. Initial merge-rule example ecosystems; Cargo, npm and uv are recommended.

These decisions do not authorize weakened validation evidence. Batch tips and intermediate revisions must not be reported as equivalent evidence. M4 raw-context access/handoff and concrete retention limits still require a design before implementation; sharing intent does not authorize sharing raw context.

## Success measurements

Collect from M1 and expose through the proposed `mai status --metrics`; no rewrite measurements exist yet. Record p50/p95 rejection-to-reland, detected-breakage-to-green, and (from M4) agent-death-to-resumed-landing. Track full-tier failures per landed change and revert rate as trunk guardrails. Split time to land into queue wait and validation; include gate duration, slot utilization, discarded speculation, reason-code mix, flaky rate and human interventions. Add late-conflict waste from M3 and resume success from M4. Compare equivalent repository workloads, hardware, suite versions and warm/cold conditions against a worktree/PR baseline.

## Possible futures

Profiled in-process Git reads may justify gitoxide. Overlay/lazy materialization requires evidence that checkout dominates bootstrap and tests. Remote agents can extend Git transport; multi-tenant authorization, cache tiers and distributed storage remain outside the MVP. Affected-test selection stays a user-owned hook rather than an inferred build graph.

## Development policy

There are no backward-compatibility obligations. Do not build legacy readers, schema migrations, converters, deprecated aliases or downgrade paths. A mismatched rewrite state schema refuses startup; disposable evaluation repositories are recreated. This policy does not relax acknowledged-write recovery or authorize destroying user work. Delete the old `src/`, `tests/`, `benchmarks/` and `examples/` only when M1 implementation starts after M0; keep licensing, assets, contribution guidance and the CI skeleton.

## Versioning

The prototype is 0.0.1. MVP releases use 0.0.x; leaving MVP uses 0.1.0 and demonstrated stability uses 1.0.0. Milestone completion alone does not promote a version. Keep package metadata, CLI tests and implemented documentation consistent.

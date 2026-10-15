# Agent-native direction

**Status: planning, 2026-10-14.** This direction supersedes the C01–C12 capability contract. Features below are planned, not implemented commands or guarantees. [Current behavior](project.md#working-today) and [architecture](architecture.md) describe the shipped MVP; implementation proceeds through issues with regression evidence.

## Audience and differentiation

Start with one developer or team running 3–30 coding agents on one repository on one machine. Deliver reliable local integration first, then shared units of change, live intent, resumable work bound to exact tree state, and draft composition. Agent time lost to late conflicts matters more than publication throughput. Final positioning wording remains a maintainer decision.

mai remains independent, Apache-2.0, and usable without core Maida or a hosted service. Anthropic's [source-control article](https://www.anthropic.com/engineering/redesigning-source-control) is historical inspiration, not an affiliation or parity claim.

## Design principles

1. Use one unit for leases, conflicts and merging: a symbol where parseable, a structured-config key path, or a path. Always report fallback explicitly.
2. Models may add conflicts or uncertainty; they never accept, merge, establish equivalence or approve landing. `UNCERTAIN` never passes a gate.
3. Lease and intent requests return promptly with holder, intent and expiry rather than waiting for another agent. Enforce ownership at publication.
4. Share structured intent and deltas as untrusted input, not raw agent context. Context access during explicit recovery needs a separate decision below.
5. Assign exactly one conflict resolver by deterministic rule: the later lander by default, with a symbol-lease holder taking precedence over a non-holder.
6. Store, bind, validate and publish; do not run or schedule agents, call models in core, plan tasks, or compress/redact context.
7. Bind successful integrated validation to the exact accepted tree and parent. Workspace-only tests are insufficient.
8. Let local Git commits be private checkpoints; mai owns workspace bases and accepted publication. This is a planned behavior, not safe current usage.

## Delivery and user gate

| Stage | Outcome | Scope |
| --- | --- | --- |
| M1 — Usable local integration | Ordinary Git use and safe integration, including candidate validation | Workspace and publication issues below; existing data safety, object formats, bounded checks, typed outcomes and workspace retirement work |
| Gate — External users | 5–10 external users on real repositories; failures recorded as issues | Release archives, runnable client examples and security reporting |
| M2 — Symbol-aware integration | Shared symbol/key-path units, structural merge and deterministic interaction checks | #38–#41 |
| M3 — Live awareness | Visible intent, activity and conflict ownership | #42–#44 |
| M4 — Resumable, explainable work | Tree-bound context, orphan recovery and provenance queries | #45–#47 |
| M5 — Draft DAG | Anonymous branching and atomic group landing | #48–#49 |
| M6 — Policy guards | Optional external instruction/policy checks | [#50](https://github.com/maida-ai/mai/issues/50) |

**Do not start M2 before the external-users gate without an explicit maintainer override.** Later-stage P1 labels express priority within that stage, not permission to bypass the gate. Release examples demonstrate only shipped commands; add resume examples when M4 exists.

## Implementation map

Track implementation through the linked issues, their subissues and milestones.

| Issue | Planned work and acceptance focus |
| --- | --- |
| [#26](https://github.com/maida-ai/mai/issues/26) | **Workspace base refs (P0):** `refs/mai/workspaces/<agent>/base`; submit/diff/status derive the delta independently of HEAD. Cover multiple local commits, amend, rebase and recovery-base matching; squash the delta into one attributed accepted commit. Define checkpoint reachability before promising retention. |
| [#36](https://github.com/maida-ai/mai/issues/36) | **Authority ref guard (P0):** install and repair a `reference-transaction` guard for `refs/heads/main` and `refs/mai/*`; mai's controlled Git calls bypass it. Test accidental direct commits and Git for Windows. This is accident prevention, not a security boundary. |
| [#27](https://github.com/maida-ai/mai/issues/27) | **Dirty update (P0):** checkpoint work, merge current trunk, and advance the base only after success. Preserve standard conflicts, a pending-trunk ref, typed `update_conflict`, continue and abort. Cover disjoint edits, shared-file hunks, true conflicts and byte-preserving recovery. |
| [#28](https://github.com/maida-ai/mai/issues/28) | **Truthful recovery (P0):** reconcile accepted work without resetting later edits; compute the remaining delta against the accepted base. Return receipt plus `accepted_with_pending_changes` and a distinct exit code when edits remain. Extend kill-after-acceptance tests. |
| [#29](https://github.com/maida-ai/mai/issues/29) | **Scoped capture (P1):** report newly added untracked files separately, support `submit --paths`, and configurable include/list-only/reject policies. The default remains undecided. |
| [#13](https://github.com/maida-ai/mai/issues/13) | **Publication construction and batching (P1):** move candidate assembly into the gate, remove workspace creation from it, then measure bounded queue draining and chained commits with one ref transaction per batch. Retain individual outcomes, metadata versioning and crash reconciliation. Hashing and long checks remain outside the gate; batching cannot reuse validation for a different integrated tree. Reduced retries/fsync cost must be measured, not assumed. |
| [#34](https://github.com/maida-ai/mai/issues/34) | **Revision/history performance (P2):** write revision trailers and batch history reads; define baseline revision handling without introducing historical-format compatibility. Profile status scans separately. |
| [#31](https://github.com/maida-ai/mai/issues/31) | **Environment bootstrap (P1):** versioned `.mai/config.toml`, bounded explicit post-create command outside the gate, optional isolated warmed templates with copy-on-write where supported and a safe fallback. Measure runnable readiness with real dependencies, not checkout alone. |
| [#38](https://github.com/maida-ai/mai/issues/38) | **Extraction (P1, M2):** feature-gated tree-sitter extraction from changed base/new files; qualified symbols and config key paths; explicit unsupported/generated/parse-failure fallback. Initial languages and formats remain undecided. |
| [#39](https://github.com/maida-ai/mai/issues/39) | **Symbol changes and leases (P1, M2):** touched-symbol deltas and symbol/key-path leases; path-level overlap if either side falls back. Editing stays advisory; publication enforces leases. |
| [#40](https://github.com/maida-ai/mai/issues/40) | **Structural merges (P1, M2):** repository/path opt-in; evaluate an existing driver versus an in-house implementation. Keep conservative rejection as default and require configured integrated checks on the result. |
| [#41](https://github.com/maida-ai/mai/issues/41) | **Definition/reference checks (P1, M2):** deterministically flag changed definitions/signatures interacting with live references; explicit warn/reject policy. Parsing is approximate; richer indexes are a later precision upgrade, not a correctness guarantee. |
| [#42](https://github.com/maida-ai/mai/issues/42) | **Intent (P1, M3):** bounded task, summary, rationale, intended units, relied-on invariants and heartbeat; no model interpretation in core. |
| [#43](https://github.com/maida-ai/mai/issues/43) | **Activity (P1, M3):** base, current delta units, intent, leases and heartbeat age, with JSON overlap warnings; no publication gate. |
| [#44](https://github.com/maida-ai/mai/issues/44) | **Conflict records (P1, M3):** both sides' agent/base/intent/delta, conflicting unit, deterministic checks, one resolver and the applied rule. Exclude raw context. |
| [#45](https://github.com/maida-ai/mai/issues/45) | **Context checkpoints (P1, M4):** opt-in opaque harness-supplied envelope tied to commit/tree/base; a notes ref and one reachable checkpoint ref per agent. Harness owns compression/redaction; define size, retention, validation and access before shipping. |
| [#46](https://github.com/maida-ai/mai/issues/46) | **Orphans and resume (P1, M4):** expired heartbeat and leases identify candidates for explicit takeover. Restore a checkpoint and report trunk changes since its base, plus prior intent/leases. Resolve context-transfer authority and lease-transfer races before implementation. |
| [#47](https://github.com/maida-ai/mai/issues/47) | **Why (P2, M4):** read-only line-to-revision attribution with task, intent rationale and permitted context summary; no publication gate. |
| [#48](https://github.com/maida-ai/mai/issues/48) | **Fork/fuse (P2, M5):** checkpoint-based workspaces and three-way composition with parent provenance. This is anonymous branching; document it plainly. |
| [#49](https://github.com/maida-ai/mai/issues/49) | **Atomic group landing (P2, M5):** ordered all-or-none acceptance of dependent drafts, parent-before-child or same-group rules, and validation of the final group tree. This differs from batching independent publication requests in #13. |
| [#50](https://github.com/maida-ai/mai/issues/50) | **Policy guard (P2, M6):** optional external semantic-diff check on instruction/policy paths. Semantic change or uncertainty requires explicit authorization; no model-backed acceptance path in core. |

Integrated-candidate validation ([#20](https://github.com/maida-ai/mai/issues/20)) remains **P0 with no blockers**: materialize and check the candidate outside the gate, then compare its parent at publication. This does not depend on a checkout-free API, extraction layer or custom changeset format. Checks provide evidence for their tested behavior, not proof against all semantic defects.

## Decisions and boundaries

Open maintainer decisions: final tagline; new-untracked-file default; initial languages/config formats; context retention and size caps; any exception to the users gate. Compatibility is governed by [development policy](project.md#development-policy): no backward compatibility. Do not add legacy workspace conversion or old-schema fallback.

Two proposals require reconciliation before implementation. First, `strict` candidate revalidation matches “validate what lands,” while accepting a changed candidate based on a check against an older parent does not. A proposed `parent` mode must remain a separately described weaker policy pending a decision; it cannot be called exact integrated validation. Second, returning a dead agent's context to a new agent conflicts with the prohibition on sharing raw context. An explicit handoff/access rule and permitted summary scope are unresolved; planning does not authorize that transfer.

No agent orchestration, core model calls, line-range locks, or automatic context summarization/redaction. Remote service, authorization, cache tiers and distributed storage remain deferred until after M4 and demonstrated user demand.

## Appendix: fleet-scale futures

C01/C03 checkout-free access, C02 custom changesets, C04 derived indexes, C05 sparse/lazy materialization, C06/C07 daemons and cache tiers, C09 remote authorization, and C12 partitioned storage are exploratory ideas, not delivery commitments. C08 publication queues are narrowed to #13's local workload; C10 validation is immediate #20 work; C11 recovery qualification applies to local workflows now. None of these historical identifiers creates a prerequisite for M1 or overrides the users gate.

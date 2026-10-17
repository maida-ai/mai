# mai architecture

Status: accepted MVP design, implementation in progress. The old worktree engine is retired; the replacement implements lifecycle persistence, authority initialization/reconciliation, a local single-writer engine, manual trusted validation and independent workspaces. See the [current CLI surface](cli.md) for implemented behavior. The diagram and mechanisms below describe the target architecture, not a shipped queue. Scoped M0 and MVP ADR acceptance are recorded in ADRs 0003/0004; [ADR 0010](adr/0010-staged-mvp-qualification.md) defers performance qualification and priority policy while retaining feature-specific correctness gates. [Philosophy](philosophy.md) motivates recovery speed with a strong trunk gate; [ADR 0003](adr/0003-git-native-integration-queue.md) defines the design and open decisions.

```mermaid
flowchart LR
  A[Agent clones] -->|push refs/for/main| R[receive-pack hook]
  R -->|durable registration| E[mai serve: single writer]
  E <--> Q[(state.db: queue and metadata)]
  E --> V[Warm validator clones]
  V -->|candidate evidence| E
  E -->|CAS landing| T[(authority.git: trunk and refs)]
  U[Forge upstream] -->|validated sync| E
  E -->|export branch or PR| U
  T -->|fetch and explicit sync| A
```

## Data and ownership

Agents use ordinary independent Git clones; mai never shares their mutable branch refs. Host transport is file Git; M1b adds loopback smart HTTP with per-agent tokens. A thin proc-receive client registers changes with one long-running engine per authority. The engine owns trunk, immutable submission refs, candidate refs, notes and upstream bookkeeping. Agents may submit virtual refs/for/* or update their own refs/drafts/<agent>. File transport identity is declared, not authenticated; filesystem owners can bypass hooks.

Git's first-parent trunk is the acceptance ledger, with r0 as import baseline. Landed changes are squash commits; validated upstream syncs alone introduce merge commits with trunk as first parent. Mai-Revision trailers assign rN. SQLite WAL stores queue, dependencies, validations, leases and intents; unsupported schema versions refuse startup. Git and SQLite are not one transaction.

## Candidate validation

Start with the serial reference path. Speculative pipelines and batches below describe optional future extensions, not MVP prerequisites.

1. Enforce receive limits, identity/namespace policy and idempotency; retain submitted objects before acknowledging registration. Success means received, not landed. Typed states include queued, blocked, awaiting_approval, validating and exactly one terminal outcome per submission version.
2. Resolve dependencies from pushed ancestry. Build candidates using Git merge-base and merge-tree with an explicit base. Map a pending parent's submitted commit to its landed squash; rejection/supersession invalidates descendant candidates. Merge policy defaults await qualification/maintainer confirmation: textual needs a gate, otherwise use path rejection.
3. Warm validator clones run configured regeneration then the gate on the final immutable tree. Pin policy and executable check inputs to approved trunk; protected changes require approval. Pipeline candidates accumulate preceding changes; failures discard/rebuild descendants. Landing requires matching tree, parent and configuration evidence. Explicit batch mode records weaker batch-tip coverage for intermediate revisions.
4. The single writer rechecks policy/leases and CAS-publishes the validated commit, records notes and reconciles queue state. Sync fetches and safely replays only commits after the submitted commit; it never silently overwrites dirty/untracked work.

## Failure and recovery

An exclusive authority lock prevents duplicate engines. Startup reconciles interrupted landing against accepted history, reconstructs evidence and requeues unlanded work without duplicates. Hooks reject if the engine cannot register durably. Retained submission/draft/candidate refs support reproduction and recovery; unpushed work is not backed up.

Slots have bounded logs, process-group timeout cleanup, private temporary directories and resource-allocation variables; they are not sandboxes. Deferred automatic full-tier recovery, after RFC #93 and its correctness tests, confirms a reproducible culprit, opens a conflict with its manifest and temporary sev fix grant, then allows bounded fix-forward. The fix passes the normal gate and full confirmation; failed/timed-out confirmation cannot claim green or extend the bound. Expiry or exhausted attempts invokes a gated revert; failed/conflicting reverts freeze and notify. Flaky/uncertain attribution cannot authorize revert. Policy/window defaults remain pending.

## Objects and priority

[ADR 0004](adr/0004-objects-manifests-and-priority.md) defines object units, containment and explicit file fallback; module scope is not leasable by default and renames are delete-plus-add. M1 uses file units; M2 adds parsers. Engine manifests are computed at receive and landing, stored in refs/notes/mai-objects, indexed in state.db and summarized in Mai-Objects trailers (full list up to 20 units). Declarations remain intent; scope_drift reports differences.

The initial MVP uses a single-class queue with dependency-aware arrival ordering. Differentiated priorities, grants, inheritance/aging and lease/slot preemption are deferred to RFC #93; ADR 0004 retains the proposal for discussion. No priority or narrower gate is implicitly enabled. Exact validation, ordinary lease enforcement and work preservation remain mandatory.

M1b validates upstream syncs and exports a branch or maintained PR. Sync conflicts get a configured owner; the queue can continue on the old upstream base where safe. Mechanism evidence supports transport, merges, forge cycles and regeneration; S3 corrected functional paths pass fresh native CI on all three platforms, with performance failures retained. S4 workload economics await MVP data, and S7 crash tests accompany each implemented feature before enablement. Git ≥2.40 and SHA-1-only authority support are proposed qualification requirements; submodules/LFS and distributed services are excluded.

Performance claims and optimization follow measured practical need. Opt-in local workload recording (#11), dogfooding and later volunteer data (#37) inform S4 and the priority RFC; synthetic success does not establish realistic throughput or starvation guarantees.

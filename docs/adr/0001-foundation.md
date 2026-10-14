# ADR 0001: Rust agent-native revision authority

Status: accepted for 0.0.1. Service deployment and storage replacements remain proposals. License: Apache-2.0. See [architecture](../../ARCHITECTURE.md) for exact implemented boundaries.

## Context

The Python 0.1.1 prototype established useful behavior with 17 passing real-Git tests, SQLite leases, and detached worktrees. Agent workflows need ordered publication, transparent rejection, cheap independent submissions, and eventual multi-machine authority. Language overhead is only one cost; Git processes, filesystem scans, full checkouts, and serialized work remain architectural constraints.

## Decisions and alternatives

1. **Rust now.** Use a native executable and one implementation language for CLI/core/future services. Python retained a runtime requirement; wrapping it in Rust would not fulfill native deployment. Rust ownership and resource management support the long-term design, but no speed claim follows from language choice alone. Cost: compilation, dependency/toolchain management, and an explicit UTF-8 filename restriction.
2. **SVN-inspired globally ordered revisions.** Keep r0 as import baseline and one single-parent publication chain. Traditional agent-managed Git branches/rebases expose avoidable coordination complexity; SVN storage/wire protocol is not required to obtain clear revision semantics. Cost: authoritative publication remains serialized.
3. **Git-backed storage.** Retain mature objects, packs, worktrees, index operations, fetch/push, and import compatibility. A custom content database would impose unjustified reliability/ecosystem work. Native gitoxide remains a possible measured adapter improvement; full worktree/index/remote parity takes priority over avoiding subprocesses now.
4. **Optional daemon, no placeholder service.** CLI-only operation is useful and deterministic. A mandatory daemon adds lifecycle/IPC failure modes before persistent caching is justified. Future Unix-socket service should reuse the engine and have explicit startup behavior; Tokio/Axum are deferred until a tested vertical slice needs them.
5. **Centralized authoritative publication.** Prepare immutable candidates concurrently, then revalidate under the local SQLite gate and Git CAS. Giant locks around staging and checkout waste concurrency; independent per-path authorities would complicate a globally ordered history. Distributed consensus is unnecessary for the current one-host deployment. Retry amplification is a measured tradeoff requiring future admission/backpressure work.
6. **Conservative conflicts.** Reject equal or ancestor/descendant changed paths, including rename delete/add pairs. Automatic semantic/model merges could silently discard accepted work and make results unpredictable. Same-file nonoverlapping text edits may reject unnecessarily; correctness takes priority over acceptance rate.
7. **GitHub is optional and downstream.** Explicit fast-forward export preserves interoperability without requiring accounts, SaaS, or proprietary authentication. Bidirectional authority risks split brain; remote outage must never invalidate local acceptance. A retryable durable outbox is future work.
8. **Correctness and agent throughput together.** Measure useful accepted revisions, rejection rates, tail latency, workspace cost, retries, resource use, and lock scope. Weakening lease/conflict/durability checks for synthetic throughput is rejected. Git main is the single acceptance ledger; auxiliary refs aid recovery and SQLite retains leases/registry rather than duplicating revision state. Multi-ref filesystem crash atomicity remains a qualification limit.

## Consequences

The implementation is small and transport-independent, but not a stable remote protocol or hardened server. Existing `.mai` and legacy `.agenttrunk` metadata open without conversion. The trusted-local filesystem model remains; Windows CI coverage is experimental; no Maida production assets migrate. Next decisions should follow interruption/backup qualification and profiles, then versioned idempotent protocol design, daemon caching, a secure single-node server, and evidence-based materialization/storage improvements.

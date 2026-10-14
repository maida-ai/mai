# Agent-native capability contract

**Status: accepted product direction; most capabilities below are not implemented.** This document defines the functionality mai intends to deliver as an independently designed, Apache-2.0 answer to agent-scale source control. [Architecture](architecture.md) owns technical decisions; [project status](project.md) owns milestone order. The public CLI remains the 0.0.1 MVP.

The goal is functional coverage with an approachable self-hosted deployment, optional Git interoperability, and no required proprietary infrastructure. Feature completion and measured workload qualification are separate gates. Do not describe this roadmap as implemented parity.

## Public evidence and boundaries

Reference: Anthropic's [Redesigning source control at Anthropic](https://www.anthropic.com/engineering/redesigning-source-control), published October 13, 2026; reviewed October 14, 2026. Its public descriptions establish these reference categories:

- **S1:** Online clients, bounded local caches, server-side source operations.
- **S2:** Checkout-free read/write APIs; sparse and on-demand working copies.
- **S3:** Immutable delta changesets, durable logging, and separate ref CAS.
- **S4:** Rebuildable indexes for ancestry, history, blame, and search.
- **S5:** Distributed source storage, shared node daemons, tiered caches, resumable object requests, isolated failure domains.
- **S6:** Repository/directory permissions, simulations, fuzzing, and rate limiting.
- **S7:** Adjacent review/IAM/search tooling and post-merge/pre-deploy validation.

The article is not a complete protocol specification. Merge policy, approval schema, replication guarantees, and wire compatibility cannot be inferred fully. The requirements below are **mai design decisions**, not claims about undisclosed Scion behavior. Review UI and build/execution systems are integrations around our source authority, not reasons to make the core an agent runner. No Scion implementation, internal artifact, or vendor-specific service is required.

## C01: Work without a checkout

**Current:** `show` and `history` read local authority data without an agent workspace. `diff` requires a worktree/index; `submit` requires a registered workspace. There is no network API or arbitrary revision-to-revision diff endpoint. Evidence: [core operations](../src/core.rs), [CLI actions](../src/main.rs).

**Target:** Agents and jobs can fetch revision/file metadata, compare changes, and create a candidate from an explicit base plus bounded file operations without a Git checkout. Workspace submission becomes an adapter onto this same candidate contract. Every remote operation authenticates and authorizes the relevant repository and paths.

**Completion:** A client with no `.git` directory or working copy can submit puts/deletes/mode changes against a published base; two disjoint candidates integrate; overlapping, invalid, oversized, and unauthorized requests reject without modifying main. File/directory replacements, binary bytes, and retries have deterministic receipts. Relates to S2.

## C02: Share checkpoints without landing them

**Current:** Candidate Git objects are built outside the publication gate, but are temporary implementation details. There is no durable agent-visible draft graph or independent checkpoint operation.

**Target:** An immutable changeset records a base/parent, normalized file delta, content references, declared provenance, and a stable identity. Persisting it shares a checkpoint without allocating an `rN`, updating main, or requiring a Git branch/ref for each draft. Rebase/merge produces another immutable candidate; published history still has a single first-parent acceptance chain. A draft's presence never means acceptance.

**Completion:** Save and inspect concurrent drafts while main stays unchanged; retry with the same scoped request identity returns the same outcome, while a different payload under that identity rejects. Partial upload/record crashes recover or remain explicitly incomplete. Retention/garbage collection cannot delete reachable drafts. Exact encoding and format migration need their own implementation ADR. Relates to S3.

## C03: Authority-side graph and source queries

**Current:** Revision lookup/history and file-at-revision exist. Git ancestry is consulted internally, but there are no public graph, path history, blame, copy search, or revision-diff APIs.

**Target:** Expose ancestry, merge base, ordered revision lookup, file/path history, blame, tree listing, revision diffs, and path-based search as repository services. Clients request bounded/paginated results rather than fetching history to compute them. Rename/copy identity must be specified independently from conservative publication overlap. Full-text indexing is an optional derived integration with an explicit freshness contract.

**Completion:** Queries operate without checkout, match known fixtures including imports/renames/merges where supported, paginate deterministically, and do not expose denied paths through metadata. Queries over published revisions have exact answers; draft graph answers never fabricate accepted revisions. Relates to S1/S4.

## C04: Incremental indexes that can be rebuilt

**Current:** Revision counts and recovery searches use Git traversals. No durable query index or generic derived-data interface exists.

**Target:** Keep source truth immutable; build versioned graph/path/blame/tree summaries from it. Cache keys include repository, source identity, computation version, and relevant options. Expensive derived results can be eager or lazy without exposing internal layouts in client protocols. Publication-critical checks require authoritative or explicitly complete data; an incomplete index cannot silently approve a change.

**Completion:** Drop/rebuild indexes and get equivalent results. Upgrade computation versions without mutating source history. Concurrent computation deduplicates safely; failure exposes unavailable/pending state or an exact bounded fallback. Large queries must not degrade invisibly into full-history scans. Relates to S4.

## C05: Sparse and lazy workspace lifecycle

**Current:** One full detached Git worktree per agent, sharing Git objects. No sparse selection, incremental watcher, lazy provider, removal/expiry command, or explicit task/workspace identity distinct from agent identity.

**Target:** A workspace descriptor separates workspace ID, owner, task context, base, materialized paths, local draft, and lifecycle state. First deliver sparse ordinary files; later qualify optional lazy providers per OS. Client caches are bounded; file watchers accelerate detection with overflow/missed-event recovery. Absence outside the sparse selection means unmaterialized, not deleted.

**Completion:** Modify one selected subtree while hidden paths remain intact after publication. Exercise expansion, hydration errors, watcher overflow, renames crossing selection boundaries, dirty refresh, and safe lifecycle cleanup. Never delete unsubmitted work implicitly. Measure readiness, disk use, and actually transferred/materialized bytes. Relates to S1/S2.

## C06: Shared local daemon, optional operation

**Current:** No daemon/IPC. CLI commands initialize their own storage/coordination calls.

**Target:** An optional daemon shares immutable object caches, workspace tracking, and bounded blocking work across local jobs. Explicit discovery/start/stop and protocol-version checks precede any auto-start behavior. Use restricted Unix sockets and an independently qualified Windows named-pipe alternative. Direct CLI operation remains a supported fallback with the same domain rules.

**Completion:** Restart the daemon without losing published or durable draft state; cache deletion changes performance, not results. Demonstrate repeated independent jobs reusing cached data with enforced quotas/permissions. A daemon failure produces a controlled error or safe direct fallback, not duplicate publication or uncontrolled server traffic. Relates to S5.

## C07: Exact object requests and optional cache tiers

**Current:** Git owns objects, packing, and maintenance. There is no object-fetch protocol, remote cache topology, or resumable content transfer.

**Target:** Fetch explicitly identified immutable blobs and manifests, with integrity checks, bounded batch sizes, and resumable transfer. Object IDs carry algorithm/type information and do not bind the public protocol to SHA-1. Node and optional regional caches implement the same read subset as the authority; the deployment can collapse tiers onto one host. Clients need only objects in their selected/read paths.

**Completion:** Fetch only requested content, resume interrupted transfer, reject corrupted/wrong-object responses, and honor revocation/authorization before returning cache hits. Enforce cache quotas, request cancellation, admission limits, and upstream budgets. Loss of a cache tier cannot create an unlimited fallback storm. Relates to S5.

## C08: Fair, durable authoritative landing

**Current:** Preparation is parallel; SQLite serializes head/lease revalidation and Git CAS. There is no persistent admission queue or request-key idempotency. Recovery receipts are per-agent refs and are verified against main. Recorded large bursts can exhaust the 64-attempt budget.

**Target:** A bounded authority owns acceptance into main. Immutable preparation and queries proceed independently of landing. Persist accepted/rejected/queued outcomes and request identity; revalidate against the exact current head and policy. Policies may request validation against a candidate; a receipt identifies the validated object and policy version. No stale green result implicitly approves a different integrated tree.

**Completion:** Independent clients make progress under sustained bursts; overload gives a defined backoff response and fairness metrics. Acknowledged acceptance survives qualified failures; retries cannot double-land. Lease fencing, policy changes during preparation, cancellation at the publication boundary, and crash recovery have dedicated tests. Main is the only accepted writer; r0 never moves. Relates to S3/S7.

## C09: Authenticated repository and path access

**Current:** Agent identity is declared text; leases enforce cooperative publication only. Direct filesystem writers can bypass policy. There is no remote authentication or authorization.

**Target:** Self-hosted identity adapters provide a verified principal independent of claimed agent/task IDs. Authorize reads, object access, queries, draft creation, landing, and reserved-path ownership. Repository and directory policies apply to derived results, inherited content, sparse views, and caches. Authorization is not a worktree sandbox.

**Completion:** Denied paths stay inaccessible through blobs, tree listings, diffs, blame, imports, and error detail. Permission changes take effect on cached requests; audit trails record actor, operation, policy, and outcome without prompts/secrets. Authenticated publication cannot impersonate another owner or reuse stale lease authority. Relates to S6.

## C10: Review, validation, revert, and bisect interfaces

**Current:** Local `--check` is explicit shell execution before preparation; history/show expose accepted snapshots. There are no immutable approval attestations, server validation policies, revert/bisect commands, or review API.

**Target:** External tools attach review/test evidence to exact changeset/tree IDs, scopes, policy versions, and dependency inputs. Authority-side rebase/merge is opt-in and deterministic, with new candidate IDs and explicit conflict receipts. Revert creates a compensating candidate/revision without rewriting history. Bisect consumers ask bounded graph queries and return results through an external test runner.

**Completion:** Changed candidate content invalidates incompatible approval; binary/path conflicts preserve both sides; revert can itself conflict safely. Required pre-land checks cannot be bypassed by calling downstream testing. Post-land results and pre-deploy evidence remain separate states. No server executes client-provided shell code by default, and no LLM is required to resolve conflicts. Relates to S7.

## C11: Recovery, observability, and workload qualification

**Current:** Process-kill/intermediate-state tests and a native fanout harness exist. Timing receipts omit rejected-request metrics; power loss, distributed service failure, and lifecycle recovery are unqualified.

**Target:** Add deterministic fault simulations and fuzzing around request parsing, object integrity, crash windows, retries, leases, index rebuilds, and cache/server failures. Observe per-operation latency, waiting/admission, rejection reasons, retries, cache hits, transfer bytes, index lag, auth failures, and durable outcomes. Monitoring tools consume open telemetry interfaces; agents are optional consumers.

**Completion:** Restore source truth, drafts, receipts, and audit state from documented backups; replay crash cases with no lost acknowledged outcome. Publish reproducible hardware/workload results across fanout, repository/history size, and network conditions, including safe overload behavior. Treat byte/memory limits and tail latency as part of correctness. Relates to S6.

## C12: Incremental adoption and portable storage

**Current:** Git import and explicit fast-forward mirroring work. Basic operation is self-hosted and Apache-2.0; the implementation still uses Git-specific storage commands and local paths.

**Target:** Start advanced capabilities over Git objects rather than replacing storage immediately. Fleet-scale storage must eventually accommodate object/history volumes beyond one authority host's capacity without requiring complete copies at clients or caches; this is an unqualified target, not a property of today's bare Git repository. Separate candidate/query/materialization contracts from Git refs/pack formats. Add stable source/receipt export, restore, and migration tooling before any backend replacement. Native binaries and straightforward onboarding should be available to small teams, not only operators of large fleets.

**Completion:** Conformance fixtures pass against the deployed adapter; partition/replica failure and restore tests qualify any distributed storage mode, and clients/caches do not need repository GC or packing; import/export/restore retains baseline, accepted revisions, contents, modes, and attribution. Upgrades require explicit, reviewable conversion when formats change. No required Spanner, proprietary object store, GitHub account, Maida service, or telemetry endpoint. Another backend requires profiles and a concrete recoverability/conformance case, not architectural purity.

## What functional coverage means

The goal is to satisfy C01–C12 through published contracts, running code, end-to-end acceptance tests, and measured operational limits. Architecture alone does not complete a capability. Current support remains the specific partial behavior listed above. Use these IDs in implementation issues and ADRs; do not equate the number of features with throughput, isolation, or production readiness.

[Delivery sequence →](project.md#roadmap) · [Technical design →](architecture.md#target-architecture-accepted-direction-not-implemented) · [All docs →](README.md)

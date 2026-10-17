# Current delivery and next decisions

The product goal is parallel agent contribution with less branch and conflict coordination. Recovery supports that goal by preserving work and helping agents resume after failures. mai 0.0.1 is an evaluable trusted-local foundation, not a completed automatic integration queue. It now covers explicit creation, submission, inspection, gated landing, covered validator recovery and conservative continuation of later committed work. Use disposable repositories while the remaining MVP acceptance work is open.

| User action | Delivered boundary |
| --- | --- |
| Create an agent workspace | Independent clone with configured trusted bootstrap; no shared writable environment |
| Submit and land work | Explicit Git push and named-generation integration; exact dependent squash mapping, file manifests, ordinary leases, protected approval and normal gate |
| Diagnose rejection | Durable typed outcome and bounded diagnostics; true conflicts require an explicit correction |
| Recover an interrupted validator | Opt-in Linux same-boot supervision with exact cleanup proof; missing proof and worker death refuse reuse |
| Continue later local commits | Explicit landed identity and clean unregistered linear suffix replayed into a new workspace; original clone and failed destinations preserved |

Start with [getting started](getting-started.md), then the [two-agent continuation/conflict/correction walkthrough](two-agent-workflow.md). The [recovery guide](validator-recovery.md) covers a separate interrupted-engine path. These commands do not automatically schedule integration, choose conflict content or accept work without its gate.

## Evidence and limits

[PR #111](https://github.com/maida-ai/mai/pull/111) delivered explicit receive/integration and its supporting invariants. [PR #112](https://github.com/maida-ai/mai/pull/112) delivered proof-backed validator recovery. [PR #113](https://github.com/maida-ai/mai/pull/113) adds copy-based continuation with an independently executed [two-agent evidence bundle](spikes/continuation/2026-10-17/README.md). Each delivered correctness path has its own failure/interruption tests and native CI evidence; the [benchmark index](benchmarking.md) distinguishes functional fixtures from performance qualification.

The published recovery fixture observed 0.160 seconds to reclaim and 0.615 seconds to a fresh validated landing. The earlier tiny Git/mai smoke comparison had unequal operational scope and establishes no advantage. Neither experiment supplies production MTTR, representative throughput, a warm-overhead guarantee or evidence for hundreds/thousands of agents. Process-kill tests do not establish power-loss durability. The intended local audience remains 3–30 agents; even that workload has not been performance-qualified.

Current manifests and leases operate on file units. The intended finer coordination unit is a semantic object—function, method, variable or configuration key. Semantic-object locking and stable object-key identity are future work and are not delivered or qualified by these increments.

## Still open

Automatic queue scheduling, draft backup, in-place sync/autostash, pending-submission replay, regeneration and forge/container workflows remain unfinished. Copy-based continuation is partial [#27](https://github.com/maida-ai/mai/issues/27); it does not close the full synchronization scope. Priority policy and automated fix-forward/revert recovery remain deferred, and no pending mode, slot, failure-window or priority default is selected by this delivery.

The external-user gate remains unfulfilled: 5–10 users on real repositories and equivalent baseline comparisons are still required. The documented maintainer exception allows technically independent development before that feedback; it does not waive exit criteria or qualify M2–M6. No later milestone is declared complete by these increments.

The next decisions are to choose a bounded dogfood/volunteer evaluation, learn whether copy-based continuation is sufficient for the agent workflow before investing in in-place synchronization, and prioritize the remaining M1 capabilities from observed failures and recovery effort. The ordering of M1b versus external-user qualification remains a maintainer decision. See the [roadmap](project.md) for stage boundaries and unresolved policy decisions.

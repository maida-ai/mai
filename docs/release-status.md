# Current delivery and limits

<<<<<<< HEAD
mai is an evaluable trusted-local foundation for coordinating changes from independent Git workspaces. An operator explicitly enables receive, submits commits, inspects durable records and selects generations for integration through configured validation.
=======
The product goal is parallel agent contribution with less branch and conflict coordination. MTTR is an underlying design principle: preserve work and help agents resume after failures so parallel contribution remains practical. mai 0.0.1 is an evaluable trusted-local foundation, not a completed automatic integration queue. It now covers explicit creation, submission, inspection, gated landing with opt-in integration on receipt, covered validator recovery and conservative continuation of later committed work. Use disposable repositories while the remaining MVP acceptance work is open.
>>>>>>> bb20058 (Document opt-in integration on receipt)

| Area | Current behavior |
| --- | --- |
<<<<<<< HEAD
| Workspaces | Create independent Git clones with configured bootstrap. |
| Receive | Explicitly enable bounded admission and register a pushed commit as a generation. Receipt does not mean acceptance. |
| Integration | Explicitly select a generation; dependency, policy, lease and gate checks precede landing. |
| Inspection | Read bounded queue summaries and exact-generation records and wait for terminal outcomes. |
| Recovery | Opt-in Linux validator recovery requires exact cleanup proof. |
| Continuation | Replay a clean, unregistered linear suffix into a new workspace after identifying the landed generation. |
=======
| Create an agent workspace | Independent clone with configured trusted bootstrap; no shared writable environment |
| Submit and land work | Explicit Git push; named-generation integration by default or opt-in textual integration on receipt; exact dependent squash mapping, file manifests, ordinary leases, protected approval and normal gate |
| Inspect pending work | Bounded read-only queue summaries and exact-generation wait, including persisted state while a gate runs or the engine is stopped; inspection does not initiate integration |
| Diagnose rejection | Durable typed outcome and bounded diagnostics; true conflicts require an explicit correction |
| Recover an interrupted validator | Opt-in Linux same-boot supervision with exact cleanup proof; missing proof and worker death refuse reuse |
| Continue later local commits | Explicit landed identity and clean unregistered linear suffix replayed into a new workspace; original clone and failed destinations preserved |
>>>>>>> bb20058 (Document opt-in integration on receipt)

## Limits

<<<<<<< HEAD
Automatic integration scheduling, draft backup, in-place sync/autostash, pending-submission replay, forge/container workflows, semantic-object locking, differentiated priority and automated fix-forward are not available. Current manifests and leases use file units. mai runs trusted repository commands without sandboxing and is intended for trusted local use, not hostile tenants.
=======
Start with [getting started](getting-started.md), [observe a failed gate and correction](check-failure.md), then the [two-agent continuation/conflict/correction walkthrough](two-agent-workflow.md). The [recovery guide](validator-recovery.md) covers a separate interrupted-engine path. These walkthroughs use explicit integration by default. Opt-in automatic integration does not choose conflict content or accept work without its gate.
>>>>>>> bb20058 (Document opt-in integration on receipt)

The documented recovery and workflow measurements are functional evidence; they do not establish production recovery distributions, representative throughput, a performance advantage or support for hundreds of agents. External-user evaluation and equivalent baseline comparisons remain open. Process interruption tests do not establish power-loss durability.

<<<<<<< HEAD
Start with [getting started](getting-started.md), then review the [CLI reference](cli.md), [operations](operations.md) and [architecture](architecture.md).
=======
`mai receive enable PROJECT --max-input-bytes 10485760 --max-file-bytes 1048576 --max-changed-files 100 --auto-integrate` opts into a textual integration attempt during each receipt. Explicit integration remains the default. This is not a background queue scheduler: it does not revisit blocked work automatically. Automatic attempts supply no lease claims, so active leases can leave work pending; protected changes still need trusted approval and a later explicit integration. Inspect the persisted outcome after a push, especially after an uncertain response. The normal gate and dependency checks still apply.

[PR #111](https://github.com/maida-ai/mai/pull/111) delivered explicit receive/integration and its supporting invariants. [PR #112](https://github.com/maida-ai/mai/pull/112) delivered proof-backed validator recovery. [PR #113](https://github.com/maida-ai/mai/pull/113) adds copy-based continuation with an independently executed [two-agent evidence bundle](spikes/continuation/2026-10-17/README.md). Each delivered correctness path has its own failure/interruption tests and native CI evidence; the [benchmark index](benchmarking.md) distinguishes functional fixtures from performance qualification.

[PR #114](https://github.com/maida-ai/mai/pull/114) adds the step-by-step newcomer workflow and a failed-gate correction exercise. [PR #115](https://github.com/maida-ai/mai/pull/115) releases abandoned native slot locks while preserving occupied claims and recovery proof requirements. [PR #116](https://github.com/maida-ai/mai/pull/116) adds bounded queue inspection and exact-generation wait; these observers do not select work or establish engine liveness. Wait bounds polling and SQLite lock waits, while snapshot processing can extend elapsed wall time. Representative workload performance remains unqualified. A [scripted Linux operator replay](spikes/product-validation/2026-10-17-operator-observation-1/README.md) received three real patches concurrently, integrated them explicitly through the gate and checked offline observation under an externally held engine lock. Its frozen runtime predates the final lease correction; it is functional evidence, not independent-agent or performance qualification.

The published recovery fixture observed 0.160 seconds to reclaim and 0.615 seconds to a fresh validated landing. The earlier tiny Git/mai smoke comparison had unequal operational scope and establishes no advantage. Neither experiment supplies production MTTR, representative throughput, a warm-overhead guarantee or evidence for hundreds/thousands of agents. Process-kill tests do not establish power-loss durability. The intended local audience remains 3–30 agents; even that workload has not been performance-qualified.

Current manifests and leases operate on file units. The intended finer coordination unit is a semantic object—function, method, variable or configuration key. Semantic-object locking and stable object-key identity are future work and are not delivered or qualified by these increments.

## Still open

Background dependency-aware queue scheduling, draft backup, in-place sync/autostash, pending-submission replay, regeneration and forge/container workflows remain unfinished. Copy-based continuation is partial [#27](https://github.com/maida-ai/mai/issues/27); it does not close the full synchronization scope. Priority policy and automated fix-forward/revert recovery remain deferred, and no pending mode, slot, failure-window or priority default is selected by this delivery.

The external-user gate remains unfulfilled: 5–10 users on real repositories and equivalent baseline comparisons are still required. The documented maintainer exception allows technically independent development before that feedback; it does not waive exit criteria or qualify M2–M6. No later milestone is declared complete by these increments.

The next decisions are to choose a bounded dogfood/volunteer evaluation, learn whether copy-based continuation is sufficient for the agent workflow before investing in in-place synchronization, and prioritize the remaining M1 capabilities from observed failures and recovery effort. The ordering of M1b versus external-user qualification remains a maintainer decision. See the [roadmap](project.md) for stage boundaries and unresolved policy decisions.
>>>>>>> bb20058 (Document opt-in integration on receipt)

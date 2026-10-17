# Philosophy

Status: rewrite direction; not implemented in 0.0.1. The mechanisms and cost model below are design goals, not measured guarantees. [ADR 0003](adr/0003-git-native-integration-queue.md) records qualification gates and unresolved defaults.

### Parallel contribution, with recovery supporting the workflow

mai's main goal is to let multiple coding agents contribute in parallel without worrying about shared branches or routine conflict coordination. The engine should handle integration bookkeeping so an agent acts when intent genuinely conflicts, validation fails or policy requires a decision. It cannot choose conflicting intent automatically.

Cheap generated code also makes fixes cheaper, but more generated code and bugs can mean more fixes to coordinate. Fast detection, rejection, restoration and resumption support the parallel workflow; mean time to recovery (MTTR) is an important supporting measure, not the product's primary feature. Integration bandwidth, validation compute and human attention still constrain useful throughput.

Trunk is shared by all agents, so a broken accepted base can multiply disruption. The blocking gate remains mandatory. Bounded automatic fix-forward and revert are proposed mechanisms below, not shipped behavior or permission to weaken that gate.

| Failure | Recovery mechanism | MTTR measured as |
| --- | --- | --- |
| Rejected change (conflict, gate, policy) | Typed outcome plus a fetchable candidate ref; the agent regenerates or resolves | Rejection → re-landed |
| Bad landing that passed the gate | `full` tier confirms the culprit by bisection; its agent gets a short `sev`-priority fix-forward window; a validated revert follows if recovery misses its bound | Detection → trunk green |
| Dead agent | Drafts already on the authority; `mai resume` | Death → resumed work landed |
| Stale work | The engine merges from any base; no agent action unless there is a true conflict | — |
| Upstream sync conflict | Conflict record with a configured owner; the queue continues on the old upstream base | Upstream advance → synced |

### Design consequences

1. **No long-lived branches.**
   - Long isolation is an MTBF tactic. It defers integration risk into large merges that are slow to recover from.
   - Work lives in small changes that land within minutes to hours. Agents never maintain named shared branches. Drafts are per-agent, disposable, and exist for recovery, not isolation.
   - `mai status` flags drafts whose base is far behind trunk (configurable threshold). It warns and never blocks.
2. **No rebase chores.**
   - Landing never requires the agent to rebase or update first. The engine merges from whatever base the agent pushed.
   - An agent acts only on a true conflict or a failed gate.
   - The one remaining chore is the post-landing `mai sync` caused by squash landing. It is automated through the harness snippets, and agents should never need to run it by hand.
3. **Materialize only where code executes.**
   - Integration work never checks out files: merging, conflict detection and candidate building all operate on Git trees.
   - Working copies exist only where code runs (agent workspaces and validator slots). Both are long-lived and warm, and they switch state incrementally so build caches survive.
   - Cost model:
     - Checkout costs O(repository files), once per workspace or slot.
     - Environment bootstrap dominates and is paid once per slot, or again only when its inputs change.
     - Per-change cost is O(changed files).
   - Overlay or lazy materialization stays deferred until S3 shows that workspace creation, not bootstrap or tests, dominates.
4. **Small changes beat big ones.** One change maps to one `rN` and one validation unit. Continued work chains as dependent changes instead of growing one change.
5. **Fix forward by default; revert as the backstop.** The proposed recovery policy gives a normal change `sev` priority and a bounded window. A revert is a compensating change through the same gate when fix-forward fails or misses its window. History is never rewritten. The shipping default and window remain pending maintainer approval and S4.
6. **The object is the unit.**
   - The intended locking and coordination units are semantic objects: functions, methods, variables, classes and configuration keys, never line ranges.
   - Lines shift with every edit, and files are too coarse for N agents.
   - Current engine-computed manifests and ordinary leases use file units. M2 proposes parsers and object precision; semantic-object locking and stable object-key identity are not shipped. Robustly identifying the same object across changes is future technical work, not an established capability.


Automatic recovery requires a reproducibly confirmed culprit; flaky or uncertain attribution never authorizes an automatic revert. A fix passes the normal gate and then full-tier confirmation before trunk is called green. Failure or timeout of confirmation cannot extend the recovery window indefinitely; it invokes the revert backstop. Draft recovery covers commits successfully pushed to retained refs, not unsaved or unpushed edits. Checkout and per-change complexity are hypotheses to measure in S3, not filesystem or build-tool guarantees.

Applying these principles requires the contracts in [ADR 0003](adr/0003-git-native-integration-queue.md): the full-failure default remains open, and uncertain recovery or a failed/conflicting revert requires an owner-visible freeze. Agents can also receive typed policy, lease and invalid-input outcomes. Git-tree integration is checkout-free, while configured regeneration runs in a validator and creates the final tree that must pass the gate.

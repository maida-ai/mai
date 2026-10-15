# Philosophy

Status: rewrite direction; not implemented in 0.0.1. The mechanisms and cost model below are design goals, not measured guarantees. [ADR 0003](adr/0003-git-native-integration-queue.md) records qualification gates and unresolved defaults.

### Philosophy: gate the trunk, optimize everything else for recovery

Classic version control, and the process built around it, maximizes **mean time between failures (MTBF)**. That process includes long-lived branches, careful manual merges, and review before integration. Code was expensive to write and human attention was scarce, so preventing a bad change was worth delaying every change.

Agents invert those economics. Code is cheap, and a rejected change can be regenerated in minutes. The scarce resources become integration bandwidth, validation compute and human attention. mai therefore optimizes for **mean time to recovery (MTTR)**: detect fast, reject fast, restore fast, resume fast.

One refinement keeps this from becoming "move fast and break trunk". In a swarm, trunk is shared by N agents. A broken trunk costs N times over, because every agent builds and validates on the broken base. So the rule is MTTR everywhere except the trunk boundary. A fast gate keeps trunk MTBF high, and when a failure slips past the gate, trunk is restored automatically rather than frozen.

| Failure | Recovery mechanism | MTTR measured as |
| --- | --- | --- |
| Rejected change (conflict, gate, policy) | Typed outcome plus a fetchable candidate ref; the agent regenerates or resolves | Rejection → re-landed |
| Bad landing that passed the gate | `full` tier finds the culprit by bisection, then a validated automatic revert lands | Detection → trunk green |
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
5. **Revert is a first-class, cheap operation.** A revert is a compensating change that passes the same gate. History is never rewritten.


Automatic recovery requires a reproducibly confirmed culprit and a validated compensating change; flaky or uncertain attribution never authorizes an automatic revert. Draft recovery covers commits successfully pushed to retained refs, not unsaved or unpushed edits. Checkout and per-change complexity are hypotheses to measure in S3, not filesystem or build-tool guarantees.

Applying these principles requires the contracts in [ADR 0003](adr/0003-git-native-integration-queue.md): the full-failure default remains open, and uncertain recovery or a failed/conflicting revert requires an owner-visible freeze. Agents can also receive typed policy, lease and invalid-input outcomes. Git-tree integration is checkout-free, while configured regeneration runs in a validator and creates the final tree that must pass the gate.

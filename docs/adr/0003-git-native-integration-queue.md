# ADR 0003: Git-native agents, single-writer queue and forge topology

Status: rewrite direction decided; design acceptance pending M0 S1–S7 and maintainer defaults. Nothing here is implemented in 0.0.1. This replaces the former workspace-base/ref-guard proposal at this path and supersedes conflicting ADR 0001 decisions 2–6 and ADR 0002. [Philosophy](../philosophy.md) supplies the motivation; [architecture](../architecture.md) is the compact overview.

## Decisions

Agents are ordinary Git clients with independent clones. They commit locally and push `HEAD:refs/for/main`; an asynchronous receive hook registers a change and reports its ID, not acceptance. Only one long-running `mai serve` engine writes trunk, via Git CLI compare-and-swap. Git CLI performs merges; gitoxide reads require profiling evidence. There is no linked-worktree base-ref repair, optimistic SQLite publication gate, custom changeset format or publication group-commit subsystem in the rewrite.

The product core is an integrated validation queue. A pending push may depend on another pending push; dependent changes are M1, not future draft-DAG work. The engine uses Git merge-base and `merge-tree --write-tree --merge-base=<base>` to build candidates without checkout. A dependent delta uses its parent's submitted commit as base and the mapped integrated parent as destination. Never recompute only a naive merge-base after squash and replay the parent's entire delta. Each ordinary landing squashes one change into an attributed revision with `Mai-Revision`, `Change-Id`, `Agent-ID` and validation metadata.

Warm validator clones retain build environments. Regeneration is an explicit exception to checkout-free tree integration: it executes in a slot, captures the final tree, then validates that exact tree. Local workspace creation uses clones with reference alternates, bootstrap and optional qualified copy-on-write templates. No overlay/lazy materializer is planned for MVP. Retention must keep objects required by alternates alive; deleting an authority must not silently destroy client recovery.

mai sits between agents and their forge. Fetch upstream and land gated sync merges with trunk as first parent and upstream as second; ordinary changes remain single-parent squash commits. Export a branch or one maintained PR. S5 must prove squash/rebase export cycles and concurrent upstream edits rather than assume identical content guarantees a clean merge. Conflicts have a configured owner; safe queued work may continue on the last synced base.

## Registration, refs and lifecycle

The engine owns `refs/heads/main`, `refs/mai/changes/<n>`, `refs/mai/candidates/<id>`, `refs/notes/mai`, `refs/notes/mai-objects` and `refs/mai/upstream`. Submission refs `refs/for/*` are virtual. An agent may update only its own `refs/drafts/<agent>`; drafts can move but accepted history cannot. File transport has declared identity and trusted-local filesystem access; token-authenticated loopback HTTP belongs to M1b.

Reference submitted objects before reporting receive success, including quarantine/ref-visibility crash windows qualified by S1/S7. A repeated identical commit returns its existing change. A newer version with the same Change-Id may supersede a queued version, but a validating version finishes first. Each version has one terminal result: landed, rejected, superseded or cancelled. Blocked and awaiting_approval are nonterminal. Define post-validation replacement and dependency retargeting explicitly; never silently substitute unvalidated content under an existing outcome.

SQLite WAL is the queue and metadata store, not the acceptance ledger. Refuse schema-version mismatch; no migrations. An exclusive lock enforces one engine, with local IPC, CLI auto-start and foreground supervision. If registration is unavailable, the hook rejects. Startup uses accepted-history evidence to reconcile landing, rebuild candidates and restore notes without duplicate publication. A Change-Id alone must not confuse superseded submission versions: match their accepted identity as well. Git ref transactions do not imply cross-store or power-loss atomicity.

## Validation and trust

Pipeline candidates accumulate queued changes and validate in parallel. Publish only with exact final tree, parent and approved configuration evidence. Failure/rejection/supersession invalidates stacked candidates; rebuild independent descendants, and block/reject changes whose prerequisite no longer exists. A later dependency on a reverted change needs explicit re-evaluation, not automatic replay onto incompatible trunk.

Explicit batch mode validates only the batch tip and bisects failure. Intermediate revisions say “validated as batch tip”; they do not satisfy pipeline's per-revision guarantee. This distinction must remain visible in notes, outcomes and metrics. Flake retries stay on the same candidate; passing after failure is recorded, never fabricated. Slots expose resource variables, private temp directories, bounded output and process-group timeout cleanup without claiming test sandboxing.

Gate configuration comes from trusted trunk. Protected paths require explicit approval. A trunk-owned command string invoking a candidate-modified script is not sufficient protection: pin executable policy inputs and check/affected-test/driver/regeneration logic to the approved version, and define the approval transition before shipping. Repository tests still execute candidate code; describe that trust boundary honestly. Git merge-driver behavior and generated-file handling must be qualified, not inferred from using Git CLI.

## D9 amendment: bounded fix-forward

Cheap code favors fix-forward, but only within a bound: new changes on red trunk compound failure and complicate later reversion while every agent tests a broken base. Give the confirmed culprit's agent a short sev lane, then use a compensating revert; the agent can cheaply re-land a corrected change. This replaces immediate automatic revert as the recommended policy, without selecting the unresolved shipping default.

The full tier bisects and confirms a reproducible culprit with before/after runs. Flaky or uncertain attribution never authorizes automatic revert. Open a conflict record containing failing logs, culprit manifest and an audited temporary sev grant. A push with `mai.fixes=<change>` uses that grant, passes the normal gate and then requires full-tier confirmation. Gate success alone does not restore green status.

The fix window bounds the entire incident, including gate and full confirmation, and persists across pushes and engine restart without resetting. Bind mai.fixes and its temporary grant to that confirmed incident; neither authorizes unrelated sev work. Expiry, exhausted gate attempts, failed full confirmation or confirmation timeout invokes the gated compensating-revert backstop; do not silently restart the clock or allow endless retries. Re-evaluate dependent queued changes after reversion, rebuilding or blocking/rejecting them as necessary. If the revert conflicts or fails its gate, freeze and notify the owner. Preserve explicit fix-forward, revert, freeze and continue policies; owner-visible degraded state is required when safe recovery cannot be established.

Priority never weakens exact-tree/parent/configuration evidence. A narrower `check.sev_gate` is an open maintainer decision, not an available bypass. [ADR 0004](0004-objects-manifests-and-priority.md) defines D10 objects, D11 engine manifests and D12 priority. The former ADR 0004's exact integrated-validation rules remain in this ADR: candidate replacement, regeneration, retries and reordering require evidence for the actual landed tree; no stale-parent or workspace check substitutes.

| Risk | Likelihood | Impact | Mitigation | Spike |
| --- | --- | --- | --- | --- |
| Fix-forward stalls as changes stack on red trunk | Medium | High | Bounded window including full confirmation; sev fix lane; revert backstop; compare 5/15/30-minute windows with immediate revert | S4 |
| Automatic revert targets an innocent change | Medium | Medium | Reproduce before/after culprit evidence; never auto-revert flaky attribution; return evidence with outcome | S4, S7 |

## Qualification and open decisions

M0 requires S1 transport (Linux/macOS and recorded Windows/fallback results), S2 differential merges, S3 warm validators, S4 queue simulation including priority mix, starvation and fix-window comparisons, S5 forge cycles, S6 lockfile regeneration and S7 crash windows. Throwaway harnesses belong under `experiments/`; reports and selected evidence belong under `docs/spikes/`. Failure requires an ADR redesign before product code. Git ≥2.40, SHA-1-only authority, unsupported submodules/LFS and best-effort Windows are qualification targets, not tested support claims.

The maintainer must settle tagline, merge default with a gate, validation mode/slot count after S4, full-failure default, M1b position relative to the users gate, initial merge-rule ecosystems, priority grant roles and whether any explicitly narrower sev gate is permitted. S4 must qualify fix_window and publish per-class time-to-land with no starvation. Direction favors textual merging with a gate, pipeline validation and bounded fix-forward with a revert backstop, but these recommendations are not selected defaults. No check means path mode and no claim of a validated trunk. Context access/retention and later parser choices remain open in their later-stage ADRs.

No compatibility shims, model acceptance, synchronous push-until-tests-complete, custom storage, line-range locks, per-draft mutable namespaces beyond one draft ref per agent, or fleet features without evidence. Product replacement waits for M0; no source deletion is authorized by merely reading this ADR.

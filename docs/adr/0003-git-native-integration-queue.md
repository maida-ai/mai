# ADR 0003: Git-native agents, single-writer queue and forge topology

Status: rewrite direction decided; design acceptance pending M0 S1–S7 and maintainer defaults. Nothing here is implemented in 0.0.1. This replaces the former workspace-base/ref-guard proposal at this path and supersedes conflicting ADR 0001 decisions 2–6 and ADR 0002. [Philosophy](../philosophy.md) supplies the motivation; [architecture](../architecture.md) is the compact overview.

## Decisions

Agents are ordinary Git clients with independent clones. They commit locally and push `HEAD:refs/for/main`; an asynchronous receive hook registers a change and reports its ID, not acceptance. Only one long-running `mai serve` engine writes trunk, via Git CLI compare-and-swap. Git CLI performs merges; gitoxide reads require profiling evidence. There is no linked-worktree base-ref repair, optimistic SQLite publication gate, custom changeset format or publication group-commit subsystem in the rewrite.

The product core is an integrated validation queue. A pending push may depend on another pending push; dependent changes are M1, not future draft-DAG work. The engine uses Git merge-base and `merge-tree --write-tree --merge-base=<base>` to build candidates without checkout. A dependent delta uses its parent's submitted commit as base and the mapped integrated parent as destination. Never recompute only a naive merge-base after squash and replay the parent's entire delta. Each ordinary landing squashes one change into an attributed revision with `Mai-Revision`, `Change-Id`, `Agent-ID` and validation metadata.

Warm validator clones retain build environments. Regeneration is an explicit exception to checkout-free tree integration: it executes in a slot, captures the final tree, then validates that exact tree. Local workspace creation uses clones with reference alternates, bootstrap and optional qualified copy-on-write templates. No overlay/lazy materializer is planned for MVP. Retention must keep objects required by alternates alive; deleting an authority must not silently destroy client recovery.

mai sits between agents and their forge. Fetch upstream and land gated sync merges with trunk as first parent and upstream as second; ordinary changes remain single-parent squash commits. Export a branch or one maintained PR. S5 must prove squash/rebase export cycles and concurrent upstream edits rather than assume identical content guarantees a clean merge. Conflicts have a configured owner; safe queued work may continue on the last synced base.

## Registration, refs and lifecycle

The engine owns `refs/heads/main`, `refs/mai/changes/<n>`, `refs/mai/candidates/<id>`, `refs/notes/mai` and `refs/mai/upstream`. Submission refs `refs/for/*` are virtual. An agent may update only its own `refs/drafts/<agent>`; drafts can move but accepted history cannot. File transport has declared identity and trusted-local filesystem access; token-authenticated loopback HTTP belongs to M1b.

Reference submitted objects before reporting receive success, including quarantine/ref-visibility crash windows qualified by S1/S7. A repeated identical commit returns its existing change. A newer version with the same Change-Id may supersede a queued version, but a validating version finishes first. Each version has one terminal result: landed, rejected, superseded or cancelled. Blocked and awaiting_approval are nonterminal. Define post-validation replacement and dependency retargeting explicitly; never silently substitute unvalidated content under an existing outcome.

SQLite WAL is the queue and metadata store, not the acceptance ledger. Refuse schema-version mismatch; no migrations. An exclusive lock enforces one engine, with local IPC, CLI auto-start and foreground supervision. If registration is unavailable, the hook rejects. Startup uses accepted-history evidence to reconcile landing, rebuild candidates and restore notes without duplicate publication. A Change-Id alone must not confuse superseded submission versions: match their accepted identity as well. Git ref transactions do not imply cross-store or power-loss atomicity.

## Validation and trust

Pipeline candidates accumulate queued changes and validate in parallel. Publish only with exact final tree, parent and approved configuration evidence. Failure/rejection/supersession invalidates stacked candidates; rebuild independent descendants, and block/reject changes whose prerequisite no longer exists. A later dependency on a reverted change needs explicit re-evaluation, not automatic replay onto incompatible trunk.

Explicit batch mode validates only the batch tip and bisects failure. Intermediate revisions say “validated as batch tip”; they do not satisfy pipeline's per-revision guarantee. This distinction must remain visible in notes, outcomes and metrics. Flake retries stay on the same candidate; passing after failure is recorded, never fabricated. Slots expose resource variables, private temp directories, bounded output and process-group timeout cleanup without claiming test sandboxing.

Gate configuration comes from trusted trunk. Protected paths require explicit approval. A trunk-owned command string invoking a candidate-modified script is not sufficient protection: pin executable policy inputs and check/affected-test/driver/regeneration logic to the approved version, and define the approval transition before shipping. Repository tests still execute candidate code; describe that trust boundary honestly. Git merge-driver behavior and generated-file handling must be qualified, not inferred from using Git CLI.

The optional full tier detects post-landing failures. Confirm a reproducible culprit before/after its change before automatic gated compensating reversion; flaky/uncertain attribution never authorizes revert. Preserve history. If the revert conflicts or fails, freeze and notify the owner. `revert`, `freeze` and `continue` are explicit policies; the default remains pending.

## Qualification and open decisions

M0 requires S1 transport (Linux/macOS and recorded Windows/fallback results), S2 differential merges, S3 warm validators, S4 queue simulation, S5 forge cycles, S6 lockfile regeneration and S7 crash windows. Reports plus throwaway scripts belong under docs/spikes. Failure requires an ADR redesign before product code. Git ≥2.40, SHA-1-only authority, unsupported submodules/LFS and best-effort Windows are qualification targets, not tested support claims.

The maintainer must settle tagline, merge default with a gate, validation mode/slot count after S4, full-failure default, M1b position relative to the users gate, and initial merge-rule ecosystems. Direction favors textual merging with a gate, pipeline validation and recovery by revert, but these recommendations are not selected defaults. No check means path mode and no claim of a validated trunk. Context access/retention and later parser choices remain open in their later-stage ADRs.

No compatibility shims, model acceptance, synchronous push-until-tests-complete, custom storage, line-range locks, per-draft mutable namespaces beyond one draft ref per agent, or fleet features without evidence. Product replacement waits for M0; no source deletion is authorized by merely reading this ADR.

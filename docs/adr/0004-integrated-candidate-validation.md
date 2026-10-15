# ADR 0004: Integrated validation evidence

Status: revised proposal under [ADR 0003](0003-git-native-integration-queue.md); unimplemented. The former scratch-worktree/publication-gate retry design and parent-only policy are superseded.

Pipeline validation binds the exact final candidate tree, accepted parent and approved base-trunk configuration to the result and attempts. Warm slots run gate commands with bounded output and process-group timeout cleanup; they are not sandboxes. Rebuild speculative descendants after rejection, supersession, changed dependencies or upstream advancement. Required checks cannot be replaced by workspace checks or stale-parent evidence.

Pin executable check policy as well as its command string to approved trunk. Candidate modifications to scripts, affected-test hooks, merge drivers and regeneration commands cannot authorize themselves. Protected changes need explicit approval. Regeneration captures its resulting tree before checking and publication; do not validate one tree and commit another.

Batch mode records batch-tip coverage and never claims each intermediate revision passed individually. Flake retries require an actual pass on the same candidate and retain failure evidence. Full-tier culprit discovery must reproduce before/after failure before a gated revert; failed reverts freeze and notify. Defaults and queue economics await M0 and maintainer decisions.

# Continue committed work after squash landing

`mai continue PROJECT SOURCE DESTINATION --change CHANGE --generation N` creates a new workspace from accepted trunk and replays only local commits after one explicitly identified landed submission. The original clone stays intact. This is a conservative copy operation, not in-place sync, autostash, automatic conflict resolution or acceptance of the replayed work.

Use it when your submitted change has landed and you have made later commits in the same clone. Obtain the exact change and generation from `mai show`; do not use a merge-base guess to identify what was squashed. The command verifies the authority's accepted identity, captures a trunk snapshot and refuses any suffix commit already registered with that authority. If another submitted child has landed, select that child's exact identity instead. Pending or rejected submitted children require diagnosis rather than replay as new work.

```sh
mai show PROJECT CHANGE --generation 1 --json
mai continue PROJECT /path/to/old-clone /path/to/new-clone \
  --change CHANGE --generation 1 --json
```

The engine may remain running. The operation obtains a serialized authority snapshot and releases it before copying. Later trunk movement does not change the reported copy/replay snapshot, but the resulting workspace may already be behind newer accepted work. When bootstrap is requested, continuation refuses if the policy loader observes a different trunk base before setup; preserve the destination and retry at a new path, or explicitly skip bootstrap when separate setup is appropriate. Submission and integration still use the normal current-trunk gate.

## Preconditions and preserved work

Stop concurrent editors and Git writers in the source while continuation runs. The source must be an ordinary standalone clone with a clean tracked tree and index, no nonignored untracked files, no merge/rebase operation and a linear local suffix of at most 256 commits after the exact submitted commit. Linked worktrees, ambiguous ancestry and index flags that can conceal changes are refused. A dirty source is not stashed or reset; commit or otherwise preserve your work explicitly before retrying.

Ignored files, such as build outputs, remain in the original clone and are not copied. The destination receives committed content plus configured bootstrap output. Source refs, index, configuration and files are not updated; the command checks its source snapshot for changes. That check cannot make concurrent external editing transactional, so keep the source idle until it returns.

The destination must not exist and must be outside the source and authority. An existing empty directory, symlink, incomplete continuation or successful prior destination is refused. The operation reserves the new directory and records `.mai-continuation-incomplete` before copying. Failure or interruption preserves the destination and its evidence. Repeating the same command does not overwrite it: inspect and retain it, then choose a new destination for a fresh attempt. The command does not automatically remove old evidence. An interrupted child operation may still be finishing in its reserved destination; a marker is not proof of process quiescence. Preserve that destination rather than editing or reusing it while such work may be active.

Configured workspace bootstrap runs after replay, using policy pinned to the selected accepted trunk. `--skip-bootstrap` explicitly skips that step. Bootstrap is trusted project code, not an OS sandbox; its external effects remain the policy author's responsibility. Use `--skip-bootstrap` when setup needs separate control. Neither successful replay nor bootstrap replaces submission, protected approval or the integration gate.

## Conflicts and next steps

If replay conflicts, the source remains the authoritative copy of your local work. Preserve the failed destination and read the reported failing commit and next action. To correct conflicting intent without rewriting the old clone, create a separate fresh workspace with `mai workspace`, inspect the accepted content and your original changes, make the intended correction, commit it and submit normally. Do not delete markers or force a failed destination to look complete.

Replayed commits retain their original author and use the explicit committer `Mai Continuation <continuation@mai.invalid>`; the output reports that identity and the old/new commit mapping. This per-operation identity does not change source Git configuration. On success, use the new workspace for subsequent work. Submit each replayed commit in order, parent before child, using the mappings returned by continuation; pushing only the tip does not register missing parents. For a single replayed commit:

```sh
git -C /path/to/new-clone push -o agent=alice mai HEAD:refs/for/main
mai show PROJECT alice-NEW_COMMIT_OID --generation 1 --json
mai integrate PROJECT alice-NEW_COMMIT_OID --generation 1 --mode textual --json
```

Keep the original clone until you have independently verified the intended result and preserved any ignored or other local material you still need. A terminal integration rejection is not changed by repeating continuation or integration. Start a corrected change from accepted trunk when necessary.

## Qualification boundary

This feature handles a clean, unregistered linear suffix after an exact landed boundary. It does not handle in-place synchronization, dirty work/autostash, replay of pending submissions, merge histories, automatic rejection repair, remote authorities or arbitrary source-repository configurations. Those remain separate work in [#27](https://github.com/maida-ai/mai/issues/27). Process-interruption tests establish preserved-state behavior for their tested boundaries, not power-loss durability or a universal recovery guarantee.

The [frozen copy-continuation evidence](spikes/continuation/2026-10-17/README.md) records source preservation, overlap rejection, preserved replay failure and a gated correction to r4, plus literal execution of the documentation. Its single 0.255-second continuation observation is a tiny functional fixture, not performance qualification.

# Trusted-local submission receipt

Receive is explicitly enabled per disposable authority. It retains a submitted commit and records its immutable lifecycle identity before acknowledging success. Receipt does not validate or land the change. Local filesystem access and the declared `agent` push option are trusted-local attribution, not authentication or a hostile multi-user boundary.

```sh
mai receive enable PROJECT --max-input-bytes 10485760 --max-file-bytes 1048576 --max-changed-files 100
# In a workspace whose mai remote points at this authority:
git push -o agent=alice mai HEAD:refs/for/main
mai show PROJECT alice-COMMIT_OID --generation 1 --json
```

These numbers are explicit example choices, not recommended defaults. `max-input-bytes` bounds the received pack; file-size and changed-file limits bound admission. They do not establish a bound on total diff-parsing memory or latency. Inspect the result of enablement before accepting submissions; engine status reports `registration_available` from its receive configuration.

Receive-hook installation requires the executable and authority paths to be UTF-8 and contain no newline. Enablement rejects unsupported paths before mutation. This restriction belongs to receive hooks; workspace and status results retain their lossless platform-tagged native-path contract. Do not decode those path objects as portable UTF-8 strings.

## Identities and retries

One push submits one single-parent commit to `refs/for/main`, with exactly one `-o agent=NAME` option. The parent must be on trunk or be an exact previously registered commit. Submit each pending parent before its child. The response identifies `NAME-<40-character-commit-OID>`, generation 1. An identical commit retry by the same declared agent returns the same identity; a different declared agent cannot take over that commit. Change replacement, supersession and draft pushes are unavailable in this receive path. Direct trunk and arbitrary ref writes are refused.

After a lost response, query the known identity or retry the identical commit; do not invent a new submission merely because the acknowledgment was lost. Git refs and SQLite are separate durable writes. Preserve interrupted authority state for reconciliation rather than deleting retained objects or editing the database.

`mai show` returns a typed submission record. Exit 0 means the query succeeded, not that the submission was accepted onto trunk. Read its state and terminal outcome. A received or queued record is not a landing receipt; a rejected record remains rejected even when querying it succeeds. A child of a rejected dependency cannot become an independent corrected change merely by retrying it. Preserve that workspace and start an independent correction from accepted trunk when appropriate.

## Explicit integration

Run `mai integrate PROJECT CHANGE --mode textual --generation 1 --wait-seconds 600` to integrate one named generation through the running engine. No scheduler selects work automatically. Integrate pending parents first; a child maps its submitted parent to the accepted squash. Results distinguish `landed`, `awaiting_approval` and `rejected`; exact command syntax and lease claims are in the [CLI reference](cli.md). Protected changes return the exact prepared candidate for explicit trusted approval by a distinct actor, then rerun integration. Approval never substitutes for a passing gate.

An active ordinary lease requires an exact `--lease file:PATH=GENERATION` claim even for its owner. Another owner's active lease blocks integration; lease generations fence stale claims. Lease operations require the live engine and use engine time. Client timeouts are not cancellations: the engine serializes integration with other requests, so inspect the submission after the operation ends rather than assuming a timed-out request had no effect.

## Diagnose before retrying

A failed or timed-out command and a rejected submission are different. If receipt or integration has an uncertain response, preserve the command output and query the same immutable identity once the engine can answer. A query may itself time out while the serial gate runs; that does not prove owner death. If the owner exited during publication, restart/reconcile before deciding whether to integrate again. A terminal rejection will not be cleared by retries: inspect its reason and available diagnostics, then submit corrected work as a new change. Replacement generations are not implemented.

`mai show PROJECT CHANGE --generation 1 --json` is the durable inspection surface. For a terminal gate rejection, inspect `state.outcome.reason` and `state.outcome.diagnostics`: `check` summarizes the validation failure, `log_tail` retains bounded labeled stdout/stderr when output exists, and `candidate_ref`/`next_action` identify reproduction context and the next step. Tails are truncated at Unicode character boundaries and NUL characters are replaced; this sanitation is not secret redaction. Review output before sharing it. A tail can omit the beginning of a failure, so preserve original command output too; absent output never means a passing check. Historical gate records in `.mai/gates.db` remain internal storage, with no public history-query command. Do not edit that database to change an outcome. The [operations guide](operations.md#diagnose-current-outcomes) maps observed states to supported next steps, including occupied validators and failed startup.

## Scope and recovery limits

The receive path rejects touched `.env`, `.env.*` and `node_modules` paths, including deletions. Repository scope restrictions still apply. Admission is not a secret scanner or a guarantee that candidate content is safe to execute. The configured gate and explicit protected-policy approval remain separate requirements.

Manual validation does not consume a submission or land it. The explicit integration path, when used, must report its own outcome. Textual integration rejects `.gitattributes`, repository-local `info/attributes` and configured custom merge drivers until their executable policy is qualified. There is no automatic queue scheduler, draft backup or in-place post-squash sync in this increment. Creating a fresh workspace from accepted trunk is useful for new work. For a clean unregistered suffix after an exact landed identity, [copy-based continuation](continuation.md) creates a new workspace and replays only later local commits. Keep the old clone and any unpushed work.

An interrupted active validator can leave an occupied slot claim. Restart/reconciliation does not prove that all descendants have stopped, so automatic slot reuse is refused. For an opted-in Linux claim whose worker completed cleanup, follow [explicit validator recovery](validator-recovery.md) before retrying. Otherwise preserve state and obtain trusted inspection; do not remove the claim merely because a PID is absent. An uncertain publication result must be reconciled before retrying. These constraints shorten diagnosis without pretending that process interruption is automatically recoverable or that process-kill tests prove power-loss durability.

# Operations

Each authority has one local engine owner. Use `mai status PROJECT` to inspect or start it, `mai serve PROJECT --foreground` to run it in the foreground, and `mai stop PROJECT` to request shutdown. See the [engine guide](engine.md) for readiness, ownership and startup reconciliation.

Receive and integration are explicit. A push registers a generation; it does not land. Inspect the durable record with `mai show PROJECT CHANGE --generation N --json`, then use [receive and integration](receive.md) to select and integrate a generation.

## Diagnose outcomes

Preserve the authority, workspace, change ID, generation and command output while diagnosing a problem.

| Observation | Action |
| --- | --- |
| Push response is lost | Query or retry the same change identity with identical commit metadata. |
| Integration times out | A client timeout does not cancel the operation. Query the same generation after the engine can respond. |
| Approval is required | Review the exact candidate and binding, then explicitly approve or deny it. |
| Terminal rejection | Inspect the reason and bounded diagnostics. Correct the work as a new change; a rejected generation cannot be retried into acceptance. |
| Infrastructure error | Preserve evidence and inspect engine output. Do not remove locks or state to force startup. |
| Validator claim remains occupied | Follow [validator recovery](validator-recovery.md); reuse requires its cleanup proof. |

Startup reconciliation checks durable Git and SQLite evidence. Missing or contradictory state refuses recovery rather than inventing a successful result. Process-interruption tests do not establish power-loss durability. See [current delivery and limits](release-status.md).

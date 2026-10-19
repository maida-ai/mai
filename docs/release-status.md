# Current delivery and limits

mai is an evaluable trusted-local foundation for coordinating changes from independent Git workspaces. An operator explicitly enables receive, submits commits, inspects durable records and selects generations for integration through configured validation. An authority can opt into automatic integration after durable receipt.

| Area | Current behavior |
| --- | --- |
| Workspaces | Create independent Git clones with configured bootstrap. |
| Receive | Explicitly enable bounded admission and register a pushed commit as a generation. Receipt does not mean acceptance. |
| Integration | Explicitly select a generation, or opt into automatic serial integration; dependency, policy, lease and gate checks precede landing. |
| Inspection | Read bounded queue summaries and exact-generation records and wait for terminal outcomes. |
| Recovery | Opt-in Linux validator recovery requires exact cleanup proof. |
| Continuation | Replay a clean, unregistered linear suffix into a new workspace after identifying the landed generation. |

## Limits

`mai receive enable PROJECT --max-input-bytes 10485760 --max-file-bytes 1048576 --max-changed-files 100 --auto-integrate` opts into automatic textual integration; these limits are examples. Receipt remains responsive during automatic gates, and a successful push acknowledges durable registration. Inspect the exact submission with `mai show` or `mai wait` to learn its outcome. Explicit integration remains the default.

Automatic attempts run serially and eligible submitted parents precede children. A pending receipt can resume after engine restart. Automatic attempts supply no lease claims; active leases and protected changes can require explicit later integration. Blocked attempts are not repeatedly retried in one engine session. Rejected dependencies need correction from accepted trunk.

Draft backup, in-place sync/autostash, pending-submission replay, forge/container workflows, semantic-object locking, differentiated priority and automated fix-forward are not available. Current manifests and leases use file units. mai runs trusted repository commands without sandboxing and is intended for trusted local use, not hostile tenants.

The documented recovery and workflow measurements are functional evidence; they do not establish production recovery distributions, representative throughput, a performance advantage or support for hundreds of agents. External-user evaluation and equivalent baseline comparisons remain open. Process interruption tests do not establish power-loss durability.

Start with [getting started](getting-started.md), then review the [CLI reference](cli.md), [operations](operations.md) and [architecture](architecture.md).

# Current delivery and limits

mai is an evaluable trusted-local foundation for coordinating changes from independent Git workspaces. An operator explicitly enables receive, submits commits, inspects durable records and selects generations for integration through configured validation.

| Area | Current behavior |
| --- | --- |
| Workspaces | Create independent Git clones with configured bootstrap. |
| Receive | Explicitly enable bounded admission and register a pushed commit as a generation. Receipt does not mean acceptance. |
| Integration | Explicitly select a generation; dependency, policy, lease and gate checks precede landing. |
| Inspection | Read bounded queue summaries and exact-generation records and wait for terminal outcomes. |
| Recovery | Opt-in Linux validator recovery requires exact cleanup proof. |
| Continuation | Replay a clean, unregistered linear suffix into a new workspace after identifying the landed generation. |

## Limits

Automatic integration scheduling, draft backup, in-place sync/autostash, pending-submission replay, forge/container workflows, semantic-object locking, differentiated priority and automated fix-forward are not available. Current manifests and leases use file units. mai runs trusted repository commands without sandboxing and is intended for trusted local use, not hostile tenants.

The documented recovery and workflow measurements are functional evidence; they do not establish production recovery distributions, representative throughput, a performance advantage or support for hundreds of agents. External-user evaluation and equivalent baseline comparisons remain open. Process interruption tests do not establish power-loss durability.

Start with [getting started](getting-started.md), then review the [CLI reference](cli.md), [operations](operations.md) and [architecture](architecture.md).

# ADR 0009: Cross-agent atomic groups

Status: revised M5 proposal, unimplemented. [ADR 0003](0003-git-native-integration-queue.md) supersedes the earlier late draft-DAG plan. Dependent submissions and retained drafts are M1 core; clone/fetch/merge provide native fork/fuse and need documentation, not a custom engine.

M5 adds atomic topic landing across agents: an explicitly ordered set publishes together or not at all. Validate missing dependencies, cycles, leases, policy and attribution. Ordinary change revisions remain single-parent; upstream syncs are the only merge commits on the accepted first-parent chain. Never treat a pending draft as accepted rN.

Define group validation scope before implementation. Pipeline guarantees require each intermediate landed tree's exact evidence; checking only the group tip must be labeled as weaker batch-tip coverage. A single ref CAS can expose a complete qualified chain, but does not establish SQLite/ref/power-loss atomicity. Reconcile all accepted members after crash without duplicate outcomes or half-reported acceptance.

The external-user gate still precedes M5. Group cancellation, supersession, rejected dependencies, retention and retry identity need tests; ordinary dependent changes must not wait for this feature.

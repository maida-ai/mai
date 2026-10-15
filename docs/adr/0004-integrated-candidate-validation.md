# ADR 0004: Validate the exact integrated candidate

Status: proposed, 2026-10-14. Integrated-candidate checks described here are not implemented. Existing exact-tree and exact-parent safety requirements remain binding.

## Proposal

Materialize an immutable candidate in an isolated scratch worktree or reusable pool and run configured checks outside the publication gate. Bind the result to candidate tree, parent, check configuration and outcome. At publication, recheck authority head, leases and other deterministic requirements, then publish through CAS only if that validated tree and parent are still the candidate being accepted.

Strict policy rebuilds and reruns checks when main changes, or returns a typed non-published retry outcome. Bound retries and cancellation without weakening validation or losing the submitted delta. A passing workspace check cannot substitute for a check of the integrated tree. Checks may execute repository code; define environment, output limits, timeout, cleanup and scratch isolation without claiming an OS sandbox.

A proposed `parent` policy that accepts after the checked parent becomes stale. This is an unresolved, weaker mode requiring an explicit maintainer decision. A label such as “validated on parent rN” does not satisfy required integrated validation; never silently fall back to it or present it as proof that the eventual tree passed. Optional post-land observations cannot retroactively authorize a failed or unchecked landing.

## Consequences and verification

Keep this work independent of checkout-free descriptors, structural merge and batching. Opt-in Git textual merge and later [structural merge](0006-units-of-change.md) must validate their actual results. [Batch publication](0005-serialized-publication-and-batching.md) cannot reuse checks bound to a different accumulated tree or parent.

Test cross-file incompatibility, check failure and timeout, parent changes during checking, restart, and successful exact-candidate publication. Required checks and deterministic conflicts cannot be overridden by a model. Optional human policy exceptions must be separate, explicit and audited; they cannot bypass required deterministic checks.

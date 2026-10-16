# Agent instructions for contributing to mai

mai is a standalone Apache-2.0 Rust project targeting 3–30 coding agents on one repository. [ADR 0003](docs/adr/0003-git-native-integration-queue.md) defines the rewrite; [architecture](docs/architecture.md) summarizes it and [philosophy](docs/philosophy.md) explains why. Current 0.0.1 still uses the old worktree engine. Never describe the rewrite as shipped.

## Work and design gates

* Follow [ADR 0010](docs/adr/0010-staged-mvp-qualification.md) for staged MVP qualification: the fresh native S3 functional evidence and MVP scope of ADRs 0003/0004 are reviewed and accepted; the maintainer-approved implementation plan may begin product replacement. S4 realistic performance and future S7 mechanisms are not blanket blockers; each implemented correctness path needs crash/failure tests before enablement. Preserve uncommitted user changes.
* Discourage premature optimization: require a measured practical problem before adding tuning or complexity. Defer realistic performance claims, the 15% warm-overhead target and differentiated priority policy until MVP observations and the priority RFC. Start with a simple single-class dependency-aware queue; unqualified priority/recovery features stay unavailable.
* Keep decided direction, spike-pending mechanisms and open maintainer decisions distinct. Do not select pending tagline, merge/mode/slot/failure defaults, ecosystem examples or M1b/user-gate ordering on the maintainer's behalf.
* Use Rust and the existing toolchain. Read source, Cargo.toml, Cargo.lock, nearby tests and relevant docs before code changes. Add no production dependency without a concrete need and authorization. Prefer Git CLI merges/ref updates; in-process readers require profiling evidence.
* No backward compatibility is required for repository formats, metadata, CLI or JSON schemas. Reject unsupported versions; add no migrations, aliases or legacy readers. Keep MVP versioning at 0.0.x, then 0.1.0 after MVP and 1.0.0 at stability.
* Add regression tests for success and failure; prefer a failing regression first. Run `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, and `cargo test --all-targets --locked`. Publication work needs separate-process, interrupted-state and dependency-chain tests.
* Keep core open source and self-hostable. Never implicitly migrate production Maida assets; follow docs/maida-migration.md. Never force-push a customer's forge remote. Write Markdown prose paragraphs on one source line.
* M2 and later wait for the external-user gate unless the maintainer explicitly overrides it. When in doubt, prefer the option that shortens recovery without weakening the trunk gate.

## Rewrite invariants (not current implementation claims)

ADR 0010 controls staging: the priority and automated recovery clauses below constrain future enabled features; they do not require those features in the initial MVP. Core durability, validation, ownership and work-preservation rules remain mandatory.

1. Trunk's first-parent chain is the acceptance ledger; each accepted first-parent commit is a change or upstream sync. r0 is the import baseline. Store rN in a trailer; do not recompute it by history traversal.
2. Only the engine landing step writes trunk, using CAS. Receive policy permits submission refs and the pusher's own drafts; local filesystem access and declared file-transport identity are not security boundaries.
3. In pipeline mode, every landed tree passed the gate exactly as landed. Record base-trunk config hash, candidate tree/parent, result and attempts. Batch-tip evidence must never claim each intermediate revision passed separately.
4. Read gate configuration and protected-path policy from trusted trunk. Protected changes require explicit approval. Pin executable check scripts, affected-test hooks, merge drivers and regeneration commands to approved policy; reading only the command string from trunk cannot prevent candidate scripts from disabling checks.
5. Reference received commits before acknowledging push success. Give each submission version exactly one typed terminal outcome; reconcile Git and SQLite after interruption rather than claiming a shared transaction.
6. mai does not mutate agent working trees except explicit convenience operations: workspace creation/bootstrap, submit's requested capture, and sync. Sync refuses to overwrite dirty or untracked files; explicit autostash must preserve and recover work.
7. Models never accept, merge or approve; they may only add conflicts or escalate. UNCERTAIN never passes a gate.
8. Agents share intent, never raw context through awareness. Each conflict has one deterministic resolver. Context checkpoint access/handoff is explicit and remains a separate unresolved M4 policy.
9. mai does not run agents, call models in core, or containerize tests. Validator slots are not OS sandboxes.
10. Landing requires no agent rebase/update first. Return actionable outcomes for true conflicts, checks, policy, leases and invalid submissions. Squash-landing sync safely replays only subsequent local work.
11. Git-tree integration needs no checkout. Configured regeneration and validation execute in warm validator slots; the regenerated final tree must be the tree checked and landed.
12. No feature depends on long-lived shared branches. Per-agent drafts are recovery refs. Pending dependent changes are M1 core; map submitted parents to landed squashes and rebuild/reject descendants after rejection, supersession or revert.
13. Restore confirmed trunk failures by bounded fix-forward with a gated compensating revert as backstop; never rewrite history. Confirm culprit evidence before issuing the temporary sev grant. Fixes need the normal gate and full-tier confirmation within the recovery bound; failed or timed-out confirmation invokes the backstop. Never auto-revert flaky/uncertain attribution. Failed/conflicting reverts freeze and notify. Policy/window defaults await maintainer/S4 decisions.
14. Objects (symbols/config keys) are the units of locking, conflict and change; file units are explicit reported fallback, never line ranges. M1 has file units; M2 adds parsers, containment and module units. Module scope is not leasable by default; renames are delete-plus-add. Follow [ADR 0004](docs/adr/0004-objects-manifests-and-priority.md).
15. The engine computes receive-time and landing-time manifests from actual diffs, with parser/fallback evidence, JSON notes and a rebuildable index. Agent-declared touches are intent, never authoritative manifests. Manifest narrowing never proves safety or authorizes a landing.
16. Priority orders queues, preempts leases after notice/grace and assigns conflict resolution; it never chooses content, overwrites work, bypasses leases or weakens the gate. Normal is default; sev/high grants are audited and expire. Engine aging and dependency inheritance are explicit controlled exceptions, with a sev cap. Lower-priority conflicts resolve; ties use later lander. check.sev_gate remains an unresolved maintainer decision and must not be silently enabled.

## Coding and testing

* No backwards compatibility required until version 0.1+
* Use Rust and the existing toolchain. Read source, Cargo.toml, Cargo.lock, nearby tests and relevant docs before code changes. Add no production dependency without a concrete need and authorization. Prefer Git CLI merges/ref updates; in-process readers require profiling evidence.
* Keep research harnesses and experiment setups in `experiments/`, separate from product tests and benchmarks; keep reviewed results under `docs/spikes/`. Experiment CI must be manual-only (`workflow_dispatch`), with explicit platform selection and bounded artifact retention. Do not dispatch expensive experiments without a request to collect that evidence.

## Branches, commits and pull requests

* Use GitHub issues and the Project to establish scope and acceptance criteria. Before making changes, create a descriptive task branch from the intended base, or reuse the existing branch for the same task. Do not commit task work directly to `main`; preserve unrelated user changes.
* Split work into atomic, self-contained commits. Commit messages must explain the change and its rationale, checks run and their results, and material limitations so this information remains in Git history independently of the PR.
* When changes are ready for review, push the task branch and use `gh pr create --base <base> --head <branch> --title <title> --body-file <file>`. Use `--draft` for unfinished work. Respect explicit local-only or no-push instructions; in that case prepare the PR title/body locally and report that it was not published.
* Before creating a PR, check whether one already exists for the task branch. Update that PR with `gh pr edit` instead of creating a duplicate. Keep its title and body aligned with the final changes, including later evidence or scope changes; do not copy stale progress reports.
* Write a descriptive PR title and a body covering the problem, resulting changes, verification commands/results, evidence locations, and remaining risks or followups. Distinguish measured results from simulations and missing verification. Reference the issue with `Refs #N` for partial work; use `Closes #N` only when all acceptance criteria are met. Merging partial evidence does not complete a qualification gate.
* Report the branch, commits and PR URL at handoff, plus checks not run and anything requiring maintainer review. PR creation does not authorize approval, merge, issue closure or changes to maintainer decisions.

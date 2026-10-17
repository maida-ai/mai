# Agent integration

## Current 0.0.1 foundation

The old worktree engine and its submit/refresh commands are retired. The replacement foundation supports authority administration, independent workspaces and manual trusted validation. `mai workspace PROJECT AGENT DESTINATION` creates a clone and runs configured bootstrap; it does not install a draft-push hook. Explicit Git submission and named-generation integration are available; automatic scheduling, draft hooks and in-place sync/autostash remain unavailable. [Copy-based continuation](continuation.md) handles a clean unregistered suffix in a new workspace after an explicitly verified landed submission. Follow the [runnable walkthrough](getting-started.md) and [receive contract](receive.md). Continue using ordinary Git and existing review processes for real work. See the [current CLI reference](cli.md); the protocol below remains planned.

## Planned protocol — not runnable yet

M1 agents use ordinary Git clones. The implemented `mai workspace PROJECT AGENT DESTINATION` creates a clone, sets `mai.agent` and runs configured bootstrap. Installing a draft-push hook remains planned. Container agents use their own clones over M1b smart HTTP. No agent receives write access to the authority filesystem as a substitute for the receive policy.

1. Make small coherent changes and commit normally. The planned post-commit hook asynchronously pushes the agent's own `refs/drafts/<agent>`; local commits alone do not prove remote recovery durability.
2. Submit with `git push mai HEAD:refs/for/main`, or the proposed convenience `mai submit`. Only the wrapper may auto-commit dirty work, with explicit path selection and untracked-file disclosure.
3. Treat push success as registration of a durable change ref. Record its change ID. The engine may queue, block, await approval, reject, supersede or cancel it; submission is not landing.
4. Use `mai wait <change>` for a terminal outcome. Continuing commits on a pending submission creates dependent changes; they must not silently land without their prerequisite.
5. On landing, the harness invokes `mai sync` to replay only commits after the last landed submission onto fetched trunk. It refuses a dirty tree unless explicit `--autostash` was requested, and must preserve conflicting or untracked work.
6. On rejection, read the typed reason, bounded log tail and candidate ref. Fetch the candidate to reproduce the failure; resolve a true conflict or regenerate the change. Do not rebase merely because trunk advanced and do not bypass the receive endpoint by pushing main.
7. Report `rN` only for a landed outcome. Keep change IDs and revision IDs distinct. Protected-path changes wait for explicit approval.

`mai sync` is an explicit local Git operation that can encounter conflicts, not permission to hard-reset a clone. Exact JSON field names, exit codes, hook settings and complete CLI syntax remain implementation work. The following snippets are instructions to add to a harness only after the protocol ships; they do not assert vendor-specific hook support.

## Claude Code instruction snippet — planned

```text
Work in your assigned Git clone. Commit coherent changes normally.
Push HEAD to mai's refs/for/main and record the returned change ID.
Wait for its terminal mai outcome before claiming integration.
After landing, invoke mai sync; preserve dirty/untracked work and surface conflicts.
Do not push main or edit the authority. Report rejected checks with the candidate ref.
```

## Codex instruction snippet — planned

```text
Use ordinary Git commits in this assigned clone and submit with
 git push mai HEAD:refs/for/main
A successful push registers work; it does not prove landing.
Use mai wait with the returned change ID. After a landed result, run mai sync.
Never reset away pending work to clear a sync failure. Return the typed failure
and reproduction candidate when integration is rejected.
```

## Aider instruction snippet — planned

```text
Keep local commits as checkpoints; they are compatible with this planned protocol.
Submit the committed HEAD with git push mai HEAD:refs/for/main.
Wait for the returned change ID to land, then have the harness run mai sync
before continuing from the integrated baseline. Preserve later commits on sync.
Do not treat a draft push or a successful submission push as a landed revision.
```

## Native Git collaboration and later capabilities

Forking or combining draft work uses ordinary clone/fetch and merge operations, not a new mai draft storage format. Fetch the selected authorized draft into an agent clone and merge it locally, then submit normally. Fetchability of Git commits does not grant access to another agent's raw model context. M3 shares structured intent and activity; M4 resume needs an explicit context access/handoff design. M5 atomic cross-agent topic landing remains a separate feature.

Harnesses own agent scheduling and context preparation. mai does not run agents, call models or summarize raw context. Checks and bootstraps may execute repository code; isolate global resources according to [configuration](configuration.md#engine-local-settings-and-isolation).

## Objects, priority and recovery — proposed amendment

[ADR 0010](adr/0010-staged-mvp-qualification.md) keeps file-unit manifests in MVP scope but defers differentiated priority and automated recovery to RFC #93. The priority/fix/grant/preemption behavior below is future proposal material, not initial MVP behavior or permission to enable it.

M1 represents every changed file as an explicit file unit and computes manifests itself at receive and landing; agents never supply the authoritative affected-object list. M2 adds symbol IDs such as `src/api.rs::Router::add_route`, config keys such as `Cargo.toml::dependencies.serde`, and module scope `src/api.rs::<module>`. Unsupported inputs fall back to file units with a reason. Class leases cover methods; file leases cover every contained object. Module scope is not leasable by default, and moves initially appear as delete-plus-add. Line ranges are never units.

Planned push options augment the normal submission:

- `-o mai.touches=<unit>[,<unit>...]` declares intended scope. The engine compares it with its manifest and emits `scope_drift` warnings; configured protected-object drift may reject. Exact escaping/identifier grammar must be specified before use.
- `-o mai.priority=<class>` requests `sev`, `high`, `normal` or `low`. Normal and low are self-assigned; severe and high require audited, expiring grants. Who may issue those grants remains a maintainer decision.
- `-o mai.fixes=<change>` links a bounded fix-forward attempt to its confirmed culprit and temporary severe grant. The engine verifies an active incident and its scoped grant; this option alone does not authorize a priority upgrade or bypass validation.

When the future features are accepted and enabled, these rules apply to the Claude Code, Codex and Aider snippets above. Respond to lease-preemption notice by preserving work, not discarding it; after grace the new owner is enforced at landing. Higher class takes queue/conflict precedence and the lower side resolves, with later lander resolving ties. Priority never silently overwrites either draft or approves a merge. Dependencies inherit class and waiting work ages up to high as engine-derived effective-priority exceptions, not self-granted requested classes; only severe work may preempt lower-class speculative slots.

Use `mai log --object <unit>` to inspect indexed object history once implemented. Grant administration is proposed as `mai priority grant <agent|change> <class> --reason ... --ttl ...` and `mai priority revoke`; do not invent permissions or revoke flags. A severe fix must pass the normal gate and full confirmation within its recovery bound. If it cannot, preserve work for relanding after the validated revert backstop. A narrower severe gate is only an open proposal, not permission to skip checks.

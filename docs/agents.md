# Agent integration

## Current 0.0.1 precautions

The existing binary uses linked worktrees, not the proposed clone/push protocol. Give each agent a unique workspace, inspect `git status --short --untracked-files=all` before `mai diff` or `mai submit`, and quiesce editing during submit/refresh. Both commands stage all nonignored files. Do not commit, amend, rebase or switch branches: HEAD is the submit base and shared main can bypass policy. A recovered receipt can describe older work while new changes remain pending; inspect accepted content before claiming success. Preserve complete drafts outside the workspace before any destructive recovery. See the [current CLI reference](cli.md).

## Planned protocol — not runnable yet

M1 agents use ordinary Git clones. `mai workspace <name>` is an explicit convenience that creates a clone, sets `mai.agent`, installs a draft-push hook and bootstraps the environment. Container agents use their own clones over M1b smart HTTP. No agent receives write access to the authority filesystem as a substitute for the receive policy.

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

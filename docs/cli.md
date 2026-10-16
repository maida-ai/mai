# CLI reference

**Current 0.0.1 behavior.** These commands describe the existing engine retained during M0. They are not the proposed clone/receive/queue interface.

Every repository command takes an authority directory (`PROJECT`). An agent works in `PROJECT/workspaces/AGENT`; it does not manage Git branches. Use `mai --help` or `mai COMMAND --help` for argument help.

## Commands

- `init PROJECT [--from GIT_REPO]`: create an empty authority or import Git HEAD as r0. The target must be empty.
- `workspace PROJECT AGENT`: create an agent's detached full Git worktree. Agent names are 1–64 ASCII letters/digits/underscore/hyphen, starting with a letter or digit.
- `refresh PROJECT AGENT [--discard]`: update a clean workspace. Explicit `--discard` destroys local edits and nonignored untracked files.
- `lease PROJECT AGENT PATH... [--ttl SEC]`: reserve file/directory paths, default 900 seconds, allowed range 1–86400. A trailing slash is normalized; foreign overlapping leases reject.
- `release PROJECT AGENT [PATH...]`: release exact reservations or all owned leases. Releasing an ancestor does not release descendants.
- `status PROJECT`: inspect head, revision, registered workspaces, and active leases.
- `diff PROJECT AGENT`: stage all changes and print a unified diff.
- `submit PROJECT AGENT -m TEXT [--check CMD]`: prepare changes, reject overlap/foreign leases, publish one revision.
- `history PROJECT [-n COUNT]`: newest revisions, including at most the import snapshot r0. The default is 10.
- `show PROJECT rN PATH`: print exact file bytes; `HEAD` is also supported.
- `github-link PROJECT URL`: configure a Git-compatible downstream remote named `github`.
- `github-sync PROJECT`: explicitly push a snapshot of accepted main, without force. Remote divergence rejects safely.
- `--json`: structured success/error responses. Place before the command for consistency across clients.
- `--version`: print `mai 0.0.1`.

## Checks and leases

`--check` executes an explicitly supplied, trusted shell command in the workspace before preparation. Output is captured so JSON responses remain parseable. A failed check preserves the draft and rejects publication. It checks the agent's base workspace, not the eventual integrated trunk. Shell syntax is platform-specific: Unix uses `sh -c`; Windows uses `cmd.exe /D /S /C`.

```sh
mai submit demo alice -m 'Add greeting' --check 'test -f hello.txt'
mai lease demo alice src/ --ttl 600
mai release demo alice src/
```

Windows check example:

```powershell
mai submit demo alice -m 'Add greeting' --check 'if not exist hello.txt exit /b 1'
```

Leases expire by wall-clock time and are checked at publication validation. Own leases can be renewed with another lease command. They remain held until release/expiration, including after submission. Editing is advisory; enforcement is through supported submit operations. No automatic renewal or distributed fencing token exists.

## JSON responses

Successful commands return `{"ok":true,"result":...}`; failures return `{"ok":false,"error":"..."}` and exit 2. For example, a no-changes rejection is:

```json
{"ok":false,"error":"There are no changes to submit"}
```

`show` encodes arbitrary file bytes in `content_base64`, alongside the requested revision/path. `diff` returns a string. Status/history return structured objects/arrays. Object identifiers and timing fields are actual operation results, not fixed values.

## Submission results and recovery

Success includes `revision`, `commit`, `agent`, `files`, `rebased`, `recovered`, `workspace_updated`, `retries`, `lock_wait_ms`, and `lock_hold_ms`. `rebased:true` means disjoint candidate paths were transplanted onto newer main, not that same-file text was merged.

Check both acceptance and workspace state. `recovered:true` returns an already accepted revision after interrupted checkout; it does not create another revision and does not prove current edits were submitted. The current recovery path can return `ok:true` before examining new edits. Do not report that old receipt as acceptance of new work. `workspace_updated:false` means the working directory needs reconciliation before continued work. Copy drafts/later edits outside the workspace, inspect accepted history/content, before deciding how to reconcile the workspace. Verify intended files with `mai show PROJECT rN PATH`; do not refresh with `--discard` until all pending work has been preserved. Never interpret a recovered receipt as permission to reset files.

Contention-budget errors explicitly mean that attempt did not publish. Retry the same submission after backoff. Foreign lease errors require release/expiration. Overlapping changes require preserving the draft and manually reconciling from a fresh base. Other infrastructure failures may need inspecting authoritative history before retrying.

## Current workspace precautions

Do not commit, amend, rebase or switch branches inside mai workspaces yet: submit derives its base from HEAD, and linked worktrees share authority refs. In particular, committing on main bypasses mai publication checks. Both `diff` and `submit` stage all nonignored files; inspect untracked files before invoking them. There is no `--paths` option or non-destructive dirty `update` command yet. Refresh and post-submit reset can overwrite ignored files when accepted paths collide; preserve valuable local files separately.

## Conflict rules

The same changed file, or an ancestor/descendant path, conflicts with net upstream changes since the workspace base. `src` overlaps `src/file`, but not `src-utils`. Renames use deletion/addition semantics; binary files are not automatically merged. Different edits within one file reject conservatively. Successful independent submissions retain earlier accepted content.

## Proposed rewrite interface — not implemented

M1 replaces the workspace-era protocol with ordinary clones and `git push mai HEAD:refs/for/main`. Planned commands are `init`, `serve`, `workspace`, `submit`, `wait`, `sync`, `status`, `queue`, `lease`, `approve` and object-history `log`; exact flags and output schemas must be implemented and tested before use. `submit` is optional auto-commit/push convenience, `wait` distinguishes registration from landing, and explicit `sync` replays post-submission work after squash landing. M3 adds `intent`/`activity`; M4 adds `checkpoint`/`resume`/`why`. See [agent protocol](agents.md), [configuration](configuration.md) and [operations](operations.md).

The typed lifecycle is received → queued → validating → landed, with dependency blocking, protected-path approval and exactly one terminal outcome including rejection, supersession or cancellation. A versioned schema and reason codes are implementation requirements; the current string-error JSON above is not that schema. No legacy aliases or migration layer are planned.

Under [ADR 0010](adr/0010-staged-mvp-qualification.md), priority and incident-recovery commands/options are deferred to RFC #93, not initial MVP scope. The amendment proposes `mai priority grant <agent|change> <class> --reason ... --ttl ...`, `mai priority revoke` and `mai log --object <unit>`. Push options `mai.fixes`, `mai.priority` and `mai.touches` declare a fix relationship, requested class and intended units respectively; none supplies trusted manifests or bypasses checks. See [agent protocol](agents.md#objects-priority-and-recovery--proposed-amendment) for grant, scope-drift and preemption semantics. All are unimplemented; current path leases above are not the proposed priority/object policy.

[Getting started →](getting-started.md) · [All docs →](README.md)

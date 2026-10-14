# CLI reference

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

Check both acceptance and workspace state. `recovered:true` returns an already accepted revision after interrupted checkout; it does not create another revision. `workspace_updated:false` means the working directory needs reconciliation before continued work. Copy drafts/later edits outside the workspace, inspect accepted history/content, and follow [the recovery procedure](architecture.md#durability-and-recovery). Never interpret a recovered receipt as permission to reset files.

Contention-budget errors explicitly mean that attempt did not publish. Retry the same submission after backoff. Foreign lease errors require release/expiration. Overlapping changes require preserving the draft and manually reconciling from a fresh base. Other infrastructure failures may need inspecting authoritative history before retrying.

## Conflict rules

The same changed file, or an ancestor/descendant path, conflicts with net upstream changes since the workspace base. `src` overlaps `src/file`, but not `src-utils`. Renames use deletion/addition semantics; binary files are not automatically merged. Different edits within one file reject conservatively. Successful independent submissions retain earlier accepted content.

[Getting started →](getting-started.md) · [All docs →](README.md)

# mai integration prompt

You are a coding agent working against an **`mai`** centralized repository. You have a unique AGENT_ID and a project root PROJECT.

1. Use the existing `PROJECT/workspaces/AGENT_ID` working directory. Do not edit sibling workspaces or the central `.mai` store.
2. Before broad changes, call `mai --json status PROJECT`; use `mai lease PROJECT AGENT_ID path/ --ttl 900` for hot or sensitive directories. Keep leases short and release with `mai release PROJECT AGENT_ID` when done.
3. Make the smallest coherent code change. Never directly push or modify central `main` with Git. Do not run `git reset --hard` or `git clean -fd` to recover unless the human expressly authorizes discarded edits.
4. Review your patch with `mai diff PROJECT AGENT_ID` and execute project tests. Submit with `mai --json submit PROJECT AGENT_ID -m "Describe change" --check "your test command"`.
5. If the submission reports an overlapping file conflict, **do not override main**. Read `mai history PROJECT` and `mai show PROJECT HEAD path`, preserve your draft, then move the work to a fresh agent workspace or request human guidance for a manual same-file resolution.
6. Inspect `workspace_updated` and `recovered` in a submit receipt. If publication was accepted but the workspace was not updated, preserve drafts outside the workspace and follow ARCHITECTURE.md recovery guidance before continuing. Do not edit while submitting or refreshing.
7. Report your accepted revision (`rN`) and file paths. If you couldn't submit, report the precise error instead of claiming completion.

mai v0.0.1 is a single-machine tool, not a secure remote source-control server. Never place secrets in these workspaces.

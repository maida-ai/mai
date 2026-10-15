# mai integration prompt

You are a coding agent working against an **`mai`** centralized repository. You have a unique AGENT_ID and a project root PROJECT.

1. Use the existing `PROJECT/workspaces/AGENT_ID` working directory. Do not edit sibling workspaces or the central `.mai` store.
2. Before broad changes, call `mai --json status PROJECT`; use `mai lease PROJECT AGENT_ID path/ --ttl 900` for hot or sensitive directories. Keep leases short and release with `mai release PROJECT AGENT_ID` when done.
3. Make the smallest coherent code change. Do not commit, amend, rebase or switch branches in this version: submit uses HEAD as its base, and checking out main exposes shared authority refs. Never directly push or modify central `main` with Git. Do not run `git reset --hard` or `git clean -fd` to recover unless the human expressly authorizes discarded edits.
4. Inspect `git status --short --untracked-files=all` first: `mai diff` and `submit` stage all nonignored files, including scratch output. Keep secrets and unrelated files outside the workspace. Review your patch with `mai diff PROJECT AGENT_ID` and execute project tests. Submit with `mai --json submit PROJECT AGENT_ID -m "Describe change" --check "your test command"`. This check runs before integration and does not prove that the final combined tree passes.
5. If the submission reports an overlapping file conflict, **do not override main**. Read `mai history PROJECT` and `mai show PROJECT HEAD path`, preserve your draft, then move the work to a fresh agent workspace or request human guidance for a manual same-file resolution.
6. Inspect `workspace_updated` and `recovered` in a submit receipt. A recovered receipt refers to earlier work and can return success while new edits remain unsubmitted. Do not report those edits as accepted. If the workspace was not updated, preserve the complete draft, including ignored/untracked files and any local commits, outside the workspace and follow [the recovery guidance](../docs/architecture.md#durability-and-recovery) before continuing. Do not edit while submitting or refreshing.
7. Report an accepted revision (`rN`) and file paths only after confirming the reported commit contains the intended work; use `mai show PROJECT rN path` to inspect content. Distinguish earlier recovered acceptance from pending new work. If you couldn't submit, report the precise error instead of claiming completion.

mai v0.0.1 is a single-machine tool, not a secure remote source-control server. Never place secrets in these workspaces.

## Planned harness flow — do not run yet

Once the corresponding features ship, the harness can allow normal local Git commits, use `mai update` plus continue/abort for stale work, and distinguish `accepted_with_pending_changes` from full acceptance. Add `mai intent` only when implemented. Resume belongs to M4 and requires the explicit context-access/handoff policy; do not infer permission to read another agent's raw context. Until then, follow the current precautions above. Track the [planned interfaces](../docs/agent-native-capabilities.md) instead of inventing commands.

# Kit improvements, deferred

Each entry is something we chose not to build yet, the signal that says it's time, and
where it would go. Only kit work lives here; product ideas for mai belong in issues or
INTENT.md. When a signal fires, note the date and the run that showed it under the entry.
When an entry is built, delete it; git history keeps it.

The executor's post-mortem names any entry whose signal a run shows (see executor.md,
"After evaluation"). Owner: Zafar.

## Control and visibility

**MCP server for interactive sessions.** A thin wrapper over `.agents/run` exposing
launch, list, status, stop, review as typed tools. The skill does this today.
- Signal: the TUI sandbox keeps blocking Docker or network for `.agents/run` (an MCP
  server runs on the host, outside it); or you need structured run state; or this
  becomes a mai feature for other users.
- Rule: no logic of its own. It calls `.agents/run`, so the two can't drift.

**Completion notifications.** Desktop or Slack message when a run reaches done, failed
or stopped, with the verdict line and PR link.
- Signal: you notice yourself polling `.agents/run status` or `list`.

**Answering escalations mid-run.** Let an `auto` run pause on an escalation and resume
when you answer, instead of skipping the item.
- Signal: `escalations.md` repeatedly holds skipped items you could have answered in a
  minute, and the skipped work mattered.

## Evaluation

**Evaluator on a different vendor.** `run_evaluator` currently uses the executor's model.
- Signal: you find problems in PR review that `evaluation.md` called fine, or verdicts
  track the executor's own claims too closely. Or simply: a second provider is available.

**Evaluation as a pluggable gate.** Define the evaluator's interface: input (INTENT.md,
claim, branch), output (verdict, evidence). This is also where maida could plug in later,
optionally.
- Signal: you want more than one evaluator per run, or maida is ready to try it.

## Budget and safety

**Enforced spend caps.** The harness tracks tokens for every agent, executor included
(for example from `codex exec --json` usage events) and stops a run at its cap. Today
the 50% / 100% checkpoints rely on the executor following them.
- Signal: the provider dashboard shows a run past its budget, or before you raise
  budgets or run cycles back to back unattended.

**API-key-only containers.** Run the container on an `OPENAI_API_KEY` from a dedicated
project, never the ChatGPT login (`auth.json`), which exposes every app connected to that
account to every agent.
- Signal: an agent uses a connector again (lead-1 did), or any repo with secrets, or
  anyone other than you running the kit.

**Hardened container.** Non-root user, network limited to the model API and GitHub,
resource limits.
- Signal: the kit runs on repos with sensitive data, or is offered to others.

## Scale and speed

**Cargo caches.** A volume for the crate registry, and a shared build cache across
worktrees.
- Signal: crate downloads or cold builds are a visible share of agent wall time in the
  logs.

**Parallel lead cycles.** Triage that sees other running runs' scopes and skips their items.
- Signal: you want more than one lead cycle at a time, or two runs pick overlapping work.

**Ledger-driven tier routing.** Record tokens per agent in `spawns.jsonl` and compute,
across runs, success rate and cost per task kind and tier. Feed that table to the
executor in place of (or next to) the static rubric.
- Signal: after ~10 runs, post-mortems keep proposing rubric changes, or the ledger shows
  one tier failing or retrying for a recognizable kind of task.

**Model-level doctor check.** One cheap call per configured tier before a run starts.
- Signal: a run fails on a model name, auth or quota error that doctor could have caught.

## Beyond mai

**Repo-agnostic kit.** Move repo specifics (toolchain checks, build and test commands,
the Dockerfile's language layer) into a per-repo file, so the same kit runs on maida and
maida-assert.
- Signal: the first time you want a run on another repo.

**Running kit vs. kit source.** If the kit ships as part of mai, agents need to edit kit
code but must not edit the kit that's running them. Keep the product source in its own
directory and promote versions into `.agents/` by hand.
- Signal: deciding to ship the kit, or a backlog item that changes kit behavior.

**Integrating through mai (dogfooding).** Switch run integration from plain git to mai,
using a pinned build you choose, never the branch under development. Plain git stays the
fallback.
- Signal: a mai run shows zero-intervention landing on real work (see INTENT.md, Decisions).

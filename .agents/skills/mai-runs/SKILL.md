---
name: mai-runs
description: Launch, monitor, stop and review unattended agent runs on this repo with the agent kit (.agents/run). Use when the user wants a long-running or parallel job done autonomously ("kick off a lead cycle", "have the agents do X"), asks how a run is going, wants a run stopped, or wants a finished run reviewed.
---

# Running the agent kit from an interactive session

The kit (`.agents/`) runs autonomous jobs in background containers: an executor scopes
the work, delegates to sub-agents, verifies, and opens a draft PR; an independent
evaluator judges it. Your job here is the PM console: turn the conversation into a
good directive, launch, monitor, and help the user review. You never do the run's work
yourself, and the run never needs you to stay open.

Run every command from the repo root. They need Docker and network access; if your
sandbox denies them, ask the user to approve `.agents/run`.

## Launch
1. Turn what the user wants into a directive: the outcome, why it matters (which part of
   INTENT.md it serves), constraints and non-goals, a budget and a time box. Use what you
   know from this conversation; that context is the main thing you add. For "decide what
   to work on", use lead mode: "Act as lead for this cycle. Assess the repo against
   INTENT.md, pick the top N changes that most move the north-star metric, groom the
   issue tracker to match, and ship them."
2. Pick a short slug (letters, digits, dashes). One slug per day; check `.agents/run list`.
3. Show the user the slug, directive and budget. Launch only after they confirm.
4. `.agents/run auto <slug> "<directive>"`. Setup and checks run in the foreground; if it
   fails, show the error and stop. Don't retry with a new slug to get around it.
5. Tell the user the run id. They can close this session; the run continues.

Don't use `.agents/run start` here: it opens another interactive session inside this one.

## Monitor
- `.agents/run list` for all runs, `.agents/run status <run-id>` for one.
- Summarize in at most 5 lines: phase, elapsed time, agents done / failed / running, and
  anything notable in the log tail. Don't paste raw output unless asked.
- For detail, read `runs/<id>/executor.log` or `runs/<id>/agents/<agent-id>.log` and
  summarize the part that answers the user's question.

## Stop
`.agents/run stop <run-id>`. It cleans up worktrees; committed work stays on `run/<id>`.
Confirm with the user first unless they asked for it directly.

## Review a finished run
A finished run ends in phase `needs-review` with `runs/<id>/review.md`: the headline
numbers, the evaluator's spot checks, the post-mortem's decisions, and the PR. The next
run can't start until it's reviewed. Your job is to make the review fast, not to do it.
- Go through the sheet item by item with the user. For spot checks, you may run the
  command and show its raw output; the user judges whether it holds.
- Write only the user's own words on `Decision:` lines. Never fill one yourself, and
  never infer a decision from silence.
- When every line is filled, run `.agents/run review <id>`. With `ready`, the PR leaves
  draft and the user merges it on GitHub. With `close`, the PR is closed.
- With `rework: <what>`, offer `.agents/run rework <id> <slug> ["<extra>"]`: a new run
  that starts from the reviewed branch with the review as its brief, and supersedes the
  old PR. Confirm the slug with the user first, like any launch.
- `.agents/run review <id> --skip "<reason>"` only when the user asks and gives the
  reason; it's recorded.
- For background, `report.md`, `evaluation.md`, `escalations.md` and `proposals.md` are in
  the same directory. If publishing failed, the fix is in `publish.log`; retry with
  `.agents/run publish <id>`.

## PM actions: only with explicit approval of the exact text
- Answering an escalation means adding a line under `## Decisions` in INTENT.md.
- Accepting a proposal means editing `.agents/executor.md` (or another kit file it names).
- Show the diff, apply only after the user approves it, then commit and push: runs start
  from the default branch on GitHub, and doctor refuses to start if the kit there differs.

Never edit INTENT.md, AGENTS.md or `.agents/` on your own initiative, never merge a
`run/<id>` branch into main, and never launch a run the user hasn't confirmed.

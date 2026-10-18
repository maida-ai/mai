# Agent kit

Runs an autonomous engineering loop on this repo. You act as PM: you give a short directive.
An executor (the tech lead) scopes it, delegates to scope-bound sub-agents, verifies the
result and writes a claim. An independent evaluator then judges the claim against
INTENT.md, and the executor proposes changes to its own instructions. Everything runs in
a disposable container.

```
you ── directive ──▶ executor ──▶ planners ──▶ implementers
                        │           (depth ≤ 3, every spawn logged)
                        ▼
                 claim.md ──▶ evaluator ──▶ evaluation.md ──▶ executor post-mortem ──▶ proposals.md
                                                                                        │
you ◀──────────────── report.md, PR on run/<id>, proposals to accept or reject ◀────────┘
```

## Files and who owns them

| File | Owner | Purpose |
| --- | --- | --- |
| `INTENT.md` | you | The only binding statement of intent: product, metric, non-goals, when to ask. |
| `AGENTS.md` | you | Project facts and conventions, for any session: yours or the kit's. |
| `CLAUDE.md` | you | `@AGENTS.md`, so Claude Code reads the same conventions. |
| `.agents/agent-rules.md` | you | Kit-only conduct, brief template, report format. Injected into every sub-agent's prompt by `spawn`; your own interactive sessions never see it. |
| `.agents/executor.md` | you | The tech-lead role. Changes only through proposals you accept. |
| `.agents/evaluator.md` | you | The independent evaluator role. |
| `.agents/tiers.sh` | you | Which harness and model run each tier, the executor and the evaluator. |
| `.agents/Dockerfile` | you | The container: Rust, git ≥ 2.40, gh, Node, Codex. |
| `.agents/run` | you | Entry point: `doctor`, `start`, `auto`, `eval`. |
| `.agents/spawn` | you | The only way agents start sub-agents. Logs every spawn and enforces depth. |
| `.agents/pre-commit` | you | Hook that blocks agent commits to the files in this table. |
| `.agents/skills/mai-runs/` | you | Skill for launching and reviewing runs from an interactive session. |
| `.agents/FUTURE.md` | you | Deferred kit improvements, each with the signal that says it's time to build it. |

Agents can't commit changes to any of these. The hook enforces that, and doctor checks
the hook is installed.

## One-time setup

1. Docker usable without sudo (rootless recommended). Codex logged in on the host
   (`~/.codex/auth.json`) or `OPENAI_API_KEY` exported.
2. Optional, for issue grooming and PRs: export `GH_TOKEN`, a fine-grained token limited
   to this repo, owned by the org, with Issues, Pull requests and Contents set to
   read/write. Protect `main` on GitHub so the token can't push to it. Doctor checks
   that the token can push.
3. In the repo:
   ```bash
   printf 'runs/\n.worktrees/\n' >> .gitignore
   cp .agents/pre-commit .git/hooks/pre-commit && chmod +x .git/hooks/pre-commit
   chmod +x .agents/run .agents/spawn
   git add INTENT.md AGENTS.md CLAUDE.md .agents .gitignore && git commit -m "Agent kit"
   .agents/run doctor
   ```
   The first `run` command builds the image (a few minutes). Doctor must be all `ok`;
   a `warn` about GH_TOKEN only means issue and PR work is off.

## Running

```bash
.agents/run doctor                            # environment check; no model calls
.agents/run start <slug> "<directive>"        # interactive: the executor's TUI, you can answer escalations
.agents/run auto  <slug> "<directive>"        # unattended, in the background: executor → evaluator → post-mortem
.agents/run status [run-id]                   # progress of a run (default: latest)
.agents/run list                              # all runs with their phase
.agents/run stop <run-id>                     # stop a background run and clean up
.agents/run review <run-id>                   # record your review (required before the next run)
.agents/run eval  <run-id>                    # evaluate a run started with `start`
.agents/run publish <run-id>                  # push run/<id>, open or update its draft PR
```

Every run ends by pushing `run/<id>` and opening a draft PR (report, claim and evaluation
as the body), if `GH_TOKEN` is set. If that fails, the run still completes; fix the token
and use `publish`.

### Monitoring an `auto` run

`auto` runs doctor, then starts a background container named `mai-<run-id>` and returns.

- `.agents/run status` shows the phase (executor / evaluating / post-mortem / done), finished
  agents with their status, agents still running and for how long, which artifacts exist,
  and the tail of the executor's log. It reads files only, so it works on the host. Use
  `watch -n 15 .agents/run status` for a live view.
- `tail -f runs/<id>/executor.log` for the executor's full output, `runs/<id>/agents/*.log`
  for any sub-agent.
- `docker exec -it mai-<run-id> bash` gives a shell in the running container, including
  the Codex session state, so any Codex inspection command works there.
- `.agents/run stop <run-id>` stops it and cleans up its worktrees; committed work stays
  on `run/<id>`.

There's no steering an `auto` run mid-flight; that's what `start` is for.

`start` and `auto` run doctor first and refuse to start if anything fails. A slug can be
used once per day: starting an existing run id fails with a message saying whether that
run may still be going and how to discard it. Setup happens in the foreground even for
`auto`, so any failure shows up before anything goes to the background. Every run
gets its id (`YYYY-MM-DD-<slug>`), a directory under `runs/`, and its own branch,
`run/<id>`, in its own worktree. Your checkout is never touched.

Runs start from the latest default branch on GitHub, not your local HEAD, so unpushed
local work never leaks into a PR. That means kit changes must be pushed before a run;
doctor checks this. Before claiming, the executor rebases onto the default branch again
and re-verifies, so the PR is current even if main moved during the run. Without
`GH_TOKEN`, runs start from local HEAD and nothing is fetched.

### Lead cycle: let it decide what to do

```bash
.agents/run auto lead-1 "Act as lead for this cycle. Assess the repo against INTENT.md,
pick the top 3 changes that most move the north-star metric, groom the issue tracker to
match, and ship them. Budget \$20, 4 hours."
```

The executor writes `triage.md` (ranked candidates with evidence), rewrites or adds
issues (each tagged `agent-groomed`, with the old body kept as a comment), ships the
picks on `run/<id>` and opens a draft PR. Start with top 3: fewer picks give clearer
evaluator signal.

### From your coding session (Claude Code, Codex, ...)

`.agents/skills/mai-runs/SKILL.md` turns an interactive session into a console for the
kit: describe the job in conversation, and the agent drafts the directive, launches it
after you confirm, summarizes progress when you ask, stops runs, and walks you through
the review. Root AGENTS.md points to it, so any harness that reads AGENTS.md finds it.
For Claude Code's native skill loading, also link it:
`mkdir -p .claude/skills && ln -s ../../.agents/skills/mai-runs .claude/skills/mai-runs`.

Your session needs to run `.agents/run` with Docker and network access. In Claude Code,
allow `Bash(.agents/run:*)`; in Codex, approve the command when asked.

Agents inside a run can't use it: `.agents/run` refuses `start`, `auto`, `stop` and
`publish` when called from a run.

## After a run

Work is delegated; responsibility isn't. A finished run ends in phase `needs-review`
with a review sheet, `runs/<id>/review.md`, and **the next run won't start until it's
reviewed**. The sheet asks for decisions, not reading:

| Section | Source | What you do |
| --- | --- | --- |
| Headline | evaluation.md, report.md | Verdict, north-star number against baseline, spend against budget, PR link. |
| Spot checks | the evaluator | Run each check (≤2 min each) and write whether it held. The evaluator picks them, not the executor, so the executor can't soften what you're asked to verify. |
| Decisions | the post-mortem's decisions.md | Confirm or reverse each escalation, accept or reject each proposal, decide on each FUTURE.md signal. |
| The PR | | `ready`, `rework: <what>`, or `close`. |

Write a decision on every `Decision:` line, then:

```bash
.agents/run review <id>                     # lists what's still open, or records the review
.agents/run review <id> --skip "<reason>"   # skip on the record; the reason stays in review.md
```

A completed review with `ready` on the PR marks the draft PR ready for review. Then
apply what you accepted (INTENT.md Decisions, executor.md edits), commit, and push.
From your coding session, the mai-runs skill walks you through the sheet.

Background, if a decision needs it: `report.md`, `evaluation.md`, `escalations.md`,
`proposals.md`, `triage.md`, `scope.md`, `claim.md`, `spawns.jsonl`, `ledger.md`,
`agents/*.log`, and `executor.log`/`postmortem.log` for `auto` runs.

## Adjusting the system

| Symptom | Change |
| --- | --- |
| Worked on the wrong thing; misread what you want | `INTENT.md`: add the missing priority, non-goal or ask-first rule |
| Right goal, bad process (scope creep, weak verification, didn't delegate) | `executor.md`, ideally from `proposals.md` |
| Sub-agents misbehave the same way repeatedly | `.agents/agent-rules.md` |
| A project convention you want everyone (you included) to follow | `AGENTS.md` |
| Too expensive, or the cheap tier keeps failing | `tiers.sh`: change a tier's model |
| Evaluator too lenient | `evaluator.md`; also move `run_evaluator` to a different vendor |
| A tool or environment problem | `Dockerfile`, then add a check to `doctor` in `run` |

After editing any kit file: commit it, rebuild the image if you changed the Dockerfile
(`docker build -t mai-agents -f .agents/Dockerfile .agents`), then run doctor.
Agents only see committed files.

To use another harness for a tier, define its `run_<tier>` in `tiers.sh`. It receives
the prompt file as `$1` and must run non-interactively in the current directory. Install
that harness in the Dockerfile.

## Cleanup

Worktrees exist only while something runs: agents' worktrees are removed when the agent
exits, and the run's when the run ends. Anything left uncommitted is saved as a patch
(`runs/<id>/agents/<agent-id>.uncommitted.patch` or `runs/<id>/executor.uncommitted.patch`).
At the end of a run, agent branches whose changes all reached `run/<id>` (by merge,
cherry-pick or rebase) are deleted. What's
left are `run/<id>` and the agent branches that were never integrated (failed or rejected
work), kept for inspection.

```bash
git branch --list 'agent/*' 'run/*'      # delete reviewed ones with git branch -D
```

After a crash or `docker stop`, remove leftover worktrees:

```bash
git worktree list --porcelain | awk '/^worktree .*\/\.worktrees\//{print $2}' \
  | xargs -r -n1 git worktree remove --force
git worktree prune
```

## Troubleshooting

| Error | Fix |
| --- | --- |
| `permission denied ... docker.sock` | Docker isn't usable as your user. Use rootless Docker; never run `run` with sudo. |
| `newuidmap ... owned by a different user` | You're in a `newgrp` shell. Open a fresh terminal. |
| `daemon executable not found` (Codex) | A host `~/.codex` got mounted. `run` mounts only `auth.json`; check you have the current `run`. |
| `kit has uncommitted changes` | Commit the kit; worktrees branch from HEAD. |
| An agent reports `needs_scope` | Working as intended: its brief was too narrow or unclear. Check the executor's brief. |

## Known limits

- Executor and evaluator are the same vendor unless you change `run_evaluator`; they
  share blind spots.
- Spend is recorded only as tokens from harness logs. Dollar caps aren't enforced.
- Agents run with the harness sandbox off, so the container is the only safety boundary.

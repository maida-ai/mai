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
| `.agents/config.sh` | you | Settings: models and effort per tier, the evaluator's model, delegation depth, git identity patterns. |
| `.agents/tiers.sh` | you | How each tier, the executor and the evaluator are launched (harness commands). |
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
   printf 'runs/\n.worktrees/\n.agents/config.local.sh\n' >> .gitignore
   cp .agents/pre-commit .git/hooks/pre-commit && chmod +x .git/hooks/pre-commit
   chmod +x .agents/run .agents/spawn
   git add INTENT.md AGENTS.md CLAUDE.md .agents .gitignore && git commit -m "Agent kit"
   .agents/run doctor
   ```
   The first `run` command builds the image (a few minutes). Doctor must be all `ok`;
   a `warn` about GH_TOKEN only means issue and PR work is off.

## The loop

```bash
.agents/run auto <slug> "<directive>"   # 1. launch; returns immediately
.agents/run status                      # 2. wait until the run shows READY:review
$EDITOR runs/<id>/review.md             # 3. answer the checks and decisions
.agents/run review <id>                 # 4. record it; prints exactly what's next
.agents/run merge <id>                  # 5a. if you decided ready: lands the PR
.agents/run rework <id> <new-slug>      # 5b. if you decided rework: starts the follow-up run
                                        # 5c. if you decided close: nothing more
```

No rebasing or pushing by hand: runs start from the latest default branch, the
executor rebases before claiming, `merge` squash-merges through GitHub (CI and branch
protection apply) and deletes the run's branch. Afterwards update your own checkout with
`git switch main && git pull`. You commit and push only kit or INTENT.md edits you chose
to make in step 3.

## Running

```bash
.agents/run doctor                            # environment check; no model calls
.agents/run start <slug> "<directive>"        # interactive: the executor's TUI, you can answer escalations
.agents/run auto  <slug> "<directive>"        # unattended, in the background: executor → evaluator → post-mortem
.agents/run status                            # running runs, dead ones, and those awaiting review
.agents/run status --all                      # every run
.agents/run status <run-id>                   # one run in detail: agents, artifacts, log tail
.agents/run stop <run-id>                     # stop a background run and clean up
.agents/run review <run-id>                   # record your review (required before the next run)
.agents/run rework <run-id> <slug>            # redo a reviewed run on top of its own branch
.agents/run merge <run-id>                    # land a run reviewed as ready
.agents/run eval  <run-id>                    # evaluate a run started with `start`
.agents/run publish <run-id>                  # push run/<id>, open or update its draft PR
```

Every run ends by pushing `run/<id>` and opening a draft PR, if `GH_TOKEN` is set. The
PR description is the executor's `pr.md`, written as history for future readers (what
changes and why, usage, testing, limits), never as a run report; run details stay in
`runs/<id>/`. Before claiming, the executor runs `.agents/run ci`, which pushes the run
branch and waits for CI, including the macOS, Windows and minimum-version jobs that no
local check reproduces. If publishing fails, the run still completes; fix the token and
use `publish`.

If the evaluator produces no verdict, it's retried once at lower effort; if it still
fails, the review sheet says so and the PR can't be marked ready.

### Monitoring an `auto` run

`auto` runs doctor, then starts a background container named `mai-<run-id>` and returns.

- `.agents/run status` lists runs that need attention, most urgent first:
  `READY:review` (finished, waiting for you), `working` (container up), `dead` (an
  unfinished step with no container; clean up with `stop`). STEP is the pipeline position:
  1/5 executor, 2/5 evaluating, 3/5 post-mortem, 4/5 publishing, 5/5 needs-review. A run
  is done only at READY; an executor saying "done" in its log means step 1 finished.
  `--all` adds finished runs. `.agents/run status <run-id>` shows the pipeline with time
  per step, failed agents, artifacts so far, and the log of the *current* step. All of it
  reads files only, so it works on the host. `watch -n 15 .agents/run status` gives a
  live view.
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

## Running in parallel, alongside your own work

Several runs can go at once, and you can keep committing, branching, pushing and
rebasing in your checkout. Each run has its own id, branch, worktree, container and run
directory, and starts from the default branch on GitHub, not from your checkout. Tested
with three concurrent runs of four parallel agents each while another process committed,
pushed and ran `git gc` in a loop.

What the kit does not do is coordinate *content* between runs. Two runs (or a run and
you) can edit the same lines; that surfaces when the second PR is merged. Each executor
rebases onto the default branch before claiming, so a run that finishes after another's
PR is merged absorbs it. The dangerous case is two PRs that merge cleanly as text but
break together. Protect `main` with required CI and "require branches to be up to date
before merging" (or a GitHub merge queue), so every merge is tested against current
`main`.

Two lead cycles at once can groom the same issues and pick the same work. Run one lead
cycle at a time; parallel runs should have separate directives.

## After a run

Work is delegated; responsibility isn't. A finished run ends in phase `needs-review`
with a review sheet, `runs/<id>/review.md`, and **the next run won't start until it's
reviewed**. The sheet asks for decisions, not reading:

| Section | Source | What you do |
| --- | --- | --- |
| What changed | the evaluator | Read: the ≤5 most consequential changes, broad or hard-to-undo ones first, with how to undo them. |
| Did it work | evaluation.md, report.md | Read: verdict, north-star number, spend in tokens, PR link. |
| Your checks | the evaluator | ≤3 judgment checks (≤2 min each): `pass`, `fail: <what you saw>` or `unsure: <why>`. Any fail blocks `ready`. Mechanical checks are the evaluator's job, never yours. |
| Your decisions | the post-mortem | ≤3 decisions only you can make, each with a recommendation: `ok` takes it. |
| The PR | | `ready`, `rework: <what>`, or `close`. |

Everything else (the executor's own reversible choices, the full proposals and
escalations) is listed at the bottom as reference, not review.

Write a decision on every `Decision:` line, then:

```bash
.agents/run review <id>                     # lists what's still open, or records the review
.agents/run review <id> --skip "<reason>"   # skip on the record; the reason stays in review.md
```

Then apply what you accepted (INTENT.md Decisions, executor.md edits), commit, and push.

### How the work gets submitted

Every run's work is already on GitHub as a draft PR from `run/<id>` to the default
branch; publishing happens at the end of the run, before review. Your PR decision
decides what happens to it:

| Decision | What `.agents/run review` does | Then |
| --- | --- | --- |
| `ready` | Takes the PR out of draft. | `.agents/run merge <id>`, or merge on GitHub. Merge squashes, so the PR description becomes the commit message. |
| `rework: <what>` | Prints the rework command. | `.agents/run rework <id> <slug>`: a new unattended run that starts *from* `run/<id>`, with your review as its brief. Its PR closes the old one as superseded. |
| `close` | Closes the PR. | Nothing; the branch stays for reference. |

To fix something small yourself instead: `git switch run/<id>`, commit, `git push`.
After publishing, `run/<id>` tracks `origin/run/<id>`, so plain `git push` and
`git pull` work. Run branches start without tracking on purpose: they're created from
`origin/<default>`, and tracking that would make `git pull` on the run branch pull
`main` into it.
From your coding session, the mai-runs skill walks you through the sheet.

Background, if a decision needs it: `report.md`, `evaluation.md`, `escalations.md`,
`proposals.md`, `triage.md`, `scope.md`, `claim.md`, `spawns.jsonl`, `ledger.md`,
`agents/*.log`, and `executor.log`/`postmortem.log` for `auto` runs.

## Settings

All settings live in `.agents/config.sh`. Precedence is environment, then
`.agents/config.local.sh` (gitignored, yours only), then the defaults:

```bash
CHEAP_MODEL=some-model .agents/run auto <slug> "<directive>"     # one run
echo ': "${CHEAP_EFFORT:=minimal}"' >> .agents/config.local.sh   # your machine
```

Each run records the values it actually used in `runs/<id>/config.txt`, so a result can
always be traced to its models and settings.

**Commit attribution.** `MAI_GIT_NAME` and `MAI_GIT_EMAIL` are patterns with `%role%`
and `%id%`, e.g. `agents+%id%@agents.maida.ai`. They make history readable, but they're
unauthenticated metadata: anyone can write any email into a commit, so a domain alone
grants nothing on GitHub. What GitHub actually authorizes is the token that pushes. For
real attribution and permissions: push with a bot identity (a machine user or GitHub
App) instead of your personal token, so agent PRs and issue edits stop appearing as
yours; and sign agent commits with that identity's key, so "Verified" means something
and a ruleset can require it. GitHub links a commit to an account only by an email
verified on that account, so per-agent addresses won't link; keep the per-agent detail
in the name and use one verified address if you want commits linked to the bot.

## Adjusting the system

| Symptom | Change |
| --- | --- |
| Worked on the wrong thing; misread what you want | `INTENT.md`: add the missing priority, non-goal or ask-first rule |
| Right goal, bad process (scope creep, weak verification, didn't delegate) | `executor.md`, ideally from `proposals.md` |
| Sub-agents misbehave the same way repeatedly | `.agents/agent-rules.md` |
| A project convention you want everyone (you included) to follow | `AGENTS.md` |
| Too expensive, or the cheap tier keeps failing | `config.sh`: change a tier's model or effort |
| Evaluator too lenient | `evaluator.md`; `EVAL_MODEL` in `config.sh`, ideally another vendor (`run_evaluator` in `tiers.sh`) |
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

After a crash or a bare `docker stop`, clean up that run only:

```bash
.agents/run stop <run-id>     # safe even if it's no longer running; touches only that run
```

Don't remove everything under `.worktrees/` while other runs are active: their agents
are working in there.

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

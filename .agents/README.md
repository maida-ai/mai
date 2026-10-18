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
| `AGENTS.md` | you | Shared facts and conduct for every agent, plus the brief template. |
| `CLAUDE.md` | you | `@AGENTS.md`, so Claude Code reads the same rules. |
| `.agents/executor.md` | you | The tech-lead role. Changes only through proposals you accept. |
| `.agents/evaluator.md` | you | The independent evaluator role. |
| `.agents/tiers.sh` | you | Which harness and model run each tier, the executor and the evaluator. |
| `.agents/Dockerfile` | you | The container: Rust, git ≥ 2.40, gh, Node, Codex. |
| `.agents/run` | you | Entry point: `doctor`, `start`, `auto`, `eval`. |
| `.agents/spawn` | you | The only way agents start sub-agents. Logs every spawn and enforces depth. |
| `.agents/pre-commit` | you | Hook that blocks agent commits to the files in this table. |

Agents can't commit changes to any of these. The hook enforces that, and doctor checks
the hook is installed.

## One-time setup

1. Docker usable without sudo (rootless recommended). Codex logged in on the host
   (`~/.codex/auth.json`) or `OPENAI_API_KEY` exported.
2. Optional, for issue grooming and PRs: export `GH_TOKEN`, a fine-grained token limited
   to this repo with Issues, Pull requests and Contents set to read/write. Protect `main`
   on GitHub so the token can't push to it.
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
.agents/run eval  <run-id>                    # evaluate a run started with `start`
```

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
- `docker stop mai-<run-id>` stops it. A stopped run doesn't clean up: remove its
  worktrees with the command in Cleanup.

There's no steering an `auto` run mid-flight; that's what `start` is for.

`start` and `auto` run doctor first and refuse to start if anything fails. Every run
gets its id (`YYYY-MM-DD-<slug>`), a directory under `runs/`, and its own branch,
`run/<id>`, in its own worktree. Your checkout is never touched.

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

## After a run

Read these in order, all under `runs/<id>/`:

1. `report.md`: outcome against acceptance, spend, open escalations, and backlog changes
   to issues you opened.
2. `evaluation.md`: the independent verdict.
3. `proposals.md`: the executor's suggested edits to its own instructions.
4. `escalations.md`: decisions it deferred or made without you (unattended runs).

Then:
- **Ship or drop the work.** Review the PR, or `git diff main...run/<id>`. Merge into main yourself.
- **Answer escalations in INTENT.md.** Each answer becomes a line there, so the question doesn't come back.
- **Apply proposals you agree with.** Edit `executor.md` and commit it yourself.

Other artifacts you may want: `triage.md`, `scope.md`, `claim.md`, `spawns.jsonl` (who
ran, at which tier, with what status), `ledger.md`, `agents/*.log`, and
`executor.log`/`postmortem.log` for `auto` runs.

## Adjusting the system

| Symptom | Change |
| --- | --- |
| Worked on the wrong thing; misread what you want | `INTENT.md`: add the missing priority, non-goal or ask-first rule |
| Right goal, bad process (scope creep, weak verification, didn't delegate) | `executor.md`, ideally from `proposals.md` |
| Sub-agents misbehave the same way repeatedly | `AGENTS.md` conduct section |
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
At the end of a run, agent branches already merged into `run/<id>` are deleted. What's
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

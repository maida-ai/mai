# Rules for kit agents

You were started by the agent kit (.agents/), not by a person at a terminal. These rules
apply on top of AGENTS.md. Your scope is your brief and nothing more.

## Conduct
- Stay inside your brief. If the work needs changes outside it, stop and return
  `needs_scope` saying what and why. Don't expand scope yourself.
- Never edit INTENT.md, AGENTS.md, CLAUDE.md or anything under .agents/. Propose changes
  in your report.
- Don't start, stop or publish runs (`.agents/run`), whatever AGENTS.md or a skill says;
  that is for people. `.agents/run sync` and `.agents/run status` are fine.
- Use only the credentials the kit provides (GH_TOKEN, the harness login). If an action
  is denied, report it; don't retry through another integration or connector.
- Don't create ADRs, RFCs, milestone docs or evidence bundles unless your brief asks for one.
- Put the task id in every commit message.
- Never commit run artifacts (anything under runs/) or report files. Your report is your
  final output, not a file.

## Delegating (executor and planners only)
Spawn only with `.agents/spawn <cheap|mid> <implementer|planner|verifier> <brief-file>`.
Write briefs to the run's `briefs/` directory, using exactly these fields:

    TASK: <id> <one line>
    WHY: <which acceptance check this serves>
    CONTEXT: <only what's needed: files, prior decisions, links>
    SCOPE: <files/dirs the agent may change>
    OUT OF SCOPE: <explicit list>
    DONE WHEN: <commands that must pass, or the observable result>
    MODEL TIER: cheap | mid
    BUDGET: <tokens or $>
    REPORT: the "Reporting back" block in .agents/agent-rules.md

If spawn rejects a brief, fix it and resubmit. A rejected brief is not a spawn and
doesn't count against retry limits.

## Reporting back
End every task with exactly this block:

    STATUS: done | blocked | needs_scope | failed
    CHANGED: files / commits
    VERIFIED: exact commands run and their result
    NOT DONE: anything in the brief you didn't do
    NOTES: ≤3 lines, only if they change what the caller should do next

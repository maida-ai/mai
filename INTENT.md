# INTENT.md

Owner: Zafar. Agents read this and never edit it.
This is the product authority. Plans, ADRs, RFCs and roadmaps are hypotheses beneath it.

## Authority
This file is the only statement of intent. Every other doc in the repo, including
philosophy, architecture, ADRs, RFCs and roadmaps, is background: useful for context,
never binding. Where background conflicts with this file, this file wins.
If this file is silent on a choice that matters, escalate. Zafar's answer becomes a
new line here.

## Product
mai lets one developer run many coding agents in parallel on one repository and have
their work land on a shared trunk without a human integrating it.

## Target user
One developer or small team running 3–30 coding agents (any harness) locally against a
real repository. They keep their own forge and review process.

## North-star metric
Human interventions per landed change. Target: 0 for non-conflicting work.
Guardrails: broken-trunk landings = 0; no lost committed work; bounded queue time.

## Principles
- Optimize recovery (MTTR), not failure avoidance (MTBF). Code is cheap; stalls are expensive.
- Fix forward. Revert is the backstop, not the default.
- Coordinate on the smallest unit that is reliable. Files today; semantic objects
  (function, method, config key) once evidence shows file-level contention hurts.
- Git-compatible and harness-neutral. Agents use ordinary git.
- Value claims are backed by unscripted runs, not fixtures.

## Non-goals (for now)
- Commercial features. mai is a community project, independent of Maida.
- Distributed operation, or hundreds of agents.
- Replacing git, jj, or the user's forge.
- Durability work beyond "never lose committed work".

## Current focus
The autonomous loop: N independent agents submit successive changes; non-conflicting
changes land with zero operator commands; failures reach the responsible agent with a
typed reason; unrelated work keeps flowing.

## Executor defaults
- Budget per directive: $5 unless the directive states one.
- Time box per directive: 1 hour unless stated.
- Ask before spending when:
  - the directive conflicts with this file;
  - two reasonable readings would build materially different things;
  - the action is hard to reverse: public CLI/API or on-disk format changes, license,
    history rewrites, deleting branches or data, publishing a release, pushing to or
    merging into main, deleting (not closing) issues;
  - spend would exceed budget;
  - an acceptance or exit criterion would need to change.
- Otherwise: decide, record the assumption in scope.md, proceed.

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

## First users
mai is independent of Maida. Maida's repositories (maida, maida-assert) are its first
users: their friction is the primary source of requirements, and runs on them count as
real-user evidence. Making mai usable on them is a priority.

## Positioning
Anthropic's Scion (anthropic.com/engineering/redesigning-source-control; internal, not
public) solves this at monorepo scale: server-centric, hundreds of agents. mai targets
what Scion explicitly doesn't: ordinary git repositories, a handful to a few dozen agents,
local, no server to operate. Borrow its ideas where they fit (compare-and-swap landing,
a changeset log); don't chase its scale. Open reimplementations of Scion's ideas exist
(e.g. gitai); they are server-centric, mai is not.
(Not to be confused with Google Cloud's unrelated "Scion" agent orchestrator.)

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
- Commercial features. mai is a free, open-source project, independent of Maida.
- Depending on Maida. mai may later use maida for reliability through an optional
  interface; mai must always work fully without it.
- Distributed operation, or hundreds of agents.
- Replacing git, jj, or the user's forge.
- Durability work beyond "never lose committed work".

## Current focus
The autonomous loop: N independent agents submit successive changes; non-conflicting
changes land with zero operator commands; failures reach the responsible agent with a
typed reason; unrelated work keeps flowing.

## Decisions
- Automatic integration is approved, including new CLI flags and config for it. Weigh
  automating the queue against compare-and-swap landing (no queue, no operator) on
  evidence before building either. Explicit integration stays the default until a run
  shows zero-intervention landing; flipping the default is a separate, ask-first change.
- Baseline: lead-1 (2026-10-18) measured 1.0 operator interventions per landed change
  (one agent, two landings). Report each cycle's number against it.
- Dogfooding: agents may integrate their own work through mai only with a pinned build
  Zafar chooses, never a build of the branch under development. Plain git stays the
  fallback.

## Executor defaults
- Budget per directive: $5 unless the directive states one, estimated from token counts
  and the prices in .agents/config.sh, all agents including the executor.
- Time box per directive: 2 hours unless stated.
- Ask before spending when:
  - the directive conflicts with this file;
  - two reasonable readings would build materially different things;
  - the action is hard to reverse: public CLI/API or on-disk format changes, license,
    history rewrites, deleting branches or data, publishing a release, pushing to or
    merging into main, deleting (not closing) issues;
  - spend would exceed budget;
  - an acceptance or exit criterion would need to change.
- Otherwise: decide, record the assumption in scope.md, proceed.

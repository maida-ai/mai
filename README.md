<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/mai-symbol-dark.svg">
    <img src="docs/assets/mai-symbol-light.svg" width="96" height="85" alt="mai: curly braces around a coral dot">
  </picture>
</p>

<h1 align="center">mai</h1>

<p align="center"><strong>Coordinate local coding agents. One history. Fewer integration chores.</strong></p>

<p align="center">
  <a href="docs/getting-started.md">Get started</a> ·
  <a href="docs/README.md">Docs</a> ·
  <a href="CONTRIBUTING.md">Contribute</a>
</p>

mai aims to shorten recovery when agents fail, conflict or lose their environment, while keeping a blocking gate at the shared trunk boundary. The proposed rewrite uses plain Git submissions and an integrated validation queue so agents spend less time on rebases and humans spend less time coordinating parallel changes.

**MVP foundations under development.** Scoped design acceptance and native S3 functional verification permit replacement under [ADR 0010](docs/adr/0010-staged-mvp-qualification.md). The old worktree engine has been retired. The foundation now provides a versioned lifecycle model, SQLite persistence, safe authority initialization, a local single-writer engine, manual trusted-gate validation and independent agent workspaces. The authority commands are described in the [engine guide](docs/engine.md); see [manual validation](docs/manual-validator.md) and [workspaces](docs/workspaces.md) for the new explicit operations. The queue flow below is planned, not shipped. Built in Rust, backed by Git, open source under [Apache 2.0](LICENSE), and independent of core Maida.

## Two agents. One integration queue.

Planned submission flow after creating Alice's and Bob's clones with the implemented `mai workspace` command:

```sh
# In Alice's clone:
printf 'Hello UI\n' > ui.txt
git add ui.txt
git commit -m 'Add UI'
git push mai HEAD:refs/for/main

# In Bob's clone:
printf 'Hello API\n' > api.txt
git add api.txt
git commit -m 'Add API'
git push mai HEAD:refs/for/main
```

Push success will acknowledge durable registration, not landing. Each agent waits for its change's typed outcome with `mai wait <change>` and runs `mai sync` after landing; the engine combines and checks candidates without requiring a pre-submission rebase. A conflict or failed check returns an actionable outcome and a fetchable candidate. Pipeline mode must validate each landed tree exactly as landed; optional batch mode must label its weaker intermediate-revision evidence explicitly.

The intended audience is one developer or team running 3–30 agents across harnesses. Humans keep their forge and review process; mai integrates agent changes into an export branch. Recovery times, human interventions, time to land and broken-trunk rate will measure whether this design earns its place. None of those improvements has been measured for the rewrite yet. See the [benchmark protocol](docs/benchmarking.md) for comparable workloads and recovery measurements. The longer-term ambition is hundreds or thousands of overlapping agents; that scale and distributed operation are outside the qualified local MVP.

## Build the MVP foundation

Use Rust 1.85+, Git 2.40+ and a C compiler for bundled SQLite:

```sh
git clone https://github.com/maida-ai/mai.git
cd mai
cargo build --locked --bin mai
cargo test --all-targets --locked
cargo run --locked -- --version
```

See [getting started](docs/getting-started.md). Submission and publication are not enabled in this layer; manual validation requires explicit trusted policy and does not land changes; no legacy worktree commands or format migrations are provided. Performance targets and differentiated priority remain deferred until practical MVP observations.

## Where mai fits

| Tool | Documented capability |
| --- | --- |
| [Git worktrees](https://git-scm.com/docs/git-worktree) | Multiple working trees share repository objects and refs. |
| [Jujutsu](https://docs.jj-vcs.dev/latest/conflicts/) | Records conflicts as first-class state; an [operation log](https://docs.jj-vcs.dev/latest/operation-log/) supports inspecting and undoing operations. |
| [GitButler](https://docs.gitbutler.com/ai-agents/parallel-agents) | Documents parallel-agent workflows using multiple branches in a shared workspace. |
| [GitHub merge queues](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue) | Run required checks on changes combined with the target branch and earlier queued work. |

mai's proposed focus is a local, harness-neutral validation queue with typed outcomes, dependent changes, engine-computed change manifests, priority-aware coordination and bounded recovery. M1 starts with file units and a simple single-class dependency-aware queue; M2 adds object precision. Differentiated priority and automated recovery remain deferred proposals under ADR 0010 and RFC #93. Priority never bypasses the trunk gate, and a confirmed post-landing failure gets a bounded fix-forward lane with a validated revert backstop. This comparison is not a claim that other tools lack integration or agent support. The final tagline remains a maintainer decision.

## Keep going

[Philosophy](docs/philosophy.md) · [Architecture](docs/architecture.md) · [Roadmap](docs/project.md) · [Agent protocol](docs/agents.md) · [Configuration](docs/configuration.md) · [Operations](docs/operations.md) · [Contribute](CONTRIBUTING.md)

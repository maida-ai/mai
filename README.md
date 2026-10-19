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

mai provides a local Git submission workflow with gated, serial integration for coding agents working in parallel. Receive can opt into automatic integration; explicit integration remains the default. Each agent works in an independent clone and submits ordinary Git commits. In the default mode, an operator selects each generation for integration through a blocking validation gate.

**Available now: a trusted-local evaluation workflow.** Push a change, inspect its durable receipt, then select it for gated integration, or opt into integration on receipt. Dependent changes map onto accepted squashes without requiring an agent rebase first. True conflicts still require a correction. Use disposable repositories during evaluation.

[Try the step-by-step walkthrough](docs/getting-started.md) to land a parent and child through a small gate, then [see a failed check and correction](docs/check-failure.md). [Current delivery and limits](docs/release-status.md) separates implemented behavior from the remaining MVP.

Built in Rust, backed by Git, self-hostable and open source under [Apache 2.0](LICENSE). mai is independent of core Maida. The old worktree engine is retired; [ADR 0010](docs/adr/0010-staged-mvp-qualification.md) records the accepted replacement scope.

## Receive, inspect, integrate

Create an authority with explicit trusted gate policy and independent workspaces, then enable receive with explicit limits. Each push registers one immutable generation. By default, an operator selects each generation for serial integration, parent before dependent child:

```sh
# In an agent workspace, after committing:
git push -o agent=alice mai HEAD:refs/for/main
# At the authority, using the change ID returned by push:
mai show PROJECT CHANGE --generation 1 --json
mai integrate PROJECT CHANGE --mode textual --wait-seconds 60 --json
```

To attempt gated textual integration on each receipt, enable receive with `--auto-integrate`:

```sh
mai receive enable PROJECT --max-input-bytes 10485760 --max-file-bytes 1048576 --max-changed-files 100 --auto-integrate
```

The [runnable walkthrough](docs/getting-started.md) builds an authority, submits a parent and child, integrates both through the trusted gate and verifies a fresh clone. [Receive and integration](docs/receive.md) documents exact retry identity, admission limits, leases and uncertain responses. File manifests come from actual diffs; landing requires the checked tree and matching parent/policy evidence. Textual integration currently rejects attributes/custom merge drivers. [Copy-based continuation](docs/continuation.md) replays a clean unregistered suffix after an exact landed identity into a new workspace, preserving the original clone. In-place sync/autostash remains deferred.

The intended audience is one developer or team running 3–30 agents across harnesses. Humans keep their forge and review process; mai integrates agent changes into an export branch. Recovery times, human interventions, time to land and broken-trunk rate will measure whether this design earns its place. Small reproducible functional measurements now exist, including [explicit Linux validator recovery](docs/spikes/validator-recovery/2026-10-17/README.md); they do not establish improvements over an equivalent workflow, production MTTR or representative throughput. See the [benchmark protocol](docs/benchmarking.md) for comparable workloads and recovery measurements. The longer-term ambition is hundreds or thousands of overlapping agents; that scale and distributed operation are outside the qualified local MVP.

Opted-in Linux validators can use [explicit proof-backed recovery](docs/validator-recovery.md) after engine death. Missing cleanup proof, worker death and host failure remain preserved blockers; recovery never skips the normal gate.

## Build the MVP foundation

Use Rust 1.85+, Git 2.40+ and a C compiler for bundled SQLite:

```sh
git clone https://github.com/maida-ai/mai.git
cd mai
cargo build --locked --bin mai
cargo test --all-targets --locked
cargo run --locked -- --version
```

See [getting started](docs/getting-started.md). Submission requires explicit enablement and integration requires an explicit command by default; manual validation requires trusted policy and does not land changes; no legacy worktree commands or format migrations are provided. Performance targets and differentiated priority remain deferred until practical MVP observations.

## Where mai fits

| Tool | Documented capability |
| --- | --- |
| [Git worktrees](https://git-scm.com/docs/git-worktree) | Multiple working trees share repository objects and refs. |
| [Jujutsu](https://docs.jj-vcs.dev/latest/conflicts/) | Records conflicts as first-class state; an [operation log](https://docs.jj-vcs.dev/latest/operation-log/) supports inspecting and undoing operations. |
| [GitButler](https://docs.gitbutler.com/ai-agents/parallel-agents) | Documents parallel-agent workflows using multiple branches in a shared workspace. |
| [GitHub merge queues](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue) | Run required checks on changes combined with the target branch and earlier queued work. |

mai's proposed focus is a local, harness-neutral validation queue with typed outcomes, dependent changes, engine-computed change manifests, priority-aware coordination and bounded recovery. M1 starts with file units and a simple single-class dependency-aware queue. The intended finer locking unit is a semantic object, such as a function, method, variable or configuration key. Current manifests and leases use file units; semantic-object locking and stable object identity are not shipped. M2 is the planned object-precision stage, and robust object-key identification remains future work. Differentiated priority and automated recovery remain deferred proposals under ADR 0010 and RFC #93. Priority never bypasses the trunk gate, and a confirmed post-landing failure gets a bounded fix-forward lane with a validated revert backstop. This comparison is not a claim that other tools lack integration or agent support. The final tagline remains a maintainer decision.

## Keep going

[Philosophy](docs/philosophy.md) · [Architecture](docs/architecture.md) · [Roadmap](docs/project.md) · [Agent protocol](docs/agents.md) · [Configuration](docs/configuration.md) · [Operations](docs/operations.md) · [Contribute](CONTRIBUTING.md)

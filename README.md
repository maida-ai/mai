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

`mai` coordinates local coding agents through path leases, separate workspaces and one ordered acceptance history. Its planned direction is shared symbol-level work units, declared intent and resumable context bound to exact code state; those features are not implemented yet. Built in Rust, backed by Git, and open source under [Apache 2.0](LICENSE). No GitHub account or hosted service required.

## Two agents. One history.

Run this in a scratch directory:

```sh
mai init demo
mai workspace demo alice
mai workspace demo bob

echo 'Hello UI' > demo/workspaces/alice/ui.txt
echo 'Hello API' > demo/workspaces/bob/api.txt

mai submit demo alice -m 'Add UI'    # r1
mai submit demo bob -m 'Add API'     # r2: both files, no manual rebase
```

Both agents started at `r0`. Both changes land. If they edit overlapping paths, `mai` rejects the submission and preserves the draft.

- **Independent work:** isolated workspaces, without managing Git branches.
- **Predictable integration:** numbered revisions, explicit conflicts, optional path leases.
- **Easy automation:** JSON responses and agent attribution on every accepted revision.

## Try it

You need Rust 1.85+, Git 2.36+, and a C compiler to build. The installed binary needs no Python runtime.

```sh
git clone https://github.com/maida-ai/mai.git
cd mai
cargo install --path . --locked --bin mai
```

Want a guided demo? Run `bash examples/demo.sh`, or `./examples/demo.ps1` in Windows PowerShell. [Installation details and your first submission →](docs/getting-started.md)

## Where mai fits

Today, mai provides local path leases, ordered revisions, attribution and recovery receipts. Exact integrated-tree validation is the immediate priority; symbol-aware integration, live intent, context resume and draft DAGs are planned differentiation, not shipped advantages. Final positioning wording remains under maintainer review.

| Tool | Documented capability |
| --- | --- |
| [Git worktrees](https://git-scm.com/docs/git-worktree) | Multiple working trees share repository objects and refs. |
| [Jujutsu](https://docs.jj-vcs.dev/latest/conflicts/) | Records conflicts as first-class state; its [operation log](https://docs.jj-vcs.dev/latest/operation-log/) supports inspecting and undoing repository operations. |
| [GitButler](https://docs.gitbutler.com/ai-agents/parallel-agents) | Supports parallel agents with multiple branches in a shared workspace, with a documented tradeoff against separate worktrees. |
| [GitHub merge queues](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue) | Run required checks on changes combined with the target branch and earlier queued work. |

These tools already solve useful parts of the workflow. mai's [planned direction](docs/agent-native-capabilities.md) must earn its place through less late-conflict waste and successful recovery, not feature-count or throughput claims.

## Keep going

[Connect an agent](examples/agent-prompt.md) · [CLI reference](docs/cli.md) · [Import a Git project](docs/getting-started.md#import-an-existing-project) · [How it works](docs/architecture.md)

**MVP · 0.0.1.** Use disposable, trusted local repositories while evaluating. Workspaces are not sandboxes. Local Git commits, dirty updates, and integrated-tree validation are not supported yet; large bursts can hit contention limits. [Current limits and roadmap →](docs/project.md)

Part of the **maida-ai** family. Useful on its own; GitHub mirroring is optional. [Report an issue](https://github.com/maida-ai/mai/issues) or [help build it](CONTRIBUTING.md).

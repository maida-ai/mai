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

`mai` gives each agent its own workspace and brings independent changes into one ordered history. Built in Rust, backed by Git, and open source under [Apache 2.0](LICENSE). No GitHub account or hosted service required.

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

## Keep going

[Connect an agent](examples/agent-prompt.md) · [CLI reference](docs/cli.md) · [Import a Git project](docs/getting-started.md#import-an-existing-project) · [How it works](docs/architecture.md)

**MVP · 0.0.1.** Use disposable, trusted local repositories while evaluating. Workspaces are not sandboxes. Local Git commits, dirty updates, and integrated-tree validation are not supported yet; large bursts can hit contention limits. [Current limits and roadmap →](docs/project.md)

Part of the **maida-ai** family. Useful on its own; GitHub mirroring is optional. [Report an issue](https://github.com/maida-ai/mai/issues) or [help build it](CONTRIBUTING.md).

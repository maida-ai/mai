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

mai is a local Git workflow for submitting and validating changes from independent coding-agent workspaces. An operator selects each generation for integration; a successful submission is not a landed change. mai belongs to the maida-ai family and runs independently of Maida infrastructure. Installation and initialization do not move Maida assets.

```sh
cargo install --path .
mai init demo --from ./my-project
mai workspace demo alice ./alice
```

See [getting started](docs/getting-started.md) for the complete submission and integration walkthrough, and [current delivery and limits](docs/release-status.md) for supported behavior and known gaps.

Built in Rust and backed by Git, mai is open source under [Apache 2.0](LICENSE).

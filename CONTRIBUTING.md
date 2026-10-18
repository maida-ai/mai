# Contributing

`mai` is an Apache-2.0 Rust project in the maida-ai family, independent of Maida infrastructure and proprietary services. Installation and initialization do not move Maida assets. Start with [Architecture](docs/architecture.md) and [AGENTS.md](AGENTS.md).

## Local workflow

Install Rust 1.85+, a C compiler for bundled SQLite, and Git 2.40+.

```sh
cargo build --locked --bin mai
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
cargo build --release --locked --bins
```

Real-Git tests use disposable repositories and local bare remotes. Add success and error coverage for current behavior. Changes to persistence, initialization, receive or landing need interruption and recovery coverage appropriate to the path; simulation alone does not establish power-loss durability.

## Security and evidence

Do not deploy this trusted-local product to hostile tenants. Agent names are declared identities, leases are not OS permissions, and configured checks execute repository code without sandboxing. Never force-push customer remotes, migrate Maida assets, deploy or publish packages without explicit authorization. Report vulnerabilities through [GitHub private reporting](https://github.com/maida-ai/mai/security/advisories/new).

Record platform, tool versions and reproducible results for experiments. Keep historical evidence distinct from claims about current functionality. Do not claim performance or recovery guarantees that measurements have not established.

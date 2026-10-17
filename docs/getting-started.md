# Build the MVP foundation

The old worktree prototype has been retired after scoped design acceptance. The foundation implements a versioned change lifecycle, SQLite persistence, repository validation, authority initialization and local engine serving. Receive and landing remain subsequent work; this is not yet a usable integration queue. Use disposable evaluation repositories throughout MVP development.

## Build and verify

Install Rust 1.85+, Git 2.40+ and a C compiler for bundled SQLite. On native Windows, use Git for Windows and the MSVC build tools. No Python runtime or standalone SQLite installation is required.

```sh
git clone https://github.com/maida-ai/mai.git
cd mai
cargo build --locked --bin mai
cargo test --all-targets --locked
cargo run --locked -- --help
cargo run --locked -- --version
```

The version remains `mai 0.0.1`. The current binary exposes initialization and engine administration; see the [engine guide](engine.md) for `init`, `serve`, `status` and `stop`. Retired commands such as `refresh`, `submit` and `github-sync` reject; there are no legacy aliases or data migrations. Old demos and benchmarks have been removed with the old engine, while experiment harnesses and reviewed evidence remain available.

## What is qualified here

Lifecycle tests cover immutable submission generations, retry deduplication, dependency blocking and rejection, protected-change approval states, explicit validation attempts, terminal outcomes and later annotations. A serialized snapshot round trip tests the model's restart contract; it does not prove durable receive acknowledgment or Git landing. Persistence and startup reconciliation now have interruption tests; receive and landing still require their own integration tests before enablement.

CI runs Rust formatting, Clippy, tests and release builds on Linux, macOS and Windows. A separate bounded Linux job builds the checksum-pinned Git 2.40.0 release and uses Rust 1.85.0 with the locked dependency graph. This minimum combination is not a Windows or macOS qualification. A local Linux pass does not establish hosted native results. See [platform qualification](platform-qualification.md) for capability probes, fallback semantics and evidence limits. See [CLI reference](cli.md), [architecture](architecture.md) and [roadmap](project.md) for the staged delivery plan.

[All docs →](README.md)

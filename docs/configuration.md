# Configuration — proposed rewrite

**Not implemented.** The 0.0.1 binary does not read this configuration. The following is the complete proposed repository configuration surface; values are examples and recommendations, not accepted defaults. M0 and the [maintainer decisions](project.md#open-maintainer-decisions) must settle policy before M1.

```toml
version = 1

[check]
gate = "cargo fmt --check && cargo test --workspace --locked"
full = "cargo test --workspace --all-features -- --include-ignored"
timeout = "15m"
retries = 1
mode = "pipeline"
on_full_failure = "revert"
affected = ""

[validators]
slots = 2
bootstrap = "cargo fetch --locked"
bootstrap_inputs = ["Cargo.lock", "rust-toolchain.toml"]

[workspaces]
bootstrap = "cargo fetch --locked"
template = true

[merge]
default = "textual"

[[merge.rule]]
paths = ["Cargo.lock"]
strategy = "regenerate"
command = "cargo generate-lockfile --offline"

[limits]
max_file_bytes = 5_000_000
max_changed_files = 500
reject_paths = ["node_modules/**", "**/.env"]

[policy]
protected = ["mai.toml", ".github/**", "AGENTS.md", "scripts/check*"]

[upstream]
remote = "origin"
branch = "main"
sync = "5m"
export = "branch"
export_branch = "mai/trunk"
```

## Repository settings

| Setting | Proposed meaning |
| --- | --- |
| `version` | Configuration schema version; unsupported versions reject |
| `check.gate` | Blocking command on the exact integrated candidate; configuration selected from base trunk |
| `check.full` | Optional asynchronous post-landing suite, introduced in M1b |
| `check.timeout` | Check time limit; runner terminates the process group and caps captured output |
| `check.retries` | Retry failures on the same candidate; record pass-after-fail as flaky, never land without a pass |
| `check.mode` | `pipeline` checks each speculative prefix; `batch` checks a combined tip, bisects failure and labels intermediate evidence distinctly |
| `check.on_full_failure` | `revert`, `freeze` or explicit `continue`; recommended revert requires confirmed nonflaky culprit and a gate-passing compensation |
| `check.affected` | Optional command receiving changed paths and returning a narrowed gate invocation; exact I/O contract needs implementation specification |
| `validators.slots` | Bounded concurrent warm validator clones; final default comes from S4 |
| `validators.bootstrap` | Environment setup for a slot, outside integration/landing |
| `validators.bootstrap_inputs` | Trunk-configured paths whose changes require slot re-bootstrap |
| `workspaces.bootstrap` | Setup for an explicitly created agent clone |
| `workspaces.template` | Optional copy-on-write warmed template with qualified filesystem behavior and ordinary-copy fallback |
| `merge.default` | Proposed `textual` with a gate; `path` without one; final gated default is open |
| `merge.rule.paths` | Paths selected by a merge rule; precedence and matching semantics must be specified and tested |
| `merge.rule.strategy` | `textual`, `regenerate`, `ours-trunk` or `reject` |
| `merge.rule.command` | For `regenerate`, start with trunk's file and run the configured generator in a validator before final tree validation |
| `limits.max_file_bytes` | Receive-time file-size bound |
| `limits.max_changed_files` | Receive-time changed-path bound |
| `limits.reject_paths` | Receive-time excluded paths; rejection does not establish secret scanning or erase transmitted objects |
| `policy.protected` | Changes requiring explicit `mai approve`, including configuration and check scripts |
| `upstream.remote` / `branch` | Forge input, distinct from export branch |
| `upstream.sync` | Fetch/sync interval; on-demand synchronization also planned |
| `upstream.export` | `branch` or `pr-batch`; per-change PR export is deferred |
| `upstream.export_branch` | Integration line exported for human review; never force-rewrite customer history |

The engine reads policy and check configuration from trunk, never from the unapproved candidate. Pin executable check, affected-test, bootstrap and merge-driver/regeneration policy to approved trunk inputs, rather than resolving trusted command strings to candidate-modified scripts. Protect scripts invoked by checks as well as `mai.toml`; an agent cannot approve its own weakened gate through its submission. Lockfile regeneration examples for Cargo, npm and uv require S6 qualification. Git merge-driver behavior is also subject to qualification, not assumed merely because the CLI performs the merge.

## Engine-local settings and isolation

Tokens, socket/pipe location and validator directories belong in `.mai/config.toml`, outside versioned source. Their exact keys, retention durations, draft-staleness threshold, batch size and resource-port allocation remain to be specified; no undocumented defaults are implied by the example.

Slots receive `MAI_SLOT`, `MAI_PORT_BASE` and a private temporary directory. Test suites must allocate ports, databases and temporary resources accordingly. mai does not provide a container or sandbox; bootstrap and checks execute trusted repository commands. Host `file://` identity is declared and best-effort; M1b loopback smart HTTP adds per-agent tokens.

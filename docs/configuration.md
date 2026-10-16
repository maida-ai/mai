# Configuration — proposed rewrite

[ADR 0010](adr/0010-staged-mvp-qualification.md) controls MVP scope. Priority, speculative/batch tuning and automated fix-forward examples below remain deferred proposals, not required or enabled initial MVP behavior. The initial queue is single-class and dependency-aware; core validation, ownership and recovery correctness remain mandatory. Priority RFC #93 and feature-specific tests precede future enablement.

**Mostly proposed.** Manual validation reads the strict version-1 subset documented in the [manual validator guide](manual-validator.md). The complete proposed configuration surface below includes unsupported fields and cannot be used directly as an enabled policy; values are examples and recommendations, not accepted defaults. Scoped M0 settles only policy needed for enabled MVP behavior; later settings await their own design acceptance.

```toml
version = 1

[check]
gate = "cargo fmt --check && cargo test --workspace --locked"
full = "cargo test --workspace --all-features -- --include-ignored"
timeout = "15m"
retries = 1
mode = "pipeline"
on_full_failure = "fix-forward"
fix_window = "15m" # Deferred example; later evidence and maintainer decision required
fix_attempts = 2
affected = ""

[priority]
grant = ["sev", "high"]
max_sev_in_flight = 2
grant_ttl = "2h"
lease_grace = "2m"
aging = "30m"

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
| `check.on_full_failure` | `fix-forward`, `revert`, `freeze` or explicit `continue`; recommended bounded fix-forward requires a confirmed nonflaky culprit, normal gate and full confirmation, then a validated revert backstop |
| `check.fix_window` | Bounded time for an attributed fix to restore trunk; S4 compares 5, 15 and 30 minutes with immediate revert before choosing the default |
| `check.fix_attempts` | Failed gate-attempt budget before reverting the culprit; the window still bounds recovery |
| `priority.grant` | Classes requiring audited, expiring grants; `sev` and `high`, while `normal` and `low` are self-assigned |
| `priority.max_sev_in_flight` | Cap concurrent severe changes to prevent priority inflation |
| `priority.grant_ttl` | Grant expiry; human versus designated-token grant authority remains open |
| `priority.lease_grace` | Notice before transfer to a higher-class requester; preserve preempted work and recheck ownership at landing |
| `priority.aging` | Waiting interval for one-class promotion, capped at `high`; qualify repeated promotion semantics and starvation behavior in S4 |
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

## Recovery and priority policy

These settings are proposed examples, not selected defaults. A confirmed full-tier failure creates an owned conflict record with logs, the culprit manifest and a temporary `sev` grant. A push with `mai.fixes=<change>` enters a bounded fix lane through the normal gate and must be confirmed by a full run within the same recovery deadline; a gate pass alone is not green trunk. Expiry or exhausted failed attempts triggers a validated compensating revert; a conflicting or failing revert freezes and notifies. Failed full confirmation does not mean recovery succeeded and cannot extend the deadline indefinitely. Flaky or uncertain culprit attribution does not authorize an automatic revert.

Priority orders `sev` > `high` > `normal` > `low`; `normal` is the proposed ordinary class. Class precedes arrival order. Only `sev` may cancel lower-class speculative validation for a slot; other classes do not preempt running validation. Dependencies inherit priority, waiting work ages no higher than `high`, and audited grants expire. Inheritance and aging are engine-derived effective-priority exceptions to requested-class grant requirements; their exact scope must be qualified in ADR 0004. Scope priority effects to queue order, lease transfer and conflict-resolver choice; it never approves changes, drops lower-priority work or bypasses checks.

`check.sev_gate` is an unresolved maintainer proposal and is deliberately absent from the example configuration. Until explicitly accepted and specified, severe fixes run the same normal gate. Any approved future alternative must pin its configuration to trunk, record the actual evidence and require full confirmation; do not label narrower evidence as the normal gate.

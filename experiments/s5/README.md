# S5 repeated forge cycles

This standalone, dependency-free Rust experiment qualifies a specific local Git topology for [#55](https://github.com/maida-ai/mai/issues/55). It does not call the current mai engine or implement its rewrite. See the [qualification report](../../docs/spikes/s5/README.md) for observed results and remaining acceptance work.

## Reproduce

Requirements: Git >=2.40, Rust with rustfmt, and Bash; on native Windows use Git for Windows/Git Bash with a native Windows Rust compiler and a local drive. The implementation uses Rust filesystem/process APIs, normalizes Windows Git path arguments, and invokes no Unix process utilities from Rust. Linux has been exercised; macOS and native Windows remain pending.

Run from the repository root, using new paths under `.throwaway/`:

```sh
bash experiments/s5/run.sh .throwaway/s5-new-run
bash experiments/s5/collect.sh .throwaway/s5-new-run .throwaway/s5-new-evidence
```

`S5_CONFLICT_OWNER` explicitly configures the fixture conflict owner; the default is `fixture-upstream-owner`, not a product default. The wrapper records it alongside tool versions, host information, source hashes, commands, raw Git output, assertion outcomes and per-command elapsed microseconds. These timings are diagnostics, not performance qualification. Existing fixtures, builds and evidence destinations are never overwritten. The collector uses an explicit filename allowlist; it excludes Git object stores, configurations, repositories, binaries and unrecognized log files. Failed runs retain available logs and exit nonzero.

## Topology and tested strategy

Each merge mode has an independent bare forge plus human, authority, agent and forge-merger clones. Human changes and merge operations push fast-forward updates to the fixture forge's `main`. The authority pushes only unique export branches to the forge and uses local CAS to land its own trunk. The fixture never contacts GitHub or force-pushes.

Each of three cycles lands an agent submission as a single-parent revision, exports one synthesized delta commit whose parent is the last successfully synced upstream and whose tree is the current trunk, then introduces an independent human edit before the forge merges. Squash mode runs `git merge --squash` followed by a commit; rebase mode runs `git rebase` on the single exported commit. This export strategy is part of the evidence: pushing raw trunk history or maintaining/reusing a single export branch is not qualified here.

Sync uses real `git merge-tree --write-tree` and creates a gated commit with old trunk first and the observed forge head second. Every accepted revision has a checked exact tree, exact parents and an assigned `Mai-Revision: rN` trailer. The harness independently reads final Git trees and the first-parent ledger, checks complete expected contents and changed path sets, and verifies the forge rewrites export identities without replaying old agent changes or causing spurious conflicts. The gate is a compiled synthetic exact-content oracle, not a project test suite or production validation system.

After the three cycles, a cleanly mergeable upstream candidate fails the synthetic gate. Both authority refs stay unchanged and independent local work lands while that bad upstream remains pending. A human then corrects their own branch with a new commit. Next, agent and human modify the same line differently: Git must report a genuine conflict with its path, and the harness records the configured owner. Further independent work lands on the old synced upstream base without modifying human main. Neither rejected attempt receives an acceptance revision.

## Manual CI

[experiment-s5.yml](../../.github/workflows/experiment-s5.yml) is manual-only and reuses the shared evidence runner: explicit `linux`, `macos`, `windows` or `all` platform input, a 15-minute per-job timeout and 30-day artifact retention. No workflow was dispatched. The dispatch workflow must first be registered on the default branch; after review and merge, manually select the desired branch and platform. A successful local topology run does not qualify GitHub APIs, branch protection or hosted platforms automatically.

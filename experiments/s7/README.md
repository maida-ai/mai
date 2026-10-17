# S7 crash protocol experiment

This isolated Rust harness uses real Git refs and CAS updates, SQLite WAL with FULL synchronous mode, an exclusive OS file lock, and native worker subprocesses. It is not the product engine. See the [qualification report](../../docs/spikes/s7/README.md) for limitations and pending acceptance work.

From the repository root:

```sh
bash experiments/s7/run.sh .throwaway/s7-new
bash experiments/s7/collect.sh .throwaway/s7-new .throwaway/s7-evidence-new
```

Use a new root and evidence destination every time. Run through Bash (Git Bash with native Rust on Windows). Dependencies are pinned in the standalone Cargo.lock; Cargo downloads, target artifacts, SQLite stores and disposable repositories remain under the chosen `.throwaway/` output root or its `.build` sibling. No production dependency changes are required. An initial download needs network access. The shared CI runner installs the Rust toolchain and bounds execution to 15 minutes with 30-day artifact retention. Dispatch only after the workflow is registered on default; no workflow is automatically triggered.

The parent waits for each selected worker checkpoint, verifies a second process cannot obtain its live lock, forcibly kills the worker using Rust's native `Child::kill` (SIGKILL on Unix, TerminateProcess on Windows), reaps it, then starts two fresh recovery workers. A stale lock file remains intentionally; only the OS lock controls exclusion. WAL transactions are killed before commit as well as after durable protocol stages. Workers do not spawn long-lived descendants. Restart clocks change from 10 to 20 to 50 in a synthetic logical clock; an independent parent receipt pins the original incident time, preventing restart from silently resetting the deadline.

The inspector reads the actual first-parent chain, exact submitted object/generation identity, retained refs, candidate/compensation trees and parents, database evidence and terminal records, and Git manifest notes. It independently recomputes file-level fallback manifests from Git diffs. The pass record binds candidate, tree, parent, trusted baseline tree and attempt. A real but deliberately tiny gate reads immutable candidate contents. Missing note/index records are rebuilt; a preexisting stale pass refuses publication. Corruption probes must fail inspection. Command logs preserve exact Git inputs, statuses and outputs; summary logs preserve checkpoint timings. Collection uses an explicit filename allowlist and excludes repositories, database files, Cargo caches and binaries.

Extend `POINTS` and the worker protocol together for new crash windows; add independent inspector expectations and deliberately corrupted negative cases. The logical clock and tiny fixture are synthetic test inputs, not production timeout or gate defaults. Grant/revoke/expiry, grace transfer, attempt preemption and dependency rejection/requeue currently test durable event-record persistence only. They do not yet implement operational scheduler effects, a real dependency candidate chain, validation cancellation, or concurrent full-confirmation/revert arbitration. Those gaps prevent S7 qualification.

The reviewed 2026-10-15 native CI passes all 26 implemented checkpoints on Linux, macOS and Windows. The [go/no-go assessment](../../docs/spikes/s7/go-no-go.md) retains NO-GO for the missing operational and competing-recovery cases.

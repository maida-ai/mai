# Performance engineering

Correctness, durable ordered history, and recoverability constrain every throughput optimization. Thousands of independent agents are a target; Rust 0.0.1 has not established production high-fanout capacity. Rust removes interpreter startup and creates a coherent native service foundation, but Git subprocesses, filesystem traversal, full worktrees, lock scheduling, and retries remain dominant candidates for profiling.

## Reproducible native harness

```sh
cargo build --release --locked --bins
# Core calls, comparable in invocation mode to the original Python harness:
target/release/mai-bench --agents 16 --workers 8
target/release/mai-bench --agents 16 --workers 8 --hotspot-agents 4
# Separate native CLI processes and CLI startup sampling:
target/release/mai-bench --agents 64 --workers 16 --cli target/release/mai
# Repository-size sensitivity:
target/release/mai-bench --agents 16 --workers 8 --repository-files 1000
# Complete bounded matrix: populations 1, 16, 64, 256; full-path contention:
bash benchmarks/run.sh
# Also 1024 agents, hardware permitting:
FULL=1 bash benchmarks/run.sh
# For true simultaneous fanout, independently qualify resource use first:
# WORKERS=256 bash benchmarks/run.sh
```

The harness creates a temporary local authority, optionally seeds files, creates all workspaces sequentially at a common base, then submits disjoint files or a shared hotspot through bounded workers. `agents` is population; `workers` is simultaneous execution capacity. A 1,024-agent population with 16 workers is not 1,024 concurrent writers. All benchmark repositories are disposable and removed on normal completion.

JSON includes accepted revisions/s, service-time p50/p95/p99, workspace readiness p50/p95, rejection counts, total optimistic retries for accepted requests, and cumulative SQLite wait/hold p95 for accepted requests. The updated harness also reports admission queue wait p95 and burst completion p95/p99, distinguishing service time from a burst's end-to-end latency. `--cli` samples ten `--version` invocations; these are warm executable startup measurements without repository I/O, not physical cold-cache CLI repository startup.

`integrity_ok` requires the entire expected workload to finish with exactly the expected conflicts and all file contents/revisions intact; exit 1 means that requirement failed. `publication_integrity_ok` separately verifies a contiguous accepted history and accepted content even if safe overload rejections prevent full completion. Error counts remain visible in JSON. Exit 2 means setup/infrastructure failure. The matrix continues after unsuccessful workloads and returns a failing status overall. Unexpected rejections are not reclassified as ordinary conflicts.

## Resource and recovery measurements

On Linux, `benchmarks/run.sh` wraps each workload in GNU `/usr/bin/time -v`, writing CPU user/system time, CPU utilization, peak RSS, context switches, and filesystem counters to a sibling `.resources` file. On other platforms use a platform profiler explicitly; the harness's JSON CPU/RSS fields remain null rather than inventing measurements. GNU time covers initialization, setup, submission, verification, cleanup, and child Git processes; RSS is the tool's reported maximum, not aggregate concurrent process-tree resident memory. `/tmp` filesystem characteristics must be recorded separately from the source directory.

Use `cargo test process_termination_after_acceptance` for an actual process-kill recovery regression: the CLI is terminated after authoritative acceptance and before checkout reset, then the same submission recovers without duplicating the revision or discarding later edits. Other tests simulate missing receipts, off-main receipts, and CAS races. These measure correctness, not recovery latency or power-loss guarantees; disk-full, filesystem crash, restore, and interrupted initialization drills remain work to qualify.

## Recorded local observations

Raw JSON and resource logs are in [benchmarks/results](benchmarks/results). The 2026-10-14 environment used Linux x86_64, AMD Ryzen 9 7950X (16 cores, 32 logical CPUs), Rust/Cargo 1.97.0, Git 2.43.0, and an ext4 source filesystem on NVMe. These are single samples in an active development environment, not statistically controlled comparisons. Some initial runs overlapped compilation or another benchmark; cache state and background workload were not controlled. No production or network workload was measured.

Before removal, Python 0.1.1 passed 17 tests in 2.583 seconds. Its 16-agent/8-worker disjoint core run accepted 16 at 18.72 revisions/s, service p50 25.19 ms and p95 853.14 ms, workspace setup 0.347 seconds. With four hotspot agents it accepted 13 and rejected three conflicts at 15.18 revisions/s, p50 22.94 ms and p95 854.45 ms, setup 0.334 seconds. These recorded baseline outputs came from the original unmodified harness.

An initial Rust 16-agent/8-worker disjoint core sample accepted 16 at 17.50 revisions/s, service p50 193.48 ms, p95/p99 914.29 ms, setup 0.527 seconds, with 48 optimistic retries. Four hotspot agents yielded 13 accepted, three conflicts, 15.78 revisions/s, p95/p99 823.48 ms, and 40 retries. Both integrity checks passed. The Rust adapter explicitly requests committed Git fsync and has a different pipeline; these are not durability-matched language-only comparisons and demonstrate no Rust throughput advantage.

The 64-agent/16-worker CLI sample accepted all 64 at 19.18 revisions/s, service p95 3,120.43 ms and p99 3,336.24 ms, 494 retries, cumulative lock-hold p95 59.32 ms, and cumulative lock-wait p95 1,604.86 ms. GNU time reported 2.68 seconds user CPU, 6.10 system CPU, 151% CPU utilization, and 5,512 KiB maximum RSS for the whole workload. Ten warm version invocations had startup p50 0.76 ms and p99 0.79 ms. A 1,000-file/16-agent core sample accepted all 16, with workspace setup 1.416 seconds and workspace p95 95.37 ms, showing materialization sensitivity in this limited workload.

At 1,024 agents with 16 workers, 978 accepted and 46 safely rejected at the contention budget; accepted-history integrity passed and full-workload completion failed. Submission throughput was 14.12 accepted revisions/s with service p99 6.69 seconds and burst p99 68.15 seconds. Workspace setup took 50.46 seconds. These measurements make fairness, retry amplification, and materialization priority work.

Larger burst results and their completion status are recorded in [the results notes](benchmarks/results/README.md). Initial 256/1,024-agent experiments exposed safe contention-budget rejections. The harness originally aborted content checking on an unaccepted file before printing JSON; this reporting defect received a failing regression and was fixed. The final reports retain operational errors, accepted-history integrity, and unsuccessful full-workload status. Do not present a partially accepted burst as established capacity.

## Bottlenecks and next measurements

Preparation is parallel, but each stale head can cause candidate trees/commits to be rebuilt. SQLite's scheduling can starve a request during repeated competing publications, and the bounded 64-attempt retry budget can reject independent requests under sustained bursts. Supported acceptance stays safe; throughput/fairness is not yet satisfactory at high population. Worktree creation is sequential and holds the registry transaction; full trees scale disk and startup costs. Revision counting/history scans grow with accepted history. Separate pure Rust validation/hash benchmarks from Git/index/ref/checkout timing before selecting native replacements.

Next measure 1/16/64/256/1,024 clients where feasible, both fully disjoint and fully contended, across 1k/100k/1M-file repositories and binary sizes, sustained and burst arrivals, cold/warm caches, admission limits, interruption/restore, and storage growth. Record repeated distributions, available RAM, filesystem/mount, CPU load, worker limits, all error classes, request waiting time, per-attempt lock duration, and rejected-request retries. Current receipt metrics aggregate lock time across attempts and omit rejected-request metrics; they are not a full contention profile.

Prioritize fairness/admission/backpressure and reduced retry amplification, persistent daemon caches, incremental change tracking, sparse or copy-on-write materialization, native Git operations where measured useful, and a retryable downstream outbox. Avoid blocking a future async executor with filesystem/Git work; use bounded blocking execution. Optimize accepted ordered changes and tail latency without weakening conflict/lease checks or fsync/recovery policy. Define production acceptance targets only after representative workloads and recovery qualification.

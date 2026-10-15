# S4 queue and recovery experiments

This standalone, dependency-free Rust model prepares [issue #54](https://github.com/maida-ai/mai/issues/54). It does not implement the product engine, run actual gates, or qualify realistic traffic. The [report](../../docs/spikes/s4/README.md) separates observed synthetic results, model simplifications and outstanding acceptance work.

## Run and collect

Use Rust with rustfmt and Bash; native Windows uses Git Bash and native Rust, not WSL. No platform-specific process APIs or downloads are needed. Only Linux has been executed locally.

```sh
bash experiments/s4/run.sh .throwaway/s4-new-run
bash experiments/s4/collect.sh .throwaway/s4-new-run .throwaway/s4-new-evidence
# Replace generated arrivals with a workload file; all four modes are replayed.
S4_WORKLOAD=experiments/s4/example-workload.csv bash experiments/s4/run.sh .throwaway/s4-import
# Optional independent Cargo verification, without touching product dependencies:
CARGO_TARGET_DIR=.throwaway/s4-cargo cargo test --all-targets --locked --offline --manifest-path experiments/s4/Cargo.toml
CARGO_TARGET_DIR=.throwaway/s4-cargo cargo clippy --all-targets --locked --offline --manifest-path experiments/s4/Cargo.toml -- -D warnings
cargo fmt --manifest-path experiments/s4/Cargo.toml --check
```

Both output and adjacent `.build` directories must be new. The wrapper records source SHA-256 values, input bytes, Git/compiler/platform provenance, formatting, compilation and unit results. `sNNN.log` files retain every generated input, grant, candidate, retry, pass, invalidation and terminal outcome. `metrics.log` contains CSV queue metrics; `recovery.log` contains CSV incident-overlay metrics. The collector copies an explicit allowlist of these generated paths, refuses symlink files, and excludes arbitrary/nested logs, binaries and builds. It also works after a failed run. Do not put private workloads into evidence intended for publication.

## Adapt the inputs

[grid.csv](grid.csv) is the initial synthetic sensitivity grid, not calibrated workload data or agreed realistic failure rates. Its 24 rows use seeds 7/42, per-agent intervals 600/1800/3600 seconds, persistent failure probabilities 0/15%, and ordinary/severe-heavy priority mixes. Each row generates ten waves from 30 agents and runs sequential, pipeline-2, pipeline-4 and batch-4: 96 queue scenarios. Gate duration is uniform over 30–90 seconds in 10-second increments; arrival jitter is at most 10% of the per-agent interval. Flake probability is integer `failure_pct / 2`; conflict probability equals failure probability. Ordinary priority percentages are 20 low / 45 normal / 30 high / 5 sev; severe-heavy is 20 / 45 / 10 / 25. Every third subsequent wave depends on the same agent's preceding wave. These are chosen synthetic inputs, not observations.

Replace grid rows to vary seed, interval, bundled failure/flake/conflict level and priority mix. For independent distributions, correlations, measured durations, bursts and dependency chains, supply `S4_WORKLOAD` instead. The exact CSV header is:

```csv
id,agent,arrival_s,class,dependency,gate_s,fail,flake,conflict,grant_expires_s,grant_revoke_s
0,0,0,low,-,60,false,false,false,-,-
1,1,10,sev,0,90,false,true,false,900,-
```

The CSV is unquoted and has no blank rows. IDs start at zero and are contiguous in topological order; agent IDs are 0–29. `dependency` is an earlier ID arriving no later, or `-`. Priority is `low`, `normal`, `high` or `sev`; booleans are exactly `true`/`false`. Times are simulated seconds, arrival and gate duration must be multiples of ten, arrival is at most 120000 and gate duration is 10–3600. High/sev require a grant expiry after arrival and no later than 240000; revocation is optional and lies within that interval. Grants are scoped to that row and issued at arrival. Low/normal use `-` for both grant columns. At most 3000 rows are accepted. The parser rejects malformed inputs, invalid dependencies and ungranted elevation. Imported metrics use seed/interval/failure `0`, severe_mix `false` and nominal load `NaN` as unavailable metadata; these fields do not describe imported traffic. Actual inputs remain in `input.log` and per-scenario traces.

Policy knobs currently live as named constants in `main.rs` (10-second resolution, 600-second repeated aging, 30-second synthetic conflict delay and lease grace, sev cap one), in `workload` (900-second grant TTL and 10% revocation at 120 seconds), and in `recovery.rs` (incident fixtures and service durations). They are research assumptions, not production defaults. Human grant cost is a sensitivity assumption of 60 seconds per synthetic grant; count and assumed seconds are both reported.

## Model boundaries

Queue candidates are immutable numeric identities, representing tree, parent and approved config rather than real Git objects. A completed attempt stores separate pass evidence; landing compares that evidence with candidate and trunk identity. Flakes fail once and retry the same candidate; permanent failures reject their submissions. A rejected prerequisite rejects its dependent; independent speculative descendants are rebuilt. Batch failure falls back to singleton diagnosis for the remainder of that scenario rather than pretending to implement an optimized bisector. Successful batch members share tip evidence, explicitly lacking individually checked intermediate revisions.

Effective priorities are recomputed after arrivals, expiry/revocation and dependency changes. Aging promotes to high, not sev. Inheritance propagates through earlier-ID dependency chains; prerequisites land before children are eligible. The cap counts inherited sev work too. If inheritance raises running work above the cap, the model invalidates/requeues the over-cap suffix (including a batch containing multiple newly severe members) and reports `cap_rebuilds`; this is an explicit experimental reconciliation choice, not a selected shipping policy. Ordinary sev preemption cancels only speculative lower-class attempts, retaining their submissions and invalidating their descendants. Expiry/revocation never fabricates a pass. Lower-class and tie conflict resolver responsibility, lease notices/grace, competing requests, revoked/expired requests and stale-holder publication are exercised separately in `policy.rs`. The queue itself models a conflict as an arrival-plus-grace delay, not a real shared object lease graph.

The recovery comparison is an **independent-service overlay**, replayed against each queue's observed landing times. Ten incident fixtures are alternatives, not ten simultaneous incidents. Each is observed at the one-third landing timestamp, attributes before/after failure over 120 seconds, then compares immediate revert with 300/900/1800-second fix windows. Fixes use normal 60-second gates, at most three attempts, and mandatory full confirmation; failed/timed-out confirmation invokes a gated revert without resetting the deadline. Flaky/uncertain attribution never grants a fix or compensates. A failed/conflicting revert freezes and notifies. Recovery service seconds are reported separately and **do not consume the queue's slots or change its throughput**. Queue utilization excludes this service. Thus these results cannot select recovery policy or claim coupled economics. Unrecovered incidents count changes on red through the queue observation horizon and report `NA` for time-to-green, not an invented recovery time. Revert gate service can finish after the fix-window deadline; the window bounds the opportunity to fix, and a separate 60-second backstop is modeled.

Queue utilization is actual simulated gate time divided by slots times elapsed time; completed speculative attempts waiting for their parent occupy a slot but consume no gate CPU in this model. Waste includes failed, retried and invalidated gate service. Latency percentiles use nearest rank among landed changes, grouped by **requested** class; submitted/landed/rejected counts prevent rejection censoring from disappearing. Aging/inheritance count unique affected changes. Expired/revoked counts evaluate each grant at landing, or at final observation for rejected changes. Overload is exposed by nominal offered single-slot work (30 agents × mean 60-second gate / per-agent interval) and backlog at the final-arrival horizon. Offered load ignores additional retry/conflict service and is not an admissibility proof. Draining finite arrivals is not proof against starvation under sustained traffic.

## Manual workflow

[experiment-s4.yml](../../.github/workflows/experiment-s4.yml) only supports manual `workflow_dispatch` and calls the existing shared runner. Select `linux`, `macos`, `windows` or `all`; its jobs have a 15-minute timeout and artifacts expire after 30 days. Register the workflow by merging it to the default branch before manually dispatching the desired branch. No workflow was dispatched for this preparation. macOS/native Windows results remain pending, and Linux success does not imply either platform passed.

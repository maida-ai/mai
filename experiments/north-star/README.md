# Delivery mechanics experiment

This harness measures mechanics in a disposable synthetic task repository. Fake-runner
results verify that the measurement plumbing works; they are not representative of
real-agent performance. Real-agent runs require a separately supplied runner and an
explicitly chosen output directory.

```sh
python3 experiments/north-star/run.py /tmp/mai-north-star-run \
  --mai "$PWD/target/debug/mai" --agents 3 --runner /absolute/path/to/agent-runner
```

The runner is called concurrently as `RUNNER WORKSPACE PROMPT_FILE`. It receives an
independent Python statistics task. It chooses its implementation and Git commands,
commits, then pushes to `mai` at `refs/for/main` with `-o agent=agent_N`. It must not
run `mai integrate` or change the trusted `gate.sh` and `mai.toml`. No model or agent
framework is built into this experiment.

`--integration explicit` is the baseline: the harness calls `integrate` once for each
received generation. Each call counts as one operator surrogate intervention;
read-only `show` calls are observations. The count is not measured human effort.
`--integration automatic` enables mai's opt-in `receive enable --auto-integrate`
setting and observes the serial engine without operator integration calls. It waits
up to five minutes (or `--timeout`, if shorter). `--integration observe` leaves mai's
setting disabled and waits for an externally configured engine.

The harness checks the fresh trunk and every landed commit against the trusted gate,
retains all submitted commit OIDs, checks the expected module identities and content,
and reports failed runners, rejected submissions, and unfinished queue entries.
Queue pages are read until the CLI cursor is exhausted. Automatic and observe modes
observe queue state after agent execution and during automatic/observe integration.
Because mai does not expose receipt timestamps, timing reports conservative
receipt-to-terminal intervals for every submitted generation. The lower bound is
the pre-agent-launch time (the initial successful queue snapshot must be empty); the
upper bound is the end time of a successful snapshot that observes that generation
as terminal. Each interval is `[0, upper - lower]`, so it includes agent execution
and integration time. It does not claim a polling detection latency. Missing
submissions, missing terminal observations, and incomplete timing fail the run.
The output directory must not already exist. Logs, commands, provenance, queue data,
authority and summary are retained even when a guardrail fails. Runner timeout defaults
to 900 seconds and cleans up the runner's process group.

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
`--integration observe` makes no integration calls and waits up to five minutes (or
`--timeout`, if shorter) for an externally configured automatic engine. It measures
that external setup only; this harness does not implement product automation.

The harness checks the fresh trunk and every landed commit against the trusted gate,
retains all submitted commit OIDs, checks the expected module identities and content,
and reports failed runners, rejected submissions, and unfinished queue entries.
Queue pages are read until the CLI cursor is exhausted. Receipt-to-terminal timing is
reported as unavailable because the current CLI does not expose receipt timestamps.
The output directory must not already exist. Logs, commands, provenance, queue data,
authority and summary are retained even when a guardrail fails. Runner timeout defaults
to 900 seconds and cleans up the runner's process group.

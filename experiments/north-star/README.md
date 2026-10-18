# Real agent intervention measurement

Build `mai`, choose an executable agent runner, and use a new output directory:

```sh
python3 experiments/north-star/run.py /tmp/mai-north-star-run \
  --mai "$PWD/target/debug/mai" --agents 3 --runner /absolute/path/to/agent-runner
```

The runner is called once per independent workspace, concurrently, as
`RUNNER WORKSPACE PROMPT_FILE`. It receives a plain text prompt describing a small,
independent Python statistics module. It chooses its own implementation, tests, and
Git commands. It must submit through the workspace's configured `mai` remote and must
not invoke `mai integrate`. No model, credentials, or agent harness are built into
this experiment.

Each output directory is new and retained, including the trusted source and gate,
authority, workspaces, prompts, runner identities, command logs, received-generation
snapshot and JSON summary. A runner exit is only an attempt: the harness discovers
durable generations with `mai queue`, inspects each, and explicitly runs `show` and
`integrate` for each received generation. It then checks durable outcomes, the fresh
trunk gate, submitted objects, and that all expected `agent_N.py` files landed.
The trusted gate accepts an empty initial candidate, and checks every submitted module
for the promised behavior. Integration is manual so the workload can be measured
against the same supplied binary before and after changes to operational tooling.

`operator_interventions_per_landed_change` compares the required `show` and
`integrate` operator surrogate commands with the 1.0 baseline. It is a count of
required operational commands, not an estimate of observed human effort. Setup,
agent, and observation commands are logged separately in `commands.json`. The run
records failed and unfinished generations, per-change integration elapsed time,
retained worktree OIDs and broken-trunk gate results. If no changes land, the metric is
`null`. Runner timeout is controlled by `--timeout` (default 900 seconds); an existing
output path is rejected and the harness never removes run data.

For plumbing checks only, a fake runner can create a module, commit it, and push it to
`mai`. Label such output as fake-run mechanics evidence; it is not evidence about
real-agent product behavior.

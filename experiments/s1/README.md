# S1 Git transport experiment

Question: can the proposed receive/registration and sync protocol satisfy the observed Git boundaries before the production engine is built? This standalone Rust harness creates tiny disposable authority/client repositories and exercises file transport and host loopback HTTP. It does not use the mai library or add production dependencies. See the [qualification report](../../docs/spikes/s1/README.md) for fixture limitations and historical results.

## Local execution

Requirements: Rust with rustfmt, Git >=2.40, and Bash. On Windows use Git for Windows and Git Bash with a native Windows Rust toolchain; this is not a WSL qualification. The portability changes are locally checked on Linux, but macOS and Windows execution remains pending until evidence is collected. Windows fixtures must use a local drive rather than a UNC share.

From the repository root:

```sh
bash experiments/s1/run.sh .throwaway/s1-new-run
bash experiments/s1/collect.sh .throwaway/s1-new-run .throwaway/s1-new-evidence
```

Both the fixture and adjacent `.build` directories must be new. `run.sh` captures provenance, source hashes, compiler output, policy-unit results and scenario assertions. The harness retains Git repositories and per-fixture push/audit logs. `collect.sh` copies only logs and source hashes to a new directory, including whatever exists after a failed run. It excludes binaries, object stores, configs and source checkouts. Nothing cleans existing fixtures automatically.

The host experiment uses an ephemeral loopback port and invokes many short Git processes. The previous Linux implementation ran 115 scenario assertions; the wrapper now has three policy/path unit checks. No benchmark threshold or timing guarantee is implied. The optional Docker Desktop client mode is described in the qualification report; it remains Unix-only, needs an explicitly selected digest-pinned image, and is not enabled in the manual hosted workflow.

## Manual GitHub execution

`.github/workflows/experiment-s1.yml` has only `workflow_dispatch`: pushes, PRs and schedules do not launch experiments. It calls the shared `evidence-experiment-runner.yml` workflow with `experiment: s1`. Choose `linux` (default), `macos`, `windows`, or `all`. `all` deliberately starts three jobs, each with a 15-minute timeout. Ordinary product CI remains separate and unchanged.

GitHub [requires the dispatch workflow on the default branch](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manually-run-a-workflow). Merge the workflow setup first; after it is registered on `main`, select any branch containing the workflow and harness for subsequent manual runs. A testing branch itself need not be merged. No automatic experiment trigger is added to work around this requirement.

```sh
gh workflow run experiment-s1.yml --ref <branch> -f platform=macos
gh workflow run experiment-s1.yml --ref <branch> -f platform=windows
# Explicitly opt into all three native runners:
gh workflow run experiment-s1.yml --ref <branch> -f platform=all
gh run list --workflow experiment-s1.yml
gh run download <run-id> --dir .throwaway/s1-ci-artifacts/<run-id>
```

Each native runner executes the file and host HTTP scenarios using its installed Git. Containers and exact Git 2.40 are not qualified by this workflow. The artifact name includes OS, architecture, run ID and attempt; artifacts expire after 30 days. Provenance records the checked-out commit and image/tool versions, while source hashes identify the harness bytes. Collection/upload runs even after an experiment failure when the runner remains available; runner loss or a hard job timeout can prevent artifact recovery. The original experiment failure still fails the job.

Follow [Turn CI artifacts into durable evidence](../README.md#turn-ci-artifacts-into-durable-evidence) for the download, review, snapshot README, qualification-report update and evidence PR process. For S1, selected snapshots belong under `docs/spikes/s1/evidence/` and the platform matrix lives in `docs/spikes/s1/README.md`. Preserve old snapshots unchanged. Current source files moved from `docs/spikes/s1/`; historical logs, commands and hashes describe their original revisions and must not be rewritten to appear current. A green CI run is evidence for that runner and scenario set, not automatic S1/M0 acceptance.

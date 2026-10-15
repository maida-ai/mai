# Experiments

This directory holds reproducible research harnesses and experiment setups. They collect evidence about proposed mechanisms; they are not shipped product code, routine regression tests, or performance benchmarks. Keep ordinary tests in `tests/`, benchmarks in `benchmarks/`, and reviewed findings plus selected evidence under `docs/spikes/`.

Use one directory per experiment, with a README describing its question, prerequisites, commands, outputs, costs and limitations. Use disposable output directories under `.throwaway/`, refuse to overwrite existing runs, and retain useful failure evidence. Harness assertions check the experiment's assumptions; passing them does not automatically satisfy a product acceptance gate.

Experiments are opt-in. Do not add them to ordinary push/PR CI or schedules. Give each experiment its own manually dispatched workflow, named `experiment-<id>.yml` (for example, `experiment-s1.yml`), with bounded jobs and artifact retention. Dispatching one experiment must not run unrelated experiments. Record the source commit, source hashes, platform and tool versions alongside results; distinguish actual execution from simulations and pending checks. Review and normalize selected artifacts before committing them as evidence. Never upload credentials, runner environments or entire working repositories.

## Shared CI runner

Each experiment keeps its own `workflow_dispatch` entry point, which calls [evidence-experiment-runner.yml](../.github/workflows/evidence-experiment-runner.yml) with an `experiment` directory name and a `platform` (`linux`, `macos`, `windows`, or `all`). The shared workflow has only `workflow_call`; it cannot start independently. Keep callers manual-only. A reusable workflow owns the runner matrix and jobs; a composite action can only share steps within a job.

The shared runner checks out the caller's revision, installs stable Rust with rustfmt, preserves LF fixtures, and runs Bash scripts from the repository root. Each matrix job has a 15-minute timeout and uploads evidence for 30 days. Add a new experiment by providing these scripts and a manual caller modeled on [experiment-s1.yml](../.github/workflows/experiment-s1.yml), changing its experiment ID and display names:

- `experiments/<id>/run.sh <fixture-root>` executes the experiment, records provenance and source hashes, and returns nonzero on failure. The runner supplies `.throwaway/<id>-ci` as the fixture root.
- `experiments/<id>/collect.sh <fixture-root> <destination>` copies only reviewable evidence to `evidence-output`, including partial logs after failures. Collection and upload are attempted even after failure; they do not mask the original result. Do not copy entire repositories, binaries or environment dumps.

Use lowercase directory IDs beginning with a letter and containing only letters, digits and hyphens. Each harness owns its collection allowlist and any experiment-specific prerequisites. The shared setup currently supports Rust/Bash experiments; extend it when another experiment has a concrete requirement. Do not add other experiments to an existing caller's jobs.

## Turn CI artifacts into durable evidence

CI artifacts are temporary working material: the manual workflow retains them for 30 days. Reviewed snapshots in Git preserve the observations and their provenance after artifacts expire, so maintainers can evaluate qualification decisions later. A successful workflow is not itself an accepted qualification result; failed or partial runs can also provide useful evidence.

1. Find the completed run for the experiment-specific workflow and download its artifacts before expiry. The commands below use S1 as the example. Replace `<run-id>` below with the GitHub Actions run ID. Use a new download directory for each collection; `gh run download` puts separately named artifacts into their own subdirectories.

   ```sh
   gh run list --workflow experiment-s1.yml
   gh run view <run-id>
   gh run download <run-id> --dir .throwaway/ci-artifacts/<run-id>
   ```

2. Review each platform and attempt separately. Check the run/job conclusion, completed assertions, failure logs, checked-out source commit, harness hashes, OS/architecture and tool versions. Compare hashes against the source revision that actually ran, not a newer checkout. Missing artifacts or an interrupted run mean missing evidence, not a pass. Keep useful failures rather than selecting only successful output.
3. Create a task branch for the evidence update, or reuse its existing branch. Copy selected logs, provenance and source hashes into a new snapshot directory such as `docs/spikes/s1/evidence/YYYY-MM-DD-macos-run-ID-attempt-N/`. Preserve older snapshots unchanged. Do not commit the entire download blindly, generated repositories, binaries, credentials or runner environment dumps. Normalize machine-specific paths or ephemeral ports if useful, recording every normalization; never change outcomes or source hashes.
4. Add a README to the snapshot with the workflow run URL, run ID/attempt, source commit, artifact names, execution command/inputs, platform/tool versions, observed results, failures, limitations and normalization notes. Link the relevant logs. Distinguish measured behavior from fixture simulations and checks that did not run.
5. Update the experiment's qualification report and platform matrix, linking the new snapshot. For S1, update `docs/spikes/s1/README.md`. Mark only criteria supported by the reviewed evidence as verified; leave remaining criteria pending.
6. Review the staged diff, commit the selected evidence and report, then push the branch and open or update its PR using `gh pr create --body-file` or `gh pr edit --body-file`. Describe the findings, checks and remaining gaps. Use `Refs #51` for partial S1 evidence; use a closing reference only when every acceptance criterion is satisfied. Maintainer review determines qualification acceptance; merging an evidence PR does not automatically complete S1 or M0.

## Available experiments

- [S1 Git transport](s1/README.md): receive protocol, registration failure recovery, fallback retention and simulated squash sync. [Recorded findings](../docs/spikes/s1/README.md).

# S2 native CI evidence — 2026-10-15

Source: [workflow run 38694778050](https://github.com/maida-ai/mai/actions/runs/38694778050), attempt 1, manual `workflow_dispatch` on `main`, commit `ff55f478f861d85c1a75347f685bfa5bc691d5c3`. The three-platform matrix corresponds to `platform=all`. The run started at 06:49:45 UTC and completed successfully at 06:50:32 UTC on 2026-10-15. All three run, collect and upload steps succeeded. This snapshot comes exclusively from those CI artifacts, not earlier local runs.

| Platform | Artifact | Artifact ID | Job | Median |
| --- | --- | --- | --- | --- |
| Linux x86_64 | `s2-Linux-X64-38694778050-1` | 11952983766 | [116288692720](https://github.com/maida-ai/mai/actions/runs/38694778050/job/116288692720) | 1.415511 ms |
| macOS arm64 | `s2-macOS-ARM64-38694778050-1` | 11954066576 | [116288692789](https://github.com/maida-ai/mai/actions/runs/38694778050/job/116288692789) | 6.393542 ms |
| Windows x86_64 | `s2-Windows-X64-38694778050-1` | 11953637176 | [116288692363](https://github.com/maida-ai/mai/actions/runs/38694778050/job/116288692363) | 15.302000 ms |

[run.json](run.json) records job and step conclusions. [artifacts.json](artifacts.json) retains GitHub artifact IDs, archive digests, sizes and expiry timestamps (2026-11-14); those API-provided archive digests are metadata, not a claim that this review independently downloaded and hashed the ZIP archives. [review.json](review.json) records independently recomputed timing summaries, unique file counts, source-hash matches and the three manifest rows per platform. Every platform has nine passing unit tests, ten passing differential cases and 31 timing samples below the strict median threshold. The missing deleted-object manifest assertion is documented in the completed [go/no-go report](../../go-no-go.md); overall S2 acceptance is not claimed.

## Evidence files

Each platform directory retains provenance, the exact `sources.sha256`, unit output, compile/format logs, run summary, selected scenario commands/output and all raw timing samples. Empty compile/format logs reflect successful quiet commands, corroborated by the job steps and subsequent unit/scenario completion. Follow [Linux](linux/provenance.log), [macOS](macos/provenance.log) and [Windows](windows/provenance.log) provenance for compiler, Git, OS image and hardware details. All runners used Rust 1.99.0; Git was 2.55.0 on Linux/macOS and 2.55.0.windows.5 on native Windows. The Windows job used native MSVC Rust with Git Bash, not WSL.

The [Linux](linux/scenarios.log), [macOS](macos/scenarios.log) and [Windows](windows/scenarios.log) scenario logs preserve expected conflicts, missing-base/driver failures and their surrounding successful assertions. Such nonzero Git operations are deliberate negative cases, not failed jobs. Timing files retain all 31 original `TIMING` lines plus the unchanged `CRITERION` line: [Linux](linux/timings.log), [macOS](macos/timings.log), [Windows](windows/timings.log).

## Collection and normalization

The workflow executed `bash experiments/s2/run.sh .throwaway/s2-ci` in full `mode=run`, then `bash experiments/s2/collect.sh .throwaway/s2-ci evidence-output`. Review downloaded the three artifacts with `gh run download 38694778050 --dir .throwaway/ci-artifacts/38694778050`. The original downloads remain local and untouched.

For each platform, review hashed every original downloaded file; those hashes are preserved in `original-artifact-files.sha256` within the platform directory. They identify the original files, not the normalized copies. The exact four `sources.sha256` entries were checked against bytes read with `git show ff55f478f861d85c1a75347f685bfa5bc691d5c3:<path>` for `main.rs`, `manifest.rs`, `run.sh` and `collect.sh`.

To keep the committed snapshot compact, exactly 100,000 generated `files/NNN/NNNNNN.txt` listing lines were omitted from each scenario log and replaced with an explicit review marker. Before omission, review verified all paths were unique and the count was exactly 100,000. No command, exit status, conflict/manifest output, timing sample or criterion line was omitted. The generator and source hashes remain available to reproduce the listing. All text logs were normalized to LF, trailing whitespace was stripped and redundant terminal blank lines removed; no paths, ports, outcomes, identities or timing values were altered. `sources.sha256` is copied byte-for-byte. Derived `timings.log` and `review.json` are explicitly review outputs, not new experiment executions.

No repositories, object stores, binaries, caches or runner environment dumps are included. No experiment was rerun or workflow dispatched while reviewing this evidence. Native execution is observed, but exact Git 2.40, cold-cache/general repository performance and real-language manifest extraction remain outside the recorded coverage.

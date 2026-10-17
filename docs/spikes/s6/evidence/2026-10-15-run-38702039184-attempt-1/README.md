# S6 native CI snapshot

[Run 38702039184, attempt 1](https://github.com/maida-ai/mai/actions/runs/38702039184) executed `3228eeb221581e4bd583bc8973cc03d24470d374` on 2026-10-15. See [qualification report](../../README.md) and [go/no-go assessment](../../go-no-go.md). `run.json`, `jobs.json`, `artifacts.json` and `issue.json` preserve the review context. `review.json` records independently checked source hashes and result summaries.

All files delivered by each artifact are retained. `file-manifest.json` maps every original artifact path, byte count and SHA-256 to its retained file and SHA-256. Text logs normalize line endings to LF and strip trailing whitespace and trailing blank lines; source hash entries retain their values. S2's full scenario log and S4's detailed scenario traces are losslessly gzip-compressed with deterministic headers; read with `gzip -dc PATH` (or Python `gzip.open`). No timing samples, filename rows, failures or scenario events are removed. Fixture repositories, caches and databases were excluded by the harness collector, not silently discarded during this review. Original downloaded artifacts remain unchanged locally.

These are hosted CI results, not local experiment results. Green job status applies only to the implemented checks. Limits and incomplete criteria are assessed separately in the reports.

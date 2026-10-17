# Scripted operator replay

This package records one scripted Linux operator replay on a local mai binary. It is functional workflow evidence, not a benchmark or a claim that the full integration queue is shipped.

## Identity and procedure

The product source baseline was `d84f26ae73c87c15224bd56e2a2596a124e542e1`. The replay runtime source was `ddcb862a9be3db80299fc2c71cd81040382511e2`, and its binary SHA-256 was `6e1b7c093ab896c0f85940a42e81c35ab1b8d178eb49922343dc876bbb5a24d2`. The runtime contained the recovery change `61da796cc5cc828077245b0eeefb3467210557c7` and observer change `e59bb8628a6ebef4e4faa86b35bc5aa4c89e64a4`; it was not built from final main or the later lease-fix branch. The task also submitted the documentation patch `2e44b1a7f0baa5f3c7f3216ac38883900888f974` as one of its three real changes; that commit is not part of the stated runtime source composition. `summary.json` retains these identities.

The trusted fixture gate ran `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, and `cargo test --all-targets --locked`, with one validator slot and an explicit build cache. All three gates passed.

The disposable authority baseline was commit `5fb2c3e6767bb7f001878573b37c0393bf199550`, whose parent was the product source baseline. Three real changes were received together and appeared as `received` in the pre-integration queue snapshot. The operator then explicitly integrated recovery, docs, and observer in sequence, observed each running integration with `wait`, compared each wait result with the exact landed commit, and checked the landed tree and parent recorded in `checked-landings.json`. The original agent clones retained their heads and were clean after the run; a fresh accepted workspace contained all three changes.

The initial observation returned pending after a stop request. That probe did not independently establish engine exit or lifecycle non-mutation. The supplemental `offline-proof.json` is the stronger check: after the three landings it adds a fourth, deliberately pending fixture, holds the exclusive engine lock during `wait`, records the same lifecycle hash and revision before and after, and leaves that extra generation received. This proves logical lifecycle equality for that fixture, not bitwise equality of SQLite WAL sidecars.

## Evidence and limits

`summary.json`, `operations.json`, `offline-proof.json`, and `checked-landings.json` are small sanitized JSON summaries. They contain no database, raw logs, absolute local paths, author emails, or machine usernames. `operations.json` retains observed command durations for audit context; they are not a comparative performance benchmark. `SHA256SUMS` covers the four JSON files, not this README.

This was a scripted replay on one Linux host, not independent human or agent behavior. It demonstrates concurrent receipt followed by explicitly sequential integration and wait, exact gated tree/parent records, preserved original clones, and a fresh accepted workspace. It does not establish throughput, scale, MTTR, comparative performance, native Windows/macOS behavior, automatic scheduling, or a general concurrency guarantee. The replay used a local harness and disposable local repositories; the harness, binaries, and raw logs are intentionally not included, so this package is not a portable rerun kit. Reproduction requires the exact source/runtime identities above and equivalent setup; no new experiment or comparison was run for this package.

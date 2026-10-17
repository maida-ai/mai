# Explicit validator recovery qualification — 2026-10-17

This evidence qualifies the bounded Linux same-boot `supervised-local-v1` path at source `ec0f6972a93bd64311733b0643c4b81e0c4e120d`. The committed [operator harness](../../../../experiments/validator-recovery/README.md) ran against a frozen debug binary with SHA-256 `d709c504abda4c8c683173fc50c4726d349c197d5327182d1f13b6a03f870f04`; the tracked source diff was empty. The binary and disposable authorities remain outside Git. [Identity](identity.json) records Linux 7.0.0-39, x86_64, Rust 1.85.0 and Git 2.53.0. Native CI and review are linked from [PR #112](https://github.com/maida-ai/mai/pull/112).

## Observed operator result

The harness killed its own foreground engine during an active gate. The surviving worker cancelled execution and recorded a claim-bound cleanup receipt. Public inspection/recovery reclaimed the slot, an exact repeated recovery returned the same result, and the interrupted generation remained queued with its first attempt marked interrupted. A fresh integration ran the gate again and produced exactly one r1 landing with its second attempt completed. The client's later dirty and untracked bytes remained unchanged.

| Milestone from engine kill | Observed elapsed seconds |
| --- | ---: |
| Cleanup receipt observed | 0.021462 |
| Explicit recovery completed | 0.160388 |
| Fresh gate and successful landing completed | 0.614622 |

These monotonic times are one tiny disposable functional observation, including command and polling overhead. They do not establish a production MTTR, distribution, scale result, comparison advantage or the deferred warm-overhead target. See the raw [commands](commands.jsonl), [receipt](receipt.json), [interrupted response](interrupted-request.json) and [outcome](outcome.json). An earlier setup attempt exceeded the Unix socket path limit before executing any gate; the committed harness therefore accepts an explicit short scratch path. An earlier successful binary was superseded by the final identity and lock-release fixes; its timing is not the final result above.

## Checks and independent review

`cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --all-targets --locked` and `cargo build --locked` passed on the final source. The [local log](local-checks.log) contains 236 passing tests, zero failures and five ignored subprocess helper entrypoints invoked explicitly by parent tests. `scripts/check_qualification.py` passed for 79 locked packages after adding the installed Rust toolchain to PATH; no production dependency was added.

The independent recovery qualification includes 11 synthetic API adversarial tests, four real Linux end-to-end tests, and separate-process interruption at three recovery journal boundaries. The native tests cover engine SIGKILL during gate and bootstrap, worker-death refusal and an escaped response exceeding 3 MiB. Synthetic proof/path tests do not substitute for the real worker tests. The claim component now uses 32 random characters from the existing tempfile facility; its format/sample-uniqueness regression does not claim to measure entropy.

Initial native Linux CI at `b3c9f78` exposed transient lock retention on completed recovery. [Failed run](https://github.com/maida-ai/mai/actions/runs/39014463026) reported slot ownership contention. `ec0f697` adds explicit unlock on successful and error exits; its deterministic test holds a duplicate of the exact locked file description across completed recovery and verifies that acquisition succeeds. The regression passed with the fix. No unobserved pre-fix deterministic run is claimed. Independent review found no remaining correctness blocker within the declared scope.

## Qualification boundary

Recovery requires the original opt-in contract, matching same-boot claim/receipt proof, exclusive ownership, validated paths and reconciliation. It never turns an interrupted gate into an accepted result. Worker death, missing proof, legacy claims, changed boot and contradictory state remain blocked. Deliberate process escape, independently ongoing offloaded jobs, completed external side-effect rollback, power-loss durability, whole-host recovery and non-Linux recovery are not qualified. The normal gate must pass again. Follow the [operator guide](../../../validator-recovery.md); do not delete occupied markers or kill arbitrary PIDs.

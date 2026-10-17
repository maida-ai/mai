# Recovery evidence follow-up

This addendum records source changes through `a6f1181` on 2026-10-17. The [original walkthrough and audit](README.md) remain a historical observation at `34d85b1`; the walkthrough was not repeated after these diagnostic and recovery-test changes. Independent QA reviewed the added tests and source changes, then independently summed the integration owner's final local required-test log. This is functional evidence, not a performance or power-loss qualification.

## New evidence and its boundaries

| Source | Observed test path and scope |
| --- | --- |
| `f950ae8`, `tests/delivery_cli.rs::timed_out_integration_keeps_running_and_exact_identity_recovers_result` | A real CLI integration times out while a trusted gate waits on an external marker. Releasing the marker permits exactly one landing; queries and identical retries recover that result before and after graceful engine restart. This Unix-only test covers client timeout/disconnect, not process kill. |
| `fa0affc`, `tests/recovery_active_gate.rs` | A real engine is externally killed after the gate's readiness marker and candidate residue are observed. Restart marks the old attempt interrupted. Retry refuses the occupied validator slot without a second gate invocation, evidence record or landing. Submission retention, trunk, original agent HEAD, dirty tracked bytes and untracked bytes remain preserved. The fixture is native-capable; this local execution is Linux. |
| `f950ae8`, `tests/delivery_cli.rs::failed_gate_exposes_bounded_diagnostics_through_show` and `src/landing.rs` | A failed real gate exposes its failure marker through `show`, retains bounded output tails, and directs the user to submit a corrected commit as a new change. A unit regression checks UTF-8 truncation and NUL replacement. |
| `a327a8d`, `src/engine.rs` | Transport regressions distinguish an identity-checked engine error from a missing response, including server disconnect after a complete reply. An ACK write failure no longer discards an already received result. Missing replies retain query-before-retry guidance. These are controlled IPC fixtures, not actual publication crashes. |
| `a6f1181`, `src/landing.rs` | If metadata reconciliation fails after successful trunk CAS, the error explicitly says publication succeeded and instructs preservation, restart and querying the same generation. This is diagnostic context; no new storage-failure injection was added for this message. |

An engine error response confirms the response was received, not that the operation made no mutation. In particular, publication can precede a reconciliation error. A timed-out request is not cancelled. The active-gate kill test establishes fail-closed preservation and refusal; it does not prove descendant cleanup, safe automatic slot reclamation, or full autonomous recovery. A refused retry can start an attempt before discovering the occupied slot, so it must not be described as automatically resetting all state.

## Verification status

The final local run at `a6f1181` passed **219 test entrypoints, with 4 ignored helper entrypoints**, across 19 targets. The [machine-readable count breakdown](follow-up-test-counts.json) was independently parsed from `/tmp/mai-final-current-tests.log`; that ephemeral log is not needed to understand the recorded counts. The integration owner also completed `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, and the release build. As before, the ignored process helpers are explicitly exercised by parent tests and do not represent omitted scenarios.

The integration owner reported all five jobs green for the earlier native workflow at `34d85b1`: [CI run 39011662749](https://github.com/maida-ai/mai/actions/runs/39011662749). That historical run does not qualify the new active-gate kill or follow-up IPC tests on Windows or macOS. Final-head native CI was pending when this addendum was written; inspect its exact source head before claiming native coverage for these additions.

No power-loss, disk-full or general I/O-failure guarantee follows from these tests. Dedicated actual landing interruptions after notes/final SQLite writes, safe slot inspection/reclamation, progress queries during a busy gate, automatic scheduling and post-squash sync remain outside this evidence. The guide's opening command inventory has now been updated, addressing that small documentation gap from the original audit.

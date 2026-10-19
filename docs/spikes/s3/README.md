# S3 warm-validator qualification

Historical status — 2026-10-17: This report preserves the 2026-10-15 assessment against the original criteria. [historical ADR 0010](https://github.com/maida-ai/mai/blob/9ee6acc844acc3e8272ebd24d8301c6b4af6870c/docs/adr/0010-staged-mvp-qualification.md) and the [2026-10-16 corrected functional review](functional-qualification.md) subsequently accepted scoped S3 functional readiness and completed scoped M0. The original measurements, NO-GO wording and broader limitations below remain historical evidence; they do not reopen that accepted gate.

**NO-GO for #53.** All three native jobs failed. Python passes the warm-overhead criterion on Linux/macOS, TypeScript fails it on both, Rust fails before warm sampling, and Windows fails a harness unit assertion before ecosystem execution. Failed criteria are retained, not averaged away.

## Observed measurements

| Platform / ecosystem | Bootstrap s | Cold gate s | Raw median s | Warm median s | Overhead | Result |
| --- | --- | --- | --- | --- | --- | --- |
| Linux / Rust | 0.737579 | 19.550607 | unavailable | unavailable | unavailable | Cold gate failed |
| macOS / Rust | 0.963112 | 10.431785 | unavailable | unavailable | unavailable | Cold gate failed |
| Linux / Python | 0.300626 | 1.102672 | 1.022435 | 1.039035 | 1.624% | PASS <15% |
| macOS / Python | 0.656186 | 0.845190 | 0.704432 | 0.706549 | 0.301% | PASS <15% |
| Linux / TypeScript | 34.228561 | 10.325781 | 6.031893 | 9.996276 | 65.724% | FAIL <15% |
| macOS / TypeScript | 27.693883 | 5.935238 | 3.217071 | 4.873397 | 51.486% | FAIL <15% |
| Windows / all | unavailable | unavailable | unavailable | unavailable | unavailable | Harness stopped before scenarios |

Each available warm/raw result contains five pairs; review recomputed medians and overhead from the raw TSVs. Bootstrap and cold-gate times are separate measurements. Nine harness units passed on Linux/macOS; Windows passed eight and failed `private_slot_paths`, which compares a native path to a slash-delimited literal. Windows has no ecosystem, isolation or cleanup results; missing artifacts are not passes.

## Criterion assessment

| #53 criterion | Observed evidence | Assessment |
| --- | --- | --- |
| Real Rust/Python/TypeScript repos and pinned commands | ripgrep `4649aa9700619f94cf9c66876e9549d83420e16c`, ItsDangerous `096c8d42545d3b68ea21a4f890fb2b2d8979c0bd`, Zod `e30870369d5b8f31ff4d0130d4439fd997deb523`; native Cargo, uv/pytest and Yarn gates recorded | Inputs exercised on Linux/macOS; Windows blocked |
| Bootstrap, raw/warm/invalidation; <15% separately | TSVs and result logs; Python passes, TypeScript fails, Rust unavailable | FAIL / incomplete |
| Rust gate validity | `feature::f1414_no_require_git` expected `bar` and `foo`, got only `bar`, on both hosts | Cold gate failure consistent with documented Git-ancestor temp-dir assumption; isolate cause and rerun |
| Two slots, private state and ports | Python/TypeScript concurrent gates pass on Linux/macOS; real port collision and subprocess retry recorded | Partial; Rust/Windows not qualified |
| Stale cache and residue | Broken native gates rejected, ignored/untracked residue removed, input change detected and bootstrap rerun for Python/TypeScript | Partial; invalidation is a harmless manifest-byte edit, not dependency-resolution change |
| Cancellation and COW/copy | Parent reaped and descendant heartbeat stopped on Linux/macOS; safe-copy independence passes; COW unavailable Linux, available macOS | Partial; Windows not reached and its COW probe remains unimplemented |
| Reproducible scripts/report | Sources verified; setup failures, logs and samples retained | PASS for reporting, not qualification |

## Required follow-up

Fix the Windows path assertion without weakening private-slot isolation; correct or isolate the Rust temporary-directory/Git-ancestor interaction while retaining the upstream gate. Investigate TypeScript switching overhead and demonstrate <15% on fresh native runs, or obtain an explicit accepted redesign rather than relaxing the threshold. Obtain complete Rust and Windows measurements and isolation/cleanup evidence, plus a real bootstrap dependency-resolution invalidation case. Record Windows COW availability with safe-copy fallback. Keep overlay/lazy work deferred unless measurements justify it. No production default or M0 acceptance follows from the passing Python subset.

## Reviewed evidence and verification

[Run 38701808476, attempt 1](https://github.com/maida-ai/mai/actions/runs/38701808476) executed `aa8d701f51f7f60c8dda730b67ab550b542c46f8` on 2026-10-15. The [snapshot](evidence/2026-10-15-run-38701808476-attempt-1/README.md) retains every collected artifact file with original and retained SHA-256s, job conclusions, source hashes and independently checked summaries. All recorded harness source hashes match the executed revision. See the [go/no-go decision](go-no-go.md) and [reproduction instructions](../../../experiments/s3/README.md).

This evidence-only review checked source identity, artifact completeness, reported outcomes against raw logs and the executed assertions, documentation links and whitespace. No experiment was rerun or workflow dispatched; no product code, thresholds, defaults or ADR status changed.

Baseline product checks passed during review: `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings` and `cargo test --all-targets --locked` (34 tests). Disposable builds stayed under `.throwaway/`. No new local experiment evidence was generated.

# S6 Cargo/npm/uv lockfile regeneration qualification

**GO for #56's three-ecosystem fixture qualification.** All three native platforms passed six unit tests and the Cargo, npm and uv scenarios. This qualifies the documented local-source fixture boundary, not arbitrary registry graphs or production validator security.

## Criterion assessment

| #56 criterion | Observed evidence | Assessment |
| --- | --- | --- |
| Merge independent manifest additions, begin with trunk lock and regenerate | Two agent branches and real Git three-way manifest merge; baseline lock bytes asserted; actual Cargo/npm/uv regeneration | PASS in all nine ecosystem/platform combinations |
| Both dependencies survive and native gate passes | Cargo executes both crates, Node requires both modules, uv builds/installs wheels and Python imports both; 11/22 values asserted | PASS |
| Cache/offline assumptions and determinism | Isolated caches, offline fixture commands; two regenerations from identical baseline locks are byte-identical | PASS within recorded tool/input versions |
| Failure behavior | Broken gate rejected; Cargo local version mismatch; npm/uv offline-unavailable versions; npm ERESOLVE and uv No solution found incompatible-constraint fixtures | PASS; offline unavailability is distinguished from solver contradiction |
| Timeout behavior | Three worker timeouts per platform, each classified non-success | PASS for native timeout probe; not an actual resolver hang or descendant cleanup proof |
| Evidence binds final regenerated tree | Each final tree equals checked tree and differs from pre-regeneration tree; parent/policy/attempt bound; stale evidence rejected; published tree/parent asserted | PASS |
| Trusted trunk configuration cannot be replaced | Baseline Git-blob checks reject candidate policy, gate script and both uv backend replacements before their execution | PASS for explicit protected closure |
| Scripts/report and no shipping-default selection | Source hashes verified; commands/results and short go/no-go report retained | PASS |

## Final-tree evidence

| Ecosystem | Final/checked tree on all three platforms |
| --- | --- |
| Cargo | `d58f2043b1da5fffb8b7d7f7c32ebf68d10b45be` |
| npm | `95e44763f781e1ecfbc26c3e47a037bf5c5a7431` |
| uv | `d5d7458f6f389a2962f5c1a18fec42a06b63e1b7` |

Review independently compared final/checked/pre-regeneration IDs and found the final fixture tree identical across native platforms for each ecosystem. Result logs preserve the distinct parent/landed commit identities, config blob and attempt. This observation does not promise reproducibility across arbitrary manager versions or registry inputs.

## Limits and disposition

Fixtures use source-pinned local alpha/beta 1.0.0 dependencies, not representative public registries or large transitive graphs. Offline gates need no registry network; uv tool bootstrap may require an index. Tool versions and commands remain in provenance/command logs. Timeout testing kills a synthetic sleeper after 100 ms, not a hung resolver; arbitrary descendants are not covered. Candidate builds still execute trusted-local code and are not sandboxed. Full policy closure for arbitrary backends/hooks and production restart/publication belong to later work. #56 asks to record these assumptions and behaviors, not to qualify every registry graph: recommend closing the scoped issue on merge. Shipping examples remain a maintainer decision; M0 still depends on other spikes and ADR acceptance.

## Reviewed evidence and verification

[Run 38702039184, attempt 1](https://github.com/maida-ai/mai/actions/runs/38702039184) executed `3228eeb221581e4bd583bc8973cc03d24470d374` on 2026-10-15. The [snapshot](evidence/2026-10-15-run-38702039184-attempt-1/README.md) retains every collected artifact file with original and retained SHA-256s, job conclusions, source hashes and independently checked summaries. All recorded harness source hashes match the executed revision. See the [go/no-go decision](go-no-go.md) and [reproduction instructions](../../../experiments/s6/README.md).

This evidence-only review checked source identity, artifact completeness, reported outcomes against raw logs and the executed assertions, documentation links and whitespace. No experiment was rerun or workflow dispatched; no product code, thresholds, defaults or ADR status changed.

Baseline product checks passed during review: `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings` and `cargo test --all-targets --locked` (34 tests). Disposable builds stayed under `.throwaway/`. No new local experiment evidence was generated.

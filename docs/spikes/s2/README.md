# S2 qualification status

Status: **pending platform qualification and maintainer review**, not M0 acceptance. The [harness](../../../experiments/s2/README.md) implements the scope of [#52](https://github.com/maida-ai/mai/issues/52) as isolated research; the current product engine is unchanged.

| Issue criterion | Evidence produced | Qualification limit |
| --- | --- | --- |
| Content, rename/edit, delete/edit, mode, binary, file/directory corpus | Bare merge-tree versus separate cherry-pick; clean trees checked against independent expected snapshots; raw conflict diagnostics | Synthetic corpus, not exhaustive Git behavior |
| Stale and squash-dependent bases | Explicit base and mapped-parent fixture; wrong-root negative must conflict | Mapping lifecycle belongs to S7/product design |
| Missing/multiple bases | Unrelated histories and criss-cross graph; reject unless a verified explicit dependency mapping applies | No guessed virtual-base policy selected |
| Median below 100 ms, 100k tracked files | Exactly 100,000 indexed paths, 31 retained raw timings, strict unchanged threshold, source/tool/hardware provenance | Warm identical-content fixture; hosted platforms pending |
| Attributes and custom drivers | Candidate attributes conflict case, approved authority attributes, actual custom driver execution and missing-script failure | Arbitrary drivers and executable policy pinning are not universally qualified |
| Object units/containment/fallback/renames | Restricted synthetic grammar unit/scenario tests; generated, unsupported, ambiguous, malformed and binary fallbacks; delete-plus-add | No real-language parser or semantic-independence claim |
| Receive versus final manifests | Actual Git diffs independently recomputed after simulated regeneration; generated output appears only in final view | Real Cargo/npm/uv regeneration belongs to S6 |
| Reproducible scripts and report | Standalone Rust, run/collect interface, manual native-platform workflow | No automatic workflow dispatch or issue closure |

## Evidence collection

Qualification evidence will be collected from manual CI workflow runs after this PR is merged. This preparation includes no local evidence snapshots or measured-result claims. Review the resulting artifacts separately before accepting the spike; no workflow is dispatched by this change.

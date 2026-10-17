# S2 Git merge qualification

**GO for #52's scoped Git CLI and restricted-manifest qualification.** The corrected harness passes on Linux, macOS and native Windows. This supersedes the earlier NO-GO for missing removed-unit assertions; the earlier evidence remains unchanged. It does not accept ADRs or complete M0 by itself.

## Results

| Platform | Unit checks | Differential cases | Samples / tracked paths | Median ms | Strict <100 ms |
| --- | --- | --- | --- | --- | --- |
| Linux x86_64 | 14 passed | 10 passed | 31 / 100,000 | 1.840866 | PASS |
| macOS arm64 | 14 passed | 10 passed | 31 / 100,000 | 13.213542 | PASS |
| Windows x86_64 | 14 passed | 10 passed | 31 / 100,000 | 21.682200 | PASS |

All samples, including process startup and evidence logging, are retained. Review independently checked sample IDs 0–30, 100,000 unique tracked paths and each median. Git was 2.55.0 on Linux/macOS and 2.55.0.windows.5 on Windows; Rust was 1.99.0. Runner image/architecture details remain in each provenance log.

## Criterion assessment

| #52 criterion | Observed evidence | Assessment |
| --- | --- | --- |
| Content, rename/edit, delete/edit, mode, binary and file/directory cases versus cherry-pick | Ten `PASS case=` rows per platform; exact expected clean trees and oracle conflicted index entries asserted | PASS |
| Stale and squash-dependent intended bases; missing/multiple base policy | Stale and dependent-squash checks, wrong-old-base rejection, zero/multiple-base checks | PASS for explicit fixture policy |
| Median <100 ms on 100k paths | Raw timing samples and independently recomputed medians above | PASS without threshold adjustment |
| Checkout-free attributes/custom-driver trust | Bare integration, approved driver, rejected candidate-relative executable and approved binary-policy conflict | PASS for tested driver boundary |
| File/symbol/config/module, containment, generated/ambiguous fallback | Unit assertions plus complete Git-derived manifests | PASS for restricted grammar |
| Receive/final post-regeneration manifests and rename delete-plus-add | Receive: host deletion and port addition only; final: those plus generated-file fallback; rename: four added and four deleted units; deletion: four deleted units | PASS; original gap closed |
| Reproducible harness and report | Hashed sources, exact commands, original artifact hashes, this report and go/no-go assessment | PASS |

## Limits and disposition

The synthetic identity grammar is not a source-language parser and cannot authorize semantic independence. Regeneration widening is a generated-file fixture; actual tool regeneration belongs to S6. The benchmark uses a warm object store and shared small blobs, not cold storage or realistic large-content workloads. Exact minimum Git 2.40 and arbitrary custom drivers remain unqualified. These are recorded scope limits, not failures of #52's agreed fixture criteria. Recommend closing #52 when this evidence is merged; no production implementation or shipping default is selected.

## Reviewed evidence and verification

[Run 38701689155, attempt 1](https://github.com/maida-ai/mai/actions/runs/38701689155) executed `ad30707b3214a7a91b3b2541d7ad90056c5c3291` on 2026-10-15. The [snapshot](evidence/2026-10-15-run-38701689155-attempt-1/README.md) retains every collected artifact file with original and retained SHA-256s, job conclusions, source hashes and independently checked summaries. All recorded harness source hashes match the executed revision. See the [go/no-go decision](go-no-go.md) and [reproduction instructions](../../../experiments/s2/README.md).

This evidence-only review checked source identity, artifact completeness, reported outcomes against raw logs and the executed assertions, documentation links and whitespace. No experiment was rerun or workflow dispatched; no product code, thresholds, defaults or ADR status changed.

Baseline product checks passed during review: `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings` and `cargo test --all-targets --locked` (34 tests). Disposable builds stayed under `.throwaway/`. No new local experiment evidence was generated.

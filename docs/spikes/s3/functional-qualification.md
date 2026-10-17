# S3 corrected functional qualification — 2026-10-16

**GO for ADR 0010's scoped functional readiness; NO-GO against the original performance criterion.** [Run 38947607178, attempt 1](https://github.com/maida-ai/mai/actions/runs/38947607178) executed `0d0ac71bc9b5fb364f78d63cc856867d665c079a` on Linux x86_64, macOS ARM64 and native Windows x86_64. All three workflow jobs failed because strict `mode=run` retains the original <15% criterion, not because a functional failure was hidden.

| Platform | Unit checks | Functional scenarios | Rust overhead | Python overhead | TypeScript overhead |
| --- | --- | --- | --- | --- | --- |
| Linux | 11 passed | PASS | 61.097% FAIL | 0.647% PASS | 64.262% FAIL |
| macOS | 11 passed | PASS | 34.757% FAIL | 2.072% PASS | 39.123% FAIL |
| Windows | 11 passed | PASS | 39.940% FAIL | 1.791% PASS | 57.600% FAIL |

Reviewed each platform's summary, ecosystem results, isolation/cancellation logs and unit output. All three report `functional_pass=true`, no functional failures and complete timing measurements. Real ripgrep, ItsDangerous and Zod gates pass; changed input triggers bootstrap, poisoned source fails the gate, residue is removed and both isolated concurrent slots pass. This confirms the corrected ripgrep non-repository temporary path, tracked poison path and native Windows path assertions in the exercised fixtures.

The [selected immutable artifact snapshot](evidence/2026-10-16-functional-review/SHA256SUMS) retains exact downloaded summary/result, isolation/cancellation, unit, provenance and source-hash files; Unit logs are losslessly gzip-compressed to preserve their original trailing blank lines; SHA-256 hashes cover the retained files. All recorded source hashes were independently compared to the executed Git revision. Full command and sample logs remain in the run artifacts; this is a selected review snapshot, not an assertion that all artifact contents are committed. No experiment was rerun or workflow dispatched for this review.

Harmless manifest-byte invalidation is not a dependency-graph change, native Windows copy-on-write probing remains unavailable with safe-copy fallback, and cooperative descendant cleanup does not qualify adversarial process escapes. These limits accompany the accepted functional fixture boundary. Realistic performance, dependency-change workloads and optimization follow MVP observations. Earlier evidence and the original NO-GO remain unchanged; this report neither claims a <15% pass nor closes the broader original #53 criteria. Actual product validator paths still require their own regressions before enablement.

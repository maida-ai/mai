# S6 lockfile regeneration qualification

**Harness prepared; CI evidence and M0/S6 acceptance remain pending.** The isolated [harness](../../../experiments/s6/README.md) runs real Cargo/npm/uv gates and Git operations over small version-pinned local dependency fixtures. No product implementation or shipping example selection is included. Refs #56.

## Criterion mapping

| Issue criterion | Harness coverage | Remaining qualification |
| --- | --- | --- |
| Cargo, npm, uv independent additions | Two stale agent commits, separate generated locks, actual three-way manifest merge, baseline lock reset, real manager regeneration and gates | Representative registry dependency graphs and native hosted platforms |
| Both additions survive | Cargo executes both crates; Node requires both installed modules; uv installs wheels and Python imports both; values 11/22 asserted | Broader ecosystem fixtures |
| Cache/offline assumptions | Isolated Cargo/npm/uv caches; all fixture commands offline; uv uses host Python with automatic downloads disabled; source dependencies need no registry downloads | Cold/warm remote registry availability, transitive dependency cache behavior |
| Deterministic output | Two regenerations starting from identical baseline lock bytes produce byte-identical final lockfiles for each ecosystem | Cross-manager-version and cross-platform reproducibility |
| Failure behavior | Cargo incompatible local version, npm conflicting exact peer constraint (ERESOLVE), uv incompatible Python requirement (No solution found), separate offline-unavailable versions and failing gates required to fail in all three ecosystems | Registry-specific diagnostics and large resolver graphs |
| Timeout behavior | Native worker process killed after 100 ms, classified as timeout, never success | Actual resolver hangs and descendant-process cleanup |
| Exact final-tree evidence | Pre-regeneration tree differs; successful evidence records final tree, parent, baseline config blob and attempt; stale tree/parent/attempt rejected; published commit matches final tree/parent | Production engine publication/restart protocol belongs to later implementation and S7 |
| Trusted configuration/executables | Both protected config and gate-script replacement independently fail baseline Git-blob verification before candidate execution | Full policy closure including arbitrary build backends, hooks and third-party scripts; candidate builds remain executable trusted-local code, not sandboxed |
| Scripts and report | Standalone Rust harness, fixture wheel backend, wrappers, manual workflow, evidence collection | Maintainer review; native macOS/Windows manual CI |

## Evidence collection

Qualification evidence will be collected from manual CI workflow runs after this PR is merged. This preparation includes no local evidence snapshots or measured-result claims. Review the resulting artifacts separately before accepting the spike; no workflow is dispatched by this change.

## Decision

These fixtures test discarding agent lockfile deltas, regenerating from trunk and binding gate evidence to the regenerated tree. Results across native platforms and representative package graphs remain pending. Keep S6 and M0 pending; use the manual workflow and additional explicit fixture inputs to collect that evidence. No shipping defaults are selected.

The harness also pins both uv fixture build backends to baseline Git blobs, rejects their replacement, and snapshots the candidate tracked tree before the gate and verifies it is unchanged afterward. Separate npm/uv offline-unavailable failures are not claimed as unsatisfiable solver constraints. Additional fixtures require npm ERESOLVE for an exact incompatible alpha peer and uv No solution found for beta requiring Python >=4 against the project supporting >=3.10.

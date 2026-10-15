# ADR 0006: One unit for leases, conflicts, and merges

Status: proposed, 2026-10-14. Symbol/key-path extraction and structural merging are not implemented. M2 and later work waits for 5–10 external users on real repositories with failures recorded, unless the maintainer explicitly overrides that gate.

## Proposal

Use the same unit for leasing, conflict detection and merging: a symbol where reliably parseable, a key path for supported structured configuration, otherwise a path. Proposed identifiers include `src/api.rs::Router::add_route` and `Cargo.toml::dependencies.serde`. Extract base and new versions only for changed files. Unsupported language, generated input, parse failure or ambiguous identity explicitly reports path fallback; never silently narrow a conflict after incomplete parsing.

Evaluate feature-gated tree-sitter extraction and an existing structural merge driver before designing a merger. Initial languages and configuration formats remain maintainer decisions; this ADR adds no dependencies. Structural merge is opt-in by repository and path pattern. Conservative path rejection remains the default. Git three-way textual merging, deterministic structural merging and model judgments are distinct mechanisms.

A deterministic definition/reference check can flag signature changes interacting with references in other deltas. Its approximation and warn/reject policy must be explicit; syntactic extraction does not establish semantic equivalence. Every configured required check runs against the exact integrated result under [ADR 0004](0004-integrated-candidate-validation.md).

## Trust and verification

Models may only add conflicts or require review. They cannot remove conflicts, declare equivalence, accept a merge or approve landing. `UNCERTAIN` fails a gate. An optional external instruction-file guard follows the same rule; a human policy exception is a separate audited action and cannot bypass deterministic requirements. No model calls or orchestration belong in the Rust core.

Test symbol additions/deletions/renames, nested and duplicate names, unsupported or malformed files, structured configuration, fallback overlap, and validation failures. Report false-conflict and semantic-break rates on real workloads. [Conflict ownership](0007-intent-and-conflict-ownership.md) uses these same units and preserves conservative behavior when extraction is incomplete.

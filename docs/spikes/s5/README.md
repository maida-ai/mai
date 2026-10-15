# S5 forge-cycle qualification

Status: S5/M0 acceptance remains pending CI evidence and maintainer review. No production implementation or shipping default is selected. [Issue #55](https://github.com/maida-ai/mai/issues/55) remains open; [ADR 0003](../../adr/0003-git-native-integration-queue.md) remains the design authority.

## Evidence and scope

The [standalone harness](../../../experiments/s5/README.md) uses real Git CLI subprocesses, a bare local forge, an authority and separate agent/human/merge clones. Exports are synthesized single commits parented on the last synced upstream, carrying the current trunk tree. It does not export raw trunk history, use GitHub's API, or maintain one reused PR branch. The validation oracle compares every expected synthetic filename and byte and rejects an explicit invalid-content fixture. It is not a production validator, receive policy, authentication layer or branch-protection implementation.

Qualification evidence will be collected from manual CI workflow runs after this PR is merged. This preparation includes no local evidence snapshots or measured-result claims. Review the resulting artifacts separately before accepting the spike; no workflow is dispatched by this change.

| Issue criterion | Harness coverage | Remaining work |
| --- | --- | --- |
| Three agent/export/forge/sync cycles with concurrent human edits | Three squash cycles and three single-commit rebase cycles; actual commits, fetches and fast-forward pushes; `CYCLE` and `MODE PASS` records | Review artifact and chosen synthesized-delta strategy; maintained PR/raw-trunk strategies not tested |
| Zero spurious conflicts and exact contents/diffs | Every successful merge exits zero; exact complete tree contents; exports contain only new agent delta and syncs only concurrent human delta | Broader real repositories, attributes, multiple exported commits and large fixtures are outside S5's small synthetic corpus |
| First-parent revisions and second-parent provenance | Every landing checks tree, parents and trailer; nine contiguous accepted revisions per mode after r0; sync's second parent is the observed forge head | Persisted production revision allocation belongs to implementation/S7 |
| Genuine upstream conflict with owner | Git exit 1 plus `shared.txt` conflict diagnostics; owner recorded in `CONFLICT` line | Maintainer still owns actual conflict ownership policy |
| Continue independent work on the last synced base | Separate independent landings while failed validation and genuine conflict are pending; last-synced ref and forge head unchanged | No claim that arbitrary overlapping work is safe |
| Failed validation | Invalid upstream tree rejected; no acceptance revision or authority ref advance; human correction appends a commit | Synthetic policy only; real project gates remain separate |
| No force-push or automatic human-main mutation | Authority exports only named branches; only human/forge-merger roles push forge main; independent landings inspect unchanged forge head | GitHub API, required checks, review rules, protected branches, credentials, webhooks and races remain pending |
| Reproducible harness/report | Fresh-root runner, source hashes, explicit collector allowlist, manual shared-runner workflow | macOS and native Windows manual CI runs pending |

Any go-criterion failure must be resolved or reflected in an ADR redesign before M1. Platform success and real-forge behavior must not be inferred from the local topology model.

## Verification

`run.sh` runs shell syntax validation, collector allowlist/overwrite/reuse/output-boundary checks, `rustfmt --check`, warning-denied standalone Rust unit compilation and tests, warning-denied harness compilation and all Linux scenarios. CI results remain pending. Product source and dependencies are untouched; the coordinator ran the unchanged baseline's `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings` and `cargo test --all-targets --locked`. Workflow YAML and local Markdown links are checked separately before publication. No CI workflow was dispatched and no issue was closed.

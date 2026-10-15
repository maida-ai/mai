# S1 linux CI observation: 2026-10-15 UTC

[Workflow run 38678128220](https://github.com/maida-ai/mai/actions/runs/38678128220); [platform job](https://github.com/maida-ai/mai/actions/runs/38678128220/job/116236292711). Run attempt: 1. Job conclusion: **success**. Artifact: `s1-Linux-X64-38678128220-1`. Source commit: `05df0a0e996ef4493308ae9d4a3badd169210ff7`. Workflow: `experiment-s1.yml`, inputs `platform=linux`; shared runner selects `ubuntu-latest` for this job.

Execution: `bash experiments/s1/run.sh .throwaway/s1-ci`, followed by `bash experiments/s1/collect.sh .throwaway/s1-ci evidence-output`. Download: `gh run download 38678128220 --repo maida-ai/mai --dir .throwaway/ci-artifacts/38678128220`. git version 2.55.0; rustc 1.99.0 (b940084d7 2026-09-28); host: x86_64-unknown-linux-gnu; image_os=ubuntu24; image_version=20261004.327.1.

115 scenario assertions passed. test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

The host experiment completed successfully. Container scenarios were not requested. The final S1 NOT QUALIFIED line is the harness's fixed per-run limitation notice, not an aggregate decision across all snapshots.

See [run.log](s1-ci.build/run.log), [unit.log](s1-ci.build/unit.log), [provenance.log](s1-ci.build/provenance.log), and [source hashes](s1-ci.build/sources.sha256). Formatting/compiler logs and fixture push/audit logs are retained where generated. All five source hashes were checked against the recorded Git revision, interpreting the Windows checksum manifest's `*` as the binary-mode marker. Provenance shows no relevant working-tree modifications.

Files are copied byte-for-byte from the artifact, preserving relative paths and line endings. No normalization was applied: runner paths, ephemeral ports, synthetic fixture object IDs and failure backtraces remain. Only `.log` files and `sources.sha256` are included; no generated Git repository, executable or environment dump is retained. The `authority.git` directories here contain audit logs only. Credentials mentioned in fixture logs are public synthetic fixture values, not real credentials.

This qualifies only the exercised host file/loopback-HTTP and simulated sync scenarios. It does not establish exact Git 2.40 compatibility, container coverage on this host, production authentication/grant lifecycle, persistence guarantees or full sync recovery. See the [qualification report](../../README.md) for the aggregate assessment and earlier Docker Desktop evidence.

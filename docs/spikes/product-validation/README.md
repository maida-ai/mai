# Local product validator ecosystem evidence

All three pinned source fixtures passed the actual manual-validator product on local Linux, including fresh bootstrap, cold gate, warm gate, candidate switch and return, deliberately failing candidate, successful retry, trusted-input invalidation and fresh workspace bootstrap. The final run is [2026-10-16-native-corrections](2026-10-16-native-corrections/summary.json); its summary records no unexpected failures. This refresh binds the later native-process corrections, including windows_arguments.rs, to a new exact source/binary manifest; execution here remains local Linux. Receive and landing remain unavailable.

The final command ran from the workspace implementation worktree:

```sh
TMPDIR=/var/tmp python3 experiments/product-validation/run.py ../next-batch/final-native-corrections --mai target/debug/mai --fixtures /tmp/mai-product-fixtures
```

The fixtures were imported as parentless commits containing their exact pinned upstream Git trees. Git tree identities are checked before adding experiment policy; archives, tree IDs, revisions and lockfile hashes are recorded in fixture identity JSON. This deliberately excludes upstream history: direct import of the pinned ItsDangerous repository was correctly rejected because its history contains retired submodule gitlinks. These results qualify product execution over the pinned source trees, not import support for those upstream histories.

Process timings below are seconds recorded by the product. Bootstrap and gate timing are separate; end-to-end command wall times, including materialization and cleanup, are in commands.jsonl. Each normal candidate passed and each deliberately invalid candidate returned typed failure with exit code 3. Bootstrap was reused for all warm/switch/failure/retry samples; declared-input invalidation forced a new successful bootstrap before the next gate.

- **ripgrep:** fresh bootstrap 0.104, cold gate 7.116, warm gate 2.912, switched gate 2.982, invalidated bootstrap 0.084, fresh workspace bootstrap 0.083. Full results: [rust cold](2026-10-16-native-corrections/rust-cold.json), [invalidation](2026-10-16-native-corrections/rust-invalidation.json), [workspace](2026-10-16-native-corrections/rust-workspace-fresh.json).
- **ItsDangerous:** fresh bootstrap 0.432, cold gate 0.610, warm gate 0.589, switched gate 0.610, invalidated bootstrap 0.143, fresh workspace bootstrap 0.174. Full results: [python cold](2026-10-16-native-corrections/python-cold.json), [invalidation](2026-10-16-native-corrections/python-invalidation.json), [workspace](2026-10-16-native-corrections/python-workspace-fresh.json).
- **Zod:** fresh bootstrap 17.529, cold gate 4.641, warm gate 4.611, switched gate 4.643, invalidated bootstrap 3.626, fresh workspace bootstrap 3.683. Full results: [typescript cold](2026-10-16-native-corrections/typescript-cold.json), [invalidation](2026-10-16-native-corrections/typescript-invalidation.json), [workspace](2026-10-16-native-corrections/typescript-workspace-fresh.json).

The Rust command was cargo test --locked --workspace; Python reported 297 passing pytest tests; TypeScript reported 544 passing tests in 58 Jest suites. These are single-run functional samples with fresh writable environments, not stable performance distributions. Host tool/download caches can be warm from preceding attempts. No realistic overhead claim or deferred 15% target is asserted.

The final binary SHA-256 is `587f3d365f1fe6d606e83cb9d5b659b243e3a6c8079e347d94904f1d90f77fc1`. [Provenance](2026-10-16-native-corrections/provenance.json) binds every src Rust file, Cargo.toml, Cargo.lock, the harness and binary by SHA-256, and records platform and relevant temporary/cache environment paths. Git, Rust/Cargo, Python, uv, Node and Yarn identities have separate compressed tool logs. The final product source, harness and binary hashes were compared with the measured files and matched. Every retained evidence directory passed SHA256SUMS verification.

Earlier runs and attempts are preserved rather than overwritten:

- [Pre-native-correction final](2026-10-16-final/summary.json): every scenario passed with the earlier exact source snapshot. The later native corrections changed source/binary identities, so the refreshed run above supersedes this evidence for the final implementation.

- [Initial attempt](2026-10-16-initial-attempt/summary.json): the host default temporary directory had a .git ancestor, so slots correctly refused it; direct ItsDangerous history import correctly rejected historical gitlinks.
- [Preview](2026-10-16-preview/summary.json): all phases succeeded on the earlier product preview; Rust and Zod used upstream history while Python used a source snapshot. It is superseded by the consistent exact-tree final run.
- [Disk-pressure preview](2026-10-16-disk-pressure-preview/summary.json): consistent snapshot preview completed Rust/Python, then TypeScript bootstrap failed when the root filesystem filled. Its typed failure and bounded process diagnostics are preserved.
- [Disk-pressure attempt](2026-10-16-disk-pressure-attempt/environment-error.txt): the final-binary retry exhausted the root filesystem during source checkout and could not write its normal summary; no gate pass is claimed. The successful final run placed workload data on the larger workspace filesystem and retained private validator temporary directories under /var/tmp.

The [manual native workflow](../../../.github/workflows/product-validation.yml) is prepared with explicit platform selection, a 20-minute job limit and 14-day artifact retention. It was not dispatched. macOS/Windows product ecosystem measurements remain pending; existing native S3 experiment results are different evidence, and Linux results do not establish Windows minimum-toolchain support.

The harness and implemented trust boundary are described in [experiments/product-validation](../../../experiments/product-validation/README.md). Candidate ecosystem code executes as test input in trusted-local environments; these slots are not OS sandboxes. Gate control scripts come from the separate trusted checkout and explicitly enter MAI_CANDIDATE_ROOT. The fixture-only trusted-trunk CAS used for invalidation is setup plumbing, not a product landing or ledger qualification.

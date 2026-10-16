# Product validator ecosystem measurements

This opt-in harness runs the actual `mai init`, `mai validator run`, and `mai workspace` commands against the same pinned ripgrep, ItsDangerous, and Zod source identities as S3. It measures a first validator bootstrap plus cold gate, repeated warm gate, candidate switch and return, deliberately failing candidate and recovery, declared bootstrap-input invalidation, and fresh workspace bootstrap. It does not dispatch workflows, enable receive or land changes, enforce the deferred 15% performance target, or substitute synthetic sleeps for ecosystem tests.

Build the product and choose a new disposable output directory on a filesystem with several gigabytes free. Validator private temporary directories must be outside any Git working tree because ripgrep tests discover ancestor `.git` entries. Set `TMPDIR` to a suitable native temporary root if the host default has a `.git` ancestor; product slot allocation rejects that layout:

```sh
cargo build --locked
python3 experiments/product-validation/run.py /tmp/mai-product-$(date -u +%Y%m%dT%H%M%SZ) --mai target/debug/mai
```

The runner requires Git, the Rust toolchain, Python 3.12, uv, Node, Yarn Classic 1.22.22, and `sh` on PATH. Native Windows needs Git Bash's `sh` on PATH; invoke the harness with `python` if `python3` is unavailable. A pre-fetched directory containing `rust`, `python`, and `typescript` ordinary Git clones may be supplied with `--fixtures PATH`; pinned commit IDs are still checked out. `--fixture rust|python|typescript` selects one fixture for diagnosis. Dependency downloads are part of fresh bootstrap; host download caches can affect timings and must be acknowledged when interpreting results.

All fixture sources, authority repositories, workspaces, raw product JSON, tool identities, source revisions, command lines, output logs and elapsed wall times remain under the new output root. An existing output path is refused. The harness catches fixture failures and continues with other fixtures, preserving their error logs. Each product subprocess also has an outer 15-minute limit; policy commands use the product's 10-minute process bound. Product runner timeout/cleanup evidence, rather than this outer bound, is the correctness evidence.

Product results record bootstrap and gate process timings separately. The harness requires fresh bootstrap on the cold sample, reuse on warm/switch/failure samples, and a new successful bootstrap after trusted declared-input invalidation; bootstrap stamp identities and timestamps are preserved alongside raw results. Wall time additionally includes policy/slot materialization and cleanup.

Each fixture is imported as a fresh repository containing the exact pinned upstream source tree; the harness checks Git tree identity and records an archive checksum. Upstream history is deliberately excluded: the pinned ItsDangerous history contains retired submodule gitlinks, which product import correctly rejects. This qualifies product execution over the pinned source trees, not import support for those upstream histories.

Policies are committed before authority initialization. Gate and bootstrap control scripts come from trusted trunk and explicitly enter `$MAI_CANDIDATE_ROOT` for ecosystem commands; ecosystem source code is candidate test input. The harness explicitly fetches candidates into disposable authority refs using local Git plumbing because product receive is unavailable. For the invalidation sample only, it explicitly advances the disposable fixture's trusted trunk with CAS; that is test setup, not a product landing or authority reconciliation qualification. Gate outcomes do not authorize landing.

The Rust fixture uses `cargo fetch --locked` and `cargo test --locked --workspace`, preserving `target`. Python uses the pinned fixture's test requirements, `flit_core==3.9.0`, an isolated `.venv`, editable installation and full pytest. TypeScript uses its checked-in Yarn lock, disabled install hooks and `test:ts-jest --runInBand`, preserving `node_modules`. These are trusted-local validation environments, not sandboxes. Network dependencies and host executable identities are recorded, not claimed to be hermetic.

Run `python3 experiments/product-validation/collect.py OUTPUT EVIDENCE` with a new destination to collect top-level logs (gzip), JSON and source/tool provenance with SHA-256 checksums; source clones, environments and caches are excluded.

The manual-only workflow accepts an explicit native platform and retains artifacts for 14 days. Broader native product ecosystem results remain pending until someone explicitly requests and dispatches it; existing S3 native evidence is a different artifact.

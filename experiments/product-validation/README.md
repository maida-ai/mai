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

## Small offline comparison

`synthetic.py` complements the ecosystem runner with a bounded synthetic comparison of plain Git clone plus the same trusted shell gate against manual mai validation. It requires only Python 3.9+, Git, `sh` and an already-built mai binary. It performs no downloads, installations, workflow dispatches or model calls. Select 1–20 iterations explicitly and a new output directory; a short path avoids native Unix socket/path limits in related product operations.

```sh
python3 experiments/product-validation/synthetic.py /tmp/mai-synthetic-run --mai /absolute/path/to/mai --iterations 3
python3 experiments/product-validation/collect.py /tmp/mai-synthetic-run /tmp/mai-synthetic-evidence
```

Each iteration creates a new independent Git clone and a new mai authority, alternating which side runs first. Setup timing is separate. Both sides use identical passing/failing candidate commits and one trusted gate; the plain-Git gate runs from the baseline policy checkout with an explicit candidate root. Four samples cover a fresh clone/slot (`cold`), reuse (`warm`), deterministic rejection (`failing`) and restored passing input (`after-failure`). No bootstrap or declared cache is configured; OS caches are not flushed. Reuse here does not demonstrate ecosystem cache effectiveness. Both sides verify expected gate truth; baseline checks its tree and unchanged worktree, while mai checks exact candidate/tree/base/policy evidence and unchanged trunk. Fixture candidates enter mai through local fetch plumbing, not product receive.

`provenance.json` records schema version 1, platform, CPU count, Python/Git/mai versions, binary and harness hashes, harness checkout identity/status, shell identity, fixed candidate trees and gate hash. The harness checkout is not asserted to be the supplied binary's source revision: retain the binary build provenance separately. `commands.jsonl` records command/cwd, monotonic elapsed seconds, expected and actual exit, and log filename. `samples.json` contains iteration, side, phase and elapsed seconds; validation samples add candidate/tree, expected outcome and verified status. `summary.json` records success or the failure diagnostic. Product JSON results and command logs remain alongside those files, including after failure. The existing collector copies only top-level evidence files and checksums, excluding fixture repositories.

Sample `elapsed_s` includes the surrounding correctness inspections as well as execution; `operation_elapsed_s` is just the shell gate on the Git side and the entire validator command on the mai side. These scopes differ deliberately and must not be plotted as equivalent engine timings. Use command records to decompose checkout, inspections and gate execution; use product result timings for its gate process. The plain baseline has no mai durability/policy machinery. This smoke experiment measures tiny-fixture overhead and gate truth only, not PR workflow latency, queue throughput, recovery MTTR, native cross-platform qualification or the external-user gate. A failed assertion invalidates the comparison. An outer 30-second subprocess timeout records a failed run; preserve the directory for inspection rather than treating it as product cleanup evidence.

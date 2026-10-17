# Explicit integration workflow and release smoke evidence

The published [walkthrough](../../../getting-started.md) completed on Linux using the release binary built from `58dd283c9bf7ea9905c0095b4f2dc94cfce30beb`. It initialized a fresh format-2 authority, manually validated its baseline, enabled bounded receive, pushed a parent and child through Git, explicitly integrated both, queried terminal results and created a fresh workspace containing both files. Alice's original clone HEAD remained unchanged. The [build provenance](build-provenance.json) records the integration owner's confirmed source revision and binary hash; later checkout changes are not inferred to belong to that build.

## What actually passed

Before integration, both `show` records had `state.kind=received`; the child had exactly the parent's generation in its dependencies. Named parent integration returned `landed` revision 1, followed by named child integration returning revision 2. Both final `show` records were terminal `landed` and identified the same commits as their integration results. The shell checked both files in the fresh clone and unchanged original HEAD. [Assertions](walkthrough-assertions.json), `walkthrough.log.gz` and the exact executed `walkthrough.sh.gz` retain this evidence. The script differs from the documented example only in the supplied absolute `MAI_BIN` path and an initial `set -eu`; execution used `TMPDIR=/var/tmp/mai-delivery-test` because the host `/tmp/.git` marker is unsuitable for validator temporary checkouts.

This is a finite happy-path dependency workflow. It does not demonstrate automatic scheduling, safe sync/replay, draft recovery, regeneration, remote identity, interrupted active-validator cleanup or power-loss safety. No real policy approval was performed. Independent product tests cover additional failure paths; this report does not substitute its happy path for those tests.

## Separate manual-validator comparison

A new three-iteration offline comparison ran against that same release binary without overwriting [earlier baseline evidence](../2026-10-17-synthetic/README.md):

```sh
TMPDIR=/var/tmp/mai-delivery-test python3 experiments/product-validation/synthetic.py /var/tmp/mai-delivery-test/synthetic-final --mai /absolute/path/to/release/mai --iterations 3
```

All 24 expected gate outcomes passed, including deterministic rejection followed by passing input. Exact candidate/tree/base/policy evidence matched; baseline worktrees and authority trunk were unchanged. Raw [samples](samples.json), [provenance](provenance.json), [validator JSON](validator-results.json), [summary](summary.json) and `commands.jsonl.gz` are retained with checksums. Original contents are not normalized; gzip timestamps are fixed at zero. `SHA256SUMS` covers the collected artifacts and supplemental build/assertion records, not this explanatory README. Detailed command logs and disposable repositories remain in the local run directory.

| Phase | Plain Git median whole sample (ms) | mai median whole sample (ms) | Samples per side |
| --- | ---: | ---: | ---: |
| Setup | 17.91 | 147.93 | 3 |
| Fresh clone/slot | 7.08 | 161.47 | 3 |
| Reused clone/slot | 6.67 | 149.90 | 3 |
| Expected gate failure | 6.83 | 124.82 | 3 |
| Passing input after failure | 7.01 | 151.06 | 3 |

The [harness timing scopes](../../../../experiments/product-validation/README.md#small-offline-comparison) differ: plain Git checkout/gate versus mai policy, materialization and evidence processing, with correctness inspections included. No equivalent PR-workflow speed ratio follows. The fixture has three tracked files and 228 bytes; no bootstrap or declared cache is configured, and OS caches were not flushed. The integration build had finished before execution, but other shared-host load was not controlled. Medians are diagnostic only; three samples do not support tail or significance claims. The different debug/release builds and uncontrolled load also prevent a causal performance comparison with the earlier report. No throughput, MTTR, scalability or external-user qualification is claimed.

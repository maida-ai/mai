# Offline synthetic validator smoke evidence

A Linux run of the [offline comparison](../../../../experiments/product-validation/README.md#small-offline-comparison) completed three iterations with identical synthetic passing/failing Git trees on both sides. All 24 gate samples matched their expected result, including failure followed by restored passing input. Exact mai candidate/tree/base/policy bindings matched, baseline worktrees remained unchanged and mai trunk never advanced. The documented foundation walkthrough also completed: authority initialization, ready status, two independent workspaces, passing manual validation and accepted shutdown request. This is manual-foundation evidence, not receive or landing qualification.

## Reproduce and identify

The harness revision was `37b3ec98e342eeaaa030bacfb4ba6a1246b42318`; [provenance](provenance.json) records its hash, the supplied debug binary hash, Git 2.53.0, Python 3.14.4, mai 0.0.1 and Linux/platform/shell identities. The integration owner confirmed that the supplied debug binary was built from clean baseline commit `7ea1b5ffa66e02b45f20273d85aa3d3fab5e59b5`, with compilation completed before integration cherry-picks. This is confirmed build provenance, not inferred from the subsequently changed integration checkout; [supplemental build provenance](build-provenance.json) records that confirmation separately from the original run artifacts. Its SHA-256 is `08db3fad236a36562489a28441f30604614e023ad35fc4d8b2dbe1bce8096b92`. Do not infer its build revision from the harness checkout revision. Use a new output path when reproducing:

```sh
TMPDIR=/var/tmp/mai-delivery-test python3 experiments/product-validation/synthetic.py /var/tmp/mai-delivery-test/synthetic-baseline --mai /absolute/path/to/built/mai --iterations 3
python3 experiments/product-validation/collect.py /var/tmp/mai-delivery-test/synthetic-baseline /var/tmp/mai-delivery-test/synthetic-baseline-evidence
```

The host's `/tmp/.git` marker means validator temporary checkouts under `/tmp` correctly reject as nested repositories. The explicit existing `/var/tmp/mai-delivery-test` directory was an environment accommodation, not a new product default or required path on other machines. The walkthrough used its published script with only `MAI_BIN` replaced by the supplied binary path and that `TMPDIR` environment setting. No network access, dependency installation or experiment workflow dispatch was used.

## Observations

| Phase | Plain Git median wall time (ms) | mai median wall time (ms) | Samples per side |
| --- | ---: | ---: | ---: |
| Setup | 17.11 | 193.68 | 3 |
| Fresh clone/slot | 6.53 | 169.70 | 3 |
| Reused clone/slot | 6.04 | 164.30 | 3 |
| Expected gate failure | 6.50 | 152.56 | 3 |
| Passing input after failure | 6.41 | 175.96 | 3 |

These are whole sample timings including correctness inspections. Plain Git performs checkout plus a shell gate; mai performs trusted policy/materialization, evidence and slot processing in addition to the gate. Setup is a clone on one side and authority initialization on the other. This is a comparison of different service responsibilities, not an equivalent end-to-end PR workflow or an isolated engine-overhead ratio. The median product-reported gate-process duration was 12 ms. No bootstrap or declared caches were configured; OS caches were not flushed. With three observations per phase, no tail-latency or statistical significance claim is warranted. Concurrent host work was not excluded, so these timings are diagnostic only.

## Retained evidence and limits

[Samples](samples.json) retain individual elapsed times, [validator results](validator-results.json) retain all exact product bindings and process outcomes, and [summary](summary.json) records successful completion. `commands.jsonl.gz` retains commands, return codes and log references; `walkthrough.log.gz` retains the walkthrough transcript. `SHA256SUMS` covers these selected artifacts. JSON/command content is copied without normalization; gzip timestamps are fixed at zero. Detailed per-command logs and disposable repositories remain in the local run directory; the separately collected bundle contains the logs but excludes repositories. No failed product run was omitted from this comparison.

The three-file, 228-byte fixture tests gate truth and repeatability only. It does not establish realistic agent throughput, recovery MTTR, cache effectiveness, power-loss durability, native macOS/Windows results, or the external-user gate. The manual fixture fetch is test setup and is not evidence of durable receive. These observations justify no performance optimization or broader scale claim.

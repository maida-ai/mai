# Two-agent continuation walkthrough

This Linux functional harness uses only fresh disposable repositories. Alice lands a parent and keeps a later local commit. Bob lands additional edits, including a change to the already accepted parent content. Continuation must preserve Bob's accepted edits while replaying only Alice's later commit. The harness then exercises a true overlapping integration rejection, a preserved conflicting continuation destination and a fresh-workspace correction that combines both intended edits.

It verifies source files, modes, refs, index and configuration remain byte-identical; ignored source material stays in the source and is not copied. Repeating continuation cannot overwrite the prior destination. All fixtures and failed destinations remain available for inspection. The harness stops only its own disposable authority engine.

```sh
cargo build --locked
mkdir -p /var/tmp/mai-continuation-fixtures
python3 experiments/continuation/operator_walkthrough.py \
  --repo "$PWD" \
  --binary "$PWD/target/debug/mai" \
  --temp-root /var/tmp/mai-continuation-fixtures \
  --output /var/tmp/mai-continuation-evidence-1
```

The output directory must not exist. Use a short scratch path outside any Git repository. Python 3, Git and Rust version reporting must be on PATH. The harness copies the executable and records its hash, source identity, command transcript and outcomes. One small fixture is functional evidence, not a workload comparison or production performance result. The product itself has no Python runtime requirement.

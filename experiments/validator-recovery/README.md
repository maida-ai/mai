# Disposable validator recovery walkthrough

This Linux-only functional harness creates a fresh seed, authority and agent clone. It freezes the supplied binary, interrupts only its own engine during an active gate, uses the public inspection/recovery commands, repeats recovery and verifies a fresh successful gate, one landing and preserved dirty/untracked agent bytes. It retains all fixture and evidence directories; it does not target an existing authority or clean real runtime state.

Build the selected source first. Use a short scratch path outside any Git repository to satisfy native socket length and validator temporary-directory constraints. The output directory must not exist. Python 3, Git, Rust version reporting and the built `mai` executable must be available.

```sh
cargo build --locked
mkdir -p /var/tmp/mai-recovery-fixtures
python3 experiments/validator-recovery/operator_walkthrough.py \
  --repo "$PWD" \
  --binary "$PWD/target/debug/mai" \
  --temp-root /var/tmp/mai-recovery-fixtures \
  --output /var/tmp/mai-recovery-evidence-1
```

The evidence records the source commit, tracked diff hash, copied binary hash, environment, command transcript and monotonic elapsed milestones from engine interruption. One tiny fixture is not a production MTTR, throughput, scale or comparative benchmark. A different build profile, host or storage can produce different times. No scheduled or expensive experiment workflow is added.

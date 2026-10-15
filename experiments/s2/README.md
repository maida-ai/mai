# S2 checkout-free merge experiment

This standalone dependency-free Rust harness qualifies Git CLI behavior for [#52](https://github.com/maida-ai/mai/issues/52). It does not implement the rewrite. See the [qualification report](../../docs/spikes/s2/README.md).

From the repository root, with Git >=2.40, Rust and Bash (Git Bash with native Rust/Git on Windows):

```sh
bash experiments/s2/run.sh .throwaway/s2-new
bash experiments/s2/collect.sh .throwaway/s2-new .throwaway/s2-evidence
```

Both output paths and the adjacent `.build` directory must be new. Windows fixtures must use a local drive; UNC paths are rejected. Everything disposable stays under `.throwaway/`. The runner compiles nine fast unit checks and executes the corpus before benchmarking. Failures stop the run and retain command output and fixture repositories. Collection copies only explicitly named harness logs and source hashes, never recursively traversing fixtures. No dependencies or network downloads are needed.

Integration uses a bare repository, `commit-tree`, index plumbing and `merge-tree --write-tree --merge-base=<base>`; it never checks out candidate files. A separate disposable clone supplies the equivalent-base cherry-pick oracle. Successful cases compare both independent expected snapshots and exact oracle tree IDs. Conflicts compare exit classifications and exact unmerged index paths, modes, stages and blob IDs, and retain Git's full conflict diagnostics. The dependent case compares a child's submitted-parent delta against an already squash-integrated and subsequently edited trunk; a deliberately wrong old base must conflict.

The benchmark generates exactly 100,000 tracked paths using plumbing, 100 directories and common `base\n` blob bytes. It changes opposite endpoints on two branches and retains 31 raw wall-clock samples, including process startup and evidence logging, without dropping a warmup. The median must be strictly below 100 ms. This is a small-content, warm-object-store Git CLI microbenchmark, not a realistic workload or cold-storage guarantee. Run on an otherwise idle host; serialize with S3 timings. The first failed threshold exits nonzero and requires ADR review, never an adjusted threshold.

Restricted `.units` fixtures exercise symbol/config/module identities, containment and explicit conservative file fallback. They are a synthetic grammar, not ecosystem parsers; they cannot qualify semantic independence. Receive and post-regeneration manifests come from actual Git diffs; renamed files are explicitly delete-plus-add. The regeneration fixture adds a generated file directly and does not qualify a real regeneration tool (S6 owns that evidence).

The trust fixtures exercise an administrator-owned `info/attributes` policy and configured constant-output driver in the bare authority. A malicious executable present in the candidate tree cannot replace the approved driver; invoking that checkout-relative file fails because integration never checks it out. Disabling the driver with approved `-merge` policy must conflict. This demonstrates the execution/trust boundary; it is not a sandbox or a claim that arbitrary existing drivers work without checkout.

The manual-only [workflow](../../.github/workflows/experiment-s2.yml) reuses the shared runner, explicit `linux`/`macos`/`windows`/`all` selection, 15-minute timeout and 30-day artifact retention. It must be registered on the default branch before manual dispatch. The reviewed 2026-10-15 native Linux/macOS/Windows run passed the implemented checks; see the [qualification report](../../docs/spikes/s2/README.md) and [go/no-go assessment](../../docs/spikes/s2/go-no-go.md). Full S2 acceptance remains blocked on complete object-manifest assertions. No workflow was dispatched during that evidence review.

For a non-timed complete corpus rerun after a harness-only change, use `S2_MODE=scenarios bash experiments/s2/run.sh .throwaway/s2-new-scenarios`. Provenance records the selected mode, and the absence of a `CRITERION` line means no timing qualification occurred in that run.

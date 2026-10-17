# Independent documented-workflow evidence

On 2026-10-17, independent QA executed the POSIX workflow in `docs/getting-started.md` against source commit `34d85b1228a81e4abd145b6f60e2df7410a2fe3a`. The debug binary was built with `cargo build --locked --offline --bin mai`, copied before execution, and verified by SHA-256 `8476b8aeb6228ac86c74cf54681e706eeccca624b523c202ee63f1ea9adf3b0b`. This is an observed Linux functional walkthrough, not a benchmark, native Windows result or power-loss qualification.

## Reproduction and observations

The documented shell block was extracted unchanged, prefixed with `set -eu`, and run with `bash -x` from a frozen directory containing `target/debug/mai`. `TMPDIR=/var/tmp/mai-delivery-test` was the only environment adjustment: this execution environment has a repository marker under `/tmp`, and validator private temporary directories deliberately reject repository ancestry. The documented `mktemp /tmp/mai-demo.XXXXXX` authority/workspace location was unchanged. The frozen binary directory and disposable repository are local inspection aids, not dependencies for reproducing the documented commands elsewhere.

[Metadata](run-metadata.json) records the source, binary and extracted-script hashes, tool versions and test-count breakdown. [Standard output](walkthrough.stdout.log) and [shell trace](walkthrough.trace.log) contain the exact commands and outputs. [Restart/retry evidence](retry-and-inspect.json) records every additional subprocess command, exit status and output, plus read-only database observations. Fixtures contain only generated demo content; no real repository or agent work was submitted.

| Observation | Actual result |
| --- | --- |
| Initial status | Ready, receipt disabled |
| Manual baseline gate | Success |
| Parent receipt | `alice-a34ce68b5e9a9ba85616c2eeb4bb7f6496c10a2f`, generation 1, received |
| Child receipt | `alice-07652b6ad73244d41cc1f5abb5a69f8bc2187bf5`, generation 1, received; dependency is the exact parent generation |
| Accepted parent | r1, `9077999b804567b6fc7a1486fb25c70fa5d0401f` |
| Accepted child | r2, `0e8a1f97a98b626cc7a4f7015fa03cdfd2170423`; sole parent is the r1 squash |
| Fresh workspace | Accepted r2 HEAD; exact `parent.txt` and `child.txt` contents present |
| Original workspace | Alice HEAD still equals submitted child; no rebase or update occurred |
| Restart | Initial engine PID 392799 stopped; subsequent status started PID 393970 |
| Retry operations | Two separate-process integrate retries and two identical commit pushes; all refs unchanged |
| Durable counts after retries | 2 submission generations, 2 evidence records, 2 landings; exactly 1 completed attempt per generation |
| Shutdown | Explicit stop requested for both walkthrough and retry engine runs |

The local required-check run contains **212 passed test entrypoints and 4 ignored helper entrypoints**, independently summed by target in the metadata. Ignored crash helpers are invoked explicitly by their parent tests; this count is not a count of independent scenarios or crash boundaries. Formatting and all-target Clippy with warnings denied also passed. Hosted native results must be checked separately at the exact CI head.

## Interruption coverage audit at this source snapshot

| Path | Actual interruption or action | What the evidence establishes |
| --- | --- | --- |
| Actual receive registration | Parent kills a separate test process at three callbacks: retained ref written; received note written; SQLite lifecycle saved | Identical retries converge to one generation/note without moving trunk. This invokes the real registration implementation; callbacks exist only in the test harness. |
| Actual serial landing bridge | Separate process exits with code 42 after passing evidence is persisted, before CAS; and after trunk CAS, before reconciliation | Reconciliation/retry yields one landing; the post-CAS case does not create a second attempt. These are abrupt process exits, not externally delivered kill signals. Validation has already finished at both points. |
| Landing adversarial checks | A test callback inserts a foreign lease or changes trunk after evidence, then the child exits after asserting refusal | Late ownership/base checks fail closed; these two cases are fault injections, not crash-boundary tests. |
| Authority reconciliation fixture | Parent kills a separate worker at four points: before CAS, after CAS, after notes, after state | Recovery converges twice against synthetic prebuilt passing evidence. This does not qualify those exact boundaries through the CLI landing implementation. |
| SQLite state WAL | Parent kills a separate writer after committed metadata and during another uncommitted transaction | Committed state survives while the incomplete transaction is discarded across repeated opens. |
| Lease WAL | Separate processes exit after committed acquisition and during an uncommitted transfer | Committed ownership and generation fencing survive; incomplete transfer does not replace the previous owner. |
| Receive CLI acknowledgment | Real push succeeds, engine is killed, a new engine starts, identical push retries | Acknowledged retention and identity survive actual owner termination. |
| Receive lost response | Complete Unix registration frame is written, client socket closes without reading response/ACK, then a real push retries | Lost response does not duplicate registration. This is a disconnect, not a process crash. |
| Orphan-ref/missing-note QA fixture | Ref is manually installed before registration; after acknowledgment, notes are deleted and actual engine restarts twice | Reconciliation of constructed durable prefixes works. The prefix construction itself is synthetic, not a kill at the corresponding write. |
| Approval exclusion | Complete denial IPC request arrives while trusted gate waits on an external test marker | Denial remains queued until publication completes; no approval write interleaves with the final gate/CAS sequence. No crash is involved. |

## Remaining recovery and usability limits

There is no tested power-loss or storage-fault guarantee here. Finite process kill/exit tests do not prove filesystem flush ordering or recovery under disk-full, I/O failure or object corruption at every write. The actual landing bridge has no dedicated interrupted callback after its notes write or final SQLite completion; those later points currently have synthetic authority-fixture coverage.

Killing the engine while its validator is actively running is distinct from the post-validation landing checkpoints above. Descendant-process cleanup and safe automatic reclamation of the resulting occupied validator slot are not established by these tests. Current guidance correctly preserves the claim for trusted inspection rather than guessing that a missing PID proves quiescence. A supported inspection/recovery command would improve recovery time, but must not delete claims without validating descendants and workspace state.

The serial engine services other requests after the current integration finishes. A status, show or approval request can therefore time out while a long gate runs; timeout is not cancellation. An explicit busy/in-progress response or independently readable progress snapshot would improve diagnosis. At this snapshot there is no actual CLI integration-response timeout regression; the receive disconnect test must not be described as covering that different operation.

There is no safe post-squash sync, automatic scheduler, or superseding receive generation in this increment. The documentation accurately directs new work to a fresh workspace while preserving old clones and subsequent work. The opening build-guide sentence listing commands understates the later demonstrated receive/integrate/lease capabilities, but the runnable workflow itself succeeded without correction.

# S1 receive transport qualification

Status: partial Linux evidence; **S1 is not qualified and #51 remains open**. This is a disposable experiment for [issue #51](https://github.com/maida-ai/mai/issues/51), not a production implementation or an accepted design. No product source, dependencies, schema, defaults or ADR acceptance status changes.

## Reproduce

Run from the repository root on Linux or macOS with Rust and Git >=2.40:

```sh
bash docs/spikes/s1/run.sh .throwaway/s1-my-run
```

The output and adjacent `.build` directories must not exist. The script compiles directly with `rustc`, runs two policy unit tests, then runs real Git experiments. It uses only Rust's standard library, Git CLI and `/bin/sh`; no Cargo dependency or container image is downloaded. It binds an ephemeral loopback port. Existing repositories, including `.throwaway/dummy_repo`, are not opened or modified. Fixtures remain available for inspection, and hooks point to the binary in the adjacent build directory. Nothing is automatically deleted.

The harness is Unix-only: native Windows needs a port or equivalent independently recorded experiment. A Linux run inside WSL would not establish native Windows support. macOS compatibility is intended but untested. The successful observed run is recorded in [evidence](evidence/2026-10-15-linux/). The observation date is UTC; it was October 14 locally in America/Los_Angeles.

## Fixture and evidence boundaries

`main.rs` implements throwaway `proc-receive`, pre/post-receive hooks, a separate registration subprocess and policy probes. `transport.rs` provides a loopback-only CGI bridge to `git http-backend`. `scenarios.rs` constructs five isolated authority/client fixtures and asserts their resulting refs, queue receipts, histories and working-tree contents. Two agent clones are created per fixture; most pushes originate from Alice, while Bob also tests credential-free HTTP rejection. These are tiny generated repositories with text files, not an application workload.

The subprocess deliberately clears Git location/quarantine variables. During pre-receive, fresh objects are visible through the hook's inherited environment but not through the clean subprocess. During proc-receive, the clean subprocess can read the objects and create retention refs before `ok`. The audit log verifies visibility → retention → queued receipt → hook acknowledgment order. This qualifies the observed Git boundary, not power-loss durability.

The queue is a directory of `queued` files, not SQLite. Change refs use the submitted object ID, not the proposed numeric product IDs. Registration is serialized by the runner, not a long-lived engine/IPC service. A fault marker makes registration return an error after ref creation; another exits the hook after registration but before a reply. A rendezvous also lets the runner kill its own file-transport push client before releasing the reply. These cover particular process/error windows, not all S7 crash points or concurrent registrations. Queue files are not fsynced; no cross-store transaction or power-loss guarantee is claimed.

The HTTP bridge accepts one public synthetic credential and maps it to Alice. It does not read real credentials. A fixed synthetic incident authorizes a scoped severe request only for authenticated Alice; no user or real grant is involved. This demonstrates the distinction between client-declared identity and server-provided identity, not production authentication, token storage, issuance, expiry/revocation or grant-role acceptance. File identity can be spoofed deliberately. Exact `mai.touches` grammar remains undecided; the fixture's simple grammar and rejection limits (16 options, 1024 bytes per option, 4096 bytes total) are experimental, not defaults. The receive packet reader collects the option list before policy rejection; this is not a memory-exhaustion qualification.

`mai.wait=true` is accepted as metadata while registration returns with only a queued receipt and unchanged trunk; no validator exists to wait on. This demonstrates receive/admission separation, not a product wait implementation. The HTTP bridge handles only small Content-Length requests, disables atomic-push advertisement and has no chunked-body support. HTTP disconnect, concurrent requests, malformed framing, and resource-exhaustion qualification remain outside this run.

Sync uses explicit simulated outcomes and handcrafted squash commits installed by local CAS. No gate or acceptance engine runs. A conservative helper refuses dirty, staged, untracked and ignored state before `git rebase --onto`; the observed cases cover dirty, untracked and ignored files. Pending/rejected parent outcomes are supplied by the fixture. The second squash sync supplies an explicit old-submission → rewritten-local-child boundary. Persisting/recovering that mapping, dependency rejection propagation, sync conflicts and explicit autostash recovery remain unqualified.

## Results and platform matrix

| Surface | Observed result | Scope |
| --- | --- | --- |
| Linux x86_64, Git 2.43.0 | 115 integration assertions and 2 unit tests passed | File transport and host loopback smart HTTP |
| Linux container client | Pending | Docker Desktop socket unavailable; no container experiment ran |
| macOS, Git >=2.40 | Pending | No macOS host available in this session |
| Native Windows | Pending / untested | No Windows host; Unix harness needs adaptation |
| Exact Git 2.40 minimum | Pending | Installed Git is 2.43.0; no minimum-version claim |

The host reported Linux 7.0.0-34-generic x86_64 and rustc 1.97.0 (2d8144b78 2026-07-07). `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, and `cargo test --all-targets --locked` passed for the unchanged existing product (1 library test and 33 end-to-end tests). Those tests do not qualify the rewrite. The standalone spike is separately formatted, compiled with `-D warnings`, and exercised by `run.sh`.

| Issue requirement | Evidence / remaining work |
| --- | --- |
| pkt-line negotiation, options, per-ref status, virtual submission, owned draft | Passed on file and host HTTP; protected/internal/cross-agent/unknown submission refs rejected; draft deletion rejected |
| Host and container over loopback HTTP | Host passed; container pending |
| Linux/macOS and Windows record | Linux observed; macOS and native Windows explicitly untested |
| Separate-engine visibility and refs before acknowledgment | Passed with clean subprocess and ordered audit; queue/storage durability not established |
| Engine down, retry, mixed refs, lost replies | Passed on file and host HTTP; a mixed push may fail overall while its allowed submission was admitted |
| Client disconnect | File client killed after registration, before reply; retained submission survives and retry is idempotent; HTTP/container disconnect pending |
| Incoming-ref plus post-receive fallback | Healthy path passed; engine-down transport succeeded without admission; incoming ref retained; explicit reconciliation registered once |
| Squash sync, later work, pending/rejected parent, dirty/untracked | Fixture-level assertions passed, including second dependent squash and ignored files; no production lifecycle or mapping implementation claimed |
| `mai.wait` cannot wait for validation | Push returned with queued receipt and unchanged trunk; no validator running |
| `mai.fixes`, `mai.priority`, `mai.touches` | Valid low/touches accepted, malformed/oversized/duplicate options and unauthenticated elevated/unknown-incident requests rejected; one synthetic authenticated fix accepted; grant lifecycle pending |
| Reproducible scripts, versions, short go/no-go report | This directory and recorded logs; missing qualification remains pending |

## Findings requiring design review

1. **Fallback transport success does not establish queue admission.** With the registration process unavailable, post-receive reports `RETAINED_PENDING_REGISTRATION`, but Git exits successfully and the incoming ref remains. Retrying the identical ref is “up to date” and does not invoke post-receive again. Startup/background reconciliation must scan retained incoming refs; a retry alone cannot be the recovery mechanism. The fixture uses immutable per-submission incoming refs and retains them even after registration; retention cleanup is not qualified.
2. **Mixed pushes need per-ref outcomes.** In proc-receive, an allowed virtual submission is retained/queued while a trunk update in the same push is rejected. The overall push exits nonzero. Clients must inspect per-ref status or query submission identity before retrying. Atomic pushes are deliberately not advertised by this experiment.
3. **Dependent sync needs local rewrite mapping.** After syncing a squash-landed parent, the pending child's local ID changes. When that child later lands, the original submitted ID is no longer an ancestor of the local tip. An explicit mapping to the rewritten local child lets a second `rebase --onto` replay only the subsequent commit. The experiment rejects the stale boundary without changing HEAD.
4. **Pre-receive is too early for the clean registration process in this setup.** Its inherited quarantine visibility must not be mistaken for visibility in a separate engine. The tested proc-receive phase provides object visibility and permits retention before acknowledgment on Git 2.43.0.

**Go/no-go:** continue S1 qualification; no M0/product go decision. The Linux observations support the tested protocol approach, subject to the fallback reconciliation and sync-mapping constraints. Container and macOS evidence remain required; record a Windows outcome, qualify the supported Git minimum and review the fixture's limitations before acceptance. Do not close #51 or mark ADRs accepted based on this report.

Git's [hook protocol documentation](https://git-scm.com/docs/githooks#_proc_receive), [post-receive contract](https://git-scm.com/docs/githooks#_post_receive), and [quarantine description](https://git-scm.com/docs/git-receive-pack#_quarantine_environment) guided the experiment. The conclusions above are tied to the recorded local runs rather than inferred solely from documentation.

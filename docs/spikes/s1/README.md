# S1 receive transport qualification

Status: **the recorded spike evidence supports closing #51 after maintainer review and merging these evidence entries**. Linux, macOS and native Windows host runs passed; earlier Linux Docker Desktop evidence covers container clients. This is a recommendation, not an issue closure or M0 approval. This is a disposable experiment for [issue #51](https://github.com/maida-ai/mai/issues/51), not a production implementation or an accepted design. No product source, dependencies, schema, defaults or ADR acceptance status changes.

## Reproduce

Run from the repository root on Linux or macOS with Rust and Git >=2.40:

```sh
bash experiments/s1/run.sh .throwaway/s1-my-run
```

The output and adjacent `.build` directories must not exist. The script compiles directly with `rustc`, runs standalone policy/path unit checks, then runs real Git experiments. It uses only Rust's standard library, Git CLI and `/bin/sh`; no Cargo dependency or container image is downloaded. It binds an ephemeral loopback port. Existing repositories, including `.throwaway/dummy_repo`, are not opened or modified. Fixtures remain available for inspection, and hooks point to the binary in the adjacent build directory. Nothing is automatically deleted.

The harness now lives under [`experiments/s1/`](../../../experiments/s1/README.md), with native Windows path/hook adaptations and a manual CI workflow. Native Linux, macOS and Windows execution is now recorded in the [CI evidence index](evidence/README.md). The final all-platform run uses source commit `2153c83546830e8cfbda2e99a122914a2e191038`, including the Windows accepted-socket blocking-mode fix. Both earlier Windows failures are retained alongside the successful reruns; they were harness HTTP failures, not evidence that receive policy passed. A Linux run inside WSL would not establish native Windows support. The initial host run is recorded in [host evidence](evidence/2026-10-15-linux/), and the follow-up run including containers is recorded in [container evidence](evidence/2026-10-15-container/). Each source checksum manifest describes its historical source snapshot, not every later revision. The observation date is UTC; it was October 14 locally in America/Los_Angeles.

To include Docker Desktop container clients, pull the recorded image once, then select its immutable digest (the runner itself uses `--pull=never`):

```sh
docker pull alpine/git@sha256:ce84fd6a057b4dc5cf5009da2dd832f1c6d096c1984bf50496cf3c256402264d
S1_CONTAINER_IMAGE=alpine/git@sha256:ce84fd6a057b4dc5cf5009da2dd832f1c6d096c1984bf50496cf3c256402264d bash experiments/s1/run.sh .throwaway/s1-container-run
```

This optional mode adds two fixtures. Each container mounts only its disposable client directory as `/work`, runs with that directory owner's UID/GID, and is removed on exit. Clone and push use the image's Git over HTTP; the host prepares subsequent fixture commits and checks authority state. The authority stays on the host and is never mounted into a container. Docker Desktop's [host service networking](https://docs.docker.com/desktop/features/networking/) routes `host.docker.internal` to the host receiver, which stays bound to `127.0.0.1`. No host-network mode, port publication, daemon configuration change or production credential is used. This topology was tested on Docker Desktop for Linux; native Docker Engine bridge networking is not qualified. The pulled image and bind-mounted fixtures remain after the run.

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
| Linux container client | 52 additional assertions passed; 167 total plus 2 unit tests | Docker 29.8.2; digest-pinned Git image; host loopback HTTP proc-receive and fallback |
| [CI Linux x86_64, Git 2.55.0](evidence/2026-10-15-linux-run-38685798444-attempt-1/) | 115 assertions and 4 unit checks passed | Ubuntu 24 runner; file and host loopback HTTP |
| [CI macOS ARM64, Git 2.56.0](evidence/2026-10-15-macos-run-38685798444-attempt-1/) | 115 assertions and 4 unit checks passed | macOS 26 runner; file and host loopback HTTP |
| [CI native Windows x86_64, Git 2.55.0.windows.5](evidence/2026-10-15-windows-run-38685798444-attempt-1/) | 115 assertions and 4 unit checks passed | Windows runner with native MSVC Rust and Git Bash; no WSL; earlier HTTP harness failures preserved |
| Exact Git 2.40 minimum | Unverified | Tested versions satisfy #51's >=2.40 criterion; no exact-floor compatibility claim |

The host reported Linux 7.0.0-34-generic x86_64 and rustc 1.97.0 (2d8144b78 2026-07-07). `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, and `cargo test --all-targets --locked` passed for the unchanged existing product (1 library test and 33 end-to-end tests). Those historical product checks do not qualify the rewrite. The standalone spike is separately formatted, compiled with `-D warnings`, and exercised by `run.sh`.

| Issue requirement | Evidence / remaining work |
| --- | --- |
| pkt-line negotiation, options, per-ref status, virtual submission, owned draft | Passed on file, host HTTP and container HTTP; protected/internal/cross-agent/unknown submission refs rejected; draft deletion rejected |
| Host and container over loopback HTTP | Both passed in the recorded Docker Desktop topology |
| Linux/macOS and Windows record | Native host runs passed on all three platforms; versions and outcomes recorded in the linked snapshots |
| Separate-engine visibility and refs before acknowledgment | Passed with clean subprocess and ordered audit; queue/storage durability not established |
| Engine down, retry, mixed refs, lost replies | Passed on file, host HTTP and container HTTP; a mixed push may fail overall while its allowed submission was admitted |
| Client disconnect | File client killed after registration, before reply; retained submission survives and retry is idempotent; HTTP/container disconnect pending |
| Incoming-ref plus post-receive fallback | Healthy path passed; engine-down transport succeeded without admission; incoming ref retained; explicit reconciliation registered once |
| Squash sync, later work, pending/rejected parent, dirty/untracked | Fixture-level assertions passed, including second dependent squash and ignored files; no production lifecycle or mapping implementation claimed |
| `mai.wait` cannot wait for validation | Push returned with queued receipt and unchanged trunk; no validator running |
| `mai.fixes`, `mai.priority`, `mai.touches` | Valid low/touches accepted, malformed/oversized/duplicate options and unauthenticated elevated/unknown-incident requests rejected; one synthetic authenticated fix accepted; grant lifecycle pending |
| Reproducible scripts, versions, short go/no-go report | Scripts under `experiments/s1/`, linked here per the repository convention; platform snapshots and assessment under `docs/spikes/s1/` |

## Findings requiring design review

1. **Fallback transport success does not establish queue admission.** With the registration process unavailable, post-receive reports `RETAINED_PENDING_REGISTRATION`, but Git exits successfully and the incoming ref remains. Retrying the identical ref is “up to date” and does not invoke post-receive again. Startup/background reconciliation must scan retained incoming refs; a retry alone cannot be the recovery mechanism. The fixture uses immutable per-submission incoming refs and retains them even after registration; retention cleanup is not qualified.
2. **Mixed pushes need per-ref outcomes.** In proc-receive, an allowed virtual submission is retained/queued while a trunk update in the same push is rejected. The overall push exits nonzero. Clients must inspect per-ref status or query submission identity before retrying. Atomic pushes are deliberately not advertised by this experiment.
3. **Dependent sync needs local rewrite mapping.** After syncing a squash-landed parent, the pending child's local ID changes. When that child later lands, the original submitted ID is no longer an ancestor of the local tip. An explicit mapping to the rewritten local child lets a second `rebase --onto` replay only the subsequent commit. The experiment rejects the stale boundary without changing HEAD.
4. **Pre-receive is too early for the clean registration process in this setup.** Its inherited quarantine visibility must not be mistaken for visibility in a separate engine. The tested proc-receive phase provides object visibility and permits retention before acknowledgment on the recorded Git 2.43.0, 2.55.0 and 2.56.0 host versions (including Git for Windows 2.55.0.windows.5).

**Go/no-go recommendation:** go for the tested S1 receive approach, subject to the fallback reconciliation, per-ref outcome and dependent-sync mapping constraints above. The evidence is sufficient for #51 as written: host and container HTTP coverage, Linux/macOS on Git >=2.40, recorded native Windows results, the specified failure/retry cases, fixture sync and bounded typed option policy. Merge and review these entries before closing the issue. No issue checkbox, ADR acceptance status or M0/product gate is changed by this report.

The issue asks for a throwaway transport qualification, not a production implementation. Exact Git 2.40 execution, HTTP/container client-kill cases, macOS/Windows container hosts, native Docker Engine networking, real grant lifecycle, concurrent registration, SQLite/IPC/power-loss durability and complete sync recovery remain unverified. They are not claimed by the recommendation. Client disconnect was exercised using file transport; container HTTP was exercised on Linux Docker Desktop. If maintainers intend the acceptance criterion to require every transport on every platform, that broader cross-product is not covered and must remain open. The stated Git >=2.40 runs do not by themselves establish support for the exact oldest version.

The harness prints a conservative fixed `S1 NOT QUALIFIED` footer because one run cannot establish aggregate qualification. That historical output is preserved verbatim; this report combines the separate platform and container observations. All successful CI runs match the historical host run's ordered 115 assertions; the final three-platform run adds the fourth unit regression for delayed HTTP requests on nonblocking accepted sockets. No experiments were rerun to create these snapshots.

Git's [hook protocol documentation](https://git-scm.com/docs/githooks#_proc_receive), [post-receive contract](https://git-scm.com/docs/githooks#_post_receive), and [quarantine description](https://git-scm.com/docs/git-receive-pack#_quarantine_environment) guided the experiment. The conclusions above are tied to the recorded local and CI runs rather than inferred solely from documentation.

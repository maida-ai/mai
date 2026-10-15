# S3 qualification: warm validators

Status: **NO-GO / evidence pending**. The harness is research infrastructure, not a shipped validator implementation. Neither a green harness run nor a merged PR accepts M0. Refs #53.

## Fixtures and gates

| Ecosystem | Pinned source commit | Dependency setup | Native gate |
| --- | --- | --- | --- |
| Rust: ripgrep 14.1.1 | `4649aa9700619f94cf9c66876e9549d83420e16c` | `cargo fetch --locked` | `cargo test --locked --workspace` |
| Python: ItsDangerous 2.2.0 | `096c8d42545d3b68ea21a4f890fb2b2d8979c0bd` | uv Python 3.12 venv, pinned upstream `requirements/tests.txt`, flit_core 3.9.0, editable install without build isolation | environment Python `-m pytest` |
| TypeScript: Zod 3.24.2 | `e30870369d5b8f31ff4d0130d4439fd997deb523` | Yarn 1.22.22 `install --frozen-lockfile --non-interactive --ignore-scripts` | Yarn `test:ts-jest --runInBand` |

Zod lifecycle hooks are disabled during dependency installation to avoid repository Git-hook setup; native package/test resolution still uses upstream lock and scripts. Python patch versions and installed Cargo/Node/uv versions are recorded, so cross-host comparisons must account for these inputs. First Cargo gate compilation is recorded separately from dependency fetching.

## Acceptance mapping

| Issue criterion | Evidence produced | Qualification limits |
| --- | --- | --- |
| Three real pinned repositories | `*-setup.log`, `tools.log`, wrapper `sources.sha256` | Network or bootstrap failure remains a failure; no synthetic substitute |
| Fresh bootstrap, raw gate, warm switching | `*-samples.tsv`, five raw/warm pairs, cold gate | Timing requires otherwise idle host; finite samples do not establish broad workload performance |
| Each warm overhead <15% | `*-result.log`, strict threshold independently for each ecosystem | Failure never relaxed or averaged across ecosystems |
| Bootstrap invalidation | Input byte comparison, changed manifest, bootstrap rerun | Harmless input edit exercises invalidation; does not qualify every dependency/toolchain invalidator |
| Two concurrent slots | `*-concurrent-0.log`, `*-concurrent-1.log`, independent directories and environment | Cooperative variables are not security boundaries |
| Ports and collision resolution | `isolation.log`, real bind collision and ephemeral retry | Dedicated fixture; upstream gates do not themselves open application servers |
| Cache reuse cannot hide failure | Native gate over deliberately invalid source | Covers one representative source failure per ecosystem |
| Ignored/untracked residue | Real Git clean and post-clean assertions | Named venv/node_modules retained intentionally; arbitrary cache poisoning not proven absent |
| Cancellation and process-group cleanup | `cancellation.log`, spawned child plus descendant heartbeat | Unix process groups; Windows taskkill path awaits native execution; escaped/detached descendants not qualified |
| COW and safe fallback | `isolation.log`, copy mutation independence | Linux/macOS capability probe; Windows COW probe pending, safe copy supported |
| Platform support | Native conditional process handling and Git Bash wrapper | Only Linux local execution planned here; macOS/Windows qualification pending |

Reproduction and artifact boundaries are documented in the [harness README](../../../experiments/s3/README.md). All raw generated evidence remains under the worktree `.throwaway/` until reviewed and deliberately promoted. External GitHub runs are not inferred from local results. Overlay/lazy work remains deferred.

## Evidence collection

Qualification evidence will be collected from manual CI workflow runs after this PR is merged. This preparation includes no local evidence snapshots or measured-result claims. Review the resulting artifacts separately before accepting the spike; no workflow is dispatched by this change.

The ripgrep temporary-directory fixture assumes no Git ancestor; worktree-local temporary directories can violate that assumption. Resolve or explicitly record that limitation before interpreting Rust measurements. Bootstrap invalidation currently detects manifest byte changes rather than a real dependency-resolution change.

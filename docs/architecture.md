# mai architecture

mai coordinates 3–30 coding agents in one repository on one machine. This is the product focus, not a measured capacity guarantee. It is a standalone Apache-2.0 Rust CLI backed by Git and SQLite; GitHub mirroring is optional.

## Five invariants

1. **One acceptance ledger:** first-parent main defines accepted revisions; `r0` is the immutable import baseline. Private work does not allocate `rN`.
2. **Publish against a known parent:** recheck live leases and compare-and-swap main with Git's ref transaction. Never overwrite accepted changes on a stale assumption.
3. **Prove acceptance from history:** receipt refs require main ancestry verification. Git and SQLite are not one transaction; neither an auxiliary ref nor a SQL row proves acceptance alone.
4. **Preserve work:** rejection and recovery must retain drafts and distinguish earlier acceptance from unsubmitted edits. Current reset/recovery defects still violate this requirement.
5. **Describe validation honestly:** disjoint paths can break each other. Today's `--check` tests the workspace only; integrated validation must bind to the exact accepted candidate tree and parent.

## Current pipeline

`src/core.rs` owns coordination, `src/storage_git.rs` executes Git, `src/protocol.rs` defines receipts, and `src/main.rs` presents the CLI. `.mai/repo.git` stores source and authority refs; `.mai/metadata.sqlite3` stores leases and workspace registration. Detached agent worktrees share Git objects and refs.

1. Lock the agent workspace. Read HEAD as its base, attempt receipt recovery, and run any explicitly requested workspace check.
2. Stage nonignored files, compare changed paths with trunk, and build an immutable candidate outside the gate. Equal or ancestor/descendant paths conflict; renames are delete/add pairs.
3. Acquire SQLite's write gate, recheck main and leases, and CAS main plus the receipt. If main moved, rebuild; stop after 64 attempts.
4. Release the gate and reset the workspace. Reset failure reports `workspace_updated:false` without undoing acceptance.

Workspace creation currently holds the gate across checkout. Per-agent locks coordinate mai commands, not ordinary editing; quiesce workspaces during submit/refresh.

## Durability and recovery

Receipts are checked against main; missing receipts can be reconstructed from first-parent history and agent/base trailers. Process-kill tests cover specific recovery windows. Object fsync and ref transactions do not establish filesystem-wide power-loss atomicity.

`recovered:true` identifies an earlier acceptance and can return before inspecting new edits. Never report it as acceptance of current work. Inspect accepted content, preserve the complete remaining draft outside the workspace, and reconcile its base before continuing. `workspace_updated:false` also requires reconciliation. See [CLI recovery and precautions](cli.md#submission-results-and-recovery).

Refresh rejects dirty workspaces; explicit discard destroys work. Ordinary refresh/post-submit reset can also overwrite ignored files at incoming paths. Local commits move HEAD away from the submit base; committing on main bypasses mai through shared refs. Until fixed, avoid commits, amend, rebase and branch switching in mai workspaces. Preserve valuable ignored/untracked files separately.

## Planned direction and boundaries

Prioritize explicit workspace bases and guarded authority refs, dirty updates, exact integrated checks, truthful recovery and scoped capture. Move candidate assembly inside the gate while keeping staging, long checks, checkout and bootstrap outside. Checked candidates require revalidation if their parent changes. Measure group commit separately; opt-in textual merging follows integrated validation. After the external-user gate, plan shared symbol/key-path units, live intent, explicit context checkpoints/resume and a private draft DAG. Models never approve or remove conflicts; raw context is not shared through awareness. [ADRs 0003–0009](README.md#architecture-decisions) record proposals and unresolved policies.

mai scrubs inherited Git environment variables and disables hooks, fsmonitor, external diff and textconv for relevant operations; it checks workspace symlinks and common-directory identity. Worktrees are not sandboxes, declared agent IDs are not authentication, and trusted Git filters/helpers can execute. Ports, databases and test-process isolation belong to callers.

[Project direction](project.md) · [Performance](performance.md) · [Foundation ADR](adr/0001-foundation.md) · [Private work ADR](adr/0002-changesets-and-publication.md) · [Agent-native direction and futures](agent-native-capabilities.md)

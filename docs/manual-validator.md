# Manual trusted validator

`mai validator run PROJECT CANDIDATE` validates an exact commit already present in `PROJECT/authority.git`. This manual command does not receive, queue, merge or land changes; explicit `mai integrate` is a separate operation. Current `refs/heads/trunk` selects the trusted configuration and executable policy checkout; the candidate selects test input. Trunk is never updated by validation or approval.

```sh
mai validator run /path/to/project COMMIT --json
mai approve /path/to/project COMMIT --actor maintainer --reason 'Reviewed policy change'
mai approve /path/to/project COMMIT --actor maintainer --reason 'Gate change rejected' --deny
```

## Implemented policy subset

The trusted trunk must contain a regular `mai.toml` with this strict version-1 subset. Numbers are explicitly selected settings, not recommended or performance-qualified defaults. Unknown fields, unsupported versions and unsupported protected patterns reject. The broader [configuration proposal](configuration.md) remains unimplemented outside this subset.

```toml
version = 1

[check]
program = "sh"
script = "scripts/gate.sh"
args = []
executable_dependencies = ["scripts/helper.sh"]
inputs = []
tools = []
timeout_ms = 60000
output_tail_bytes = 8192

[validators]
slots = 2
port_base = 24000
ports_per_slot = 10
caches = [".cache"]

[validators.bootstrap]
program = "sh"
script = "scripts/bootstrap.sh"
inputs = ["package-lock.json"]
tools = ["node", "npm"]
timeout_ms = 60000
output_tail_bytes = 8192

[workspaces.bootstrap]
program = "sh"
script = "scripts/bootstrap.sh"
timeout_ms = 60000
output_tail_bytes = 8192

[policy]
protected = [".github/**", "AGENTS.md"]
```

`check`, `validators`, slot count, port range, command program/script/time/output limits are explicit. `args`, `executable_dependencies`, `inputs`, `tools`, caches and protected paths default to empty lists. Both bootstrap sections are optional. All script, executable dependency and declared input paths must name regular tracked files through components without symlinks in trusted trunk. Paths are normalized repository-relative paths; `.git` and backslash paths reject. Protected patterns support exact paths and `directory/**`; other glob forms reject. Output tails must be positive and at most 1 MiB. Cache directories must not overlap tracked content, each other or Git metadata. Changing pool settings requires a fresh explicitly managed engine-owned pool; mai does not silently change existing allocations.

## Executable trust boundary

Commands execute with their working directory in a separate trusted trunk checkout. The host executable is resolved from absolute PATH entries to a canonical path; its first argument is the absolute trusted script, followed by `args`. The environment supplies `MAI_POLICY_ROOT` for trusted executable dependencies and `MAI_CANDIDATE_ROOT` for the independent candidate clone. A shell gate can use:

```sh
# scripts/gate.sh, retained from trusted trunk
sh "$MAI_POLICY_ROOT/scripts/helper.sh"
cd "$MAI_CANDIDATE_ROOT"
cargo test --locked
```

Relative helper invocations initially resolve from the trusted policy checkout. After changing directory to candidate input, scripts must explicitly use `MAI_POLICY_ROOT` for executable policy dependencies. Candidate code and candidate build/test tools execute as test input and can run arbitrary native code. Neither validator slots nor the separate control checkout are an OS sandbox or a hostile multi-user boundary. Trusted policy authors must distinguish control dependencies from intentionally executable candidate test input; declarations do not analyze arbitrary scripts or dynamically discover their dependencies.

The runner supplies a private temporary directory, `MAI_SLOT` and `MAI_PORT_BASE`, bounds output and time, and terminates native process descendants. Incomplete cleanup or abrupt owner termination leaves a persistent occupied claim and prevents automatic slot reuse. Preserve interrupted state for trusted inspection. A PID's absence does not prove descendant cleanup.

Successful validator bootstrap is reusable only when its serialized configuration, trusted script/dependency contents, candidate declared input contents and canonical executable tool identities match, and every explicitly retained cache directory still exists. Version-probe results and executable contents contribute to tool identity. Cache content is trusted service state, not cryptographically verified dependency output. Missing caches invalidate reuse; zero configured caches rerun bootstrap each time. Previous state is invalidated before rebuilding, and failures never mark bootstrap successful. Only explicitly configured cache paths survive candidate cleanup. Successful bootstrap and gate execution must leave tracked candidate content unchanged. mai compares it with the bound tree using trusted authority objects and a fresh service-owned index; a mutation returns `failure`, suppresses the gate after bootstrap, and never records a successful bootstrap stamp or validation for the original tree. Generated untracked cache/build output remains allowed.

## Approval and evidence

`mai.toml`, every declared command script, executable dependency and input, and explicit protected paths are protected automatically according to trunk policy. Both sides of renames/deletions are considered. Candidate policy narrowing cannot remove these protections. A protected change returns `approval_required` until explicitly approved; a recorded denial returns `policy_denied`. A later explicit approval can supersede a denial for the same binding. Approval permits the normal trusted gate to run and never counts as a passing gate or authorizes landing.

Approvals bind the canonical native authority path, exact candidate commit/tree, current base trunk and full trusted policy commit. Every changed identity invalidates reuse. Reusable binding contracts carry an optional submitted identity and reject self-approval when it is known. Manual validation has no registered submission identity; actor and reason are required trusted-local attribution, not authentication or proof of human identity. Agents and models are not granted approval authority by an actor string. Local filesystem access is not a security boundary. Approval/result records persist in the separately versioned `.mai/gates.db`; unsupported versions reject without migration.

Versioned JSON reports exact identities, protected native paths, approval, normal gate and optional bootstrap process results, bootstrap reuse, bounded diagnostic tails and elapsed time. Outcomes include `success`, `failure`, `timeout`, `cancelled`, `cleanup_failure`, `approval_required`, `policy_denied` and `policy_changed`. Concurrent trunk movement invalidates the result. Initialization/infrastructure failures use the CLI's versioned error object. Native paths use the same lossless platform-tagged encoding as engine output. Approval/denial and validation evidence have no receive or landing side effect.

Linux regressions exercise protected narrowing, candidate gate and helper replacement, exact approval replay invalidation, denial/retry, symlink/rename policy input rejection, missing-cache rebuilding and failed bootstrap followed by reverted inputs. Native macOS/Windows process qualification remains separate CI evidence; local Linux execution alone establishes neither native results nor performance claims.

## Supervised recovery opt-in

On Linux, `validators.recovery = "supervised-local-v1"` selects separate-worker supervision and permits explicit recovery only with matching cleanup proof. The contract applies to bootstrap, gates and all tools they invoke: no process/session escape or independently ongoing offloaded work; completed external writes are not rolled back and must be safe to repeat. It is absent by default and cannot qualify older interrupted claims retroactively. See [validator recovery](validator-recovery.md) before opting in.

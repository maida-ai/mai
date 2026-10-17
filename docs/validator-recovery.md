# Explicit validator recovery

Supervised local recovery addresses one failure: the engine exits while a validator is running, but its separate worker survives long enough to cancel execution and prove cleanup. Recovery makes the occupied slot usable again; it does not accept the interrupted candidate. The next integration rebuilds and runs the normal trusted gate before landing.

This path is opt-in and Linux-only. A missing receipt, killed worker, different kernel boot, legacy claim, contradictory metadata or unsupported platform remains a preserved blocker. The command never decides that an absent PID means cleanup succeeded, kills a stored PID, or clears a claim without proof.

## Execution contract

Configure the trusted trunk policy before running recoverable validation:

```toml
[validators]
# Keep the project's existing explicit slot, port and cache settings.
recovery = "supervised-local-v1"
```

The maintainer must ensure that bootstrap, the gate and all tools they invoke stay within their supervised process groups. They must not daemonize, start new sessions to escape supervision, or leave offloaded work running independently. Unix process groups are not an OS sandbox. The worker's native cleanup result proves only this declared execution boundary; it cannot discover or terminate arbitrary remote jobs or escaped processes.

Recovery does not roll back completed external writes. Gates that touch external state must already be safe to repeat, or those effects must be reconciled separately before retrying integration. Explicit opt-in is not evidence that an older interrupted claim followed this contract. Existing unqualified claims cannot be upgraded by changing policy after the fact. Policy changes on an existing authority still follow the normal protected-policy approval and landing path; do not write trunk directly to enable recovery.

## Operator sequence

Preserve the authority and agent workspaces. Stop a still-running engine normally before inspection or recovery; these commands take exclusive authority ownership and refuse an active engine. The engine serializes integration and shutdown, so `stop` does not cancel a running gate; wait for the bounded gate/cleanup operation to finish. If the engine already exited, do not restart it merely to inspect the slot. Inspection reconciles authority metadata before reporting the slot.

```sh
# When an engine is still running, request its normal shutdown first:
mai stop PROJECT --json
# Once the owner has exited:
mai validator inspect PROJECT --slot 0 --json
```

Record the returned `claim_id`. A cleanup-receipt file being present is not, by itself, proof that it is valid; recovery verifies its identity and contents under the slot lock. If the worker is still cleaning up, wait for its bounded operation to finish and inspect again. Never kill the worker to hurry cleanup: destroying it may also destroy the only process capable of producing proof.

```sh
mai validator recover PROJECT --slot 0 --claim CLAIM_ID --json
mai status PROJECT --json
mai show PROJECT CHANGE --generation 1 --json
mai integrate PROJECT CHANGE --generation 1 --mode textual --wait-seconds 600 --json
```

Recovery returns the quarantine location. Keep it and the original authority for inspection. It contains evidence and interrupted artifacts, not an accepted result. The command keeps the permanent slot lock in place, and a subsequent attempt receives fresh candidate/cache/temporary state. It never edits an agent's working tree, submission ref or accepted trunk to make recovery pass.

If the recovery response is lost, repeat the exact command with the same slot and claim ID. Do not substitute the next claim ID automatically. Completed recovery is idempotent; a stale command must not reclaim a newer attempt. Likewise, query the submission after an uncertain integration response before deciding whether to retry.

## When recovery refuses

An active engine or worker must finish or release ownership before recovery can proceed. A missing or mismatched receipt, legacy claim, changed boot identity, disallowed current policy, unsafe path or contradictory journal means the command cannot establish the required facts. Preserve those files and the exact error. There is no force option, PID-based override or supported manual marker-deletion shortcut.

A killed worker or whole-host failure is outside this same-boot recovery route. Creating another authority on the same machine does not establish that external resources are safe to reuse. These cases still require a separately established quiescence and recovery procedure; the feature does not claim universal self-service recovery.

## Proof and qualification

The worker acquires the slot before acknowledging readiness. The engine authorizes execution through a lifeline; loss of that lifeline requests cancellation. A terminal cleanup receipt binds the original execution claim and trusted policy, and cannot authorize another launch. Explicit recovery reconciles Git/SQLite, validates the claim and receipt, quarantines interrupted artifacts and journals completion before permitting reuse. Recovery never promotes a cancelled or uncertain result to a passing gate.

Qualification must distinguish real engine/worker process deaths from synthetic malformed-metadata tests and interrupted recovery journals. Process-kill evidence does not prove power-loss durability, arbitrary storage-fault recovery or containment of deliberate process escape. See the [recovery evidence](spikes/validator-recovery/2026-10-17/README.md) and its linked native CI for the tested source and platform limits.

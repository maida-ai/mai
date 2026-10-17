# Copy-based continuation qualification — 2026-10-17

The independent operator runs used frozen source `4d1eb89bf87ce4cb0e646478f3ccf9a2807ca20e`, with an empty tracked diff, and debug binary SHA-256 `c83494e11225552dee9d900c92a1125914781ecbbbdc9785a2f4c6e15dc961a5`. [Identity](identity.json) records Linux 7.0.0-39 x86_64, Rust 1.85.0 and Git 2.53.0. The binary and disposable repositories remain outside Git. Native CI and final review are linked from [PR #113](https://github.com/maida-ai/mai/pull/113).

## Observed workflow

Alice's parent landed as r1. Bob changed accepted content and added a file at r2. Alice continued later unsubmitted work into a new workspace on r2: Bob's work remained present, the accepted parent was excluded from replay, and source bytes, modes, configuration, index, refs and ignored files remained unchanged. Repeating the destination refused. Bob then landed an overlapping edit at r3; Alice's submitted continuation was rejected as a true conflict. Continuing the original source again preserved a marked failed destination. An explicit correction combining both intents passed the normal gate and landed as r4.

The [portable harness](../../../../experiments/continuation/README.md) records the [command transcript](commands.jsonl), [outcome](outcome.json) and [source snapshot](source-snapshot.json). Error messages are on stderr in the transcript; the outcome's empty error stdout fields do not indicate success. The continuation command took 0.255102 seconds in this one tiny functional fixture. This is not a representative latency distribution, production MTTR, comparative advantage or scale result.

Independent QA also extracted and executed all three shell blocks of the [two-agent guide](../../../two-agent-workflow.md) unmodified with `/bin/sh -x`. The script exited zero, reached both expected refusal assertions, landed correction r4 and stopped the engine. [Literal documentation identity](literal-docs/identity.json), [script](literal-docs/literal-blocks.sh), [stdout](literal-docs/stdout.log) and [stderr](literal-docs/stderr.log) preserve that separate run against the same frozen binary.

## Checks and review

The [local checks](local-checks.log) passed formatting, strict all-target locked Clippy, all-target locked tests (255 passed, zero failed, five ignored subprocess helper entrypoints exercised by parent tests), build and qualification policy for 79 locked packages. The independent continuation tests include 13 adversarial cases and six end-to-end cases: exact source preservation, stale/registered/dirty/unsupported source refusal, accepted-parent exclusion, newer trunk content, preserved conflict copies, real CLI interruption after clone and replay, repeat refusal and new-path retry, trusted bootstrap ordering and structured failure evidence. Independent review found no remaining correctness blocker within this bounded contract.

## Qualification boundary

This is partial issue #27: a clean, unregistered linear suffix with explicit verified landed identity can be copied into a new workspace. It does not provide in-place sync/autostash, pending-submission replay, automatic scheduling or dirty-tree continuation. Source must remain idle during the operation. Failed and interrupted destinations remain owned evidence and are not automatically deleted or resumed. Process-kill tests do not establish power-loss durability or child-process quiescence; a child may still finish writing its reserved failed destination. Trusted bootstrap is not an OS sandbox. Follow the [continuation contract](../../../continuation.md). External-user and realistic performance gates remain open.

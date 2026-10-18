# Smoke claim: 2026-10-18-smoke
Mechanics only; submitted for independent evaluation. Acceptance is not satisfied.

| Check | Result | Command and output |
| --- | --- | --- |
| One cheap planner → one cheap implementer, both done | FAIL | `python3 -c 'import json; from pathlib import Path; r=[json.loads(s) for s in Path("runs/2026-10-18-smoke/spawns.jsonl").read_text().splitlines()]; print(r); assert len(r)==2'` — spawns.jsonl; checks.log; topology.log. One planner, status needs_scope; no implementer. |
| Exactly one new comment | FAIL | `git diff --numstat 5c802a7 -- src/main.rs` and `git diff 5c802a7 -- src/main.rs` — source-diff.log (empty); checks.log. No source change. |
| Build and all-target tests exit 0 | FAIL | `cargo build --locked --bin mai` exited 0, build.log. `cargo test --all-targets --locked` exited 101, test.log: 29 passed, 2 failed, 2 ignored in library suite. Both failures require Git >=2.40; installed version is recorded by `git --version` in environment.log. Remaining targets were not reached. |
| Claim exists with commands/output locations | PASS | `test -s runs/2026-10-18-smoke/claim.md` exited 0; claim-check.log. |

Output paths above are relative to runs/2026-10-18-smoke/.

The planner's worktree inherited committed AGENTS.md, which lacks the delegation template present in the dirty main checkout. It stopped with needs_scope and committed its report (ac446d0, integrated as 06a440a). See agents/175832-planner-16045.log and planner-current-report.md. Harness exit 0 did not mean task completion.

Not done: implementer spawn, source comment, passing test suite, retry. No harness or instruction files changed. Existing user edits and deletion of planner-report.md preserved.

Known risks: Worktrees do not inherit uncommitted instructions. Git version prevents required tests. Dollar usage is unavailable; ledger records 10,115 cheap-tier harness tokens and unknown executor usage against the $5 budget. This run does not establish product value or north-star improvement.

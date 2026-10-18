# Scope
Directive: Smoke test only: spawn one planner (cheap) that spawns one implementer (cheap) to add a one-line comment to src/main.rs. Then write claim.md.
Interpretation: Exercise the delegated agent workflow with a harmless source comment.
Serves: "One developer ... run many coding agents ... without a human integrating it."
Acceptance:
1. A Python check of spawns.jsonl finds exactly one cheap planner parented by executor and one cheap implementer parented by that planner, both exit 0 and status done.
2. git diff --numstat 5c802a7 -- src/main.rs reports exactly one added line and zero removed; git diff confirms it is a comment.
3. cargo build --locked --bin mai and cargo test --all-targets --locked both exit 0.
4. test -s runs/2026-10-18-smoke/claim.md exits 0 after commands and output paths are recorded.
Non-goals: Product behavior changes, product-value certification, harness edits, additional agents.
Budget: $5; time box: 1 hour.
Assumptions: Existing dirty files and deleted prior planner report belong to the user; preserve them. This is a new invocation atop existing smoke commits.
Assumptions: Use the current main checkout's absolute .agents/spawn path for both levels because worktrees inherit an older committed harness.
Assumptions: Usage records may expose tokens without dollar charges; report unavailable dollar spend honestly.
Kill check: Yes for smoke mechanics: planned tasks establish all acceptance checks and exercise delegation relevant to reducing human integration. No measured north-star improvement is claimed.

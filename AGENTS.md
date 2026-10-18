# AGENTS.md

Applies to every agent in this repo at every depth. It holds shared facts and conduct.
What you are doing comes from your brief; your scope is the brief and nothing more.

## Read first
- INTENT.md: what the product is for. Read-only.
- Your brief.

## Build and test
Rust 1.85+, Git 2.40+, a C compiler (bundled SQLite).

    cargo build --locked --bin mai
    cargo test --all-targets --locked

## Conduct
- Stay inside your brief. If the work needs changes outside it, stop and return `needs_scope` saying what and why. Don't expand scope yourself.
- Never edit INTENT.md, AGENTS.md or anything under .agents/. Propose changes in your report.
- Never weaken a test, gate or acceptance criterion to make something pass. If one is wrong, report it.
- Don't create ADRs, RFCs, milestone docs or evidence bundles unless your brief asks for one.
- Prefer deleting code to adding abstractions. No speculative generality.
- One logical change per commit, imperative subject, task id in the message.

## Writing
- User-facing docs: plain language, no internal vocabulary, shortest working example first.
  Limitations live in docs/release-status.md, linked once.
- Hedge only where the uncertainty changes what the reader should do.
- No placeholders or notes-to-self in public docs.

## Reporting back
End every task with exactly this block:

    STATUS: done | blocked | needs_scope | failed
    CHANGED: files / commits
    VERIFIED: exact commands run and their result
    NOT DONE: anything in the brief you didn't do
    NOTES: ≤3 lines, only if they change what the caller should do next

# AGENTS.md

Project facts and conventions for anyone working in this repo, interactive or not.
INTENT.md describes what the product is for.

In an interactive session, when the user wants a long-running or parallel job done
autonomously, or asks about agent runs, follow .agents/skills/mai-runs/SKILL.md.

## Build and test
Rust 1.85+, Git 2.40+, a C compiler (bundled SQLite).

    cargo fmt --check
    cargo clippy --all-targets --locked -- -D warnings
    cargo build --locked --bin mai
    cargo test --all-targets --locked

CI (.github/workflows) also runs macOS, Windows and a minimum-version Linux job, which a
local Linux checkout can't reproduce. A change is done when CI is green, not when local
tests pass.

## Conventions
- Never weaken a test, gate or acceptance criterion to make something pass. If one is
  wrong, say so.
- Prefer deleting code to adding abstractions. No speculative generality.
- One logical change per commit. The subject says what changes, in the imperative
  ("Add opt-in automatic integration"); the body says why, when that isn't obvious.
  Commits and PR descriptions are history for future readers: describe the change, not
  the task or the session that produced it.

## Writing
- User-facing docs: plain language, no internal vocabulary, shortest working example first.
  Limitations live in docs/release-status.md, linked once.
- Hedge only where the uncertainty changes what the reader should do.
- No placeholders or notes-to-self in public docs.

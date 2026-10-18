# AGENTS.md

Project facts and conventions for anyone working in this repo, interactive or not.
INTENT.md describes what the product is for.

In an interactive session, when the user wants a long-running or parallel job done
autonomously, or asks about agent runs, follow .agents/skills/mai-runs/SKILL.md.

## Build and test
Rust 1.85+, Git 2.40+, a C compiler (bundled SQLite).

    cargo build --locked --bin mai
    cargo test --all-targets --locked

## Conventions
- Never weaken a test, gate or acceptance criterion to make something pass. If one is
  wrong, say so.
- Prefer deleting code to adding abstractions. No speculative generality.
- One logical change per commit, imperative subject.

## Writing
- User-facing docs: plain language, no internal vocabulary, shortest working example first.
  Limitations live in docs/release-status.md, linked once.
- Hedge only where the uncertainty changes what the reader should do.
- No placeholders or notes-to-self in public docs.

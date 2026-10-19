# CLI reference

Run `mai --help` for the command list and `mai COMMAND --help` for command options. Commands are local and explicit: the binary does not run agents or schedule integrations automatically.

## Main workflow

- `mai init PROJECT [--from SOURCE]` initializes an authority.
- `mai workspace PROJECT AGENT DESTINATION` creates an independent clone.
- `mai receive enable PROJECT ...` enables bounded receive.
- Push a committed change to `refs/for/main` to register a generation.
- `mai show PROJECT CHANGE` and `mai queue PROJECT` inspect received work.
- `mai integrate PROJECT CHANGE --mode textual` explicitly selects a generation for validation and integration.

See [getting started](getting-started.md) for a complete walkthrough and [receive and integration](receive.md) for limits and results.

Other implemented commands cover engine status and shutdown, ordinary file leases, exact-generation waiting, manual validation and approval, validator recovery, and copy-based continuation. Their contracts are documented in [engine](engine.md), [manual validator](manual-validator.md), [validator recovery](validator-recovery.md), and [continuation](continuation.md).

Submission does not mean landing. Integration requires an explicit command and the configured gate. See [current delivery and limits](release-status.md).

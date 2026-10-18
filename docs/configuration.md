# Configuration

The trusted trunk must contain a regular `mai.toml` for manual validation and configured validator execution. The accepted version-1 fields, defaults, path rules and executable trust boundary are documented in [Manual trusted validator](manual-validator.md). Unsupported keys and versions are rejected.

Configure validation commands, bootstrap commands, slot resources and protected paths explicitly. mai executes repository commands as trusted local processes; it does not sandbox them. Supervised validator recovery is an opt-in Linux setting described in [Validator recovery](validator-recovery.md).

The engine's authority identity and local runtime state live under `.mai/`; see [authority startup](authority-startup.md) and [engine](engine.md). These local files are not tracked project policy.

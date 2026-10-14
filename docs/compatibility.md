# Repository compatibility

Existing Python v0.1.1 `.mai/metadata.sqlite3`, `.mai/repo.git`, workspaces, baseline refs, and history open in place. Early `.agenttrunk` stores and `refs/agenttrunk/base` also remain supported. No metadata schema conversion occurs. New writes add `locks/` files and Git `refs/mai/receipts/AGENT`; these are compatible additions.

Stop old clients while changing binaries. Mixed Python/Rust operations against the same workspace are unsupported because Python does not honor the new per-agent lock. Back up Git, metadata, and important drafts before evaluating important stores. Compatibility does not imply production recovery qualification.

Repository paths and tracked filenames must be UTF-8. Unsupported filenames fail before publication; raw contents remain arbitrary bytes. The Python implementation's surrogate-escaped non-UTF-8 filename behavior is not retained. Normal shell tilde expansion works, but literal quoted `~` paths are not expanded by mai.

Windows support uses Git for Windows, native process locks, and platform-appropriate check syntax. POSIX executable permissions and symlink/process-control tests remain Unix-specific. Case-sensitive names, platform filesystem restrictions, and cross-platform materialization deserve qualification before important use.

The source-generated [compatibility fixtures](../tests/fixtures/README.md) preserve the prototype format and baseline evidence. The [architecture](architecture.md#revision-and-repository-format) explains the supported storage layout.

[All docs →](README.md)

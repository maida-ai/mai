# Authority engine foundation

This MVP foundation initializes an authority, reconciles its durable state, and runs one local owner. It supports explicitly enabled receive and serial integration of one named generation. [Manual validation](manual-validator.md) and [workspace creation](workspaces.md) remain separate operations. A ready engine is not an automatic queue scheduler. See [receive and integration](receive.md) for enablement, integration, ordinary leases and limitations.

See the [authority startup contract](authority-startup.md) for initialization and exact-evidence reconciliation.

## Commands

```sh
mai init /path/to/project
mai init /path/to/import --from /path/to/existing-repository
mai serve /path/to/project --foreground
```

Run foreground serving under your supervisor, or let a status request start it:

```sh
mai status /path/to/project --json
mai status /path/to/project --no-autostart
mai stop /path/to/project
```

`status` waits up to five seconds for autostart readiness. The child writes startup errors to `.mai/engine.log`; inspect that log or run `serve --foreground` for diagnosis. A timeout never kills an owner that may still be reconciling. Concurrent starters converge on the OS lock owner. `--no-autostart` reports unavailability without starting a process. `stop` never starts an engine and requests graceful shutdown; a successful response means shutdown was accepted, and a supervisor can wait for process exit.

`--json` returns a versioned status object containing `protocol_version`, `pid`, canonical `authority`, `state` (`ready` or `stopping`), and `registration_available` (false until explicit receive enablement). The `authority` field is a lossless native-path object: `{"encoding":"unix_bytes","data":"<base64>"}` on Unix, or `{"encoding":"windows_utf16le","data":"<base64>"}` on Windows. `data` is canonical padded standard base64 of raw Unix pathname bytes or little-endian Windows UTF-16 code units, including unpaired surrogate units. The same object is returned by initialization and by the default pretty-printed output; it is never a lossy Unicode pathname string. These objects identify local native paths, not portable cross-platform paths. Decoders reject foreign-platform tags, unknown fields, malformed/noncanonical base64, NUL units and odd UTF-16 byte counts. It is available for status, stop and initialization output. Unsupported local protocol versions fail. Error JSON uses the lifecycle version and stable infrastructure exit status; invocation errors remain Clap's exit status 2.

## Ownership and recovery

The persistent `.mai/engine.lock` file is locked exclusively through the OS before opening SQLite or reconciling. Its presence and the diagnostic `.mai/engine.pid` do not indicate ownership: stale PID values cannot steal a held lock, and the lock file is never removed. Each engine validates the authority's local configuration and database schema before making its IPC endpoint ready. Unsupported or corrupt state refuses startup; it is not recreated in place.

`mai stop` permits orderly endpoint cleanup while ownership is held. Ctrl-C, process termination, and crashes use the OS's normal lock release; the next owner cleans a stale Unix socket only after acquiring the lock. Git refs are preserved across restart and reconciliation checks exact durable identities. Process-kill tests do not establish power-loss durability.

Local IPC uses `.mai/engine.sock` on Unix and a native Windows named pipe derived from the canonical authority path. Unix socket permissions are owner-only; Windows pipes reject remote clients and use the OS default local access control. These interfaces are trusted-local administration, not a sandbox or a hostile multi-user security boundary. Unix authority paths must fit the OS socket path limit. Engine files, endpoints and logs remain outside tracked gate policy; this layer adds no tracked `mai.toml` setting or default validation command.

Requests and responses are bounded, with bounded IO and protocol parsing. Integration waits up to its explicit client deadline (default 600 seconds, maximum 3600); a timeout is not cancellation. The single engine serializes a gate with receive, status, approval and lease requests. Ordinary IPC waits are bounded at 10 seconds and hook waits at 15 seconds; a busy engine can therefore produce an uncertain response. `mai show` reads persisted state independently of the engine, including while the gate runs; reconcile interrupted publication before retrying. Malformed or incomplete clients cannot request shutdown. A bounded acknowledgment permits Windows response delivery without blocking indefinitely in `FlushFileBuffers`. Native macOS and Windows CI remain required; Linux success alone does not qualify their process or IPC behavior.

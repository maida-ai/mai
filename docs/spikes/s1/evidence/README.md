# S1 evidence index

Historical local snapshots remain unchanged: [Linux host](2026-10-15-linux/) and [Docker Desktop container clients](2026-10-15-container/). The CI snapshots below retain every downloaded platform artifact from the five manual runs reviewed on 2026-10-15 UTC, including both failed Windows observations. Each snapshot records provenance, commands, result, limits and source-hash verification. No new experiment was dispatched during collection.

| Run / platform | Source | Job result | Passed scenario assertions |
| --- | --- | --- | --- |
| [38678128220 / linux](2026-10-15-linux-run-38678128220-attempt-1/) | `05df0a0` | success | 115 |
| [38678562675 / linux](2026-10-15-linux-run-38678562675-attempt-1/) | `05df0a0` | success | 115 |
| [38678562675 / windows](2026-10-15-windows-run-38678562675-attempt-1/) | `05df0a0` | failure | 74 |
| [38678562675 / macos](2026-10-15-macos-run-38678562675-attempt-1/) | `05df0a0` | success | 115 |
| [38685578351 / windows](2026-10-15-windows-run-38685578351-attempt-1/) | `05df0a0` | failure | 96 |
| [38685738493 / windows](2026-10-15-windows-run-38685738493-attempt-1/) | `2153c83` | success | 115 |
| [38685798444 / linux](2026-10-15-linux-run-38685798444-attempt-1/) | `2153c83` | success | 115 |
| [38685798444 / windows](2026-10-15-windows-run-38685798444-attempt-1/) | `2153c83` | success | 115 |
| [38685798444 / macos](2026-10-15-macos-run-38685798444-attempt-1/) | `2153c83` | success | 115 |

The final all-platform run `38685798444` is the primary host qualification evidence: 115 assertions and four unit checks on each native platform. Windows also passed the preceding Windows-only run `38685738493`. Before the socket fix, Windows failed at different HTTP requests with the same `WouldBlock` root cause; these partial runs must not be counted as successful qualification.

All successful runs have the same ordered 115 scenario assertions as the historical Linux snapshot. Historical container evidence adds 52 container-specific assertions; hosted workflows did not repeat those scenarios. Source hashes identify different harness revisions, so agreement is behavioral rather than a claim of identical source bytes.

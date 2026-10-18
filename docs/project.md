# Project overview

mai is an Apache-2.0 Rust project for coordinating local coding-agent changes through ordinary Git. It belongs to the maida-ai family but has no runtime dependency on Maida infrastructure. Installation and initialization do not move Maida assets.

The current product creates independent workspaces, receives explicitly pushed changes, records their state, and lets an operator select a generation for gated integration. See [getting started](getting-started.md) and [current delivery and limits](release-status.md).

## Development

Build and check the project with the commands in [Contributing](../CONTRIBUTING.md). The [architecture](architecture.md) describes the current implementation boundary. Historical design and qualification evidence remain under [spikes](spikes/) and experiments; they document historical evidence rather than current behavior.

# Current integration contract

This document records the current behavior of the local integration workflow. It is a concise reference, not an implementation roadmap.

- Agents submit ordinary Git commits from independent clones.
- Receive must be explicitly enabled and applies configured size limits.
- A successful push registers a durable generation; it does not land the change.
- An operator explicitly selects one generation for integration.
- Dependencies, policy, ordinary leases and trusted validation are checked before publication.
- Accepted changes are recorded on the authority's first-parent trunk. Rejections preserve the submitted work and report a typed outcome.
- Integration is serial. mai does not automatically schedule work or synchronize existing agent clones.

See [receive and integration](../receive.md), the [CLI reference](../cli.md), and [current delivery and limits](../release-status.md).

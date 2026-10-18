# Agent integration

Each agent works in an independent Git clone created with `mai workspace`. Configure and enable receive at the authority, then submit committed work explicitly:

```sh
git push -o agent=alice mai HEAD:refs/for/main
```

A successful push registers a generation; it does not mean the change landed. Record the returned change identity, inspect it with `mai show`, and wait for an operator to select it for integration. A landed change has passed the configured gate. A rejection preserves the submitted work and its available diagnostics for correction.

For full examples, follow [getting started](getting-started.md), [failed gate and correction](check-failure.md), and [two-agent workflow](two-agent-workflow.md). After landing, [copy-based continuation](continuation.md) can replay a clean local suffix into a new workspace. It does not update the existing clone.

mai does not run agents, install agent hooks, or choose conflicting intent. Keep using the team's existing review process. The authority and commands run by its configured checks are trusted; mai is not a sandbox. See [current delivery and limits](release-status.md).

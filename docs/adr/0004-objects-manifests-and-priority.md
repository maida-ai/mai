# ADR 0004: Objects, change manifests and priority

Status: MVP file-unit/manifest design acceptance pending, unimplemented. [ADR 0010](0010-staged-mvp-qualification.md) governs staged M0; D12 priority policy is deferred to RFC #93 and is not an initial MVP requirement. D10–D12 extend [ADR 0003](0003-git-native-integration-queue.md), which now carries all exact integrated-validation requirements from the former ADR 0004. Existing 0.0.1 has none of these interfaces.

## D10: one unit scheme

| Unit | Example | Source |
| --- | --- | --- |
| Symbol | src/api.rs::Router::add_route | Parser |
| Config key | Cargo.toml::dependencies.serde | Structured-format parser |
| Module scope | `src/api.rs::<module>` | Imports/top-level code outside objects |
| File | assets/logo.svg | Explicit fallback with reason |

Use these identifiers in leases, conflicts, intents and manifests; never use line ranges. A class lease covers methods; a file lease overlaps every object in the file. Module scope is not leasable by default, avoiding serialization of imports and glue; merge textually and run the gate. M2 renames are delete-plus-add; stable identity across moves is deferred. Unsupported inputs, generated code, parse failures and ambiguous identity report conservative file fallback rather than silently narrowing protection.

M1 establishes identifiers, manifests and indexing using file units only, with no parser. M2 fills symbols, config keys and module scope using qualified parsers/tree-sitter and applies object leases. Initial ecosystems remain maintainer choices. Locking on finer units never proves semantic independence; integrated validation stays mandatory where configured.

## D11: engine-computed manifests

Compute a receive manifest from the submitted commit and its resolved base, then recompute a landing manifest against the actual landed parent after merge/regeneration. Preserve both identities and evidence; do not silently replace the submitted view with the integrated view. Include affected unit, added/modified/deleted kind, parser versions and fallback reasons. Agent commit-message claims cannot supply these records.

Store JSON in refs/notes/mai-objects and index unit-to-change/revision relationships in state.db. Notes, trunk and SQLite are separate writes, not one transaction. Reconcile/rebuild the index and missing notes across interrupted landing using retained immutable inputs. Mai-Objects trailers contain the full list for at most 20 units, otherwise a count and note pointer. Define deterministic ordering, escaping and note association for submitted versus landed commits before implementation.

Agents declare intent through mai intent or mai.touches push options. Compare declarations with computed manifests: scope_drift warns about undeclared units; configured protected-object policy may reject. M2 owns precise intent comparison. Manifests support lease enforcement, awareness, affected-test input, why and mai log --object. They can narrow bisection search, never exclude other possible culprits without fallback or replace reproducible evidence. They never approve a change.

## D12: priority and safeguards (deferred RFC proposal)

The following is input to [RFC #93](https://github.com/maida-ai/mai/issues/93), not an accepted MVP policy. The initial MVP uses one class with dependency-aware arrival ordering; priority-driven preemption, grants, aging and resolver precedence remain unavailable until separately accepted and tested.

Classes are sev > high > normal > low; normal is default. Normal/low are self-assigned. Sev/high need audited expiring grants from authorized humans or a designated role; who may grant remains a maintainer decision. The engine can grant temporary scoped sev for a confirmed trunk fix. Record grant, reason, subject, issuer, scope, expiry and revocation. Distinguish requested/granted priority from effective aged/inherited priority; engine promotions apply only to the affected change and its specified queue, lease and resolver effects, not portable grants. Recompute effective priority on revocation, expiry or dependency changes.

Apply priority only to queue order, lease preemption and resolver assignment. Order by class then arrival. Sev may cancel lower-class speculative validation attempts, never their changes, to free slots; stop processes, clean slots safely, retain and requeue their work and invalidate descendants correctly. Lower classes cannot preempt running validation. Cancellation never creates a passing check or permits stale evidence.

Higher-class lease requests notify holders and transfer ownership only after lease_grace. Preserve preempted work; check effective lease ownership at landing so stale holders cannot publish over new owners. Resolve expiry, revocation and competing transfer requests deterministically. M1 applies this to path leases; M2 extends it to contained objects. In a conflict, the higher-priority side retains precedence and the lower-priority side is resolver; ties use later lander. “Wins” means responsibility/ordering, never automatically choosing text or discarding another change. Lease enforcement and checks still apply.

Dependencies inherit the higher effective class to avoid inversion; remove inherited elevation when its cause ends. Aging promotes waiting changes one class per configured interval, capped at high, with audited engine promotion distinct from self-assigned grants. max_sev_in_flight caps sev validation including inherited work; avoid deadlock by scheduling prerequisites before dependents within the cap. Any future starvation claim requires stated admissible-load assumptions and evidence; aging alone does not establish it. This is not a pre-MVP gate.

Priority never skips or weakens the normal gate. check.sev_gate remains an unresolved maintainer decision; no narrower gate is enabled or implied here. Any future approved narrower policy must identify its distinct evidence and mandatory full confirmation honestly, and cannot inherit the ordinary full-gate guarantee.

## Qualification and risks

[S1 (#51)](https://github.com/maida-ai/mai/issues/51) was accepted on 2026-10-15; its [report](../spikes/s1/README.md) and [evidence](../spikes/s1/evidence/README.md) qualify Git transport and typed policy rejection for `mai.fixes`, `mai.priority` and `mai.touches`. The fixture accepts valid low-priority/touch declarations, rejects malformed or oversized options, duplicate options, unauthorized elevation and inactive incident references, and admits a synthetic scoped severe fix for an authenticated identity. Client-declared file identity cannot authorize that elevated request.

This evidence uses synthetic authentication/grants and provisional option grammar and limits, not production policy. Options are collected before policy-limit rejection, so the result does not establish bounded memory use while receiving malicious input. Real grant issuance, expiry/revocation and roles, priority scheduling and starvation, authoritative manifests and precise touch comparison remain separate qualification or implementation work. S1 does not accept this ADR, settle grant roles, make declared touches authoritative or enable a narrower severe gate.

S4 varies priority mix, failures/flakes/conflicts, sequential/pipeline/batch modes and 5/15/30-minute fix windows versus immediate revert. Publish per-class time-to-land, time-to-green and changes stacked on red; use these as exploratory inputs; final policy and realistic performance claims await MVP observations under ADR 0010. Policy values such as grant TTL, grace, aging and sev cap are proposed configuration values, not measured guarantees.

| Risk | Likelihood | Impact | Mitigation | Spike |
| --- | --- | --- | --- | --- |
| Everything is elevated to sev | High | Medium | Grants, audit, expiry and in-flight cap | S4 |
| Starvation or priority inversion | Medium | Medium | Controlled aging, dependency inheritance and per-class latency evidence | S4 |
| Wrong manifests from macros/generated/dynamic code | Medium | Medium | Explicit reasoned file fallback; no approval from manifests | S2 |
| Imports/module glue serialize all work | High | Low | Module scope not leasable by default; textual integration plus gate | — |

Test containment, rename/delete/add, fallback, manifest reconciliation and trailer bounds; test lease grace/expiry, grant revocation, inherited caps, cancellation and concurrent priority ties. Test that priority cannot bypass protected approval, lease ownership, exact validation or preservation of submitted work.

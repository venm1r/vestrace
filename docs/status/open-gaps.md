# Open gaps and safe implementation boundaries

## Complete external memory workflow

The existing detail response does not expose content/current revision; retrieval exposes
ContextPack counts rather than text. Proposed list/history/detail and correction workflows
allow users to inspect exactly what they intend to edit. MW-01 handles read surfaces;
MW-02 handles whole-command mutation and replay.

Repository atomicity for memory/revision/source/search is not atomicity for the whole business
command: the service subsequently writes outbox and idempotency separately. Preserve the
existing CAS and fix the larger transaction boundary. Do not claim all lost responses are
safe to retry without verifying the relevant contract.

## P04 dependencies after 14E

Task 14E final acceptance is recorded in the supplied evidence and must not be relabeled
as an unimplemented proposal. However, its own verdict leaves P04/G0/v1.0 incomplete and
retains a rotation-before-adoption deferral. Read the latest evidence and actual composition
before claiming complete indexing or retrieval readiness. MW cannot bypass those dependencies
with a legacy embedding adapter.

A separately qualified text-only path may have a narrower claim. It must not imply that
the complete vector pipeline is ready.

## Installation and authorization

A valid mounted bootstrap secret store remains a prerequisite. Creating an empty volume
does not make the clean-machine setup complete. The guide describes prepared-environment
startup and diagnostics; it does not invent an unverified key-store format.

A route inventory establishes declared admission/denial policy, not successful execution of
every handler. Preserve explicit 501/unavailable states, missing qualification/policy checks,
and any library capabilities not wired into the production root.

## Upgrade and qualification

The current archive already uses migration number 0196. Candidate MW-M01 and all dependent
references must be reconciled before SQL implementation. Documentation names do not reserve
numbers. Never repair this collision by editing applied migration checksums.

CI definitions, another branch's results, and JSON schemas do not qualify an installation.
Backup/restore, browser workflows, restricted-runtime authorization, and crash behavior need
actual observations. A new improvement removes an old limitation only when evidence closes
that exact limitation; a renamed document is insufficient.

[Current status](../status.md) · [MW preflight](../implementation/memory-workspace/plans/00-preflight.md)

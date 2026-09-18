# PostgreSQL schema and migrations

This restores a missing documentation entry point. It is an implementation guide, not a new schema or migration.

## Source of truth

The ordered [migrations directory](../migrations/) in a full repository defines implementation schema. Do not maintain a claimed maximum migration number in prose. Runtime compatibility compares the embedded migration set with applied version, success, and checksum records. Verify the exact startup path for the selected build.

The [Domain Model](specs/en/vestrace-domain-model-v0.2.md), [Temporal Model](specs/en/vestrace-data-temporal-model-v0.2.md), and [Governance Contract](specs/en/vestrace-crypto-data-governance-contract-v0.2.md) define targets. A type in a target specification does not prove a corresponding table exists.

## Transactional state and projections

PostgreSQL stores authoritative identities, policies, Memory/revision/provenance, Run history, jobs/outbox/idempotency, and audit. Governed artifact bytes may have a separate material lifecycle. A hash identifies bytes, not ownership or authorization.

Run events, stream versions, checkpoints, and read projections have different roles. Table presence alone does not establish authority. Keep raw evaluation_facts separate from advisory learned_projections and proposal-only learning_proposals.

## Writes and evolution

The reviewed memory repository commits memory/revision/source/search together with CAS. The enclosing service's outbox/idempotency remains a separate boundary; see [transactions](design/transactions.md).

Workspace RLS is defense in depth, not a substitute for application authorization. Applied migrations remain immutable. New schema requirements need explicit planning and tests against both clean and populated databases, including grants, history, invariants, and recovery.

**Snapshot-specific note:** `0196_retired_credential_erasure.sql` already exists. The old MW candidate `0196_memory_workspace_receipts.sql` therefore requires renumbering through MW-00 before SQL work. This documentation does not reserve another number or modify either contract silently.

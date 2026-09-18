# Transactions and request replay

## What is already atomic

PgMemoryRepository writes memory, a new revision, source, and search projection in one transaction. A revision update checks expected content/state versions. This prevents publishing active memory without its content and basis.

MemoryService subsequently saves outbox and idempotency records separately. An atomic part does not make the full command atomic. Failures between those parts and lost responses need separate tests.

## Shared unit of work

GovernedMutationApply and GovernedMutationRepository provide `commit` and `commit_in`. The latter participates in the caller's open transaction; it must not silently commit a second transaction. Outbox, idempotency, and audit ports can also participate in one unit of work.

MW-02 proposes committing the receipt with mutation, audit, and follow-up work. This is a requirement for the new path, not a claim that the legacy service already satisfies it.

## Idempotency

A key identifies a logical request. The same key with different semantics conflicts. Newly generated attempt UUIDs must not make an identical retry a different command; the current memory fingerprint already excludes them.

Reauthorize under the current request. An old receipt does not grant another principal access to its result. Describe the retention horizon explicitly; an expiring cache does not support an unlimited replay claim.

## Outbox

Delivery is at-least-once: a crash after processing commit and before acknowledgement can repeat an event. Handlers must converge on the same domain identity. A queue name does not establish exactly-once semantics.

An unknown topic must remain unprocessed. Persist failure, apply backoff, and retain exhausted attempts as dead-letter rather than dropping promised work. Every payload needs a consumer. Outbox is not a second canonical event log.

## Lock order and external work

Use the order from the accepted implementation package. Do not hold a broad SQL transaction across slow network/vault work without a justified contract. Cross-system gaps require durable intent/witnesses and recovery. One successful half does not establish overall success.

**Sources:** [ports](../../crates/vestrace-application/src/memory/ports.rs), [service](../../crates/vestrace-application/src/memory/services.rs), [repository](../../crates/vestrace-infrastructure/src/postgres/memory_repository.rs), [governed mutation](../../crates/vestrace-application/src/governed_mutation.rs), [outbox](../../crates/vestrace-application/src/outbox.rs), [idempotency](../../crates/vestrace-application/src/idempotency.rs).

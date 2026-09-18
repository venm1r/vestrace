# Operational runbook

## Observe before changing

Identify the build/configuration target and a safe workspace. Check processes and database/schema readiness, then backlog and relevant Run state. Retain request/job/effect/revision IDs rather than sensitive request bodies. Do not begin by deleting failed rows or restarting everything without understanding external outcomes.

Requested, Running, ResultPrepared, and Ready generation refer to different phases. Waiting for keys, providers, or authority does not become definite failure solely because a timeout elapsed. Show the allowed action or concrete missing prerequisite.

## Outbox

At-least-once delivery may repeat after processing commit and before acknowledgement. Handlers converge by domain identity. An unhandled topic remains pending and must not be deleted merely to reduce backlog. Dead-letter records need a reason and a diagnosable resolution.

Add every topic together with its real consumer and tests. Outbox is not a second general event log.

## Worker --once

Exit 0 means the cycle performed work; 3 means idle; 1 means poll/delivery error. Processing an item may legitimately produce refusal, retry, or dead-letter rather than business success. Schedulers should interpret these codes by contract, not classify idle as corruption.

## Restart and incidents

Verify restart safety and persistent roots first. After restart, inspect recovery phase and unresolved work. A lost response is not permission to repeat an external effect. Critical manual actions require their operation-specific grant/approval.

Preserve safe evidence and limit affected capabilities. Fixing a cause, recovering data, and restoring dangerous authority are separate decisions. Successful health probes do not restore trusted status without requalification.

**Sources:** [outbox](../../crates/vestrace-application/src/outbox.rs), [CLI](../../crates/vestrace-cli/src/main.rs), [P04 evidence](../development-evidence/v1-g0-04-embedding-transition-foundation.md), [recovery contract](../specs/en/vestrace-health-repair-incident-contract-v0.2.md).

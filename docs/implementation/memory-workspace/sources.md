# Source register and review boundaries

Original review commit: `6f6102536e9a535b7086db14573bf45fe750ad71`, reviewed 2026-09-07.

Hashes identify whole source files; inspected ranges in [source-manifest.json](source-manifest.json) identify what the original reviewer read. No runtime test follows from a hash match. These observations are historical, not a full audit. See the [current delta](../../maintenance/source-delta.json) for the supplied archive.

## S01

[docs/development-evidence/v1-g0-04-embedding-transition-foundation.md](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/docs/development-evidence/v1-g0-04-embedding-transition-foundation.md)

Historical review at 6f610253: 14B–14D and worker --once; 14D accepted through ResultPrepared and 14E then proposed. See current source delta for later recorded acceptance.

## S02

[docs/specs/README.md](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/docs/specs/README.md)

Normative document hierarchy; the roadmap is not expanded automatically.

## S03

[crates/vestrace-application/src/memory/mod.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/memory/mod.rs)

Existing MemoryUseCases without a list/history read model.

## S04

[crates/vestrace-application/src/memory/ports.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/memory/ports.rs)

MemoryRepository and atomic memory/revision/source/search persistence.

## S05

[crates/vestrace-application/src/memory/services.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/memory/services.rs)

Outbox/idempotency are persisted separately; fingerprints exclude random IDs and label transitions are refused.

## S06

[crates/vestrace-infrastructure/src/postgres/memory_repository.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-infrastructure/src/postgres/memory_repository.rs)

begin_scoped, memory CAS, and revision/source writes; the memory_sources INSERT has no revision_id.

## S07

[crates/vestrace-application/src/governed_mutation.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/governed_mutation.rs)

GovernedMutationRepository::commit_in and GovernedMutationApply.

## S08

[crates/vestrace-application/src/outbox.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/outbox.rs)

At-least-once delivery requires idempotent handlers; five attempts, backoff, and dead-letter outcomes.

## S09

[crates/vestrace-application/src/idempotency.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/idempotency.rs)

IdempotencyRepository::save_in; records include workspace and retention expiry.

## S10

[crates/vestrace-http/src/api/memory.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/memory.rs)

MemoryResponse exposes metadata, not content or the active revision number.

## S11

[crates/vestrace-http/src/api/retrieval.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/retrieval.rs)

ContextPackDto summarizes without sections; temporal and intent parameters already exist.

## S12

[crates/vestrace-application/src/retrieval/context_builder.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/retrieval/context_builder.rs)

ContextItem.rendered_text/provenance_refs; UTF-8 bytes / 4 token estimate and fallback to explanation.

## S13

[apps/console/src/memory/MemoryConsole.tsx](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/memory/MemoryConsole.tsx)

A presentation component receives memory via props, without its own editing workflow.

## S14

[apps/console/src/main.tsx](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/main.tsx)

The reviewed Routes table has no full memory route.

## S15

[apps/console/src/sdk/client.ts](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/sdk/client.ts)

Existing ApiRequestError/client DTOs; extend the transport rather than creating parallel authentication.

## S16

[apps/console/package.json](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/package.json)

React, TypeScript, Vite; test:protocol exists but no general npm test at the reviewed baseline.

## S17

[crates/vestrace-application/src/material/commands.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/material/commands.rs)

MaterialIntentCommands reserve/prepare_content/bind/finalize_bound; recovery does not invent lost plaintext.

## S18

[apps/console/src/routes](https://github.com/venm1r/vestrace/tree/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/routes)

Complete non-truncated route subtree inventory; MemoryPage absent.

## S19

[apps/console/src/sdk](https://github.com/venm1r/vestrace/tree/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/sdk)

client.ts, agUiClient.ts, and useApiResource.ts.

## E01

[https://doc.rust-lang.org/cargo/commands/cargo-test.html](https://doc.rust-lang.org/cargo/commands/cargo-test.html)

--all-targets does not include --doc; execute doctests explicitly.

## E02

[https://www.postgresql.org/docs/17/ddl-rowsecurity.html](https://www.postgresql.org/docs/17/ddl-rowsecurity.html)

Superusers/BYPASSRLS bypass row security; FORCE RLS does not remove superuser bypass.

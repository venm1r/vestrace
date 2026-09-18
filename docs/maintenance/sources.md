# Sources and review limits

The editing input is the user-supplied archive at `3e05dfbdce063aa44a3a9e5a7a84c274597e8188`. Original S/R records below keep the commits, hashes, and inspection ranges of their earlier reviews. Translating their descriptions does not update that evidence.

## Current source delta

Exact blob comparisons in [source-delta.json](source-delta.json) separate unchanged source bytes, changed files, absent historical guide entries, and directory metadata. Selected S03–S17 implementation blobs match; the P04 evidence changed. The final Task 14E approval is recorded, but its verdict leaves P04/G0/v1.0 incomplete. Migration 0196 is occupied. No product tests were executed for this edition.

## Original source records

### S01

[docs/development-evidence/v1-g0-04-embedding-transition-foundation.md](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/docs/development-evidence/v1-g0-04-embedding-transition-foundation.md)

Historical review at 6f610253: 14B–14D and worker --once; 14D accepted through ResultPrepared and 14E then proposed. See current source delta for later recorded acceptance.

### S02

[docs/specs/README.md](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/docs/specs/README.md)

Normative document hierarchy; the roadmap is not expanded automatically.

### S03

[crates/vestrace-application/src/memory/mod.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/memory/mod.rs)

Existing MemoryUseCases without a list/history read model.

### S04

[crates/vestrace-application/src/memory/ports.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/memory/ports.rs)

MemoryRepository and atomic memory/revision/source/search persistence.

### S05

[crates/vestrace-application/src/memory/services.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/memory/services.rs)

Outbox/idempotency are persisted separately; fingerprints exclude random IDs and label transitions are refused.

### S06

[crates/vestrace-infrastructure/src/postgres/memory_repository.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-infrastructure/src/postgres/memory_repository.rs)

begin_scoped, memory CAS, and revision/source writes; the memory_sources INSERT has no revision_id.

### S07

[crates/vestrace-application/src/governed_mutation.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/governed_mutation.rs)

GovernedMutationRepository::commit_in and GovernedMutationApply.

### S08

[crates/vestrace-application/src/outbox.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/outbox.rs)

At-least-once delivery requires idempotent handlers; five attempts, backoff, and dead-letter outcomes.

### S09

[crates/vestrace-application/src/idempotency.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/idempotency.rs)

IdempotencyRepository::save_in; records include workspace and retention expiry.

### S10

[crates/vestrace-http/src/api/memory.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/memory.rs)

MemoryResponse exposes metadata, not content or the active revision number.

### S11

[crates/vestrace-http/src/api/retrieval.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/retrieval.rs)

ContextPackDto summarizes without sections; temporal and intent parameters already exist.

### S12

[crates/vestrace-application/src/retrieval/context_builder.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/retrieval/context_builder.rs)

ContextItem.rendered_text/provenance_refs; UTF-8 bytes / 4 token estimate and fallback to explanation.

### S13

[apps/console/src/memory/MemoryConsole.tsx](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/memory/MemoryConsole.tsx)

A presentation component receives memory via props, without its own editing workflow.

### S14

[apps/console/src/main.tsx](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/main.tsx)

The reviewed Routes table has no full memory route.

### S15

[apps/console/src/sdk/client.ts](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/sdk/client.ts)

Existing ApiRequestError/client DTOs; extend the transport rather than creating parallel authentication.

### S16

[apps/console/package.json](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/package.json)

React, TypeScript, Vite; test:protocol exists but no general npm test at the reviewed baseline.

### S17

[crates/vestrace-application/src/material/commands.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/material/commands.rs)

MaterialIntentCommands reserve/prepare_content/bind/finalize_bound; recovery does not invent lost plaintext.

### S18

[apps/console/src/routes](https://github.com/venm1r/vestrace/tree/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/routes)

Complete non-truncated route subtree inventory; MemoryPage absent.

### S19

[apps/console/src/sdk](https://github.com/venm1r/vestrace/tree/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/sdk)

client.ts, agUiClient.ts, and useApiResource.ts.

### R01

[crates/vestrace-http/src/auth.rs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-http/src/auth.rs)

Bearer identity; read lines 85–160

### R02

[crates/vestrace-http/src/route_inventory.rs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-http/src/route_inventory.rs)

Route inventory; read lines 1–250; not proof of every handler

### R03

[crates/vestrace-cli/src/main.rs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-cli/src/main.rs)

Clap command surface; read lines 1–320

### R04

[docker-compose.yml](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docker-compose.yml)

Read lines 1–110 and 340–495; bootstrap volumes and browser proxy

### R05

[docs/getting-started.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/getting-started.md)

Original guide, complete read; contains stale claims

### R06

[docs/domain-model.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/domain-model.md)

Original domain entry, complete read

### R07

[docs/database-schema.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/database-schema.md)

Original schema entry, historical snapshot

### R08

[docs/security-and-rls.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/security-and-rls.md)

Original security entry, historical snapshot

### R09

[docs/specs/vestrace-architecture-contract-v0.2.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md)

Normative contract from preceding pinned review; target, not implementation

### R10

[docs/adr/0001-memory-first-persistent-cognition.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/adr/0001-memory-first-persistent-cognition.md)

Accepted product boundary from preceding pinned review

### R11

[docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md)

Fixed P01–P12 program from preceding pinned review

### R12

[.github/workflows/ci.yml](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/.github/workflows/ci.yml)

CI definition from preceding pinned review, no new CI execution

### R13

[crates/vestrace-infrastructure/src/postgres/revision_hydrator.rs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-infrastructure/src/postgres/revision_hydrator.rs)

Exact revision hydration from preceding pinned review

### R14

[scripts/p04-scope.mjs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/scripts/p04-scope.mjs)

Protected authorities from preceding pinned review

### R15

[apps/console/vite.config.ts](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/apps/console/vite.config.ts)

Read at 07e2977; Vite identity injection is not Bearer authentication.

### R16

[apps/console/nginx.conf.template](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/apps/console/nginx.conf.template)

Read at 07e2977; nginx attaches deployment token, loopback trust boundary.

### R17

[crates/vestrace-mcp/src/server.rs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-mcp/src/server.rs)

Read get_memory/search on unchanged code ancestor; metadata-only result.

### R18

[docs/implementation/memory-workspace/README.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/implementation/memory-workspace/README.md)

Integrated MW tree verified against Git tree 1f69acad; existing proposal preserved.

## Interpretation

[sources.json](sources.json) retains original source IDs/hashes. Normative sources define requirements, not implementation evidence; directory metadata is not a code audit. Historical guide entries may refer to missing older paths, now restored as English entry points.

Temporary download URLs, credentials, and private user content are excluded. Competitor comparisons, prices, and current external community rules are outside this source-based editing task.

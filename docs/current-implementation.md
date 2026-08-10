# Vestrace Current Implementation Snapshot

**Snapshot source:** `main@729d456f70f4de93c97d05cce795c09025c62f24`  
**Snapshot date:** 2026-08-07  
**Verified during gap analysis:** 2026-08-10

> This document describes what is currently implemented/wired in the source snapshot. It is not the target Architecture Contract v0.2.

## 1. Target architecture

The normative target is defined in:

- [`specs/vestrace-architecture-contract-v0.2.md`](specs/vestrace-architecture-contract-v0.2.md)
- [`specs/README.md`](specs/README.md)

Gap-analysis artifacts:

- [`gap-analysis-v0.2.md`](gap-analysis-v0.2.md)
- [`requirement-coverage-v0.2.md`](requirement-coverage-v0.2.md)
- [`implementation-plan-v0.2.md`](implementation-plan-v0.2.md)

Canonical product definition:

> **Vestrace is a memory-first platform for persistent cognition shared across agents and executions.**

## 2. Repository/runtime shape

Current source is a Rust Edition 2024 workspace with:

```text
crates/vestrace-domain
crates/vestrace-application
crates/vestrace-infrastructure
crates/vestrace-http
crates/vestrace-cli
crates/vestrace-mcp
crates/vestrace-rig-spike
```

plus root integration tests, SQLx migrations, Docker/Compose, console and documentation.

The intended dependency direction is inward:

```text
CLI / HTTP / Infrastructure
        ↓
Application
        ↓
Domain
```

`vestrace-rig-spike` is experimental and not part of the supported runtime contract.

## 3. Currently wired Run/execution foundation

Current runtime includes:

- event-sourced Run command execution;
- deterministic replay/reducer;
- optimistic Run concurrency via `RunVersion`;
- event type/version validation;
- causation/correlation IDs;
- `occurred_at` + `recorded_at` on Run events;
- run checkpoints;
- checkpoint validation/restoration;
- run projection rebuild;
- append-only RunEvent history;
- PostgreSQL-backed projection/event repositories.

The repository also contains `WorkflowExecution`, `StepExecution`, model/tool execution records and other execution-history types. Gap analysis treats these as reusable typed history/operation foundations that must be explicitly reconciled to the Run-first ownership model so they do not evolve into a competing runtime.

## 4. Currently wired Memory foundation

Implemented/wired paths include:

- stable Memory identity;
- Memory revisions with sequential revision number;
- Memory lifecycle transitions;
- memory creation with first revision;
- structured memory variants;
- provenance source persistence;
- derivation primitives;
- memory revision with optimistic concurrency at the application/repository boundary;
- relation linking;
- event recording;
- idempotency/outbox integration;
- memory/event read paths;
- active-source database invariant;
- authorized hard purge path.

Current gaps relative to target include Claim/ClaimAssessment/Conflict, generic validity intervals, richer exact evidence references, classification/change-reason/actor/hash metadata on revisions and normalized reconciliation semantics.

## 5. Retrieval currently wired

Current runtime includes:

- retrieval request normalization;
- `RetrievalIntent` and `TimePerspective` domain/request types;
- PostgreSQL full-text search channel;
- RRF fusion;
- deterministic reranking;
- token-bounded ContextPack domain/builder;
- representation enum `Full/Summary/Atomic/Reference`;
- retrieval journaling;
- degraded flag/warnings when the wired text channel fails.

Important semantic limitations confirmed during gap analysis:

- only the text/FTS channel is wired into `RetrievalService` even though additional retriever ports exist;
- wired FTS does not implement `AsOf`/Timeline semantics from `TimePerspective`;
- FTS candidates currently return `revision_id: None`;
- ContextPackBuilder uses retrieval candidate explanation text as `rendered_text` rather than hydrating the exact MemoryRevision content;
- section assignment is currently a structural placeholder rather than semantic kind/authority-based placement;
- the current overflow path truncates text and labels it `Reference`; it is not yet a true provenance-preserving semantic compression ladder;
- reranking uses placeholder/neutral metadata for recency/provenance and derives importance/confidence from candidate score rather than canonical Memory metadata;
- full capability/classification/share/conflict governance is not yet enforced by the retrieval pipeline.

Therefore current retrieval is a useful **pipeline shell**, not yet target ContextPack 2.0.

## 6. Feedback / evaluations currently wired

Current source includes:

- model routing/execution foundations;
- workflow/step execution records;
- HTTP evaluation create/list/get endpoints;
- PostgreSQL evaluation repository;
- versioned Agent/Skill/Workflow domain definitions.

Current `EvaluationRecord` is a generic record (`model_id?`, name, status, score, summary, created_at). It does not yet implement target evaluator authority, exact execution/policy revision references, evidence references or learned-projection/proposal lifecycle.

## 7. Security/governance currently wired

Current code contains:

- 21 stable capability enum/string identifiers;
- sensitivity ordering `Public < Internal < Confidential < Restricted`;
- approval records with optional operation hash + expiry;
- `PolicyEngine` abstraction;
- allow-all, capability-set and workspace-scoped policy helpers;
- redaction service;
- audit repository;
- HTTP auth/local-trusted foundation;
- a simple numeric/monetary BudgetAccount reservation primitive;
- policy-bundle and approval schema foundations.

Important limitations:

- no target durable constrained `CapabilityGrant` aggregate was found;
- no delegation attenuation/depth runtime was found;
- no central risk model/effective-risk input was found;
- no evidence-producing target `PolicyDecision` was found;
- PolicyEngine enforcement is not demonstrated as a universal boundary across every HTTP/MCP/worker/internal path;
- full multi-dimensional budget accounting is not implemented.

## 8. Workspace / sharing / enterprise seeds

Current RLS/workspace isolation is a strong implemented foundation.

The repository also contains early enterprise types/schema:

```text
WorkspaceKek
CrossWorkspaceMemoryGrant
```

and migration 0087 creates workspace key metadata and a simple cross-workspace memory-grant table.

These are **type/schema seeds**, not target sharing conformance. The current simple grant is one-sided and lacks:

- immutable grant revisions;
- target-owned MemoryMount acceptance;
- grant/mount lifecycle and stale/revoke semantics;
- operation-specific permissions;
- SharedMemoryRef;
- independent source/target policy decisions.

## 9. Diagnostics / doctor / rebuild currently wired

Gap analysis corrected an earlier documentation assumption: **`doctor` and `rebuild` are implemented in the current main snapshot.**

### Doctor

`vestrace doctor`:

- connects to PostgreSQL;
- constructs `PgDiagnosticsRepository` + `DoctorService`;
- runs checks for migrations, extensions, broken refs, outbox lag, missing search documents, stale model health, expired leases and dead-letter jobs;
- prints Error/Warning/Info findings;
- returns non-zero on diagnostic errors.

### Rebuild

Current rebuild CLI supports conceptual targets:

```text
SearchDocuments
Embeddings
All
```

Search-document rebuild directly performs a deterministic SQL projection reconstruction and then runs diagnostics verification.

The current embeddings rebuild path does **not** regenerate embeddings; it validates prerequisite search-document coverage and returns zero rebuilt vectors. It must therefore be treated as incomplete functionality.

Target v0.5 will preserve useful low-level rebuild logic but place it behind:

```text
InvariantDefinition
→ HealthFinding
→ immutable RepairPlan
→ capability/policy
→ execution
→ verification
→ finding closure
```

## 10. Current diagnostics vs target Health model

Current `DiagnosticFinding` contains:

- code;
- severity `Error/Warning/Info`;
- optional object;
- message;
- remediation.

It does not yet contain target invariant/version, normalized scope, evidence refs, repairability, impact, fingerprint, occurrences, lifecycle/disposition, five-level severity or repair risk.

## 11. Current tool/external-effect seed

Current ToolInvocation models:

```text
Prepared
Committed
Verified
Failed
```

This is a useful seed, but target External Effects are not implemented. Missing major semantics include:

- immutable ExternalEffectIntent;
- adapter delivery/idempotency/reconciliation declarations;
- `UNKNOWN` / `RECONCILING` outcome;
- effect receipt;
- ACK vs confirmed outcome distinction;
- reversibility/compensation;
- precondition recheck/stale intent.

No high-risk autonomous external-effect support should be inferred from the current type.

## 12. Recovery currently wired

Run checkpoint/replay/projection recovery is implemented and should be preserved.

The current snapshot does not establish target cross-domain:

- Incident lifecycle;
- TrustState;
- startup recovery classification (`SAFE_TO_RESUME`, `MUST_RECONCILE`, etc.);
- RevalidationRun;
- trust-restoration barrier.

## 13. Crypto / data-governance seeds

Current code/schema contains useful seeds:

- sensitivity levels;
- redaction destinations;
- WorkspaceKek alias/algorithm metadata;
- ArtifactRevision content hash;
- SignedRunExport type;
- RLS.

No target runtime `SecretRef`, `DataPolicy`, retention/hold/deletion verification, classification lineage, complete key lifecycle or Qualification-backed crypto/governance profile was identified.

## 14. Artifacts

Current domain has Artifact and ArtifactRevision including media type/content hash/byte size, and artifact-related migrations exist.

The target requires additional governed storage/classification/provenance semantics and actual local content-addressed-storage wiring. Presence of artifact domain/schema does not prove that CAS is an active authoritative runtime path.

## 15. Acceptance / qualification state

Current v0.1 acceptance suite is valuable and includes deterministic scenario seeds for memory lifecycle, retrieval, workspace denial, provider sensitivity, self-elevation, purge approval, degraded mode, model fallback and audit.

However, many v0.1 acceptance scenarios compose domain objects directly rather than proving every wired runtime surface.

The suite is **not** target conformance/qualification because it lacks:

- stable requirement-ID mapping;
- profile dependency closure;
- deterministic named fault injection around crash/effect boundaries;
- exact build/config/environment identity;
- QualificationBundle;
- hard-gate profile runner/known-limitations contract.

## 16. Current API/CLI surfaces

Current implementation includes HTTP/MCP/CLI surfaces beyond the earliest P0 documentation. The exact current API surface should be read from router/source/schema artifacts rather than stale historical README claims.

Confirmed current CLI functionality includes at least:

- `server`;
- `migrate`;
- `worker`;
- `mcp`;
- `doctor`;
- `rebuild`.

Target architecture examples for future `repair`/`qualify` semantics do not imply those commands are currently implemented.

## 17. Target features not implied by current types

The current snapshot does **not** establish complete target support for:

- Claim-based persistent cognition model;
- full temporal/as-of semantics;
- complete mutation/reconciliation engine;
- ContextPack 2.0;
- governed learning lifecycle;
- constrained CapabilityGrant/delegation/risk model;
- cross-workspace grant+mount federation;
- findings-first Health/Repair protocol;
- governed ExternalEffectIntent/receipt/reconciliation;
- Incident/Trust/Revalidation model;
- target Crypto/Data Governance contract;
- Qualification/Conformance profiles.

## 18. Documentation rule

When current implementation and target v0.2 architecture differ:

1. this file describes current wiring from the inspected snapshot;
2. `docs/specs/vestrace-*.md` describes normative target architecture;
3. accepted `docs/adr/*` resolves target architecture decisions;
4. `docs/gap-analysis-v0.2.md` describes the transition gap;
5. no target feature may be claimed implemented without a wired runtime path and appropriate conformance evidence.

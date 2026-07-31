# Vestrace State Engine Follow-up Documentation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Convert the approved State Engine reconciliation into independent amendments and ADR/design packages without starting Rust implementation, migrations, runtime services or infrastructure work.

**Architecture:** Treat `Vestrace State Engine` only as an internal durable-state boundary spanning existing Horizons. Each independent concern receives a separate design document and review gate; implementation plans are written only after the corresponding design is explicitly approved.

**Tech Stack:** Markdown, existing Vestrace v0.1 and Harness v0.2 specifications, Horizon H1–H11B plans, repository search with `rg`, `git diff --check` and GitHub draft PR review.

## Global Constraints

- This plan is documentation-only.
- Do not modify Rust source, Cargo manifests, SQL migrations, CI, Docker files, generated schemas or runtime configuration.
- Do not create `H12 State Engine`, a second event store, a second execution runtime, a generic mutable `Task/Action/Attempt` hierarchy or a direct-SQL agent API.
- H1 remains the owner of `AgentRun`, `RunStep`, `RunEvent`, checkpoints, leases and logical replay.
- H2 remains the owner of final authorization, approvals, authorization tickets, budgets and quotas.
- H6 remains the owner of Artifact and ContextSnapshot bytes, provenance, mounts, export policy, retention, purge and memory consolidation.
- H8 remains the owner of secret material, connections and credential leases.
- H10 telemetry and projections never become authoritative Run state.
- H11 public interfaces never expose arbitrary SQL or storage credentials.
- `Unknown` never grants permission to retry an external action.
- A hash, identifier, reference or mount handle never grants read permission.
- Hidden chain-of-thought and provider reasoning traces are never persisted or exported.
- Each concern uses a dedicated branch and draft PR from current `main`.
- Merging a documentation plan does not authorize implementation.

## Dependency order

```text
1. State Engine boundary amendment
2. Capture-profile ADR
3. Event-schema compatibility ADR
4. Signed Run export design
5. Webhook extension design
6. Workspace envelope-encryption design
7. Cross-workspace memory-sharing design
8. Cross-plan normalization audit
9. One implementation plan per approved design
```

Tasks 4–7 are independent after Tasks 1–3 and may proceed in parallel branches.

---

### Task 1: Define the State Engine boundary

**Files:**
- Create: `docs/superpowers/specs/2026-07-31-vestrace-state-engine-boundary-amendment.md`
- Modify: `docs/superpowers/specs/2026-07-31-vestrace-harness-v0.2-design.md`
- Reference: `docs/superpowers/specs/2026-07-31-vestrace-state-engine-reconciliation-design.md`

**Interfaces:**
- Consumes: approved reconciliation terminology and H1–H11 ownership boundaries.
- Produces: one normative definition referenced by all later State Engine documents.

- [ ] **Step 1: Write the amendment**

The amendment must define:

```text
Vestrace State Engine = internal durable execution-state boundary formed by
existing H1–H10 application ports, authoritative PostgreSQL aggregates,
journals, checkpoints, policies, artifacts, memory and projections.
```

It must explicitly state that State Engine is not a product, service, database, public API, binary, second event store or second orchestration runtime.

- [ ] **Step 2: Record authoritative ownership**

Include this table:

```text
H1      Run lifecycle, journal, checkpoint, lease, logical replay
H2      authority, approvals, budgets
H3–H5   typed execution, planning, delegation
H6      context, artifacts, evidence, consolidation
H7      interactions, continuations, triggers
H8      connections, credentials
H10     audit, evaluation, projections
H11     public adapters
```

- [ ] **Step 3: Add anti-duplication rules**

The amendment must prohibit generic mutable `Task`, `Action` and `Attempt` aggregates when an owning Horizon already defines a typed aggregate, and must prohibit direct agent SQL.

- [ ] **Step 4: Add a concise reference to the Harness design**

Insert one subsection after the Harness architectural principles. Link to the reconciliation and amendment; do not copy the 56-row matrix.

- [ ] **Step 5: Verify and commit**

```bash
rg -n "State Engine|H12 State Engine|direct SQL|generic (Task|Action|Attempt)" docs/superpowers
git diff --check
git add docs/superpowers/specs/2026-07-31-vestrace-state-engine-boundary-amendment.md \
  docs/superpowers/specs/2026-07-31-vestrace-harness-v0.2-design.md
git commit -m "docs: define Vestrace State Engine boundary"
```

Expected: only the amendment and Harness design change.

---

### Task 2: Define State capture profiles

**Files:**
- Create: `docs/superpowers/specs/2026-07-31-vestrace-state-capture-profiles-adr.md`
- Modify: `docs/superpowers/plans/2026-07-31-vestrace-h10-observability-evaluation.md`
- Reference: `docs/superpowers/plans/2026-07-31-vestrace-h6-context-artifact-runtime.md`

**Interfaces:**
- Consumes: H10 capture modes and H6 governed capture Artifacts.
- Produces: operator presets that never alter canonical events or mandatory audit.

- [ ] **Step 1: Write the preset mapping**

```text
minimal       -> MetadataOnly baseline
operational   -> StructuredOnly baseline
reproducible  -> Redacted plus exact revision/reference manifests
forensic      -> Full only where current policy explicitly permits
```

- [ ] **Step 2: State the invariants**

The ADR must say:

1. Presets are configuration aliases, not a new event taxonomy.
2. Mandatory H1/H2/H6/H8 facts and H10 security audit remain enabled.
3. Classification, transfer, secret and retention policy may reduce capture.
4. No preset captures hidden reasoning or secret material.
5. Optional capture bytes are H6 Artifact revisions and remain purgeable.

- [ ] **Step 3: Amend H10 without replacing its enum**

Add one subsection mapping the presets to existing H10 capture modes. Preserve `Full`, `Redacted`, `StructuredOnly`, `MetadataOnly` and `Disabled` as the underlying contract.

- [ ] **Step 4: Verify and commit**

```bash
rg -n "minimal|operational|reproducible|forensic|MetadataOnly|StructuredOnly|Redacted|Full|Disabled" docs/superpowers
git diff --check
git add docs/superpowers/specs/2026-07-31-vestrace-state-capture-profiles-adr.md \
  docs/superpowers/plans/2026-07-31-vestrace-h10-observability-evaluation.md
git commit -m "docs: define State capture profiles"
```

Expected: no statement says a preset disables mandatory audit or changes the canonical Run journal.

---

### Task 3: Define durable event-schema compatibility

**Files:**
- Create: `docs/superpowers/specs/2026-07-31-vestrace-event-schema-compatibility-adr.md`
- Modify: `docs/superpowers/plans/2026-07-31-vestrace-h1-durable-run-core.md`
- Modify: `docs/superpowers/plans/2026-07-31-vestrace-h7-conversations-channels-triggers.md`
- Modify: `docs/superpowers/plans/2026-07-31-vestrace-h11-universal-vertical-slice-product-surface.md`

**Interfaces:**
- Consumes: H1 Run events, H7 public events and H11 public schema publication.
- Produces: stable event kinds, per-kind schema versions, generated schemas and pure read-time upcasters.

- [ ] **Step 1: Define the envelope**

```text
EventEnvelope
├── event_id
├── stable_event_kind
├── schema_version
├── aggregate_or_reference_ids
├── sequence_or_cursor
├── occurred_at?
├── recorded_at
├── payload
└── payload_hash
```

- [ ] **Step 2: Define compatibility rules**

The ADR must require:

1. Schema version starts at `1` for each stable event kind.
2. Existing event-kind semantics never change silently.
3. Breaking payload changes create a new version or event kind.
4. Generated JSON Schemas live in versioned repository paths.
5. Upcasters are pure read-time functions and never rewrite journal rows.
6. Unsupported future schemas fail closed with a stable compatibility error.
7. Models and extensions cannot register arbitrary runtime event kinds.
8. Internal Rust and external serialized compatibility are tested separately.

- [ ] **Step 3: Clarify ownership**

H1 owns Run ordering and persistence. H7 owns interaction/public-event semantics. H11 owns external schema publication and transport compatibility.

- [ ] **Step 4: Verify and commit**

```bash
rg -n "schema_version|event kind|RunEvent|PublicEvent|upcast|schema registry" docs/superpowers
git diff --check
git add docs/superpowers/specs/2026-07-31-vestrace-event-schema-compatibility-adr.md \
  docs/superpowers/plans/2026-07-31-vestrace-h1-durable-run-core.md \
  docs/superpowers/plans/2026-07-31-vestrace-h7-conversations-channels-triggers.md \
  docs/superpowers/plans/2026-07-31-vestrace-h11-universal-vertical-slice-product-surface.md
git commit -m "docs: define durable event compatibility"
```

Expected: no plan proposes in-place journal rewriting or a mutable model-controlled schema registry.

---

### Task 4: Design signed portable Run export

**Files:**
- Create: `docs/superpowers/specs/2026-07-31-vestrace-signed-run-export-design.md`
- Reference: H1, H2, H5, H6, H8, H10 and H11 documents.

**Interfaces:**
- Consumes: exact Run journal/checkpoint references, Artifact export policy, audit integrity proofs and administrative interfaces.
- Produces: a transport-neutral, side-effect-free `vestrace-run-export` format.

- [ ] **Step 1: Define goals and non-goals**

Support audit, migration, support diagnostics and offline evaluation. Do not define a database backup, credential backup, automatic replay package or Run activation mechanism.

- [ ] **Step 2: Define the archive layout**

```text
vestrace-run-export/
├── manifest.json
├── run.json
├── events.ndjson
├── plan-revisions/
├── checkpoints/
├── policy-and-budget-references/
├── context-manifests/
├── artifact-manifest.json
├── artifacts/
├── audit-chain-proof/
├── schemas/
├── checksums.sha256
└── signature.json
```

Define required and optional entries, canonical hashing, signature scope, format versioning and tombstones for purged content.

- [ ] **Step 3: Define authorization and import safety**

Require H2 authorization and H6 classification/export checks. Exclude credential bytes, usable secret references, backend paths and hidden reasoning. Import must verify format, hashes, signature and workspace binding before parsing, and must never start work or activate memory.

- [ ] **Step 4: Add acceptance scenarios**

Include complete export, purged capture, classification denial, tampered event file, unknown schema version, cross-workspace import without approved remap and inert inspection without side effects.

- [ ] **Step 5: Verify and commit**

```bash
rg -n "credential|secret|replay|import|activate|side effect|signature|checksum" \
  docs/superpowers/specs/2026-07-31-vestrace-signed-run-export-design.md
git diff --check
git add docs/superpowers/specs/2026-07-31-vestrace-signed-run-export-design.md
git commit -m "docs: design signed Run export"
```

Stop after the design PR and wait for written approval.

---

### Task 5: Design the webhook extension boundary

**Files:**
- Create: `docs/superpowers/specs/2026-07-31-vestrace-webhook-extension-design.md`
- Reference: H2, H7, H8, H10 and H11 documents.

**Interfaces:**
- Consumes: H7 trigger/public-event semantics, H2 authorization, H8 credentials and H10 audit.
- Produces: inbound and outbound webhook contracts disabled by default.

- [ ] **Step 1: Define inbound processing**

```text
request
→ endpoint revision lookup
→ origin/allowlist check
→ signature and timestamp verification
→ replay-window and dedup check
→ bounded payload quarantine
→ source fact creation
→ H7 trigger evaluation
→ optional proposal or Run under H2 policy
```

State that a webhook never mutates a Run directly.

- [ ] **Step 2: Define outbound processing**

```text
canonical source event
→ transactional delivery intent
→ destination and classification policy
→ bounded versioned payload
→ signature
→ at-least-once delivery
→ receipt/failure/retry audit
```

Define receiver idempotency requirements before retrying an external commitment.

- [ ] **Step 3: Define security and failure cases**

Cover endpoint revisioning, allowlists, maximum bytes, safe headers, replay protection, signing-key references, invalid signatures, stale timestamps, duplicate delivery, ambiguous completion and receivers without idempotency support.

- [ ] **Step 4: Verify and commit**

```bash
rg -n "direct.*Run|disabled by default|signature|replay|idempot|at-least-once|allowlist" \
  docs/superpowers/specs/2026-07-31-vestrace-webhook-extension-design.md
git diff --check
git add docs/superpowers/specs/2026-07-31-vestrace-webhook-extension-design.md
git commit -m "docs: design webhook extension boundary"
```

Stop after the design PR and wait for written approval.

---

### Task 6: Design workspace envelope encryption

**Files:**
- Create: `docs/superpowers/specs/2026-07-31-vestrace-workspace-envelope-encryption-design.md`
- Reference: H6, H8 and H10 documents.

**Interfaces:**
- Consumes: H8 `SecretBackendPort`, H6 blob storage/purge and H10 capture/audit.
- Produces: workspace-scoped key hierarchy, rotation, erasure and backup/restore contracts.

- [ ] **Step 1: Define the key hierarchy**

```text
workspace KEK reference       -> H8 SecretBackendPort
per-object/per-generation DEK -> wrapped by workspace KEK
ciphertext                    -> H6 blob store or bounded encrypted payload store
searchable metadata           -> minimized classified PostgreSQL projection
```

- [ ] **Step 2: Define encryption scope**

Encrypt optional context captures, sensitive Artifact/representation blobs, sensitive diagnostic captures, bounded non-indexed side payloads and exports when policy requires it.

Do not opaquely encrypt identifiers, revisions, RLS ownership columns, lifecycle state, operation fingerprints, content-free audit facts or fields required for deterministic uniqueness and concurrency.

- [ ] **Step 3: Define rotation and erasure**

Rotation creates a new key generation and resumable rewrap/re-encryption work. Cryptographic erasure destroys wrapped DEKs or the KEK generation and leaves a content-free tombstone. Historical hashes must not change silently.

- [ ] **Step 4: Define backup and failure cases**

Cover missing key material, interrupted rotation, restore without old KEK, search-projection cleanup after erasure and cross-workspace dedup disabled by default.

- [ ] **Step 5: Verify and commit**

```bash
rg -n "KEK|DEK|rotation|erasure|backup|restore|search projection|dedup" \
  docs/superpowers/specs/2026-07-31-vestrace-workspace-envelope-encryption-design.md
git diff --check
git add docs/superpowers/specs/2026-07-31-vestrace-workspace-envelope-encryption-design.md
git commit -m "docs: design workspace envelope encryption"
```

Stop after the design PR and wait for written approval.

---

### Task 7: Design explicit cross-workspace memory sharing

**Files:**
- Create: `docs/superpowers/specs/2026-07-31-vestrace-cross-workspace-memory-sharing-design.md`
- Reference: v0.1 memory design, H2, H6, H8, H10 and H11.

**Interfaces:**
- Consumes: workspace-bound Memory/MemoryRevision, H2 authorization and H6 context assembly.
- Produces: explicit `MemoryShareGrant` and read-only `MemoryMount` contracts without weakening RLS or redefining `global`.

- [ ] **Step 1: Preserve the v0.1 invariant**

State that `global` continues to mean the entire current workspace. Cross-workspace visibility never arises from a scope value, UUID, content hash, user identity or physical deduplication.

- [ ] **Step 2: Define the identities**

```text
MemoryShareGrant
├── source_workspace_id
├── target_workspace_id
├── allowed revision IDs or approved query scope
├── maximum classification
├── purpose
├── validity window
├── granted_by
├── policy_decision_id
└── revocation state

MemoryMount
├── target_workspace_id
├── share_grant_id
├── read_only = true
├── mounted_namespace
├── refresh policy
└── current source revision manifest
```

Write access is out of scope. A target may propose a local derived memory but cannot mutate source memory.

- [ ] **Step 3: Define retrieval, provenance and revocation**

Mounted memory must preserve source workspace, Memory ID, exact revision, classification, provenance and grant ID. Define behavior for expiry, revocation, active ContextSnapshots, superseded source revisions and purged source content.

- [ ] **Step 4: Prevent existence oracles**

Denied targets must not discover source memories, hashes, shares, counts or embedding matches. Filter authorization before ranking and aggregation.

- [ ] **Step 5: Verify and commit**

```bash
rg -n "global|MemoryShareGrant|MemoryMount|read-only|revocation|provenance|existence" \
  docs/superpowers/specs/2026-07-31-vestrace-cross-workspace-memory-sharing-design.md
git diff --check
git add docs/superpowers/specs/2026-07-31-vestrace-cross-workspace-memory-sharing-design.md
git commit -m "docs: design cross-workspace memory sharing"
```

Stop after the design PR and wait for written approval.

---

### Task 8: Run a cross-plan normalization audit

**Files:**
- Create: `docs/superpowers/specs/2026-07-31-vestrace-state-engine-normalization-report.md`
- Modify only existing documents containing confirmed contradictions.

**Interfaces:**
- Consumes: approved Tasks 1–7 documents.
- Produces: a contradiction report and minimal wording corrections; no new behavior.

- [ ] **Step 1: Search for duplicate architecture patterns**

```bash
rg -n "State Engine|second event store|direct SQL|arbitrary SQL|Run → Task|Task → Step|Action → Attempt|outcome_unknown|recovery_required|cross-workspace|memory mount|schema registry" \
  docs/superpowers/specs docs/superpowers/plans
```

- [ ] **Step 2: Classify every match**

Use exactly:

```text
consistent
terminology-only correction
behavioral conflict
intentional historical context
owned by future design
```

- [ ] **Step 3: Apply minimal corrections**

Every changed sentence must reference the owning approved document. Do not restructure unrelated plans or renumber Horizons/migrations.

- [ ] **Step 4: Verify ownership invariants**

Confirm one Run owner (H1), one final authorization owner (H2), one Artifact/Context byte owner (H6), one credential owner (H8), one audit/evaluation owner (H10), no direct agent SQL, no H12 State Engine and no automatic side-effect replay.

- [ ] **Step 5: Validate and commit**

```bash
git diff --check
git diff --stat
git add docs/superpowers/specs docs/superpowers/plans
git commit -m "docs: normalize State Engine ownership wording"
```

The PR body must list each modified document and the contradiction resolved.

---

### Task 9: Write one implementation plan per approved design

**Files:**
- Create one file per approved concern under `docs/superpowers/plans/`.

**Interfaces:**
- Consumes: one explicitly approved design from Tasks 2–7.
- Produces: one independently executable implementation plan.

- [ ] **Step 1: Record approval evidence**

Record the merged design path and PR/commit. Draft status or absence of comments is not approval.

- [ ] **Step 2: Create exactly one plan**

Expected paths:

```text
docs/superpowers/plans/2026-07-31-vestrace-state-capture-profiles.md
docs/superpowers/plans/2026-07-31-vestrace-event-schema-compatibility.md
docs/superpowers/plans/2026-07-31-vestrace-signed-run-export.md
docs/superpowers/plans/2026-07-31-vestrace-webhook-extension.md
docs/superpowers/plans/2026-07-31-vestrace-workspace-envelope-encryption.md
docs/superpowers/plans/2026-07-31-vestrace-cross-workspace-memory-sharing.md
```

Do not combine concerns.

- [ ] **Step 3: Apply the implementation-plan quality gate**

Each plan must contain exact files, typed interfaces, migrations only when required, failing tests before implementation, deterministic acceptance/security/replay tests and repository-boundary checks.

- [ ] **Step 4: Scan for placeholders**

```bash
rg -n "TBD|TODO|implement later|fill in details|appropriate error handling|write tests for|similar to" \
  docs/superpowers/plans/2026-07-31-vestrace-state-capture-profiles.md \
  docs/superpowers/plans/2026-07-31-vestrace-event-schema-compatibility.md \
  docs/superpowers/plans/2026-07-31-vestrace-signed-run-export.md \
  docs/superpowers/plans/2026-07-31-vestrace-webhook-extension.md \
  docs/superpowers/plans/2026-07-31-vestrace-workspace-envelope-encryption.md \
  docs/superpowers/plans/2026-07-31-vestrace-cross-workspace-memory-sharing.md
git diff --check
```

Expected: no matches and no whitespace errors.

- [ ] **Step 5: Open one draft PR per plan**

Each PR must state that merging the plan does not authorize implementation. Implementation begins only after the user explicitly ends the documentation-only phase.

## Completion gate

This program is complete only when:

1. the State Engine boundary amendment is merged;
2. capture-profile and event-compatibility ADRs are merged;
3. signed Run export, webhooks, workspace encryption and cross-workspace memory sharing each have an approved design;
4. the normalization report has no unresolved behavioral conflict;
5. each approved concern has a separate implementation plan;
6. no Rust code, migration or runtime service was created during the documentation-only program.

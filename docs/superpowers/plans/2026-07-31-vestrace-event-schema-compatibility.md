# Vestrace Durable Event Schema Compatibility Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:test-driven-development` for every implementation task and `superpowers:verification-before-completion` before claiming a task or branch complete. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Creating or merging this file does not authorize a feature branch, Rust changes, SQL migrations, schema generation, backfill, producer activation, journal reads, replay, import or release publication. Implementation begins only after an explicit future instruction ending the documentation-only phase.

**Approval evidence:**

- approved ADR: `docs/superpowers/specs/2026-07-31-vestrace-event-schema-compatibility-adr.md`;
- ADR merge commit: `3f7a196dfc879f628628b31d9eddb9b18770de10`;
- State Engine boundary: `docs/superpowers/specs/2026-07-31-vestrace-state-engine-boundary-amendment.md`;
- normalization report: `docs/superpowers/specs/2026-07-31-vestrace-state-engine-normalization-report.md`;
- normalization merge commit: `788d47ec6c3ac5e0336f3cc8fb4af4df685da3a3`;
- preceding implementation-plan merge: `33bd2325f6d299648ec6fb864c90a92a821277d6`.

**Goal:** Implement repository-owned durable event schema descriptors, deterministic canonical serialization, immutable per-event schema bindings, pure read-time upcasters, compatibility baselines, public schema publication and fail-closed reader/producer rollout across H1, H7, H10 and H11 without creating a global event store or changing owning-Horizon authority.

**Architecture:** Each owning Horizon continues to persist and order its own authoritative records. A shared library defines `StableEventKind`, positive per-kind `EventSchemaVersion`, immutable schema descriptors, canonical document hashing and pure upcaster contracts. A content-free sidecar binding records which exact descriptor and payload hash apply to each durable source record; it stores no event payload and owns no lifecycle. Existing historical source rows are never rewritten. H11 publishes public descriptors and compatibility metadata, while internal replay/import paths refuse unknown versions or incomplete upcast chains.

**Tech stack:** Existing Vestrace v0.1 plus H1, H7, H10 and H11; Rust Edition 2024; Tokio; Serde/Schemars; SQLx; PostgreSQL 17; repository canonical JSON; SHA-256; JSON Schema 2020-12; H1 logical replay; H7 durable public cursors; H10 audit-chain verification; H11 release/schema generation; proptest; deterministic fixtures and fake clocks.

---

## Global constraints

- H1 remains the only authority for `AgentRun`, `RunEvent`, Run sequence/version and logical replay.
- H7 remains the only authority for interaction events, public-event records and workspace public cursors.
- H10 remains the only authority for security-audit chains, evaluation records and safe replay.
- H11 remains the only authority for public DTO publication, generated SDK contracts and release compatibility assets.
- This concern does not create H12, a global event sequence, a global event payload table or a second journal.
- Every durable serialized event is identified by:

```text
(stable_event_kind, schema_version)
```

- `stable_event_kind` has immutable business meaning.
- `schema_version` is a positive integer scoped independently to one stable kind.
- Any serialized contract change creates a new version.
- A semantic change creates a new stable kind even when fields are similar.
- Existing event payloads, source columns, event IDs, sequences, cursors, timestamps, audit hashes and source rows are never rewritten to resemble the current model.
- Legacy rows receive content-free sidecar schema bindings; the source rows themselves are untouched.
- Upcasters are pure, deterministic, one-event-to-one-event read transformations. They perform no I/O, clock reads, randomness, policy decisions, authorization, decryption, secret lookup, external calls or durable writes.
- An upcaster never changes source event ID, workspace, owning authority, source sequence/cursor, recorded timestamp or historical payload hash.
- Unknown kind/version, descriptor-hash mismatch, invalid source payload, ambiguous upcast path or missing binding fail closed for semantic interpretation, replay, resume, import and mutation.
- H10 audit-chain verification always verifies original stored bytes and original chain hashes before any presentation upcast.
- Public transports may preserve an already-authorized unknown public envelope as an opaque SDK value, but cannot execute typed behavior, replay it or map it to a known semantic variant.
- Models, packages, extensions, webhooks and administrators cannot register Vestrace-owned stable kinds at runtime.
- Registry descriptors and generated schema assets change only through reviewed repository changes owned by the source Horizon.
- Registry metadata is compiled/generated repository content, not a mutable PostgreSQL schema registry.
- A schema hash for an existing `(surface, stable_event_kind, schema_version)` never changes.
- Producers of a new version are activated only after all live required readers advertise support for that version.
- Rollout state and reader capability heartbeats are operational metadata; they never alter event authority or ordering.
- Existing applied migrations are never edited. This concern reserves:

```text
0089_durable_event_schema_bindings_and_backfill.sql
0090_event_schema_rollout_capabilities_and_activations.sql
0091_event_schema_rls_constraints_and_indexes.sql
```

- No other concern may reuse migration numbers `0089`–`0091`.
- CI requires no public model, network service, telemetry backend or permanent credential.
- Future implementation branch: `feat/durable-event-schema-compatibility`.

---

## Locked file structure

```text
Cargo.toml
Cargo.lock
.github/workflows/ci.yml

crates/vestrace-domain/src/
  id.rs
  event_schema/{mod,kind,version,reference,descriptor,compatibility,error}.rs
  run/event.rs
  conversation/interaction.rs
  channel/event.rs
  audit/record.rs

crates/vestrace-application/src/
  event_schema/{mod,ports,registry,writer,reader,upcast,backfill,rollout}.rs
  run/replay.rs
  channel/projection.rs
  audit/query.rs
  composition/event_schema.rs

crates/vestrace-application/tests/
  event_schema_registry.rs
  event_schema_writer.rs
  event_schema_reader.rs
  event_schema_upcast.rs
  event_schema_rollout.rs
  event_schema_backfill.rs

crates/vestrace-event-schema-runtime/
  Cargo.toml
  src/{lib,canonical,hash,registry,validator,upcaster,generated,error}.rs

crates/vestrace-event-schema-test-support/
  Cargo.toml
  src/{lib,fixtures,schemas,upcasters,legacy,corruption,conformance}.rs

crates/vestrace-infrastructure/src/postgres/
  event_schema/{mod,binding_repository,backfill_repository,rollout_repository,capability_repository}.rs
  run/store.rs
  conversation/interaction_repository.rs
  channel/event_repository.rs
  audit/record_repository.rs

crates/vestrace-domain/src/public_api/
  event.rs
  schema.rs

crates/vestrace-channel-http/src/
  routes/events.rs
  routes/schemas.rs
  dto/event.rs

crates/vestrace-sdk-rust/src/
  events.rs
  models.rs

packages/sdk-typescript/src/
  events.ts
  generated/events.ts

schemas/
  events/internal/h1/*.json
  events/internal/h7/*.json
  events/internal/h10/*.json
  events/v1/*.json
  events/v1/index.json
  events/v1/compatibility.json
  compatibility/event-schema-baseline.json

migrations/
  0089_durable_event_schema_bindings_and_backfill.sql
  0090_event_schema_rollout_capabilities_and_activations.sql
  0091_event_schema_rls_constraints_and_indexes.sql

tests/
  run_event_schema_persistence.rs
  run_replay_schema_compatibility.rs
  interaction_event_schema_persistence.rs
  public_event_schema_cursor.rs
  audit_event_schema_integrity.rs
  event_schema_backfill_restart.rs
  event_schema_binding_atomicity.rs
  event_schema_rollout_readers_first.rs
  event_schema_unknown_fail_closed.rs
  event_schema_release_baseline.rs
  event_schema_rls.rs
  event_schema_acceptance.rs

scripts/
  generate-event-schemas.sh
  verify-event-schema-registry.sh
  verify-event-schema-baseline.sh
  verify-event-upcaster-purity.sh
  verify-no-event-source-row-rewrite.sh
  verify-event-schema-migration-ownership.sh
```

---

## Normative domain contracts

### Stable kind

```rust
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct StableEventKind(String);

impl StableEventKind {
    pub fn parse(value: impl Into<String>) -> Result<Self, EventSchemaError>;
    pub fn as_str(&self) -> &str;
}
```

Validation:

- lowercase ASCII dotted segments;
- each segment is 1–64 bytes;
- total length is at most 255 bytes;
- empty segments, leading/trailing dots, whitespace and transport-specific names are rejected;
- kind names describe facts in past tense or durable fact form;
- Rust `Debug` output and enum variant names are never used implicitly as wire names.

### Per-kind version

```rust
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct EventSchemaVersion(u16);

impl EventSchemaVersion {
    pub const V1: Self = Self(1);
    pub fn new(value: u16) -> Result<Self, EventSchemaError>;
    pub const fn get(self) -> u16;
}
```

Zero is invalid. Version is not API major, migration number, release number or Run version.

### Authority and exposure

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventAuthorityFamily {
    H1Run,
    H7Interaction,
    H7Public,
    H10Audit,
    H10Evaluation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventSchemaExposure {
    Internal,
    Audit,
    Public,
    ExportOnly,
}
```

The initial compatibility slice must support H1 Run events, H7 interaction/public events and H10 audit records. H10 evaluation events use the same registry contracts and may be registered when their owning implementation exists; no placeholder descriptor is published.

### Schema reference

```rust
#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct EventSchemaRef {
    pub stable_kind: StableEventKind,
    pub version: EventSchemaVersion,
}
```

`EventSchemaRef` identifies a serialized contract, not a permission or event instance.

### Compatibility classification

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventSchemaChangeClass {
    Initial,
    WireChange,
    NewSemanticKind,
    DocumentationOnly,
}
```

Rules:

- version 1 is `Initial`;
- any changed schema bytes, validation, canonicalization, bounds or serialized enum set are `WireChange` with a new version;
- changed business meaning is `NewSemanticKind` and cannot reuse the old stable kind;
- `DocumentationOnly` requires byte-identical generated schema and descriptor hashes.

### Descriptor

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventSchemaDescriptor {
    pub schema_ref: EventSchemaRef,
    pub authority: EventAuthorityFamily,
    pub exposure: EventSchemaExposure,
    pub rust_type_name: &'static str,
    pub json_schema_bytes: &'static [u8],
    pub schema_hash: [u8; 32],
    pub canonicalization_revision: &'static str,
    pub change_class: EventSchemaChangeClass,
    pub previous_version: Option<EventSchemaVersion>,
    pub upcaster_key_from_previous: Option<&'static str>,
    pub test_vector_hash: [u8; 32],
}
```

Descriptor invariants:

- `(kind, version)` is globally unique in the compiled registry;
- versions for one kind are contiguous from 1;
- schema hash matches exact generated bytes;
- V1 has no predecessor/upcaster;
- V2+ has exactly one predecessor and one registered upcaster;
- exposure `Public` has a generated public JSON Schema asset;
- authority agrees with the module that exports the descriptor;
- a descriptor cannot be constructed from runtime JSON, SQL or extension content.

### Source reference and binding

```rust
#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct DurableEventSourceRef {
    pub authority: EventAuthorityFamily,
    pub source_id: uuid::Uuid,
}

#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct DurableEventSchemaBinding {
    pub id: DurableEventSchemaBindingId,
    pub workspace_id: WorkspaceId,
    pub source: DurableEventSourceRef,
    pub schema_ref: EventSchemaRef,
    pub schema_hash: [u8; 32],
    pub original_payload_hash: [u8; 32],
    pub original_recorded_at: Timestamp,
    pub binding_origin: EventSchemaBindingOrigin,
    pub created_at: Timestamp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventSchemaBindingOrigin {
    AtomicWriter,
    LegacyBackfill,
    VerifiedImport,
}
```

Binding rules:

- one binding exists per exact `(authority, source_id)`;
- binding is append-only;
- binding contains no event payload;
- source UUID, hash or schema reference is never a capability;
- new source record and binding commit atomically in the owning transaction;
- legacy backfill creates only the binding and never updates the source row;
- an imported binding is accepted only after schema, payload hash and import integrity verification;
- hard purge may retain the content-free binding according to owning audit policy.

### Canonical event document

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalEventDocument {
    pub schema_ref: EventSchemaRef,
    pub event_id: uuid::Uuid,
    pub workspace_id: WorkspaceId,
    pub authority: EventAuthorityFamily,
    pub aggregate_references: Vec<ExactRevisionRef>,
    pub sequence_or_cursor: Option<u64>,
    pub correlation_id: Option<CorrelationId>,
    pub causation_id: Option<CausationId>,
    pub occurred_at: Option<Timestamp>,
    pub recorded_at: Timestamp,
    pub payload: serde_json::Value,
}
```

This is a serialization/read boundary, not a persisted universal aggregate. Owning tables remain authoritative.

Canonical bytes use the repository canonical JSON contract:

- object keys sorted by UTF-8 byte order;
- array order preserved;
- no insignificant whitespace;
- finite JSON numbers only;
- timestamps normalized to the repository wire format;
- schema-specific units and bounds validated before hashing;
- no transport framing, compression, HTTP headers or database physical metadata.

### Read result

```rust
#[derive(Clone, Debug)]
pub struct InterpretedDurableEvent<T> {
    pub source_ref: DurableEventSourceRef,
    pub original_schema_ref: EventSchemaRef,
    pub current_schema_ref: EventSchemaRef,
    pub original_payload_hash: [u8; 32],
    pub original_document: CanonicalEventDocument,
    pub current_document: CanonicalEventDocument,
    pub typed_value: T,
    pub applied_upcasters: Vec<&'static str>,
}
```

The original document/hash remain available for audit and diagnostics. `current_document` never replaces persisted bytes.

---

## Compile-time registry

### Registry interface

```rust
pub trait EventSchemaRegistryPort: Send + Sync {
    fn descriptor(
        &self,
        schema_ref: &EventSchemaRef,
    ) -> Result<&EventSchemaDescriptor, EventSchemaError>;

    fn current_version(
        &self,
        kind: &StableEventKind,
    ) -> Result<EventSchemaVersion, EventSchemaError>;

    fn upcast_path(
        &self,
        from: &EventSchemaRef,
        to: &EventSchemaRef,
    ) -> Result<Vec<&'static dyn EventUpcaster>, EventSchemaError>;

    fn registry_hash(&self) -> [u8; 32];
}
```

The production registry is assembled explicitly in `composition/event_schema.rs` from source-owned descriptor lists:

```rust
h1_run_event_descriptors()
h7_interaction_event_descriptors()
h7_public_event_descriptors()
h10_audit_event_descriptors()
```

No dynamic plugin discovery, inventory submitted by models or SQL-loaded descriptor is allowed.

### Baseline inventory

Each source owner adds an exhaustive descriptor function beside its typed event definitions.

Requirements:

- every persisted H1 `RunEvent` variant maps through an exhaustive match to one stable kind and schema version;
- every persisted H7 interaction/public event representation has an explicit descriptor;
- H10 audit record-body schema and chain-record envelope are separately identified when their serialized contracts differ;
- no wildcard match maps unknown future enum variants to a generic kind;
- descriptor tests fail when an owning enum gains a variant without a registered mapping;
- internal and public mappings are explicit and may use different stable kinds only through a reviewed mapping table.

### Generated assets

`generate-event-schemas.sh` performs deterministic generation and writes:

```text
schemas/events/internal/h1/<kind>.v<version>.json
schemas/events/internal/h7/<kind>.v<version>.json
schemas/events/internal/h10/<kind>.v<version>.json
schemas/events/v1/<public-kind>.v<version>.json
schemas/events/v1/index.json
schemas/events/v1/compatibility.json
schemas/compatibility/event-schema-baseline.json
```

Generated index entries contain:

```text
stable kind
schema version
schema hash
owning authority
exposure
previous version
upcaster key
canonicalization revision
test-vector hash
```

Generation sorts entries deterministically and rejects duplicate paths or keys.

---

## Upcaster contract

```rust
pub trait EventUpcaster: Send + Sync {
    fn key(&self) -> &'static str;
    fn source(&self) -> EventSchemaRef;
    fn target(&self) -> EventSchemaRef;

    fn upcast(
        &self,
        source: &CanonicalEventDocument,
    ) -> Result<CanonicalEventDocument, EventSchemaError>;
}
```

Purity and identity invariants:

- source and target stable kind are equal;
- target version is source version + 1;
- event ID, workspace, authority, sequence/cursor, correlation, causation and recorded timestamp are byte-for-byte preserved;
- original payload hash is not overwritten or recomputed as historical truth;
- generated fields derive only from source fields and descriptor constants;
- no current clock, random UUID, environment variable, locale, network, database, policy, secret or mutable registry access;
- one input produces exactly one output or an error;
- split/merge semantics require new stable kinds and are not represented as an upcaster;
- every output validates against the exact target schema before the next step;
- the complete path validates against the current target schema;
- unsupported source versions fail before typed interpretation.

`verify-event-upcaster-purity.sh` statically rejects imports/usages of SQLx, Reqwest, Tokio time, random generators, environment access, H2/H8 ports and filesystem APIs from upcaster modules.

---

## Writer contract

```rust
pub struct DurableEventWriteDocument {
    pub source_ref: DurableEventSourceRef,
    pub schema_ref: EventSchemaRef,
    pub canonical_document: CanonicalEventDocument,
    pub payload_hash: [u8; 32],
    pub schema_hash: [u8; 32],
}

pub trait DurableEventSchemaWriterPort: Send + Sync {
    fn prepare<T: serde::Serialize>(
        &self,
        source_ref: DurableEventSourceRef,
        schema_ref: EventSchemaRef,
        value: &T,
    ) -> Result<DurableEventWriteDocument, EventSchemaError>;
}
```

Writer behavior:

1. resolve exact compiled descriptor;
2. serialize through the owner adapter into a canonical document;
3. validate source document against exact schema;
4. compute canonical payload hash;
5. return a binding payload for the owner transaction;
6. refuse an unregistered kind/version or descriptor hash mismatch.

The owning repository inserts source row and schema binding in one transaction. The shared writer never writes event rows itself.

### H1 atomicity

`RunStorePort` transaction remains authoritative:

```text
updated AgentRun aggregate
+ exactly one RunEvent
+ optional step/checkpoint changes
+ follow-up work items
+ exactly one schema binding for the RunEvent
= one transaction
```

Failure to create the schema binding rolls back the logical Run mutation.

### H7 atomicity

Interaction/public-event writes keep their existing ownership. Each durable interaction or public record receives its binding in the same transaction as source creation and cursor allocation.

### H10 atomicity

Audit intent/record ownership remains H10. The schema binding is committed with the audit record append. Existing audit chain hash covers the exact original audit body according to its own versioned algorithm; the sidecar payload hash is additional evidence, not a replacement chain.

---

## Reader contract

```rust
pub trait DurableEventSourceLoaderPort: Send + Sync {
    async fn load_original_document(
        &self,
        context: &RequestContext,
        source: &DurableEventSourceRef,
    ) -> Result<CanonicalEventDocument, ApplicationError>;
}

pub struct DurableEventReader<R, L> {
    registry: R,
    loader: L,
}
```

Reader algorithm:

```text
1. authorize/read source through owning Horizon
2. load immutable schema binding
3. resolve exact descriptor and verify schema hash
4. reconstruct canonical original document from source row
5. validate original document against source schema
6. recompute and compare original payload hash
7. resolve one unique contiguous upcast path
8. apply and validate each pure step
9. deserialize only the final validated document
10. return original and interpreted views
```

Reader errors are typed:

```rust
pub enum EventSchemaReadError {
    BindingMissing,
    UnknownKind,
    UnsupportedVersion,
    DescriptorHashMismatch,
    OriginalPayloadHashMismatch,
    SourceSchemaInvalid,
    UpcastPathMissing,
    UpcastPathAmbiguous,
    UpcastStepFailed,
    TargetSchemaInvalid,
    TypedDecodeFailed,
}
```

No error falls back to the current Rust enum or best-effort field guessing.

---

## Legacy binding and backfill

### Principle

Historical source rows remain byte/column identical. Backfill creates only `durable_event_schema_bindings` rows and operational progress records.

### Backfill job

```rust
pub struct EventSchemaBackfillJob {
    pub id: EventSchemaBackfillJobId,
    pub authority: EventAuthorityFamily,
    pub legacy_schema_ref: EventSchemaRef,
    pub lower_bound: Option<uuid::Uuid>,
    pub upper_bound: Option<uuid::Uuid>,
    pub cursor: Option<uuid::Uuid>,
    pub status: EventSchemaBackfillStatus,
    pub processed: u64,
    pub bound: u64,
    pub failed: u64,
    pub lease_owner: Option<WorkerId>,
    pub lease_until: Option<Timestamp>,
    pub state_revision: u64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
```

Status:

```text
Created
Running
Paused
Completed
Failed
Cancelled
```

Backfill behavior:

- reads rows in stable UUID order in bounded batches;
- reconstructs the exact V1 document using an owner-specific legacy adapter;
- validates against the frozen V1 descriptor;
- computes payload hash;
- inserts binding idempotently;
- equal existing binding is success;
- conflicting binding is a hard integrity failure;
- restart resumes after the durable cursor;
- source row update/delete is never issued;
- source payload is never copied into the backfill table;
- failure stores bounded code and source reference, not payload;
- backfill does not advance Run/public/audit sequence;
- backfill never emits a new canonical source event for each historical row.

`verify-no-event-source-row-rewrite.sh` rejects `UPDATE` statements against H1/H7/H10 source tables from event-schema backfill modules.

---

## Rollout and readers-first activation

### Rollout phase

```rust
pub enum EventSchemaRolloutPhase {
    LegacyReaders,
    DualWriteBindings,
    Backfilling,
    BindingsRequired,
    VersionedProducersEnabled,
}

pub struct EventSchemaRolloutState {
    pub deployment_id: DeploymentId,
    pub phase: EventSchemaRolloutPhase,
    pub required_registry_hash: [u8; 32],
    pub expected_revision: u64,
    pub state_revision: u64,
    pub changed_by: PrincipalId,
    pub policy_decision_id: PolicyDecisionId,
    pub updated_at: Timestamp,
}
```

`expected_revision` belongs to the phase-change command and is not persisted in the aggregate. Persisted state contains only `state_revision`.

### Reader capability

```rust
pub struct EventSchemaReaderCapability {
    pub deployment_id: DeploymentId,
    pub node_id: DeploymentNodeId,
    pub release_hash: [u8; 32],
    pub registry_hash: [u8; 32],
    pub supported_schema_refs_hash: [u8; 32],
    pub supports_bindings_required: bool,
    pub heartbeat_at: Timestamp,
    pub valid_until: Timestamp,
}
```

Capabilities are operational and expire. They are not capabilities in the authorization sense.

### Producer activation

```rust
pub struct EventSchemaProducerActivation {
    pub deployment_id: DeploymentId,
    pub schema_ref: EventSchemaRef,
    pub producer_component: ExactRevisionRef,
    pub minimum_reader_registry_hash: [u8; 32],
    pub activated_by: PrincipalId,
    pub policy_decision_id: PolicyDecisionId,
    pub activated_at: Timestamp,
    pub revoked_at: Option<Timestamp>,
    pub state_revision: u64,
}
```

Activation rules:

- descriptor exists in the installed release;
- every non-expired required reader advertises the target schema reference;
- no live reader reports a conflicting registry hash;
- all backfill jobs required for `BindingsRequired` completed without unresolved conflict;
- H2 authorizes rollout-state and producer-activation changes;
- activation never changes old event rows;
- revocation stops new writes of that version but leaves existing events readable;
- an unavailable rollout repository prevents producing a new version;
- V1 dual-write can continue according to explicit safe deployment configuration.

---

## Persistence model

### Migration `0089_durable_event_schema_bindings_and_backfill.sql`

Create:

```text
durable_event_schema_bindings
event_schema_backfill_jobs
event_schema_backfill_failures
```

Required columns for bindings:

```text
id uuid primary key
workspace_id uuid not null
authority_family text not null
source_id uuid not null
stable_event_kind text not null
schema_version integer not null
schema_hash bytea not null
original_payload_hash bytea not null
original_recorded_at timestamptz not null
binding_origin text not null
created_at timestamptz not null
```

Constraints:

- unique `(authority_family, source_id)`;
- positive schema version;
- hashes exactly 32 bytes;
- valid authority/origin enums;
- application role cannot update/delete bindings;
- failures contain bounded error code/reference only;
- backfill job state uses optimistic concurrency and leases;
- no payload JSON/body/blob column exists in these tables.

### Migration `0090_event_schema_rollout_capabilities_and_activations.sql`

Create:

```text
event_schema_rollout_states
event_schema_reader_capabilities
event_schema_producer_activations
event_schema_rollout_history
```

Constraints:

- one rollout state per deployment;
- one current reader capability per `(deployment_id, node_id)`;
- one active producer activation per `(deployment_id, stable_event_kind, schema_version, producer_component)`;
- rollout history append-only;
- activation/revocation use optimistic state revision;
- hashes exactly 32 bytes;
- reader heartbeats are operational and do not emit canonical Run/public/audit events.

### Migration `0091_event_schema_rls_constraints_and_indexes.sql`

Add:

- forced RLS for workspace-owned bindings/backfill failures;
- deployment-admin policies for rollout/capability/activation tables;
- indexes by workspace/source, kind/version, backfill status/cursor, reader expiry and active activation;
- deferred source-existence validation by `authority_family` for new atomic-writer bindings;
- a privileged backfill validation path that verifies source ownership without disabling RLS;
- append-only triggers for bindings and rollout history;
- application-role denial of direct arbitrary inserts outside repository functions/transactions;
- migration ownership comments for `0089`–`0091`.

No migration modifies historical source event rows.

---

## H1 integration

### Source-owned mapping

Modify `run/event.rs` so every `RunEvent` provides:

```rust
pub trait DurableRunEventSchema {
    fn stable_kind(&self) -> StableEventKind;
    fn schema_version(&self) -> EventSchemaVersion;
    fn to_canonical_document(&self) -> Result<CanonicalEventDocument, EventSchemaError>;
}
```

Requirements:

- exhaustive variant match;
- one logical Run mutation still emits exactly one RunEvent;
- schema binding insert is part of the existing `RunStorePort` transaction;
- lease/heartbeat/work-queue operational changes remain outside Run events and event-schema bindings unless they already have an owning durable event contract;
- replay reads source binding and schema before applying event semantics;
- unknown/invalid version produces `RunReplayDisposition::SchemaUnsupported` and does not partially mutate the reconstructed aggregate;
- replay from checkpoint validates every later event binding in sequence order;
- upcasting does not increment `RunVersion` and emits no new RunEvent.

### H1 tests

Required tests:

- create Run commits `run.created` source row and binding atomically;
- forced binding failure rolls back Run creation;
- every later mutation has sequence equal to RunVersion and one binding;
- V1 fixture replays through a V1→V2 upcaster without rewriting fixture bytes;
- unknown V99 blocks replay before applying the unknown event;
- payload-hash corruption is detected;
- an upcaster cannot change sequence, Run ID or recorded timestamp;
- restart produces the same reconstructed state and applied-upcaster list.

---

## H7 integration

### Interaction events

Every persisted `InteractionEvent` receives an explicit stable kind/version binding in the same transaction as the append-only interaction row.

Rules:

- corrections/retractions/redaction notices remain new events with their own bindings;
- external channel metadata cannot choose kind or version;
- large H6 references remain references and do not enter schema registry as embedded bytes;
- legacy interaction rows are bound through the owner-specific V1 adapter.

### Public events

`PublicEventRecord` remains the sole source for H11 event streaming.

Public projection rules:

- internal source event is read and validated through its owner reader;
- mapping to public kind/version is explicit and repository-owned;
- public payload validates against the exact public schema before cursor allocation;
- source reference and public schema binding commit with the public record/cursor transaction;
- failure to project one event does not fabricate a different semantic event;
- a projection failure is durable and operator-visible;
- cursor ordering remains H7-owned and is not replaced by event ID or schema sequence.

### H7 tests

Required tests:

- duplicate interaction idempotency key returns original event/binding;
- public reconnect remains at-least-once and deduplicable by event ID/cursor;
- same cursor never represents different schema/payload bytes;
- unknown internal source schema blocks typed public projection;
- public mapping cannot expose an internal-only descriptor;
- public schema version is stable across HTTP SSE, CLI follow and SDK decoding;
- an older SDK receives a bounded `UnknownPublicEvent` rather than treating it as a known variant;
- opaque unknown public event cannot invoke command helpers or continuation behavior.

---

## H10 integration

### Audit records

H10 audit-chain bytes remain authoritative.

Integration rules:

- schema descriptor identifies exact audit body/envelope format;
- binding payload hash is computed from exact original canonical audit document;
- existing `record_hash`/previous-hash chain remains unchanged;
- audit verification first checks chain sequence/hashes/signatures against original bytes;
- optional upcast occurs only after integrity succeeds;
- upcast result is presentation/query state and never replaces chain evidence;
- old audit rows are not re-signed after upcast;
- unknown schema produces `AuditIntegrityDisposition::Incomplete` or a typed schema-unsupported result, never `Valid` by assumption;
- purge leaves content-free schema binding/tombstone according to H10 policy.

### H10 tests

Required tests:

- V1 audit fixture retains identical chain head before and after new-reader deployment;
- upcasted view does not alter record hash or checkpoint signature input;
- payload hash and chain record hash are distinguished and both verified;
- unknown version prevents semantic audit query but preserves raw integrity diagnostics;
- schema binding missing is detected as incomplete evidence;
- audit export includes original schema reference, original payload hash and applied-upcaster metadata without claiming rewritten history.

---

## H11 publication and SDK behavior

### Public envelope

```rust
pub struct PublicEventEnvelope {
    pub event_id: PublicEventId,
    pub cursor: PublicEventCursor,
    pub stable_event_kind: StableEventKind,
    pub schema_version: EventSchemaVersion,
    pub recorded_at: Timestamp,
    pub correlation: PublicCorrelationRefs,
    pub payload: serde_json::Value,
    pub payload_hash: [u8; 32],
}
```

The DTO contains no internal schema path, database type, secret, backend reference or authority shortcut.

### Schema endpoints

Add read-only routes:

```text
GET /v1/schemas/events
GET /v1/schemas/events/{stable_kind}/versions/{schema_version}
GET /v1/schemas/events/compatibility
```

Rules:

- routes serve installed immutable generated assets;
- ETag is the exact schema/index hash;
- unknown kind/version returns bounded `not_found`;
- authorization follows public schema visibility policy;
- internal/audit/export-only descriptors are never served from public routes;
- routes do not create runtime schema registrations.

### SDKs

Rust and TypeScript SDKs expose:

```text
KnownPublicEvent<T>
UnknownPublicEvent
PublicEventEnvelope
EventSchemaRef
```

Known decoding requires exact generated `(kind, version)` support and schema validation. Unknown decoding preserves only already-authorized bounded public payload and metadata. It never casts to the latest known type.

### Compatibility baseline

`verify-event-schema-baseline.sh` fails when:

- an existing key has changed schema hash;
- a schema file disappeared without explicit public-major removal policy;
- a version was skipped;
- V2+ lacks one predecessor/upcaster;
- a stable kind changed owner or exposure unexpectedly;
- generated bytes differ from committed assets;
- SDK generated unions omit a public descriptor;
- release manifest omits exact event-schema registry/baseline hashes.

---

## Import/export interaction

Signed Run export and other portable bundles preserve original event identity:

```text
stable kind
original schema version
schema hash
original payload hash
original serialized document or referenced bytes
```

Import behavior:

- integrity verification may succeed for an unknown future schema;
- semantic import, replay, resume and activation remain blocked until the reader supports it;
- import never rewrites unknown payload to the current version;
- upcast occurs only in the interpreted view after original integrity verification;
- exported bundle records applied-upcaster metadata separately from original event files;
- event schema support grants no Run/memory/artifact capability.

---

## Failure semantics

### Missing binding

After rollout reaches `BindingsRequired`, a missing binding is an integrity error. The reader does not infer the current schema from Rust types.

During explicit `LegacyReaders`/`DualWriteBindings` transition only, owner-specific V1 inference may be used for read diagnostics and backfill. Inference:

- is limited to frozen source tables/rows predating the rollout boundary;
- cannot interpret rows written after dual-write activation;
- cannot enable V2+ production;
- records a bounded diagnostic;
- is removed from the normal read path when `BindingsRequired` activates.

### Unknown kind/version

- no typed interpretation;
- no replay/resume/import mutation;
- no automatic downcast;
- no model-assisted guess;
- no new source event emitted merely to hide the failure;
- operator receives exact safe schema reference and supported-version range.

### Hash mismatch

Descriptor hash mismatch or payload hash mismatch is an integrity failure. It is never treated as a compatibility warning.

### Upcaster failure

- source event remains immutable;
- reader returns typed failure with failing step key;
- partial upcast output is discarded;
- no current aggregate mutation occurs;
- H10 records content-free diagnostic/audit intent when required.

### Registry unavailable

The production registry is compiled into the binary. A construction/validation failure prevents service readiness rather than starting with a partial registry.

### Rollout repository unavailable

Existing enabled V1 writers may continue only under the exact safe rollout configuration. A new schema version producer fails closed.

---

## Implementation tasks

### Task 1: Add event-schema value types and registry invariants

**Files:**

- create `crates/vestrace-domain/src/event_schema/*`;
- modify `crates/vestrace-domain/src/id.rs` and `lib.rs`;
- create `crates/vestrace-application/tests/event_schema_registry.rs`.

- [ ] Write failing tests for kind validation, positive versions, descriptor uniqueness, contiguous version chains, owner/exposure rules and deterministic registry hash.
- [ ] Implement IDs and domain types.
- [ ] Implement descriptor construction validation.
- [ ] Implement explicit registry assembly with no runtime registration method.
- [ ] Run domain/application tests and clippy.

### Task 2: Implement canonical serialization, validation and payload hashing

**Files:**

- create `crates/vestrace-event-schema-runtime/*`;
- create `crates/vestrace-event-schema-test-support/*`;
- create `crates/vestrace-application/tests/event_schema_writer.rs`.

- [ ] Write failing canonicalization vectors covering nested maps, arrays, Unicode, integers, timestamps and non-finite-number rejection.
- [ ] Implement canonical document serialization and SHA-256 hashing.
- [ ] Implement descriptor-schema validation.
- [ ] Implement `DurableEventSchemaWriterPort`.
- [ ] Verify same logical document produces identical bytes/hash across process restarts.

### Task 3: Add pure upcasters and the fail-closed reader

**Files:**

- create application `event_schema/{reader,upcast}.rs`;
- create runtime `upcaster.rs`;
- create tests `event_schema_reader.rs` and `event_schema_upcast.rs`;
- create `scripts/verify-event-upcaster-purity.sh`.

- [ ] Write failing tests for missing, ambiguous and cyclic paths.
- [ ] Write identity-preservation and no-I/O conformance tests.
- [ ] Implement contiguous one-step upcaster registration and path resolution.
- [ ] Implement source/target validation around every step.
- [ ] Implement typed reader errors and original/current view preservation.
- [ ] Run purity script and test suite.

### Task 4: Add binding/backfill persistence

**Files:**

- create migrations `0089` and binding/backfill repositories;
- create `tests/event_schema_backfill_restart.rs`, `event_schema_binding_atomicity.rs` and `event_schema_rls.rs`.

- [ ] Write failing database tests for append-only bindings, uniqueness, RLS, idempotent equal insert and conflicting insert.
- [ ] Write failing restart tests for bounded backfill leases/cursors.
- [ ] Create binding/backfill tables without payload columns.
- [ ] Implement repository ports and owner-specific legacy adapters.
- [ ] Prove no source-row update occurs during backfill.
- [ ] Run PostgreSQL tests and migration ownership script.

### Task 5: Integrate H1 Run events and replay

**Files:**

- modify H1 domain/application/infrastructure files listed above;
- create `tests/run_event_schema_persistence.rs` and `run_replay_schema_compatibility.rs`.

- [ ] Write failing atomicity and replay tests.
- [ ] Add exhaustive stable-kind/version mapping for Run events.
- [ ] Insert binding in the existing `RunStorePort` transaction.
- [ ] Gate replay on binding/schema/hash/upcast validation.
- [ ] Preserve exactly-one-event-per-logical-mutation and RunVersion semantics.
- [ ] Run H1 restart, replay and atomicity suites.

### Task 6: Integrate H7 interaction and public events

**Files:**

- modify H7 domain/application/repositories;
- modify public projection and cursor logic;
- create `tests/interaction_event_schema_persistence.rs` and `public_event_schema_cursor.rs`.

- [ ] Write failing interaction binding/idempotency tests.
- [ ] Add explicit source-owned descriptors.
- [ ] Add explicit internal-to-public mappings.
- [ ] Validate public schema before cursor allocation.
- [ ] Preserve at-least-once cursor semantics and opaque unknown SDK handling.
- [ ] Run H7 SSE/reconnect/parity tests.

### Task 7: Integrate H10 audit records

**Files:**

- modify H10 audit domain/query/repository files;
- create `tests/audit_event_schema_integrity.rs`.

- [ ] Write failing fixtures proving chain head/signatures remain unchanged.
- [ ] Bind exact audit body/envelope versions.
- [ ] Verify chain before upcast.
- [ ] Return original and interpreted views in audit queries/exports.
- [ ] Preserve incomplete/unsupported dispositions honestly.
- [ ] Run audit integrity/checkpoint/purge suites.

### Task 8: Implement readers-first rollout and producer activation

**Files:**

- create migrations `0090` and rollout/capability repositories;
- create application rollout service;
- create `tests/event_schema_rollout_readers_first.rs`.

- [ ] Write failing phase-transition and expired-reader tests.
- [ ] Implement operational reader capability heartbeat.
- [ ] Implement H2-authorized rollout phase changes.
- [ ] Implement per-schema producer activation/revocation.
- [ ] Reject V2 production while any required live reader lacks support.
- [ ] Verify rollout changes do not affect Run/public/audit sequences.

### Task 9: Add RLS, source validation and indexes

**Files:**

- create migration `0091`;
- extend `tests/event_schema_rls.rs` and binding atomicity tests.

- [ ] Write failing cross-workspace and unauthorized-admin tests.
- [ ] Add forced RLS and deployment-admin boundaries.
- [ ] Add deferred source-existence validation for atomic writes.
- [ ] Add bounded indexes and append-only triggers.
- [ ] Verify direct application SQL cannot create arbitrary bindings/activations.

### Task 10: Generate and publish H11 public schemas

**Files:**

- add generator/runtime files, schema assets and public routes;
- modify Rust/TypeScript SDK event files;
- create `tests/event_schema_release_baseline.rs`.

- [ ] Write failing deterministic-generation and baseline tests.
- [ ] Generate internal/public schema documents and indexes.
- [ ] Add read-only schema endpoints with ETag.
- [ ] Generate known unions and `UnknownPublicEvent` SDK variants.
- [ ] Include registry/baseline hashes in H11 release assets.
- [ ] Run H11 public API, SSE, SDK and release-manifest tests.

### Task 11: Add import/export and unknown-version conformance tests

**Files:**

- create `tests/event_schema_unknown_fail_closed.rs` and acceptance fixtures;
- integrate with signed Run export/import readers when that implementation exists.

- [ ] Verify unknown bundle schema can pass byte integrity but cannot semantic-import or replay.
- [ ] Verify original event files remain unchanged after interpreted upcast.
- [ ] Verify schema references/hashes are included in export manifests.
- [ ] Verify no event schema reference grants resource access.
- [ ] Verify unsupported versions never call model/tool/remote/human ports.

### Task 12: Add repository-wide boundary and acceptance gates

**Files:**

- create verification scripts and `tests/event_schema_acceptance.rs`;
- update `.github/workflows/ci.yml` only during implementation.

- [ ] Verify no runtime registration API exists.
- [ ] Verify no source-row rewrite SQL exists.
- [ ] Verify no duplicate `(kind, version)` and no changed baseline hash.
- [ ] Verify every current V2+ descriptor has exactly one pure predecessor upcaster.
- [ ] Verify every owning enum variant has an explicit descriptor.
- [ ] Verify unknown versions fail replay/resume/import/mutation.
- [ ] Verify H1/H7/H10 authority and ordering remain unchanged.
- [ ] Run formatting, clippy, unit, PostgreSQL, schema-generation, SDK and acceptance suites.

---

## Mandatory acceptance matrix

| Scenario | Required result |
|---|---|
| Existing H1 V1 Run journal read by new binary | validates original bytes, optionally upcasts, replays deterministically |
| V1 source row after backfill | source row unchanged; one append-only sidecar binding exists |
| New H1 mutation | source event and binding commit atomically |
| Unknown H1 version | replay stops before applying unknown event |
| Invalid payload hash | integrity failure, no best-effort decode |
| Missing binding after enforcement | integrity failure |
| V1→V2 upcast | event identity/order/timestamps preserved |
| Upcaster output invalid | read fails; partial output discarded |
| H7 public projection | exact explicit public schema validates before cursor allocation |
| Old SDK sees new public version | bounded `UnknownPublicEvent`, no typed behavior |
| H10 audit query with upcast | original chain/hash verified first and unchanged |
| Generated schema same key changed bytes | CI fails baseline check |
| Runtime extension requests canonical kind | rejected; no registry mutation path |
| New producer before readers | activation denied |
| Reader heartbeat expires | new-version producer gate fails closed |
| Run export contains unknown future schema | integrity may pass; semantic import/replay blocked |
| Backfill restart | resumes cursor idempotently without source updates |
| Cross-workspace binding access | denied by forced RLS |
| Binding conflict | hard integrity failure |
| Schema registry construction invalid | service readiness fails |

---

## Completion gate

The implementation may be considered complete only when all of the following are proven:

1. registry descriptors are repository-owned and immutable at runtime;
2. every in-scope H1/H7/H10 durable serialized event has an exact schema reference;
3. source rows are never rewritten by compatibility/backfill code;
4. new source events and bindings commit atomically;
5. canonical bytes and payload hashes are deterministic;
6. upcasters are pure, contiguous, validated and identity-preserving;
7. unknown versions fail closed for semantic operations;
8. H1 Run ordering, H7 cursor ordering and H10 audit-chain authority remain unchanged;
9. readers-first rollout blocks unsupported producers;
10. H11 generated schemas, SDK unions and release baselines are deterministic;
11. migrations are limited to `0089`–`0091`;
12. RLS, restart, import/export and corruption tests pass;
13. no new event store, H12 authority, direct agent SQL or runtime schema registration exists;
14. no implementation work starts merely because this plan is merged.

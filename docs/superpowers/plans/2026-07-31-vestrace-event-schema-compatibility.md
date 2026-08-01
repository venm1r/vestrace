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

**Architecture:** Each owning Horizon continues to persist and order its own authoritative records. A shared library defines `StableEventKind`, positive per-kind `EventSchemaVersion`, immutable schema descriptors, canonical document hashing and pure upcaster contracts. A content-free sidecar binding records which exact descriptor and original payload hash apply to each durable source record; it stores no event payload and owns no lifecycle. Existing historical source rows are never rewritten. H11 publishes public descriptors and compatibility metadata, while internal replay/import paths refuse unknown versions, corrupted bindings or incomplete upcast chains.

**Tech stack:** Existing Vestrace v0.1 plus H1, H7, H10 and H11; Rust Edition 2024; Tokio; Serde/Schemars; SQLx; PostgreSQL 17; repository canonical JSON; SHA-256; JSON Schema 2020-12; H1 logical replay; H7 durable public cursors; H10 audit/evaluation integrity; H11 release/schema generation; proptest; deterministic fixtures and fake clocks.

---

## Global constraints

- H1 remains the only authority for `AgentRun`, `RunEvent`, Run sequence/version and logical replay.
- H7 remains the only authority for interaction events, public-event records and workspace public cursors.
- H10 remains the only authority for audit chains, evaluation/verification records and safe replay.
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
- H10 integrity verification checks original stored bytes and original chain/hash evidence before any presentation upcast.
- Public transports may preserve an already-authorized unknown public envelope as an opaque SDK value, but cannot execute typed behavior, replay it or map it to a known semantic variant.
- Models, packages, extensions, webhooks and administrators cannot register Vestrace-owned stable kinds at runtime.
- Registry descriptors and generated schema assets change only through reviewed repository changes owned by the source Horizon.
- Registry metadata is compiled/generated repository content, not a mutable PostgreSQL schema registry.
- A schema hash for an existing `(surface, stable_event_kind, schema_version)` never changes.
- Documentation-only edits cannot change descriptor bytes, schema bytes, test-vector hashes or the compiled registry hash.
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
  event_schema/{mod,kind,version,reference,descriptor,compatibility,binding,rollout,error}.rs

crates/vestrace-application/src/
  event_schema/{mod,ports,registry,writer,reader,upcast,backfill,rollout}.rs
  event_schema/owners/{mod,h1,h7,h10}.rs
  run/replay.rs
  channel/projection.rs
  audit/query.rs
  evaluation/query.rs
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
  evaluation/run_repository.rs

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
  evaluation_event_schema_integrity.rs
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

The initial implementation registers every durable serialized record that exists in the implemented H1, H7 and H10 owners at implementation time. A source owner cannot ship a new durable event variant without adding its descriptor, schema fixture and mapping in the same change.

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

### Version change class

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventSchemaVersionClass {
    Initial,
    WireChange,
    SemanticSuccessor,
}
```

Rules:

- version 1 is `Initial`;
- changed schema bytes, validation, canonicalization, bounds or serialized enum set are `WireChange` with a new version;
- changed business meaning uses a new stable kind whose first descriptor may identify the old kind as a `SemanticSuccessor` for documentation only;
- a semantic successor does not create an upcast path between different kinds;
- documentation-only edits create no descriptor version and do not alter registry assets.

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
    pub version_class: EventSchemaVersionClass,
    pub previous_version: Option<EventSchemaVersion>,
    pub semantic_predecessor_kind: Option<StableEventKind>,
    pub upcaster_key_from_previous: Option<&'static str>,
    pub test_vector_hash: [u8; 32],
}
```

Descriptor invariants:

- `(kind, version)` is globally unique in the compiled registry;
- versions for one kind are contiguous from 1;
- schema hash matches exact generated bytes;
- V1 has no previous version or upcaster;
- V2+ has exactly one previous version and one registered upcaster;
- `SemanticSuccessor` is V1 of a new kind and has no cross-kind upcaster;
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
h1_event_schema_descriptors()
h7_event_schema_descriptors()
h10_event_schema_descriptors()
```

No dynamic plugin discovery, model submission or SQL-loaded descriptor is allowed.

### Owner inventory

Each owner adapter contains exhaustive mappings from persisted record types to stable kinds and versions.

Requirements:

- every persisted H1 `RunEvent` variant maps through an exhaustive match;
- every persisted H7 interaction/public event representation has an explicit descriptor;
- H10 audit, evaluation, verification and replay records that are serialized durably have explicit descriptors;
- no wildcard match maps future enum variants to a generic kind;
- descriptor tests fail when an owner enum gains a variant without a mapping;
- internal and public mappings are explicit and may differ only through a reviewed mapping table.

### Generated assets

`generate-event-schemas.sh` writes deterministic assets:

```text
schemas/events/internal/h1/<kind>.v<version>.json
schemas/events/internal/h7/<kind>.v<version>.json
schemas/events/internal/h10/<kind>.v<version>.json
schemas/events/v1/<public-kind>.v<version>.json
schemas/events/v1/index.json
schemas/events/v1/compatibility.json
schemas/compatibility/event-schema-baseline.json
```

Index entries contain stable kind, schema version/hash, owner, exposure, previous version, upcaster key, canonicalization revision and test-vector hash. Generation sorts entries and rejects duplicate paths or keys.

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
- event ID, workspace, authority, sequence/cursor, correlation, causation and recorded timestamp are preserved;
- original payload hash is not overwritten or presented as a hash of the current view;
- generated fields derive only from source fields and descriptor constants;
- no current clock, random UUID, environment variable, locale, network, database, policy, secret or mutable registry access;
- one input produces exactly one output or an error;
- split/merge semantics require new stable kinds and are not represented as an upcaster;
- every output validates against the exact target schema before the next step;
- unsupported source versions fail before typed interpretation.

`verify-event-upcaster-purity.sh` rejects SQLx, Reqwest, Tokio time, random generators, environment access, H2/H8 ports and filesystem APIs from upcaster modules.

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
3. validate against the exact source schema;
4. compute canonical payload hash;
5. return binding data for the owner transaction;
6. refuse unregistered kind/version or descriptor hash mismatch.

The owning repository inserts source row and schema binding in one transaction. The shared writer never writes source event rows itself.

### Owner atomicity

H1 transaction:

```text
updated AgentRun
+ exactly one RunEvent
+ optional step/checkpoint changes
+ follow-up work
+ one schema binding for the RunEvent
= one transaction
```

H7 interaction/public-event rows and bindings commit in their existing append/cursor transactions. H10 audit/evaluation source records and bindings commit in their owning transactions. Binding failure rolls back the source mutation.

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
```

Reader algorithm:

```text
1. authorize and load source through the owning Horizon
2. load immutable schema binding
3. resolve descriptor and verify schema hash
4. reconstruct canonical original document
5. validate original document against source schema
6. recompute and compare original payload hash
7. resolve one unique contiguous upcast path
8. apply and validate every pure step
9. deserialize only the final validated document
10. return original and interpreted views
```

Typed failures:

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

No error falls back to the current Rust enum or field guessing.

---

## Legacy binding and backfill

Historical source rows remain unchanged. Backfill creates only sidecar bindings and operational progress records.

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

Status values: `Created`, `Running`, `Paused`, `Completed`, `Failed`, `Cancelled`.

Backfill behavior:

- reads stable UUID-ordered bounded batches;
- reconstructs exact frozen V1 documents using owner adapters;
- validates and hashes the source representation;
- inserts binding idempotently;
- equal existing binding is success;
- conflicting binding is a hard integrity failure;
- restart resumes after durable cursor;
- source row update/delete is never issued;
- failure stores bounded code/reference, never source payload;
- no Run/public/audit sequence advances;
- no replacement canonical source event is emitted.

---

## Rollout and readers-first activation

### Rollout state

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
    pub state_revision: u64,
    pub changed_by: PrincipalId,
    pub policy_decision_id: PolicyDecisionId,
    pub updated_at: Timestamp,
}

pub struct AdvanceEventSchemaRollout {
    pub deployment_id: DeploymentId,
    pub target_phase: EventSchemaRolloutPhase,
    pub required_registry_hash: [u8; 32],
    pub expected_state_revision: u64,
    pub idempotency_key: String,
}
```

`expected_state_revision` belongs only to the command.

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

These are operational capability advertisements, not authorization capabilities.

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

Activation requires:

- descriptor in installed release;
- all non-expired required readers support the schema;
- no conflicting live registry hash;
- required backfills completed without unresolved conflicts;
- H2-authorized rollout change;
- no change to historical rows.

Revocation stops new writes of that version but does not invalidate existing events.

---

## Persistence model

### Migration `0089_durable_event_schema_bindings_and_backfill.sql`

Create:

```text
durable_event_schema_bindings
event_schema_backfill_jobs
event_schema_backfill_failures
```

Binding columns:

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
- application role cannot update/delete bindings;
- failures contain bounded code/reference only;
- backfill state uses optimistic concurrency and leases;
- no payload/body/blob column exists.

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
- one current reader advertisement per `(deployment_id, node_id)`;
- one active producer activation per exact deployment/schema/component;
- rollout history append-only;
- activation/revocation use optimistic revision;
- hashes exactly 32 bytes;
- heartbeat writes are operational and emit no canonical event.

### Migration `0091_event_schema_rls_constraints_and_indexes.sql`

Add:

- forced RLS for workspace-owned bindings/failures;
- deployment-admin policies for rollout tables;
- indexes by workspace/source, kind/version, backfill cursor/status, reader expiry and activation;
- deferred source-existence validation by authority family for atomic writes;
- a scoped privileged backfill validation path that never disables RLS;
- append-only triggers for bindings and rollout history;
- denial of arbitrary direct application inserts outside owner repositories;
- migration ownership comments.

No migration updates historical H1/H7/H10 source rows.

---

## Owner integration

### H1

- owner adapter exhaustively maps every `RunEvent` variant;
- binding insert participates in existing `RunStorePort` transaction;
- logical mutation still increments `RunVersion` exactly once and appends exactly one event;
- replay validates binding/schema/hash before applying semantics;
- unsupported version yields `SchemaUnsupported` before partial aggregate mutation;
- upcasting emits no Run event and changes no Run version.

Required H1 tests:

- Run creation source/binding atomicity;
- binding failure rolls back mutation;
- V1 fixture replays through V2 reader without byte rewrite;
- unknown V99 blocks before application;
- corruption is detected;
- sequence, Run ID and timestamp are preserved through upcast;
- restart yields identical state and upcaster list.

### H7

- every append-only interaction receives an explicit binding;
- channel/external metadata cannot choose kind/version;
- internal-to-public mapping is explicit;
- public payload validates before cursor allocation;
- public source row, mapping reference, binding and cursor commit atomically;
- projection failure is durable and cannot fabricate another semantic event;
- old SDKs expose bounded `UnknownPublicEvent` without typed behavior.

Required H7 tests:

- idempotent duplicate interaction returns original binding;
- reconnect remains at-least-once and cursor-stable;
- same cursor cannot represent changed schema/payload;
- unknown internal schema blocks typed public projection;
- internal-only descriptor cannot be published;
- HTTP/SSE/CLI/SDK agree on kind/version.

### H10

- audit chain and evaluation evidence verify original bytes before upcast;
- sidecar payload hash is additional evidence, not a replacement for record/chain hash;
- upcasted view never changes chain head, checkpoint signature input or stored evaluation evidence;
- unknown schema produces incomplete/unsupported disposition, never optimistic validity;
- query/export exposes original schema/hash plus applied upcasters;
- purge keeps only content-free binding/tombstone according to H10 policy.

Required H10 tests:

- old audit chain head remains identical;
- upcast does not alter record hash;
- payload hash and chain hash are separately checked;
- missing binding produces incomplete evidence;
- evaluation/replay records fail closed on unsupported schema;
- exported diagnostics never claim rewritten history.

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
    pub payload_hash_hex: String,
}
```

The DTO contains no internal schema path, database type, secret, backend reference or authority shortcut.

### Schema routes

```text
GET /v1/schemas/events
GET /v1/schemas/events/{stable_kind}/versions/{schema_version}
GET /v1/schemas/events/compatibility
```

Routes serve installed immutable assets with exact ETags, never expose internal/audit-only schemas and never register schemas at runtime.

### SDKs

Rust and TypeScript SDKs expose:

```text
KnownPublicEvent<T>
UnknownPublicEvent
PublicEventEnvelope
EventSchemaRef
```

Known decode requires exact generated support and validation. Unknown decode preserves only the already-authorized bounded public envelope and never casts it to the latest known type.

### Baseline gate

CI fails when:

- an existing key changes schema hash;
- a schema disappears without public-major removal policy;
- a version is skipped;
- V2+ lacks one predecessor/upcaster;
- owner/exposure changes unexpectedly;
- generated bytes differ from committed assets;
- SDK unions omit a public descriptor;
- release manifest omits registry/baseline hashes.

---

## Portable export/import interaction

Portable event entries preserve:

```text
stable kind
original schema version
schema hash
original payload hash
original serialized document or exact reference
```

Rules:

- integrity may succeed for an unknown future schema;
- semantic import, replay, resume and activation remain blocked;
- unknown payload is never rewritten to the current version;
- upcast occurs only in an interpreted view after original verification;
- applied-upcaster metadata is separate from original event files;
- schema support grants no resource capability.

The signed Run export implementation consumes `DurableEventReaderPort` and `EventSchemaRegistryPort`; this plan provides fixture-backed conformance tests without implementing Run export itself.

---

## Failure semantics

### Missing binding

After `BindingsRequired`, a missing binding is an integrity error. During explicit transition phases only, frozen owner-specific V1 inference may support diagnostics/backfill for rows predating the dual-write boundary. It cannot enable V2 production or mutate source rows.

### Unknown kind/version

- no typed interpretation;
- no replay/resume/import mutation;
- no automatic downcast;
- no model-assisted guess;
- no substitute event emitted to hide failure;
- operator receives safe schema reference and supported range.

### Hash mismatch

Descriptor or payload hash mismatch is an integrity failure, never a compatibility warning.

### Upcaster failure

- source remains immutable;
- reader returns failing step key;
- partial output is discarded;
- no aggregate mutation occurs;
- content-free diagnostic/audit intent is recorded where required.

### Registry construction failure

The service fails readiness rather than running with a partial registry.

### Rollout repository failure

A new-version producer fails closed. Existing V1 writing continues only under the exact safe rollout phase configured for that deployment.

---

## Implementation tasks

### Task 1: Add value types and registry invariants

**Files:** domain event-schema modules and `event_schema_registry.rs` tests.

- [ ] Write failing tests for kind validation, positive versions, uniqueness, contiguous chains, semantic successors, owner/exposure rules and deterministic registry hash.
- [ ] Implement IDs and value types.
- [ ] Implement descriptor validation and explicit registry assembly.
- [ ] Prove there is no runtime registration method.
- [ ] Run domain/application tests and clippy.

### Task 2: Implement canonical serialization and hashing

**Files:** runtime/test-support crates and writer tests.

- [ ] Write failing vectors for nested maps, arrays, Unicode, integers, timestamps and non-finite rejection.
- [ ] Implement canonical bytes and SHA-256.
- [ ] Implement exact schema validation.
- [ ] Implement writer preparation.
- [ ] Verify identical bytes/hash across restart.

### Task 3: Implement pure upcasters and reader

**Files:** application reader/upcast modules, runtime upcaster and purity script.

- [ ] Write failing missing/ambiguous/cyclic path tests.
- [ ] Write identity-preservation and no-I/O tests.
- [ ] Implement contiguous one-step registration/path resolution.
- [ ] Validate source and each target step.
- [ ] Preserve original/current views and typed failures.
- [ ] Run purity and conformance suites.

### Task 4: Add binding/backfill persistence

**Files:** migration `0089`, repositories and backfill tests.

- [ ] Write failing append-only, uniqueness, RLS, equal-idempotency and conflict tests.
- [ ] Write bounded lease/cursor restart tests.
- [ ] Create tables without payload columns.
- [ ] Implement binding/backfill ports and owner adapters.
- [ ] Prove source-row updates are absent.

### Task 5: Integrate H1

- [ ] Write failing source/binding atomicity and replay tests.
- [ ] Add exhaustive owner mapping.
- [ ] Insert binding in `RunStorePort` transaction.
- [ ] Gate replay on binding/schema/hash/upcast.
- [ ] Preserve exactly-one-event and RunVersion semantics.
- [ ] Run H1 restart/replay/atomicity suites.

### Task 6: Integrate H7

- [ ] Write failing interaction binding/idempotency tests.
- [ ] Add explicit interaction/public descriptors and mappings.
- [ ] Validate public schema before cursor allocation.
- [ ] Preserve at-least-once cursor semantics.
- [ ] Add opaque unknown SDK behavior tests.
- [ ] Run H7 SSE/reconnect/parity suites.

### Task 7: Integrate H10

- [ ] Write failing audit/evaluation integrity fixtures.
- [ ] Register exact audit/evaluation/verification/replay record schemas.
- [ ] Verify original evidence before upcast.
- [ ] Return original/interpreted query views.
- [ ] Preserve honest incomplete/unsupported disposition.
- [ ] Run H10 integrity/checkpoint/replay suites.

### Task 8: Implement readers-first rollout

**Files:** migration `0090`, rollout service and tests.

- [ ] Write failing phase, expired-reader and conflicting-registry tests.
- [ ] Implement reader advertisements.
- [ ] Implement H2-authorized phase commands with expected revision in the command.
- [ ] Implement per-schema producer activation/revocation.
- [ ] Reject unsupported production.
- [ ] Verify operational writes do not change domain sequences.

### Task 9: Add RLS, validation and indexes

**Files:** migration `0091` and database tests.

- [ ] Write failing cross-workspace/admin-boundary tests.
- [ ] Add forced RLS and deployment-admin policies.
- [ ] Add deferred source-existence validation.
- [ ] Add indexes and append-only triggers.
- [ ] Deny arbitrary direct application inserts.

### Task 10: Generate/publish H11 schemas

- [ ] Write failing deterministic generation and baseline tests.
- [ ] Generate internal/public assets and indexes.
- [ ] Add schema routes with ETag.
- [ ] Generate known unions and unknown variants.
- [ ] Include registry/baseline hashes in release assets.
- [ ] Run H11 API/SSE/SDK/release tests.

### Task 11: Add portable-boundary and unknown-version conformance

- [ ] Verify unknown portable schema may pass integrity but cannot semantic-import/replay.
- [ ] Verify original files remain unchanged after upcast.
- [ ] Verify manifest schema/hash inclusion.
- [ ] Verify schema references grant no access.
- [ ] Verify unsupported records never call model/tool/remote/human ports.

### Task 12: Add repository-wide acceptance gates

- [ ] Verify no runtime registration API.
- [ ] Verify no source-row rewrite SQL.
- [ ] Verify no duplicate key or changed baseline hash.
- [ ] Verify every V2+ descriptor has one pure predecessor upcaster.
- [ ] Verify every durable owner variant has an explicit descriptor.
- [ ] Verify unknown versions fail semantic operations.
- [ ] Verify H1/H7/H10 ownership/order remain unchanged.
- [ ] Run formatting, clippy, unit, PostgreSQL, generator, SDK and acceptance suites.

---

## Mandatory acceptance matrix

| Scenario | Required result |
|---|---|
| Existing H1 V1 journal read by new binary | original validates; view may upcast; replay deterministic |
| V1 source row after backfill | source unchanged; one append-only binding exists |
| New H1 mutation | source event and binding atomic |
| Unknown H1 V99 | replay stops before application |
| Invalid payload hash | integrity failure |
| Missing binding after enforcement | integrity failure |
| V1→V2 upcast | identity/order/timestamps preserved |
| Invalid upcast output | read fails; partial output discarded |
| H7 public projection | exact public schema validates before cursor |
| Old SDK sees new public version | bounded unknown value; no typed behavior |
| H10 query with upcast | original integrity verified first and unchanged |
| Same schema key with changed bytes | CI fails |
| Runtime extension requests canonical kind | rejected |
| New producer before readers | activation denied |
| Reader advertisement expires | new-version gate fails closed |
| Portable bundle has unknown future schema | integrity may pass; semantics blocked |
| Backfill restart | cursor resumes idempotently, no source update |
| Cross-workspace binding access | denied by RLS |
| Binding conflict | hard integrity failure |
| Invalid compiled registry | readiness fails |

---

## Completion gate

Implementation is complete only when:

1. descriptors are repository-owned and runtime-immutable;
2. every implemented H1/H7/H10 durable serialized record has an exact schema reference;
3. compatibility/backfill code never rewrites source rows;
4. new source records and bindings commit atomically;
5. canonical bytes/hashes are deterministic;
6. upcasters are pure, contiguous, validated and identity-preserving;
7. unknown versions fail closed for semantic operations;
8. H1 Run ordering, H7 cursor ordering and H10 integrity authority remain unchanged;
9. readers-first rollout blocks unsupported producers;
10. H11 schemas, SDK unions and baselines are deterministic;
11. migrations are limited to `0089`–`0091`;
12. RLS, restart, portable-boundary and corruption tests pass;
13. no second event store, H12 authority, direct agent SQL or runtime schema registration exists;
14. merging this plan authorizes no implementation work.

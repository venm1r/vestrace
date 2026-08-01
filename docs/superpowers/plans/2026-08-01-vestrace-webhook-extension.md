# Vestrace Webhook Extension Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:test-driven-development` for every implementation task and `superpowers:verification-before-completion` before claiming a task or branch complete. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Creating or merging this file does not authorize a feature branch, Rust changes, SQL migrations, endpoint activation, secret issuance, network listeners, external requests, webhook delivery or trigger execution. Implementation begins only after an explicit future instruction ending the documentation-only phase.

**Approval evidence:**

- approved design: `docs/superpowers/specs/2026-08-01-vestrace-webhook-extension-design.md`;
- design merge commit: `8dfb8c11e8ff2ec9fba5ef95aa158d49d1b27f50`;
- State Engine boundary: `docs/superpowers/specs/2026-07-31-vestrace-state-engine-boundary-amendment.md`;
- durable event compatibility ADR: `docs/superpowers/specs/2026-07-31-vestrace-event-schema-compatibility-adr.md`;
- H7 plan: `docs/superpowers/plans/2026-07-31-vestrace-h7-conversations-channels-triggers.md`;
- H8 plan: `docs/superpowers/plans/2026-07-31-vestrace-h8-connections-credentials-secret-backends.md`;
- H10 plan: `docs/superpowers/plans/2026-07-31-vestrace-h10-observability-evaluation.md`;
- H11 plan: `docs/superpowers/plans/2026-07-31-vestrace-h11-universal-vertical-slice-product-surface.md`;
- normalization report: `docs/superpowers/specs/2026-07-31-vestrace-state-engine-normalization-report.md`;
- preceding implementation-plan merge: `7762f005875e17044e041c19e4367d415cde7b46`.

**Goal:** Implement disabled-by-default inbound and outbound webhooks as secure adapters over existing H7 source events/triggers and H11 product surfaces. Inbound requests become verified, bounded and still-untrusted durable source facts before H7 evaluation. Outbound notifications originate from already-committed source events, create durable delivery intents, use H8-backed signing, preserve immutable payloads across retries and fail safely on ambiguous completion.

**Architecture:** This extension completes and hardens the H11 webhook slice; it does not create a second webhook runtime. H2 authorizes endpoint management, inbound consequence evaluation and outbound delivery. H7 owns inbound source facts, trigger occurrences and outbound source-event selection. H8 owns secret references, MAC/signature operations and request-scoped cryptographic leases. H6 owns quarantined large inbound bodies and immutable outbound payload bytes. H10 owns content-free security audit, bounded diagnostics and delivery metrics. H11 owns management, inspection and transport adapters. H1 Run state is reachable only through existing H7/H2 application paths after a source fact is durably committed.

**Tech stack:** Existing Vestrace v0.1 plus H2, H6, H7, H8, H10 and H11; Rust Edition 2024; Tokio; Axum/Tower; Serde/Schemars; SQLx; PostgreSQL 17; SHA-256; HMAC-SHA-256 and Ed25519 through H8; rustls-compatible TLS; bounded JSON Schema validation; H6 Artifact stores; deterministic DNS/HTTP/proxy/clock fixtures; proptest; local receiver and reconciliation fixtures.

---

## Global constraints

- Webhook capability is disabled by default at deployment and workspace scope.
- An endpoint definition, endpoint revision or route handle does not activate an endpoint.
- H1 remains the sole owner of `AgentRun`, `RunStep`, `RunEvent`, Run version and checkpoint state.
- An inbound webhook never invokes an H1 mutation directly.
- An inbound request may create only a durable receipt and, after successful verification, a bounded `WebhookSourceFact` owned by H7.
- Trigger evaluation begins only after the source fact transaction commits.
- A source fact remains external/untrusted data even when its transport signature is valid.
- Fields such as `approved=true`, `run_status=completed` or `role=admin` never create approvals, completion, roles or capabilities.
- H7 trigger autonomy ceilings and H2 policy determine every optional consequence.
- Outbound delivery begins only from an already-committed H7 source/public event and an active immutable subscription revision.
- Webhook delivery success/failure never rewrites the source event or owning aggregate.
- Existing H7 public-event stream remains authoritative; webhooks are at-least-once notification adapters.
- A model, package, extension or incoming payload cannot choose an arbitrary destination URL, secret reference, event kind, template program or retry policy.
- Endpoint revisions, subscription revisions, filters, templates, auth profiles and network policies are immutable.
- Endpoint and subscription lifecycle identities are mutable aggregates with optimistic concurrency.
- Mutable lifecycle changes use expected revision in commands; expected revisions are not persisted as aggregate fields.
- Endpoint handles, endpoint IDs, event IDs, hashes, URLs, signature key IDs and delivery receipts are references, not capabilities.
- Inbound route handles have high entropy, contain no workspace identifier and never replace authentication/signature verification.
- Route-handle bytes are returned only at creation/rotation time; persistence stores only a keyed hash and bounded fingerprint.
- Unknown and revoked handles return indistinguishable bounded responses.
- Raw Authorization headers, cookies, proxy credentials, private keys, MAC secrets and bearer tokens never enter domain/application DTOs, PostgreSQL webhook payloads, logs or telemetry.
- Whole request/response header sets are never persisted.
- Hidden reasoning, model scratchpads and unrestricted external text never enter webhook audit records.
- Inbound signature verification uses exact raw bytes or one explicitly versioned provider canonicalization profile; canonicalization is never guessed.
- Algorithm downgrade and silent unsigned fallback are prohibited.
- Inbound timestamp/replay windows use a trusted clock and bounded skew.
- Duplicate inbound requests do not create a second source fact or trigger occurrence.
- Reuse of the same external event ID/nonce with different body bytes is a security conflict, not a duplicate success.
- Large or content-bearing inbound payloads enter H6 quarantine; ordinary receipt rows contain hashes, bounded normalized fields and references only.
- Outbound payloads are rendered once from an exact source event and immutable template revision.
- Every retry uses the same exact payload bytes, payload hash, delivery ID and source-event reference.
- Per-attempt timestamp and signature may change; semantic body bytes do not.
- Outbound payload bytes are stored as an H6 Artifact revision with exact classification, retention and provenance.
- Outbound authentication/signing uses H8 operation-bound ports. Secret bytes never leave H8.
- Redirects, cookies, ambient proxy variables and automatic authentication forwarding are disabled.
- Destination validation defends against SSRF, DNS rebinding, mixed public/private answers and post-validation address changes.
- Private, loopback, link-local, multicast, metadata-service and reserved destinations are denied unless an exact deployment policy explicitly permits a specific local target.
- TLS certificate and hostname verification remain enabled; disabling verification is not a supported policy option.
- Delivery is at-least-once. Receivers are expected to deduplicate by immutable delivery ID.
- A timeout or connection loss after request bytes may have reached the receiver produces `OutcomeUnknown`.
- `OutcomeUnknown` is never blindly retried unless exact receiver idempotency or reconciliation support is configured and current policy permits it.
- Current endpoint lifecycle, active revision, subscription lifecycle, H2 policy, destination classification and H8 availability are rechecked before each external attempt.
- Disabling or revoking an endpoint/subscription prevents new attempts. Existing successful evidence is retained according to policy.
- H10 audit/metrics are content-free and bounded-cardinality.
- No second event store, trigger scheduler, policy engine, secret store or delivery authority is introduced.
- H11 migration `0083_webhook_subscriptions_deliveries_and_attempts.sql` is a baseline and is never edited.
- This extension reuses semantically identical H11 webhook IDs/tables and adds forward-only fields/tables instead of duplicating them.
- Existing applied migrations are never edited. This concern reserves:

```text
0096_webhook_endpoints_revisions_and_capabilities.sql
0097_inbound_webhook_receipts_source_facts_and_dedup.sql
0098_outbound_webhook_subscriptions_deliveries_reconciliation.sql
0099_webhook_rls_network_snapshots_indexes_and_worker_state.sql
```

- No other concern may reuse migration numbers `0096`–`0099`.
- CI requires no public DNS, internet, cloud secret manager, public webhook service or permanent credential.
- Future implementation branch: `feat/webhook-extension`.

---

## Compatibility with the H11 baseline

H11 already reserves a product-surface webhook slice with:

```text
WebhookSubscriptionId
WebhookDeliveryId
WebhookDeliveryAttemptId
0083_webhook_subscriptions_deliveries_and_attempts.sql
```

This plan treats those contracts as the baseline outbound identity set.

Implementation rules:

- reuse existing IDs when their semantics match;
- extend baseline subscription/delivery rows through migrations `0096`–`0099`;
- do not create `WebhookSubscriptionV2` or a parallel delivery table merely to avoid a forward migration;
- introduce new IDs only for endpoint identity/revision, route handles, immutable subscription/filter/template revisions, inbound receipt/source fact, reconciliation and network-resolution evidence;
- adapt H11 routes/SDKs to the richer application contracts without leaving the baseline worker active in parallel;
- exactly one composition root registers webhook workers;
- exactly one durable worker lease namespace owns outbound attempts;
- baseline H11 tests remain valid or are explicitly upgraded in the same implementation branch.

---

## Locked file structure

```text
Cargo.toml
Cargo.lock
.github/workflows/ci.yml

crates/vestrace-domain/src/
  id.rs
  webhook/
    mod.rs
    capability.rs
    endpoint.rs
    route_handle.rs
    revision.rs
    authentication.rs
    network.rs
    payload.rs
    inbound.rs
    source_fact.rs
    subscription.rs
    delivery.rs
    attempt.rs
    reconciliation.rs
    error.rs

crates/vestrace-application/src/
  webhook/
    mod.rs
    ports.rs
    commands.rs
    endpoint_service.rs
    route_handle_service.rs
    inbound_service.rs
    inbound_verifier.rs
    source_fact_service.rs
    subscription_service.rs
    source_projector.rs
    payload_renderer.rs
    delivery_service.rs
    delivery_worker.rs
    retry.rs
    reconciliation.rs
    audit.rs
  composition/webhook.rs

crates/vestrace-application/tests/
  webhook_endpoint_lifecycle.rs
  webhook_route_handle.rs
  inbound_webhook_verification.rs
  inbound_webhook_dedup.rs
  inbound_webhook_h7_integration.rs
  outbound_webhook_subscription.rs
  outbound_webhook_projection.rs
  outbound_webhook_payload.rs
  outbound_webhook_retry.rs
  outbound_webhook_unknown.rs
  webhook_network_policy.rs
  webhook_failure_semantics.rs

crates/vestrace-webhook-runtime/
  Cargo.toml
  src/
    lib.rs
    raw_request.rs
    header_allowlist.rs
    body_limit.rs
    signature.rs
    replay.rs
    json_bounds.rs
    resolver.rs
    egress.rs
    tls.rs
    receiver.rs
    error.rs

crates/vestrace-webhook-test-support/
  Cargo.toml
  src/
    lib.rs
    clock.rs
    signer.rs
    resolver.rs
    proxy.rs
    receiver.rs
    reconciliation.rs
    fixtures.rs
    faults.rs
    conformance.rs

crates/vestrace-infrastructure/src/postgres/
  webhook/
    mod.rs
    endpoint_repository.rs
    route_handle_repository.rs
    revision_repository.rs
    inbound_repository.rs
    source_fact_repository.rs
    subscription_repository.rs
    delivery_repository.rs
    attempt_repository.rs
    reconciliation_repository.rs
    network_snapshot_repository.rs
    worker_repository.rs

crates/vestrace-channel-http/src/
  routes/webhooks.rs
  routes/inbound_webhook.rs
  dto/webhook.rs
  middleware/webhook_body_limit.rs

crates/vestrace-channel-cli/src/
  commands/webhook.rs

crates/vestrace-public-schema/src/
  webhook.rs

crates/vestrace-sdk-rust/src/
  webhooks.rs

packages/sdk-typescript/src/
  webhooks.ts
  generated/webhooks.ts

schemas/
  webhooks/v1/endpoint.schema.json
  webhooks/v1/inbound-receipt.schema.json
  webhooks/v1/source-fact.schema.json
  webhooks/v1/subscription.schema.json
  webhooks/v1/delivery.schema.json
  webhooks/v1/attempt.schema.json
  webhooks/v1/reconciliation.schema.json
  compatibility/webhook-schema-baseline.json

migrations/
  0096_webhook_endpoints_revisions_and_capabilities.sql
  0097_inbound_webhook_receipts_source_facts_and_dedup.sql
  0098_outbound_webhook_subscriptions_deliveries_reconciliation.sql
  0099_webhook_rls_network_snapshots_indexes_and_worker_state.sql

tests/
  webhook_endpoint_persistence.rs
  webhook_endpoint_revision_immutability.rs
  webhook_route_handle_security.rs
  inbound_webhook_atomicity.rs
  inbound_webhook_replay.rs
  inbound_webhook_quarantine.rs
  inbound_webhook_restart.rs
  outbound_webhook_subscription_lifecycle.rs
  outbound_webhook_intent_dedup.rs
  outbound_webhook_payload_immutability.rs
  outbound_webhook_signature.rs
  outbound_webhook_ssrf.rs
  outbound_webhook_dns_rebinding.rs
  outbound_webhook_retry_restart.rs
  outbound_webhook_reconciliation.rs
  webhook_secret_boundary.rs
  webhook_rls.rs
  webhook_h11_parity.rs
  webhook_acceptance.rs

scripts/
  verify-webhook-boundary.sh
  verify-webhook-no-secrets.sh
  verify-webhook-network-policy.sh
  verify-webhook-outcome-unknown.sh
  verify-webhook-migration-ownership.sh
```

---

## Normative domain contracts

### Identifiers

Reuse existing H11 identifiers where present and add:

```text
WebhookEndpointId
WebhookEndpointRevisionId
WebhookRouteHandleId
InboundWebhookReceiptId
WebhookSourceFactId
WebhookSubscriptionRevisionId
WebhookEventFilterRevisionId
WebhookPayloadTemplateRevisionId
WebhookPayloadSchemaRevisionId
WebhookProviderProfileRevisionId
WebhookReconciliationProfileId
WebhookReconciliationId
WebhookNetworkResolutionSnapshotId
WebhookWorkerLeaseId
```

All IDs use the repository UUID strategy and reject nil values.

### Capability posture

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WebhookCapabilityState {
    Disabled,
    InboundOnly,
    OutboundOnly,
    InboundAndOutbound,
}

pub struct WebhookCapabilitySnapshot {
    pub deployment_state: WebhookCapabilityState,
    pub workspace_state: WebhookCapabilityState,
    pub policy_bundle_revision_id: PolicyBundleRevisionId,
}
```

Effective capability is the intersection of deployment, workspace and current H2 policy.

### Endpoint identity

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WebhookDirection {
    Inbound,
    Outbound,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WebhookEndpointState {
    Draft,
    Disabled,
    Active,
    Suspended,
    Degraded,
    Revoked,
    Deleted,
}

pub struct WebhookEndpoint {
    pub id: WebhookEndpointId,
    pub workspace_id: WorkspaceId,
    pub direction: WebhookDirection,
    pub display_name: BoundedText,
    pub state: WebhookEndpointState,
    pub active_revision_id: Option<WebhookEndpointRevisionId>,
    pub state_revision: u64,
    pub created_by: PrincipalId,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
```

Rules:

- direction never changes for one endpoint identity;
- lifecycle changes require expected `state_revision` in the command;
- deleted/revoked endpoint cannot be reactivated;
- active revision must belong to the endpoint and match direction;
- endpoint identity contains no secret or destination authority.

### Endpoint lifecycle commands

```rust
pub struct ActivateWebhookEndpoint {
    pub endpoint_id: WebhookEndpointId,
    pub revision_id: WebhookEndpointRevisionId,
    pub expected_state_revision: u64,
    pub idempotency_key: String,
    pub requested_by: PrincipalId,
}

pub struct ChangeWebhookEndpointState {
    pub endpoint_id: WebhookEndpointId,
    pub target_state: WebhookEndpointState,
    pub expected_state_revision: u64,
    pub idempotency_key: String,
    pub requested_by: PrincipalId,
}
```

Expected revisions belong to commands and are not persisted as historical expectations.

### Route handle binding

```rust
pub struct WebhookRouteHandleBinding {
    pub id: WebhookRouteHandleId,
    pub workspace_id: WorkspaceId,
    pub endpoint_revision_id: WebhookEndpointRevisionId,
    pub handle_hash: [u8; 32],
    pub handle_fingerprint: [u8; 8],
    pub valid_from: Timestamp,
    pub valid_until: Option<Timestamp>,
    pub revoked_at: Option<Timestamp>,
    pub created_at: Timestamp,
}
```

Rules:

- handle bytes use at least 256 bits of CSPRNG entropy;
- raw handle is returned once and never persisted;
- hash uses a deployment-bound keyed domain separator;
- binding belongs only to an inbound endpoint revision;
- rotation creates a new binding and revokes the previous binding according to explicit overlap policy;
- handle lookup returns no workspace-specific error detail.

### Endpoint revision

```rust
pub struct WebhookEndpointRevision {
    pub id: WebhookEndpointRevisionId,
    pub endpoint_id: WebhookEndpointId,
    pub workspace_id: WorkspaceId,
    pub revision: u32,
    pub direction: WebhookDirection,
    pub protocol_version: WebhookProtocolVersion,
    pub route_handle_id: Option<WebhookRouteHandleId>,
    pub authentication: WebhookAuthenticationProfile,
    pub signing: Option<WebhookSigningProfile>,
    pub network_policy: WebhookNetworkPolicy,
    pub payload_policy: WebhookPayloadPolicy,
    pub replay_policy: Option<WebhookReplayPolicy>,
    pub retry_policy: Option<WebhookRetryPolicy>,
    pub classification_ceiling: DataClassification,
    pub secret_binding_revision_ids: Vec<ServiceCredentialBindingRevisionId>,
    pub canonical_hash: [u8; 32],
    pub policy_decision_id: PolicyDecisionId,
    pub created_by: PrincipalId,
    pub created_at: Timestamp,
}
```

Revision invariants:

- revision is positive and contiguous per endpoint;
- direction agrees with endpoint identity;
- inbound revisions require `route_handle_id`, replay policy and no retry policy;
- outbound revisions require an exact normalized destination in network policy, retry policy and no route handle/replay policy;
- secret bindings are exact H8 references, never raw secrets;
- canonical hash covers all semantic configuration;
- revision is immutable.

### Authentication profiles

```rust
#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WebhookAuthenticationProfile {
    HmacSha256 {
        key_binding_revision_id: ServiceCredentialBindingRevisionId,
        wire_profile: HmacWebhookWireProfile,
    },
    Ed25519 {
        public_key_binding_revision_id: ServiceCredentialBindingRevisionId,
        wire_profile: Ed25519WebhookWireProfile,
    },
    MutualTls {
        trust_profile_revision_id: TlsTrustProfileRevisionId,
        identity_allowlist: Vec<CertificateIdentityHash>,
    },
    ProviderSpecific {
        profile_revision_id: WebhookProviderProfileRevisionId,
    },
}
```

Initial production support must include HMAC-SHA-256 and Ed25519. mTLS and provider-specific profiles use the same registry/conformance contracts and remain disabled until their adapters pass dedicated acceptance tests. No profile permits unsigned fallback.

### Network policy

```rust
pub struct WebhookNetworkPolicy {
    pub allowed_schemes: std::collections::BTreeSet<NormalizedScheme>,
    pub exact_destination: Option<NormalizedUrl>,
    pub allowed_ports: std::collections::BTreeSet<u16>,
    pub allowed_cidrs: Vec<NormalizedCidr>,
    pub trusted_proxy_profile_id: Option<TrustedProxyProfileId>,
    pub permit_private_target: bool,
    pub redirects: WebhookRedirectPolicy,
    pub maximum_resolved_addresses: u16,
    pub connect_timeout_ms: u64,
    pub request_timeout_ms: u64,
}
```

Rules:

- outbound initial default permits only HTTPS;
- `permit_private_target=true` requires exact H2/deployment policy and exact target constraints;
- wildcard private CIDRs are rejected for ordinary workspace endpoints;
- redirects are `Deny` in the initial version;
- ambient HTTP proxy environment variables are ignored;
- every resolved address is validated before connection;
- mixed allowed/denied answer sets fail closed;
- connection uses a validated address while TLS SNI/Host remains the normalized hostname;
- a new resolution snapshot is required for each attempt.

### Payload policy

```rust
pub struct WebhookPayloadPolicy {
    pub allowed_content_types: std::collections::BTreeSet<BoundedMediaType>,
    pub maximum_wire_bytes: u64,
    pub maximum_decoded_bytes: u64,
    pub maximum_json_depth: u16,
    pub maximum_array_items: u32,
    pub maximum_string_bytes: u64,
    pub maximum_expansion_ratio_basis_points: u32,
    pub schema_revision_id: WebhookPayloadSchemaRevisionId,
    pub retention_class: RetentionClassRef,
}
```

Bounds are validated at revision creation and again during processing.

### Replay policy

```rust
pub struct WebhookReplayPolicy {
    pub maximum_clock_skew_ms: u64,
    pub replay_window_ms: u64,
    pub require_external_event_id: bool,
    pub require_nonce: bool,
    pub duplicate_response: WebhookDuplicateResponsePolicy,
}
```

The replay window is bounded by deployment policy. External event ID and nonce values are stored only as normalized hashes.

### Inbound receipt

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InboundWebhookDisposition {
    Accepted,
    AcceptedDuplicate,
    RejectedAuthentication,
    RejectedReplay,
    RejectedAllowlist,
    RejectedPayload,
    RejectedSchema,
    RejectedPolicy,
    Quarantined,
    SuspendedEndpoint,
}

pub struct InboundWebhookReceipt {
    pub id: InboundWebhookReceiptId,
    pub workspace_id: WorkspaceId,
    pub endpoint_revision_id: WebhookEndpointRevisionId,
    pub request_fingerprint: [u8; 32],
    pub external_event_id_hash: Option<[u8; 32]>,
    pub nonce_hash: Option<[u8; 32]>,
    pub signature_result: WebhookVerificationResult,
    pub timestamp_result: WebhookVerificationResult,
    pub replay_result: WebhookVerificationResult,
    pub allowlist_result: WebhookVerificationResult,
    pub body_hash: [u8; 32],
    pub payload_artifact_revision_id: Option<ArtifactRevisionId>,
    pub source_fact_id: Option<WebhookSourceFactId>,
    pub disposition: InboundWebhookDisposition,
    pub safe_reason_code: String,
    pub retention_class: RetentionClassRef,
    pub recorded_at: Timestamp,
}
```

Receipt contains no raw signature, secret, unrestricted header or external body.

### Source fact

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WebhookSourceTrustClass {
    TransportAuthenticatedExternal,
    MutualTlsAuthenticatedExternal,
    ProviderVerifiedExternal,
}

pub struct WebhookSourceFact {
    pub id: WebhookSourceFactId,
    pub workspace_id: WorkspaceId,
    pub endpoint_revision_id: WebhookEndpointRevisionId,
    pub external_event_type: BoundedText,
    pub external_event_id_hash: Option<[u8; 32]>,
    pub occurred_at: Option<Timestamp>,
    pub recorded_at: Timestamp,
    pub trust_class: WebhookSourceTrustClass,
    pub normalized_payload: BoundedJsonObject,
    pub payload_artifact_revision_id: Option<ArtifactRevisionId>,
    pub payload_hash: [u8; 32],
    pub schema_revision_id: WebhookPayloadSchemaRevisionId,
    pub dedup_key: [u8; 32],
    pub quarantine_state: WebhookPayloadQuarantineState,
}
```

Source fact invariants:

- normalized payload uses an allowlisted schema and bounded keys/values;
- unrecognized fields are rejected or placed only in a quarantined Artifact according to policy;
- source fact is append-only;
- transport authentication does not elevate content to system instruction;
- dedup key is deterministic for exact endpoint revision and external identity/body policy;
- one accepted dedup key maps to one source fact.

### Subscription lifecycle identity

Reuse H11 `WebhookSubscriptionId`:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WebhookSubscriptionState {
    Draft,
    Disabled,
    Active,
    Suspended,
    Revoked,
}

pub struct WebhookSubscription {
    pub id: WebhookSubscriptionId,
    pub workspace_id: WorkspaceId,
    pub state: WebhookSubscriptionState,
    pub active_revision_id: Option<WebhookSubscriptionRevisionId>,
    pub state_revision: u64,
    pub created_by: PrincipalId,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

pub struct ActivateWebhookSubscription {
    pub subscription_id: WebhookSubscriptionId,
    pub revision_id: WebhookSubscriptionRevisionId,
    pub expected_state_revision: u64,
    pub idempotency_key: String,
    pub requested_by: PrincipalId,
}
```

Subscription lifecycle does not live inside immutable revisions.

### Subscription revision

```rust
pub struct WebhookSubscriptionRevision {
    pub id: WebhookSubscriptionRevisionId,
    pub subscription_id: WebhookSubscriptionId,
    pub workspace_id: WorkspaceId,
    pub revision: u32,
    pub endpoint_revision_id: WebhookEndpointRevisionId,
    pub source_event_filter_revision_id: WebhookEventFilterRevisionId,
    pub payload_template_revision_id: WebhookPayloadTemplateRevisionId,
    pub classification_policy_revision_id: PolicyRevisionId,
    pub canonical_hash: [u8; 32],
    pub created_by: PrincipalId,
    pub policy_decision_id: PolicyDecisionId,
    pub created_at: Timestamp,
}
```

Filter/template contracts are bounded declarative data. They cannot execute code, SQL, shell, unrestricted regex, network reads or model calls.

### Delivery intent

Reuse H11 `WebhookDeliveryId`:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WebhookDeliveryStatus {
    Pending,
    Delivering,
    Succeeded,
    FailedRetryable,
    FailedPermanent,
    OutcomeUnknown,
    Cancelled,
    DeadLetter,
}

pub struct WebhookDeliveryIntent {
    pub id: WebhookDeliveryId,
    pub workspace_id: WorkspaceId,
    pub subscription_revision_id: WebhookSubscriptionRevisionId,
    pub endpoint_revision_id: WebhookEndpointRevisionId,
    pub source_event_ref: PublicEventRef,
    pub source_event_hash: [u8; 32],
    pub payload_schema_version: u16,
    pub payload_artifact_revision_id: ArtifactRevisionId,
    pub payload_hash: [u8; 32],
    pub idempotency_key: String,
    pub status: WebhookDeliveryStatus,
    pub next_attempt_at: Option<Timestamp>,
    pub state_revision: u64,
    pub attempt_count: u16,
    pub created_at: Timestamp,
    pub terminal_at: Option<Timestamp>,
}
```

Unique key:

```text
(subscription_revision_id, source_event_ref)
```

This guarantees one logical delivery intent for one subscription revision and source event.

### Attempt

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WebhookTransportDisposition {
    NotSent,
    RejectedBeforeBody,
    ResponseReceived,
    PossiblyDelivered,
    ConfirmedByReconciliation,
}

pub struct WebhookDeliveryAttempt {
    pub id: WebhookDeliveryAttemptId,
    pub delivery_id: WebhookDeliveryId,
    pub attempt_number: u16,
    pub network_resolution_snapshot_id: WebhookNetworkResolutionSnapshotId,
    pub request_fingerprint: [u8; 32],
    pub signing_key_revision_ref: Option<String>,
    pub started_at: Timestamp,
    pub completed_at: Option<Timestamp>,
    pub transport_disposition: WebhookTransportDisposition,
    pub http_status_class: Option<u16>,
    pub response_body_hash: Option<[u8; 32]>,
    pub receiver_receipt_hash: Option<[u8; 32]>,
    pub bounded_error_category: Option<WebhookDeliveryErrorCategory>,
}
```

Attempt rows are append-only. Delivery status is updated with optimistic concurrency.

### Reconciliation

```rust
pub struct WebhookReconciliationRecord {
    pub id: WebhookReconciliationId,
    pub delivery_id: WebhookDeliveryId,
    pub endpoint_revision_id: WebhookEndpointRevisionId,
    pub method: WebhookReconciliationMethod,
    pub request_fingerprint: [u8; 32],
    pub result: WebhookReconciliationResult,
    pub receiver_receipt_hash: Option<[u8; 32]>,
    pub policy_decision_id: PolicyDecisionId,
    pub created_at: Timestamp,
}
```

A reconciliation result may move `OutcomeUnknown` to `Succeeded`, `FailedPermanent` or `FailedRetryable` according to exact policy. It never changes the source event.

---

## Application ports

### H2 authorization

```rust
#[async_trait::async_trait]
pub trait WebhookAuthorizationPort: Send + Sync {
    async fn authorize_endpoint_revision(
        &self,
        request: WebhookEndpointAuthorizationRequest,
    ) -> Result<WebhookEndpointAuthorizationDecision, WebhookError>;

    async fn authorize_inbound_consequence(
        &self,
        request: WebhookInboundConsequenceRequest,
    ) -> Result<WebhookInboundConsequenceDecision, WebhookError>;

    async fn authorize_delivery_attempt(
        &self,
        request: WebhookDeliveryAuthorizationRequest,
    ) -> Result<WebhookDeliveryAuthorizationDecision, WebhookError>;
}
```

### H7 source and event ports

```rust
#[async_trait::async_trait]
pub trait WebhookSourceFactPort: Send + Sync {
    async fn record_verified_source_fact(
        &self,
        receipt: InboundWebhookReceipt,
        fact: WebhookSourceFact,
    ) -> Result<RecordedWebhookSourceFact, WebhookError>;

    async fn evaluate_source_fact(
        &self,
        source_fact_id: WebhookSourceFactId,
    ) -> Result<WebhookTriggerEvaluationReceipt, WebhookError>;
}

#[async_trait::async_trait]
pub trait WebhookPublicEventPort: Send + Sync {
    async fn read_after(
        &self,
        workspace_id: WorkspaceId,
        cursor: PublicEventCursor,
        limit: u16,
    ) -> Result<Vec<PublicEventRecord>, WebhookError>;
}
```

`record_verified_source_fact` is the only inbound bridge. It does not expose a Run mutation port.

### H8 cryptographic ports

```rust
#[async_trait::async_trait]
pub trait WebhookCryptographicPort: Send + Sync {
    async fn verify_inbound(
        &self,
        request: VerifyInboundWebhookRequest,
    ) -> Result<InboundWebhookCryptographicResult, WebhookError>;

    async fn sign_outbound(
        &self,
        request: SignOutboundWebhookRequest,
    ) -> Result<SignedWebhookHeaders, WebhookError>;
}
```

Requests contain exact binding revision references and hashes/bytes under bounded request-scoped handling. Responses never expose secret bytes.

### H6 payload port

```rust
#[async_trait::async_trait]
pub trait WebhookPayloadArtifactPort: Send + Sync {
    async fn quarantine_inbound(
        &self,
        request: QuarantineInboundWebhookPayload,
    ) -> Result<ArtifactRevisionId, WebhookError>;

    async fn persist_outbound_payload(
        &self,
        request: PersistOutboundWebhookPayload,
    ) -> Result<ArtifactRevisionId, WebhookError>;

    async fn open_exact_payload(
        &self,
        revision_id: ArtifactRevisionId,
        expected_hash: [u8; 32],
    ) -> Result<BoundedByteStream, WebhookError>;
}
```

### Network ports

```rust
#[async_trait::async_trait]
pub trait WebhookResolverPort: Send + Sync {
    async fn resolve_and_validate(
        &self,
        request: WebhookResolutionRequest,
    ) -> Result<WebhookNetworkResolutionSnapshot, WebhookError>;
}

#[async_trait::async_trait]
pub trait WebhookTransportPort: Send + Sync {
    async fn send(
        &self,
        request: OutboundWebhookRequest,
    ) -> Result<OutboundWebhookTransportResult, WebhookError>;
}
```

Transport receives only a validated resolution snapshot, exact immutable payload stream and allowlisted headers.

---

## Inbound processing pipeline

Normative order:

```text
request accepted by bounded route
→ opaque handle lookup
→ endpoint/capability/lifecycle check
→ trusted proxy normalization
→ origin/network allowlist
→ header allowlist and body limits
→ exact raw-byte signature verification
→ timestamp and replay-window verification
→ event ID/nonce dedup check
→ bounded decoding and schema validation
→ classification/quarantine
→ atomic receipt + source fact commit
→ response to sender
→ post-commit H7 trigger evaluation
```

### Failure ordering

- body bytes are never parsed before wire-size and content-type gates;
- compressed bytes are streamed through expansion-ratio and decoded-size gates;
- signature verification occurs against the exact required byte representation;
- invalid authentication never reaches schema or trigger logic where timing/details could leak configuration;
- schema failure creates a bounded rejected receipt only when policy permits storing that security fact;
- no failed request creates a source fact;
- no pre-commit stage invokes H7 trigger evaluation.

### Atomicity

For large payloads, H6 quarantine finalizes first and returns an exact revision reference. The accepted PostgreSQL transaction then writes:

```text
InboundWebhookReceipt
WebhookSourceFact
dedup binding
H6 Artifact reference
content-free H10 audit intent/outbox
```

If the database transaction fails after H6 staging/finalization, orphan cleanup follows the H6 ingestion/retention contract and never converts the bytes into an accepted source fact.

H7 trigger evaluation is post-commit and idempotent by source fact ID.

A crash after commit but before evaluation is recovered by a durable source-fact evaluation cursor. A crash before commit leaves no accepted source fact.

### Deduplication

- exact duplicate external event ID/nonce and body hash returns `AcceptedDuplicate` and original receipt/source-fact references;
- same ID/nonce with different body hash returns a replay/conflict rejection and security audit fact;
- when no provider ID exists and policy allows it, dedup uses endpoint revision + timestamp bucket + body hash + bounded sender identity;
- duplicate receipt does not create a new trigger occurrence;
- dedup retention covers at least the configured replay window plus maximum clock skew.

### Large payloads

- inline normalized JSON is bounded and schema allowlisted;
- payloads exceeding inline limits are streamed to H6 quarantine before acceptance;
- executable/multipart/archive content is denied by default;
- a quarantined Artifact cannot enter Run context until H6 inspection and H2/H7 consequence policy allow it;
- source fact records exact quarantine state and Artifact revision.

---

## Outbound processing pipeline

Normative order:

```text
committed H7 public/source event
→ subscription projector reads durable cursor
→ active subscription/revision check
→ exact filter evaluation
→ unique delivery intent creation
→ bounded payload rendering
→ classification and H2 export/delivery authorization
→ immutable H6 payload Artifact
→ Pending delivery
→ current endpoint/subscription/policy/H8 checks
→ fresh DNS resolution and SSRF validation
→ attempt row + lease
→ H8 signing
→ one HTTP request
→ response classification / OutcomeUnknown
→ retry, reconciliation or terminal state
```

### Source projector

- H7 cursor is persisted after delivery intents are committed;
- unique `(subscription_revision_id, source_event_ref)` prevents duplicates on cursor replay;
- unsupported/unknown source event schemas are not rendered optimistically;
- filter evaluation uses bounded typed fields and exact event schema versions;
- source event commit never waits for receiver availability.

### Payload rendering

- template revision is immutable and declarative;
- rendered body is validated against exact outbound payload schema;
- body contains only allowlisted fields and safe references;
- Artifact bytes are included only through explicit authorized representations/links, never arbitrary backend paths;
- payload is canonicalized, hashed and persisted once;
- retries reopen exact H6 bytes by revision and expected hash;
- renderer does not run a model or arbitrary code;
- rendering failure creates a permanent bounded delivery failure without external attempt.

### Signing

Reference initial outbound signature contract:

```text
signature_input = timestamp + "." + delivery_id + "." + exact_body_bytes
```

Each wire profile defines exact header names, signature encoding and algorithm.

- delivery ID and exact body remain stable;
- timestamp/signature are attempt-scoped;
- key revision is selected by endpoint revision/H8 policy, not the model;
- key rotation does not rewrite previous attempts;
- signature failure before network send is `NotSent` and may be retried after H8 recovery within policy.

### Response classification

Initial classification:

```text
2xx                         -> Succeeded
408, 425, 429, selected 5xx -> FailedRetryable only when retry safety is known
other bounded 4xx           -> FailedPermanent
connect failure before send -> FailedRetryable
TLS/SSRF/policy failure      -> FailedPermanent or suspended endpoint
loss after body may be sent  -> OutcomeUnknown
```

`Retry-After` is parsed only when syntactically valid and capped by policy.

### OutcomeUnknown

When request body may have reached the receiver but no conclusive response exists:

1. append a `PossiblyDelivered` attempt;
2. move delivery to `OutcomeUnknown`;
3. stop ordinary retry scheduling;
4. attempt configured reconciliation, when available;
5. permit a retry only when receiver idempotency for exact delivery ID is contractually configured and current H2 policy permits it;
6. otherwise require intervention or move to dead letter according to policy.

No blind duplicate external commitment is allowed.

---

## Retry policy

```rust
pub struct WebhookRetryPolicy {
    pub maximum_attempts: u16,
    pub initial_delay_ms: u64,
    pub maximum_delay_ms: u64,
    pub multiplier_basis_points: u32,
    pub jitter_basis_points: u16,
    pub maximum_elapsed_ms: u64,
    pub receiver_idempotency: WebhookReceiverIdempotency,
    pub reconciliation_profile_id: Option<WebhookReconciliationProfileId>,
}
```

Rules:

- all fields are bounded by deployment policy;
- maximum attempts include the first attempt;
- retry policy cannot override endpoint/subscription revocation, H2 denial or H8 unavailability policy;
- retry does not rerender payload;
- retry does not create a new delivery ID;
- dead-letter transition is terminal until an explicit new authorized redelivery command creates a new logical delivery linked to the old one.

---

## Persistence model

### Migration `0096_webhook_endpoints_revisions_and_capabilities.sql`

Create/extend:

```text
webhook_capability_settings
webhook_endpoints
webhook_endpoint_revisions
webhook_endpoint_state_history
webhook_route_handles
webhook_provider_profiles
```

Constraints:

- endpoint direction immutable;
- revision uniqueness `(endpoint_id, revision)`;
- canonical hash uniqueness per exact revision;
- one active revision per endpoint lifecycle identity;
- raw route handle never stored;
- immutable revision and history rows;
- no secret bytes or destination credentials;
- optimistic concurrency on endpoint state;
- deployment/workspace capability defaults are disabled.

### Migration `0097_inbound_webhook_receipts_source_facts_and_dedup.sql`

Create:

```text
inbound_webhook_receipts
webhook_source_facts
webhook_inbound_dedup_bindings
webhook_source_fact_evaluation_cursors
```

Constraints:

- one accepted source fact per dedup key;
- same external identity with changed body hash records conflict, not overwrite;
- source facts append-only;
- receipts append-only except retention/purge markers;
- Artifact references use composite workspace ownership;
- normalized payload size/check constraints;
- no raw signature/header columns;
- restart-safe evaluation cursor.

### Migration `0098_outbound_webhook_subscriptions_deliveries_reconciliation.sql`

Forward-extend H11 baseline tables and create:

```text
webhook_subscription_revisions
webhook_subscription_state_history
webhook_event_filter_revisions
webhook_payload_template_revisions
webhook_delivery_payload_bindings
webhook_reconciliations
webhook_dead_letters
```

Extend baseline subscription/delivery/attempt rows with exact revision, active revision, payload, outcome and state-revision fields.

Constraints:

- mutable subscription identity separated from immutable revision;
- unique logical intent `(subscription_revision_id, source_event_authority, source_event_id)`;
- immutable filter/template/subscription revisions;
- exact H6 payload revision/hash binding;
- attempts append-only and sequential;
- reconciliation append-only;
- terminal delivery states cannot return to active states;
- no body bytes or secrets in attempt rows.

### Migration `0099_webhook_rls_network_snapshots_indexes_and_worker_state.sql`

Create/extend:

```text
webhook_network_resolution_snapshots
webhook_projection_cursors
webhook_worker_leases
webhook_endpoint_health
```

Add:

- forced RLS on every workspace-owned table;
- composite workspace foreign keys;
- due-delivery partial indexes;
- source-fact evaluation indexes;
- dedup expiry indexes;
- endpoint/subscription lifecycle indexes;
- lease fencing tokens;
- retention/purge indexes;
- constraints preventing cross-workspace endpoint/subscription/delivery bindings.

Worker leases and heartbeats are operational state and do not increment endpoint, subscription or delivery logical revisions unless a lifecycle transition occurs.

---

## H10 audit and metrics

Mandatory content-free audit facts:

```text
webhook.endpoint_revision_created
webhook.endpoint_activated
webhook.endpoint_suspended
webhook.endpoint_revoked
webhook.route_handle_rotated
webhook.inbound_rejected_authentication
webhook.inbound_replay_conflict
webhook.inbound_source_fact_recorded
webhook.subscription_revision_created
webhook.subscription_activated
webhook.delivery_intent_created
webhook.delivery_attempted
webhook.delivery_outcome_unknown
webhook.delivery_reconciled
webhook.delivery_dead_lettered
```

Audit fields may include IDs, revisions, hashes, disposition, policy decision references and bounded reason codes. They exclude raw payloads, signatures, URLs with credentials, secret backend details and receiver response bodies.

Bounded metrics:

```text
webhook_inbound_total{profile,outcome}
webhook_inbound_bytes_bucket{profile}
webhook_inbound_duplicate_total{profile}
webhook_delivery_total{outcome}
webhook_delivery_attempts_bucket{endpoint_class}
webhook_delivery_unknown_total{endpoint_class}
webhook_delivery_reconciliation_total{outcome}
webhook_delivery_latency_ms{endpoint_class,outcome}
```

Workspace, endpoint ID, URL, event ID and delivery ID are prohibited metric labels.

---

## H11 surfaces

### Management routes

```text
POST   /v1/workspaces/{workspace}/webhook-endpoints
POST   /v1/workspaces/{workspace}/webhook-endpoints/{id}/revisions
POST   /v1/workspaces/{workspace}/webhook-endpoints/{id}:activate
POST   /v1/workspaces/{workspace}/webhook-endpoints/{id}:disable
POST   /v1/workspaces/{workspace}/webhook-endpoints/{id}:suspend
POST   /v1/workspaces/{workspace}/webhook-endpoints/{id}:revoke
POST   /v1/workspaces/{workspace}/webhook-endpoints/{id}:rotate-handle
GET    /v1/workspaces/{workspace}/webhook-endpoints/{id}
POST   /v1/workspaces/{workspace}/webhook-subscriptions
POST   /v1/workspaces/{workspace}/webhook-subscriptions/{id}/revisions
POST   /v1/workspaces/{workspace}/webhook-subscriptions/{id}:activate
POST   /v1/workspaces/{workspace}/webhook-subscriptions/{id}:disable
POST   /v1/workspaces/{workspace}/webhook-subscriptions/{id}:revoke
GET    /v1/workspaces/{workspace}/webhook-deliveries/{id}
POST   /v1/workspaces/{workspace}/webhook-deliveries/{id}:reconcile
POST   /v1/workspaces/{workspace}/webhook-deliveries/{id}:redeliver
```

State-changing routes require `Idempotency-Key`; lifecycle changes require `If-Match`/expected revision.

### Inbound route

```text
POST /hooks/v1/{opaque_handle}
```

The response is bounded and does not reveal workspace, endpoint configuration, policy, signature details or trigger outcome.

### Public DTO rules

DTOs expose:

- IDs and immutable revision references;
- lifecycle state and logical revision;
- safe destination origin summary, never embedded credentials;
- auth/signing profile kind and key revision fingerprint, never secret reference details;
- receipt/delivery status and bounded reason codes;
- hashes, timestamps, attempt counts and reconciliation state.

DTOs do not expose raw payloads by default. Authorized H6 Artifact access is a separate operation.

HTTP, CLI, MCP where applicable, Rust SDK and TypeScript SDK call the same application services and preserve idempotency/concurrency semantics.

---

## Failure semantics

### H8 unavailable

- inbound signed requests fail authentication safely; no unsigned fallback;
- outbound attempts remain unsent and retryable according to bounded dependency policy;
- no secret or signature material is cached in application state.

### H6 unavailable

- large inbound payload acceptance fails closed or remains quarantined according to exact policy;
- outbound payload rendering cannot complete and no network attempt starts;
- ordinary logs are never used as payload fallback.

### H7 unavailable after inbound commit

- accepted source fact remains durable;
- sender may already receive accepted response;
- evaluation cursor retries idempotently;
- no second source fact is created.

### PostgreSQL failure

- no accepted response is returned before receipt/source-fact commit;
- outbound attempt cannot begin without durable lease/attempt intent;
- ambiguous transaction result is reconciled by idempotency key and unique constraints.

### DNS or network policy failure

- no connection occurs to an unvalidated address;
- endpoint may be degraded/suspended after bounded repeated policy failures;
- failure does not trigger fallback to a different URL.

### Receiver response body

- body is bounded before reading;
- raw body is not persisted;
- optional receiver receipt is validated and stored only as a hash/bounded typed value;
- response text never becomes instructions or a source event.

---

## Implementation tasks

### Task 1: Freeze H11 baseline compatibility and boundary tests

- [ ] Write failing compile/boundary tests proving one webhook composition root, reuse of H11 IDs and absence of direct H1 mutation ports.
- [ ] Add forbidden-import checks for SQLx/Axum/H8 adapter types in domain/application contracts.
- [ ] Add migration-ownership assertions that `0083` is unchanged and `0096`–`0099` are uniquely reserved.
- [ ] Add minimal module/type scaffolding until tests compile and pass.
- [ ] Commit independently.

### Task 2: Implement endpoint identity, route handles, immutable revisions and capability posture

- [ ] Write failing constructor tests for disabled defaults, immutable direction, route-handle one-time exposure, revision hashes and invalid auth/network combinations.
- [ ] Write failing service tests for expected revision, idempotent activation, handle rotation and terminal revocation.
- [ ] Implement domain constructors/state transitions without persistence.
- [ ] Implement H2 authorization requests and content-free audit intents.
- [ ] Verify definition/revision/handle creation cannot activate an endpoint.
- [ ] Commit.

### Task 3: Add endpoint persistence migration

- [ ] Write failing PostgreSQL tests for revision immutability, keyed route-handle hashing, disabled defaults, workspace ownership and optimistic concurrency.
- [ ] Add migration `0096` without editing `0083`.
- [ ] Implement repositories using scoped transactions and forced RLS.
- [ ] Test migration forward application from exact H11 migration head.
- [ ] Test restart and idempotent command recovery.
- [ ] Commit.

### Task 4: Implement bounded inbound runtime verification

- [ ] Write failing unit/property tests for wire/decoded limits, JSON depth, compression ratio, header allowlist and canonicalization profiles.
- [ ] Add HMAC/Ed25519 fixture conformance tests including algorithm downgrade and malformed signatures.
- [ ] Add trusted-proxy tests proving spoofed forwarding headers are ignored.
- [ ] Implement streaming gates and verification in normative order.
- [ ] Verify invalid authentication never reaches schema/trigger evaluation.
- [ ] Commit.

### Task 5: Persist inbound receipts/source facts and integrate H7

- [ ] Write failing atomicity tests for accepted receipt + source fact + dedup binding.
- [ ] Write failing duplicate tests for same body and conflict tests for changed body.
- [ ] Write failing restart test for crash after commit before trigger evaluation.
- [ ] Add migration `0097` and repositories.
- [ ] Implement post-commit H7 evaluation cursor keyed by source fact ID.
- [ ] Prove no direct Run mutation occurs and duplicate delivery creates no second trigger occurrence.
- [ ] Commit.

### Task 6: Implement subscription lifecycle, revisions and source projector

- [ ] Write failing tests proving mutable subscription lifecycle is separate from immutable revisions.
- [ ] Write failing tests for declarative filter/template bounds.
- [ ] Write failing projector restart test and unique source-event intent test.
- [ ] Add forward migration extensions to H11 baseline tables in `0098`.
- [ ] Implement durable H7 cursor projection and unique delivery creation.
- [ ] Verify unsupported event schema fails closed without cursor data loss.
- [ ] Commit.

### Task 7: Render and persist immutable outbound payloads

- [ ] Write failing tests that retries receive byte-identical payloads.
- [ ] Write failing tests for classification ceiling, missing Artifact authorization and secret/redaction failure.
- [ ] Implement bounded declarative rendering and exact schema validation.
- [ ] Persist exact payload as H6 Artifact revision and bind hash/revision to delivery intent.
- [ ] Verify no body bytes are stored in attempt rows or ordinary logs.
- [ ] Commit.

### Task 8: Implement safe resolver, egress and H8 signing

- [ ] Write failing SSRF tests for loopback, private, link-local, metadata, mixed answers and DNS rebinding.
- [ ] Write failing tests for redirects, ambient proxies, invalid TLS and credential-bearing URLs.
- [ ] Write failing H8 tests for key unavailability, rotation and no secret leakage.
- [ ] Implement per-attempt resolution snapshots and validated-address connections.
- [ ] Implement attempt-scoped H8 signing over exact immutable body bytes.
- [ ] Commit.

### Task 9: Implement delivery state machine, retries and reconciliation

- [ ] Write failing state-machine tests for 2xx, bounded 4xx, 429, 5xx, pre-send failures and post-body timeout.
- [ ] Write failing `OutcomeUnknown` tests proving no ordinary retry scheduling.
- [ ] Write failing idempotent-receiver and reconciliation tests.
- [ ] Add migration `0099`, worker leases, network snapshots and due indexes.
- [ ] Implement fenced worker claims, append-only attempts and optimistic delivery transitions.
- [ ] Implement explicit reconciliation and dead-letter lifecycle.
- [ ] Commit.

### Task 10: Add H10 audit, metrics and retention/purge integration

- [ ] Write failing no-secrets audit/log tests.
- [ ] Write failing bounded-cardinality tests.
- [ ] Write failing purge tests for inbound/outbound H6 payload Artifacts and retained content-free evidence.
- [ ] Implement audit intents and metrics from approved allowlists.
- [ ] Verify endpoint URLs/IDs and workspace IDs never become metric labels.
- [ ] Commit.

### Task 11: Add H11 routes, CLI, SDK and schema parity

- [ ] Write failing contract tests for idempotency, expected revision, one-time handle return, safe errors and bounded DTOs.
- [ ] Write failing parity tests across HTTP, local CLI and SDK clients for overlapping operations.
- [ ] Add inbound route with opaque handle and indistinguishable failure responses.
- [ ] Generate schemas and update compatibility baseline in same change.
- [ ] Verify public surfaces cannot retrieve raw payload without separate H6 authorization.
- [ ] Commit.

### Task 12: Complete security, restart and acceptance gates

- [ ] Run PostgreSQL RLS tests across all webhook tables.
- [ ] Run restart tests at every inbound and outbound state boundary.
- [ ] Run SSRF/DNS-rebinding/proxy/TLS/security suites.
- [ ] Run H8/H6 failure-injection suites.
- [ ] Run migration ownership and forward-only checks.
- [ ] Run all webhook acceptance scenarios.
- [ ] Run repository formatting, linting, unit, integration and schema-baseline checks.
- [ ] Use `superpowers:verification-before-completion` and record exact command evidence in implementation PR.

---

## Acceptance scenarios

1. Deployment/workspace defaults keep inbound and outbound disabled.
2. Creating endpoint/revision/route handle does not activate endpoint.
3. Raw route handle is returned once and never retrievable from persistence.
4. Unknown/revoked inbound handles return indistinguishable bounded responses.
5. Invalid HMAC/Ed25519 signatures create no source fact.
6. Stale timestamps and replayed nonces are rejected.
7. Exact duplicate body/event ID returns original accepted binding.
8. Reused event ID with changed body is a security conflict.
9. Spoofed forwarding headers from untrusted proxy are ignored.
10. Wire, decoded, depth, array, string and expansion limits fail before unbounded allocation.
11. Large allowed inbound content enters H6 quarantine and cannot enter Run context directly.
12. Accepted receipt/source fact commit is atomic.
13. Crash after source-fact commit resumes one H7 evaluation without duplicate occurrence.
14. `approved=true` in payload creates no approval.
15. A verified source fact cannot directly mutate a Run.
16. Subscription revision creation does not activate subscription.
17. One source event and active subscription revision create one delivery intent across projector restart.
18. Payload rendering is deterministic and retries use exact same body bytes/hash.
19. Missing H8 key causes no network request and no unsigned fallback.
20. H8 key rotation affects new attempt signatures without rewriting prior attempts.
21. Public HTTPS destination with valid resolution/TLS can receive one attempt.
22. Loopback/private/link-local/metadata/multicast/reserved targets are denied by default.
23. Mixed public/private DNS answers fail closed.
24. DNS rebinding between validation and connection is prevented.
25. Redirects, cookies and ambient proxies are not used.
26. 2xx marks delivery succeeded without mutating source state.
27. Permanent 4xx terminates according to policy.
28. Bounded 429/5xx retries preserve delivery ID and payload bytes.
29. Connection failure before send is safely retryable.
30. Timeout after body may have been sent produces `OutcomeUnknown`.
31. Receiver without idempotency/reconciliation is not blindly retried from `OutcomeUnknown`.
32. Receiver idempotency or reconciliation can resolve unknown outcome under explicit policy.
33. Endpoint or subscription revocation prevents new attempts immediately.
34. Receiver response bodies remain bounded and are not stored raw.
35. RLS prevents cross-workspace endpoint, receipt, source fact, subscription and delivery access.
36. Purge removes governed H6 payload bytes while retaining minimal content-free evidence.
37. HTTP/CLI/SDK parity produces same canonical state and idempotency binding.
38. No webhook path creates a second event store, trigger runtime, policy engine or H1 authority.

---

## Verification commands required during future implementation

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run -p vestrace-domain webhook
cargo nextest run -p vestrace-application webhook
cargo nextest run -p vestrace-webhook-runtime
cargo nextest run -p vestrace-webhook-test-support
cargo nextest run --test webhook_endpoint_persistence
cargo nextest run --test webhook_route_handle_security
cargo nextest run --test inbound_webhook_atomicity
cargo nextest run --test inbound_webhook_replay
cargo nextest run --test outbound_webhook_subscription_lifecycle
cargo nextest run --test outbound_webhook_intent_dedup
cargo nextest run --test outbound_webhook_payload_immutability
cargo nextest run --test outbound_webhook_ssrf
cargo nextest run --test outbound_webhook_dns_rebinding
cargo nextest run --test outbound_webhook_retry_restart
cargo nextest run --test outbound_webhook_reconciliation
cargo nextest run --test webhook_secret_boundary
cargo nextest run --test webhook_rls
cargo nextest run --test webhook_h11_parity
cargo nextest run --test webhook_acceptance
bash scripts/verify-webhook-boundary.sh
bash scripts/verify-webhook-no-secrets.sh
bash scripts/verify-webhook-network-policy.sh
bash scripts/verify-webhook-outcome-unknown.sh
bash scripts/verify-webhook-migration-ownership.sh
```

Exact package/test selectors may be adjusted to repository conventions only when the implementation PR records equivalent full evidence.

---

## Rollout order

1. Land domain/application contracts and disabled capability defaults.
2. Apply migrations `0096`–`0099` with every endpoint/subscription disabled.
3. Deploy readers, management queries and audit support.
4. Deploy inbound verification/source-fact code with routes unavailable externally.
5. Deploy outbound projector/worker code with no active endpoints/subscriptions.
6. Run local conformance, restart, RLS and security suites.
7. Enable one fixture workspace and local receiver under exact policy.
8. Verify duplicate, retry, `OutcomeUnknown`, reconciliation and purge behavior.
9. Enable production workspaces individually through explicit H2-authorized endpoint/subscription activation.
10. Keep a global deployment kill switch that rejects new inbound acceptance and suspends outbound attempts without rewriting historical evidence.

Rollback disables endpoint capability and workers. It never down-migrates or deletes accepted receipts, source facts, delivery intents or attempts.

---

## Documentation-only boundary

Merging this plan approves only the implementation sequence and contracts. It does not authorize:

- creating `feat/webhook-extension`;
- editing Rust, SQL, schemas, CI or dependencies;
- applying migrations `0096`–`0099`;
- exposing `/hooks/v1/*`;
- creating endpoint secrets or H8 bindings;
- performing DNS resolution or outbound HTTP requests;
- accepting inbound requests;
- activating subscriptions, triggers or endpoint revisions;
- sending notifications or retrying deliveries.

Implementation requires a separate explicit instruction ending the documentation-only phase.

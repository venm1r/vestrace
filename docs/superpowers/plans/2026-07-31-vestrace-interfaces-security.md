# Vestrace Interfaces and Security Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose the approved Vestrace memory, retrieval and operation capabilities through stable HTTP and MCP adapters while enforcing authentication, scoped authorization, delegated authority, sensitivity controls, redaction, approvals, auditing, idempotency and optimistic concurrency.

**Architecture:** HTTP and MCP are thin adapters over the same application services and DTO mapping layer. Authentication establishes a trusted `RequestContext`; clients cannot choose an unrelated workspace or principal. A policy engine evaluates capabilities and scopes before application calls, while PostgreSQL RLS remains the second isolation layer. Sensitive data is classified and redacted before persistence or external model transmission.

**Tech Stack:** Existing Foundation, Memory Core and Retrieval plans, Rust, Axum, Tower, rmcp, Tokio, Serde, Schemars, SQLx, bearer tokens with Argon2 hashing, tracing, tower test utilities.

## Global Constraints

- Complete the first three plans before starting.
- MCP and HTTP call the same application services and preserve the same domain error semantics.
- MCP is agent-facing and narrower than HTTP/admin APIs.
- A client-supplied workspace ID is validated against authenticated context and never establishes authority.
- Authorization defaults to deny.
- Downstream agents, workflows, models and tools cannot gain more authority than their initiator.
- Reading memory and sending memory to a provider are separate capabilities.
- Hard purge, permission changes, external-provider enablement and Restricted remote transfer require approval.
- Secrets are never returned in API DTOs, MCP results, logs or audit payloads.
- All write operations require an idempotency key; versioned mutations require expected revision.
- Stable public error codes must not expose SQL, stack traces or secret values.

---

## Locked file structure additions

```text
crates/vestrace-domain/src/
  security/mod.rs
  security/capability.rs
  security/sensitivity.rs
  security/approval.rs

crates/vestrace-application/src/
  security/mod.rs
  security/ports.rs
  security/policy_engine.rs
  security/redaction.rs
  security/approval.rs
  operations/mod.rs
  operations/ports.rs
  dto/mod.rs
  dto/envelope.rs
  dto/memory.rs
  dto/retrieval.rs
  dto/operations.rs

crates/vestrace-http/src/
  auth.rs
  error.rs
  middleware.rs
  routes/mod.rs
  routes/events.rs
  routes/memories.rs
  routes/retrieval.rs
  routes/operations.rs
  routes/admin.rs
  openapi.rs

crates/vestrace-mcp/
  Cargo.toml
  src/lib.rs
  src/server.rs
  src/context.rs
  src/error.rs
  src/tools/mod.rs
  src/tools/events.rs
  src/tools/memory.rs
  src/tools/retrieval.rs
  src/tools/operations.rs
  src/resources.rs

crates/vestrace-cli/src/commands/mcp.rs

migrations/
  0012_tokens_policies_and_approvals.sql
  0013_audit_and_redaction.sql

tests/
  http_contract.rs
  http_idempotency.rs
  mcp_contract.rs
  http_mcp_equivalence.rs
  authorization.rs
  delegated_authority.rs
  redaction.rs
  provider_data_policy.rs
  approvals.rs
```

---

### Task 1: Add security domain types and capabilities

**Files:**
- Create: `crates/vestrace-domain/src/security/mod.rs`
- Create: `crates/vestrace-domain/src/security/capability.rs`
- Create: `crates/vestrace-domain/src/security/sensitivity.rs`
- Create: `crates/vestrace-domain/src/security/approval.rs`
- Modify: `crates/vestrace-domain/src/id.rs`
- Modify: `crates/vestrace-domain/src/lib.rs`

**Interfaces:**
- Adds `AccessTokenId`, `PolicyId`, `ApprovalRecordId`, `AuditEventId`.
- Produces `Capability`, `Sensitivity`, `DataDestination`, `ApprovalKind`, `ApprovalStatus`, `ApprovalRecord`.

- [ ] **Step 1: Write failing capability parsing tests**

```rust
#[test]
fn capability_round_trips_to_stable_name() {
    let capability: Capability = "memory.read".parse().unwrap();
    assert_eq!(capability.to_string(), "memory.read");
}

#[test]
fn unknown_capability_is_rejected() {
    assert!("root.everything".parse::<Capability>().is_err());
}
```

- [ ] **Step 2: Implement the approved capability set**

Include typed variants for memory, event, context, agent, skill, workflow, execution, model, provider, workspace, audit, export and purge operations. Serialize to stable dotted lowercase names.

- [ ] **Step 3: Implement sensitivity ordering**

```rust
pub enum Sensitivity {
    Public,
    Internal,
    Confidential,
    Restricted,
    Secret,
}
```

Implement `Ord` so unknown or missing classification is treated at least as `Confidential` by policy adapters, never `Public`.

- [ ] **Step 4: Implement approval lifecycle**

Allowed transitions:

```text
Requested → AwaitingApproval → Approved | Rejected | Expired
```

Approved records include approver, approved operation hash and expiry; they cannot authorize a different operation payload.

- [ ] **Step 5: Run and commit**

```bash
cargo test -p vestrace-domain security
git add crates/vestrace-domain
git commit -m "feat(security): add capabilities sensitivity and approvals"
```

---

### Task 2: Define public DTOs, envelopes and stable errors

**Files:**
- Create: `crates/vestrace-application/src/dto/mod.rs`
- Create: `crates/vestrace-application/src/dto/envelope.rs`
- Create: `crates/vestrace-application/src/dto/memory.rs`
- Create: `crates/vestrace-application/src/dto/retrieval.rs`
- Create: `crates/vestrace-application/src/dto/operations.rs`
- Modify: `crates/vestrace-application/src/lib.rs`
- Test: inline schema snapshot tests

**Interfaces:**
- Produces `ApiEnvelope<T>`, `ApiWarning`, `ApiProvenance`, `ApiError`, and versioned request/response DTOs.
- DTOs contain domain IDs and values but no SQLx or provider-client types.

- [ ] **Step 1: Write failing JSON contract tests**

```rust
#[test]
fn success_envelope_uses_stable_fields() {
    let value = serde_json::to_value(ApiEnvelope::success(RequestId::new(), 42)).unwrap();
    assert_eq!(value["status"], "succeeded");
    assert_eq!(value["data"], 42);
    assert_eq!(value["degraded"], false);
}
```

- [ ] **Step 2: Implement success and error envelopes**

```rust
pub struct ApiEnvelope<T> {
    pub request_id: RequestId,
    pub status: OperationStatus,
    pub data: Option<T>,
    pub error: Option<ApiError>,
    pub warnings: Vec<ApiWarning>,
    pub provenance: ApiProvenance,
    pub degraded: bool,
}
```

- [ ] **Step 3: Map stable errors**

Support exactly: `invalid_argument`, `unauthenticated`, `forbidden`, `not_found`, `revision_conflict`, `idempotency_conflict`, `policy_violation`, `budget_exceeded`, `provider_unavailable`, `rate_limited`, `operation_failed`, `internal`.

- [ ] **Step 4: Generate and snapshot JSON Schemas**

Use Schemars for each public request and response. Store snapshots under `schemas/v1/`; CI fails when generated output differs from committed schemas.

- [ ] **Step 5: Run and commit**

```bash
cargo test -p vestrace-application dto
git add crates/vestrace-application schemas
git commit -m "feat(api): define stable public DTO contracts"
```

---

### Task 3: Add token, policy and approval persistence

**Files:**
- Create: `migrations/0012_tokens_policies_and_approvals.sql`
- Create: `crates/vestrace-application/src/security/ports.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/security_repository.rs`
- Create: `tests/authorization.rs`

**Interfaces:**
- Produces tables `access_tokens`, `policy_bindings`, `policy_scopes`, `approval_records`.
- Produces `SecurityRepository` and `ApprovalRepository` ports.
- Only token hash, prefix, principal, workspace, expiry and revocation metadata are stored.

- [ ] **Step 1: Write failing token-storage test**

Create token plaintext `vst_test_secret`, persist it, then query raw table and assert plaintext is absent while verification succeeds through the repository.

- [ ] **Step 2: Create schema**

Use Argon2id encoded hashes. Make token prefix unique enough for lookup but not authentication. Include `revoked_at`, `expires_at`, and `last_used_at`.

- [ ] **Step 3: Define policy scope schema**

Represent explicit constraints for workspace, project, user, agent, workflow, memory kinds, sensitivity ceiling and labels. Avoid arbitrary executable policy code in v0.1.

- [ ] **Step 4: Implement repositories and RLS**

Security administration uses a privileged repository; evaluation reads only policies visible to the current workspace.

- [ ] **Step 5: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test authorization
git add migrations/0012_tokens_policies_and_approvals.sql crates tests/authorization.rs
git commit -m "feat(security): persist tokens policies and approvals"
```

---

### Task 4: Implement authentication middleware and trusted request context

**Files:**
- Create: `crates/vestrace-http/src/auth.rs`
- Create: `crates/vestrace-http/src/middleware.rs`
- Modify: `crates/vestrace-http/src/router.rs`
- Create: `tests/http_contract.rs`

**Interfaces:**
- Produces `AuthenticatedPrincipal` and an Axum extractor for `RequestContext`.
- Supports `LocalTrusted` and `BearerToken` modes.
- Request context derives workspace and principal from credentials; body/path workspace is a consistency assertion only.

- [ ] **Step 1: Write failing unauthenticated test**

Call a protected route in bearer mode without a token and expect HTTP `401` plus error code `unauthenticated`.

- [ ] **Step 2: Write failing workspace-substitution test**

Authenticate for workspace A, submit body with workspace B and expect `403 policy_violation`; verify no repository call occurred.

- [ ] **Step 3: Implement bearer extraction**

Reject malformed, expired and revoked tokens with the same external error shape. Log token ID/prefix only, never token text.

- [ ] **Step 4: Implement local trusted mode**

Permit only configured loopback/stdio contexts. Construct one configured principal and workspace; do not accept arbitrary identity headers.

- [ ] **Step 5: Run and commit**

```bash
cargo test --test http_contract
git add crates/vestrace-http tests/http_contract.rs
git commit -m "feat(http): authenticate principals and bind context"
```

---

### Task 5: Implement scoped policy evaluation and delegated authority

**Files:**
- Create: `crates/vestrace-application/src/security/mod.rs`
- Create: `crates/vestrace-application/src/security/policy_engine.rs`
- Create: `tests/delegated_authority.rs`
- Modify application services to accept authorization decisions at command boundary

**Interfaces:**
- Produces `AuthorizationRequest`, `AuthorizationDecision`, `PolicyEngine::authorize`.
- Produces `DelegationContext` carrying initiator, current actor and accumulated capability ceiling.

- [ ] **Step 1: Write failing deny-by-default test**

No matching policy for `memory.read` must return `Denied(NoMatchingPolicy)`.

- [ ] **Step 2: Write failing escalation test**

A user with `memory.read` delegates to an agent requesting `memory.revise`; the effective permissions must not include revise even if the agent definition lists it.

- [ ] **Step 3: Implement capability intersection**

```text
initiator grants
∩ actor grants
∩ workflow/node limits
∩ requested operation requirements
```

Explicit deny wins over allow. Scope must match every constrained dimension.

- [ ] **Step 4: Add service guards**

Guard each application command before opening a write transaction. Still retain RLS to stop accidental unscoped repository access.

- [ ] **Step 5: Run and commit**

```bash
cargo test --test authorization --test delegated_authority
git add crates/vestrace-application tests
git commit -m "feat(security): enforce scoped delegated authority"
```

---

### Task 6: Implement redaction and provider data-sharing policy

**Files:**
- Create: `crates/vestrace-application/src/security/redaction.rs`
- Create: `migrations/0013_audit_and_redaction.sql`
- Create: `crates/vestrace-infrastructure/src/postgres/redaction_repository.rs`
- Create: `tests/redaction.rs`
- Create: `tests/provider_data_policy.rs`

**Interfaces:**
- Produces `RedactionPolicy`, `RedactionRule`, `RedactionResult`, `ProviderDataPolicy`.
- Separates capabilities `memory.read`, `model.data.share.local`, `model.data.share.remote`.

- [ ] **Step 1: Write failing secret-redaction tests**

Input containing `sk-test-1234567890` must be replaced before event persistence and model request construction. Assert the plaintext does not appear in database rows, logs captured by the test subscriber or serialized DTOs.

- [ ] **Step 2: Implement deterministic rules**

Support built-in patterns for API keys, bearer tokens, private-key blocks and password assignments, plus workspace-configured regular expressions. Store original hash, redacted content, matched rule IDs and policy version.

- [ ] **Step 3: Implement provider destination checks**

```rust
pub fn authorize_transfer(
    classification: Sensitivity,
    destination: DataDestination,
    capabilities: &CapabilitySet,
    redaction: &RedactionResult,
) -> Result<(), PolicyViolation>;
```

Restricted remote transfer requires both explicit capability and approved operation. Secret values cannot be transferred or stored as memory.

- [ ] **Step 4: Integrate before provider calls**

Extraction, embedding and future reranking requests must pass the policy gate before HTTP serialization.

- [ ] **Step 5: Run and commit**

```bash
cargo test --test redaction --test provider_data_policy
git add migrations/0013_audit_and_redaction.sql crates tests
git commit -m "feat(security): redact sensitive data and gate providers"
```

---

### Task 7: Implement approval and audit services

**Files:**
- Create: `crates/vestrace-application/src/security/approval.rs`
- Create: `crates/vestrace-application/src/security/audit.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/audit_repository.rs`
- Create: `tests/approvals.rs`

**Interfaces:**
- Produces `RequestApprovalService`, `ResolveApprovalService`, `VerifyApprovalService`, `AuditWriter`.
- Approval binds operation kind, normalized payload hash, requester and expiry.
- Audit events are append-only and exclude sensitive payload content.

- [ ] **Step 1: Write failing payload-binding test**

Approve purge of memory A. Attempt purge of memory B with the same approval and expect `policy_violation`.

- [ ] **Step 2: Implement operation hashing**

Canonicalize a typed command DTO and hash bytes with SHA-256. Include command type and workspace in the signed material.

- [ ] **Step 3: Implement audit writes**

Record allow/deny decision, policy IDs, principal, resource reference, operation and correlation ID. For Restricted reads, always write audit; for ordinary local reads, follow workspace audit policy.

- [ ] **Step 4: Integrate with hard purge and policy changes**

Require a live approval and consume it atomically with the dangerous operation.

- [ ] **Step 5: Run and commit**

```bash
cargo test --test approvals --test authorization
git add crates tests/approvals.rs
git commit -m "feat(security): add approvals and audit trail"
```

---

### Task 8: Expose versioned HTTP event, memory and retrieval routes

**Files:**
- Create: `crates/vestrace-http/src/error.rs`
- Create: `crates/vestrace-http/src/routes/mod.rs`
- Create: `crates/vestrace-http/src/routes/events.rs`
- Create: `crates/vestrace-http/src/routes/memories.rs`
- Create: `crates/vestrace-http/src/routes/retrieval.rs`
- Modify: `crates/vestrace-http/src/router.rs`
- Create: `tests/http_idempotency.rs`

**Interfaces:**
- Exposes:

```text
POST /v1/events
POST /v1/memories
POST /v1/memories/search
POST /v1/context-packs
GET  /v1/memories/{id}
POST /v1/memories/{id}/revisions
POST /v1/memories/{id}/forget
POST /v1/relations
GET  /v1/timeline
```

- Uses `Idempotency-Key` header for writes and `If-Match` or explicit `expected_revision` consistently; choose one public convention and document it. Use explicit `expected_revision` in JSON to align MCP.

- [ ] **Step 1: Write route contract tests before handlers**

Test status, content type, envelope fields and stable error codes for success, invalid input, not found, revision conflict and idempotency conflict.

- [ ] **Step 2: Implement DTO mapping**

Handlers deserialize DTOs, validate authenticated workspace consistency, authorize, call one application service, map result. No business transitions occur in handlers.

- [ ] **Step 3: Enforce body and response limits**

Configure maximum JSON body size and timeout. Return `invalid_argument` for oversized payloads without reading arbitrary amounts into memory.

- [ ] **Step 4: Run and commit**

```bash
cargo test --test http_contract --test http_idempotency
git add crates/vestrace-http tests
git commit -m "feat(http): expose memory and retrieval API"
```

---

### Task 9: Add operation and administrative HTTP routes

**Files:**
- Create: `crates/vestrace-application/src/operations/mod.rs`
- Create: `crates/vestrace-application/src/operations/ports.rs`
- Create: `crates/vestrace-http/src/routes/operations.rs`
- Create: `crates/vestrace-http/src/routes/admin.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/operation_repository.rs`
- Modify: `crates/vestrace-http/src/router.rs`

**Interfaces:**
- Exposes:

```text
GET  /v1/operations/{id}
POST /v1/operations/{id}/cancel
POST /admin/v1/memories/{id}/purge
GET  /admin/v1/audit
GET  /admin/v1/diagnostics
```

- Long-running actions return `operation_id` and never hold an HTTP request until completion.

- [ ] **Step 1: Define operation state model**

States: `queued`, `running`, `waiting`, `succeeded`, `failed`, `cancelled`. Cancellation is best-effort and must not mark already committed authoritative writes as undone.

- [ ] **Step 2: Write purge authorization route test**

Without `data.purge` and valid approval: `403`. With both: `202` and operation ID.

- [ ] **Step 3: Implement admin route separation**

Use a distinct router layer requiring administrative capabilities. Do not rely on URL prefix alone.

- [ ] **Step 4: Run and commit**

```bash
cargo test -p vestrace-http operations admin
git add crates
git commit -m "feat(http): expose operations and admin endpoints"
```

---

### Task 10: Create the MCP adapter crate and stdio server

**Files:**
- Create: `crates/vestrace-mcp/Cargo.toml`
- Create: `crates/vestrace-mcp/src/lib.rs`
- Create: `crates/vestrace-mcp/src/server.rs`
- Create: `crates/vestrace-mcp/src/context.rs`
- Create: `crates/vestrace-mcp/src/error.rs`
- Create: `crates/vestrace-mcp/src/tools/mod.rs`
- Create: `crates/vestrace-mcp/src/tools/events.rs`
- Create: `crates/vestrace-mcp/src/tools/memory.rs`
- Create: `crates/vestrace-mcp/src/tools/retrieval.rs`
- Create: `crates/vestrace-mcp/src/tools/operations.rs`
- Modify: root `Cargo.toml`
- Modify: `crates/vestrace-cli/src/commands/mcp.rs`
- Create: `tests/mcp_contract.rs`

**Interfaces:**
- Exposes stdio tools:

```text
vestrace_event_record
vestrace_memory_remember
vestrace_memory_search
vestrace_context_build
vestrace_memory_get
vestrace_memory_revise
vestrace_memory_forget
vestrace_memory_link
vestrace_timeline_get
vestrace_operation_get
vestrace_operation_cancel
```

- Logs only to stderr in stdio mode.

- [ ] **Step 1: Write failing tool-list test**

Start the server over in-memory duplex transport and assert the exact approved tool names and JSON input schemas are advertised.

- [ ] **Step 2: Implement MCP context profile**

`vestrace mcp --profile <name>` resolves configured local principal/workspace and capability ceiling. Tool arguments cannot override that identity.

- [ ] **Step 3: Implement thin tool handlers**

Each handler maps MCP input to the same DTO/application service used by HTTP and maps the common envelope to structured MCP content.

- [ ] **Step 4: Protect stdio protocol output**

Initialize tracing writer to stderr; add a test that stdout contains only protocol frames.

- [ ] **Step 5: Run and commit**

```bash
cargo test -p vestrace-mcp --test mcp_contract
git add Cargo.toml Cargo.lock crates/vestrace-mcp crates/vestrace-cli tests/mcp_contract.rs
git commit -m "feat(mcp): expose memory tools over stdio"
```

---

### Task 11: Add MCP resources and Streamable HTTP

**Files:**
- Create: `crates/vestrace-mcp/src/resources.rs`
- Modify: `crates/vestrace-mcp/src/server.rs`
- Modify: `crates/vestrace-http/src/router.rs`
- Create: `tests/http_mcp_equivalence.rs`

**Interfaces:**
- Exposes read-only resources:

```text
vestrace://workspaces/{workspace_id}/memories/{memory_id}
vestrace://workspaces/{workspace_id}/context/current
vestrace://workspaces/{workspace_id}/models
vestrace://workspaces/{workspace_id}/agents/{agent_id}
vestrace://workspaces/{workspace_id}/skills/{skill_id}
vestrace://workspaces/{workspace_id}/workflows/{workflow_id}
vestrace://workspaces/{workspace_id}/executions/{execution_id}
vestrace://workspaces/{workspace_id}/operations/{operation_id}
```

Resources whose backing subsystem arrives in Plan 5 must be registered only in Plan 5; do not return fake empty objects in this plan.

- [ ] **Step 1: Write a memory-resource test**

Read a known memory resource and assert content, revision, provenance and sensitivity-safe fields match the HTTP `GET /v1/memories/{id}` result.

- [ ] **Step 2: Mount Streamable HTTP MCP endpoint**

Mount at `/mcp`, reuse bearer authentication and request context, enforce origin/host configuration and request limits.

- [ ] **Step 3: Write HTTP/MCP equivalence tests**

For record event, memory search, context build and revise conflict, compare normalized common envelopes and domain error codes.

- [ ] **Step 4: Run and commit**

```bash
cargo test --test mcp_contract --test http_mcp_equivalence
git add crates tests
git commit -m "feat(mcp): add resources and Streamable HTTP"
```

---

### Task 12: Generate OpenAPI, MCP schema snapshots and security acceptance tests

**Files:**
- Create: `crates/vestrace-http/src/openapi.rs`
- Create: `schemas/openapi-v1.json`
- Create: `schemas/mcp-tools-v1.json`
- Create: `tests/security_acceptance.rs`
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Produces reproducible schema artifacts checked in CI.
- Adds security-negative suite covering cross-workspace access, self-elevation, provider transfer, purge, logs and deleted-memory cache access.

- [ ] **Step 1: Implement deterministic schema generation commands**

```bash
cargo run -p vestrace-cli -- schema http > schemas/openapi-v1.json
cargo run -p vestrace-cli -- schema mcp > schemas/mcp-tools-v1.json
```

Sort maps and omit runtime timestamps so output is stable.

- [ ] **Step 2: Write security acceptance cases**

Cover:

```text
foreign workspace read denied
workspace body substitution denied
agent self-permission update denied
Restricted remote model transfer denied
hard purge without approval denied
secret absent from logs and persisted event content
deleted memory absent from search and context cache
```

- [ ] **Step 3: Add schema drift check to CI**

Regenerate to a temporary directory and diff against committed files.

- [ ] **Step 4: Run and commit**

```bash
cargo test --test security_acceptance --test http_mcp_equivalence
cargo run -p vestrace-cli -- schema http | diff -u schemas/openapi-v1.json -
cargo run -p vestrace-cli -- schema mcp | diff -u schemas/mcp-tools-v1.json -
git add crates schemas tests .github/workflows/ci.yml
git commit -m "test: lock API schemas and security boundaries"
```

---

## Interfaces and Security completion gate

Run with fresh output:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --workspace --all-features
cargo run -p vestrace-cli -- schema http | diff -u schemas/openapi-v1.json -
cargo run -p vestrace-cli -- schema mcp | diff -u schemas/mcp-tools-v1.json -
```

Then run HTTP and MCP smoke clients against a started server. Confirm:

- the same command produces equivalent semantics through both adapters;
- identity always comes from trusted authentication/profile context;
- authorization is deny-by-default and delegated authority cannot expand;
- RLS still blocks cross-workspace rows when application checks are deliberately bypassed in a test;
- Restricted data is not sent remotely without capability and approval;
- secrets are absent from logs, audit payloads and public DTOs;
- all writes enforce idempotency;
- all revision writes enforce optimistic concurrency;
- MCP stdio writes no logs to stdout;
- admin operations are not exposed as ordinary MCP tools.

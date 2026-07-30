# Vestrace Interfaces and Security Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose the approved Vestrace memory, retrieval and operation capabilities through stable HTTP and MCP adapters while enforcing authentication, scoped authorization, delegated authority, sensitivity controls, redaction, approvals, auditing, idempotency and optimistic concurrency.

**Architecture:** HTTP and MCP are thin adapters over the same application services and DTO mapping layer. Authentication establishes a trusted `RequestContext`; clients cannot choose an unrelated workspace or principal. A policy engine evaluates capabilities and scopes before application calls, while PostgreSQL RLS remains the second isolation layer. Sensitive data is classified and redacted before persistence or external model transmission.

**Tech Stack:** Existing Foundation, Memory Core and Retrieval plans, Rust, Axum, Tower, rmcp, Tokio, Serde, Schemars, SQLx, Argon2id, tracing, tower test utilities.

## Global Constraints

- Complete the first three plans before starting.
- MCP and HTTP call the same application services and preserve the same domain error semantics.
- MCP is agent-facing and narrower than HTTP/admin APIs.
- Client-supplied workspace IDs are consistency assertions, never authority.
- Authorization defaults to deny.
- Delegation cannot increase authority.
- Reading memory and sending memory to a provider are separate capabilities.
- Hard purge, permission changes, external-provider enablement and Restricted remote transfer require approval.
- Secrets never appear in DTOs, MCP results, logs or audit payloads.
- All writes require idempotency; versioned mutations require expected revision.
- Public errors never expose SQL, stack traces or secret values.
- Root-level contract and security tests run through the root `vestrace-integration-tests` package created by Foundation.

---

## Locked file structure additions

```text
crates/vestrace-domain/src/security/
  mod.rs
  capability.rs
  sensitivity.rs
  approval.rs

crates/vestrace-application/src/
  dto/mod.rs
  dto/envelope.rs
  dto/memory.rs
  dto/retrieval.rs
  dto/operations.rs
  security/mod.rs
  security/ports.rs
  security/policy_engine.rs
  security/redaction.rs
  security/approval.rs
  security/audit.rs
  operations/mod.rs
  operations/ports.rs

crates/vestrace-infrastructure/src/postgres/
  security_repository.rs
  redaction_repository.rs
  audit_repository.rs
  operation_repository.rs

crates/vestrace-http/src/
  auth.rs
  error.rs
  middleware.rs
  openapi.rs
  routes/mod.rs
  routes/events.rs
  routes/memories.rs
  routes/retrieval.rs
  routes/operations.rs
  routes/admin.rs

crates/vestrace-mcp/
  Cargo.toml
  src/lib.rs
  src/server.rs
  src/context.rs
  src/error.rs
  src/resources.rs
  src/tools/mod.rs
  src/tools/events.rs
  src/tools/memory.rs
  src/tools/retrieval.rs
  src/tools/operations.rs

crates/vestrace-cli/src/commands/
  mcp.rs
  schema.rs

migrations/
  0012_tokens_policies_and_approvals.sql
  0013_audit_and_redaction.sql

schemas/
  v1/*.json
  openapi-v1.json
  mcp-tools-v1.json

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
  security_acceptance.rs
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

- [ ] **Step 1: Write capability parsing tests**

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

- [ ] **Step 2: Implement approved capabilities**

Include typed memory, event, context, agent, skill, workflow, execution, model, provider, workspace, audit, export and purge variants serialized as stable dotted lowercase names.

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

Policy adapters treat unknown/missing classification as at least `Confidential`.

- [ ] **Step 4: Implement approval lifecycle**

```text
Requested → AwaitingApproval → Approved | Rejected | Expired
```

Approved records bind approver, exact operation hash and expiry.

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
- Test: inline schema tests

**Interfaces:**
- Produces `ApiStatus::{Succeeded, Accepted, Failed}`, `ApiEnvelope<T>`, `ApiWarning`, `ApiProvenance`, `ApiError`, and versioned DTOs.
- DTOs contain no SQLx or provider-client types.

- [ ] **Step 1: Write envelope contract test**

```rust
#[test]
fn success_envelope_uses_stable_fields() {
    let value = serde_json::to_value(ApiEnvelope::success(RequestId::new(), 42)).unwrap();
    assert_eq!(value["status"], "succeeded");
    assert_eq!(value["data"], 42);
    assert_eq!(value["degraded"], false);
}
```

- [ ] **Step 2: Implement envelope types**

```rust
#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApiStatus {
    Succeeded,
    Accepted,
    Failed,
}

pub struct ApiEnvelope<T> {
    pub request_id: RequestId,
    pub status: ApiStatus,
    pub data: Option<T>,
    pub error: Option<ApiError>,
    pub warnings: Vec<ApiWarning>,
    pub provenance: ApiProvenance,
    pub degraded: bool,
}
```

- [ ] **Step 3: Map stable errors**

Support exactly `invalid_argument`, `unauthenticated`, `forbidden`, `not_found`, `revision_conflict`, `idempotency_conflict`, `policy_violation`, `budget_exceeded`, `provider_unavailable`, `rate_limited`, `operation_failed`, and `internal`.

- [ ] **Step 4: Generate DTO schema snapshots**

Store deterministic Schemars output under `schemas/v1/` and add drift tests.

- [ ] **Step 5: Run and commit**

```bash
cargo test -p vestrace-application dto
git add crates/vestrace-application schemas/v1
git commit -m "feat(api): define stable public DTO contracts"
```

---

### Task 3: Add token, policy and approval persistence

**Files:**
- Create: `migrations/0012_tokens_policies_and_approvals.sql`
- Create: `crates/vestrace-application/src/security/ports.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/security_repository.rs`
- Create: `tests/authorization.rs`
- Modify: workspace dependencies for Argon2

**Interfaces:**
- Produces `access_tokens`, `policy_bindings`, `policy_scopes`, `approval_records`.
- Produces `SecurityRepository` and `ApprovalRepository`.
- Stores token hash, prefix, principal, workspace, expiry and revocation metadata only.

- [ ] **Step 1: Write token-storage test**

Persist plaintext `vst_test_secret`; verify raw table contains no plaintext while repository verification succeeds.

- [ ] **Step 2: Create schema**

Use Argon2id encoded hashes, lookup prefix, `revoked_at`, `expires_at`, and `last_used_at`.

- [ ] **Step 3: Define explicit policy scopes**

Support workspace, project, user, agent, workflow, memory kinds, sensitivity ceiling and labels. Do not implement arbitrary policy code.

- [ ] **Step 4: Implement repositories and RLS**

Security management uses privileged repository paths; evaluation reads only current-workspace policy rows.

- [ ] **Step 5: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test authorization
git add Cargo.toml Cargo.lock migrations/0012_tokens_policies_and_approvals.sql crates tests/authorization.rs
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
- Produces `AuthenticatedPrincipal` and Axum `RequestContext` extractor.
- Supports `LocalTrusted` and `BearerToken` modes.
- Workspace/principal derive from credentials.

- [ ] **Step 1: Write unauthenticated and workspace-substitution tests**

Bearer mode without token returns `401 unauthenticated`. Credentials for workspace A plus body workspace B return `403 policy_violation` before repositories are called.

- [ ] **Step 2: Implement bearer extraction**

Malformed, expired and revoked tokens share the external unauthenticated shape. Log token ID/prefix only.

- [ ] **Step 3: Implement local trusted mode**

Permit configured loopback/stdio contexts only; never accept arbitrary identity headers.

- [ ] **Step 4: Run and commit**

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
- Modify: application service command boundaries

**Interfaces:**
- Produces `AuthorizationRequest`, `AuthorizationDecision`, `PolicyEngine::authorize`, and `DelegationContext`.

- [ ] **Step 1: Write deny-by-default and escalation tests**

No policy for `memory.read` returns `Denied(NoMatchingPolicy)`. A user with read only cannot delegate revise even when agent metadata requests it.

- [ ] **Step 2: Implement capability intersection**

```text
initiator grants
∩ actor grants
∩ workflow/node limits
∩ requested operation requirements
```

Explicit deny wins; every constrained scope must match.

- [ ] **Step 3: Guard application services before transactions**

Keep RLS as a second boundary.

- [ ] **Step 4: Run and commit**

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
- Separates `memory.read`, `model.data.share.local`, and `model.data.share.remote`.

- [ ] **Step 1: Write secret-redaction tests**

An input containing `sk-test-1234567890` is replaced before persistence and provider serialization. Plaintext is absent from DB, captured logs and DTOs.

- [ ] **Step 2: Implement deterministic rules**

Support API keys, bearer tokens, private-key blocks, password assignments and workspace regexes. Store original hash, redacted content, rule IDs and policy version.

- [ ] **Step 3: Implement provider transfer check**

```rust
pub fn authorize_transfer(
    classification: Sensitivity,
    destination: DataDestination,
    capabilities: &CapabilitySet,
    redaction: &RedactionResult,
) -> Result<(), PolicyViolation>;
```

Restricted remote transfer needs capability plus approval. Secret values cannot be stored as memory or transferred.

- [ ] **Step 4: Integrate before extraction, embedding and reranking calls**

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
- Produces `RequestApprovalService`, `ResolveApprovalService`, `VerifyApprovalService`, and `AuditWriter`.
- Approval binds operation kind, canonical payload hash, requester and expiry.

- [ ] **Step 1: Write payload-binding test**

Approval for purge of memory A cannot purge memory B.

- [ ] **Step 2: Implement canonical operation hashing**

Hash command type, workspace and canonical typed DTO bytes with SHA-256.

- [ ] **Step 3: Implement append-only audit writes**

Record decision, policy IDs, principal, resource reference, operation and correlation ID without content. Restricted reads always audit.

- [ ] **Step 4: Consume approval atomically with dangerous operation**

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

- Writes use `Idempotency-Key`; versioned writes use JSON `expected_revision` to match MCP.

- [ ] **Step 1: Write route contract tests**

Cover success, invalid input, not found, revision conflict and idempotency conflict.

- [ ] **Step 2: Implement thin handlers**

Deserialize, verify authenticated workspace, authorize, call one application service, map envelope. No domain transitions in handlers.

- [ ] **Step 3: Add body and timeout limits**

Oversized JSON returns `invalid_argument` without unbounded buffering.

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

- Long-running actions return operation ID.

- [ ] **Step 1: Define operation states**

`queued`, `running`, `waiting`, `succeeded`, `failed`, `cancelled`. Cancellation never claims committed writes were undone.

- [ ] **Step 2: Write purge route test**

Without capability and approval: `403`; with both: `202` plus operation ID.

- [ ] **Step 3: Apply separate admin authorization layer**

URL prefix alone is not authorization.

- [ ] **Step 4: Run and commit**

```bash
cargo test -p vestrace-http operations admin
git add crates
git commit -m "feat(http): expose operations and admin endpoints"
```

---

### Task 10: Create MCP adapter crate and stdio server

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

- [ ] **Step 1: Add crate to workspace and root test dev-dependencies**

Add `crates/vestrace-mcp` to workspace members and `vestrace-mcp = { path = "crates/vestrace-mcp" }` to root dev-dependencies.

- [ ] **Step 2: Write tool-list contract test**

Start over in-memory duplex transport and assert exact names and schemas.

- [ ] **Step 3: Implement trusted MCP profile**

`vestrace mcp --profile <name>` resolves local principal/workspace/capability ceiling; arguments cannot override identity.

- [ ] **Step 4: Implement thin handlers and protect stdout**

Use common DTO/services. Send tracing to stderr only; test stdout contains protocol frames only.

- [ ] **Step 5: Run and commit**

```bash
cargo test --test mcp_contract
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
- Exposes backed read-only resources for memories, current context and operations in this plan.
- Model/agent/skill/workflow/execution resources are registered only in Plan 5 when their stores exist.

- [ ] **Step 1: Write memory-resource equivalence test**

MCP memory resource matches normalized HTTP memory response for content, revision and provenance.

- [ ] **Step 2: Mount authenticated Streamable HTTP at `/mcp`**

Reuse bearer context, origin/host restrictions, size limits and timeouts.

- [ ] **Step 3: Test HTTP/MCP equivalence**

Compare record event, search, context build and revision conflict semantics.

- [ ] **Step 4: Run and commit**

```bash
cargo test --test mcp_contract --test http_mcp_equivalence
git add crates tests
git commit -m "feat(mcp): add resources and Streamable HTTP"
```

---

### Task 12: Generate schemas and run security acceptance tests

**Files:**
- Create: `crates/vestrace-http/src/openapi.rs`
- Create: `crates/vestrace-cli/src/commands/schema.rs`
- Modify: `crates/vestrace-cli/src/commands/mod.rs`
- Modify: `crates/vestrace-cli/src/main.rs`
- Create: `schemas/openapi-v1.json`
- Create: `schemas/mcp-tools-v1.json`
- Create: `tests/security_acceptance.rs`
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Produces CLI:

```text
vestrace schema http
vestrace schema mcp
```

- Produces deterministic schema artifacts and security-negative suite.

- [ ] **Step 1: Implement deterministic schema commands**

```bash
cargo run -p vestrace-cli -- schema http > schemas/openapi-v1.json
cargo run -p vestrace-cli -- schema mcp > schemas/mcp-tools-v1.json
```

Sort maps and omit timestamps.

- [ ] **Step 2: Write security acceptance cases**

Cover foreign workspace read, workspace substitution, self-permission update, Restricted remote transfer, purge without approval, secret leakage and deleted-memory cache/search access.

- [ ] **Step 3: Add schema drift CI**

Regenerate to temporary files and diff.

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

Confirm equivalent HTTP/MCP semantics, trusted identity context, deny-by-default authorization, non-expanding delegation, RLS isolation, provider transfer policy, secret-free outputs/logs, write idempotency, optimistic concurrency, clean stdio protocol output and no administrative MCP tools.

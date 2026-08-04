# Vestrace R1.4B — Persisted Command Idempotency Specification v0.1

**Status:** Approved  
**Stage:** R1.4B — Persisted Idempotency  
**Approved:** 2026-08-05  
**Prerequisites:** R1.1–R1.3 and a stable P1 Typed Core  
**Initial scope:** `POST /v1/runs`

## 1. Purpose

A repeated `CreateRun` request with the same `Idempotency-Key` must not create another run, append another `run.created` event, or mutate the projection a second time.

The guarantee must survive process restarts, response loss, retries routed to another Vestrace instance, and concurrent duplicate requests. PostgreSQL is the source of this guarantee; process-local memory and Redis are not required.

## 2. Required semantics

### 2.1 First request

```http
POST /v1/runs
Idempotency-Key: create-retention-run-01
Content-Type: application/json

{"title":"Verify retention policy"}
```

Result:

```http
201 Created
Location: /v1/runs/{run_id}
```

One transaction creates:

- the command receipt;
- the canonical `run.created` event;
- the `agent_runs` read projection.

### 2.2 Exact replay

A request with the same workspace, principal, operation, idempotency key, and semantically identical command returns the original command result:

```http
200 OK
Location: /v1/runs/{original_run_id}
Idempotency-Replayed: true
```

The retry must not change run, event, projection, or receipt counts.

### 2.3 Same key, different command

A request with the same scope and key but a different command fingerprint returns:

```http
409 Conflict
```

Machine-readable error code:

```text
idempotency_conflict
```

No new event, projection, or receipt is created.

### 2.4 Request without a key

When `Idempotency-Key` is absent, normal command semantics remain. Two equal requests without a key may create two distinct runs.

### 2.5 Key scope

A key is unique inside:

```text
workspace_id
+ principal_id
+ operation
+ idempotency_key_hash
```

The same raw key may be used by another principal, in another workspace, or for another operation.

## 3. R1.4B boundaries

### Included

- parsing and validation of `Idempotency-Key`;
- SHA-256 key hashing;
- versioned command fingerprinting;
- persisted command receipts;
- atomic receipt/event/projection commit;
- exact replay of the original result snapshot;
- payload conflict detection;
- concurrent duplicate coordination;
- RLS and runtime permissions;
- HTTP `201`, `200`, and `409` semantics;
- database and Compose acceptance tests.

### Excluded

- receipt retention and automatic deletion;
- idempotency for reads;
- tool-execution idempotency;
- Redis or distributed cache;
- arbitrary replay of failed commands;
- idempotency for every future run command in this first slice.

The architecture must be extensible to later commands.

## 4. Idempotency-Key contract

The key must:

- appear at most once;
- contain 1–255 bytes;
- contain no control characters;
- be a valid HTTP header value;
- be case-sensitive;
- be treated as an opaque string.

An invalid key returns:

```http
400 Bad Request
```

with code `invalid_idempotency_key`.

The raw key must not be persisted, logged, returned by the API, or used as a metric label. Persist only:

```text
SHA-256(exact_header_value)
```

The database column is `BYTEA` constrained to exactly 32 bytes.

## 5. Command fingerprint

The key does not prove that two requests contain the same command. A separate request fingerprint is required:

```text
request_hash = SHA-256(canonical_command_representation)
```

### 5.1 CreateRun fingerprint v1

The canonical representation contains:

```text
namespace: vestrace.run-command
fingerprint_version: 1
operation: create_run
command_schema_version: 1
expected_version: 0
title: domain-normalized title
```

Logical byte representation:

```text
vestrace.run-command\0
1\0
create_run\0
1\0
0\0
<normalized-title>
```

The fingerprint must use the same normalized title that is written into `run.created`. It must not hash raw JSON because JSON whitespace, field order, and transport-only fields are not command semantics.

### 5.2 Versioning

Each receipt stores:

- operation;
- command schema version;
- fingerprint version;
- request hash.

Changing the algorithm requires a new fingerprint version. Old receipts remain readable under their original version.

## 6. SQL schema

Use the next available migration number. Recommended name when applicable:

```text
0112_run_command_receipts.sql
```

```sql
CREATE TABLE run_command_receipts (
    receipt_id UUID PRIMARY KEY,

    workspace_id UUID NOT NULL,
    principal_id UUID NOT NULL,

    operation TEXT NOT NULL,
    command_schema_version INTEGER NOT NULL,
    fingerprint_version INTEGER NOT NULL,

    idempotency_key_hash BYTEA NOT NULL,
    request_hash BYTEA NOT NULL,

    original_command_id UUID NOT NULL,
    original_correlation_id UUID NOT NULL,

    run_id UUID NOT NULL,
    resulting_version BIGINT,

    result_schema_version INTEGER,
    result JSONB,

    created_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,

    CONSTRAINT run_command_receipts_operation_nonempty
        CHECK (length(trim(operation)) > 0),

    CONSTRAINT run_command_receipts_command_version_positive
        CHECK (command_schema_version > 0),

    CONSTRAINT run_command_receipts_fingerprint_version_positive
        CHECK (fingerprint_version > 0),

    CONSTRAINT run_command_receipts_key_hash_length
        CHECK (octet_length(idempotency_key_hash) = 32),

    CONSTRAINT run_command_receipts_request_hash_length
        CHECK (octet_length(request_hash) = 32),

    CONSTRAINT run_command_receipts_resulting_version_positive
        CHECK (resulting_version IS NULL OR resulting_version > 0),

    CONSTRAINT run_command_receipts_completion_consistent
        CHECK (
            (
                completed_at IS NULL
                AND resulting_version IS NULL
                AND result_schema_version IS NULL
                AND result IS NULL
            )
            OR
            (
                completed_at IS NOT NULL
                AND resulting_version IS NOT NULL
                AND result_schema_version IS NOT NULL
                AND result IS NOT NULL
            )
        ),

    UNIQUE (
        workspace_id,
        principal_id,
        operation,
        idempotency_key_hash
    )
);
```

The receipt is claimed before the new projection row is inserted, so the run foreign key must be deferred:

```sql
ALTER TABLE run_command_receipts
ADD CONSTRAINT run_command_receipts_run_fk
FOREIGN KEY (workspace_id, run_id)
REFERENCES agent_runs (workspace_id, id)
DEFERRABLE INITIALLY DEFERRED;
```

Required indexes:

```sql
CREATE INDEX run_command_receipts_run_idx
ON run_command_receipts (workspace_id, run_id);

CREATE INDEX run_command_receipts_created_idx
ON run_command_receipts (workspace_id, created_at);
```

## 7. Original result snapshot

A receipt stores the original command result rather than reading the latest projection during replay.

Example:

```text
CreateRun → version 1
later transitions → version 5
retry original CreateRun → return original version 1 snapshot
```

Result schema v1:

```rust
pub struct CreateRunReceiptResultV1 {
    pub run: AgentRun,
}
```

Stored values:

```text
result_schema_version = 1
resulting_version = 1
result = serialized CreateRunReceiptResultV1
```

A completed result snapshot is immutable.

## 8. Application contracts

```rust
pub struct IdempotencyKey {
    hash: [u8; 32],
}
```

The raw header value exists only at the HTTP boundary and is converted immediately.

```rust
pub struct RunCommandFingerprint {
    pub operation: &'static str,
    pub command_schema_version: u16,
    pub fingerprint_version: u16,
    pub request_hash: [u8; 32],
}
```

```rust
pub struct RunCommandExecutionRequest {
    pub envelope: RunCommandEnvelope,
    pub idempotency_key: Option<IdempotencyKey>,
}
```

```rust
pub struct RunCommandExecutionResult {
    pub run: AgentRun,
    pub replayed: bool,
}
```

```rust
#[async_trait]
pub trait RunCommandExecutor: Send + Sync {
    async fn execute(
        &self,
        context: &RequestContext,
        request: RunCommandExecutionRequest,
    ) -> Result<RunCommandExecutionResult, ApplicationError>;
}
```

The atomic commit contract must carry optional idempotency data and return either `Committed` or `Replayed`.

## 9. Transaction algorithm

### 9.1 Without a key

Keep the R1.3 algorithm:

```text
load/replay
→ decide
→ build events
→ validate projection
→ append events
→ update projection
→ commit
```

### 9.2 With a key

```text
BEGIN scoped transaction

1. Claim the receipt:
   INSERT ... ON CONFLICT DO NOTHING RETURNING receipt_id

2A. Insert succeeded:
    validate event batch
    create event
    create projection
    finalize receipt
    COMMIT
    return Committed

2B. Insert did not succeed:
    SELECT existing receipt by full scope
    compare operation and schema versions
    compare request_hash

    mismatch:
        ROLLBACK
        return IdempotencyConflict

    matching completed receipt:
        deserialize stored result
        COMMIT
        return Replayed

    matching incomplete receipt:
        return ReceiptCorrupted
```

### 9.3 Concurrent duplicates

The unique index coordinates concurrent requests:

```text
A inserts receipt
B waits on the unique index

A commits:
    B observes conflict, loads completed receipt, returns replay

A rolls back:
    B insert succeeds and becomes owner
```

No process-local mutex, Redis lock, advisory lock, or serializable isolation is required.

## 10. Receipt finalization and immutability

Finalize before commit:

```sql
UPDATE run_command_receipts
SET
    resulting_version = $resulting_version,
    result_schema_version = $result_schema_version,
    result = $result,
    completed_at = $completed_at
WHERE receipt_id = $receipt_id
  AND workspace_id = $workspace_id
  AND completed_at IS NULL;
```

Exactly one row must be updated. Otherwise the whole transaction fails.

A database guard must allow only:

```text
pending → completed
```

After completion it must reject changes to scope, hashes, operation/version metadata, original command metadata, run identity, and result. Runtime roles receive no `DELETE` permission.

## 11. RLS

```sql
ALTER TABLE run_command_receipts ENABLE ROW LEVEL SECURITY;
ALTER TABLE run_command_receipts FORCE ROW LEVEL SECURITY;
```

Workspace isolation policy:

```sql
CREATE POLICY run_command_receipts_workspace_isolation
ON run_command_receipts
FOR ALL
USING (workspace_id = vestrace_current_workspace_id())
WITH CHECK (workspace_id = vestrace_current_workspace_id());
```

Every application query must still explicitly filter by workspace, principal, operation, and key hash. RLS is defense in depth, not a replacement for explicit predicates.

## 12. HTTP contract

### First execution

```http
201 Created
Location: /v1/runs/{run_id}
```

`Idempotency-Replayed` may be omitted for the first request.

### Replay

```http
200 OK
Location: /v1/runs/{original_run_id}
Idempotency-Replayed: true
```

### Conflict

```http
409 Conflict
```

```json
{
  "error": {
    "code": "idempotency_conflict",
    "message": "The idempotency key was already used for a different request."
  }
}
```

The response must not disclose the original payload, title, request hash, or original command ID.

### Corrupt receipt

```http
500 Internal Server Error
```

with code `idempotency_receipt_corrupt` and a high-severity diagnostic log.

## 13. Request and correlation identity

`Idempotency-Key` and `x-request-id` have different meanings.

The first request stores:

```text
original_command_id = x-request-id
original_correlation_id = x-correlation-id
```

A replay may use new request and correlation IDs for tracing the retry, but it must not replace the original receipt metadata or create a new domain event.

## 14. Failure semantics

- Domain validation failure creates no receipt, event, or projection.
- Any SQL failure after receipt claim rolls back receipt, event, and projection together.
- A client retry after response loss returns the committed stored result.
- A rolled-back key remains reusable.

## 15. Typed errors

Add stable typed errors rather than parsing error strings:

```rust
pub enum IdempotencyError {
    InvalidKey,
    Conflict { operation: String },
    ReceiptIncomplete,
    UnsupportedFingerprintVersion { version: u16 },
    UnsupportedResultVersion { version: u16 },
    ReceiptCorrupted,
}
```

## 16. Observability

Required counters:

```text
run_idempotency_requests_total
run_idempotency_commits_total
run_idempotency_replays_total
run_idempotency_conflicts_total
run_idempotency_invalid_keys_total
run_idempotency_receipt_errors_total
run_idempotency_wait_seconds
```

Only low-cardinality labels such as operation and outcome are allowed. Keys, hashes, workspace IDs, principal IDs, and run IDs must not be metric labels.

Structured logs may include workspace, principal, operation, run ID, current request command ID, original command ID, replay flag, and outcome. Do not log the raw key.

## 17. Required tests

### Fingerprint unit tests

- equal normalized titles produce equal hashes;
- different titles produce different hashes;
- operation and command-version changes alter the hash;
- JSON formatting does not affect the hash;
- fingerprinting is deterministic.

### PostgreSQL integration tests

- first commit creates one receipt, event, and projection;
- exact sequential replay returns the same result and creates no rows;
- same key with a different title returns conflict;
- the same key is allowed in another workspace or for another principal;
- concurrent duplicates produce one commit and one replay;
- SQL failure after claim leaves no receipt, event, or projection;
- replay after later run transitions returns the original version-1 snapshot;
- workspace B cannot observe or replay workspace A receipts.

### HTTP tests

- first keyed request returns `201`;
- replay returns `200` and `Idempotency-Replayed: true`;
- replay returns the original run ID and `Location`;
- conflict returns `409`;
- invalid key returns `400`;
- unkeyed requests preserve normal `201` behavior;
- a new `x-request-id` does not prevent replay.

### Compose acceptance

Execute first request, exact replay, and conflicting replay. Verify:

```text
first status = 201
replay status = 200
conflict status = 409
agent_runs count = 1
run.created count = 1
receipt count = 1
```

## 18. Implementation slices

1. **R1.4B.1 — Fingerprint kernel**
   - key validation and hashing;
   - versioned fingerprint;
   - deterministic unit tests.
2. **R1.4B.2 — Receipt schema**
   - migration, constraints, indexes, deferred FK, RLS, permissions.
3. **R1.4B.3 — Commit contract**
   - optional idempotency data;
   - committed/replayed outcome;
   - typed errors.
4. **R1.4B.4 — PostgreSQL atomic flow**
   - claim, replay, conflict, finalization, rollback, concurrency.
5. **R1.4B.5 — HTTP integration**
   - header parsing, `201/200/409`, replay and location headers.
6. **R1.4B.6 — Acceptance and documentation**
   - Compose, RLS, API documentation, final verification.

## 19. Definition of Done

R1.4B is complete only when:

1. receipts are persisted in PostgreSQL;
2. receipt, event, and projection commit atomically;
3. exact replay creates no second run or event;
4. replay returns the original immutable result snapshot;
5. a different command under the same key returns typed conflict;
6. concurrent duplicates create only one run;
7. rollback leaves no receipt;
8. keys are scoped by workspace, principal, and operation;
9. raw keys are neither stored nor logged;
10. RLS hides receipts across workspaces;
11. completed receipts are immutable;
12. first execution returns `201`, replay `200`, conflict `409`;
13. unkeyed requests preserve prior semantics;
14. fingerprints are versioned and deterministic;
15. Compose verifies database row counts;
16. formatting, Clippy, Rust/PostgreSQL tests, console, and Compose are green.

## 20. Priority placement

This specification is approved but deferred into **P1 supporting infrastructure**. It must not displace the higher-priority P1 Typed Core work: checkpoints, replay, rebuild, pause/resume, and retry semantics.

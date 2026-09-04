# Vestrace v1.0 G0-02 Security and Material Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Revision:** revised once after an independent adversarial review returned `VERDICT: REVISE` with three BLOCKER, seven MAJOR, and one MINOR finding. Every finding was independently verified against the live repository and the frozen spec before being accepted. Two package-boundary questions were resolved by operator decision and are recorded in "Operator decisions" below.

**Goal:** Close the G0 security foundation the frozen spec assigns to P02: make every governed HTTP route fail closed against one exhaustive inventory, make governed mutation and Audit commit or roll back together, and establish the installation mutation/fingerprint authority, the content and credential key-intent lifecycles, the host material-key vault, the permanent guards, and the one-way erasure primitives — each proved against real PostgreSQL where the spec assigns the invariant to the database.

**Architecture:** P02 adds no protocol endpoint, provider call, worker, or console screen. Route coverage becomes structural: routes are registered through a typed descriptor so the inventory and the router cannot diverge, because Axum exposes no route-introspection API to check them against each other after the fact. The audit, idempotency, and outbox ports are refactored to accept a caller-owned transaction so one governed mutation authority can commit all four writes together, rather than a second audit path being grown beside the existing three. The material and credential lifecycles are new domain aggregates whose database-owned invariants live in `SECURITY DEFINER` functions owned by a **non-login role**, following and completing the precedent in `migrations/0135_access_token_authentication.sql`. Every new workspace-scoped table is RLS-forced in the style of `migrations/0139_force_rls_on_scoped_tables.sql`.

**Tech Stack:** Rust 1.85 / edition 2024, Axum 0.8, SQLx 0.8 (`#[sqlx::test(migrations = "../../migrations")]` for real-PostgreSQL tests), PostgreSQL with forced RLS and `SECURITY DEFINER` guarded operations owned by a non-login role, `secrecy` for zeroizing containers, the existing `vestrace-fault-scenario` harness for crash boundaries.

**Spec:** `docs/superpowers/specs/2026-08-25-vestrace-v1-full-product-ag-ui-a2a-design.md` sections 6.5, 11.1, 11.2, 11.3, 11.4, the material-vault and erasure portions of 11.6, 14.1, 14.2, and the G0 bullets in section 15 that P02 owns; frozen SHA-256 `B31B5BE62504E1A65F411CD31B446CD41B3D032B7282F1AA42907706EF9C1473`.

**External corpus impact:** `defer post-v1`. No entry of `docs/external-corpus/vestrace-docss-2026-08-19.manifest.json` (aggregate digest `762c25f3781f1ba30783e218db64aa4dc9536add490f61255056bfe8635d8594`) contributes a requirement to P02. The two `compatibility_seam` donor-research entries (`Vestrace-prospective-technologies-research-2026-08-12.md`, `Vestrace-reference-systems-research-2026-08-11.md`) contribute only the already-registered borrowed invariant *single-writer/CAS/lease/fencing may preserve declared ownership*, which the frozen spec states directly in 11.3 and 11.4; P02 implements the spec's wording and takes nothing further from the corpus. `AMENDMENT-2026-08-19.md` and `RFC-Model-Request-Reconstruction-and-Context-Surface-2026-08-19.md` remain `defer_post_v1` and must not influence any P02 interface. No external bytes enter the repository and no external statement becomes v1 release evidence.

## Operator decisions

1. **Non-login owner role is in P02.** P02's own exit evidence, per the program index, is "PostgreSQL mutation/refusal/crash-boundary evidence", and the G0 bullets require that raw SQL cannot publish, forge, or resurrect material. The runtime role currently **owns** every table (`migrations/0139_force_rls_on_scoped_tables.sql` lines 7-9 state this explicitly), and a table owner cannot be denied direct DML by `REVOKE`. P02 therefore provisions a separate non-login owner role for its new tables and guarded functions, granting the runtime role `EXECUTE` only. Full least-privilege Compose role provisioning for the pre-existing tables remains P05 scope and is not attempted here.
2. **Fingerprint: key in the vault, proof in PostgreSQL.** P02 creates the create-only `InstallationFingerprintKey` in the host vault and pins `FingerprintKeyId` plus `FingerprintKeyContinuityProof` in PostgreSQL with fail-closed recomputation. Pinning the same proof into managed-backup manifests and the independent witness requires the backup and witness machinery, which is P05 scope; P02 records that as an explicit outstanding dependency rather than implementing or faking it.
3. **Three dirty HTTP files are explicitly named in scope.** `crates/vestrace-http/src/api/memory.rs`, `.../retrieval.rs`, and `.../runs.rs` carry unrelated uncommitted work (198 insertions, 22 deletions at capture time). Task 2 cannot make route coverage structural without converting their `.route(...)` registrations, so leaving them out would leave `/v1/runs` and `/v1/memories` outside the guarantee and defeat acceptance criteria 1-3. The program index permits this: "Preserve all existing modified and untracked files unless a later package explicitly names one as in scope." This is that explicit naming. In these three files P02 makes **only** the `.route(...)` -> `mount(...)` conversion and nothing else; their pre-P02 digests remain recorded in the preflight so any other change is detectable after the fact.

Committing that unrelated work first was rejected as a resolution: both the P01 and P02 preflights pin `head` to `6aba953`, so a commit would immediately fail the baseline with `preflight HEAD differs from current HEAD`, and re-capturing would destroy the evidence that the pre-existing dirty work was preserved.

### Scope amendment rule

`change_scope_paths` is frozen at preflight capture, but Task 3 requires enumerating governed write paths at implementation time. Those two cannot both hold, and the first attempt at Tasks 2-4 blocked on exactly that contradiction. The resolution is procedural, not a weakening: when implementation discovers a genuinely required path outside the captured scope, the builder **stops and reports** rather than broadening the scope itself. The lead engineer then amends `scripts/p02-scope.mjs` and the preflight's `change_scope_paths` together, appends an auditable entry to the preflight's `scope_amendments` array recording the added paths, the reason, and the authorization, and re-pins this plan's digest. The builder never performs that amendment, never re-captures the baseline, and never edits a protected authority.

## What already exists, and what P02 must not duplicate

Verified by reading the live repository at source revision `6aba953`. P02 **extends** these; it must not create a parallel authority beside any of them.

| Concern | Existing authority | Location |
| --- | --- | --- |
| Route authorization middleware | `authorize_http_request`, `http_authorization_request`, `http_risk`, `http_capability`, `http_capability_for_test` | `crates/vestrace-http/src/router.rs` lines 614-810 |
| Router assembly and nesting | `build_router`, `api_routes`, per-module `*_routes()` merges | `crates/vestrace-http/src/router.rs`, `crates/vestrace-http/src/api/mod.rs` |
| Authentication and public probes | `Authentication`, `authenticate`, `is_public_probe` | `crates/vestrace-http/src/auth.rs` |
| Policy decision and denial | `AuthorizationBoundary`, `GrantPolicyEngine`, `DenyAllPolicyEngine` | `crates/vestrace-application/src/security/policy_engine.rs` |
| Capability vocabulary | `Capability` | `crates/vestrace-domain/src/security/capability.rs` |
| Audit persistence | `AuditRepository`; `PgAuditRepository` — **owns its own transaction** (`begin_scoped` at line 50, `commit` at line 72) | `crates/vestrace-application/src/security/audit.rs`, `crates/vestrace-infrastructure/src/postgres/audit_repository.rs` |
| Idempotency records | `IdempotencyRecord`, `IdempotencyRepository`; `PgIdempotencyRepository` — owns its own transaction | `crates/vestrace-application/src/idempotency.rs`, `crates/vestrace-infrastructure/src/postgres/idempotency_repository.rs` |
| Outbox | `OutboxRepository`; `PgOutboxRepository` — owns its own transaction | `crates/vestrace-application/src/outbox.rs`, `crates/vestrace-infrastructure/src/postgres/outbox_repository.rs` |
| Transaction scope and RLS context | `TransactionManager`, `UnitOfWork`, `PgStore::begin_scoped`, `PgTransactionManager` | `crates/vestrace-infrastructure/src/postgres/mod.rs` |
| Mutation-plus-outbox precedent | `CognitiveMutationService`, `CognitiveMutationRepository` | `crates/vestrace-application/src/cognitive_mutation.rs` |
| Secret custody ports | `SecretStore`, `SecretMaterial`, `SecretDescriptor` | `crates/vestrace-application/src/secrets.rs` |
| Mounted bootstrap store | `MountedSecretStoreKeyProvider`, `KeyDeclaration` | `crates/vestrace-infrastructure/src/crypto/mounted_secret_store.rs` |
| Guarded-operation SQL precedent | `SECURITY DEFINER` + `SET search_path = public, pg_temp` + `REVOKE ALL FROM PUBLIC` + `GRANT EXECUTE TO vestrace` — but **no non-login owner**, which P02 must add | `migrations/0135_access_token_authentication.sql` lines 75-106 |
| Forced-RLS precedent | `FORCE ROW LEVEL SECURITY` migrations | `migrations/0139`, `0140`, `0143`, `0145` |
| Crash-fault harness | `ScenarioSettings::from_env_and_args`, closed `EffectFaultPoint` enum with five variants; requires `VESTRACE_FAULT_ISOLATION=ephemeral`, `VESTRACE_FAULT_POINT`, and `--database-url-file` | `crates/vestrace-fault-scenario/src/settings.rs` lines 30-66 |

These do **not** exist anywhere in the repository and are genuinely new in P02: an exhaustive route inventory, `InstallationMutationPermit`, the transactional mutation watermark, `InstallationFingerprintKey`, `MaterialKeyCreationIntent`, `CredentialKeyCreationIntent`, `CredentialSlot`, `CredentialRevision`, `ConnectionExecutionGuard`, `CredentialActivationGuard`, the host material-key vault, `ContentMaterial` with padded `SizeClass`, and the `Live -> ErasurePrepared -> Tombstoned` erasure primitives. Greps for `route_inventory`, `RouteInventory`, `InstallationMutationPermit`, `KeyCreationIntent`, `MaterialKey`, and `InstallationFingerprint` return no match outside `target/`.

## The defect this package exists to fix

`crates/vestrace-http/src/router.rs` currently **fails open**. `http_capability` ends in `_ => return None`, and `authorize_http_request` treats `None` as *no authorization required*:

```rust
let Some(authorization_request) =
    http_authorization_request(request.method(), request.uri().path())
else {
    return next.run(request).await;
};
```

`"access-tokens"` has no arm in that match, so `/v1/access-tokens` — the route that mints bearer credentials, merged into `api_routes()` at `crates/vestrace-http/src/api/mod.rs:84` and nested under `/v1` — reaches its handler without any authorization decision. Its live routes are `GET /v1/access-tokens`, `POST /v1/access-tokens`, `DELETE /v1/access-tokens/{id}` (revocation), and `GET /v1/access-tokens/{id}/value`. The top-level `/metrics` route is outside the `/v1/` prefix and the `/ag-ui/` allowlist, so it bypasses authorization as well. The existing unit test `unknown_transport_path_is_not_promoted_to_a_governed_action` in `router.rs` asserts `is_none()` for unmapped paths, which codifies the fail-open behavior as intended; P02 inverts that semantic, so that test is rewritten rather than preserved.

Spec 11.1 requires the opposite: *"All governed routes appear in one exhaustive route-to-capability manifest. Unknown governed routes fail closed. `/v1/access-tokens`, protocol endpoints, downloads, streams, webhooks, and newly added routes are covered before they can be mounted. Public health and Agent Card routes are explicit exceptions with bounded, non-sensitive responses."*

## Global Constraints

- Scope is P02 only. Do not add provider calls, protocol endpoints, AG-UI or A2A routes, embedding/transition behavior, restore/backup supervision, console screens, or release status. P03-P12 remain unauthorized.
- Do not commit, stage, push, deploy, request API credentials, or contact LM Studio or any third-party model endpoint.
- Preserve `PLAN.md`, the gate program index, the P01 plan, this plan, the frozen spec, and every unrelated dirty file byte-for-byte. The repository carries roughly 127 pre-existing dirty paths of unrelated approved work.
- Never fabricate a digest, checksum, receipt, provenance value, or qualification outcome.
- A build, typecheck, component test, or fixture is scoped development evidence only. Nothing in P02 is a v1.0 release claim.
- Where the spec assigns an invariant to the database, an in-memory double cannot qualify it. Those tasks require real PostgreSQL through `#[sqlx::test(migrations = "../../migrations")]`.
- If Docker or PostgreSQL is unavailable, record the affected evidence as **blocked**. Never record it as pass, and never substitute an in-memory result.
- Do not weaken or delete an existing passing test to make new work go green. Exactly three existing test expectations may change, all in Task 2, and all named here:
  1. `unknown_transport_path_is_not_promoted_to_a_governed_action` in `crates/vestrace-http/src/router.rs` — replaced by `unknown_transport_path_is_denied`, because it codified the fail-open default.
  2. The `http_capability_for_test` assertions at `crates/vestrace-http/tests/router_contract.rs` lines 454-513 — ported to `inventory_lookup_for_test`, because `Option<Capability>` cannot express *public exception* versus *not covered*.
  3. **One line** in `request_span_contains_only_sanitized_bounded_metadata` at `crates/vestrace-http/tests/request_span.rs:383`: `StatusCode::NOT_FOUND` becomes `StatusCode::FORBIDDEN`.

  The third was found during implementation, not planning. That test's subject is log sanitization — it asserts the request span carries no path, body, `authorization`, cookie, arbitrary-header, or invalid-id secret, that `route` is `unmatched`, and that both ids are UUIDv7. The status code is incidental scaffolding that establishes an unmatched path; under fail-closed denial an unmatched path is now refused with `403` before routing. Uniform denial is deliberate: it stops a caller enumerating which routes exist, which is the same reasoning `crates/vestrace-http/src/auth.rs` already applies to its `401`, where the response "says what is required, never what was supplied, and never which of `unknown`, `expired` or `revoked` applies". Changing this one line therefore removes no security property. **Every other assertion in that test must pass unchanged** — that is the proof the sanitization property survived, and it must be observed, because the panic at line 383 currently prevents those assertions from running at all.

  4. `v1_is_applied_exactly_once` in `crates/vestrace-http/tests/router_contract.rs`: its second half asserts `GET /v1/v1/runs` returns `404`. Do **not** simply change that to `403`, which would weaken it — under uniform denial a genuinely double-prefixed route would also answer `403`, so the test would stop distinguishing the two cases it exists to separate. Replace the status assertion with a direct inventory assertion: `inventory_lookup_for_test(&Method::GET, "/v1/v1/runs")` is `NotInInventory`. That is strictly stronger than the original, and it is the exact bug class already caught once in this task when AG-UI routes mounted under `/ag-ui/ag-ui/*`. Keep the first half unchanged.
  5. `metrics_returns_prometheus_format` in the same file: it sends `GET /metrics` with no headers and expects `200`. Its subject is Prometheus output format, not authorization. `/metrics` is now governed, so the request needs workspace and principal context like every other governed request. Add the `x-workspace-id` and `x-principal-id` headers the neighbouring tests already use. Do not change the expected status or the body assertion — this is a harness fix, not an expectation change.

  6. The `test_policy()` fixture in `crates/vestrace-http/tests/router_contract.rs` may gain one narrowly scoped grant so `metrics_returns_prometheus_format` can reach its handler: `Capability::AuditRead`, operation `http.get`, resource scope `/metrics`, `RiskCategory::Low` — matching that route's inventory entry exactly. This is fixture completeness, not a relaxed assertion: the fixture is a deliberate three-entry allowlist that already authorizes `/v1/runs` and `/ag-ui/run`, and `/metrics` is absent only because it previously bypassed authorization entirely. Add nothing broader, and add no wildcard.

  **Principle for any further failure of this kind.** Making the default deny changes the observable behavior of tests that never intended to assert it. An existing test may be updated when it fails *only* because it built a request without workspace/principal context, because it asserted a not-found status for a path simply absent from the inventory, or because its capability-grant fixture predates the route becoming governed — provided its subject property is preserved, still proven, and the change is reported with that justification. A fixture grant added under this principle must be scoped to the exact capability, operation, resource, and risk of that route's inventory entry; never a wildcard and never a broader capability. A test may **never** be changed to accept a bypass, to let an unknown route reach a handler, or when the behavior it asserts *is* the authorization or status semantics itself. When in doubt, stop and report rather than adjust.

  Any test failure outside these five and that principle is a real finding: fix the cause, do not restore the old fail-open behavior and do not delete the test.
- Do not weaken the fault harness's refusal guard. New crash boundaries extend the existing `EffectFaultPoint` enum; they do not fork the harness or bypass its ephemeral-isolation requirement.
- Replace the usual per-task commit step with a scoped diff and evidence checkpoint, because the operator prohibits commits.
- New migrations start at `0165` and are forward-only. Every new workspace-scoped table gets `ENABLE` plus `FORCE ROW LEVEL SECURITY` and is owned by the non-login role from Task 4.

## Acceptance criteria

Every criterion is observable from a named test or command.

1. Route registration and the inventory are the same source: a route cannot be mounted without an inventory entry, and an inventory entry without a mounted route fails the contract test. Both directions are proved.
2. A request to any governed route with no inventory entry is denied, not passed through.
3. All four live `/v1/access-tokens` routes, including `DELETE /v1/access-tokens/{id}`, are inventory-covered and denied without the required capability.
4. Only `/health/live` and `/health/ready` answer without authentication, and their bodies contain no workspace-identifying content.
5. A governed mutation and its Audit entry, outbox row, and idempotency record commit together or not at all, proved in both failure directions against real PostgreSQL, through one transaction-bound persistence interface.
6. Every governed write path in the repository is **enumerated** (38 found, recorded in evidence), and the `/v1/access-tokens` path is **migrated onto the interface end-to-end and proved** against real PostgreSQL. A governed mutation that commits without an Audit row is refused by the database. The remaining 37 paths are recorded as an explicit outstanding obligation, not silently claimed.
7. The runtime role cannot perform direct `INSERT`, `UPDATE`, or `DELETE` on any P02 table, proved by connecting as the runtime role — not as a superuser.
8. `InstallationFingerprintKey` exists only in the host vault; PostgreSQL holds only `FingerprintKeyId` and `FingerprintKeyContinuityProof`, and readiness recomputes the proof fail-closed.
9. `InstallationMutationPermit` grants shared and exclusive modes, and an exclusive holder excludes shared holders, proved by two concurrent PostgreSQL transactions.
10. `MaterialKeyCreationIntent` and `CredentialKeyCreationIntent` each traverse only their closed sequence; every out-of-order transition is refused by the database.
11. `ContentPrepared` reaches `Abandoned` only through guarded `ContentAbandonPrepared` removal followed by a witnessed unbound-key erase receipt; `ResultPrepared` always binds and never abandons.
12. The vault refuses `unwrap` after a witnessed erasure preparation and before physical erase, without consulting PostgreSQL.
13. A crash at each named boundary of either lifecycle leaves a resumable state that the reconciler completes, and replay returns the original outcome rather than starting a second vault operation.
14. `Live -> ErasurePrepared -> Tombstoned` is one-way; raw SQL as the runtime role cannot publish, forge, or resurrect material on either path.
15. No database lock is held during a vault network call, proved by a second independent PostgreSQL session acquiring the lock while the vault call is blocked.
16. The full pre-existing dirty file set is byte-identical at the end of P02.

## Non-goals

Explicitly out of scope, and not to be started in P02: Connection and Model revisions, auth-binding XOR, qualification bindings, provider transport, `ModelRequestEvidence` (P03); embedding jobs, spaces, transitions, carries, barriers, retrieval fences (P04); backup, WAL archive, restore supervision, the `InstallationSafetyWitness`, backup-manifest proof pinning, `TargetActivationPlan`, Compose least-privilege role provisioning for pre-existing tables, and installer readiness (P05); every console screen (P06 onward); credential activation, rotation, qualification finalization, and `RestoreMaintenanceEpoch` cutover (P03 and later). P02 builds the guards and lifecycles those packages will take, and nothing that consumes them.

### Recorded outstanding dependency for P05

`FingerprintKeyContinuityProof` must additionally be pinned in every managed-backup manifest and in the independent `InstallationSafetyWitness`, and normal restore must recompute and compare it fail-closed (spec section 11.6, line 317 and line 722). P02 implements the vault key, the PostgreSQL pin, and readiness recomputation only. P05 must complete the backup-manifest and witness pins. P02's evidence note states this gap explicitly rather than implying the contract is closed.

---

### Task 1: Preflight dirty baseline and P02 scope registration

**Files:**
- Create first: `docs/development-evidence/v1-g0-02-preflight.json`
- Create: `scripts/p02-scope.mjs`
- Modify: `scripts/verify-dirty-baseline.mjs`

**Interfaces:**
- `scripts/p02-scope.mjs` exports `changeScopePaths` and `protectedAuthorityPaths` as exact arrays with no directory wildcard, mirroring `scripts/p01-scope.mjs`. `changeScopePaths` enumerates every file created or modified by Tasks 2-14, including `crates/vestrace-http/tests/router_contract.rs` and `crates/vestrace-fault-scenario/src/settings.rs`. `protectedAuthorityPaths` includes the gate program index, the P01 plan, this plan, and the frozen spec. No path appears in both arrays.
- `scripts/verify-dirty-baseline.mjs` gains a `--scope <module>` argument selecting `p01-scope.mjs` or `p02-scope.mjs`, defaulting to `p01-scope.mjs` so the existing P01 invocation keeps its exact current behavior and output.
- The preflight JSON has the same shape P01 used: HEAD, UTC capture time, Base64 and SHA-256 of raw NUL-delimited porcelain-v1 status bytes, the two exact path arrays, and one sorted entry per pre-existing dirty file recording status, repository-relative path, byte count, and lowercase working-tree SHA-256 or `absent`.

- [ ] **Step 1: Capture preflight before the first implementation write**

```powershell
git -c safe.directory=E:/Soft/vestrace rev-parse HEAD
git -c safe.directory=E:/Soft/vestrace status --porcelain=v1 -z --untracked-files=all
```

Persist `docs/development-evidence/v1-g0-02-preflight.json` as the first implementation write. Do not stage anything. If status changes between capture and persistence, discard and repeat.

- [ ] **Step 2: RED**

`tests/p02_scope.test.mjs` asserts both arrays exist, are disjoint, contain all four protected authority paths, and that `--scope p02-scope.mjs` rejects a mutated protected path in a temporary fixture repository.

```powershell
node --test tests/p02_scope.test.mjs
```

- [ ] **Step 3: GREEN**

```powershell
node --test tests/p02_scope.test.mjs
node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-01-preflight.json
node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-02-preflight.json --scope p02-scope.mjs
```

The P01 invocation must produce byte-identical output to before the change.

- [ ] **Step 4: Scoped diff review**

Confirm the preflight predates every other P02 write and the P01 verifier path is behaviorally unchanged.

---

### Task 2: Structural route inventory with fail-closed default deny

**Files:**
- Create: `crates/vestrace-http/src/route_inventory.rs`
- Modify: `crates/vestrace-http/src/router.rs`
- Modify: `crates/vestrace-http/src/lib.rs`
- Modify: `crates/vestrace-http/src/api/mod.rs`
- Modify: every `crates/vestrace-http/src/api/*.rs` that calls `.route(...)`
- Modify: `crates/vestrace-http/tests/router_contract.rs`
- Create: `crates/vestrace-http/tests/route_inventory_is_exhaustive.rs`

**Interfaces:**
- `pub struct RouteDescriptor { pub method: Method, pub path_pattern: &'static str, pub capability: Capability, pub risk: RiskCategory, pub exposure: RouteExposure }`.
- `pub enum RouteExposure { Governed, PublicBounded }`. `PublicBounded` is permitted only for `/health/live` and `/health/ready`; the inventory constructor rejects any other `PublicBounded` entry at load time.
- `pub fn route_inventory() -> &'static [RouteDescriptor]` is the single manifest.
- `pub fn mount(router: Router<AppState>, descriptor: &RouteDescriptor, handler: MethodRouter<AppState>) -> Router<AppState>` is the **only** sanctioned way to add a route. It panics at construction time if the descriptor is not in `route_inventory()`. This is the structural half of acceptance criterion 1: because Axum exposes no public route-introspection API, coverage cannot be checked after the fact, so it is enforced at registration instead.
- `pub fn inventory_lookup(method: &Method, path: &str) -> RouteDecision` where `pub enum RouteDecision { Governed(AuthorizationRequest), PublicBounded, NotInInventory }`.
- `authorize_http_request` maps `NotInInventory` to a denial response, never to `next.run(request)`. This is the behavioral inversion of the current code.
- `http_capability_for_test` is removed and replaced by `pub fn inventory_lookup_for_test(method: &Method, path: &str) -> RouteDecision`, because `Option<Capability>` cannot express the difference between *public exception* and *not covered*.

- [ ] **Step 1: RED**

`crates/vestrace-http/tests/route_inventory_is_exhaustive.rs`:

- `mounting_a_route_absent_from_the_inventory_panics` — the registration guard, proved with `#[should_panic]`.
- `every_inventory_entry_is_mounted` — build the real router and assert each descriptor's method/path answers something other than 404-from-no-route, closing the reverse direction so a stale entry cannot linger.
- `access_token_routes_are_governed` — assert `GET /v1/access-tokens`, `POST /v1/access-tokens`, `DELETE /v1/access-tokens/{id}`, and `GET /v1/access-tokens/{id}/value` each resolve to `Governed` with `Capability::WorkspaceAdmin` and `RiskCategory::Critical`.
- `unknown_governed_route_is_denied_not_bypassed` — request `/v1/not-a-real-surface` through the assembled router with a valid credential; assert a denial, not a handler 404 and not a 200.
- `top_level_metrics_is_governed` — assert `GET /metrics` resolves to `Governed`, closing the current bypass.
- `only_health_probes_are_public` — assert exactly `/health/live` and `/health/ready` are `PublicBounded`, and their bodies carry no workspace identifier.
- `public_exception_cannot_be_widened` — the constructor rejects any other `PublicBounded` entry.

```powershell
cargo test -p vestrace-http --test route_inventory_is_exhaustive -- --nocapture
```

Expected: compile failure, because `route_inventory.rs` does not exist.

- [ ] **Step 2: GREEN**

Transcribe every arm currently in `http_capability` into descriptors, preserving each existing capability and risk decision exactly — including the deliberate ones the comments document (`MemoryPurge` for `DELETE /v1/memories/{id}`, `Critical` for `/approve`, `/v1/secrets` writes, `/v1/memories` delete and `/v1/effects`, `WorkspaceAdmin` for `capability-grants`). Add the four `/v1/access-tokens` routes and top-level `/metrics`. Convert every `.route(...)` call site to `mount(...)`. Invert `authorize_http_request`.

Replace `unknown_transport_path_is_not_promoted_to_a_governed_action` with `unknown_transport_path_is_denied`, and port the `router_contract.rs` assertions at lines 454-513 from `http_capability_for_test` to `inventory_lookup_for_test`.

```powershell
cargo test -p vestrace-http -- --nocapture
cargo test --workspace --locked
```

- [ ] **Step 3: Scoped diff review**

Confirm no capability or risk decision changed except the named additions, the public exception set did not grow, no route was removed to make a test pass, and no `.route(` call survives outside `mount`.

---

### Task 3: Transaction-bound persistence and atomic mutation plus Audit

**Files:**
- Modify: `crates/vestrace-application/src/security/audit.rs`
- Modify: `crates/vestrace-application/src/idempotency.rs`
- Modify: `crates/vestrace-application/src/outbox.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/audit_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/idempotency_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/outbox_repository.rs`
- Create: `crates/vestrace-application/src/governed_mutation.rs`
- Modify: `crates/vestrace-application/src/lib.rs`
- Modify: every governed write path enumerated in Step 1
- Create: `migrations/0165_governed_mutation_audit_atomicity.sql`
- Create: `crates/vestrace-infrastructure/tests/governed_mutation_is_atomic.rs`

**Interfaces:**
- Each of the three ports gains a transaction-bound method taking `&mut UnitOfWork` instead of opening its own scope: `record_in`, `save_in`. The existing self-transacting methods become thin wrappers that open a scope and delegate, so non-governed callers keep working unchanged during migration.
- `pub struct GovernedMutation<T> { pub context: RequestContext, pub audit: AuditEntry, pub idempotency: Option<IdempotencyRecord>, pub outbox: Vec<OutboxMessage>, pub apply: T }`.
- `pub trait GovernedMutationRepository: Send + Sync { async fn commit<T>(&self, mutation: GovernedMutation<T>) -> Result<GovernedMutationReceipt, ApplicationError>; }` — one implementation, one transaction opened via `PgStore::begin_scoped`, all four writes inside it.
- A deferred database constraint refuses a governed mutation transaction that commits without an Audit row, so atomicity is database-enforced and not only service-enforced.

### Task 3 scope decision, taken during implementation

The enumeration below was completed first and found **38** governed non-read routes. Tracing them showed that migrating all of them reaches roughly 38 further files — about fifteen application services and ports, about twenty PostgreSQL adapters, and the CLI composition root. That is a whole-codebase refactor, not one task.

Acceptance criterion 6 originally demanded all of them. That demand was the lead engineer's own addition and overreached the program index, which asks P02 for an "atomic mutation-plus-Audit transaction authority" and "Audit-rollback tests" — a mechanism and its proof. The index also states that P03's provider mutations "use P02's atomic Audit authority", so later packages adopt the mechanism for their own paths as they are built.

Operator decision: build the mechanism and migrate **one** path end-to-end. The chosen path is `/v1/access-tokens`, because it is the credential-minting surface, because its handler is the concrete mutate-then-audit-separately defect already identified, and because its files are clean.

Spec 11.3 still requires every governed mutation to share one transaction with its outbox, idempotency, and audit writes. P02 therefore records the remaining 37 paths as an explicit outstanding obligation in its evidence note. P02 must not claim that invariant holds system-wide.

- [ ] **Step 1: Enumerate every governed write path**

Before changing behavior, produce the exhaustive list of mutation roots. Start from every handler behind a `Governed` inventory entry with a non-read method, and record the list in the task's evidence. `crates/vestrace-http/src/api/access_tokens.rs` around line 196 is the known defect: it mutates first and audits afterward, in separate transactions.

Note when recording it that `POST /v1/retrieval/search` appears in the list because it is a governed non-read method, but it is query-like; do not force a mutation authority onto it if it performs no governed mutation. Say so explicitly instead.

- [ ] **Step 2: RED**

`crates/vestrace-infrastructure/tests/governed_mutation_is_atomic.rs`, using `#[sqlx::test(migrations = "../../migrations")]`:

- `failed_mutation_leaves_no_audit_entry`
- `failed_audit_write_rolls_back_the_mutation`
- `failed_outbox_write_rolls_back_mutation_and_audit`
- `mutation_without_audit_is_refused_by_the_database`
- `successful_mutation_commits_all_four_together`
- `access_token_creation_is_atomic_with_its_audit` — the concrete regression for the known-bad path.

```powershell
cargo test -p vestrace-infrastructure --test governed_mutation_is_atomic -- --nocapture
```

- [ ] **Step 3: GREEN**

```powershell
cargo test -p vestrace-infrastructure --test governed_mutation_is_atomic -- --nocapture
cargo test --workspace --locked
```

If PostgreSQL is unavailable, record this task as **blocked**; Tasks 5, 6, and 9 onward depend on it.

- [ ] **Step 4: Scoped diff review**

Confirm no second audit-writing path was introduced, `CognitiveMutationService` was not forked, the wrapper methods preserve existing non-governed callers, and every enumerated path from Step 1 was migrated.

---

### Task 4: Non-login owner role and guarded-operation foundation

**Files:**
- Create: `migrations/0166_guarded_operation_owner_role.sql`
- Create: `crates/vestrace-infrastructure/tests/runtime_role_cannot_write_directly.rs`
- Modify: `docker-compose.yml`

**Interfaces:**
- Migration `0166` creates a non-login role (`NOLOGIN`, `NOSUPERUSER`, `NOBYPASSRLS`) that owns every table and `SECURITY DEFINER` function P02 creates from Task 5 onward. The runtime `vestrace` role receives `EXECUTE` on the guarded functions and **no** `INSERT`, `UPDATE`, or `DELETE` on the tables.
- Every later P02 migration sets ownership explicitly rather than inheriting the migration runner's identity.
- The Compose change provisions the role for a clean local database; it does **not** re-own the pre-existing tables, which stays P05 scope.
- A shared test helper opens a connection as the runtime role — not as a superuser — because `crates/vestrace-infrastructure/tests/row_level_security.rs` already establishes that superuser connections bypass RLS outright and prove nothing here.

- [ ] **Step 1: RED**

`crates/vestrace-infrastructure/tests/runtime_role_cannot_write_directly.rs`:

- `runtime_role_is_not_the_owner_of_p02_tables`
- `runtime_role_direct_insert_is_refused`
- `runtime_role_direct_update_is_refused`
- `runtime_role_direct_delete_is_refused`
- `runtime_role_may_execute_guarded_functions`
- `owner_role_cannot_log_in`

At this point the tests reference tables that Task 5 onward will create, so the suite is written against a minimal fixture table created by `0166` itself, and extended by each later task.

```powershell
cargo test -p vestrace-infrastructure --test runtime_role_cannot_write_directly -- --nocapture
```

- [ ] **Step 2: GREEN**

```powershell
cargo test -p vestrace-infrastructure --test runtime_role_cannot_write_directly -- --nocapture
cargo test --workspace --locked
```

- [ ] **Step 3: Scoped diff review**

Confirm the role is `NOLOGIN`, pre-existing table ownership was not changed, and no test connects as a superuser to assert a refusal.

---

### Task 5: Installation fingerprint key in the vault, continuity proof in PostgreSQL

**Files:**
- Create: `crates/vestrace-domain/src/installation/fingerprint.rs`
- Modify: `crates/vestrace-domain/src/lib.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/installation_fingerprint.rs`
- Create: `migrations/0167_installation_fingerprint_continuity.sql`
- Create: `crates/vestrace-infrastructure/tests/fingerprint_continuity_is_fail_closed.rs`

**Interfaces:**
- The create-only, non-overwritable `InstallationFingerprintKey` record lives **only in the host vault**. PostgreSQL stores `InstallationId`, the opaque `FingerprintKeyId`, and `FingerprintKeyContinuityProof` — never the key.
- `pub fn continuity_proof(key: &FingerprintKey, installation: &InstallationId, key_id: &FingerprintKeyId) -> FingerprintKeyContinuityProof` computes exactly `HMAC-SHA-256(key, "vestrace-installation-fingerprint-v1" || InstallationId || FingerprintKeyId)`, per spec line 317. The domain-separation string is exact and is asserted by test.
- `pub fn external_id_fingerprint(key: &FingerprintKey, scope: &FingerprintScope, external_id: &str) -> ExternalIdFingerprint` is the **separate** uniqueness index over untrusted external identifiers, scoped by workspace/principal/protocol/endpoint. It is not the continuity proof and the two must not be conflated.
- Readiness recomputes the proof through the supervisor and compares without exporting the key. Absence, wrong identity, wrong version, or proof mismatch fails readiness **closed**.
- V1 has no rotation, overwrite, import, or silent replacement operation.

- [ ] **Step 1: RED**

- `key_record_is_never_stored_in_postgres` — a schema assertion that no column can hold key material.
- `continuity_proof_uses_the_exact_domain_separation_string`
- `readiness_fails_closed_on_missing_proof`
- `readiness_fails_closed_on_wrong_key_id`
- `readiness_fails_closed_on_same_key_id_different_key`
- `no_rotation_or_overwrite_operation_exists`
- `external_id_fingerprint_is_not_the_continuity_proof`
- `external_id_fingerprint_never_hashes_retention_governed_content`
- `runtime_role_direct_write_is_refused` — extends Task 4's suite.

```powershell
cargo test -p vestrace-infrastructure --test fingerprint_continuity_is_fail_closed -- --nocapture
```

- [ ] **Step 2: GREEN**

```powershell
cargo test -p vestrace-infrastructure --test fingerprint_continuity_is_fail_closed -- --nocapture
cargo test --workspace --locked
```

- [ ] **Step 3: Scoped diff review**

Confirm no key material reaches PostgreSQL, the two fingerprint authorities are distinct types, and the P05 backup-manifest and witness pins are recorded as outstanding rather than stubbed.

---

### Task 6: InstallationMutationPermit and transactional mutation watermark

**Files:**
- Create: `crates/vestrace-application/src/installation/permit.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/installation_permit.rs`
- Create: `migrations/0168_installation_mutation_permit.sql`
- Create: `crates/vestrace-infrastructure/tests/installation_permit_excludes.rs`
- Modify: `crates/vestrace-application/src/governed_mutation.rs`

**Interfaces:**
- `pub enum PermitMode { Shared, Exclusive }`.
- `pub trait InstallationMutationPermit { async fn acquire(&self, mode: PermitMode, ctx: &RequestContext) -> Result<PermitHandle, ApplicationError>; }`.
- Exclusivity is a PostgreSQL lock held for the transaction's lifetime, so it releases on crash without a reaper.
- The watermark advances inside the same transaction as the governed mutation; a mutation committing without advancing it is refused by a deferred constraint.
- Every governed mutation from Task 3 takes a `Shared` permit. P05's restore path will take `Exclusive`; P02 wires `Shared` only.

- [ ] **Step 1: RED**

- `exclusive_excludes_shared` — two concurrent transactions.
- `shared_permits_are_concurrent`
- `crashed_holder_releases_its_permit`
- `mutation_without_watermark_advance_is_refused`
- `watermark_is_monotonic_under_concurrency`

```powershell
cargo test -p vestrace-infrastructure --test installation_permit_excludes -- --nocapture
```

- [ ] **Step 2: GREEN**

```powershell
cargo test -p vestrace-infrastructure --test installation_permit_excludes -- --nocapture
cargo test -p vestrace-infrastructure --test governed_mutation_is_atomic -- --nocapture
cargo test --workspace --locked
```

- [ ] **Step 3: Scoped diff review**

Confirm the permit is transaction-scoped, no background reaper was added, and Task 3's tests still pass.

---

### Task 7: Shared material domain contract

**Files:**
- Create: `crates/vestrace-domain/src/material/mod.rs`
- Create: `crates/vestrace-domain/src/material/identity.rs`
- Create: `crates/vestrace-domain/src/material/size_class.rs`
- Modify: `crates/vestrace-domain/src/lib.rs`
- Create: `crates/vestrace-domain/tests/material_contract.rs`

**Interfaces:**
- `MaterialKeyId`, `IntentNonce`, `VaultReceipt`, `ErasureReceipt`, `AssociatedData`, `ZeroizingDek`, `SizeClass`, and the intent-state enums, all exported before any consumer exists.
- `ZeroizingDek` wraps `secrecy`'s zeroizing container; no `Clone`, no `Debug` that reveals bytes, no `Serialize`.
- `pub fn size_class_for(byte_len: usize) -> SizeClass` never returns a class below 4 KiB and never exposes an exact plaintext byte count.

This task exists because the review found Tasks 8-12 all consume these types while nothing declared them; their RED tests would otherwise have failed for an undeclared API rather than for behavior.

- [ ] **Step 1: RED**

- `size_class_boundaries` — exactly the spec's cases at 0, 1, 2, 4095, 4096, and 4097 bytes, with no class below 4 KiB.
- `size_class_does_not_reveal_exact_length`
- `zeroizing_dek_has_no_revealing_debug`
- `zeroizing_dek_is_not_cloneable` — a compile-fail assertion.

```powershell
cargo test -p vestrace-domain --test material_contract -- --nocapture
```

- [ ] **Step 2: GREEN**

```powershell
cargo test -p vestrace-domain --test material_contract -- --nocapture
cargo test --workspace --locked
```

- [ ] **Step 3: Scoped diff review**

Confirm every type Tasks 8-12 reference is exported here, and no persistence or vault behavior leaked into the domain crate.

---

### Task 8: Host material-key vault with witnessed erasure fence

**Files:**
- Create: `crates/vestrace-application/src/material/vault.rs`
- Create: `crates/vestrace-infrastructure/src/crypto/material_vault.rs`
- Modify: `crates/vestrace-infrastructure/src/crypto/mod.rs`
- Create: `crates/vestrace-infrastructure/tests/material_vault_contract.rs`

**Interfaces:**
- `pub trait MaterialKeyVault: Send + Sync` with `create_if_absent`, `unwrap`, `prepare_erasure`, and `erase`.
- `create_if_absent(&self, key_id, nonce) -> Result<VaultReceipt, VaultError>` is idempotent on the exact `(key_id, nonce)` pair and returns the original receipt on replay. It never returns a different key for a reserved identity, and it refuses a different nonce for the same `key_id`.
- **`prepare_erasure(&self, key_id) -> Result<FenceReceipt, VaultError>` is the fence the review found missing.** It records durable host-side state and must be witnessed before the database's phase-one erasure preparation is actionable. After it, `unwrap` refuses **without consulting PostgreSQL**, closing the window between preparation and physical erase.
- `erase(&self, key_id) -> Result<ErasureReceipt, VaultError>` is idempotent and witnessed.
- The adapter reuses `MountedSecretStoreKeyProvider` for the bootstrap reference that locates the vault; DEK envelopes never enter the mounted store, which the spec restricts to installation and bootstrap references.

- [ ] **Step 1: RED**

- `create_if_absent_is_idempotent_for_the_same_nonce`
- `create_if_absent_refuses_a_different_nonce_for_the_same_key_id`
- `unwrap_refuses_after_prepare_erasure_without_consulting_postgres`
- `prepare_erasure_is_idempotent_and_witnessed`
- `erase_after_prepare_is_idempotent`
- `erase_without_prepare_is_refused`
- `dek_is_zeroized_after_use`
- `vault_never_writes_dek_envelopes_into_the_mounted_bootstrap_store`

```powershell
cargo test -p vestrace-infrastructure --test material_vault_contract -- --nocapture
```

- [ ] **Step 2: GREEN**

```powershell
cargo test -p vestrace-infrastructure --test material_vault_contract -- --nocapture
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
```

- [ ] **Step 3: Scoped diff review**

Confirm no plaintext DEK is logged, serialized, or held beyond one invocation, and the fence is genuinely independent of the database.

---

### Task 9: MaterialKeyCreationIntent lifecycle and ContentMaterial guards

**Files:**
- Create: `crates/vestrace-domain/src/material/intent.rs`
- Create: `crates/vestrace-domain/src/material/content.rs`
- Create: `crates/vestrace-application/src/material/commands.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/material_intent.rs`
- Create: `migrations/0169_material_key_creation_intents.sql`
- Create: `migrations/0170_content_material_guards.sql`
- Create: `crates/vestrace-infrastructure/tests/material_intent_lifecycle.rs`

**Interfaces:**
- Closed sequence `Reserved -> ProvisionalCreated -> ProvisionalReceipted -> ContentPrepared -> Bound -> Live`.
- The abort branch is `ContentPrepared -> ContentAbandonPrepared -> Abandoned`, and per spec line 1075 the transition to `Abandoned` requires **both** removal of the prepared ciphertext and attachment **and** a witnessed unbound-key erase receipt. A plain `ContentPrepared` marker may never jump to `Abandoned`.
- `ResultPrepared` always binds and finalizes; it never abandons.
- `PreparedMaterialAttachment` is nonordinary and internal: it cannot be queried, hydrated, executed, shared, accepted, or referenced as Live. Only the exact `Bound` finalizer atomically promotes the ordinary reference and Live material.
- Deferred database invariants enforce the sequence and the abort branch. Transitions occur only through `SECURITY DEFINER` operations owned by Task 4's non-login role.

- [ ] **Step 1: RED**

- `closed_sequence_is_enforced`
- `content_prepared_is_not_referenceable`
- `content_prepared_cannot_jump_to_abandoned`
- `abandon_requires_ciphertext_removal_and_witnessed_receipt`
- `result_prepared_always_binds_and_never_abandons`
- `raw_sql_cannot_publish_a_prepared_identity`
- `raw_sql_cannot_resurrect_an_abandoned_identity`
- `runtime_role_direct_write_is_refused`

```powershell
cargo test -p vestrace-infrastructure --test material_intent_lifecycle -- --nocapture
```

- [ ] **Step 2: GREEN**

```powershell
cargo test -p vestrace-infrastructure --test material_intent_lifecycle -- --nocapture
cargo test --workspace --locked
```

- [ ] **Step 3: Scoped diff review**

Confirm the sequence is database-enforced, the abort branch cannot complete without its receipt, and no exact plaintext length reaches a persisted column.

---

### Task 10: Permanent guards, CredentialSlot, and CredentialRevision

**Files:**
- Create: `crates/vestrace-domain/src/credential/slot.rs`
- Create: `crates/vestrace-domain/src/credential/revision.rs`
- Create: `crates/vestrace-domain/src/credential/guard.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/credential_guard.rs`
- Create: `migrations/0171_connection_execution_guards.sql`
- Create: `migrations/0172_credential_slots_and_revisions.sql`
- Create: `crates/vestrace-infrastructure/tests/credential_guards.rs`

**Interfaces:**
- `ConnectionExecutionGuard` is permanent, keyed by `(workspace, ConnectionId)`, never deleted and never reused for another Connection.
- `CredentialActivationGuard` is permanent, keyed by `(workspace, ConnectionId, CredentialSlotId)`, created before Candidate material or qualification exists.
- Canonical lock order is `ConnectionExecutionGuard -> CredentialActivationGuard -> slot -> revision/material rows`, enforced by a helper so no call site can invert it.
- A database-enforced live-occupancy constraint permits at most one nonterminal preparing/Candidate per permanent guard.
- `CredentialSlot` holds compare-and-swap current-revision and tombstone fields and **no ciphertext**. `CredentialRevision` is immutable metadata naming a per-revision `MaterialKeyId` and an associated-data profile.

P02 creates these guards and their occupancy rules only. First activation, rotation, and qualification finalization are P03 and later.

- [ ] **Step 1: RED**

- `execution_guard_is_permanent`
- `activation_guard_precedes_candidate_material`
- `occupancy_permits_at_most_one_nonterminal_candidate`
- `second_concurrent_preparing_returns_a_typed_conflict`
- `lock_order_helper_refuses_an_inverted_acquisition`
- `slot_holds_no_ciphertext_column`
- `runtime_role_direct_write_is_refused`

```powershell
cargo test -p vestrace-infrastructure --test credential_guards -- --nocapture
```

- [ ] **Step 2: GREEN**

```powershell
cargo test -p vestrace-infrastructure --test credential_guards -- --nocapture
cargo test --workspace --locked
```

- [ ] **Step 3: Scoped diff review**

Confirm no activation, rotation, or qualification behavior leaked in, and the guards are permanent rows rather than mutable pointers.

---

### Task 11: CredentialKeyCreationIntent lifecycle and guarded pre-live abort

**Files:**
- Create: `crates/vestrace-domain/src/credential/intent.rs`
- Create: `crates/vestrace-application/src/credential/commands.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/credential_intent.rs`
- Create: `migrations/0173_credential_key_creation_intents.sql`
- Create: `crates/vestrace-infrastructure/tests/credential_intent_lifecycle.rs`

**Interfaces:**
- Closed sequence `Reserved -> ProvisionalCreated -> ProvisionalReceipted -> CredentialPrepared -> Bound -> Candidate`.
- Pre-live abort branch `Reserved | ProvisionalCreated | ProvisionalReceipted | CredentialPrepared -> CredentialAbandonPrepared -> Abandoned`, taking `ConnectionExecutionGuard`, then `CredentialActivationGuard`, then association expected version, then intent, then prepared-material rows, in exactly that order.
- The abort requires the association still `Preparing`, no `Bound`/`Candidate`/`ErasurePrepared`, and atomically appends `CredentialAssociationCancelled` plus `CredentialAbandonPrepared`. It creates no `Candidate` and no `ErasurePrepared` fact.
- Only the witnessed unbound key erase finalizes the intent `Abandoned`. A prepared row never jumps directly to `Abandoned`.
- The revision identity is allocated **before** encryption, so concurrent writers cannot encrypt under one identity and return another.
- One `SECURITY DEFINER` creator owned by Task 4's non-login role, with fixed `search_path` and no dynamic SQL, inserts `CredentialPrepared` ciphertext and its internal prepared attachment exactly once without exposing the revision.

- [ ] **Step 1: RED**

- `closed_sequence_is_enforced`
- `abort_branch_never_creates_candidate`
- `abort_branch_never_creates_erasure_prepared`
- `prepared_row_cannot_jump_to_abandoned`
- `sql_rollback_does_not_erase_a_key_already_created_in_the_vault`
- `revision_identity_is_allocated_before_encryption`
- `concurrent_creation_returns_a_typed_conflict`
- `raw_sql_cannot_forge_a_prepared_attachment`
- `raw_sql_cannot_forge_a_candidate_event`
- `occupancy_releases_only_after_cancelled_plus_witnessed_terminal_receipt`

```powershell
cargo test -p vestrace-infrastructure --test credential_intent_lifecycle -- --nocapture
```

- [ ] **Step 2: GREEN**

```powershell
cargo test -p vestrace-infrastructure --test credential_intent_lifecycle -- --nocapture
cargo test --workspace --locked
```

- [ ] **Step 3: Scoped diff review**

Confirm the two branches are never conflated and the pre-live path genuinely cannot reach `Candidate` or `ErasurePrepared`.

---

### Task 12: One-way erasure primitives and two-phase destruction

**Files:**
- Create: `crates/vestrace-application/src/material/erasure.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/erasure.rs`
- Create: `migrations/0174_material_erasure_primitives.sql`
- Create: `crates/vestrace-infrastructure/tests/erasure_is_one_way.rs`
- Create: `crates/vestrace-infrastructure/tests/erasure_holds_no_lock_during_vault_call.rs`

**Interfaces:**
- One-way `Live -> ErasurePrepared -> Tombstoned` on the content path and `material -> erasure prepared -> Destroyed` on the credential path. No reverse transition exists in code or SQL.
- Phase one validates every blocker under the canonical lock order, appends the erasure preparation, commits, and **releases every database lock before any vault call**. The vault's `prepare_erasure` fence from Task 8 makes the window between preparation and physical erase safe.
- Phase two erases outside every transaction, then a finalizer reacquires the locks in canonical order, verifies the witnessed receipt, appends the terminal event, removes only that ciphertext row, and writes the audit tombstone.
- Idempotent replay returns the original preparation or receipt without starting a second erasure.

- [ ] **Step 1: RED**

`erasure_is_one_way.rs`:
- `live_cannot_return_from_erasure_prepared`
- `tombstoned_cannot_return_to_live`
- `raw_sql_cannot_resurrect_a_tombstoned_identity`
- `replay_returns_the_original_receipt_without_a_second_erase`
- `blocker_refuses_preparation` — a nonterminal effect, a usable unexpired lease, or a nonterminal intent each block.
- `metadata_and_audit_survive_erasure`

`erasure_holds_no_lock_during_vault_call.rs` — a `#[sqlx::test]` concurrency test. A **second independent PostgreSQL session** acquires the relevant lock while the vault call is deliberately blocked; if the acquisition succeeds, no lock was held. A flag inside the vault double is not accepted as evidence here, because it proves only application control flow.

```powershell
cargo test -p vestrace-infrastructure --test erasure_is_one_way -- --nocapture
cargo test -p vestrace-infrastructure --test erasure_holds_no_lock_during_vault_call -- --nocapture
```

- [ ] **Step 2: GREEN**

```powershell
cargo test -p vestrace-infrastructure --test erasure_is_one_way -- --nocapture
cargo test -p vestrace-infrastructure --test erasure_holds_no_lock_during_vault_call -- --nocapture
cargo test --workspace --locked
```

- [ ] **Step 3: Scoped diff review**

Confirm no database lock spans a vault call, and erasure removes ciphertext only — never the evidence that the material existed.

---

### Task 13: Crash-boundary and idempotent-replay fault suite

**Files:**
- Modify: `crates/vestrace-fault-scenario/src/settings.rs`
- Create: `crates/vestrace-fault-scenario/src/scenarios/material_intent_crash.rs`
- Create: `crates/vestrace-fault-scenario/src/scenarios/credential_intent_crash.rs`
- Modify: `crates/vestrace-fault-scenario/src/main.rs`
- Create: `crates/vestrace-infrastructure/tests/intent_crash_boundaries.rs`

**Interfaces:**
- The existing harness contract is preserved, not bypassed: it requires `VESTRACE_FAULT_ISOLATION=ephemeral`, a recognized `VESTRACE_FAULT_POINT`, and `--database-url-file`. New boundaries are added as variants of the existing `EffectFaultPoint` enum in `settings.rs`; the refusal guard at lines 34-40 stays exactly as strict.
- New fault points, for each of the two lifecycles: `after_reserved`, `after_vault_create_before_receipt`, `after_receipt_before_prepared`, `after_prepared_before_bound`, `after_bound_before_promotion`, `after_abort_before_witnessed_erase`, `after_erase_receipt_before_terminal_append`.
- For each boundary the assertion pair is: the surviving state is one of the resumable states the spec names, and the reconciler drives it to a lawful terminal without creating a second identity, a second vault key, or a second effect.

- [ ] **Step 1: RED**

`crates/vestrace-infrastructure/tests/intent_crash_boundaries.rs`, one test per boundary per lifecycle, named `<lifecycle>_crash_after_<boundary>_is_resumable`.

```powershell
cargo test -p vestrace-infrastructure --test intent_crash_boundaries -- --nocapture
```

- [ ] **Step 2: GREEN**

Each scenario runs under the real harness contract. Create the ephemeral database and write its URL to a file first, then, for every boundary:

```powershell
$env:VESTRACE_FAULT_ISOLATION = "ephemeral"
$env:VESTRACE_FAULT_POINT = "after_vault_create_before_receipt"
cargo run -p vestrace-fault-scenario -- --scenario material_intent_crash --database-url-file .fault-db-url
```

```powershell
cargo test -p vestrace-infrastructure --test intent_crash_boundaries -- --nocapture
cargo test --workspace --locked
```

If the destructive harness cannot run in this environment, record it as **blocked**, not as pass.

- [ ] **Step 3: Scoped diff review**

Confirm no crash boundary is skipped, the ephemeral-isolation guard was not weakened, and no test asserts merely that "the reconciler ran" without asserting the resulting state.

---

### Task 14: Integrated verification and evidence boundary

**Files:**
- Create: `docs/development-evidence/v1-g0-02-security-material-foundation.md`
- Inspect only: every P02 file named above
- Inspect only: repository-wide status

**Interfaces:**
- The evidence note records observed commands and exit codes, source revision, the scoped file list, the pre-existing dirty-file acknowledgement, the enumerated governed-write-path list from Task 3 Step 1, and explicit non-claims. It contains no secret, token, credential, payload, or model output.

- [ ] **Step 1: Run focused verification from a fresh process**

```powershell
node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-02-preflight.json --scope p02-scope.mjs
node scripts/protocol-lock.mjs --check .
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
npm --prefix apps/console run typecheck
```

Every command must exit `0`.

**Do not run the P01 verifier against the P01 preflight here.** That verifier rejects any new dirty path outside P01's change scope, so it necessarily exits `1` as soon as P02 creates its first file — this was observed at `exit 1` naming `scripts/p02-scope.mjs` and the other P02 additions. That is the verifier working correctly, not a regression, and re-running it here would either produce a permanent false failure or invite someone to weaken it.

P01's outputs are protected by two other checks instead, and both must exit `0`: the P02 preflight captured every P01 artifact as a pre-existing dirty file, so the P02 verifier proves they are byte-identical unless they are also in P02's change scope; and `protocol-lock.mjs --check` independently proves the P01 protocol baseline still reconstructs exactly.

- [ ] **Step 2: Repository hygiene**

```powershell
git -c safe.directory=E:/Soft/vestrace status --porcelain=v1 --untracked-files=all
git -c safe.directory=E:/Soft/vestrace diff --check
```

Compare the complete status against the P02 preflight. Do not stage, delete, reset, or rewrite any unrelated file.

- [ ] **Step 3: Persist truthful scoped evidence**

State explicitly:

- what P02 established: the structural route inventory and fail-closed default deny, the transaction-bound governed mutation/Audit authority, the non-login owner role, the fingerprint key and continuity proof, the installation permit and watermark, both key-intent lifecycles, the material vault with its erasure fence, the permanent guards, and the erasure primitives;
- P02 implements **no** Connection or Model revision, provider execution, qualification, embedding, restore, protocol endpoint, or console workflow;
- the recorded outstanding P05 dependency: `FingerprintKeyContinuityProof` is not yet pinned in backup manifests or the independent witness;
- least-privilege ownership was established for P02's own tables only; pre-existing tables remain runtime-role-owned and are P05 scope;
- no LM Studio, remote API, browser, TCK, accessibility, Compose readiness, or release qualification was established;
- any blocked evidence, named as blocked with its reason;
- G0 and v1.0 remain incomplete.

- [ ] **Step 4: Independent review gate**

An independent reviewer traces every P02 requirement to live files and fresh command output and returns material findings plus `VERDICT: APPROVE` or `VERDICT: REVISE`. Correct every material finding with the same persistent cdx builder, then repeat focused verification and review.

- [ ] **Step 5: Operator handoff**

Report the scoped outcome and the remaining package count. Do not start P03 until P02 is accepted. Do not call the result G0-complete or v1.0-ready.

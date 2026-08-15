# G2 Universal Authorization Boundary Delta

Date: 2026-08-11

## Authority

- `docs/plans/v0.2-to-v1.0-36-pr-execution-matrix.md`: G2 follows G1 and requires an all-entry-point deny/bypass suite at the Govern gate.
- `docs/implementation-plan-v0.2.md`: G2 is the universal authorization boundary across HTTP/MCP/worker/internal paths.
- `docs/specs/vestrace-qualification-conformance-spec-v0.2.md`: the policy bypass matrix requires equivalent policy behavior across HTTP, MCP, CLI, worker/background job, internal replay/recovery, and federation adapter paths.
- `docs/specs/vestrace-normative-invariants-v0.2.md`: CAP-001..003 and CAP-011 require effective capability/policy authority, default deny, constrained requests, and decision evidence.

## Scope implemented

1. Added application `AuthorizationBoundary` with `evaluate` and fail-closed `require` operations over the G1 `PolicyDecisionEngine`. A deny becomes `ApplicationError::Policy` before the downstream operation is entered.
2. Added HTTP `AppState` policy injection with production-safe `DenyAllPolicyEngine` defaults. Governed `/v1` and `/ag-ui` routes map method/path to typed capabilities, operation/resource selectors, and operation risk (`low` through `critical`), parse the trusted workspace/principal context, and authorize in middleware before handlers.
3. Kept health, readiness, metrics, and unmatched transport paths outside the governed action mapper; they do not execute domain actions. Unsupported action routes remain no-op handlers and are not evidence of feature availability.
4. Added MCP policy injection with a fail-closed default. Every listed tool is mapped to an authorization request with typed resource and risk; unknown tools are rejected as policy errors even when a broad test policy is present, and denied requests do not reach repositories.
5. Added policy-aware `RunWorker` construction with a fail-closed default. A leased work item is authorized by exact worker operation and run scope before run lease acquisition, snapshot processing, or handler invocation; denied items are dead-lettered as non-retryable authorization failures.
6. Added `tests/fixtures/qualification/g2-universal-authorization.json` with application, HTTP, MCP, worker, and internal evidence mappings.

## Exit evidence

- RED tests were observed before the G2 APIs existed for the common boundary, HTTP default deny, MCP dispatch denial, and worker handler denial.
- `tests/g2_authorization_boundary.rs` covers shared deny semantics and the evidence-only fixture manifest.
- `crates/vestrace-http/tests/run_routes.rs` proves default deny occurs before the canonical run command executor; positive route fixtures use explicit test-only matching policy.
- `crates/vestrace-mcp/src/server.rs` proves known and unknown denied tools stop before dispatch; positive unit fixtures use a test-only matching policy.
- `crates/vestrace-application/tests/run_worker.rs` proves denied run work is dead-lettered without handler invocation; existing positive worker tests use a test-only matching policy.
- Production server/MCP/worker constructors remain fail closed until policy grants are loaded from a trusted durable source.

## Explicit non-claims

This is a G2 wiring and bypass-evidence slice, not a `QualificationBundle` and not a qualified `GOVERNANCE` profile. It does not claim:

- durable PostgreSQL grant/decision persistence or runtime audit/event history;
- delegation attenuation, bounded delegation depth, hierarchical effect-time budgets, or approval override/material-intent semantics; those remain G3/later governance work;
- the generic job poller as a governed action path, because its `JobRepository` contract has no trusted workspace/principal context; the governed worker evidence path is `RunWorker`;
- federation adapter enforcement or complete CLI/agent-facing policy composition;
- complete external-effect authorization or full qualification closure.

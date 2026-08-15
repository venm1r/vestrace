# G2 Universal Authorization Boundary Implementation Plan

> **For agentic workers:** Execute this plan in bounded blocks. Keep the
> existing dirty worktree intact and do not commit, reset, rebase, or broad-
> format the checkout.

**Goal:** Make the G1 `PolicyDecision` kernel the single fail-closed authorization boundary for governed HTTP, MCP, worker, and internal execution entry points.

**Architecture:** Add one application-level boundary adapter that evaluates a typed `AuthorizationRequest` and rejects denied decisions before downstream handlers or repositories run. HTTP middleware, MCP tool dispatch, and the run worker will map their entry-point-specific operation/resource to that same boundary. Existing health/metrics/unmatched transport paths remain explicitly outside the governed action set; unsupported feature handlers remain no-op endpoints. Production constructors default to `DenyAllPolicyEngine` until durable grant loading is implemented.

**Tech Stack:** Rust 2024, Axum middleware, async-trait, Cargo workspace tests, JSON qualification fixture, Markdown implementation delta.

## Progress

- [x] Block 1: RED boundary and bypass tests
- [x] Block 2: shared application authorization boundary
- [x] Block 3: HTTP middleware and route mapping
- [x] Block 4: MCP and worker/internal wiring
- [x] Block 5: documentation, focused/full verification, and independent review

## Authority and scope

- `docs/plans/v0.2-to-v1.0-36-pr-execution-matrix.md`: G2 follows G1 and requires the universal HTTP/MCP/worker/internal wiring and all-entry-point deny/bypass evidence.
- `docs/implementation-plan-v0.2.md`: G2 is the universal authorization boundary across HTTP/MCP/worker/internal paths.
- `docs/specs/vestrace-qualification-conformance-spec-v0.2.md`: the policy bypass matrix requires equivalent policy behavior across HTTP, MCP, CLI, worker/background job, internal replay/recovery, and federation adapter paths.
- `docs/specs/vestrace-normative-invariants-v0.2.md`: CAP-001..003, CAP-011, and the default-deny semantics govern this slice. G3 delegation attenuation and hierarchical effect-time budgets are not in scope.

## Global constraints

- Every governed action must be authorized by the shared `PolicyDecisionEngine`; no surface may replace it with a role check or local allow-list.
- A denied decision must stop the downstream handler/repository before side effects.
- Production defaults remain fail closed through `DenyAllPolicyEngine`; no production allow-all constructor or bypass flag is introduced.
- Exact operation/tool and resource selectors are preserved in each adapter mapping.
- Health, readiness, metrics, and unmatched transport paths are documented non-action exceptions; they do not execute governed domain actions.
- This slice does not add durable grant/decision persistence, approval override semantics, delegation attenuation, hierarchical budgets, federation wiring, or full qualification.

## Blocks

### Block 1: Establish RED boundary and bypass tests

Files:

- Create focused application, HTTP, MCP, and worker tests as close as practical to each entry point.
- Create `tests/fixtures/qualification/g2-universal-authorization.json`.

Tests must prove that a deny decision is observable, that the common boundary returns a policy error, and that denied HTTP handlers, MCP repository calls, and worker handlers are not invoked. Include an unknown MCP tool denial and a cross-surface default-deny case.

Run the focused tests before implementation and record the expected compile/API failures.

### Block 2: Implement the shared application boundary

Files:

- Add an application security boundary module and exports.
- Keep the existing coarse `PolicyEngine` compatibility layer intact.

Provide `AuthorizationBoundary::evaluate` and `AuthorizationBoundary::require` (or equivalent) around `PolicyDecisionEngine`. `require` returns the decision on allow and `ApplicationError::Policy` on deny, retaining the decision reason in the error message.

### Block 3: Wire HTTP universally

Files:

- Modify HTTP `AppState` constructors and router middleware.
- Add a stable HTTP method/path-to-`AuthorizationRequest` mapper.
- Update only the tests that need an explicit matching grant; retain safe default-deny constructors.

Governed `/v1` and `/ag-ui` actions must parse the existing trusted workspace/principal headers and invoke the shared boundary before handlers. Health/readiness/metrics and unmatched paths retain their documented transport behavior. Route families use the existing typed capabilities; operation and resource selectors remain exact and deterministic.

### Block 4: Wire MCP and worker/internal execution

MCP `handle_tool_call` must authorize before the tool `match`, deny unknown tools, and map every listed tool to an exact operation/resource request. Existing tests receive an explicit test-only grant engine; production CLI construction remains fail closed.

`RunWorker` must authorize the work-item kind after leasing and before run lease acquisition or handler invocation. A denied item is dead-lettered with a non-retryable authorization failure, and the handler must not run. The production worker command passes the explicit deny-all engine until grants are loaded. The generic infrastructure job poller remains an orchestration-only path unless its repository contract carries a trusted workspace/principal context.

### Block 5: Documentation, verification, and review

Create the G2 implementation delta and update current-status/coverage navigation only for evidence-backed G2 behavior. Add the fixture as evidence-only and state all non-claims. Run scoped rustfmt, `git diff --check`, focused tests, workspace check/tests/no-run, and the relevant acceptance suite. Request independent review of the exact diff, fix Critical/Important findings, and record the G2 handoff with G3 as the next gate.

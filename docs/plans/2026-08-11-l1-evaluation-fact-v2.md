# L1 Evaluation Fact v2

Date: 2026-08-11

## Authority

- `docs/plans/v0.2-to-v1.0-36-pr-execution-matrix.md`: L1 is an additive `LRN` slice for typed evaluation evidence.
- `docs/implementation-plan-v0.2.md`: L1 is `Evaluation Fact v2 with exact target/evaluator/evidence refs`.
- `docs/specs/vestrace-normative-invariants-v0.2.md`: `LRN-001`, `LRN-002`, `LRN-005`, `LRN-007`, and the non-escalation boundary in `LRN-003..004`.
- `docs/specs/vestrace-domain-model-v0.2.md`: Evaluation target, evaluator kind, metric/check, result, evidence refs, and evaluator revision.
- `docs/specs/vestrace-architecture-contract-v0.2.md`: raw evaluation facts remain separate from learned projections; feedback cannot silently change authority.

## Scope

This slice adds an additive typed raw-fact path:

1. domain `EvaluationFact` with exact model/workflow/tool execution targets;
2. evaluator kind, evaluator model/principal/revision, metric, result, evidence refs, authority, and policy version;
3. domain validation for finite metrics, non-blank policy versions, required evidence, and complete model-judge/human identities;
4. application repository methods with legacy `EvaluationRecord` CRUD preserved;
5. PostgreSQL `evaluation_facts` storage and workspace RLS migration;
6. HTTP `/evaluation-facts` create/list/get endpoints;
7. no learned projection, proposal, asset mutation, capability change, or self-escalation path.

## Exit criteria

- [x] Exact target/evaluator/evidence/policy fields are typed and serializable.
- [x] Raw facts are represented separately from legacy evaluation CRUD and learned projections.
- [x] Invalid metric/evidence/evaluator/policy inputs are rejected before persistence.
- [x] Repository methods preserve workspace context and expose no update/delete operation.
- [x] PostgreSQL migration adds the isolated RLS-protected raw-fact table.
- [x] HTTP create/list/get routes expose typed facts without changing legacy routes.
- [x] Focused L1 tests pass.
- [x] Documentation records that L2 owns learned projection/proposal boundaries.

## Verification

```text
cargo test --test l1_evaluation_fact
cargo check --workspace
```

PostgreSQL migration execution remains environment-dependent until `DATABASE_URL` and the isolated database runtime are available.

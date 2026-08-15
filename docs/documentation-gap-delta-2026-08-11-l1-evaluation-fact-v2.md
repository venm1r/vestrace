# Documentation Gap Delta — L1 Evaluation Fact v2

Date: 2026-08-11

## Authority and boundary

L1 is the first `v0.3 Learn` implementation slice and is explicitly additive. It implements typed raw evaluation evidence; it does not implement learned conclusions, proposals, asset mutation, capability changes, or qualification of the `COGNITION` profile. Those boundaries remain L2/L3 work.

## Implemented

`crates/vestrace-domain/src/evaluation.rs` adds:

- exact `EvaluationTarget` variants for model, workflow, and tool executions;
- `EvaluatorKind` and `EvaluatorRef` with model/principal/revision identity;
- typed `EvaluationMetric`, `EvaluationResult`, and `EvaluationAuthority`;
- `EvaluationFact` with evidence references, policy version, workspace, and creation time;
- validation for finite metrics, required evidence, policy version, and exact model-judge/human evaluator identity.

`EvaluationRepository` gains additive `create_fact`, `list_facts`, and `find_fact` methods. Existing legacy `EvaluationRecord` CRUD remains unchanged.

`migrations/0123_evaluation_facts_v2.sql` creates an isolated workspace-RLS-protected raw fact table with JSONB typed payloads, evidence-array validation, policy-version validation, and workspace/target indexes. `PgEvaluationRepository` persists and hydrates the typed fact without adding a learned projection table.

HTTP exposes:

- `POST /evaluation-facts`;
- `GET /evaluation-facts`;
- `GET /evaluation-facts/{id}`.

## Verification evidence

`tests/l1_evaluation_fact.rs` contains four focused tests covering:

1. exact target/evaluator/metric/evidence binding;
2. serialization without a learned projection field;
3. invalid evidence, metric, policy, and incomplete evaluator rejection;
4. HTTP create-payload shape.

## Remaining gaps

- LRN-003/004 proposal-only learning and no-self-escalation enforcement remain L2 work.
- Learned projections/conclusions and measurement lineage remain L2/L3 work.
- Legacy `/evaluations` CRUD is preserved but is not retroactively converted to typed Fact v2.
- PostgreSQL migration runtime/RLS execution still needs an isolated `DATABASE_URL` environment.
- This slice does not claim COGNITION qualification or a QualificationBundle.

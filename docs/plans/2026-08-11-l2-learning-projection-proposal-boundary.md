# L2 Learned Projection / Proposal Boundary

Date: 2026-08-11

## Authority

- `docs/plans/v0.2-to-v1.0-36-pr-execution-matrix.md`: L2 is additive, proposal-only learning with no self-escalation.
- `docs/implementation-plan-v0.2.md`: L2 is the learned projection/proposal boundary.
- `docs/specs/vestrace-normative-invariants-v0.2.md`: `LRN-002` through `LRN-008`.
- `docs/specs/vestrace-architecture-contract-v0.2.md`: raw facts remain separate, projections are advisory, and asset changes require versioned governance.
- `docs/specs/vestrace-domain-model-v0.2.md`: derived projections retain source generation and rebuild provenance.

## Scope implemented

1. Add typed `LearnedProjection` with raw `EvaluationId` lineage, evidence refs, source generation, generator revision, and an explicit `Advisory` authority.
2. Add typed `LearningProposal` with projection/fact provenance, expected target revision, rationale, policy version, creator, and a draft/submitted/rejected/withdrawn lifecycle.
3. Limit proposal targets and changes to versioned agent, skill, workflow, model, tool, or routing assets. No capability, permission, security-policy, or workspace-admin variant exists in the contract.
4. Reject nested capability/permission/grant fields in structured proposal payloads.
5. Persist projections and proposals in separate forced-RLS tables. Repository creation verifies all raw fact/projection IDs belong to the request workspace; raw `evaluation_facts` are not updated or deleted.
6. Expose additive HTTP create/list/get/submit routes under `/learning`. No approval, apply, or automatic mutation endpoint is present.

## Exit criteria

- [x] Learned projections are advisory and retain raw evaluation/evidence provenance.
- [x] Proposal creation requires source projections, raw facts, policy version, rationale, and expected target revision.
- [x] Proposal changes are target-typed and have no self-escalation capability/permission path.
- [x] Submission changes proposal status only; it does not apply an asset mutation.
- [x] Projection and proposal persistence is separate from raw evaluation facts and workspace-scoped with RLS.
- [x] Focused L2 boundary tests pass.
- [x] Workspace compile and route compilation pass.

## Explicit non-claims

This slice does not approve, apply, publish, or verify a proposal, does not modify agent/skill/workflow/routing state, and does not qualify the `COGNITION` profile. PostgreSQL migration execution remains dependent on the configured database runtime.

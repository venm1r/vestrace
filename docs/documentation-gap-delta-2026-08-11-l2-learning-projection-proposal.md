# Documentation Gap Delta — L2 Learned Projection / Proposal Boundary

Date: 2026-08-11

## Authority and boundary

L2 is the additive `v0.3 Learn` slice after L1 raw evaluation facts. It implements a typed learned projection and proposal-only boundary. It intentionally stops before governance approval, asset mutation, capability changes, or self-escalation.

## Implemented

`crates/vestrace-domain/src/learning.rs` adds:

- `LearnedProjection` with explicit advisory authority;
- source `EvaluationId` and `EvidenceRef` lineage plus source generation;
- deterministic/model generator version provenance;
- typed `LearningTarget` and `LearningChange` variants limited to versioned cognitive, model, tool, and routing assets;
- `LearningProposal` with expected target revision, rationale, policy version, creator, and draft/submitted/rejected/withdrawn status.

`EvidenceRef::EvaluationRef` makes the L1 raw fact link first-class. The proposal contract has no capability, permission, security-policy, or workspace-admin target/change variant, and no `apply` transition or endpoint.

Structured change payloads also reject nested governance keys such as `capabilities`, `permissions`, `required_capabilities`, and `grants`, so an asset proposal cannot smuggle a self-escalation request through JSON configuration.

`LearningRepository` and `PgEvaluationRepository` provide additive workspace-scoped create/list/find operations. `migrations/0124_learning_projection_proposal_boundary.sql` creates separate `learned_projections` and `learning_proposals` tables with source-array, revision, advisory-authority, status, policy-version, rationale, index, and forced-RLS constraints.

Persistence validates every referenced raw fact and projection ID against the request workspace before insert. Both new tables use forced RLS so the runtime table owner cannot bypass the workspace policy.

HTTP exposes:

- `POST/GET /learning/projections` and `GET /learning/projections/{id}`;
- `POST/GET /learning/proposals`, `GET /learning/proposals/{id}`, and `POST /learning/proposals/{id}/submit`.

The existing `AppState::new` remains compatible through an unavailable learning-repository fallback; production wiring uses the PostgreSQL implementation through `new_with_learning`.

## Verification evidence

`tests/l2_learning_boundary.rs` contains six focused tests covering:

1. advisory projection authority and raw fact provenance;
2. versioned proposal submission without apply;
3. required provenance and positive target revision;
4. absence of capability/permission target fields.

`tests/migrations.rs` records the separate tables, advisory-only constraint, RLS policies, and proposal index.

## Remaining gaps

- Governance review/approval, verified proposal application, and audit/approval integration remain later `CAP`/`MEM`/`COGNITION` work.
- The projection generator does not yet provide a rebuild job or benchmark qualification.
- PostgreSQL migration/RLS runtime execution still requires an isolated `DATABASE_URL` environment.
- This delta does not claim COGNITION qualification or a `QualificationBundle`.

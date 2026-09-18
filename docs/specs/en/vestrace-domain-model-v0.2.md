# Vestrace Domain Model v0.2

> **English reading edition · 2026-09-08.** Complete editorial translation of the [frozen original](../vestrace-domain-model-v0.2.md) from supplied snapshot `3e05dfbd`. Requirement IDs, normative strength, technical states, and examples are preserved. This translation does not amend the original contract or claim implementation. If wording differs, the frozen original and applicable Accepted ADRs take precedence.

**Status:** Normative domain model draft
**Date:** 2026-08-10
**Foundational contract:** `docs/specs/vestrace-architecture-contract-v0.2.md`

> This document defines the target domain model. It does not claim that these aggregates are already implemented on current `main`.

## 1. Model purpose

Domain Model v0.2 provides a shared vocabulary for persistent cognition, provenance, temporal state, governed execution, and trust without creating parallel runtime models.

Primary distinction:

```text
Evidence / Sources
       ↓
Persistent Cognitive State
       ↓
Derived Retrieval / Context
       ↓
Execution / Effects
       ↓
Feedback / Health / Governance
```

## 2. Authority tiers

### Tier A — external/source evidence

Facts about what the system received or observed:

- `Event`;
- `DocumentSource` / artifact revision reference;
- tool/model/external receipts;
- human feedback;
- imported/federated source references.

These records do not automatically become true semantic claims; they establish that a particular source payload/fact was recorded.

### Tier B — canonical cognitive state

- `Memory` + `MemoryRevision`;
- `Claim` + claim lifecycle/assessment;
- provenance/derivation links;
- conflicts/supersession;
- decisions, tasks, procedures, preferences, constraints, and other typed cognitive objects.

### Tier C — canonical operational/governance state

- identities/workspaces;
- capability grants/delegations;
- policies/approvals/budgets;
- execution state;
- external effect intents/receipts;
- health findings/repair plans;
- incidents/revalidation;
- classification/retention/share grants.

### Tier D — derived state

- embeddings;
- search documents;
- ranking features;
- summaries that exist only as projections;
- context packs/cache;
- aggregate health snapshots;
- inventory/metrics;
- graph/search projections.

Tier D MUST be rebuildable from more authoritative tiers.

---

# 3. Identity primitives

All durable entities use typed IDs. UUIDv7 is the preferred identity representation for new objects unless a specialized ADR specifies otherwise.

Minimum ID families:

```text
WorkspaceId
PrincipalId
EventId
MemoryId
MemoryRevisionId
ClaimId
EvidenceLinkId
DerivationId
ConflictId
AgentId / AgentRevisionId
SkillId / SkillRevisionId
WorkflowId / WorkflowRevisionId
RunId
StepId
OperationId
ExternalEffectId
HealthFindingId
RepairPlanId
IncidentId
PolicyId / PolicyVersionId
CapabilityGrantId
ShareGrantId
MemoryMountId
ArtifactId / ArtifactRevisionId
```

An ID is an identity/reference and MUST NOT confer authority by itself.

---

# 4. Workspace and actor model

## 4.1 Workspace

`Workspace` — primary authority/isolation boundary.

```text
Workspace
├─ workspace_id
├─ lifecycle_state
├─ policy_set_ref
├─ default_classification
├─ retention_profile_ref
├─ created_at
└─ updated_at
```

Workspace-local `global` scope means the entire current workspace and MUST NOT mean federation-global.

## 4.2 Principal

```text
Principal
├─ principal_id
├─ principal_kind
├─ workspace_memberships[]
├─ status
└─ identity_metadata
```

`principal_kind` includes at least:

- `User`;
- `Agent`;
- `ServiceAccount`;
- `Workflow`;
- `SystemComponent`;
- `FederatedPrincipal`.

Identity and authority are separate: a Principal without capability does not gain operational rights from its type or name.

## 4.3 ActorRef

Domain facts retain the exact actor:

```text
ActorRef
├─ principal_id?
├─ actor_kind
├─ delegated_from?
└─ execution_ref?
```

---

# 5. Event and source evidence

## 5.1 Event

`Event` is an immutable record of a significant observed or received fact.

```text
Event
├─ event_id
├─ workspace_id
├─ event_type
├─ actor_ref
├─ subject_ref?
├─ occurred_at?
├─ recorded_at
├─ correlation_id?
├─ causation_id?
├─ payload_ref / governed payload
├─ classification
└─ integrity_metadata
```

An Event is immutable after recording. A correction creates a new Event.

## 5.2 EvidenceRef

A unified reference to an evidence source:

```text
EvidenceRef =
    EventRef
  | ArtifactRevisionRef
  | DocumentRef
  | MemoryRevisionRef
  | ModelExecutionRef
  | ToolResultRef
  | ExternalEffectReceiptRef
  | HumanFeedbackRef
  | FederatedEvidenceRef
  | ExternalReference
```

`EvidenceRef` does not copy source content; it preserves exact identity/version.

## 5.3 EvidenceRole

Association of evidence with a cognitive statement:

- `Primary`;
- `Supporting`;
- `Contradicting`;
- `Contextual`;
- `DerivedFrom`.

---

# 6. Memory aggregate

## 6.1 Memory

`Memory` is the stable identity of a long-lived cognitive object.

```text
Memory
├─ memory_id
├─ workspace_id
├─ kind
├─ lifecycle_status
├─ active_revision_id?
├─ state_revision
├─ created_at
└─ updated_at
```

Memory does not directly store mutable content. Content belongs to a revision.

## 6.2 MemoryKind

The baseline set is preserved:

- `Fact`;
- `Preference`;
- `Constraint`;
- `Decision`;
- `Task`;
- `Procedure`;
- `Observation`;
- `Outcome`;
- `Summary`.

Add new kinds only when their lifecycle or structured semantics differ, not merely to introduce a different prompt label.

## 6.3 Memory lifecycle

```text
Candidate
├─ Active
└─ Rejected

Active
├─ Superseded
├─ Expired
└─ Deleted
```

`Superseded`, `Expired`, `Rejected`, and `Deleted` MUST NOT silently return to `Active`. Restoring knowledge creates a new revision/Memory according to mutation policy.

## 6.4 MemoryRevision

```text
MemoryRevision
├─ memory_revision_id
├─ memory_id
├─ revision_number
├─ content
├─ structured_payload
├─ valid_from?
├─ valid_until?
├─ confidence
├─ importance
├─ classification
├─ change_reason
├─ created_by
├─ created_at
└─ canonical_hash
```

Rules:

1. revision immutable;
2. revision_number increases monotonically within a Memory;
3. the active revision belongs to the same Memory;
4. a semantic content change always creates a new revision;
5. `valid_until >= valid_from` when both dates are known;
6. confidence and importance are normalized to `[0,1]`;
7. Active Memory MUST have admissible evidence/source;
8. derived Memory MUST have a Derivation.

## 6.5 Structured payloads

`structured_payload` must be a schema-versioned discriminated representation.

Example semantic shapes:

```text
Assertion
  subject / predicate / value

Decision
  choice / rationale / alternatives

Task
  state / assignee / due_at / dependencies

Procedure
  preconditions / steps / expected_result

Preference
  subject / preference / strength / conditions
```

Unrestricted JSON without schema identity is insufficient as a long-lived domain model.

---

# 7. Claim model

## 7.1 Purpose of Claim

`Claim` is used when Vestrace needs to reason about a distinct assertion independently of the original Memory's form.

Memory and Claim are not synonyms:

- Memory is a long-lived cognitive object/lifecycle container;
- Claim is a normalized semantic assertion that can be supported by multiple memories/evidence sources and participate in conflict/reconciliation.

Not every Memory must produce a Claim. Procedures, tasks, and some outcomes can exist without a proposition-level Claim.

## 7.2 Claim

```text
Claim
├─ claim_id
├─ workspace_id
├─ semantic_key
├─ subject
├─ predicate
├─ value
├─ lifecycle_status
├─ valid_from?
├─ valid_until?
├─ state_revision
├─ created_at
└─ updated_at
```

`semantic_key` normalizes the identity of an assertion's subject and supports finding potential conflicts; it MUST NOT serve as universal automatic deduplication without a domain rule.

## 7.3 Claim status

```text
Proposed
Supported
Contested
Superseded
Expired
Rejected
Deleted
```

`Supported` means the claim passed the applicable evidence/policy assessment. It does not mean absolute external truth.

`Contested` means admissible contradictory evidence/claims exist and reconciliation has not resolved them.

## 7.4 ClaimEvidenceLink

```text
ClaimEvidenceLink
├─ link_id
├─ claim_id
├─ evidence_ref
├─ role
├─ evidence_weight_metadata?
├─ source_classification
├─ added_by
└─ created_at
```

Deletion or unavailability of the last admissible supporting evidence MUST trigger claim revalidation.

## 7.5 ClaimAssessment

Assessment of a claim is separate from its assertion text:

```text
ClaimAssessment
├─ assessment_id
├─ claim_id
├─ assessment_kind
├─ confidence
├─ basis_refs[]
├─ policy_version
├─ assessor
└─ created_at
```

The current projection MAY use the latest assessment, while historical assessments remain preserved.

A model-generated assessment is heuristic evidence unless policy explicitly increases its authority.

---

# 8. Provenance and derivation

## 8.1 Derivation

```text
Derivation
├─ derivation_id
├─ workspace_id
├─ method
├─ input_refs[]
├─ output_ref
├─ execution_ref?
├─ model_ref?
├─ prompt/policy version refs?
├─ created_by
└─ created_at
```

Methods include at least:

- `Extraction`;
- `Summarization`;
- `Inference`;
- `Consolidation`;
- `ConflictResolution`;
- `RuleBased`;
- `HumanAuthored`;
- `Import`;
- `FederatedDerivation`.

## 8.2 Provenance closure

Every derived canonical cognitive object must have a path to admissible source evidence or explicit human-authored authority.

Broken provenance closure is an integrity finding.

---

# 9. Conflict and supersession

## 9.1 Conflict

```text
Conflict
├─ conflict_id
├─ workspace_id
├─ conflict_kind
├─ participant_refs[]
├─ status
├─ detected_by
├─ evidence_refs[]
├─ reconciliation_ref?
└─ created_at
```

Status:

`Open / ReconciliationProposed / Resolved / AcceptedAmbiguity / Obsolete`

## 9.2 Conflict rules

- a conflict does not choose its own winner;
- current-state retrieval must account for an Open conflict;
- the Repair Engine does not resolve semantic contradiction;
- reconciliation results preserve their basis, evidence, policy, and human decision;
- resolving a conflict does not delete the losing historical claim/memory.

## 9.3 Supersession

Supersession replaces current applicability without erasing history.

A supersession link MUST identify the replacement object/revision and reason.

---

# 10. Temporal model

## 10.1 Time axes

The domain distinguishes:

```text
occurred_at     — source-world occurrence
recorded_at     — authoritative registration in Vestrace
valid_from/to   — applicability of knowledge
created_at      — creation of domain object/revision
```

These axes must not automatically substitute for one another.

## 10.2 Unknown time

Unknown `occurred_at`, `valid_from`, or `valid_until` is allowed. Unknown MUST be represented as missing data, not a synthetic timestamp.

## 10.3 As-of cognition

Historical queries use revision/claim/conflict state known or applicable under the selected temporal perspective, as defined by the separate temporal-query specification.

---

# 11. Scopes

## 11.1 Scope types

A canonical cognitive object MAY have typed scope relationships:

- workspace;
- project;
- user;
- agent;
- team;
- workflow;
- execution;
- session;
- task;
- workspace-global.

A scope relationship does not expand workspace authority.

## 11.2 Scope matching

Retrieval scope matching concerns ranking/filtering, but authorization MUST be checked independently.

---

# 12. Cognitive assets

## 12.1 AgentDefinition

```text
Agent
└─ AgentRevision
   ├─ purpose/role
   ├─ instructions
   ├─ model requirements
   ├─ skill/tool references
   ├─ requested capability template refs
   ├─ memory/context policy refs
   └─ budget policy refs
```

An Agent revision does not contain its own effective runtime authority. The governance layer grants effective capabilities.

## 12.2 SkillDefinition

A Skill revision contains:

- purpose/instructions;
- input/output schemas;
- dependency refs;
- required capability template;
- implementation kind/reference;
- conditions/examples;
- version/integrity metadata.

## 12.3 WorkflowDefinition

A workflow definition remains a versioned typed graph/plan asset, separate from execution history.

---

# 13. Execution domain references

Domain Model v0.2 preserves the Run-first execution hierarchy:

```text
AgentRun
├─ ExecutionPlanRevision
├─ RunStep[]
├─ RunEvent[]
├─ RunCheckpoint[]
└─ typed operation references
   ├─ ModelExecution
   ├─ ToolInvocation
   ├─ ExternalEffect
   ├─ SubRun
   ├─ RemoteAgentInvocation
   ├─ HumanRequest
   └─ VerificationAttempt
```

No generic mutable `Task → Action → Attempt` hierarchy is introduced.

Task remains a valid `MemoryKind`, but is not an execution root.

---

# 14. Execution evidence and feedback

## 14.1 ExecutionOutcome

Every significant execution completion retains a typed outcome/reference:

- success;
- partial success;
- failed;
- cancelled;
- unknown/ambiguous when the outcome cannot be established.

## 14.2 Evaluation

```text
Evaluation
├─ evaluation_id
├─ target_execution_ref
├─ evaluator_kind
├─ metric/check kind
├─ result
├─ evidence_refs[]
├─ evaluator_revision?
└─ created_at
```

Evaluator kinds:

- deterministic;
- human;
- heuristic;
- model judge.

Authority is determined by policy, not the evaluator's name.

## 14.3 Learned state

Performance summaries/recommendations are separate from raw evaluation facts and preserve provenance.

---

# 15. Capability and policy domain

## 15.1 CapabilityGrant

```text
CapabilityGrant
├─ grant_id
├─ subject_principal
├─ issuer
├─ operation/tool selector
├─ resource selector
├─ constraints
│  ├─ valid_from/until
│  ├─ budget
│  ├─ risk_ceiling
│  └─ conditions
├─ delegation_policy
├─ status
└─ audit refs
```

A role is a template/issuance convenience, not runtime authority.

## 15.2 DelegatedCapability

Child authority is calculated as the intersection of:

```text
parent effective authority
∩ explicit delegation
∩ child policy constraints
```

The result cannot exceed the parent's authority.

## 15.3 PolicyDecision

```text
PolicyDecision
├─ decision_id
├─ policy_id/version
├─ subject
├─ operation
├─ resource/scope
├─ input_state_ref
├─ risk
├─ result
├─ obligations[]
└─ decided_at
```

Result:

`DENY / ALLOW / PREPARE_ONLY / REQUIRE_APPROVAL`

## 15.4 ApprovalRecord

Approval binds an immutable operation/intent hash, scope, approver, and expiry. Changing material parameters invalidates approval.

---

# 16. Cross-workspace sharing

## 16.1 MemoryShareGrant

Source-owned governed disclosure relationship.

Lifecycle:

`Draft / PendingAcceptance / Active / Suspended / Revoked / Expired / Deleted`

The target is always an exact workspace; wildcards are prohibited.

## 16.2 MemoryShareGrantRevision

An immutable revision records:

- selection contract;
- kinds/statuses/classification ceiling;
- content mode;
- discover/read/context/model/derive/export permissions;
- provider/indexing policy;
- limits/expiry;
- policy/approval refs.

## 16.3 MemoryMount

Target-owned acceptance exact grant revision.

Lifecycle:

`Pending / Active / Suspended / Stale / Revoked / Expired / Deleted`

A stale mount discloses no content until revalidation/reacceptance.

## 16.4 SharedMemoryRef

```text
SharedMemoryRef
├─ source_workspace_id
├─ memory_id
├─ memory_revision_id
├─ grant_revision_id
└─ source_generation
```

It is not a local `MemoryId`, capability, or foreign key into a local memory aggregate.

## 16.5 Local derivation/import

Mounted content does not automatically become local authority. Local derived knowledge requires a `MemoryImportProposal`; after approval, the ordinary local Memory lifecycle applies with full provenance.

---

# 17. Artifact domain

## 17.1 Artifact

```text
Artifact
├─ artifact_id
├─ workspace_id
├─ artifact_kind
├─ lifecycle_state
├─ active_revision_id?
└─ metadata
```

## 17.2 ArtifactRevision

```text
ArtifactRevision
├─ artifact_revision_id
├─ artifact_id
├─ media_type
├─ content_hash
├─ size
├─ storage_ref
├─ classification
├─ provenance
└─ created_at
```

Artifact bytes MAY reside in local CAS. `content_hash` identifies bytes/integrity, not permission.

Human-readable virtual paths/aliases are presentation mappings, not replacements for typed identity.

---

# 18. External effect domain

## 18.1 ExternalEffectIntent

```text
ExternalEffectIntent
├─ effect_id
├─ execution_ref
├─ actor
├─ adapter/tool
├─ operation
├─ target
├─ arguments_digest
├─ expected_effect
├─ preconditions[]
├─ risk
├─ reversibility
├─ idempotency_profile_ref
├─ required_capability
└─ created_at
```

An intent is immutable after authorization.

## 18.2 ExternalEffectReceipt

```text
ExternalEffectReceipt
├─ effect_id
├─ provider
├─ dispatched_at
├─ transport_result
├─ external_resource_id?
├─ external_version?
├─ response_digest?
├─ outcome_status
└─ evidence_refs[]
```

Outcome status supports `UNKNOWN`.

---

# 19. Health and repair domain

## 19.1 InvariantDefinition

```text
InvariantDefinition
├─ invariant_id
├─ domain
├─ scope_kind
├─ description
├─ default_severity
├─ checker_kind
├─ repair_strategy
├─ check_modes[]
└─ version
```

## 19.2 HealthFinding

```text
HealthFinding
├─ finding_id
├─ invariant_id
├─ fingerprint
├─ scope
├─ severity
├─ impact
├─ evidence_refs[]
├─ repairability
├─ lifecycle_status
├─ first_seen_at
└─ last_seen_at
```

## 19.3 HealthOccurrence

A distinct episode of a logical finding, with detected state, evidence, and time.

## 19.4 RepairPlan

```text
RepairPlan
├─ plan_id
├─ finding_ids[]
├─ input_state_ref
├─ scope
├─ preconditions[]
├─ operations[]
├─ expected_postconditions[]
├─ verification_checks[]
├─ risk
├─ reversibility
├─ required_capability
├─ created_at
└─ expires_at
```

Plan immutable.

## 19.5 VerificationRun

Repair and finding closure are separate. `VerificationRun` retains checks, evidence, and result; only successful verification permits `RESOLVED`.

---

# 20. Incident and recovery domain

## 20.1 Incident

```text
Incident
├─ incident_id
├─ type
├─ severity
├─ scope
├─ triggering_findings[]
├─ affected_resources[]
├─ status
├─ containment_state
├─ recovery_state
└─ revalidation_state
```

## 20.2 TrustState

Scope projection:

`TRUSTED / DEGRADED_TRUST / UNTRUSTED / REVALIDATING`

## 20.3 RecoveryPoint

```text
RecoveryPoint
├─ state_ref
├─ sequence/event_position
├─ scope
├─ integrity_status
├─ provenance
└─ created_at
```

## 20.4 RevalidationRun

```text
RevalidationRun
├─ run_id
├─ incident_id?
├─ scope
├─ level
├─ baseline_state_ref
├─ checks[]
├─ evidence_refs[]
├─ result
└─ completed_at
```

Result:

`PASSED / PASSED_WITH_DEGRADATION / FAILED / INCONCLUSIVE`

---

# 21. Governance domain

## 21.1 DataClassification

```text
DataClassification
├─ sensitivity
├─ categories[]
├─ jurisdiction_tags[]
├─ handling_requirements[]
└─ provenance
```

Sensitivity baseline:

`PUBLIC < INTERNAL < CONFIDENTIAL < RESTRICTED`

## 21.2 SecretRef

Secret material is not a Memory/Artifact payload. Durable state stores an opaque reference and safe metadata.

## 21.3 DataPolicy

```text
DataPolicy
├─ policy_id/version
├─ applies_to
├─ classification_constraints
├─ retention_rule
├─ export_rule
├─ deletion_rule
├─ residency_rule
├─ sharing_rule
└─ purpose_constraints
```

## 21.4 DataHold

A separate governed object that temporarily blocks disposal for an exact scope, reason, and authority.

## 21.5 DeletionRequest / DeletionPlan

Deletion is a dependency-aware workflow that ends with verification, not merely a successful SQL delete.

## 21.6 DataExportPlan / ExportBundle

Export records exact scope, purpose, recipient, redaction/classification, encryption/signing, provenance, and manifest.

---

# 22. Derived projections

The following objects are not independent semantic authorities:

- `SearchDocument`;
- embeddings;
- retrieval candidates/ranking scores;
- `ContextPack` cache;
- graph projection;
- `HealthSnapshot`;
- `DataInventory`;
- aggregate metrics;
- convenience read models.

A projection MAY be durable for performance/audit, but must have a source generation/version and a rebuild path.

---

# 23. Cross-domain reference rules

1. A typed reference SHOULD preserve an exact revision when semantic reproducibility depends on version.
2. An ID/URI/hash MUST NOT serve as authorization proof.
3. Cross-workspace references are namespaced and do not become local IDs.
4. A derived projection must not create ownership over its source object.
5. Deletion/revocation MAY leave a content-free tombstone/reference for audit/provenance.
6. Secrets are never copied into provenance payloads.

---

# 24. Aggregate ownership summary

| Aggregate / entity | Authority owner | Mutable identity | Immutable revisions/facts |
|---|---|---:|---:|
| Workspace | Workspace governance | yes | policy/history refs |
| Memory | Memory Core | yes | MemoryRevision |
| Claim | Cognition Core | yes | assessments/evidence links are append-oriented |
| Event | Evidence Core | no | Event itself |
| Agent/Skill/Workflow | Cognitive Assets | yes | revisions |
| AgentRun | Execution Runtime | yes | RunEvents/plan revisions/checkpoints |
| CapabilityGrant | Governance | yes | decisions/audit history |
| MemoryShareGrant | Source workspace | yes | grant revisions |
| MemoryMount | Target workspace | yes | acceptance/history refs |
| Artifact | Artifact Core | yes | ArtifactRevision |
| ExternalEffectIntent | Execution boundary | no after prepare/authorize | receipt/outcome history |
| HealthFinding | Integrity | yes lifecycle | occurrences/evidence |
| RepairPlan | Integrity | no | plan itself |
| Incident | Incident response | yes lifecycle | recovery/revalidation facts |
| DataPolicy | Governance | versioned identity | policy versions |

---

# 25. Forbidden conflations

The following pairs MUST remain distinct:

```text
Memory != Evidence
Memory != Claim
Claim != Truth
Role != Capability
Identity != Authority
Scope != Permission
Hash != Permission
Receipt != Confirmed Outcome
Repair Success != Finding Resolution
Health != Trust
Recovery != Revalidation
Encryption != Governance
Mount != Local Memory
Compensation != Rollback
Projection != Source of Truth
```

---

# 26. Follow-up specifications

The following documents must elaborate this model without changing its foundational boundaries:

1. Normative Invariants Catalog;
2. Trust & Authority Model;
3. Data & Temporal Model;
4. Execution & External Effects Contract;
5. Health / Repair / Incident Contract;
6. Crypto & Data Governance Contract;
7. Qualification / Conformance Specification.

Every new entity in these documents must have a clear authority owner or be identified as a projection/value object. Creating another independent runtime aggregate requires an ADR.

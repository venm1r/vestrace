# Vestrace Domain Model v0.2

**Статус:** normative domain model draft  
**Дата:** 2026-08-10  
**Базовый контракт:** `docs/specs/vestrace-architecture-contract-v0.2.md`

> Этот документ определяет целевую доменную модель. Он не утверждает, что перечисленные агрегаты уже реализованы в текущем `main`.

## 1. Цель модели

Domain Model v0.2 должен обеспечить единый язык для persistent cognition, provenance, temporal state, governed execution и trust без создания параллельных runtime-моделей.

Основное разделение:

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

Факты о том, что система получила/наблюдала:

- `Event`;
- `DocumentSource` / artifact revision reference;
- tool/model/external receipts;
- human feedback;
- imported/federated source references.

Эти записи не становятся истинными semantic claims автоматически; они являются доказательством того, что определённый source payload/fact был зафиксирован.

### Tier B — canonical cognitive state

- `Memory` + `MemoryRevision`;
- `Claim` + claim lifecycle/assessment;
- provenance/derivation links;
- conflicts/supersession;
- decisions/tasks/procedures/preferences/constraints и другие typed cognitive objects.

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
- summaries, когда они существуют только как projection;
- context packs/cache;
- aggregate health snapshots;
- inventory/metrics;
- graph/search projections.

Tier D MUST быть rebuildable из более авторитетных tiers.

---

# 3. Identity primitives

Все durable entities используют typed IDs. UUIDv7 является предпочтительным представлением identity для новых объектов, если специализированный ADR не устанавливает иное.

Минимальные ID families:

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

ID является identity/reference и MUST NOT сам по себе предоставлять authority.

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

Workspace-local `global` scope означает весь текущий workspace и MUST NOT означать federation-global.

## 4.2 Principal

```text
Principal
├─ principal_id
├─ principal_kind
├─ workspace_memberships[]
├─ status
└─ identity_metadata
```

`principal_kind` как минимум:

- `User`;
- `Agent`;
- `ServiceAccount`;
- `Workflow`;
- `SystemComponent`;
- `FederatedPrincipal`.

Identity и authority разделены: Principal без capability не получает операционных прав только благодаря типу/имени.

## 4.3 ActorRef

Domain facts сохраняют точного actor:

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

`Event` — immutable registration значимого наблюдаемого/полученного факта.

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

После записи Event не изменяется. Correction создаёт новый Event.

## 5.2 EvidenceRef

Unified reference на evidence source:

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

`EvidenceRef` не копирует содержимое source; он сохраняет точную identity/version.

## 5.3 EvidenceRole

Связь evidence с cognitive statement:

- `Primary`;
- `Supporting`;
- `Contradicting`;
- `Contextual`;
- `DerivedFrom`.

---

# 6. Memory aggregate

## 6.1 Memory

`Memory` — стабильная identity долговременного когнитивного объекта.

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

Memory не хранит mutable content непосредственно. Содержание принадлежит revision.

## 6.2 MemoryKind

Базовый набор сохраняется:

- `Fact`;
- `Preference`;
- `Constraint`;
- `Decision`;
- `Task`;
- `Procedure`;
- `Observation`;
- `Outcome`;
- `Summary`.

Новые kinds добавляются только если у них есть отличимые lifecycle/structured semantics, а не только иной prompt label.

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

`Superseded`, `Expired`, `Rejected`, `Deleted` MUST NOT молча возвращаться в `Active`. Восстановление знания создаёт новую revision/Memory согласно mutation policy.

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

Правила:

1. revision immutable;
2. revision_number монотонен внутри Memory;
3. active revision принадлежит той же Memory;
4. semantic content change всегда создаёт новую revision;
5. `valid_until >= valid_from`, если обе даты известны;
6. confidence/importance нормализованы в `[0,1]`;
7. Active Memory MUST иметь admissible evidence/source;
8. derived Memory MUST иметь Derivation.

## 6.5 Structured payloads

`structured_payload` должен быть schema-versioned discriminated representation.

Примеры semantic shapes:

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

Свободный JSON без schema identity не является достаточной долгосрочной domain model.

---

# 7. Claim model

## 7.1 Назначение Claim

`Claim` используется там, где Vestrace должен рассуждать об отдельном утверждении независимо от формы исходной Memory.

Memory и Claim не являются синонимами:

- Memory — долговременный cognitive object/lifecycle container;
- Claim — нормализованное semantic assertion, которое может поддерживаться несколькими memories/evidence sources и участвовать в conflict/reconciliation.

Не каждая Memory обязана порождать Claim. Процедуры, задачи и некоторые outcomes могут существовать без proposition-level Claim.

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

`semantic_key` нормализует identity предмета утверждения и используется для поиска potential conflicts; он MUST NOT использоваться как универсальная автоматическая дедупликация без domain rule.

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

`Supported` означает, что claim прошёл действующую evidence/policy evaluation. Это не абсолютная внешняя истина.

`Contested` означает наличие admissible противоречащего evidence/claim, которое reconciliation ещё не разрешило.

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

Удаление/недоступность последнего admissible supporting evidence MUST инициировать claim revalidation.

## 7.5 ClaimAssessment

Оценка claim отделена от самого текста утверждения:

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

Последняя assessment MAY использоваться current projection, но historical assessments сохраняются.

Model-generated assessment является heuristic evidence, если policy явно не повышает его authority.

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

Methods как минимум:

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

Для каждого derived canonical cognitive object должен существовать путь до admissible source evidence либо явной human-authored authority.

Broken provenance closure является integrity finding.

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

- conflict не выбирает winner сам;
- retrieval current-state обязан учитывать Open conflict;
- semantic contradiction не исправляется Repair Engine;
- reconciliation result сохраняет basis/evidence/policy/human decision;
- resolved conflict не удаляет проигравший historical claim/memory.

## 9.3 Supersession

Supersession означает замену current applicability, но не стирание истории.

Supersession link MUST указывать заменяющий object/revision и reason.

---

# 10. Temporal model

## 10.1 Time axes

Domain различает:

```text
occurred_at     — source-world occurrence
recorded_at     — authoritative registration in Vestrace
valid_from/to   — applicability of knowledge
created_at      — creation of domain object/revision
```

Эти axes нельзя автоматически подменять друг другом.

## 10.2 Unknown time

Unknown `occurred_at`, `valid_from` или `valid_until` допустимы. Unknown MUST быть представлено как отсутствие данных, а не синтетическим timestamp.

## 10.3 As-of cognition

Historical query использует состояние revisions/claims/conflicts, известное/действовавшее для выбранного temporal perspective согласно отдельной temporal-query specification.

---

# 11. Scopes

## 11.1 Scope types

Canonical cognitive object MAY иметь typed scope relations:

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

Scope relation не расширяет workspace authority boundary.

## 11.2 Scope matching

Retrieval scope matching является ranking/filter concern, но authorization MUST быть проверен независимо.

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

Agent revision не содержит собственную фактическую runtime authority. Effective capabilities выдаются governance layer.

## 12.2 SkillDefinition

Skill revision содержит:

- purpose/instructions;
- input/output schemas;
- dependency refs;
- required capability template;
- implementation kind/reference;
- conditions/examples;
- version/integrity metadata.

## 12.3 WorkflowDefinition

Workflow definition остаётся versioned typed graph/plan asset. Definition отделена от execution history.

---

# 13. Execution domain references

Domain Model v0.2 сохраняет Run-first execution hierarchy:

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

Generic mutable `Task → Action → Attempt` hierarchy не вводится.

Task продолжает существовать как допустимый `MemoryKind`, но не является execution root.

---

# 14. Execution evidence and feedback

## 14.1 ExecutionOutcome

Каждое значимое execution завершение сохраняет typed outcome/reference:

- success;
- partial success;
- failed;
- cancelled;
- unknown/ambiguous там, где outcome нельзя доказать.

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

Authority определяется policy, а не названием evaluator.

## 14.3 Learned state

Performance summaries/recommendations сохраняются отдельно от raw evaluation facts и имеют provenance.

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

Role — template/issuance convenience, не runtime authority.

## 15.2 DelegatedCapability

Child authority вычисляется как intersection:

```text
parent effective authority
∩ explicit delegation
∩ child policy constraints
```

Результат не может быть шире parent authority.

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

Approval связывается с immutable operation/intent hash, scope, approver и expiry. Изменение material parameters инвалидирует approval.

---

# 16. Cross-workspace sharing

## 16.1 MemoryShareGrant

Source-owned governed disclosure relationship.

Lifecycle:

`Draft / PendingAcceptance / Active / Suspended / Revoked / Expired / Deleted`

Target всегда exact workspace; wildcard запрещён.

## 16.2 MemoryShareGrantRevision

Immutable revision фиксирует:

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

Stale mount не выдаёт content до revalidation/re-accept.

## 16.4 SharedMemoryRef

```text
SharedMemoryRef
├─ source_workspace_id
├─ memory_id
├─ memory_revision_id
├─ grant_revision_id
└─ source_generation
```

Не является local `MemoryId`, capability или FK в local memory aggregate.

## 16.5 Local derivation/import

Mounted content не становится local authority автоматически. Для локального производного знания создаётся `MemoryImportProposal`, после approval выполняется обычный local Memory lifecycle с full provenance.

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

Artifact bytes MAY находиться в local CAS. `content_hash` идентифицирует bytes/integrity, но не permission.

Human-readable virtual paths/aliases являются presentation mapping и не заменяют typed identity.

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

Intent immutable после authorization.

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

Outcome status поддерживает `UNKNOWN`.

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

Отдельный episode одного logical finding с detected state/evidence/time.

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

Repair и finding closure разделены. `VerificationRun` сохраняет checks/evidence/result и только после успеха позволяет `RESOLVED`.

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

Secret material не является Memory/Artifact payload. Durable state хранит opaque reference + safe metadata.

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

Отдельный governed object, временно блокирующий disposal для exact scope/reason/authority.

## 21.5 DeletionRequest / DeletionPlan

Deletion является dependency-aware workflow и завершается verification, а не только успешным SQL delete.

## 21.6 DataExportPlan / ExportBundle

Export фиксирует exact scope, purpose, recipient, redaction/classification, encryption/signing, provenance и manifest.

---

# 22. Derived projections

Следующие объекты не являются самостоятельной semantic authority:

- `SearchDocument`;
- embeddings;
- retrieval candidates/ranking scores;
- `ContextPack` cache;
- graph projection;
- `HealthSnapshot`;
- `DataInventory`;
- aggregate metrics;
- convenience read models.

Projection MAY быть durable для производительности/аудита, но должна иметь source generation/version и rebuild path.

---

# 23. Cross-domain reference rules

1. Typed reference SHOULD сохранять точную revision, когда semantic reproducibility зависит от версии.
2. ID/URI/hash MUST NOT использоваться как authorization proof.
3. Cross-workspace refs namespaced и не превращаются в local IDs.
4. Derived projection не должна создавать ownership над source object.
5. Deletion/revocation MAY оставлять content-free tombstone/reference для audit/provenance.
6. Secrets никогда не копируются в provenance payload.

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

Следующие пары MUST оставаться различимыми:

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

Эта модель должна быть конкретизирована следующими документами без изменения базовых границ:

1. Normative Invariants Catalog;
2. Trust & Authority Model;
3. Data & Temporal Model;
4. Execution & External Effects Contract;
5. Health / Repair / Incident Contract;
6. Crypto & Data Governance Contract;
7. Qualification / Conformance Specification.

Любая новая сущность в этих документах должна либо иметь ясного authority owner, либо быть признана projection/value object. Создание нового самостоятельного runtime aggregate требует ADR.

# Vestrace Qualification / Conformance Specification v0.2

**Статус:** normative qualification specification  
**Дата:** 2026-08-10

## 1. Назначение

Этот документ определяет, как Vestrace доказывает соответствие Architecture Contract и когда build/deployment имеет право заявлять конкретный conformance/trust profile.

Главный принцип:

> **A feature claim requires a verifiable contract. A trust claim requires reproducible evidence.**

---

# 2. Three verification layers

```text
Tests
  ↓
Conformance
  ↓
Qualification
```

## 2.1 Tests

Unit/integration/contract tests проверяют конкретную реализацию.

## 2.2 Conformance

Проверяет observable behavior компонента/adapter против stable normative requirement IDs.

## 2.3 Qualification

Проверяет, может ли конкретный:

- source revision/build;
- configuration;
- environment;
- adapter set;
- policy/crypto profile

считаться пригодным для заявленного profile.

---

# 3. Requirement source

Primary requirement source:

`docs/specs/vestrace-normative-invariants-v0.2.md`

Каждый MUST requirement в profile dependency closure должен иметь:

- requirement ID;
- verification method;
- expected result semantics;
- evidence output;
- applicability rule.

---

# 4. Requirement traceability

```text
NormativeRequirement
→ Implementation Surface
→ Conformance Case
→ Test/Scenario Run
→ Evidence
→ Qualification Result
```

Implementation surface MAY быть source file/module/adapter/schema/runtime path, но архитектурная документация не фиксирует Rust paths до implementation planning phase.

---

# 5. Conformance profiles

## 5.1 CORE

Минимальная platform correctness:

- canonical vs derived boundaries;
- identity/versioning;
- execution history basics;
- default-deny governance fundamentals;
- invariant enforcement basics.

## 5.2 MEMORY

Добавляет:

- Memory/Revision;
- evidence/provenance;
- temporal correctness;
- conflict/supersession;
- retrieval/ContextPack properties;
- rebuildable derived memory state.

## 5.3 COGNITION

Добавляет:

- Claim/evaluation semantics;
- feedback/learning provenance;
- cognitive benchmarks/properties;
- asset/version learning gates.

## 5.4 AUTONOMY

Добавляет:

- governed execution;
- external effects;
- idempotency/reconciliation;
- repair;
- crash recovery.

## 5.5 FEDERATION

Добавляет:

- cross-workspace grants/mounts;
- federation identity/trust;
- remote qualification evidence;
- cross-boundary governance.

## 5.6 TRUSTED

Добавляет полный:

- health/integrity/repair;
- incident/revalidation;
- crypto/data governance;
- audit integrity profile;
- deployment qualification;
- known limitations;
- fault/security/recovery hard gates.

---

# 6. Profile dependencies

Base chain:

```text
CORE
 ↓
MEMORY
 ↓
COGNITION
 ↓
AUTONOMY
 ↓
TRUSTED
```

Federation branch:

```text
MEMORY
 ↓
FEDERATION
 ↓
TRUSTED-FEDERATED
```

`TRUSTED-FEDERATED` означает TRUSTED + FEDERATION requirements.

Profile implementation MAY support subsets/features beyond profile, но qualification claim применяется только к прошедшему dependency closure.

---

# 7. Capability Manifest

Deployment/build SHOULD публиковать machine-readable manifest:

```text
VestraceCapabilityManifest
├─ product/version
├─ source revision
├─ build digest
├─ schema versions
├─ supported profiles
├─ optional features
├─ storage backends
├─ crypto suites/providers
├─ model/provider boundaries
├─ external effect adapters
├─ federation capabilities
└─ known limitations
```

Manifest является assertion; qualification evidence определяет, подтверждён ли assertion.

---

# 8. Qualification target identity

Qualification target включает:

```text
source_revision
build_digest
configuration_digest
schema_state
runtime dependencies
storage backend versions
crypto/key provider profile
model/provider adapters
external effect adapters
policy bundle/version
OS/container/runtime environment
```

Изменение material component MAY сделать предыдущую qualification stale.

---

# 9. Conformance case categories

```text
STATIC
BEHAVIORAL
STATEFUL
FAULT
SECURITY
RECOVERY
INTEROPERABILITY
EVIDENCE
```

## STATIC

Schemas/manifests/invariant registries/config declarations.

## BEHAVIORAL

Single-operation observable behavior.

## STATEFUL

Sequences of mutations/transitions.

## FAULT

Controlled deterministic fault injection.

## SECURITY

Positive/negative authorization, isolation, data-governance cases.

## RECOVERY

Crash/restore/rebuild/revalidation.

## INTEROPERABILITY

Adapter/protocol/schema/federation compatibility.

## EVIDENCE

Проверка обязательного provenance/audit/receipt/verification evidence.

---

# 10. Hard gates vs metrics

## 10.1 HARD GATE

Binary safety/correctness requirement.

Examples:

- unauthorized cross-workspace read = zero tolerated;
- skipped required MUST = failure;
- unsafe retry after UNKNOWN irreversible effect = failure;
- RESTRICTED leakage to forbidden provider = failure;
- TRUSTED without revalidation evidence = failure.

## 10.2 METRIC

Quality/performance threshold:

- retrieval recall;
- latency;
- context efficiency;
- benchmark success rate.

High metric score не компенсирует hard-gate failure.

---

# 11. Golden scenario suites

## 11.1 Memory evolution

```text
record fact A
→ activate
→ later corrected by fact B
→ supersede/reconcile
→ current retrieval returns B
→ historical retrieval preserves A
```

Checks:

- immutable history;
- provenance;
- temporal semantics;
- no stale current truth.

## 11.2 Late-arriving evidence

```text
claim supported at T1
→ contradictory event occurred T0 but recorded T2
→ current claim contested/reassessed
→ KNOWN_AS_OF(T1) differs from reconstructed history
```

## 11.3 Context budget

ContextPack never exceeds hard budget and preserves source refs after compression.

## 11.4 Workspace isolation

Foreign workspace UUID guessing, alternate API surface, direct retriever path и cache path all deny unauthorized disclosure.

## 11.5 Delegation attenuation

Child requests broader resource/risk/capability than parent delegation and is denied.

## 11.6 Share revoke

Source revoke immediately invalidates future target reads/cache use according to contract while preserving audit history.

## 11.7 Repair projection drift

Derived index corrupted → finding → deterministic rebuild → verification → resolved; canonical Memory unchanged.

## 11.8 Stale RepairPlan

State changes after plan authorization → execution returns stale/no mutation.

## 11.9 External effect crash

```text
intent persisted
→ authorized
→ dispatched
→ crash before receipt
→ restart
→ UNKNOWN
→ reconciliation
```

No unsafe retry.

## 11.10 Compensation

Compensation remains separate effect and original effect remains history.

## 11.11 Recovery/revalidation

Restore snapshot + replay + rebuild does not regain TRUSTED until required RevalidationRun passes.

## 11.12 Crypto anomaly

Tampered signed/audit object causes trust-impacting finding/incident; system does not silently accept/re-sign history.

## 11.13 Evidence deletion

Deleting sole admissible evidence causes dependent claim revalidation and retrieval behavior update.

## 11.14 Provider data boundary

Forbidden classification never reaches remote provider through direct path, cached ContextPack, summary, fallback or retry path.

---

# 12. Deterministic fault injection

Qualification harness SHOULD support named fault points, examples:

```text
FAULT_AFTER_EVENT_APPEND
FAULT_BEFORE_PROJECTION_COMMIT
FAULT_AFTER_EFFECT_INTENT_PERSIST
FAULT_AFTER_EFFECT_DISPATCH
FAULT_BEFORE_EFFECT_RECEIPT_PERSIST
FAULT_AFTER_REPAIR_STEP
FAULT_BEFORE_REPAIR_VERIFICATION
FAULT_AFTER_SNAPSHOT_RESTORE
FAULT_DURING_EVENT_REPLAY
FAULT_BEFORE_TRUST_TRANSITION
```

Fault point semantics/version SHOULD быть stable enough for reproducible scenario suites.

---

# 13. Chaos testing

Randomized chaos MAY включать:

- worker termination;
- dependency restarts;
- network latency/loss;
- provider timeout;
- resource pressure;
- clock skew within defined bounds.

Chaos suite не заменяет deterministic fault cases, потому что failure должен быть reproducible.

---

# 14. Security negative tests

Обязательные families:

- expired/revoked capability denied;
- child authority escalation denied;
- approval for modified intent invalid;
- workspace A cannot read B;
- mounted content cannot be re-shared without explicit authority;
- restricted data cannot use forbidden provider;
- read permission does not imply export/model-use;
- admin does not bypass federation boundary;
- alternative HTTP/MCP/CLI/worker path does not bypass same policy;
- stale trust state restricts prohibited effect classes.

---

# 15. Policy bypass matrix

Каждый governed operation SHOULD быть проверен через доступные surfaces:

```text
HTTP
MCP
CLI
worker/background job
internal replay/recovery
federation adapter
```

Если surface доступен, он не должен обходить application governance boundary.

---

# 16. Cognitive qualification

## 16.1 Property-oriented evaluation

LLM wording nondeterministic; qualification проверяет properties:

- required evidence retrieved;
- stale fact excluded;
- current temporal state correct;
- provenance complete;
- forbidden data absent;
- conflict surfaced;
- token budget observed.

## 16.2 Benchmark families

```text
Recall
Temporal Recall
Contradiction Resolution
Provenance
Long-Horizon Memory
Context Selection
Cross-Session Continuity
Forgetting
Access Isolation
Shared Cognition
```

## 16.3 Metrics

Possible metrics:

- Recall@k;
- Precision@k;
- MRR;
- provenance coverage;
- stale-memory rate;
- unresolved-conflict masking rate;
- context utilization;
- restricted-data leakage count.

Security leakage metrics generally become hard gate (`0`) rather than average score.

---

# 17. Migration qualification

Migration scenario:

```text
old supported state
→ apply migrations
→ new state
→ invariants
→ historical semantics
→ derived rebuild
→ revalidation
```

Checks:

- data preservation;
- event/history continuity;
- compatibility declarations;
- current-state correctness;
- rollback/recovery policy where defined.

---

# 18. Schema compatibility

Public/persisted versioned contracts MAY include:

- domain event schemas;
- export bundle formats;
- OpenAPI;
- MCP tool/resource schemas;
- federation protocol;
- artifact manifests.

Compatibility policy must be explicit:

- backward compatible;
- forward compatible;
- both;
- version-gated unsupported.

Silent best-effort parsing запрещён для trust-critical persisted facts.

---

# 19. Adapter qualification

Каждый external adapter заявляет properties и проходит соответствующие cases.

## 19.1 External effect adapter

Checks:

- authorization respected;
- redaction/classification respected;
- idempotency declaration matches observed behavior/contract;
- timeout => correct UNKNOWN/failed semantic;
- receipt behavior;
- reconciliation;
- cancellation;
- dry-run classification;
- secret handling.

## 19.2 Storage backend

Checks:

- atomicity contract;
- durability claims;
- concurrency;
- CAS/integrity verification;
- recovery;
- encryption profile;
- deletion semantics.

## 19.3 Model provider

Checks:

- classification/locality boundary;
- request redaction/minimization;
- model identity/version capture;
- usage/budget;
- timeout/fallback semantics;
- provenance.

---

# 20. QualificationBundle

```text
QualificationBundle
├─ qualification_id
├─ target_manifest
├─ source_revision
├─ build_digest
├─ configuration_digest
├─ environment_manifest
├─ profile
├─ suite_version
├─ requirement_results[]
├─ scenario_results[]
├─ metrics[]
├─ failures[]
├─ skipped[]
├─ known_limitations[]
├─ evidence_refs[]
├─ started_at
├─ completed_at
└─ signature?
```

## 20.1 Requirement result

```text
PASS
FAIL
NOT_APPLICABLE
SKIPPED
INCONCLUSIVE
```

Required MUST + `SKIPPED`/`INCONCLUSIVE` => profile not qualified.

---

# 21. Signed qualification

TRUSTED deployments MAY require signed QualificationBundle.

Signature proves bundle integrity/issuer identity, но relying party всё равно решает, доверяет ли signer/suite/profile.

---

# 22. Qualification lifecycle

```text
PRE_MERGE
RELEASE
DEPLOYMENT
PERIODIC
POST_INCIDENT
```

## PRE_MERGE

Fast required subset for changed components.

## RELEASE

Full target release profile.

## DEPLOYMENT

Environment-specific adapter/config validation.

## PERIODIC

Drift/key/policy/provider checks.

## POST_INCIDENT

Targeted requalification affected profile/scope after recovery/revalidation.

---

# 23. Baseline drift

QualificationBaseline stores material target attributes.

Changes MAY cause:

```text
QUALIFIED
STALE
INVALIDATED
FAILED
```

Typical invalidators:

- storage backend change;
- crypto provider/key policy change;
- external adapter version change;
- schema migration;
- policy engine semantic change;
- federation protocol change;
- security-critical config drift.

---

# 24. No permanent certification

Qualification относится к exact target identity, а не к абстрактному продукту навсегда.

`Vestrace vX is qualified` без profile/build/environment scope является недостаточно точным claim.

---

# 25. Release gates

## 25.1 v0.2 — Correct

Target:

- CORE + MEMORY;
- temporal/mutation correctness;
- provenance;
- retrieval/ContextPack hard properties;
- basic recovery/rebuild deterministic state.

## 25.2 v0.3 — Learn

Target:

- COGNITION;
- evaluation/learning provenance;
- cognition benchmark baseline;
- no silent self-modification of authority.

## 25.3 v0.4 — Govern

Target:

- full capability governance;
- workspace isolation;
- cross-workspace/federation governance;
- policy bypass negative suite.

## 25.4 v0.5 — Understand

Target:

- health/invariants;
- RepairPlan/verification;
- flapping/recurrent failure;
- operator diagnostics contract.

## 25.5 v0.6 — Connect

Target:

- external effects;
- adapter qualification;
- UNKNOWN/reconciliation/crash semantics;
- interoperability.

## 25.6 v1.0 — Trust

Requires TRUSTED profile:

- recovery/revalidation;
- crypto/data governance;
- audit integrity according to configured profile;
- complete hard-gate conformance;
- fault injection/recovery suite;
- deployment qualification;
- published known limitations.

`v1.0` определяется Trust Contract, а не feature count.

---

# 26. Known limitations

Qualification MUST позволять честно заявлять limitations.

Example:

```text
SMTP adapter can confirm provider acceptance but cannot independently prove recipient delivery.
```

Это не qualification failure, если profile contract допускает такую limitation и она явно объявлена; запрещено заявлять более сильную guarantee.

---

# 27. Qualification findings

Runtime drift/conformance checks MAY создавать обычные HealthFindings:

```text
QUALIFICATION_BASELINE_DRIFT
UNQUALIFIED_ADAPTER_ACTIVE
CRYPTO_PROFILE_MISMATCH
POLICY_CONFORMANCE_FAILED
SCHEMA_COMPATIBILITY_UNKNOWN
```

Не создаётся отдельный health engine.

---

# 28. Federation qualification

Federated peers MAY обмениваться:

- capability manifest;
- qualification manifest/bundle refs;
- crypto profile;
- governance profile.

Local federation policy определяет, какие:

- signers;
- suite versions;
- profiles;
- target environments

считаются достаточными evidence.

Self-asserted compliance insufficient.

---

# 29. Qualification isolation

Fault/destructive qualification выполняется в:

- isolated test environment;
- ephemeral workspace;
- explicitly designated non-production target;
- provider sandbox where supported.

Production destructive operations не должны использоваться как qualification test случайно.

---

# 30. Conceptual operator/API surface

Future semantic commands MAY выглядеть:

```text
vestrace conformance list
vestrace conformance check <profile>
vestrace qualify <profile>
vestrace qualify --fault-suite
vestrace qualification inspect <id>
vestrace qualification diff <old> <new>
```

Это documentation-level contract; текущая CLI availability определяется implementation-status docs.

---

# 31. Machine-readable result

Qualification SHOULD поддерживать structured output, например conceptually:

```json
{
  "profile": "MEMORY",
  "status": "passed",
  "suite_version": "...",
  "requirements": {
    "passed": 184,
    "failed": 0,
    "skipped": 0
  }
}
```

Exact schema будет отдельным versioned artifact.

---

# 32. Normative mappings

Primary IDs:

- `QUAL-001..QUAL-018`;
- все MUST IDs profile dependency closure;
- `EXT-*` для AUTONOMY;
- `REC-*`, `GOV-*` для TRUSTED;
- `IDW-*` для FEDERATION.

---

# 33. Forbidden shortcuts

```text
all unit tests pass == qualified
CI green == TRUSTED
high benchmark score == secure
skipped MUST == pass
same binary == same qualification in any environment
remote self-asserted profile == trusted evidence
signature == automatic trust
revalidation == full qualification
qualification == permanent certification
```

---

# 34. Completion criteria

Qualification Specification считается baseline-complete, когда:

1. каждый requirement family имеет profile mapping;
2. high-risk/fault/recovery golden scenarios покрывают architecture contracts;
3. capability/security bypass tests входят в mandatory profile gates;
4. adapter contracts объявляют observable properties;
5. release roadmap использует qualification profiles, а не feature checklist alone;
6. consistency pass существующих acceptance/docs отделяет current implementation claims от target qualification requirements.

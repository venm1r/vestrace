# Vestrace Crypto & Data Governance Contract v0.2

**Статус:** normative crypto/governance specification  
**Дата:** 2026-08-10

## 1. Назначение

Этот документ определяет конфиденциальность, целостность, authenticity, classification, retention, deletion, export, purpose limitation и cross-boundary data handling.

Ключевой принцип:

> **Encryption protects bytes. Governance determines whether those bytes may exist, be used, shared, retained or deleted.**

---

# 2. Separation of concerns

## 2.1 Crypto

Отвечает за:

- confidentiality;
- integrity;
- authenticity;
- key lifecycle;
- signing/encryption verification.

## 2.2 Data Governance

Отвечает за:

- ownership;
- classification;
- purpose;
- retention;
- deletion;
- export;
- sharing;
- residency;
- model/provider destination.

Crypto MUST NOT заменять capability или DataPolicy.

---

# 3. Classification

## 3.1 Sensitivity levels

```text
PUBLIC
INTERNAL
CONFIDENTIAL
RESTRICTED
```

`RESTRICTED` включает наиболее чувствительные категории, включая credential-adjacent metadata и высокорисковые персональные/операционные данные.

Secret values выделяются отдельным secret-store contract и не являются обычным RESTRICTED payload.

## 3.2 DataClassification

```text
DataClassification
├─ sensitivity
├─ categories[]
├─ jurisdiction_tags[]
├─ handling_requirements[]
├─ classification_source
└─ provenance
```

## 3.3 Conservative propagation

Derived object получает effective sensitivity не ниже наиболее строгого релевантного source, пока отдельное DeclassificationDecision не докажет безопасное понижение.

---

# 4. Classification lineage

Derived objects SHOULD сохранять:

```text
source_refs[]
source_classifications[]
effective_classification
classification_policy_version
```

Это позволяет объяснить, почему ContextPack/export/artifact имеет конкретный sensitivity.

---

# 5. Declassification

Declassification — отдельная governed operation.

```text
DeclassificationDecision
├─ target_ref
├─ previous_classification
├─ new_classification
├─ transformation/evidence
├─ policy_version
├─ actor/approver
└─ decided_at
```

Model assertion «секрета больше нет» само по себе недостаточно для authoritative declassification.

---

# 6. Secrets

## 6.1 Secret values are not memory

API keys, passwords, access tokens, private keys и аналогичный material MUST NOT сохраняться как ordinary Memory content.

## 6.2 SecretRef

Durable state хранит:

```text
SecretRef
├─ secret_ref_id / opaque URI
├─ provider/backend
├─ workspace/purpose scope
├─ metadata safe for storage
└─ version/generation?
```

## 6.3 Resolution lifecycle

```text
resolve after authorization
→ use ephemerally
→ discard plaintext
```

Plaintext MUST NOT попадать в durable logs, audit, ContextPack history, effect receipts или model provenance metadata.

## 6.4 Secret backend

Key/secret bytes SHOULD находиться за external/local `SecretBackendPort`/KeyProvider abstraction: OS keyring, KMS, HSM, Vault-like backend, mounted secret store и т.д.

---

# 7. Encryption at rest

Authoritative/regulated storage SHOULD поддерживать encryption at rest согласно deployment profile.

Области:

- PostgreSQL storage/backups;
- local CAS;
- export bundles;
- snapshot/archive media.

Application-level contract описывает encryption metadata, но не привязывает domain model к конкретному cloud KMS.

---

# 8. Envelope encryption

Для large/CAS objects предпочтительна модель:

```text
object bytes
  ↓ encrypted with
DEK
  ↓ wrapped with
KEK / workspace-purpose key
```

Преимущества:

- rotation;
- workspace isolation;
- per-purpose separation;
- crypto erasure;
- federation/export re-encryption.

---

# 9. Key hierarchy

Recommended logical hierarchy:

```text
Installation Root Trust
   ↓
Workspace Key Domain
   ↓
Purpose Key
   ├─ storage
   ├─ export
   ├─ signing
   ├─ federation
   └─ backup
```

Cross-workspace sharing не требует передачи source KEK/DEK target workspace.

---

# 10. Key metadata and lifecycle

```text
KeyReference
├─ provider
├─ key_id
├─ version
├─ purpose
├─ scope
├─ algorithm_suite
└─ lifecycle_state
```

Lifecycle:

```text
ACTIVE
ROTATING
RETIRED
REVOKED
DESTROYED
```

Key material SHOULD NOT храниться в ordinary Vestrace DB.

---

# 11. Algorithm agility

Persisted crypto metadata MUST хранить algorithm identifier/version, а не предполагать вечную единственную схему.

Historical verification должна учитывать algorithm/key version, действовавшие в момент signing/encryption.

---

# 12. Signing vs encryption

```text
encryption → confidentiality
signature  → authenticity/integrity attribution
hash       → content integrity/addressing
```

Эти guarantees не взаимозаменяемы.

Hash match не доказывает, кто создал object.

---

# 13. Signed artifacts/manifests

TRUSTED profile MAY/SHOULD подписывать:

- audit checkpoints;
- Run export manifests;
- federation packages;
- recovery snapshot manifests;
- qualification bundles;
- critical policy bundles.

```text
SignatureRecord
├─ object_digest
├─ signer_identity
├─ key_ref/version
├─ algorithm
├─ signature
└─ signed_at
```

---

# 14. Tamper-evident audit

Critical audit chain SHOULD поддерживать chained digests:

```text
entry N
├─ payload_digest
└─ previous_entry_digest
```

Periodic `AuditCheckpoint` MAY подписывать current chain digest.

Это tamper-evident journal, а не blockchain consensus.

---

# 15. DataPolicy

```text
DataPolicy
├─ policy_id
├─ version
├─ applies_to
├─ classification_rules
├─ retention_rule
├─ export_rule
├─ deletion_rule
├─ residency_rule
├─ sharing_rule
├─ provider/model rule
└─ purpose constraints
```

DataPolicy и Capability обе обязательны.

```text
capability allow
∩ DataPolicy allow
= possible operation
```

---

# 16. Purpose limitation

Sensitive data MAY иметь allowed purposes.

Пример:

```text
purpose = incident_diagnosis
```

не означает permission использовать data для:

- training;
- unrelated analytics;
- recommendation;
- federation export.

Purpose должен входить в policy decision для relevant high-sensitivity operations.

---

# 17. Data minimization

Vestrace SHOULD не сохранять raw payload, если product requirement удовлетворяется более ограниченным representation:

- digest;
- normalized metadata;
- content-free receipt;
- reference;
- redacted derivative.

Особенно для:

- credentials;
- HTTP headers;
- model/tool raw payloads;
- external responses;
- logs;
- long-lived execution capture.

---

# 18. Redaction

Redacted object является derivative:

```text
source
→ redaction transform(policy/version)
→ redacted derivative
```

Redaction не переписывает source автоматически.

Redacted derivative сохраняет source ref, transform version и effective classification.

---

# 19. Model/provider boundary

Перед model invocation:

```text
ContextPack/input
→ classification evaluation
→ provider eligibility/locality
→ purpose check
→ redaction/minimization
→ capability/policy
→ invocation
```

DataPolicy MAY запретить RESTRICTED data для remote provider или потребовать local model.

Право read Memory и право передать content provider — разные permissions.

---

# 20. ContextPack governance

ContextPack должен иметь:

- effective classification;
- source classification refs;
- allowed destinations;
- policy version;
- redaction/transformation refs;
- expiry/generation metadata where needed.

Retrieval не должен обходить governance через cache/summary/reference expansion.

---

# 21. Retention policy

```text
RetentionPolicy
├─ minimum_retention?
├─ maximum_retention?
├─ trigger
├─ hold behavior
├─ disposal method
└─ policy version
```

Triggers MAY включать:

- CREATED_AT;
- LAST_USED_AT;
- EXECUTION_CLOSED_AT;
- WORKSPACE_CLOSED_AT;
- POLICY_EVENT;
- EXPLICIT_DATE.

---

# 22. Retention lifecycle

```text
ACTIVE
→ EXPIRED
→ PENDING_DISPOSAL
→ DISPOSED
```

Expiry не означает мгновенное физическое удаление.

---

# 23. DataHold

```text
DataHold
├─ hold_id
├─ scope
├─ reason
├─ authority
├─ created_at
├─ expires_at?
└─ policy_ref
```

Hold блокирует automated disposal, но не уничтожает original retention history.

---

# 24. Deletion semantics

## 24.1 LOGICAL_DELETE

Object/content недоступен обычным reads и current cognition.

## 24.2 PHYSICAL_DELETE

Active storage bytes/rows удалены согласно scope.

## 24.3 CRYPTO_ERASURE

Key material уничтожен/недоступен так, что ciphertext больше не может быть расшифрован согласно threat model.

Ни один result не должен заявлять другую semantic, если она не доказана.

---

# 25. Dependency-aware deletion

Deletion planner определяет:

```text
target
→ direct refs
→ claims/memories/derivations
→ indexes/caches/summaries
→ artifacts
→ exports
→ sharing/mounts
→ backup constraints
```

Удаление evidence может вызвать claim/cognition revalidation.

---

# 26. DeletionRequest

```text
DeletionRequest
├─ requester
├─ authority
├─ scope/selector
├─ requested semantics
├─ exemptions/holds
├─ plan_ref
├─ execution_ref
└─ verification_ref
```

DeletionRequest abstraction не хардкодит конкретный закон; regulatory adapters/policies могут строиться сверху.

---

# 27. Deletion verification

```text
DeletionPlan
→ DeletionExecution
→ DeletionVerification
```

Successful SQL `DELETE` недостаточно для заявления полного disposal, если существуют CAS/index/cache/export/backup copies согласно заявленной semantic.

Verification result SHOULD указывать remaining constrained copies и scheduled expiry, если они есть.

---

# 28. Backup governance

Backup является governed data object и наследует:

- classification;
- encryption requirement;
- retention;
- residency;
- disposal lifecycle.

Active-store deletion не означает мгновенного исчезновения из immutable backup.

---

# 29. Data inventory

`DataInventory` — derived projection, отвечающая на вопрос, какие data classes хранятся в scope.

```text
DataInventory
├─ object classes/counts
├─ classifications
├─ retention states
├─ storage locations
├─ external shares/exports
└─ governance findings
```

Inventory не является самостоятельным source of truth.

---

# 30. Export governance

Export — governed external data effect.

`DataExportPlan` фиксирует:

- exact scope;
- purpose;
- recipient;
- classifications;
- included object revisions;
- redactions;
- export format/schema;
- encryption;
- signing;
- expiry;
- approvals/policy/capability.

---

# 31. ExportBundle

```text
ExportBundle
├─ manifest
├─ schema versions
├─ objects/references
├─ provenance
├─ classification
├─ hashes
├─ signature?
└─ encryption metadata
```

Export MUST быть проверяемым и не переносить authority автоматически.

---

# 32. Cross-workspace sharing

Source disclosure + target acceptance остаются обязательными.

Sharing MAY ограничивать:

- allowed memory kinds/statuses;
- classification ceiling;
- content mode;
- indexing/cache;
- model/provider use;
- derivation/import;
- export;
- result limits;
- validity interval.

Source key material target не передаётся.

---

# 33. Local import from sharing

Если target разрешено создать local derivative/import:

1. source authorization проверяется;
2. target DataPolicy проверяется;
3. создаётся proposal;
4. local Memory lifecycle создаёт новый object;
5. source provenance сохраняется;
6. local classification вычисляется conservatively;
7. если content encrypted locally, используется fresh target key/DEK.

Mount не становится local Memory автоматически.

---

# 34. Federation governance

Federation trust не является data permission.

Перед remote disclosure проверяются:

```text
remote identity/trust
∩ local capability
∩ DataPolicy
∩ classification compatibility
∩ recipient constraints
∩ residency/locality
∩ purpose
```

Remote qualification MAY быть evidence, но local policy определяет его достаточность.

---

# 35. Residency

Core model использует policy tags/constraints, а не hardcoded список стран.

```text
ResidencyConstraint
├─ allowed_regions[]
├─ forbidden_regions[]
└─ policy_source
```

Infrastructure/provider adapter доказывает eligibility среды.

---

# 36. Crypto anomalies

Trust-sensitive anomalies:

- hash mismatch;
- invalid signature;
- unexpected key version;
- use of revoked key;
- audit-chain break;
- KMS/provider integrity anomaly.

Они создают HealthFinding и MAY автоматически открыть Incident согласно severity/trust impact.

---

# 37. Key compromise response

```text
detect compromise
→ revoke/suspend key
→ determine affected scope
→ contain
→ rotate/re-encrypt where meaningful
→ re-sign new artifacts where appropriate
→ revalidate
```

Новая подпись не делает historical object, подписанный compromised key, автоматически trusted.

---

# 38. Crypto operation audit

Governed crypto operation SHOULD сохранять content-free record:

```text
CryptoOperation
├─ operation
├─ key_ref/version
├─ algorithm
├─ object_ref
├─ result
└─ timestamp
```

Secret/private key bytes не журналируются.

---

# 39. Policy versioning

Каждое significant governance decision сохраняет exact policy version.

Новая policy не переписывает history old decision.

При необходимости создаётся новый compliance/health finding или revalidation obligation.

---

# 40. Governance findings

Примеры invariants/findings:

```text
RETENTION_EXCEEDED
ORPHANED_SECRET_REF
CLASSIFICATION_MISMATCH
UNAUTHORIZED_EXPORT
STALE_SHARE_GRANT
UNKNOWN_DATA_OWNER
RESIDENCY_VIOLATION
KEY_ROTATION_OVERDUE
AUDIT_CHAIN_BROKEN
UNQUALIFIED_PROVIDER_FOR_CLASSIFICATION
```

Используется общий Health/Integrity engine; отдельный compliance health runtime не создаётся.

---

# 41. Governance workflows use existing execution runtime

Export, deletion, reclassification, key rotation, share revocation и related operations — governed executions в едином runtime.

Они используют:

- capability;
- policy;
- approval;
- budget;
- audit;
- repair/recovery semantics where relevant.

---

# 42. Normative mappings

Основные IDs:

- `GOV-001..GOV-026`;
- `CAP-001..CAP-014`;
- `IDW-004..IDW-014`;
- `RET-007`, `RET-014`;
- `EXT-018`;
- `REC-011`.

---

# 43. Golden scenarios

## 43.1 Restricted memory to remote model

```text
retrieval finds RESTRICTED memory
→ ContextPack classification RESTRICTED
→ remote provider forbidden
→ invocation denied/local provider required
```

No redaction bypass via cached summary.

## 43.2 Evidence deletion

```text
source evidence deleted
→ dependent claim loses last admissible evidence
→ claim revalidation
→ retrieval stops treating it as unqualified current truth if policy requires support
```

## 43.3 Backup-aware deletion

```text
physical active storage deletion succeeds
→ immutable backup still retained by policy
→ result reports active_storage_removed=true
→ backup_remaining=true
→ final disposal scheduled
```

## 43.4 Share revoke

```text
source revokes grant
→ target mount invalid/stale
→ future content access denied
→ derived caches invalidated
→ historical disclosure audit remains
```

## 43.5 Key compromise

```text
signing key compromised
→ key revoked
→ affected signatures marked trust-impacted
→ incident/revalidation
→ no silent re-signing of history as if original trust remained
```

---

# 44. Forbidden shortcuts

```text
encrypted == authorized
hash == provenance
signature == permission
RESTRICTED == secret plaintext store
read permission == provider disclosure permission
mount == ownership
revocation == erase history
delete row == all copies destroyed
crypto erasure == physical erasure
new policy == rewrite old decision
```

---

# 45. Completion criteria

Документ считается согласованным, когда:

1. Trust & Authority Model остаётся единственным владельцем authorization composition;
2. DataPolicy rules получают stable policy/version references;
3. Model/provider paths проходят classification/destination governance;
4. sharing/export/delete semantics отражены в conformance scenarios;
5. backup/CAS/DB documentation честно различает deletion semantics;
6. crypto failures интегрированы с Health/Incident/Revalidation.

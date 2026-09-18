# Vestrace Crypto & Data Governance Contract v0.2

> **English reading edition · 2026-09-08.** Complete editorial translation of the [frozen original](../vestrace-crypto-data-governance-contract-v0.2.md) from supplied snapshot `3e05dfbd`. Requirement IDs, normative strength, technical states, and examples are preserved. This translation does not amend the original contract or claim implementation. If wording differs, the frozen original and applicable Accepted ADRs take precedence.

**Status:** Normative crypto/governance specification
**Date:** 2026-08-10

## 1. Purpose

This document defines confidentiality, integrity, authenticity, classification, retention, deletion, export, purpose limitation, and cross-boundary data handling.

Core principle:

> **Encryption protects bytes. Governance determines whether those bytes may exist, be used, shared, retained or deleted.**

---

# 2. Separation of concerns

## 2.1 Crypto

Responsible for:

- confidentiality;
- integrity;
- authenticity;
- key lifecycle;
- signing/encryption verification.

## 2.2 Data Governance

Responsible for:

- ownership;
- classification;
- purpose;
- retention;
- deletion;
- export;
- sharing;
- residency;
- model/provider destination.

Cryptography MUST NOT replace capability or DataPolicy.

---

# 3. Classification

## 3.1 Sensitivity levels

```text
PUBLIC
INTERNAL
CONFIDENTIAL
RESTRICTED
```

`RESTRICTED` includes the most sensitive categories, including credential-adjacent metadata and high-risk personal/operational data.

Secret values belong to a separate secret-store contract and are not ordinary RESTRICTED payloads.

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

A derived object's effective sensitivity is at least that of the most restrictive relevant source until a separate DeclassificationDecision establishes that lowering it is safe.

---

# 4. Classification lineage

Derived objects SHOULD retain:

```text
source_refs[]
source_classifications[]
effective_classification
classification_policy_version
```

This makes the sensitivity assigned to a ContextPack, export, or artifact explainable.

---

# 5. Declassification

Declassification is a separate governed operation.

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

A model's assertion that a secret is gone is not sufficient for authoritative declassification.

---

# 6. Secrets

## 6.1 Secret values are not memory

API keys, passwords, access tokens, private keys, and similar material MUST NOT be stored as ordinary Memory content.

## 6.2 SecretRef

Durable state stores:

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

Plaintext MUST NOT enter durable logs, audit, ContextPack history, effect receipts, or model-provenance metadata.

## 6.4 Secret backend

Key/secret bytes SHOULD reside behind an external/local `SecretBackendPort`/KeyProvider abstraction, such as an OS keyring, KMS, HSM, Vault-like backend, or mounted secret store.

---

# 7. Encryption at rest

Authoritative/regulated storage SHOULD support encryption at rest according to the deployment profile.

Areas:

- PostgreSQL storage/backups;
- local CAS;
- export bundles;
- snapshot/archive media.

The application-level contract describes encryption metadata without coupling the domain model to a particular cloud KMS.

---

# 8. Envelope encryption

For large/CAS objects, the preferred model is:

```text
object bytes
  ↓ encrypted with
DEK
  ↓ wrapped with
KEK / workspace-purpose key
```

Benefits:

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

Cross-workspace sharing does not require transferring the source KEK/DEK to the target workspace.

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

Key material SHOULD NOT be stored in the ordinary Vestrace database.

---

# 11. Algorithm agility

Persisted cryptographic metadata MUST contain algorithm identifiers/versions rather than assume one permanent scheme.

Historical verification must account for the algorithm/key version used when signing/encrypting.

---

# 12. Signing vs encryption

```text
encryption → confidentiality
signature  → authenticity/integrity attribution
hash       → content integrity/addressing
```

These guarantees are not interchangeable.

A hash match does not establish who created an object.

---

# 13. Signed artifacts/manifests

The TRUSTED profile MAY/SHOULD sign:

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

A critical audit chain SHOULD support chained digests:

```text
entry N
├─ payload_digest
└─ previous_entry_digest
```

A periodic `AuditCheckpoint` MAY sign the current chain digest.

This is a tamper-evident journal, not blockchain consensus.

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

Both DataPolicy and Capability are mandatory.

```text
capability allow
∩ DataPolicy allow
= possible operation
```

---

# 16. Purpose limitation

Sensitive data MAY have permitted purposes.

Example:

```text
purpose = incident_diagnosis
```

does not imply permission to use the data for:

- training;
- unrelated analytics;
- recommendation;
- federation export.

Purpose must be part of the policy decision for relevant high-sensitivity operations.

---

# 17. Data minimization

Vestrace SHOULD avoid retaining raw payloads when a more limited representation satisfies the product requirement:

- digest;
- normalized metadata;
- content-free receipt;
- reference;
- redacted derivative.

Especially for:

- credentials;
- HTTP headers;
- model/tool raw payloads;
- external responses;
- logs;
- long-lived execution capture.

---

# 18. Redaction

A redacted object is a derivative:

```text
source
→ redaction transform(policy/version)
→ redacted derivative
```

Redaction does not automatically rewrite the source.

A redacted derivative preserves the source reference, transform version, and effective classification.

---

# 19. Model/provider boundary

Before model invocation:

```text
ContextPack/input
→ classification evaluation
→ provider eligibility/locality
→ purpose check
→ redaction/minimization
→ capability/policy
→ invocation
```

DataPolicy MAY prohibit RESTRICTED data at a remote provider or require a local model.

Reading Memory and transmitting its content to a provider are different permissions.

---

# 20. ContextPack governance

A ContextPack must have:

- effective classification;
- source classification refs;
- allowed destinations;
- policy version;
- redaction/transformation refs;
- expiry/generation metadata where needed.

Retrieval must not bypass governance through cache, summary, or reference expansion.

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

Triggers MAY include:

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

Expiry does not mean immediate physical deletion.

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

A hold blocks automated disposal but does not destroy the original retention history.

---

# 24. Deletion semantics

## 24.1 LOGICAL_DELETE

The object/content is unavailable to ordinary reads and current cognition.

## 24.2 PHYSICAL_DELETE

Active-storage bytes/rows are removed within the declared scope.

## 24.3 CRYPTO_ERASURE

Key material is destroyed or made unavailable so the ciphertext can no longer be decrypted under the declared threat model.

No result may claim different deletion semantics unless those semantics are established.

---

# 25. Dependency-aware deletion

The deletion planner determines:

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

Deleting evidence may trigger claim/cognition revalidation.

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

The DeletionRequest abstraction does not hardcode a particular law; regulatory adapters/policies may build on it.

---

# 27. Deletion verification

```text
DeletionPlan
→ DeletionExecution
→ DeletionVerification
```

A successful SQL `DELETE` is insufficient to claim complete disposal when CAS/index/cache/export/backup copies remain within the declared semantics.

The verification result SHOULD identify remaining constrained copies and scheduled expiry, when applicable.

---

# 28. Backup governance

A backup is a governed data object and inherits:

- classification;
- encryption requirement;
- retention;
- residency;
- disposal lifecycle.

Deleting from active storage does not mean immediate removal from an immutable backup.

---

# 29. Data inventory

`DataInventory` is a derived projection describing the data classes stored in a scope.

```text
DataInventory
├─ object classes/counts
├─ classifications
├─ retention states
├─ storage locations
├─ external shares/exports
└─ governance findings
```

Inventory is not an independent source of truth.

---

# 30. Export governance

Export — governed external data effect.

`DataExportPlan` records:

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

Export MUST be verifiable and must not transfer authority automatically.

---

# 32. Cross-workspace sharing

Source disclosure authorization and target acceptance remain mandatory.

Sharing MAY constrain:

- allowed memory kinds/statuses;
- classification ceiling;
- content mode;
- indexing/cache;
- model/provider use;
- derivation/import;
- export;
- result limits;
- validity interval.

Source key material is not transferred to the target.

---

# 33. Local import from sharing

When the target is permitted to create a local derivative/import:

1. Check source authorization.
2. Check target DataPolicy.
3. Create a proposal.
4. Create a new object through the local Memory lifecycle.
5. Preserve source provenance.
6. Calculate local classification conservatively.
7. If content is encrypted locally, use a fresh target key/DEK.

A mount does not automatically become local Memory.

---

# 34. Federation governance

Federation trust is not permission to access data.

Before remote disclosure, check:

```text
remote identity/trust
∩ local capability
∩ DataPolicy
∩ classification compatibility
∩ recipient constraints
∩ residency/locality
∩ purpose
```

Remote qualification MAY provide evidence, but local policy determines whether it is sufficient.

---

# 35. Residency

The core model uses policy tags/constraints rather than a hardcoded country list.

```text
ResidencyConstraint
├─ allowed_regions[]
├─ forbidden_regions[]
└─ policy_source
```

The infrastructure/provider adapter establishes the environment's eligibility.

---

# 36. Crypto anomalies

Trust-sensitive anomalies:

- hash mismatch;
- invalid signature;
- unexpected key version;
- use of revoked key;
- audit-chain break;
- KMS/provider integrity anomaly.

These produce HealthFindings and MAY automatically open an Incident according to severity/trust impact.

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

A new signature does not automatically make a historical object signed with a compromised key trusted.

---

# 38. Crypto operation audit

A governed cryptographic operation SHOULD retain a content-free record:

```text
CryptoOperation
├─ operation
├─ key_ref/version
├─ algorithm
├─ object_ref
├─ result
└─ timestamp
```

Secret/private-key bytes are not logged.

---

# 39. Policy versioning

Every significant governance decision records its exact policy version.

A new policy does not rewrite the history of an earlier decision.

Where necessary, create a new compliance/health finding or revalidation obligation.

---

# 40. Governance findings

Example invariants/findings:

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

Use the shared Health/Integrity engine; do not create a separate compliance-health runtime.

---

# 41. Governance workflows use existing execution runtime

Export, deletion, reclassification, key rotation, share revocation, and related operations are governed executions in the single runtime.

They use:

- capability;
- policy;
- approval;
- budget;
- audit;
- repair/recovery semantics where relevant.

---

# 42. Normative mappings

Principal IDs:

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

This document is consistent when:

1. The Trust & Authority Model remains the sole owner of authorization composition.
2. DataPolicy rules have stable policy/version references.
3. Model/provider paths enforce classification/destination governance.
4. Sharing, export, and deletion semantics are reflected in conformance scenarios.
5. Backup/CAS/database documentation accurately distinguishes deletion semantics.
6. Cryptographic failures are integrated with Health, Incident, and Revalidation.

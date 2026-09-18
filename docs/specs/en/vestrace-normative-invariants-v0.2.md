# Vestrace Normative Invariants Catalog v0.2

> **English reading edition · 2026-09-08.** Complete editorial translation of the [frozen original](../vestrace-normative-invariants-v0.2.md) from supplied snapshot `3e05dfbd`. Requirement IDs, normative strength, technical states, and examples are preserved. This translation does not amend the original contract or claim implementation. If wording differs, the frozen original and applicable Accepted ADRs take precedence.

**Status:** Normative requirements baseline
**Date:** 2026-08-10
**Dependencies:**

- `docs/specs/vestrace-architecture-contract-v0.2.md`
- `docs/specs/vestrace-domain-model-v0.2.md`

> This catalog defines stable requirement IDs for requirements that need subsequent conformance tests/evidence. It does not claim the current implementation already satisfies them.

## 1. Format

Each requirement has:

- **ID** — a stable identifier;
- **Level** — MUST / SHOULD / MAY;
- **Class** — the type of check;
- **Statement** — the normative requirement.

Verification classes:

- `STATIC` — schema/config/manifest inspection;
- `DOMAIN` — deterministic domain rule;
- `STATEFUL` — a sequence of states/mutations;
- `SECURITY` — authorization/isolation negative/positive tests;
- `FAULT` — controlled failure injection;
- `RECOVERY` — crash/rebuild/revalidation;
- `INTEROP` — adapter/protocol/federation interoperability;
- `EVIDENCE` — the presence of provenance/audit/verification evidence.

---

# 2. Architecture (`ARC-*`)

| ID | Level | Class | Statement |
|---|---|---|---|
| ARC-001 | MUST | STATIC | Vestrace distinguishes authoritative/canonical state from derived state. |
| ARC-002 | MUST | STATEFUL | Derived state has a rebuild path from a more authoritative layer. |
| ARC-003 | MUST NOT | STATEFUL | Derived state cannot automatically rewrite more authoritative state merely to eliminate drift. |
| ARC-004 | MUST | STATIC | Repair, recovery, governance, and external-effect workflows use one execution/policy/audit boundary. |
| ARC-005 | MUST NOT | STATIC | The architecture does not introduce another event store or parallel orchestration runtime without a separate ADR. |
| ARC-006 | MUST | EVIDENCE | Historical corrections preserve the fact of the original state/event. |
| ARC-007 | MUST | STATEFUL | Uncertainty has an explicit state representation and is not automatically reduced to success/failure. |
| ARC-008 | MUST | STATIC | An identity, reference, hash, or URI does not confer authorization by itself. |
| ARC-009 | SHOULD | STATIC | New durable entities have typed IDs and explicit authority owners. |
| ARC-010 | MUST | STATIC | Every new projection is identified as a projection and does not become a source of truth by default. |

---

# 3. Persistent Cognition / Memory (`MEM-*`)

| ID | Level | Class | Statement |
|---|---|---|---|
| MEM-001 | MUST | DOMAIN | `Memory` has stable identity; semantic content is stored in immutable revisions. |
| MEM-002 | MUST | DOMAIN | Changing semantic content creates a new `MemoryRevision`; in-place overwrite is prohibited. |
| MEM-003 | MUST | DOMAIN | The active revision belongs to the same Memory. |
| MEM-004 | MUST | DOMAIN | Revision numbers are monotonic and immutable within a Memory. |
| MEM-005 | MUST | DOMAIN | Active Memory has at least one admissible evidence/source reference. |
| MEM-006 | MUST | DOMAIN | Derived Memory has a Derivation. |
| MEM-007 | MUST | STATEFUL | `Superseded`, `Expired`, `Rejected`, and `Deleted` do not silently return to `Active`. |
| MEM-008 | MUST | DOMAIN | When specified, `valid_until` cannot precede `valid_from`. |
| MEM-009 | MUST | DOMAIN | Confidence and importance are finite and within `[0,1]`. |
| MEM-010 | MUST | STATIC | Structured payloads have a schema identity/version. |
| MEM-011 | MUST | DOMAIN | A scope relationship cannot expand workspace authority. |
| MEM-012 | MUST | EVIDENCE | Derived canonical cognition has provenance closure to source evidence or explicit human-authored authority. |
| MEM-013 | MUST | DOMAIN | `Claim` and `Memory` remain distinct domain concepts. |
| MEM-014 | MUST | DOMAIN | Claim status `Supported` is not interpreted as absolute external truth. |
| MEM-015 | MUST | STATEFUL | A claim that loses all admissible supporting evidence undergoes revalidation. |
| MEM-016 | MUST | STATEFUL | An open semantic conflict remains explicit until reconciliation. |
| MEM-017 | MUST NOT | STATEFUL | A conflict is not resolved automatically through universal `latest timestamp wins`. |
| MEM-018 | MUST | EVIDENCE | Conflict resolution preserves its basis, evidence, policy, and human decision. |
| MEM-019 | MUST | STATEFUL | Supersession does not delete superseded history. |
| MEM-020 | SHOULD | STATIC | Cognitive assets such as agents, skills, and workflows are separate versioned assets, not Memory records. |

---

# 4. Temporal & Concurrency (`TMP-*`)

| ID | Level | Class | Statement |
|---|---|---|---|
| TMP-001 | MUST | DOMAIN | `recorded_at` and `occurred_at` have distinct semantics. |
| TMP-002 | MUST | DOMAIN | Unknown `occurred_at` does not block authoritative recording when `recorded_at` is known. |
| TMP-003 | MUST | DOMAIN | Knowledge validity (`valid_from/to`) is independent of recording time. |
| TMP-004 | MUST | STATEFUL | Current retrieval does not present superseded/expired knowledge as current. |
| TMP-005 | MUST | STATEFUL | An `as_of` query uses the appropriate historical-state semantics, not the current projection. |
| TMP-006 | MUST | DOMAIN | Canonical mutation has an expected version/revision precondition when concurrency is possible. |
| TMP-007 | MUST | STATEFUL | A version mismatch produces a conflict/stale result without silent overwrite. |
| TMP-008 | MUST NOT | DOMAIN | A wall-clock timestamp is not proof of a global causal order without an additional ordering contract. |
| TMP-009 | MUST | DOMAIN | A lease/heartbeat does not by itself increase authority or change a logical revision. |
| TMP-010 | SHOULD | STATEFUL | Canonical append-only streams have a verifiable monotonic order within their aggregate/scope. |

---

# 5. Mutation & Reconciliation (`MUT-*`)

| ID | Level | Class | Statement |
|---|---|---|---|
| MUT-001 | MUST | EVIDENCE | A significant cognitive mutation retains actor, target, expected state, reason/intent, and resulting revision/fact. |
| MUT-002 | MUST NOT | STATEFUL | Correction does not rewrite an earlier revision. |
| MUT-003 | MUST | DOMAIN | Deterministic reconciliation uses a more authoritative source/state. |
| MUT-004 | MUST | DOMAIN | Semantic ambiguity is not disguised as deterministic repair. |
| MUT-005 | MUST | STATEFUL | Policy-guided reconciliation references an exact policy version. |
| MUT-006 | MUST | STATEFUL | Human-required reconciliation does not execute automatically. |
| MUT-007 | MUST | STATIC | The State Engine does not introduce generic Task/Action aggregates duplicating existing owners of execution semantics. |
| MUT-008 | MUST | EVIDENCE | A reconciliation result preserves its evidence/basis and relationships to the conflicting inputs. |

---

# 6. Retrieval / ContextPack (`RET-*`)

| ID | Level | Class | Statement |
|---|---|---|---|
| RET-001 | MUST | SECURITY | Security/workspace/classification/share filters apply before denied content is disclosed to downstream ranking/model stages. |
| RET-002 | MUST | STATEFUL | Current retrieval excludes superseded/expired cognition as current truth. |
| RET-003 | MUST | STATEFUL | Unresolved conflicts are explicit, not converted into selected facts without reconciliation. |
| RET-004 | MUST | DOMAIN | `ContextPack.used_budget` does not exceed the hard token/content budget. |
| RET-005 | MUST | EVIDENCE | Every ContextPack item preserves a source/revision provenance reference. |
| RET-006 | MUST | EVIDENCE | Every ContextPack item has an inclusion explanation sufficient for diagnostics. |
| RET-007 | MUST | SECURITY | A ContextPack respects classification/model-destination policy. |
| RET-008 | MUST NOT | DOMAIN | Including an item in a ContextPack does not increase its semantic authority. |
| RET-009 | MUST | STATEFUL | Failure of a derived retrieval channel produces an explicit degraded mode when a safe fallback exists. |
| RET-010 | MUST | STATEFUL | When no safe fallback exists, retrieval fails closed. |
| RET-011 | SHOULD | DOMAIN | Incomparable channel scores are combined through rank-based fusion rather than direct addition. |
| RET-012 | MUST | STATEFUL | Cache/context generation is invalidated by relevant canonical, policy, sharing, or classification changes. |
| RET-013 | MUST | DOMAIN | Representation levels `Full/Summary/Atomic/Reference` retain their original source/revision reference. |
| RET-014 | MUST | SECURITY | Cross-workspace mounted content passes both source and target policy checks on every governed access. |
| RET-015 | MUST | EVIDENCE | A retrieval run can be journaled with policy, version, and degradation metadata. |

---

# 7. Feedback & Learning (`LRN-*`)

| ID | Level | Class | Statement |
|---|---|---|---|
| LRN-001 | MUST | EVIDENCE | Feedback signals reference exact execution/model/tool/policy revisions. |
| LRN-002 | MUST | DOMAIN | Raw evaluation facts are stored separately from learned projections/recommendations. |
| LRN-003 | MUST NOT | SECURITY | The learning pipeline does not automatically increase capabilities/permissions. |
| LRN-004 | MUST NOT | SECURITY | The learning pipeline does not bypass governance policy when changing agents, skills, or workflows. |
| LRN-005 | MUST | STATEFUL | Learning-driven cognitive-asset changes are versioned and preserve provenance. |
| LRN-006 | SHOULD | DOMAIN | Deterministic/human signals have higher default authority than heuristic model-judge signals. |
| LRN-007 | MUST | EVIDENCE | A learned performance conclusion retains references to its underlying measurements. |
| LRN-008 | MUST | STATEFUL | Deleting a learned projection does not destroy raw execution/evaluation history. |

---

# 8. Capability Governance (`CAP-*`)

| ID | Level | Class | Statement |
|---|---|---|---|
| CAP-001 | MUST | SECURITY | Effective capabilities/policy determine runtime authorization, not role names. |
| CAP-002 | MUST | SECURITY | Default policy — deny. |
| CAP-003 | MUST | STATIC | A capability supports constraints on operation/tool, resource/scope, validity, budget, risk, and conditions. |
| CAP-004 | MUST | SECURITY | An expired capability is rejected. |
| CAP-005 | MUST | SECURITY | A child/subagent's effective authority cannot exceed its parent's effective authority. |
| CAP-006 | MUST | SECURITY | Delegation requires explicit delegation permission/contract. |
| CAP-007 | MUST | SECURITY | Delegation depth is bounded. |
| CAP-008 | MUST | DOMAIN | Risk categories include at least `low/medium/high/critical`. |
| CAP-009 | MUST | DOMAIN | Context can increase effective risk. |
| CAP-010 | MUST NOT | SECURITY | Approval does not override explicit denial, a hard budget, or a capability ceiling. |
| CAP-011 | MUST | EVIDENCE | A policy decision retains the exact policy version and relevant input state. |
| CAP-012 | MUST | SECURITY | A material change to an approved operation/intent invalidates approval. |
| CAP-013 | SHOULD | EVIDENCE | Budget reservations/accounting are auditable. |
| CAP-014 | MUST | SECURITY | An agent-facing API does not provide a self-escalation capability. |

---

# 9. Identity / Workspace / Federation (`IDW-*`)

| ID | Level | Class | Statement |
|---|---|---|---|
| IDW-001 | MUST | SECURITY | Workspace is an authority/isolation boundary. |
| IDW-002 | MUST | SECURITY | Workspace-local `global` does not mean cross-workspace global. |
| IDW-003 | MUST | SECURITY | Actor identity and authority are checked independently. |
| IDW-004 | MUST | SECURITY | Cross-workspace Memory access requires an explicit source grant and target acceptance. |
| IDW-005 | MUST NOT | SECURITY | Wildcard target grants are prohibited. |
| IDW-006 | MUST NOT | SECURITY | Mounted memory cannot be automatically reshared to a third workspace. |
| IDW-007 | MUST | SECURITY | Source and target policies are checked independently. |
| IDW-008 | MUST | STATEFUL | A revoked, expired, or stale mount discloses no content. |
| IDW-009 | MUST | DOMAIN | `SharedMemoryRef` is namespaced by source workspace and exact memory/revision/grant context. |
| IDW-010 | MUST NOT | DOMAIN | SharedMemoryRef is not substituted for a local MemoryId. |
| IDW-011 | MUST | EVIDENCE | Local derivation/import from mounted content preserves source provenance. |
| IDW-012 | MUST | SECURITY | Federation trust or identity recognition does not by itself permit data disclosure. |
| IDW-013 | MUST | STATEFUL | Revocation does not rewrite the historical fact of an already completed disclosure. |
| IDW-014 | MUST | SECURITY | Cross-workspace access does not require disabling RLS or normal workspace isolation. |

---

# 10. Health / Integrity / Repair (`HLT-*`)

| ID | Level | Class | Statement |
|---|---|---|---|
| HLT-001 | MUST | EVIDENCE | HealthFinding contains invariant, scope, evidence, severity, and repairability. |
| HLT-002 | MUST | DOMAIN | Aggregate health is a projection, not canonical truth. |
| HLT-003 | MUST | DOMAIN | `UNKNOWN` health is not interpreted as `HEALTHY`. |
| HLT-004 | MUST | DOMAIN | Severity and repair risk are separate dimensions. |
| HLT-005 | MUST | STATIC | An invariant definition has a stable ID and version. |
| HLT-006 | MUST | STATEFUL | Deterministic automatic repair is permitted only from a more authoritative source/state. |
| HLT-007 | MUST NOT | STATEFUL | A checker does not directly perform hidden mutations. |
| HLT-008 | MUST | STATEFUL | Repair uses an immutable RepairPlan before execution. |
| HLT-009 | MUST | STATEFUL | A RepairPlan with stale preconditions receives `STALE_PLAN` and does not execute. |
| HLT-010 | MUST | SECURITY | Repair execution passes capability/policy gates appropriate to its risk and repairability. |
| HLT-011 | MUST | STATEFUL | `RepairExecution=SUCCEEDED` does not automatically close a finding. |
| HLT-012 | MUST | EVIDENCE | A finding's `RESOLVED` state requires successful verification evidence. |
| HLT-013 | MUST | STATEFUL | Verification failure leaves the finding open or reopened. |
| HLT-014 | MUST | STATEFUL | Occurrences of a logical finding are stored separately from finding identity. |
| HLT-015 | MUST | STATEFUL | Automatic repair has a retry/attempt budget and cooldown. |
| HLT-016 | MUST | STATEFUL | Flapping stops an endless automatic-repair loop. |
| HLT-017 | MUST | DOMAIN | Health propagation follows declared dependencies, not a global worst-state rule. |
| HLT-018 | MUST | EVIDENCE | Suppression/AcceptedRisk has actor, reason, expiry/policy, and audit. |
| HLT-019 | MUST NOT | DOMAIN | Suppression does not delete a finding or turn raw integrity into healthy state. |
| HLT-020 | SHOULD | STATEFUL | Repair steps are idempotent/restartable where technically possible. |

---

# 11. External Effects (`EXT-*`)

| ID | Level | Class | Statement |
|---|---|---|---|
| EXT-001 | MUST | STATEFUL | An immutable ExternalEffectIntent is persisted before external dispatch. |
| EXT-002 | MUST | SECURITY | An effect passes capability/policy/budget authorization before dispatch. |
| EXT-003 | MUST | DOMAIN | An adapter declares delivery semantics: at-most-once, at-least-once, effectively-once, or unknown. |
| EXT-004 | MUST NOT | STATIC | An adapter/runtime does not claim universal exactly-once behavior without a verifiable provider contract. |
| EXT-005 | MUST | DOMAIN | Idempotency/retry safety is an adapter contract, not an unrestricted agent decision. |
| EXT-006 | MUST | STATEFUL | A transport timeout does not automatically mean an effect did not occur. |
| EXT-007 | MUST | STATEFUL | `UNKNOWN` outcome is a first-class state. |
| EXT-008 | MUST NOT | STATEFUL | An UNKNOWN external effect does not automatically become FAILED. |
| EXT-009 | MUST NOT | STATEFUL | An UNKNOWN effect is not automatically retried unless the adapter establishes retry-safe semantics. |
| EXT-010 | MUST | STATEFUL | Reconciliation uses the strongest available read-back/evidence mechanism. |
| EXT-011 | MUST | DOMAIN | `REVERSIBLE`, `COMPENSATABLE`, `IRREVERSIBLE`, and `UNKNOWN_REVERSIBILITY` remain distinct. |
| EXT-012 | MUST NOT | DOMAIN | Compensation is not called rollback. |
| EXT-013 | MUST | EVIDENCE | Compensation references the compensated effect. |
| EXT-014 | MUST | STATEFUL | Critical external preconditions are checked immediately before dispatch. |
| EXT-015 | MUST | SECURITY | A material change to an approved intent requires new authorization. |
| EXT-016 | MUST | EVIDENCE | A dispatched effect has a receipt/evidence record. |
| EXT-017 | MUST NOT | EVIDENCE | A receipt/ACK is not automatically treated as the desired business outcome. |
| EXT-018 | MUST | SECURITY | Secret material is not stored as plaintext in effect traces/receipts. |

---

# 12. Incident / Recovery / Revalidation (`REC-*`)

| ID | Level | Class | Statement |
|---|---|---|---|
| REC-001 | MUST | DOMAIN | HealthFinding and Incident are separate concepts. |
| REC-002 | MUST | STATEFUL | The Incident lifecycle includes containment before trust restoration. |
| REC-003 | MUST | STATEFUL | Process restart does not automatically return a scope to TRUSTED. |
| REC-004 | MUST | RECOVERY | Startup recovery classifies unfinished work rather than resuming everything. |
| REC-005 | MUST | RECOVERY | Ambiguous external dispatch after a crash enters the UNKNOWN/reconciliation path. |
| REC-006 | MUST NOT | RECOVERY | An ambiguous irreversible effect is not automatically repeated after restart. |
| REC-007 | MUST | RECOVERY | A recovery point references demonstrably consistent state and a scope position. |
| REC-008 | MUST | RECOVERY | A snapshot passes integrity/schema/continuity validation before use as a recovery source. |
| REC-009 | MUST | RECOVERY | After restore, derived state is rebuilt/revalidated from canonical state. |
| REC-010 | MUST | DOMAIN | `AVAILABLE`, `HEALTHY`, `TRUSTED`, `RECOVERED`, and `REVALIDATED` remain distinct. |
| REC-011 | MUST | RECOVERY | Trust cannot transition `UNTRUSTED → TRUSTED` without the required successful RevalidationRun. |
| REC-012 | MUST | EVIDENCE | RevalidationRun retains scope, level, checks, evidence, and result. |
| REC-013 | MUST | DOMAIN | `INCONCLUSIVE` revalidation is a valid outcome and is not counted as PASSED. |
| REC-014 | MUST NOT | RECOVERY | Divergent histories are not automatically merged through `latest wins`. |
| REC-015 | MUST | RECOVERY | Recovery automation has bounded retry/budgets and detects recovery loops. |
| REC-016 | SHOULD | EVIDENCE | Forensic evidence is preserved before destructive recovery. |
| REC-017 | MUST | STATEFUL | Capability restoration after a trust incident follows policy and MAY be progressive. |
| REC-018 | MUST | EVIDENCE | Recovery does not rewrite history as though the failure/incident never occurred. |

---

# 13. Crypto / Data Governance (`GOV-*`)

| ID | Level | Class | Statement |
|---|---|---|---|
| GOV-001 | MUST | STATIC | Cryptographic controls and DataPolicy controls are independent gates. |
| GOV-002 | MUST | DOMAIN | Base sensitivity ordering: PUBLIC < INTERNAL < CONFIDENTIAL < RESTRICTED. |
| GOV-003 | MUST | STATEFUL | Derived data does not automatically lower classification relative to its sources. |
| GOV-004 | MUST | EVIDENCE | Declassification is a separate governed decision with provenance. |
| GOV-005 | MUST NOT | SECURITY | Secret plaintext is not stored as ordinary Memory. |
| GOV-006 | MUST | SECURITY | Durable secret usage relies on opaque SecretRef and a controlled resolver/backend. |
| GOV-007 | SHOULD | SECURITY | Large/CAS data at rest is protected with envelope encryption when the crypto profile is enabled. |
| GOV-008 | MUST | STATIC | Cryptographic metadata supports algorithm/version agility. |
| GOV-009 | MUST | DOMAIN | Hash integrity and signature/authorship are distinct guarantees. |
| GOV-010 | SHOULD | EVIDENCE | Critical audit history supports tamper-evident chaining/checkpoints in the TRUSTED profile. |
| GOV-011 | MUST | SECURITY | A capability grant does not bypass a DataPolicy restriction. |
| GOV-012 | MUST | SECURITY | A DataPolicy allow decision does not bypass a missing required capability. |
| GOV-013 | MUST | STATEFUL | The retention lifecycle distinguishes ACTIVE, EXPIRED, PENDING_DISPOSAL, and DISPOSED. |
| GOV-014 | MUST | EVIDENCE | DataHold has scope, authority, and reason. |
| GOV-015 | MUST | DOMAIN | Logical deletion, physical deletion, and cryptographic erasure are distinct. |
| GOV-016 | MUST NOT | EVIDENCE | A deletion result does not claim that backup copies were destroyed unless this is established. |
| GOV-017 | MUST | STATEFUL | Evidence deletion triggers revalidation of dependent claims/derived state. |
| GOV-018 | MUST | SECURITY | Export is a governed operation with exact scope, recipient, purpose, and classification. |
| GOV-019 | MUST | EVIDENCE | An export bundle has manifest, provenance, and integrity metadata. |
| GOV-020 | MUST | SECURITY | Federation identity/trust does not by itself authorize classified data transfer. |
| GOV-021 | MUST | SECURITY | Model invocation checks destination, locality, classification, and redaction policy. |
| GOV-022 | MUST | SECURITY | RESTRICTED data is not sent to a provider prohibited by DataPolicy. |
| GOV-023 | MUST | RECOVERY | Signature, hash, audit-chain, or key-compromise anomalies affect trust and initiate the appropriate incident/revalidation path. |
| GOV-024 | MUST | EVIDENCE | A governance decision references the exact policy version. |
| GOV-025 | MUST NOT | STATEFUL | A new policy version does not retroactively rewrite the history of an earlier decision. |
| GOV-026 | MUST | EVIDENCE | Deletion is complete only after DeletionVerification under the declared deletion semantics. |

---

# 14. Qualification / Conformance (`QUAL-*`)

| ID | Level | Class | Statement |
|---|---|---|---|
| QUAL-001 | MUST | STATIC | Tests, conformance, and qualification are separate concepts. |
| QUAL-002 | MUST | STATIC | Every MUST requirement in a claimed profile has a verification path. |
| QUAL-003 | MUST | EVIDENCE | A qualification result retains build, source, configuration, and environment identity. |
| QUAL-004 | MUST | STATIC | Profiles have dependency closure. |
| QUAL-005 | MUST | STATIC | The installation capability manifest explicitly identifies limitations of the claimed profile. |
| QUAL-006 | MUST | STATEFUL | Skipping a mandatory MUST requirement causes profile qualification to fail. |
| QUAL-007 | MUST | SECURITY | A high quality score cannot offset failure of a security/governance hard gate. |
| QUAL-008 | MUST | FAULT | AUTONOMY/TRUSTED qualification includes deterministic crash/fault scenarios. |
| QUAL-009 | MUST | RECOVERY | TRUSTED qualification includes recovery and revalidation scenarios. |
| QUAL-010 | SHOULD | STATEFUL | Cognitive qualification checks properties/evidence, not exact LLM wording. |
| QUAL-011 | MUST | INTEROP | Adapter/backend qualification verifies declared idempotency, recovery, and security semantics. |
| QUAL-012 | MUST | INTEROP | Unsupported compatibility is explicit, not silent best effort. |
| QUAL-013 | MUST | EVIDENCE | QualificationBundle contains suite version and per-requirement evidence/results. |
| QUAL-014 | MUST | STATEFUL | Material environment, provider, backend, policy, cryptographic, or schema changes can invalidate a qualification baseline or make it stale. |
| QUAL-015 | MUST | STATEFUL | Qualification is not a permanent product certificate. |
| QUAL-016 | MUST | SECURITY | Destructive qualification scenarios do not run against a production workspace without a special isolated qualification contract. |
| QUAL-017 | MUST | INTEROP | A federation participant's self-asserted `TRUSTED` status is not sufficient trust evidence. |
| QUAL-018 | MUST | EVIDENCE | A v1.0 Trust claim requires a successful TRUSTED profile and published known limitations. |

---

# 15. Release profile mapping

## v0.2 — Correct

Minimum target requirement families:

- `ARC-*` core laws;
- `MEM-*`;
- `TMP-*`;
- `MUT-*`;
- `RET-*`;
- foundational `CAP-*` security requirements required to protect Memory.

## v0.3 — Learn

Adds:

- `LRN-*`;
- cognition benchmarks/properties;
- feedback provenance.

## v0.4 — Govern

Adds complete:

- `CAP-*`;
- `IDW-*`;
- governance-facing negative tests.

## v0.5 — Understand

Adds:

- `HLT-*`;
- repair verification;
- recurrence/flapping scenarios.

## v0.6 — Connect

Adds:

- `EXT-*`;
- adapter/interoperability qualification.

## v1.0 — Trust

Requires the applicable:

- `REC-*`;
- `GOV-*`;
- `QUAL-*`;
- complete dependency closure of the preceding profiles.

---

# 16. Change control

1. Once used in release/conformance artifacts, a requirement ID SHOULD NOT change meaning.
2. Wording may be clarified without a new ID only when the observable obligation is unchanged.
3. Weakening or changing observable semantics requires a new version/replacement requirement and an ADR/contract update.
4. A removed requirement remains in historical mapping as superseded/retired.
5. Where possible, a new specialized specification MUST reference the relevant requirement IDs instead of creating duplicate, unidentified MUST statements.

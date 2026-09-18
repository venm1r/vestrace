# Vestrace Qualification / Conformance Specification v0.2

> **English reading edition · 2026-09-08.** Complete editorial translation of the [frozen original](../vestrace-qualification-conformance-spec-v0.2.md) from supplied snapshot `3e05dfbd`. Requirement IDs, normative strength, technical states, and examples are preserved. This translation does not amend the original contract or claim implementation. If wording differs, the frozen original and applicable Accepted ADRs take precedence.

**Status:** Normative qualification specification
**Date:** 2026-08-10

## 1. Purpose

This document defines how Vestrace demonstrates Architecture Contract conformance and when a build/deployment may claim a particular conformance/trust profile.

Main principle:

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

Unit, integration, and contract tests verify a particular implementation.

## 2.2 Conformance

Checks a component/adapter's observable behavior against stable normative requirement IDs.

## 2.3 Qualification

Determines whether a particular:

- source revision/build;
- configuration;
- environment;
- adapter set;
- policy/crypto profile

is suitable for the claimed profile.

---

# 3. Requirement source

Primary requirement source:

`docs/specs/vestrace-normative-invariants-v0.2.md`

Every MUST requirement in a profile's dependency closure needs:

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

An implementation surface MAY be a source file, module, adapter, schema, or runtime path, but architectural documentation does not fix Rust paths before implementation planning.

---

# 5. Conformance profiles

## 5.1 CORE

Minimum platform correctness:

- canonical vs derived boundaries;
- identity/versioning;
- execution history basics;
- default-deny governance fundamentals;
- invariant enforcement basics.

## 5.2 MEMORY

Adds:

- Memory/Revision;
- evidence/provenance;
- temporal correctness;
- conflict/supersession;
- retrieval/ContextPack properties;
- rebuildable derived memory state.

## 5.3 COGNITION

Adds:

- Claim/evaluation semantics;
- feedback/learning provenance;
- cognitive benchmarks/properties;
- asset/version learning gates.

## 5.4 AUTONOMY

Adds:

- governed execution;
- external effects;
- idempotency/reconciliation;
- repair;
- crash recovery.

## 5.5 FEDERATION

Adds:

- cross-workspace grants/mounts;
- federation identity/trust;
- remote qualification evidence;
- cross-boundary governance.

## 5.6 TRUSTED

Adds complete:

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

`TRUSTED-FEDERATED` means TRUSTED plus FEDERATION requirements.

A profile implementation MAY support subsets/features beyond that profile, but its qualification claim applies only to the dependency closure that passed.

---

# 7. Capability Manifest

A deployment/build SHOULD publish a machine-readable manifest:

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

A manifest is an assertion; qualification evidence determines whether it is substantiated.

---

# 8. Qualification target identity

The qualification target includes:

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

A material component change MAY make an earlier qualification stale.

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

Checks the required provenance, audit, receipt, and verification evidence.

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

A high metric score does not offset a hard-gate failure.

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

Foreign-workspace UUID guessing, alternate APIs, direct retriever paths, and cache paths all deny unauthorized disclosure.

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

Fault-point semantics/version SHOULD be stable enough for reproducible scenario suites.

---

# 13. Chaos testing

Randomized chaos MAY include:

- worker termination;
- dependency restarts;
- network latency/loss;
- provider timeout;
- resource pressure;
- clock skew within defined bounds.

A chaos suite does not replace deterministic fault cases because failures must be reproducible.

---

# 14. Security negative tests

Mandatory families:

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

Every governed operation SHOULD be tested through its available surfaces:

```text
HTTP
MCP
CLI
worker/background job
internal replay/recovery
federation adapter
```

An available surface must not bypass application governance.

---

# 16. Cognitive qualification

## 16.1 Property-oriented evaluation

LLM wording is nondeterministic; qualification checks properties:

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

Silent best-effort parsing is prohibited for trust-critical persisted facts.

---

# 19. Adapter qualification

Every external adapter declares properties and passes the corresponding cases.

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

A signature establishes bundle integrity and issuer identity, but the relying party still decides whether to trust the signer, suite, and profile.

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

Qualification applies to an exact target identity, not to an abstract product forever.

`Vestrace vX is qualified` is insufficiently precise without profile, build, and environment scope.

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

`v1.0` is defined by the Trust Contract, not feature count.

---

# 26. Known limitations

Qualification MUST permit accurate disclosure of limitations.

Example:

```text
SMTP adapter can confirm provider acceptance but cannot independently prove recipient delivery.
```

A limitation is not a qualification failure when the profile contract permits it and it is explicitly declared; claiming a stronger guarantee is prohibited.

---

# 27. Qualification findings

Runtime drift/conformance checks MAY create ordinary HealthFindings:

```text
QUALIFICATION_BASELINE_DRIFT
UNQUALIFIED_ADAPTER_ACTIVE
CRYPTO_PROFILE_MISMATCH
POLICY_CONFORMANCE_FAILED
SCHEMA_COMPATIBILITY_UNKNOWN
```

No separate health engine is created.

---

# 28. Federation qualification

Federated peers MAY exchange:

- capability manifest;
- qualification manifest/bundle refs;
- crypto profile;
- governance profile.

Local federation policy determines which:

- signers;
- suite versions;
- profiles;
- target environments

constitute sufficient evidence.

Self-asserted compliance insufficient.

---

# 29. Qualification isolation

Fault/destructive qualification runs in:

- isolated test environment;
- ephemeral workspace;
- explicitly designated non-production target;
- provider sandbox where supported.

Production destructive operations must not accidentally be used as qualification tests.

---

# 30. Conceptual operator/API surface

Future semantic commands MAY look like:

```text
vestrace conformance list
vestrace conformance check <profile>
vestrace qualify <profile>
vestrace qualify --fault-suite
vestrace qualification inspect <id>
vestrace qualification diff <old> <new>
```

This is a documentation-level contract; implementation-status documentation defines current CLI availability.

---

# 31. Machine-readable result

Qualification SHOULD support structured output, conceptually:

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

The exact schema will be a separate versioned artifact.

---

# 32. Normative mappings

Primary IDs:

- `QUAL-001..QUAL-018`;
- every MUST ID in the profile dependency closure;
- `EXT-*` for AUTONOMY;
- `REC-*` and `GOV-*` for TRUSTED;
- `IDW-*` for FEDERATION.

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

The Qualification Specification is baseline-complete when:

1. Every requirement family has a profile mapping.
2. High-risk, fault, and recovery golden scenarios cover the architecture contracts.
3. Mandatory profile gates include capability/security bypass tests.
4. Adapter contracts declare observable properties.
5. The release roadmap uses qualification profiles, not feature checklists alone.
6. The consistency pass across existing acceptance/docs separates current implementation claims from target qualification requirements.

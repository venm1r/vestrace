# Vestrace v0.2 Documentation Status

**Location:** `main`
**Inspected implementation baseline:** `729d456f70f4de93c97d05cce795c09025c62f24`
**Architecture integration:** PR #31, merge commit `a4d15d762b5da7b8b870bdb36c77c51c40b14f98`
**Date:** 2026-08-10
**Documentation state:** **FROZEN IN MAIN — POST-MERGE AUDIT PASSED**

See:

- pre-merge readiness record: [`documentation-readiness-v0.2.md`](documentation-readiness-v0.2.md);
- post-merge audit: [`documentation-post-merge-audit-v0.2.md`](documentation-post-merge-audit-v0.2.md).

## 1. Normative architecture

Completed and frozen:

- Architecture Contract v0.2;
- Domain Model v0.2;
- Normative Invariants Catalog;
- Trust & Authority Model;
- Data & Temporal Model;
- Execution & External Effects Contract;
- Health / Repair / Incident Contract;
- Crypto & Data Governance Contract;
- Qualification / Conformance Specification;
- v0.2 → v1.0 roadmap;
- ADR-0001…ADR-0010.

ADR-0009 clarifies finding disposition vs integrity state. ADR-0010 clarifies milestone labels vs named qualification-profile evidence closure.

## 2. Current implementation / gap analysis

The implementation snapshot and gap analysis remain pinned to implementation commit `729d456f70f4de93c97d05cce795c09025c62f24`:

- `docs/current-implementation.md`;
- `docs/gap-analysis-v0.2.md`;
- `docs/requirement-coverage-v0.2.md`;
- `docs/implementation-plan-v0.2.md`.

The current dirty checkout contains bounded, uncommitted C1-C4 deltas. See [`documentation-gap-delta-2026-08-11-c1-c4.md`](documentation-gap-delta-2026-08-11-c1-c4.md) for the foundation and [`documentation-gap-delta-2026-08-11-c4-service.md`](documentation-gap-delta-2026-08-11-c4-service.md) for the application service. These deltas do not replace the pinned baseline or qualify any profile.

The same checkout now contains a bounded C5 retrieval delta: [`documentation-gap-delta-2026-08-11-c5-exact-revision.md`](documentation-gap-delta-2026-08-11-c5-exact-revision.md). It records exact active-revision hydration and trusted workspace binding, while leaving capability/share/classification policy and PostgreSQL runtime qualification explicitly open.

The checkout also contains the bounded C6 ContextPack delta: [`documentation-gap-delta-2026-08-11-c6-context-pack.md`](documentation-gap-delta-2026-08-11-c6-context-pack.md). It records typed section routing, exact revision provenance, and the deterministic representation ladder; temporal, conflict, classification, mounted-retrieval, and profile-qualification gates remain open.

The checkout now contains the bounded C7 temporal/multi-channel delta: [`documentation-gap-delta-2026-08-11-c7-temporal-multichannel.md`](documentation-gap-delta-2026-08-11-c7-temporal-multichannel.md). It records validity-based `AsOf`, timeline/history selection, optional-channel degradation, and fail-closed retrieval; cache invalidation, conflicts, classification, mounts, provider wiring, and C8 qualification remain open.

The checkout now contains the bounded C8 fixture delta: [`documentation-gap-delta-2026-08-11-c8-core-memory-fixtures.md`](documentation-gap-delta-2026-08-11-c8-core-memory-fixtures.md). It maps the in-scope CORE+MEMORY MUST requirements and adds four deterministic golden fixtures, while explicitly retaining blocked mappings and the distinction between fixture evidence and profile qualification.

The checkout now contains the bounded L1 Evaluation Fact v2 delta: [`documentation-gap-delta-2026-08-11-l1-evaluation-fact-v2.md`](documentation-gap-delta-2026-08-11-l1-evaluation-fact-v2.md). It adds typed raw evaluation facts with exact target/evaluator/evidence/policy references and an isolated RLS persistence path; learned projections, proposals, self-escalation prevention, and COGNITION qualification remain open.

The checkout now also contains the bounded L2 learned projection/proposal delta: [`documentation-gap-delta-2026-08-11-l2-learning-projection-proposal.md`](documentation-gap-delta-2026-08-11-l2-learning-projection-proposal.md). It adds advisory learned projections, raw-fact lineage, typed proposal-only changes, and separate RLS persistence with no apply or self-escalation path; governance approval, verified mutation, rebuild/benchmark evidence, and COGNITION qualification remain open.

The checkout now contains the bounded L3 COGNITION benchmark delta: [`documentation-gap-delta-2026-08-11-l3-cognition-benchmarks.md`](documentation-gap-delta-2026-08-11-l3-cognition-benchmarks.md). It aligns the executable LRN registry with the normative catalog, adds evaluator-authority ordering and deterministic projection rebuild from canonical facts, and records a baseline/comparative property fixture. The fixture is evidence-only; full runtime qualification and `QualificationBundle` closure remain open.

The checkout now contains the bounded G1 Capability / Policy Decision kernel delta: [`documentation-gap-delta-2026-08-11-g1-capability-policy-kernel.md`](documentation-gap-delta-2026-08-11-g1-capability-policy-kernel.md). It adds typed constrained grants, fail-closed expiry/revocation/scope/budget/risk/condition evaluation, and evidence-bearing policy decisions with exact policy version and input state. G2 consumes this kernel for governed entry-point wiring; durable persistence and GOVERNANCE qualification remain open.

The checkout now contains the bounded G2 universal authorization boundary delta: [`documentation-gap-delta-2026-08-11-g2-universal-authorization-boundary.md`](documentation-gap-delta-2026-08-11-g2-universal-authorization-boundary.md). It wires the shared G1 boundary into governed HTTP, MCP, and run worker/internal paths with deny-before-dispatch evidence. Durable grant/decision persistence, generic job-poller context, federation adapters, and GOVERNANCE qualification remain open.

The checkout now contains the bounded G3 delegation/budget delta: [`documentation-gap-delta-2026-08-12-g3-delegation-budgets.md`](documentation-gap-delta-2026-08-12-g3-delegation-budgets.md). It adds an explicit `capability.delegate` identifier, fail-closed `DelegatedCapability` attenuation with bounded depth and validity/risk/scope checks, and an in-memory hierarchical reservation/charge ledger. Durable grant revisions, reservation/audit persistence, and full GOVERNANCE qualification remain open.

The checkout now contains the bounded G4 cross-workspace sharing delta: [`documentation-gap-delta-2026-08-12-g4-cross-workspace-sharing.md`](documentation-gap-delta-2026-08-12-g4-cross-workspace-sharing.md). It adds immutable-in-process source grant revisions, exact target-owned `MemoryMount` acceptance, independent source/target operation checks, stale/revoke/expiry denial, namespaced `SharedMemoryRef`, historical disclosure preservation, and an explicit legacy non-upgrade bridge. Durable persistence, transport/retrieval/cache wiring, PostgreSQL runtime evidence, federation trust, and full GOVERNANCE qualification remain open.

The checkout now contains the bounded G5/G6 federation/governance delta: [`documentation-gap-delta-2026-08-12-g5-g6-federation-governance.md`](documentation-gap-delta-2026-08-12-g5-g6-federation-governance.md). G5 adds exact remote identity and relationship lifecycle evaluation, with federated memory access requiring the independent G4 local grant/mount/policy intersection. G6 adds a deterministic FEDERATION/TRUSTED hard-gate evaluator for profile closure, required evidence, exact governance policy versions, skipped/inconclusive denial, duplicate/missing evidence and remote self-assertion denial. Durable federation/QualificationBundle persistence, transport/cryptographic/runtime wiring and release qualification remain open.

The checkout now contains the bounded H1–H5 Understand delta: [`documentation-gap-delta-2026-08-12-h1-h5-understand.md`](documentation-gap-delta-2026-08-12-h1-h5-understand.md). It adds the in-memory invariant/finding/occurrence registry, immutable repair and verification lifecycle, bounded recurrence/flapping/disposition/budget controls, inspect/plan/repair application and CLI contracts, and an HLT-001..HLT-019 Understand milestone gate. Durable health/repair persistence, execution adapters, restart/recovery evidence and v0.5 qualification remain open.

The checkout now contains the bounded E1–E4 Connect delta: [`documentation-gap-delta-2026-08-12-e1-e4-connect.md`](documentation-gap-delta-2026-08-12-e1-e4-connect.md). It adds explicit external-effect intents, adapter semantics, shared authorization, receipts, first-class UNKNOWN, strongest-evidence reconciliation, compensation-as-new-effect, deterministic fault cases and a Connect evidence gate. Durable effect history, provider adapters, crash/restart runtime evidence and v0.6 qualification remain open.

The checkout now contains the bounded T1–T8 Trust delta: [`documentation-gap-delta-2026-08-12-t1-t8-trust.md`](documentation-gap-delta-2026-08-12-t1-t8-trust.md). It adds incident/containment/trust-state barriers, recovery classification/revalidation, opaque secret/key lifecycle contracts, classification/DataPolicy/model boundaries, retention/hold/deletion verification, governed export/audit integrity, target-bound qualification baselines and a TRUSTED hard gate. Durable governance/recovery repositories, provider/KMS adapters, signed deployment bundles and v1.0 qualification remain open.

The checkout now also contains the bounded Q1 qualification-artifact delta: [`documentation-gap-delta-2026-08-12-q1-qualification-artifact.md`](documentation-gap-delta-2026-08-12-q1-qualification-artifact.md). It emits a target-bound machine-readable `QualificationBundle` from the deterministic conformance evaluator and preserves failed/skipped results, but durable qualification persistence, signed deployment evidence, runtime fault/recovery execution and a passing v1.0 profile remain open.

The checkout now also contains the bounded Q2 qualification-persistence delta: [`documentation-gap-delta-2026-08-12-q2-qualification-persistence.md`](documentation-gap-delta-2026-08-12-q2-qualification-persistence.md). It adds a global application repository port and PostgreSQL `qualification_bundles` storage with immutable retry/conflict semantics and lossless payload round-trip. The repository contract compiles, while live PostgreSQL verification remains blocked until `DATABASE_URL` is provided; signed deployment evidence, automatic runtime wiring, fault/recovery execution and a passing v1.0 profile remain open.

The checkout now also contains the bounded Q3 CLI persistence delta: [`documentation-gap-delta-2026-08-12-q3-cli-qualification-persistence.md`](documentation-gap-delta-2026-08-12-q3-cli-qualification-persistence.md). `conformance bundle --persist` now composes the standard configuration, migration verification, and `PgQualificationRepository` path after writing the local artifact. The current environment proves artifact-first failure ordering but still lacks `DATABASE_URL` for a successful live insert; worker/server automation, signed deployment evidence, runtime fault/recovery execution and a passing v1.0 profile remain open.

The checkout now also contains the bounded Q4 capability-manifest delta: [`documentation-gap-delta-2026-08-12-q4-capability-manifest.md`](documentation-gap-delta-2026-08-12-q4-capability-manifest.md). `conformance manifest` emits a normalized, digest-bearing `VestraceCapabilityManifest` without database access and records source/build/configuration/environment identity declarations. It remains an assertion only: runtime discovery, manifest signing, bundle binding, successful deployed qualification, and release approval remain open.

The checkout now also contains the bounded Q5 manifest-qualified-bundle delta: [`documentation-gap-delta-2026-08-12-q5-manifest-qualified-bundle.md`](documentation-gap-delta-2026-08-12-q5-manifest-qualified-bundle.md). `conformance bundle --target-manifest-file` validates the Q4 JSON assertion, binds its digest and exact identity fields into `QualificationBundle`, and rejects unsupported profiles or tampered declarations before artifact emission. Docker evidence verifies the restricted runtime PostgreSQL role, RLS scope, migration compatibility, and persistence of failed evidence; complete deployment qualification, signatures, automatic runtime qualification, and release approval remain open.

The checkout now also contains the bounded Q6 deployment-qualification delta: [`documentation-gap-delta-2026-08-12-q6-deployment-qualification.md`](documentation-gap-delta-2026-08-12-q6-deployment-qualification.md). `conformance verify` performs read-only exact manifest/bundle identity checks, rejects failed bundle status, and records migration/runtime-role evidence in a machine-readable result. Docker evidence shows the runtime and migration checks passing while the current bundle remains failed.

The checkout now also contains the bounded Q7 signed-artifacts delta: [`documentation-gap-delta-2026-08-12-q7-signed-artifacts.md`](documentation-gap-delta-2026-08-12-q7-signed-artifacts.md). Structured `SignatureRecord` metadata, canonical metadata-bound payloads, real Ed25519 local sign/verify commands, and tamper/wrong-key negative tests now cover capability manifests and qualification bundles. KMS/HSM/Vault adapters, signer trust policy, automatic server/worker qualification, fault/recovery execution, and release approval remain open; no profile or v1.0 claim follows from Q7.

The checkout now also contains the bounded Q8 automatic-runtime delta: [`documentation-gap-delta-2026-08-12-q8-automatic-runtime-qualification.md`](documentation-gap-delta-2026-08-12-q8-automatic-runtime-qualification.md). An explicit default-disabled configuration makes `server` and `worker` share a fail-closed deployment-qualification runner that collects live runtime-role/migration evidence, writes an artifact, and persists failed or passed bundles before startup continues. KMS/HSM/Vault resolution, signer trust policy, fault/recovery execution, post-incident requalification, release approval, and a passing v1.0 profile remain open.

The checkout now also contains the bounded Q9 key-provider/signer-trust delta: [`documentation-gap-delta-2026-08-12-q9-key-provider-signer-trust.md`](documentation-gap-delta-2026-08-12-q9-key-provider-signer-trust.md). Signing resolves material through the existing `KeyProvider` boundary, the explicit local-file adapter rejects unsupported providers, and both signature verification paths separate cryptographic validity from exact signer/key trust policy. KMS/HSM/Vault/OS-keyring custody, durable policy storage, rotation execution, fault/recovery execution, post-incident requalification, release approval, and a passing v1.0 profile remain open.

The checkout now also contains the bounded Q10 post-incident delta: [`documentation-gap-delta-2026-08-12-q10-post-incident-requalification.md`](documentation-gap-delta-2026-08-12-q10-post-incident-requalification.md). `POST_INCIDENT` bundles require typed incident/revalidation evidence, failed or inconclusive revalidation cannot yield a passed bundle, and a deterministic recovery gate checks one evidence-bearing safe action per target. Durable incident/revalidation repositories, runtime recovery/fault execution, and release approval remain open.

The checkout now also contains the bounded Q11 recovery-persistence delta: [`documentation-gap-delta-2026-08-12-q11-recovery-persistence.md`](documentation-gap-delta-2026-08-12-q11-recovery-persistence.md). Migration `0126` and `PgRecoveryRepository` persist Incident/RevalidationRun/TrustState/RecoveryPoint payloads with indexed integrity checks and retry semantics. Startup recovery orchestration, runtime reconciliation/fault execution, and release approval remain open.

The checkout now also contains the bounded Q12 startup-recovery-orchestration delta: [`documentation-gap-delta-2026-08-12-q12-startup-recovery-orchestration.md`](documentation-gap-delta-2026-08-12-q12-startup-recovery-orchestration.md). `StartupRecoveryService` validates candidate identity, applies the domain recovery classification, rebuilds safe projections, and reports reconciliation/abort/human-review barriers fail-closed. Durable candidate discovery, production startup wiring, runtime reconciliation/fault execution, and release approval remain open.

The checkout now also contains the bounded Q13 runtime-reconciliation delta: [`documentation-gap-delta-2026-08-12-q13-runtime-reconciliation.md`](documentation-gap-delta-2026-08-12-q13-runtime-reconciliation.md). `ExternalEffectReconciliationService` restricts recovery to the intent workspace and `UNKNOWN` receipts, requires evidence-bearing observations, and preserves the domain strongest-evidence outcome without automatic retry. Durable effect/reconciliation persistence, provider adapters, deterministic fault execution, progressive trust restoration, and release approval remain open.

The checkout now also contains the bounded Q14 effect-persistence delta: [`documentation-gap-delta-2026-08-12-q14-effect-persistence.md`](documentation-gap-delta-2026-08-12-q14-effect-persistence.md). Migration `0127` and `PgExternalEffectRepository` persist immutable intent/receipt/reconciliation payloads with indexed integrity checks and retry/conflict semantics. Startup discovery, provider read-back, deterministic fault execution, progressive trust restoration, and release approval remain open.

The checkout now also contains the bounded Q15 effect-fault-suite delta: [`documentation-gap-delta-2026-08-12-q15-effect-fault-suite.md`](documentation-gap-delta-2026-08-12-q15-effect-fault-suite.md). `ExternalEffectFaultSuiteService` executes all required deterministic fault points through an injected executor and preserves failed/unsafe evidence without converting it into a qualification claim. Production fault wiring, evidence persistence, progressive trust restoration, and release approval remain open.

The checkout now also contains the bounded Q16 effect-recovery-discovery delta: [`documentation-gap-delta-2026-08-12-q16-effect-recovery-discovery.md`](documentation-gap-delta-2026-08-12-q16-effect-recovery-discovery.md). Migration `0128`, indexed reconciliation identity, workspace-scoped UNKNOWN discovery, and an injected provider read-back/reconciliation service now survive restart at the repository boundary. Production startup wiring, provider-specific adapters, production fault evidence, progressive trust restoration, and release approval remain open.

The checkout now also contains the bounded Q17 fault-suite-evidence delta: [`documentation-gap-delta-2026-08-12-q17-fault-suite-evidence-persistence.md`](documentation-gap-delta-2026-08-12-q17-fault-suite-evidence-persistence.md). Migration `0129` and `PgFaultSuiteEvidenceRepository` persist target-bound deterministic fault observations, including failed/unsafe decisions, with immutable retry/conflict and payload-integrity semantics. Production fault injection, qualification-runner consumption, progressive trust restoration, and release approval remain open.

The checkout now also contains the bounded Q18 fault-suite-qualification delta: [`documentation-gap-delta-2026-08-12-q18-fault-suite-qualification-orchestration.md`](documentation-gap-delta-2026-08-12-q18-fault-suite-qualification-orchestration.md). `ExternalEffectFaultQualificationService` composes deterministic execution with durable evidence persistence and preserves failed results without promotion. Production fault injector wiring, complete qualification-bundle consumption, progressive trust restoration, and release approval remain open.

The checkout now also contains the bounded Q19 fault-suite-evidence-admission delta: [`documentation-gap-delta-2026-08-12-q19-fault-suite-evidence-admission.md`](documentation-gap-delta-2026-08-12-q19-fault-suite-evidence-admission.md). `ExternalEffectFaultEvidenceAdmissionService` reads target-bound evidence and admits only an exact-target passing result; missing, failed, and mismatched evidence remain policy failures. Complete qualification-bundle consumption, trust restoration, and release approval remain open.

The checkout now also contains the bounded Q20 fault-gate-evidence delta: [`documentation-gap-delta-2026-08-12-q20-fault-gate-evidence.md`](documentation-gap-delta-2026-08-12-q20-fault-gate-evidence.md). `ExternalEffectFaultGateEvidenceService` exposes exact-target fault evidence as local `QUAL-008` hard-gate evidence and preserves failed results as `Fail`. Complete bundle assembly, trust restoration, and release approval remain open.

The checkout now also contains the bounded Q21 fault-runtime-wiring delta: [`documentation-gap-delta-2026-08-12-q21-fault-runtime-wiring.md`](documentation-gap-delta-2026-08-12-q21-fault-runtime-wiring.md). `ConfiguredEffectFaultScenarioExecutor` provides a target-bound, non-production runtime hook with disabled-by-default caller control and point validation. Process crash execution, provider-specific destructive adapters, complete bundle assembly, trust restoration, and release approval remain open.

The checkout now also contains the bounded Q22 fault-bundle-assembly delta: [`documentation-gap-delta-2026-08-12-q22-fault-bundle-assembly.md`](documentation-gap-delta-2026-08-12-q22-fault-bundle-assembly.md). `ExternalEffectQualificationBundleService` consumes exact-target persisted fault evidence, adds it as `QUAL-008` hard-gate evidence and a conformance case, then persists a manifest-bound passed or failed bundle. A real isolated process/provider fault driver, trust restoration, and release approval remain open.

The checkout now also contains the bounded Q23 fault-process-runtime delta: [`documentation-gap-delta-2026-08-12-q23-fault-process-runtime.md`](documentation-gap-delta-2026-08-12-q23-fault-process-runtime.md). `ProcessFaultInjectionRuntime` executes an explicitly configured child with target/point/isolation forwarding, minimal inherited environment, timeout termination, and strict observation parsing. Provider sandbox adapters, Docker qualification evidence, trust restoration, and release approval remain open.

The checkout now also contains the bounded Q24 progressive-trust-restoration delta: [`documentation-gap-delta-2026-08-12-q24-progressive-trust-restoration.md`](documentation-gap-delta-2026-08-12-q24-progressive-trust-restoration.md). `ProgressiveTrustRestorationService` persists `REVALIDATING` and permits `Trusted` only after exact post-incident evidence, manifest binding, qualified baseline matching, and full local Trusted hard-gate closure.

The checkout now also contains the bounded Q25 Trusted release-approval delta: [`documentation-gap-delta-2026-08-12-q25-release-approval.md`](documentation-gap-delta-2026-08-12-q25-release-approval.md). `ReleaseApprovalService` requires exact release/profile/manifest identity, a passed bundle, a qualified matching baseline, Trusted state, full hard-gate closure, cryptographically verified and policy-trusted signatures, and published non-blank limitations. This closes the aggregate application decision boundary; fresh exact-environment qualification and v1.0 release evidence are still required before claiming v1.0.

The checkout now also contains the bounded Q26 capability-restoration delta: [`documentation-gap-delta-2026-08-12-q26-capability-restoration.md`](documentation-gap-delta-2026-08-12-q26-capability-restoration.md). `CapabilityRestorationService` enforces explicit progressive stages and trust/evidence barriers, including diagnostics-only behavior for Untrusted/Revalidating and a deterministic-write ceiling for DegradedTrust. Durable grant mutation, provider/KMS qualification, and exact-environment v1.0 evidence remain open.

The checkout now also contains the bounded Q27 crypto/provider qualification delta: [`documentation-gap-delta-2026-08-12-q27-crypto-adapter-qualification.md`](documentation-gap-delta-2026-08-12-q27-crypto-adapter-qualification.md). `CryptoAdapterQualificationService` rejects development-only custody and requires exact key metadata plus complete production evidence. It defines the gate but does not provide KMS/HSM/Vault adapters or live provider evidence; exact-environment v1.0 qualification remains open.

The checkout now also contains the bounded Q28 exact-environment release-evidence delta: [`documentation-gap-delta-2026-08-12-q28-v1-release-evidence.md`](documentation-gap-delta-2026-08-12-q28-v1-release-evidence.md). `V1ReleaseEvidenceService` binds exact release/build/configuration/environment identity to typed release approval, runtime deployment, crypto, recovery/fault, and capability-restoration decisions, rejects missing or drifting evidence, and requires unique evidence references and published limitations. It is the final application-side v1.0 gate; live Docker/runtime/provider evidence and a passed deployment-specific TRUSTED profile are still required before making a v1.0 claim.

The checkout now also contains the bounded Q29 diagnostics lease-schema correction: [`documentation-gap-delta-2026-08-12-q29-diagnostics-lease-schema.md`](documentation-gap-delta-2026-08-12-q29-diagnostics-lease-schema.md). The PostgreSQL doctor query now uses `run_leases.lease_until`, matching migration `0021` and the run lease repository; a source contract prevents regression to the nonexistent `expires_at` column. A fresh exact-environment Docker audit is still required.

The pinned implementation baseline remains the historical comparison point. The current dirty checkout contains bounded, uncommitted runtime and documentation deltas; each slice above records its actual scope and non-claims, and none of them qualifies a release profile.

Main conclusion: preserve the existing Run/RLS/Memory/idempotency/jobs/outbox/diagnostics foundation; normalize target authority models instead of rewriting the repository.

## 3. Future 36-PR implementation package

Documentation complete for:

```text
0.1–0.2
C1–C8
L1–L3
G1–G6
H1–H5
E1–E4
T1–T8
```

Primary navigation:

- `docs/plans/README.md`;
- `docs/plans/v0.2-to-v1.0-pr-specification-index.md`.

Global planning artifacts include:

- 36-PR execution matrix;
- dependency/parallelization map;
- requirement → PR traceability;
- schema impact summary;
- migration rollout/compatibility contract;
- conformance-case index;
- release evidence gates;
- risk register;
- universal future PR review checklist.

Per-phase artifacts include detailed contracts plus migration/backfill and conformance matrices for Correct, Learn, Govern, Understand, Connect and Trust.

## 4. Consistency corrections completed

The two documentation audits together resolved:

- stable conformance case-ID collision;
- applicability vs applicable-case execution status;
- `MEM-011` delivery/evidence ownership;
- `RET-014` mounted-retrieval applicability handoff;
- v0.2 foundational Memory security closure;
- AUTONOMY/FEDERATION/TRUSTED claim boundaries through ADR-0010;
- current vs historical planning-document classification;
- stale pre-merge branch/phase wording after integration into `main`.

## 5. Conflict precedence

For target architecture:

```text
Architecture Contract v0.2
→ newer Accepted ADR
→ specialized normative v0.2 spec
→ Normative Invariants Catalog
→ transition planning docs
→ historical designs/plans
```

For implementation reality:

```text
source/migrations/tests
→ current implementation docs
→ planning assumptions
```

## 6. Completion checklist

- [x] 12 architecture blocks consolidated.
- [x] stable requirement IDs defined.
- [x] implementation reality separated from target architecture.
- [x] source-based gap analysis completed.
- [x] all 36 future PRs documented.
- [x] detailed contracts created for all phases.
- [x] migration/backfill matrices created.
- [x] conformance-case matrices/index created.
- [x] requirement → PR traceability created.
- [x] rollout/release/risk/review contracts created.
- [x] stable case IDs reconciled.
- [x] applicability semantics reconciled.
- [x] milestone/profile claims reconciled.
- [x] current/historical plans classified.
- [x] architecture baseline merged into `main`.
- [x] post-merge stale branch/phase wording corrected.
- [x] post-merge audit found no remaining blocker in the current v0.2 documentation set.

## 7. Maintenance rule

The v0.2 architecture baseline remains frozen unless changed deliberately through spec/ADR amendment.

If runtime code, migrations, provider boundaries, policy semantics, or other implementation assumptions move materially from the inspected implementation baseline, re-run a gap delta before executing the 36-PR package unchanged.

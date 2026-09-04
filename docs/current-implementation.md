# Vestrace Current Implementation Snapshot

**Snapshot source:** `main@729d456f70f4de93c97d05cce795c09025c62f24`
**Snapshot date:** 2026-08-07

This document retains the historical `main@729d456...` comparison snapshot. The current uncommitted G1-G4 implementation slices are tracked by their dated deltas; the G4 slice reviewed against `HEAD 568f3d5` is [`documentation-gap-delta-2026-08-12-g4-cross-workspace-sharing.md`](documentation-gap-delta-2026-08-12-g4-cross-workspace-sharing.md). The current H1–H5 slice is tracked by [`documentation-gap-delta-2026-08-12-h1-h5-understand.md`](documentation-gap-delta-2026-08-12-h1-h5-understand.md), E1–E4 by [`documentation-gap-delta-2026-08-12-e1-e4-connect.md`](documentation-gap-delta-2026-08-12-e1-e4-connect.md), and T1–T8 by [`documentation-gap-delta-2026-08-12-t1-t8-trust.md`](documentation-gap-delta-2026-08-12-t1-t8-trust.md).
**Verified during gap analysis:** 2026-08-10

> This document describes what is currently implemented/wired in the source snapshot. It is not the target Architecture Contract v0.2.

## 1. Target architecture

The normative target is defined in:

- [`specs/vestrace-architecture-contract-v0.2.md`](specs/vestrace-architecture-contract-v0.2.md)
- [`specs/README.md`](specs/README.md)

Gap-analysis artifacts:

- [`gap-analysis-v0.2.md`](gap-analysis-v0.2.md)
- [`requirement-coverage-v0.2.md`](requirement-coverage-v0.2.md)
- [`implementation-plan-v0.2.md`](implementation-plan-v0.2.md)

Canonical product definition:

> **Vestrace is a memory-first platform for persistent cognition shared across agents and executions.**

## 2. Repository/runtime shape

Current source is a Rust Edition 2024 workspace with:

```text
crates/vestrace-domain
crates/vestrace-application
crates/vestrace-infrastructure
crates/vestrace-http
crates/vestrace-cli
crates/vestrace-mcp
crates/vestrace-rig-spike
```

plus root integration tests, SQLx migrations, Docker/Compose, console and documentation.

The intended dependency direction is inward:

```text
CLI / HTTP / Infrastructure
        ↓
Application
        ↓
Domain
```

`vestrace-rig-spike` is experimental and not part of the supported runtime contract.

## 3. Currently wired Run/execution foundation

Current runtime includes:

- event-sourced Run command execution;
- deterministic replay/reducer;
- optimistic Run concurrency via `RunVersion`;
- event type/version validation;
- causation/correlation IDs;
- `occurred_at` + `recorded_at` on Run events;
- run checkpoints;
- checkpoint validation/restoration;
- run projection rebuild;
- append-only RunEvent history;
- PostgreSQL-backed projection/event repositories.

The repository also contains `WorkflowExecution`, `StepExecution`, model/tool execution records and other execution-history types. Gap analysis treats these as reusable typed history/operation foundations that must be explicitly reconciled to the Run-first ownership model so they do not evolve into a competing runtime.

## 4. Currently wired Memory foundation

Implemented/wired paths include:

- stable Memory identity;
- Memory revisions with sequential revision number;
- Memory lifecycle transitions;
- memory creation with first revision;
- structured memory variants;
- provenance source persistence;
- derivation primitives;
- memory revision with optimistic concurrency at the application/repository boundary;
- relation linking;
- event recording;
- idempotency/outbox integration;
- memory/event read paths;
- active-source database invariant;
- authorized hard purge path.

Current gaps relative to target include Claim/ClaimAssessment/Conflict, generic validity intervals, richer exact evidence references, classification/change-reason/actor/hash metadata on revisions and normalized reconciliation semantics.

## 5. Retrieval currently wired

Current runtime includes:

- retrieval request normalization;
- `RetrievalIntent` and `TimePerspective` domain/request types;
- PostgreSQL full-text search channel;
- RRF fusion;
- deterministic reranking;
- token-bounded ContextPack domain/builder;
- representation enum `Full/Summary/Atomic/Reference`;
- retrieval journaling;
- degraded flag/warnings when the wired text channel fails.

Important semantic limitations confirmed during gap analysis:

- the HTTP server wires text/FTS and, when configured, the vector channel; MCP currently wires text/FTS only, while exact and structured retriever ports have no production composition root;
- text/FTS resolves `Current`, `AsOf`, Timeline and History against explicit revision selections;
- every discovered candidate carries an exact revision reference and is hydrated from that revision before fusion and reranking;
- `ContextPackBuilder` renders the hydrated revision content and retains its stored source classification;
- section assignment is currently a structural placeholder rather than semantic kind/authority-based placement;
- the current overflow path truncates text and labels it `Reference`; it is not yet a true provenance-preserving semantic compression ladder;
- reranking uses placeholder/neutral metadata for recency/provenance and derives importance/confidence from candidate score rather than canonical Memory metadata;
- exact-revision classification is enforced by the configured retrieval policy with structured, non-content-disclosing withholding; capability/share/conflict governance is not yet enforced by the retrieval pipeline.

Therefore current retrieval is a useful **pipeline shell**, not yet target ContextPack 2.0.

## 6. Feedback / evaluations currently wired

Current source includes:

- model routing/execution foundations;
- workflow/step execution records;
- HTTP evaluation create/list/get endpoints;
- PostgreSQL evaluation repository;
- versioned Agent/Skill/Workflow domain definitions.

Legacy `EvaluationRecord` remains a generic record (`model_id?`, name, status, score, summary, created_at); it is not the typed raw-fact or learned-projection/proposal path.

The bounded L1/L2/L3 implementation delta adds an additive typed `EvaluationFact` path, separate `LearnedProjection`/`LearningProposal` paths, and a deterministic COGNITION property fixture. L2 projections are advisory, retain raw-fact/evidence provenance, and proposals target versioned cognitive/model/tool/routing assets only. L3 rebuilds an advisory projection from canonical facts and retains source measurements; no proposal approval, asset application, capability/permission mutation, or self-escalation path is implemented. The legacy evaluation CRUD remains unchanged, and the fixture does not claim a full COGNITION qualification.

The bounded G1/G2/G3/G4 deltas add a typed constrained `CapabilityGrant` / `PolicyDecision` kernel, application `GrantPolicyEngine` and `BudgetPolicyEngine` adapters, and a shared `AuthorizationBoundary`. G2 wires that boundary into governed HTTP `/v1`/`/ag-ui`, MCP tool dispatch, and the run worker/internal handler path with production default deny and deny-before-dispatch tests. G3 adds an explicit `capability.delegate` identifier, immutable-in-process `DelegatedCapability` attenuation, bounded delegation depth, and `HierarchicalBudget` reservation/charge checks before an allow is returned. G4 adds immutable-in-process source grant revisions, exact target-owned mounts, independent policy intersection, stale/revoke/expiry denial, namespaced shared references, and historical disclosure preservation. G5 adds exact remote identity/federation relationship lifecycle and a trust evaluator that cannot authorize local memory without the G4 grant/mount/policy intersection. G6 adds a deterministic Govern/Federation hard gate for dependency closure and required evidence, including skipped/inconclusive and remote-self-assertion denial. Durable grant/decision/revision/mount/trust persistence, generic job-poller trusted context, transport/federation adapters, classification wiring, full QualificationBundle and release qualification remain open.

## 7. Security/governance currently wired

Current code contains:

- 26 stable capability enum/string identifiers;
- sensitivity ordering `Public < Internal < Confidential < Restricted`;
- approval records with optional operation hash + expiry;
- `PolicyEngine` abstraction;
- test-only allow-all, capability-set and workspace-scoped policy helpers;
- redaction service;
- audit repository;
- HTTP auth/local-trusted foundation;
- a simple numeric/monetary BudgetAccount reservation primitive;
- policy-bundle and approval schema foundations.

Important limitations:

- the G1 domain now contains a typed constrained `CapabilityGrant` aggregate, but no durable PostgreSQL persistence path is wired;
- G3 attenuation is an in-memory value-object kernel: child authority requires an active parent, an explicit `capability.delegate` grant, bounded selectors/validity/risk/depth, preserved parent conditions, and a reserved budget subset; grant revisions and delegation audit persistence are not wired;
- G1 adds central risk categories/effective-risk input and an evidence-producing target `PolicyDecision` in the domain/application layer;
- G2 demonstrates shared `PolicyDecisionEngine` enforcement across the governed HTTP, MCP, and run worker/internal paths, including typed operation risk and distinct evaluation/learning read/write capabilities; the generic job poller lacks trusted workspace/principal context and federation adapter enforcement remains open;
- the G3 ledger accounts for one hierarchical unit dimension only; full multi-dimensional budget accounting, releases/corrections, and durable audit are not implemented.

## 8. Workspace / sharing / enterprise seeds

Current RLS/workspace isolation is a strong implemented foundation.

The repository also contains early enterprise types/schema:

```text
WorkspaceKek
CrossWorkspaceMemoryGrant
```

and migration 0087 creates workspace key metadata and a simple cross-workspace memory-grant table.

These are **type/schema seeds**, not target sharing conformance. The current simple grant is one-sided and lacks:

- immutable grant revisions;
- target-owned MemoryMount acceptance;
- grant/mount lifecycle and stale/revoke semantics;
- operation-specific permissions;
- SharedMemoryRef;
- independent source/target policy decisions.

The additive G4 domain kernel now covers those semantics in memory through `MemoryShareGrantRevision`, `MemoryShareGrant`, `TargetSharePolicy`, `MemoryMount`, `SharedMemoryRef`, and `ShareDisclosure`. It does not retrofit migration 0087, persist lifecycle/audit state, wire transport or retrieval paths, or claim PostgreSQL runtime/RLS evidence. The legacy one-sided type cannot auto-upgrade into a mount.

The additive G5/G6 kernel now adds `RemoteIdentity`, `FederationRelationship`, `FederationTrustDecision`, and `GovernanceFederationGate`. Federation trust is scoped to an exact local workspace, remote workspace/principal, operation and policy version; it is never a local data permission. Federated memory access still requires the G4 source grant, target mount and target policy. The hard gate checks profile closure and fails on missing, duplicate, failed, not-applicable, skipped, inconclusive, unreferenced, stale-policy or remote-self-asserted evidence. These are in-memory/domain and fixture boundaries, not durable federation adapters or a QualificationBundle.

The additive H1–H5 kernel now adds a unique in-memory `InvariantRegistry`, evidence-bearing `HealthFinding` plus separate `HealthOccurrence`, declared-dependency `HealthProjection`, immutable `RepairPlan`/`RepairExecution`/`VerificationRun`, bounded recurrence/flapping and repair-budget controls, auditable suppression/accepted-risk dispositions, and inspect/plan/repair application/CLI contracts. Understand evidence is evaluated by a dedicated HLT-001..HLT-019 milestone gate. These contracts remain in-memory and do not claim durable lifecycle persistence, an execution adapter, migration changes, or v0.5 qualification.

The additive E1–E4 kernel now adds immutable `ExternalEffectIntent`, adapter-declared delivery/idempotency/reversibility/dry-run semantics, shared application authorization, final precondition recheck, immutable receipts, explicit `UNKNOWN`/reconciliation-required outcomes, strongest-evidence reconciliation, compensation-as-new-effect, deterministic fault observations and a v0.6 `ConnectGate`. These are contract and evidence boundaries only: durable effect history, provider/network adapters, crash injection and deployed qualification remain open.

The additive T1–T8 kernel now adds typed Incident/Containment/TrustState and RevalidationRun contracts, startup recovery classification, opaque SecretRef/KeyProvider and key lifecycle metadata, conservative DataClassification/DataPolicy/model-boundary decisions, retention/hold/deletion verification, governed export bundles and digest-chained audit integrity, target-bound QualificationBundle/Baseline lifecycle, and a TRUSTED hard gate over the existing requirement registry. These are contract-first/in-memory boundaries: durable repositories, KMS/HSM/Vault wiring, storage/network disposal, crash orchestration and v1.0 qualification remain open. Q7 now adds the signed-artifact contract and local Ed25519 verification described below; Q8 adds opt-in automatic server/worker deployment qualification described below; Q9 adds provider-bound local signing and exact signer trust evaluation described below.

The bounded Q1 artifact slice now makes the existing conformance result reproducible as a target-bound JSON `QualificationBundle`, including lifecycle, profile-scoped results, target digest and known limitations. It intentionally writes failed evidence before returning a non-zero command result; this is an artifact/reporting boundary, not a passing profile or release certificate. See [`documentation-gap-delta-2026-08-12-q1-qualification-artifact.md`](documentation-gap-delta-2026-08-12-q1-qualification-artifact.md).

The bounded Q2 persistence slice adds the global application `QualificationRepository` port and PostgreSQL `PgQualificationRepository` over migration `0125`. It retains the complete bundle JSON plus indexed target/lifecycle/profile/status metadata, validates indexed metadata on read, and treats exact retries as idempotent while rejecting conflicting reuse of a bundle id. This makes qualification evidence durable, but does not automatically persist CLI/worker runs, sign evidence, prove deployment identity, or qualify a profile. See [`documentation-gap-delta-2026-08-12-q2-qualification-persistence.md`](documentation-gap-delta-2026-08-12-q2-qualification-persistence.md).

The bounded Q3 CLI wiring adds explicit `conformance bundle --persist`. The command keeps local artifact emission database-free unless requested; with `--persist`, it writes the artifact first, then uses the standard `AppConfig`/`PgStore` migration and compatibility path before inserting through `PgQualificationRepository`. Persistence or database setup failures remain errors, and a persisted failed qualification still exits non-zero. Automatic worker/server qualification, successful deployed PostgreSQL evidence, signatures, and release approval remain open. See [`documentation-gap-delta-2026-08-12-q3-cli-qualification-persistence.md`](documentation-gap-delta-2026-08-12-q3-cli-qualification-persistence.md).

The bounded Q4 capability-manifest slice adds a domain-owned `VestraceCapabilityManifest` and database-free `conformance manifest` artifact command. It requires and deterministically normalizes product/build/configuration/environment identity declarations plus supported capability lists, and computes a stable `sha256:` assertion digest. The digest is not a signature or runtime proof; the command does not discover deployment state, bind the manifest into a qualification bundle, or establish runtime qualification. See [`documentation-gap-delta-2026-08-12-q4-capability-manifest.md`](documentation-gap-delta-2026-08-12-q4-capability-manifest.md).

The bounded Q5 manifest-binding slice adds validated serialized-manifest loading, `QualificationBundle::from_conformance_report_for_manifest`, and `conformance bundle --target-manifest-file`. File mode uses the manifest digest and exact source/build/configuration/environment identity, rejects unsupported profiles and tampered/non-normalized manifests, and preserves legacy explicit-string mode. Docker live evidence now proves compatible migration history, restricted runtime role/RLS behavior, and persistence of a failed deployment bundle; it does not qualify the profile or establish signatures, automatic qualification, or v1.0 readiness. See [`documentation-gap-delta-2026-08-12-q5-manifest-qualified-bundle.md`](documentation-gap-delta-2026-08-12-q5-manifest-qualified-bundle.md).

The bounded Q8 runtime-wiring slice adds default-disabled typed qualification configuration and a shared automatic runner invoked by both `server` and `worker` before readiness/polling. It collects live migration/runtime-role evidence, writes and persists a deployment bundle including failed evidence, and fails closed on a failed decision. It does not claim a passed profile, signature trust, KMS/HSM/Vault resolution, fault/recovery execution, or release approval. See [`documentation-gap-delta-2026-08-12-q8-automatic-runtime-qualification.md`](documentation-gap-delta-2026-08-12-q8-automatic-runtime-qualification.md).

The bounded Q9 trust-boundary slice adds `SignerTrustRule`/`SignerTrustPolicy` exact metadata allowlisting, fail-closed lifecycle checks, and a `KeyProvider`-backed local-file signing adapter. `conformance verify-signature` and `conformance verify` now expose cryptographic and trusted-signer verdicts separately and require an explicit trust policy for a trusted result. It does not add KMS/HSM/Vault/OS-keyring custody, remote signing, durable policy storage, rotation execution, or release approval. See [`documentation-gap-delta-2026-08-12-q9-key-provider-signer-trust.md`](documentation-gap-delta-2026-08-12-q9-key-provider-signer-trust.md).

The bounded Q10 slice adds typed `PostIncidentQualificationEvidence`, binds it to `POST_INCIDENT` bundles, requires `conformance bundle --post-incident-evidence-file`, and keeps unsuccessful revalidation from producing a passed bundle while preserving the artifact. It also adds a deterministic recovery qualification gate that checks exactly-once target observations and safe classified actions. Durable incident/revalidation repositories, runtime recovery orchestration, isolated crash/fault execution, and release approval remain open. See [`documentation-gap-delta-2026-08-12-q10-post-incident-requalification.md`](documentation-gap-delta-2026-08-12-q10-post-incident-requalification.md).

The bounded Q11 slice adds the application `RecoveryRepository`, migration `0126`, and `PgRecoveryRepository` for lossless Incident/RevalidationRun/TrustState/RecoveryPoint evidence. Immutable records are retry-idempotent with indexed-payload integrity checks; trust state is append-oriented with latest-by-scope lookup. See [`documentation-gap-delta-2026-08-12-q11-recovery-persistence.md`](documentation-gap-delta-2026-08-12-q11-recovery-persistence.md).

The bounded Q12 slice adds `StartupRecoveryService`, which validates unique recovery candidates, classifies them through the domain recovery matrix, rebuilds projections only for `SAFE_TO_RESUME` and `SAFE_TO_RETRY`, and emits explicit reconciliation/abort/human-review outcomes for unsafe barriers. Any rebuild failure aborts the report fail-closed. This is an application orchestration boundary; durable candidate discovery, server/worker startup wiring, runtime reconciliation, isolated fault execution, and release approval remain open. See [`documentation-gap-delta-2026-08-12-q12-startup-recovery-orchestration.md`](documentation-gap-delta-2026-08-12-q12-startup-recovery-orchestration.md).

The bounded Q13 slice adds `ExternalEffectReconciliationService` over the existing immutable intent/receipt model. It is workspace-scoped, accepts only `UNKNOWN` receipts, requires observations, delegates strongest-evidence selection to the domain reconciler, and exposes no automatic dispatch/retry path. Durable effect/reconciliation persistence, startup discovery wiring, provider read-back adapters, deterministic fault injection, progressive trust restoration, and release approval remain open. See [`documentation-gap-delta-2026-08-12-q13-runtime-reconciliation.md`](documentation-gap-delta-2026-08-12-q13-runtime-reconciliation.md).

The bounded Q14 slice adds `ExternalEffectRepository`, migration `0127`, and `PgExternalEffectRepository` for immutable intent, receipt, and reconciliation evidence. JSONB payloads remain authoritative; indexed identity/status fields are validated on read; exact retries are idempotent and conflicting immutable IDs fail closed. Durable startup discovery, provider read-back adapters, deterministic fault execution, progressive trust restoration, and release approval remain open. See [`documentation-gap-delta-2026-08-12-q14-effect-persistence.md`](documentation-gap-delta-2026-08-12-q14-effect-persistence.md).

The bounded Q15 slice adds `ExternalEffectFaultSuiteService` over an injected deterministic executor. It runs every required effect fault point, rejects point/observation mismatches, preserves unsafe-retry failures as a non-passed decision, and fails closed when the executor is unavailable. This is a harness/application boundary only; it does not inject faults into production adapters, persist fault evidence, perform chaos testing, restore trust, or qualify a release. See [`documentation-gap-delta-2026-08-12-q15-effect-fault-suite.md`](documentation-gap-delta-2026-08-12-q15-effect-fault-suite.md).

The bounded Q16 slice adds durable UNKNOWN-effect discovery and provider read-back orchestration. `PgExternalEffectRepository` returns only workspace-scoped UNKNOWN receipts without an existing reconciliation, validates indexed identity against the authoritative payload, and `ExternalEffectRecoveryService` delegates injected observations to the existing strongest-evidence reconciler before persisting a new immutable fact. It never dispatches or retries the original effect. Production startup wiring, provider-specific adapters, production fault injection/evidence, progressive trust restoration, and release qualification remain open. See [`documentation-gap-delta-2026-08-12-q16-effect-recovery-discovery.md`](documentation-gap-delta-2026-08-12-q16-effect-recovery-discovery.md).

The bounded Q17 slice adds target-bound `ExternalEffectFaultSuiteEvidence`, migration `0129`, and `PgFaultSuiteEvidenceRepository`. It persists all required fault observations and failed/unsafe decisions with immutable retry/conflict semantics and indexed-payload integrity checks. It does not execute production fault injection, consume evidence into qualification, restore trust, or qualify a release. See [`documentation-gap-delta-2026-08-12-q17-fault-suite-evidence-persistence.md`](documentation-gap-delta-2026-08-12-q17-fault-suite-evidence-persistence.md).

The bounded Q18 slice adds `ExternalEffectFaultQualificationService`, which composes the deterministic fault suite with target-bound evidence persistence. Passed and failed decisions are both persisted with their original semantics, while executor errors fail before persistence. Production injector selection, full `QualificationBundle` consumption, progressive trust restoration, and release approval remain open. See [`documentation-gap-delta-2026-08-12-q18-fault-suite-qualification-orchestration.md`](documentation-gap-delta-2026-08-12-q18-fault-suite-qualification-orchestration.md).

The bounded Q19 slice adds `ExternalEffectFaultEvidenceAdmissionService`, a read-side gate that admits only persisted passed evidence whose target digest exactly matches the requested deployment target. Missing, failed, or mismatched evidence is rejected as a policy failure, and repository errors propagate unchanged. It does not attach evidence to a complete `QualificationBundle`, restore trust, or approve a release. See [`documentation-gap-delta-2026-08-12-q19-fault-suite-evidence-admission.md`](documentation-gap-delta-2026-08-12-q19-fault-suite-evidence-admission.md).

The bounded Q20 slice adds `ExternalEffectFaultGateEvidenceService`, which maps exact-target persisted fault evidence into local executable `QUAL-008` hard-gate evidence. Passed evidence maps to `Pass`, failed evidence remains `Fail`, and missing, mismatched, or unreadable evidence fails closed. It does not assemble a complete `QualificationBundle`, restore trust, or approve a release. See [`documentation-gap-delta-2026-08-12-q20-fault-gate-evidence.md`](documentation-gap-delta-2026-08-12-q20-fault-gate-evidence.md).

The bounded Q21 slice adds `ConfiguredEffectFaultScenarioExecutor` with explicit non-production isolation settings and a `FaultInjectionRuntime` hook. Disabled injection is rejected before runtime invocation; configured runs carry the exact target digest and fault point, validate the returned point, and propagate runtime failures. Real process crash/kill orchestration, provider-specific destructive execution, and automatic release qualification remain open. See [`documentation-gap-delta-2026-08-12-q21-fault-runtime-wiring.md`](documentation-gap-delta-2026-08-12-q21-fault-runtime-wiring.md).

The bounded Q22 slice adds `ExternalEffectQualificationBundleService`, which loads exact-target persisted fault evidence, maps it to `QUAL-008` hard-gate evidence and a conformance result, binds the assembled report to the capability manifest, and persists passed or failed bundles. Duplicate `QUAL-008` inputs and repository failures fail closed. A real isolated process/provider fault driver, progressive trust restoration, and release approval remain open. See [`documentation-gap-delta-2026-08-12-q22-fault-bundle-assembly.md`](documentation-gap-delta-2026-08-12-q22-fault-bundle-assembly.md).

The bounded Q23 slice adds `ProcessFaultInjectionRuntime`, which starts an explicitly configured helper with exact target/point/isolation inputs, clears inherited environment state, kills timed-out children, and parses one strict JSON `FaultObservation`. Non-zero exit, timeout, malformed output, unknown values, and identity mismatches fail closed. Provider sandbox adapters, Docker-backed qualification execution, progressive trust restoration, and release approval remain open. See [`documentation-gap-delta-2026-08-12-q23-fault-process-runtime.md`](documentation-gap-delta-2026-08-12-q23-fault-process-runtime.md).

The bounded Q24 slice adds `ProgressiveTrustRestorationService`, which persists `REVALIDATING` before evaluating exact incident/revalidation/bundle relationships. `Trusted` promotion requires exact manifest identity, a qualified matching baseline, and the full local `TrustedQualificationGate`; missing or failed closure remains `Untrusted`, inconclusive revalidation remains `Revalidating`, and degraded success remains `DegradedTrust`. See [`documentation-gap-delta-2026-08-12-q24-progressive-trust-restoration.md`](documentation-gap-delta-2026-08-12-q24-progressive-trust-restoration.md).

The bounded Q25 slice adds `ReleaseApprovalService`, a fail-closed aggregate gate for an exact `RELEASE`/`TRUSTED` manifest-bound and passed bundle, qualified matching baseline, `Trusted` scope state, full Trusted hard-gate closure, valid trusted signatures with upstream cryptographic verification, and non-blank known limitations. It is an application decision boundary, not live deployment evidence or a v1.0 claim; KMS/HSM/Vault custody, provider adapter qualification, and exact-environment release execution remain open. See [`documentation-gap-delta-2026-08-12-q25-release-approval.md`](documentation-gap-delta-2026-08-12-q25-release-approval.md).

The bounded Q26 slice adds `CapabilityRestorationService` and an explicit capability-to-stage policy. Diagnostics remain available while `Untrusted`/`Revalidating`; `DegradedTrust` can restore only evidence-backed deterministic writes; semantic mutation and external effects require `Trusted`, qualification evidence, revalidation evidence, and referenced facts. Unknown capabilities and unreferenced evidence remain blocked. This is an application decision boundary, not durable grant issuance or live provider/KMS qualification. See [`documentation-gap-delta-2026-08-12-q26-capability-restoration.md`](documentation-gap-delta-2026-08-12-q26-capability-restoration.md).

The bounded Q27 slice adds `CryptoAdapterQualificationService`, which requires exact production custody/provider/key metadata and evidence for authorized resolution, scope isolation, cryptographic round-trip, lifecycle/rotation, secret non-disclosure, and referenced facts. It explicitly rejects development-only local-file custody for production qualification. Q28 adds `V1ReleaseEvidenceService`, which fail-closes unless exact release/build/configuration/environment identity matches and typed release approval, runtime, crypto, recovery/fault, and capability-restoration decisions all pass. Q29 corrects the PostgreSQL diagnostics lease check to use the canonical `run_leases.lease_until` column. These are application/runtime boundaries, not live KMS/HSM/Vault or final Docker certification evidence; backend integrations and exact-environment execution remain open. See [`documentation-gap-delta-2026-08-12-q27-crypto-adapter-qualification.md`](documentation-gap-delta-2026-08-12-q27-crypto-adapter-qualification.md), [`documentation-gap-delta-2026-08-12-q28-v1-release-evidence.md`](documentation-gap-delta-2026-08-12-q28-v1-release-evidence.md), and [`documentation-gap-delta-2026-08-12-q29-diagnostics-lease-schema.md`](documentation-gap-delta-2026-08-12-q29-diagnostics-lease-schema.md).

## 9. Diagnostics / doctor / rebuild currently wired

Gap analysis corrected an earlier documentation assumption: **`doctor` and `rebuild` are implemented in the current main snapshot.**

### Doctor

`vestrace doctor`:

- connects to PostgreSQL;
- constructs `PgDiagnosticsRepository` + `DoctorService`;
- runs checks for migrations, extensions, broken refs, outbox lag, missing search documents, stale model health, expired leases and dead-letter jobs;
- prints Error/Warning/Info findings;
- returns non-zero on diagnostic errors.

### Rebuild

Current rebuild CLI supports conceptual targets:

```text
SearchDocuments
Embeddings
All
```

Search-document rebuild directly performs a deterministic SQL projection reconstruction and then runs diagnostics verification.

The current embeddings rebuild path does **not** regenerate embeddings; it validates prerequisite search-document coverage and returns zero rebuilt vectors. It must therefore be treated as incomplete functionality.

Target v0.5 will preserve useful low-level rebuild logic but place it behind:

```text
InvariantDefinition
→ HealthFinding
→ immutable RepairPlan
→ capability/policy
→ execution
→ verification
→ finding closure
```

The H1–H5 implementation exposes the domain/application contract for that chain. `vestrace plan` and `vestrace repair` are deliberately contract-only CLI surfaces: they do not load an implicit plan, mutate SQL projections, or start an execution without an integration adapter.

## 10. Current diagnostics vs target Health model

Current legacy `DiagnosticFinding` contains:

- code;
- severity `Error/Warning/Info`;
- optional object;
- message;
- remediation.

It does not contain the target shape. The additive `health` module provides that target contract through `InvariantDefinition`, `HealthFinding`, `HealthOccurrence`, `RepairPlan`, `VerificationRun`, and explicit lifecycle/disposition types; the legacy doctor report remains a separate diagnostic projection until a durable adapter is designed.

## 11. Current tool/external-effect seed

Current ToolInvocation models:

```text
Prepared
Committed
Verified
Failed
```

This remains a useful legacy projection and is not the canonical owner of the new external-effect contract. The additive E1–E4 boundary now provides:

- immutable `ExternalEffectIntent`;
- adapter delivery/idempotency/reconciliation declarations;
- `UNKNOWN` / `RECONCILING` outcome;
- immutable `ExternalEffectReceipt`;
- ACK vs confirmed outcome distinction;
- reversibility and compensation relation;
- precondition recheck/stale intent;
- deterministic fault-suite and Connect-gate contracts.

No durable provider execution, universal exactly-once guarantee or high-risk autonomous external-effect support should be inferred from this additive contract.

## 12. Recovery currently wired

Run checkpoint/replay/projection recovery is implemented and should be preserved.

The bounded T1–T8 contract now establishes additive domain semantics for:

- Incident lifecycle and containment-first recovery;
- explicit TrustState and revalidation barrier;
- startup recovery classification (`SAFE_TO_RESUME`, `MUST_RECONCILE`, etc.);
- typed RevalidationRun/RecoveryPoint evidence;
- target-bound QualificationBundle/Baseline and TRUSTED gate.

Post-incident qualification now has a typed local evidence input and a fail-closed bundle lifecycle; deterministic recovery action evaluation, durable Incident/RevalidationRun/TrustState/RecoveryPoint and external effect/reconciliation persistence, startup recovery orchestration, workspace-scoped UNKNOWN-effect reconciliation, an injected deterministic effect fault-suite boundary, fault-evidence-backed qualification-bundle assembly, an isolated subprocess fault-runtime boundary, and evidence-backed progressive trust restoration are available. Capability-level restoration policy and release approval remain open.

## 13. Crypto / data-governance seeds

Current code/schema contains useful seeds:

- sensitivity levels;
- redaction destinations;
- WorkspaceKek alias/algorithm metadata;
- ArtifactRevision content hash;
- SignedRunExport type;
- RLS.

The bounded T1–T8 contract now provides opaque `SecretRef`, `KeyReference` lifecycle, `DataPolicy`, conservative classification lineage, retention/hold/deletion verification, governed export and audit-chain contracts. Q9 adds an exact metadata signer trust policy and provider-port local signing boundary. No KMS/HSM/Vault adapter, durable secret/key store or signer-policy store, deletion executor, or Qualification-backed crypto/governance deployment profile is wired.

## 14. Artifacts

Current domain has Artifact and ArtifactRevision including media type/content hash/byte size, and artifact-related migrations exist.

The target requires additional governed storage/classification/provenance semantics and actual local content-addressed-storage wiring. Presence of artifact domain/schema does not prove that CAS is an active authoritative runtime path.

## 15. Acceptance / qualification state

Current v0.1 acceptance suite is valuable and includes deterministic scenario seeds for memory lifecycle, retrieval, workspace denial, provider sensitivity, self-elevation, purge approval, degraded mode, model fallback and audit.

However, many v0.1 acceptance scenarios compose domain objects directly rather than proving every wired runtime surface.

The suite is **not** target conformance/qualification because it lacks:

- stable requirement-ID mapping;
- profile dependency closure;
- deterministic named fault injection around crash/effect boundaries;
- runtime recovery execution and reconciliation after process failure;
- exact build/config/environment identity;
- hard-gate profile runner/known-limitations contract.

## 16. Current API/CLI surfaces

Current implementation includes HTTP/MCP/CLI surfaces beyond the earliest P0 documentation. The exact current API surface should be read from router/source/schema artifacts rather than stale historical README claims.

Confirmed current CLI functionality includes at least:

- `server`;
- `migrate`;
- `worker`;
- `mcp`;
- `doctor`;
- `rebuild`.

Target architecture examples for future `repair`/`qualify` semantics do not imply those commands are currently implemented.

## 17. Target features not implied by current types

The current snapshot does **not** establish complete target support for:

- Claim-based persistent cognition model;
- full temporal/as-of semantics;
- complete mutation/reconciliation engine;
- ContextPack 2.0;
- governed learning lifecycle;
- universal risk-policy integration beyond the bounded G2 entry-point mappings (the bounded G1/G2/G3 grant, decision, boundary, attenuation, and budget slices are described above);
- durable cross-workspace grant+mount/trust persistence and federation adapters;
- findings-first Health/Repair protocol;
- governed ExternalEffectIntent/receipt/reconciliation;
- Incident/Trust/Revalidation model;
- target Crypto/Data Governance contract;
- durable QualificationBundle persistence now exists through the bounded Q2 repository slice, Q4 emits a machine-readable capability/deployment assertion, Q5 binds that assertion into bundle identity, Q6 adds a read-only `conformance verify` deployment gate for exact binding, migration compatibility, and restricted runtime-role evidence, Q7 adds structured Ed25519 signatures, Q8 wires automatic server/worker qualification, Q9 adds provider-bound local signing plus signer trust evaluation, Q10 adds post-incident evidence gating, Q11 adds durable recovery/trust evidence persistence, Q12 adds startup recovery orchestration, Q13 adds UNKNOWN-effect reconciliation, Q14 adds durable effect/reconciliation evidence, Q15 adds the deterministic effect fault-suite application boundary, Q17 adds durable fault evidence, Q20 maps it to `QUAL-008`, Q21 wires the configured runtime hook, Q22 assembles and persists fault-evidence-backed bundles, Q23 adds an isolated subprocess fault-runtime boundary, Q24 adds fail-closed progressive trust restoration, Q25 adds aggregate Trusted release approval, Q26 adds capability-level restoration policy, Q27 adds a production crypto/provider qualification boundary, and Q28 adds the exact-environment v1.0 evidence gate. Live KMS/HSM/Vault custody, provider adapters, Docker/runtime execution evidence, and a passing v1.0 deployment claim remain open.

## 18. Documentation rule

When current implementation and target v0.2 architecture differ:

1. this file describes current wiring from the inspected snapshot;
2. `docs/specs/vestrace-*.md` describes normative target architecture;
3. accepted `docs/adr/*` resolves target architecture decisions;
4. `docs/gap-analysis-v0.2.md` describes the transition gap;
5. no target feature may be claimed implemented without a wired runtime path and appropriate conformance evidence.

# Vestrace Version Roadmap — v0.2 to v1.0

> **English reading edition · 2026-09-08.** Complete editorial translation of the [frozen original](../vestrace-version-roadmap-v0.2-to-v1.0.md) from supplied snapshot `3e05dfbd`. Requirement IDs, normative strength, technical states, and examples are preserved. This translation does not amend the original contract or claim implementation. If wording differs, the frozen original and applicable Accepted ADRs take precedence.

**Status:** Normative release-readiness roadmap
**Date:** 2026-08-10
**Basis:** Architecture Contract v0.2, Qualification / Conformance Specification, and ADR-0010.

## 1. Roadmap principle

A version is defined by completion of its architectural capability layer and the required evidence gate, not the number of feature flags. Milestone labels and named qualification profiles are related but are not synonyms.

```text
v0.2 Correct
→ v0.3 Learn
→ v0.4 Govern
→ v0.5 Understand
→ v0.6 Connect
→ v1.0 Trust
```

Each successive milestone inherits earlier hard requirements. A named profile can be claimed only with complete applicable dependency/evidence closure for the exact target.

---

# 2. v0.2 — Correct

## Goal

Make persistent cognition correct, reproducible, and temporally consistent.

## Required architecture scope

- Persistent Cognition Core;
- Temporal & Concurrency;
- Mutation & Reconciliation;
- Retrieval / ContextPack 2.0;
- the minimum Capability Governance needed to protect the memory boundary.

## Required qualification profiles

```text
CORE
+
MEMORY
```

## Exit criteria

- immutable revisions and provenance;
- current/as-of/timeline semantics;
- optimistic concurrency;
- explicit conflict/reconciliation;
- retrieval does not present superseded state as current;
- ContextPack respects budget, provenance, and security;
- derived retrieval state rebuildable;
- Memory scope does not expand workspace authority;
- no unauthorized workspace access;
- profile-scoped foundational capability/default-deny/no-self-escalation checks pass;
- requirement families `ARC/MEM/TMP/MUT/RET` pass their applicable hard gates.

`RET-014` may be `NOT_APPLICABLE` only when the qualified target excludes mounted cross-workspace retrieval; applicability requires evidence.

## Non-goal

Full capability governance, autonomy, federation, cryptographic governance, and external side effects are not v0.2 release gates.

---

# 3. v0.3 — Learn

## Goal

Make execution feedback a durable source of cognitive improvement without uncontrolled self-modification.

## Adds

- Execution Feedback & Learning;
- evaluation provenance;
- model/tool performance evidence;
- learning proposals/versioned cognitive asset changes;
- Cognition Bench baseline.

## Required profile

```text
COGNITION
```

## Exit criteria

- raw feedback is separate from learned projections;
- deterministic/human evidence has explicit authority;
- model-judge signals do not automatically become canonical;
- agents, skills, and workflows do not change secretly;
- long-horizon, contradiction, provenance, and forgetting benchmarks are defined and reproducible.

---

# 4. v0.4 — Govern

## Goal

Make authority, delegation, workspace isolation, and controlled sharing complete runtime boundaries.

## Adds

- Capability Governance complete;
- Identity / Workspace / Federation;
- role templates + runtime capabilities;
- risk/approval/budget model;
- delegation attenuation;
- MemoryShareGrant + MemoryMount;
- federation trust/evidence model.

## Required qualification state

```text
CORE
MEMORY
COGNITION
+
full CAP/IDW governance evidence
+
FEDERATION only when enabled and its complete applicable evidence closure passes
```

## Exit criteria

- default deny;
- no self-escalation;
- child authority cannot exceed parent;
- approvals bound immutable intent;
- workspace isolation bypass suite passes;
- cross-workspace sharing requires source + target consent;
- mounted retrieval satisfies `RET-014`;
- no wildcard/transitive sharing;
- federation identity does not imply data permission;
- a formal FEDERATION profile claim is made only with complete enabled-profile evidence closure.

---

# 5. v0.5 — Understand

## Goal

Make system integrity observable and automatically recoverable only where demonstrably safe.

## Adds

- Health / Integrity / Repair;
- invariant registry;
- findings-first health;
- hybrid checks;
- RepairPlan / RepairExecution / Verification;
- recurrence/flapping;
- scope-aware health propagation.

## Qualification status

`Understand` is a milestone label, not a separate named qualification profile. Its HLT/repair evidence belongs to the subsequent AUTONOMY/TRUSTED dependency closure.

## Exit criteria

- every health state backed by findings/check evidence;
- `UNKNOWN != HEALTHY`;
- deterministic repair does not mutate higher-authority layers;
- repair plan stale protection;
- finding resolution requires verification;
- flapping stops repair loops;
- doctor/diagnostic interface truthfully separates inspect/plan/execute.

---

# 6. v0.6 — Connect

## Goal

Make external side effects safe, auditable, and recoverable under uncertain outcomes.

## Adds

- External Effects;
- effect adapters;
- intent-before-dispatch;
- delivery semantics;
- adapter idempotency/reconciliation;
- effect receipts;
- compensation model;
- external effect budgets;
- adapter qualification.

## Required profile

```text
AUTONOMY
```

A formal AUTONOMY claim requires complete applicable dependency closure. Under ADR-0010, AUTONOMY crash/fault safety covers governed execution, repair, and effect state across process failure; installation-level Incident/TrustState restoration and post-incident revalidation belong to TRUSTED.

## Exit criteria

- no universal exactly-once claim;
- UNKNOWN outcome first-class;
- timeout does not mean effect absent;
- unsafe retry after ambiguous dispatch prohibited;
- approval bound exact effect intent;
- compensation distinct from rollback;
- deterministic crash/fault scenarios around execution/effect boundaries pass;
- ambiguous effect state is safely preserved/reconstructed after process failure;
- adapters publish limitations.

## Non-claim

v0.6/AUTONOMY does not imply TRUSTED incident containment, trust restoration, or post-incident revalidation.

---

# 7. v1.0 — Trust

## Goal

Make Vestrace an evidence-backed trusted cognitive runtime for production-like autonomous operation.

## Adds / completes

- Incident / Recovery / Revalidation;
- Crypto / Data Governance;
- Qualification / Conformance;
- audit integrity profile;
- signed manifests/bundles where configured;
- deletion/export/retention governance;
- deployment qualification;
- post-incident requalification;
- trust-aware progressive restoration.

## Required profile

```text
TRUSTED
```

or, for federated deployments:

```text
TRUSTED-FEDERATED
```

## Exit criteria

- process restart does not imply trust;
- recovery requires revalidation before trust restoration;
- crypto anomalies affect trust;
- secrets are not ordinary memory;
- classification/lineage/purpose enforced at model/export/federation boundaries;
- deletion semantics are explicit and verified;
- qualification bundle identifies exact build/config/environment;
- security/governance hard gates cannot be offset by quality metrics;
- known limitations are published;
- full TRUSTED dependency closure passes;
- federation deployment claims TRUSTED-FEDERATED only when FEDERATION closure also passes.

---

# 8. Cross-version principles

## 8.1 No feature-driven version inflation

A wider API surface alone does not justify increasing the version.

## 8.2 No hidden architecture debt

A milestone implemented by bypassing a normative boundary is incomplete even when its demo works.

## 8.3 Documentation before implementation

Before code work, each milestone needs:

- normative requirement mapping;
- domain/API/adapter semantics;
- acceptance/qualification scenarios;
- migration impact analysis where necessary.

## 8.4 Current implementation status separate from target roadmap

README/current architecture documentation must separately show:

- what is implemented now;
- what belongs to the target architecture;
- which profile has actually been qualified.

A roadmap section does not establish implementation availability.

## 8.5 Milestone vs profile claim

Release metadata MUST record milestone labels and claimed qualification profiles separately. Partial capability evidence must not be described as a complete profile claim.

---

# 9. Release evidence

Every release SHOULD have:

```text
ReleaseEvidence
├─ version
├─ source revision
├─ build digest
├─ schema version set
├─ milestone label
├─ claimed profiles
├─ qualification bundle refs
├─ applicability decisions
├─ known limitations
└─ signed manifest? (profile-dependent)
```

---

# 10. Roadmap completion definition

The roadmap is fulfilled not when every checkbox is manually marked, but when the v1.0 target passes the TRUSTED profile for a particular supported deployment configuration and the Architecture Contract has no unmet applicable MUST requirements for that profile.

# Vestrace Version Roadmap — v0.2 to v1.0

**Статус:** normative release-readiness roadmap  
**Дата:** 2026-08-10  
**Основание:** Architecture Contract v0.2 + Qualification / Conformance Specification.

## 1. Принцип roadmap

Версия определяется не количеством feature flags, а завершённостью соответствующего архитектурного capability layer и прохождением требуемого qualification profile.

```text
v0.2 Correct
→ v0.3 Learn
→ v0.4 Govern
→ v0.5 Understand
→ v0.6 Connect
→ v1.0 Trust
```

Каждый следующий milestone наследует предыдущие hard requirements.

---

# 2. v0.2 — Correct

## Цель

Сделать persistent cognition корректным, воспроизводимым и темпорально согласованным.

## Required architecture scope

- Persistent Cognition Core;
- Temporal & Concurrency;
- Mutation & Reconciliation;
- Retrieval / ContextPack 2.0;
- минимально необходимый Capability Governance для защиты memory boundary.

## Required qualification profiles

```text
CORE
+
MEMORY
```

## Exit criteria

- immutable revisions и provenance;
- current/as-of/timeline semantics;
- optimistic concurrency;
- explicit conflict/reconciliation;
- retrieval не возвращает superseded state как current;
- ContextPack соблюдает budget/provenance/security;
- derived retrieval state rebuildable;
- no unauthorized workspace access;
- requirement families `ARC/MEM/TMP/MUT/RET` проходят hard gates.

## Non-goal

Полная автономность, federation, crypto governance и внешние side effects не являются release gate v0.2.

---

# 3. v0.3 — Learn

## Цель

Сделать execution feedback устойчивым источником улучшения cognition без неконтролируемой self-modification.

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

- raw feedback отделён от learned projections;
- deterministic/human evidence имеет понятную authority;
- model-judge signals не становятся canonical автоматически;
- agents/skills/workflows не меняются скрыто;
- long-horizon, contradiction, provenance и forgetting benchmarks определены и воспроизводимы.

---

# 4. v0.4 — Govern

## Цель

Сделать authority, delegation, workspace isolation и controlled sharing полноценной runtime boundary.

## Adds

- Capability Governance complete;
- Identity / Workspace / Federation;
- role templates + runtime capabilities;
- risk/approval/budget model;
- delegation attenuation;
- MemoryShareGrant + MemoryMount;
- federation trust/evidence model.

## Required profiles

```text
CORE
MEMORY
COGNITION
+
FEDERATION where federation is enabled
```

## Exit criteria

- default deny;
- no self-escalation;
- child authority cannot exceed parent;
- approvals bound immutable intent;
- workspace isolation bypass suite passes;
- cross-workspace sharing requires source + target consent;
- no wildcard/transitive sharing;
- federation identity does not imply data permission.

---

# 5. v0.5 — Understand

## Цель

Сделать целостность системы наблюдаемой и автоматически восстанавливаемой только там, где это доказуемо безопасно.

## Adds

- Health / Integrity / Repair;
- invariant registry;
- findings-first health;
- hybrid checks;
- RepairPlan / RepairExecution / Verification;
- recurrence/flapping;
- scope-aware health propagation.

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

## Цель

Сделать external side effects безопасными, auditable и recoverable при неопределённых outcomes.

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

## Exit criteria

- no universal exactly-once claim;
- UNKNOWN outcome first-class;
- timeout does not mean effect absent;
- unsafe retry after ambiguous dispatch prohibited;
- approval bound exact effect intent;
- compensation distinct from rollback;
- fault scenarios around dispatch boundary pass;
- adapters publish limitations.

---

# 7. v1.0 — Trust

## Цель

Сделать Vestrace evidence-backed trusted cognitive runtime для production-like autonomous operation.

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

или для federation deployments:

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
- full TRUSTED dependency closure passes.

---

# 8. Cross-version principles

## 8.1 No feature-driven version inflation

Версия не повышается только потому, что API surface стал шире.

## 8.2 No hidden architecture debt

Если milestone реализован через обход normative boundary, он не считается завершённым даже при работающем demo.

## 8.3 Documentation before implementation

Для каждого milestone до code work должны существовать:

- normative requirement mapping;
- domain/API/adapter semantics;
- acceptance/qualification scenarios;
- migration impact analysis, если требуется.

## 8.4 Current implementation status separate from target roadmap

README/current architecture docs должны отдельно показывать:

- что реализовано сейчас;
- что target architecture;
- какой profile фактически квалифицирован.

Наличие roadmap section не означает implementation availability.

---

# 9. Release evidence

Каждый release SHOULD иметь:

```text
ReleaseEvidence
├─ version
├─ source revision
├─ build digest
├─ schema version set
├─ claimed profiles
├─ qualification bundle refs
├─ known limitations
└─ signed manifest? (profile-dependent)
```

---

# 10. Roadmap completion definition

Roadmap считается выполненным не когда все пункты отмечены вручную, а когда v1.0 target проходит TRUSTED profile для конкретной supported deployment configuration и Architecture Contract не содержит незакрытых MUST requirements для этого profile.

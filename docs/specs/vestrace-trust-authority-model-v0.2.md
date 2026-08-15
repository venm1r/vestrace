# Vestrace Trust & Authority Model v0.2

**Статус:** normative trust/governance specification  
**Дата:** 2026-08-10  
**Базовые документы:**

- `vestrace-architecture-contract-v0.2.md`
- `vestrace-domain-model-v0.2.md`
- `vestrace-normative-invariants-v0.2.md`

## 1. Назначение

Этот документ определяет, кто и при каких условиях может читать, изменять, исполнять, делегировать, экспортировать и восстанавливать состояние Vestrace.

Главное правило:

> **Identity describes who/what an actor is. Capability and policy determine what that actor may do. Trust describes what evidence-backed confidence Vestrace has in a scope/state. Эти три понятия не взаимозаменяемы.**

---

# 2. Основные понятия

## 2.1 Identity

Identity отвечает на вопрос: **кто действует?**

Примеры:

- user;
- agent;
- service account;
- workflow;
- system component;
- federated principal.

Identity MUST быть stable/typed и auditable.

## 2.2 Authority

Authority отвечает на вопрос: **что разрешено этому субъекту сейчас?**

Authority вычисляется из:

```text
identity / membership
    ↓
role templates (optional issuance input)
    ↓
capability grants
    ↓
delegation attenuation
    ↓
policy evaluation
    ↓
risk + budget + conditions
    ↓
approval obligations
    ↓
effective authority
```

## 2.3 Trust

Trust отвечает на вопрос: **насколько состояние данного scope доказанно пригодно для дальнейших операций?**

Trust не является permission и не является health score.

---

# 3. Default deny

Vestrace использует fail-closed authorization.

Если система не может доказать, что operation разрешена соответствующей capability + policy + data-governance checks, operation MUST быть denied или переведена в non-effectful prepare/diagnostic mode.

Отсутствие explicit deny не означает allow.

---

# 4. Roles

## 4.1 Role как template

Role MAY содержать шаблон желаемых capability grants для удобства администрирования.

```text
RoleTemplate
├─ role_id
├─ name
├─ suggested_capabilities[]
├─ default_constraints[]
└─ version
```

Role name не проверяется как фактическое runtime permission.

## 4.2 Role assignment

Role assignment является input к capability issuance/reconciliation, но operation authorization MUST работать по effective capability set.

Изменение role template не должно скрыто менять уже выданные immutable/explicit grants, если policy не определяет dynamic role binding отдельным контрактом.

---

# 5. Capability model

## 5.1 CapabilityGrant

```text
CapabilityGrant
├─ grant_id
├─ issuer
├─ subject
├─ operation_selector
├─ resource_selector
├─ tool_selector?
├─ valid_from
├─ valid_until?
├─ budget_constraints?
├─ risk_ceiling
├─ conditions[]
├─ obligations[]
├─ delegation_policy
├─ lifecycle_state
└─ audit_metadata
```

## 5.2 Operation selector

Capability должна быть granular. Примеры semantic operations:

```text
memory.read
memory.write
memory.revise
memory.delete
memory.purge
context.retrieve
artifact.read
artifact.write
execution.start
execution.cancel
external_effect.prepare
external_effect.dispatch
capability.delegate
workspace.admin
share.create
share.accept
share.revoke
export.create
incident.contain
repair.execute
revalidation.execute
```

Каталог operation names должен версионироваться отдельно от role names.

## 5.3 Resource selector

Capability MAY быть ограничена exact resource, resource class, workspace, project или typed selector.

Wildcard selector допускается только там, где он явно разрешён policy и не пересекает запрещённую authority boundary.

Cross-workspace wildcard запрещён для memory sharing.

## 5.4 Validity

Capability вне своего validity interval считается недействительной.

Clock uncertainty/expiry conflict разрешается fail closed для risky operations.

## 5.5 Conditions

Conditions MAY включать:

- trust state requirement;
- classification ceiling;
- local-only provider;
- human-present requirement;
- allowed schedule/window;
- specific execution/run;
- target/resource state;
- precondition version;
- network/location/environment profile.

---

# 6. Capability lifecycle

Рекомендуемый lifecycle:

```text
PROPOSED
  ↓
ACTIVE
  ├─ SUSPENDED
  ├─ EXPIRED
  ├─ REVOKED
  └─ REPLACED
```

`REVOKED`/`EXPIRED` grants не возвращаются в ACTIVE; выдаётся новый grant/revision.

Revocation прекращает future authority, но не переписывает исторические actions, совершённые при действовавшем grant.

---

# 7. Effective authority computation

## 7.1 Evaluation inputs

Перед governed operation вычисляются:

1. actor identity;
2. exact operation;
3. exact resource/target;
4. current workspace scope;
5. direct grants;
6. delegated grants;
7. trust state;
8. risk classification;
9. budget state;
10. DataPolicy/classification constraints;
11. applicable approvals;
12. environmental/precondition state.

## 7.2 Decision

```text
EffectiveAuthorityDecision
├─ operation
├─ subject
├─ resource
├─ matched_grants[]
├─ policy_versions[]
├─ effective_risk
├─ budget_state
├─ trust_state
├─ result
├─ obligations[]
└─ evidence_refs[]
```

Result:

- `DENY`;
- `ALLOW`;
- `PREPARE_ONLY`;
- `REQUIRE_APPROVAL`.

## 7.3 Precedence

Normative precedence:

```text
hard deny / hard boundary
      > capability allow
      > approval
```

Approval подтверждает конкретное действие, но не создаёт отсутствующую base authority и не отменяет hard deny.

---

# 8. Risk model

## 8.1 Categories

```text
LOW
MEDIUM
HIGH
CRITICAL
```

## 8.2 Intrinsic vs effective risk

`intrinsic_risk` задаётся operation/adapter/resource semantics.

`effective_risk` вычисляется с учётом context:

- classification;
- trust state;
- reversibility;
- blast radius;
- amount/cost;
- novelty;
- target criticality;
- externality;
- unresolved health findings/incidents.

Context MAY повысить risk.

Понижение intrinsic risk допускается только по versioned rule, которая доказывает уменьшение exposure (например native dry-run вместо dispatch).

## 8.3 Default policy

Пример базовой маршрутизации:

```text
LOW      → capability + policy
MEDIUM   → capability + policy + stronger logging/verification
HIGH     → approval or explicit high-risk authority
CRITICAL → explicit critical authority + approval + strong preconditions
```

Конкретный profile задаётся policy, а не hardcoded в UI.

---

# 9. Approval model

## 9.1 Approval binds immutable intent

Approval MUST быть привязан к immutable operation hash/intent.

Material fields включают:

- actor/delegated actor;
- operation;
- target;
- relevant arguments/content digest;
- risk;
- budget impact;
- classification/export destination;
- expected external effect.

Изменение material field инвалидирует approval.

## 9.2 Approval lifecycle

```text
REQUESTED
  ↓
AWAITING_APPROVAL
  ├─ APPROVED
  ├─ REJECTED
  └─ EXPIRED
```

Approval имеет expiry и MAY иметь usage count (обычно one-shot для dangerous intent).

## 9.3 Approval cannot override

Approval MUST NOT преодолевать:

- missing capability ceiling;
- explicit deny;
- hard budget;
- revoked workspace/share relationship;
- DataPolicy prohibition;
- trust barrier, если policy требует revalidation;
- stale material preconditions.

---

# 10. Delegation and subagents

## 10.1 Attenuation

Delegation всегда уменьшает или сохраняет authority:

```text
child authority =
parent effective authority
∩ explicit delegated subset
∩ child-specific constraints
∩ current policy
```

Никакая child capability не может возникнуть только потому, что child agent «нуждается» в ней.

## 10.2 Explicit delegation

Parent должен иметь право delegating relevant authority.

`capability.delegate` MAY быть ограничено:

- capability family;
- resource scope;
- max depth;
- max lifetime;
- max risk;
- budget;
- allowed child types.

## 10.3 Depth

Safe default delegation depth — `1`.

Policy MAY увеличить depth, но chain должен оставаться auditable и bounded.

## 10.4 Budget delegation

Child MAY получить только выделенный/зарезервированный budget subset.

Child usage должен учитываться и в собственном allocation, и в соответствующем parent/global budget according to accounting policy.

## 10.5 Context delegation

Delegated context и delegated authority разделены.

Наличие memory/context item в child ContextSnapshot не означает permission использовать его для любого operation/provider/export.

---

# 11. Budget governance

## 11.1 Dimensions

Budget MAY включать:

- execution steps;
- wall time;
- CPU/GPU/resource time;
- model input/output tokens;
- monetary cost;
- external tool/effect count;
- artifact bytes/count;
- subruns;
- retries;
- high-risk operations.

## 11.2 Reservation

Для конкурентно расходуемых hard budgets reservation SHOULD выполняться до irrevocable operation.

## 11.3 Exhaustion

Hard budget exhaustion приводит к DENY/STOP/PREPARE_ONLY согласно operation contract и не может быть преодолено approval, если approval не относится к отдельной budget-increase operation.

## 11.4 Audit

Reservations, releases, charges и corrections должны быть auditable и idempotent.

---

# 12. Workspace isolation

## 12.1 Single-workspace default

Обычный request/execution работает внутри одного active workspace authority context.

## 12.2 Defense in depth

Workspace isolation должна иметь минимум два независимых слоя:

1. application authorization/scope validation;
2. storage-level isolation (например PostgreSQL RLS/transaction context).

## 12.3 Foreign IDs

Знание foreign UUID/resource ID не предоставляет read permission.

## 12.4 Workspace admin

`workspace.admin` не означает автоматическую federation/global authority за пределами собственного workspace.

---

# 13. Cross-workspace memory authority

## 13.1 Two-sided consent

Memory sharing требует:

```text
source grant
+
target acceptance
```

Source решает, что допустимо раскрыть. Target решает, что допустимо принять/использовать.

## 13.2 Operation separation

Sharing policy различает:

- discover metadata;
- read content;
- include in context;
- send to model/provider;
- index/cache;
- derive local memory;
- export;
- re-share.

Разрешение одного operation не подразумевает другие.

## 13.3 No transitive authority

Target не получает право re-share mounted memory по умолчанию.

Mount-of-mount prohibited without separate source-authorized protocol.

## 13.4 Revocation

После revoke:

- новые reads/content disclosure denied;
- derived caches/indexes invalidated according to policy;
- active runs получают stale/revalidation signal;
- historical disclosure audit сохраняется.

---

# 14. Federation authority

## 14.1 Federation relationship

Federation relationship подтверждает возможность взаимодействия/identity trust, но не data/content permission.

## 14.2 Remote qualification

Remote node MAY представить qualification evidence/manifest, но локальная policy решает:

- какие signers trusted;
- какие profiles признаются;
- какой scope допускается;
- какие operations разрешены.

Self-asserted `TRUSTED` status недостаточен.

## 14.3 Remote principal

Federated principal получает локально вычисляемый effective authority, а не переносит свои remote permissions как local authority.

---

# 15. Trust state model

## 15.1 States

```text
TRUSTED
DEGRADED_TRUST
UNTRUSTED
REVALIDATING
```

Trust state вычисляется для scope, а не обязательно для всей installation.

## 15.2 Sources of trust degradation

Примеры:

- failed critical invariant;
- unresolved corruption;
- unknown external effects affecting critical state;
- cryptographic integrity failure;
- recovery from unvalidated snapshot;
- stale qualification baseline;
- policy/security bypass finding.

## 15.3 Trust increase

Повышение trust требует evidence-backed transition согласно policy.

Критический переход:

```text
UNTRUSTED/REVALIDATING
     ↓
successful required RevalidationRun
     ↓
policy decision
     ↓
TRUSTED or DEGRADED_TRUST
```

Операторский `AcceptedRisk` сам по себе не превращает UNTRUSTED в TRUSTED.

---

# 16. Trust-aware capability restrictions

Policy MAY автоматически снижать доступный operation set при деградации trust.

Пример progressive restoration:

```text
REVALIDATING
  → diagnostics/read-only

DEGRADED_TRUST
  → internal deterministic writes
  → reversible external effects (policy-dependent)

TRUSTED
  → semantic mutations
  → irreversible effects within normal policy
```

CRITICAL effects MAY требовать более строгий trust profile даже в целом TRUSTED workspace.

---

# 17. External effects authority

## 17.1 Prepare vs dispatch

Capability на `external_effect.prepare` не предоставляет `external_effect.dispatch`.

Это позволяет строить/просматривать intent без side effect.

## 17.2 Adapter declaration

Policy учитывает adapter properties:

- idempotency;
- reconciliation;
- reversibility;
- dry-run support;
- provider trust;
- data destination.

## 17.3 Unknown outcome

UNKNOWN outcome блокирует unsafe duplicate/retry paths до reconciliation/approval according to adapter contract.

---

# 18. Repair/recovery authority

## 18.1 Diagnostic capability

Read-only health/doctor operations должны быть отделены от repair execution authority.

## 18.2 Repair capability

Repair authorization должна связывать:

- exact RepairPlan;
- target scope;
- input state/preconditions;
- risk;
- reversibility;
- expiry.

## 18.3 Incident containment authority

Containment MAY иметь emergency policy path, но все emergency actions остаются auditable и не создают скрытую постоянную authority.

## 18.4 Trust restoration authority

Ни capability, ни admin role не могут напрямую выставить TRUSTED без required evidence/revalidation.

---

# 19. Data governance interplay

Authorization операции с data = минимум пересечение:

```text
identity valid
∩ capability allows
∩ workspace/share allows
∩ DataPolicy allows
∩ classification/destination allows
∩ trust allows
∩ budget allows
∩ approval obligations satisfied
```

Любой hard deny => operation denied.

---

# 20. Audit requirements

Следующие decisions/actions должны иметь audit/evidence record:

- capability issue/revoke/delegate;
- policy decision для high/critical operation;
- approvals;
- budget override/increase;
- cross-workspace grant/accept/revoke;
- sensitive export;
- restricted model/provider disclosure;
- repair/recovery execution;
- trust transition;
- emergency containment;
- federation trust establishment/change.

Audit не должен содержать secret plaintext.

---

# 21. Normative requirement mapping

Основные связанные IDs:

- `CAP-001..CAP-014` — capability/policy/delegation;
- `IDW-001..IDW-014` — workspace/sharing/federation;
- `EXT-002`, `EXT-015`, `EXT-018` — external authority;
- `HLT-010`, `HLT-018` — repair/accepted risk;
- `REC-003`, `REC-011`, `REC-017` — trust/revalidation;
- `GOV-011`, `GOV-012`, `GOV-018`, `GOV-020..022` — governance intersections.

---

# 22. Forbidden authority shortcuts

Vestrace MUST NOT использовать следующие shortcuts:

```text
role name == permission
admin == global federation authority
workspace membership == unrestricted data access
memory scope == permission
object UUID == capability
approval == capability
signed object == authorized object
TRUSTED remote claim == local trust
successful repair == TRUSTED
process running == TRUSTED
context inclusion == permission to export/use externally
```

---

# 23. Completion criteria

Trust & Authority Model считается согласованным, когда:

1. все capability operations получают единый namespaced catalog;
2. Data Governance spec ссылается на эту модель, не дублируя authority logic;
3. External Effects и Repair contracts используют prepare/authorize/execute boundary;
4. Revalidation contract является единственным механизмом повышения trust после trust-sensitive incident;
5. Qualification spec содержит negative tests для bypass/delegation/workspace/federation cases.

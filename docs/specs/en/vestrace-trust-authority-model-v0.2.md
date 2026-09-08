# Vestrace Trust & Authority Model v0.2

> **English reading edition · 2026-09-08.** Complete editorial translation of the [frozen original](../vestrace-trust-authority-model-v0.2.md) from supplied snapshot `3e05dfbd`. Requirement IDs, normative strength, technical states, and examples are preserved. This translation does not amend the original contract or claim implementation. If wording differs, the frozen original and applicable Accepted ADRs take precedence.

**Status:** Normative trust/governance specification
**Date:** 2026-08-10
**Foundational documents:**

- `vestrace-architecture-contract-v0.2.md`
- `vestrace-domain-model-v0.2.md`
- `vestrace-normative-invariants-v0.2.md`

## 1. Purpose

This document defines who may read, modify, execute, delegate, export, or restore Vestrace state, and under which conditions.

Principal rule:

> **Identity describes who/what an actor is. Capability and policy determine what that actor may do. Trust describes what evidence-backed confidence Vestrace has in a scope/state. These three concepts are not interchangeable.**

---

# 2. Core concepts

## 2.1 Identity

Identity answers: **Who is acting?**

Examples:

- user;
- agent;
- service account;
- workflow;
- system component;
- federated principal.

Identity MUST be stable, typed, and auditable.

## 2.2 Authority

Authority answers: **What is this principal permitted to do now?**

Authority is calculated from:

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

Trust answers: **How well is the state of this scope demonstrated to be fit for further operations?**

Trust is neither permission nor a health score.

---

# 3. Default deny

Vestrace uses fail-closed authorization.

When the system cannot establish that an operation is permitted by the relevant capability, policy, and data-governance checks, it MUST deny the operation or place it in a non-effectful preparation/diagnostic mode.

The absence of explicit denial does not imply permission.

---

# 4. Roles

## 4.1 Role as a template

A role MAY contain a template of desired capability grants for administrative convenience.

```text
RoleTemplate
├─ role_id
├─ name
├─ suggested_capabilities[]
├─ default_constraints[]
└─ version
```

A role name is not checked as an effective runtime permission.

## 4.2 Role assignment

Role assignment is input to capability issuance/reconciliation, but operation authorization MUST use the effective capability set.

Changing a role template must not silently change existing immutable/explicit grants unless policy defines dynamic role binding through a separate contract.

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

Capabilities must be granular. Example semantic operations:

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

The operation-name catalog is versioned independently of role names.

## 5.3 Resource selector

A capability MAY be restricted to an exact resource, resource class, workspace, project, or typed selector.

A wildcard selector is permitted only where policy explicitly allows it and it does not cross a prohibited authority boundary.

Cross-workspace wildcards are prohibited for memory sharing.

## 5.4 Validity

A capability outside its validity interval is invalid.

Clock uncertainty or expiry conflicts fail closed for risky operations.

## 5.5 Conditions

Conditions MAY include:

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

Recommended lifecycle:

```text
PROPOSED
  ↓
ACTIVE
  ├─ SUSPENDED
  ├─ EXPIRED
  ├─ REVOKED
  └─ REPLACED
```

`REVOKED`/`EXPIRED` grants do not return to ACTIVE; issue a new grant/revision.

Revocation ends future authority without rewriting historical actions performed under a valid grant.

---

# 7. Effective authority computation

## 7.1 Evaluation inputs

Before a governed operation, calculate:

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

Approval confirms a particular action but neither creates missing base authority nor overrides hard denial.

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

Operation, adapter, and resource semantics define `intrinsic_risk`.

Calculate `effective_risk` using context:

- classification;
- trust state;
- reversibility;
- blast radius;
- amount/cost;
- novelty;
- target criticality;
- externality;
- unresolved health findings/incidents.

Context MAY increase risk.

Reducing intrinsic risk requires a versioned rule establishing reduced exposure, such as a native dry run rather than dispatch.

## 8.3 Default policy

Example baseline routing:

```text
LOW      → capability + policy
MEDIUM   → capability + policy + stronger logging/verification
HIGH     → approval or explicit high-risk authority
CRITICAL → explicit critical authority + approval + strong preconditions
```

Policy defines the particular profile; the UI does not hardcode it.

---

# 9. Approval model

## 9.1 Approval binds immutable intent

Approval MUST bind an immutable operation hash/intent.

Material fields include:

- actor/delegated actor;
- operation;
- target;
- relevant arguments/content digest;
- risk;
- budget impact;
- classification/export destination;
- expected external effect.

Changing a material field invalidates approval.

## 9.2 Approval lifecycle

```text
REQUESTED
  ↓
AWAITING_APPROVAL
  ├─ APPROVED
  ├─ REJECTED
  └─ EXPIRED
```

Approval has an expiry and MAY have a usage count, typically one-shot for dangerous intent.

## 9.3 Approval cannot override

Approval MUST NOT override:

- missing capability ceiling;
- explicit deny;
- hard budget;
- revoked workspace/share relationship;
- DataPolicy prohibition;
- a trust barrier when policy requires revalidation;
- stale material preconditions.

---

# 10. Delegation and subagents

## 10.1 Attenuation

Delegation always narrows or preserves authority:

```text
child authority =
parent effective authority
∩ explicit delegated subset
∩ child-specific constraints
∩ current policy
```

A child capability cannot arise merely because the child agent needs it.

## 10.2 Explicit delegation

The parent must have permission to delegate the relevant authority.

`capability.delegate` MAY be restricted by:

- capability family;
- resource scope;
- max depth;
- max lifetime;
- max risk;
- budget;
- allowed child types.

## 10.3 Depth

Safe default delegation depth — `1`.

Policy MAY increase depth, but the chain must remain auditable and bounded.

## 10.4 Budget delegation

A child MAY receive only an allocated/reserved subset of the budget.

Child usage is charged to both its own allocation and the relevant parent/global budget according to accounting policy.

## 10.5 Context delegation

Delegated context and delegated authority are separate.

A memory/context item in a child's ContextSnapshot does not grant permission to use it for every operation, provider, or export.

---

# 11. Budget governance

## 11.1 Dimensions

A budget MAY include:

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

For concurrently consumed hard budgets, reservation SHOULD occur before an irrevocable operation.

## 11.3 Exhaustion

Hard-budget exhaustion produces DENY/STOP/PREPARE_ONLY according to the operation contract. Approval cannot override it unless the approval concerns a separate budget-increase operation.

## 11.4 Audit

Reservations, releases, charges, and corrections must be auditable and idempotent.

---

# 12. Workspace isolation

## 12.1 Single-workspace default

An ordinary request/execution operates within one active workspace authority context.

## 12.2 Defense in depth

Workspace isolation needs at least two independent layers:

1. application authorization/scope validation;
2. Storage-level isolation, such as PostgreSQL RLS/transaction context.

## 12.3 Foreign IDs

Knowing a foreign UUID/resource ID grants no read permission.

## 12.4 Workspace admin

`workspace.admin` does not automatically grant federation/global authority outside the principal's own workspace.

---

# 13. Cross-workspace memory authority

## 13.1 Two-sided consent

Memory sharing requires:

```text
source grant
+
target acceptance
```

The source decides what may be disclosed. The target decides what may be accepted and used.

## 13.2 Operation separation

Sharing policy distinguishes:

- discover metadata;
- read content;
- include in context;
- send to model/provider;
- index/cache;
- derive local memory;
- export;
- re-share.

Permission for one operation does not imply permission for the others.

## 13.3 No transitive authority

The target does not gain permission to reshare mounted memory by default.

Mount-of-mount prohibited without separate source-authorized protocol.

## 13.4 Revocation

After revocation:

- new reads/content disclosure are denied;
- derived caches/indexes invalidated according to policy;
- active runs receive a stale/revalidation signal;
- historical disclosure audit remains intact.

---

# 14. Federation authority

## 14.1 Federation relationship

A federation relationship establishes interaction eligibility/identity trust, not permission to access data or content.

## 14.2 Remote qualification

A remote node MAY present qualification evidence/manifests, but local policy determines:

- which signers are trusted;
- which profiles are recognized;
- which scope is admissible;
- which operations are permitted.

Self-asserted `TRUSTED` status is insufficient.

## 14.3 Remote principal

A federated principal receives locally calculated effective authority rather than importing remote permissions as local authority.

---

# 15. Trust state model

## 15.1 States

```text
TRUSTED
DEGRADED_TRUST
UNTRUSTED
REVALIDATING
```

Trust state is calculated per scope, not necessarily for the entire installation.

## 15.2 Sources of trust degradation

Examples:

- failed critical invariant;
- unresolved corruption;
- unknown external effects affecting critical state;
- cryptographic integrity failure;
- recovery from unvalidated snapshot;
- stale qualification baseline;
- policy/security bypass finding.

## 15.3 Trust increase

Increasing trust requires an evidence-backed transition under policy.

Critical transition:

```text
UNTRUSTED/REVALIDATING
     ↓
successful required RevalidationRun
     ↓
policy decision
     ↓
TRUSTED or DEGRADED_TRUST
```

An operator's `AcceptedRisk` does not by itself turn UNTRUSTED into TRUSTED.

---

# 16. Trust-aware capability restrictions

Policy MAY automatically narrow available operations when trust degrades.

Example progressive restoration:

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

CRITICAL effects MAY require a stricter trust profile even within an otherwise TRUSTED workspace.

---

# 17. External effects authority

## 17.1 Prepare vs dispatch

Capability for `external_effect.prepare` does not grant `external_effect.dispatch`.

This permits constructing and inspecting an intent without a side effect.

## 17.2 Adapter declaration

Policy accounts for adapter properties:

- idempotency;
- reconciliation;
- reversibility;
- dry-run support;
- provider trust;
- data destination.

## 17.3 Unknown outcome

An UNKNOWN outcome blocks unsafe duplicate/retry paths until reconciliation/approval under the adapter contract.

---

# 18. Repair/recovery authority

## 18.1 Diagnostic capability

Read-only health/doctor operations must be separate from authority to execute repairs.

## 18.2 Repair capability

Repair authorization must bind:

- exact RepairPlan;
- target scope;
- input state/preconditions;
- risk;
- reversibility;
- expiry.

## 18.3 Incident containment authority

Containment MAY use an emergency policy path, but every emergency action remains auditable and creates no hidden permanent authority.

## 18.4 Trust restoration authority

Neither a capability nor an administrative role can directly assign TRUSTED without required evidence/revalidation.

---

# 19. Data governance interplay

Authorization for a data operation is at least the intersection of:

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

Any hard denial denies the operation.

---

# 20. Audit requirements

The following decisions/actions need audit/evidence records:

- capability issue/revoke/delegate;
- policy decisions for high/critical operations;
- approvals;
- budget override/increase;
- cross-workspace grant/accept/revoke;
- sensitive export;
- restricted model/provider disclosure;
- repair/recovery execution;
- trust transition;
- emergency containment;
- federation trust establishment/change.

Audit must not contain secret plaintext.

---

# 21. Normative requirement mapping

Principal related IDs:

- `CAP-001..CAP-014` — capability/policy/delegation;
- `IDW-001..IDW-014` — workspace/sharing/federation;
- `EXT-002`, `EXT-015`, `EXT-018` — external authority;
- `HLT-010`, `HLT-018` — repair/accepted risk;
- `REC-003`, `REC-011`, `REC-017` — trust/revalidation;
- `GOV-011`, `GOV-012`, `GOV-018`, `GOV-020..022` — governance intersections.

---

# 22. Forbidden authority shortcuts

Vestrace MUST NOT use the following shortcuts:

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

The Trust & Authority Model is consistent when:

1. All capability operations use one namespaced catalog.
2. The Data Governance specification references this model without duplicating authority logic.
3. External Effects and Repair contracts use the prepare/authorize/execute boundary.
4. The Revalidation contract is the sole mechanism for raising trust after a trust-sensitive incident.
5. The Qualification specification includes negative tests for bypass, delegation, workspace, and federation cases.

# Security & Tenant Isolation

## Documentation status

This document describes the **current implementation security foundation** for `main@729d456f70f4de93c97d05cce795c09025c62f24`.

It is not the complete target security/governance contract.

Normative target documents:

- [`specs/vestrace-trust-authority-model-v0.2.md`](specs/vestrace-trust-authority-model-v0.2.md)
- [`specs/vestrace-crypto-data-governance-contract-v0.2.md`](specs/vestrace-crypto-data-governance-contract-v0.2.md)
- [`specs/vestrace-normative-invariants-v0.2.md`](specs/vestrace-normative-invariants-v0.2.md)

## 1. Current workspace isolation

Current runtime uses PostgreSQL Row-Level Security as a defense-in-depth workspace boundary.

A scoped transaction establishes workspace/principal context using transaction-local configuration equivalent to:

```sql
SELECT set_config('vestrace.workspace_id', '<workspace-uuid>', true);
SELECT set_config('vestrace.principal_id', '<principal-uuid>', true);
```

RLS policies then evaluate the active workspace context and reject cross-workspace access on protected tables.

Application-level workspace predicates remain required; RLS is a second isolation layer, not a replacement for authorization.

## 2. Current capability foundation

The current codebase contains granular capability identifiers, policy abstractions and workspace-scoped policy helpers.

Current capabilities include memory/event/context and other product-domain permissions.

Important target clarification:

> **Roles are administrative templates. Effective runtime authority comes from capabilities + policy.**

The complete v0.2 target additionally requires capability constraints for resource, operation/tool, validity, budget, risk, conditions and delegation attenuation.

Those target semantics must not be inferred as fully wired merely because capability types exist today.

## 3. Current sensitivity model

Current domain sensitivity ordering is:

```text
Public
Internal
Confidential
Restricted
```

The target v0.2 governance contract keeps the same four-level baseline (`PUBLIC / INTERNAL / CONFIDENTIAL / RESTRICTED`) and adds lineage-aware classification, purpose limitation, provider/model destination checks, retention/export/deletion governance and explicit secret handling.

## 4. Current redaction foundation

The current code includes a redaction service with sensitivity-aware destination filtering and recursive JSON redaction.

Target rule:

> Permission to read data is distinct from permission to transmit that data to a model/provider/export destination.

## 5. Secrets

Target v0.2 rule:

> **Secret plaintext is not ordinary Memory.**

Durable state stores references/safe metadata; secret bytes are resolved only through controlled secret backends at execution time and must not leak into logs, receipts or persisted context.

Current provider/environment handling should be interpreted as implementation foundation, not evidence that the full target SecretRef/KeyProvider governance lifecycle is already complete.

## 6. Cross-workspace / federation target

Current workspace isolation remains the safe default.

Target cross-workspace sharing does **not** relax RLS or reinterpret `global` as cross-workspace.

The normative target uses:

```text
source MemoryShareGrant
+
target MemoryMount acceptance
```

with no wildcard or implicit transitive sharing.

## 7. Trust is not authorization

The target model separates:

```text
identity != authority
health != trust
trust != permission
```

After a trust-sensitive incident/recovery, dangerous capabilities may remain restricted until revalidation evidence permits restoration.

## 8. Current limitation boundary

The current snapshot contains policy/security foundations, but the documentation must not claim complete target support for:

- universal capability-policy enforcement across every entry point;
- full subagent capability attenuation/delegation;
- full risk/budget/approval governance;
- cross-workspace grant+mount sharing;
- federation trust evaluation;
- full crypto/key lifecycle;
- data retention/export/deletion governance;
- qualification-backed TRUSTED profile.

Those are target contracts and require later implementation plus conformance evidence.

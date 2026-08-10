# ADR-0006: Cross-workspace Sharing Uses Grant + Mount

**Status:** Accepted  
**Date:** 2026-08-10

## Context

Workspace isolation is a primary security boundary, but Vestrace needs limited knowledge sharing across related workspaces. Direct cross-workspace scopes, wildcard RLS exceptions or silent copies would weaken source ownership and revoke semantics.

## Decision

Cross-workspace memory sharing is a two-sided governed relationship:

```text
source workspace
→ MemoryShareGrant (exact target)
→ target acceptance
→ MemoryMount
```

The source grant defines what may be disclosed. The target mount accepts an exact grant revision.

Mount is read-only by default and is not a local Memory, authority transfer or new source of truth.

No wildcard target, implicit transitive sharing or mount-of-mount is allowed by default.

Local derivation/import requires a separate proposal and ordinary local Memory lifecycle with provenance.

## Consequences

- source retains disclosure control;
- target retains acceptance/use control;
- revoke has clear future-access semantics;
- RLS/workspace isolation can remain intact;
- provenance survives sharing and later local derivation.

## Rejected alternatives

1. `global` scope across all workspaces.
2. Direct cross-workspace SQL joins as authorization model.
3. Automatic memory copying.
4. Target gaining write access to source Memory.
5. Transitive sharing from mounted content.

## Normative references

- Architecture Contract Block 7;
- Trust & Authority Model §13;
- Domain Model §16;
- `IDW-004..014`.

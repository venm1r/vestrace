# ADR-0003: Capabilities Are Runtime Authority

**Status:** Accepted  
**Date:** 2026-08-10

## Context

Role-based authorization is convenient administratively but too coarse for autonomous agents, subagent delegation, budgets, risk constraints and time-bounded external operations.

## Decision

Roles are templates for issuing authority. Effective runtime authority is determined by capabilities plus policy.

A capability can be restricted by:

- operation/tool;
- resource/scope;
- validity/expiry;
- budget;
- risk ceiling;
- conditions/obligations;
- delegation policy.

Default policy is deny.

Subagent authority is an explicitly delegated, further attenuated subset of parent effective authority:

```text
child authority ⊆ parent effective authority
```

Approval confirms a specific immutable intent but cannot override missing capability, explicit deny, hard budget or DataPolicy prohibition.

## Consequences

- least privilege can be expressed per operation;
- delegated agents cannot self-escalate;
- role names are not security boundaries;
- risk/budget/time constraints become enforceable parts of authority.

## Rejected alternatives

1. Roles as direct runtime permission checks.
2. Inheriting all parent permissions into subagents.
3. Human approval as universal override.
4. Tool access represented as unscoped boolean permission.

## Normative references

- Trust & Authority Model;
- Architecture Contract Block 6;
- `CAP-001..014`.

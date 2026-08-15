# Documentation Gap Delta — Configured Capability Policy Engine

**Date:** 2026-08-13
**Scope:** authorization engine selection for deployments without a grant store
**Repository state:** dirty, implementation changes uncommitted

## Why this exists

Every governed HTTP route was returning `403 authorization denied: DefaultDeny`,
so the console could not display anything. That was not a misconfiguration. The
chain is:

1. `server.rs` wired `DenyAllPolicyEngine`;
2. `DenyAllPolicyEngine::decide` evaluates against an **empty grant list**;
3. with no grant to match, `evaluate_capability_grants` returns `Deny` with
   reason `DefaultDeny`;
4. `AuthorizationBoundary::require` turns that into `ApplicationError::Policy`,
   which the HTTP layer maps to 403.

The root cause is that capability grants cannot be provisioned at all: there is
no `capability_grants` table, no grant repository or application port, and no
HTTP route that issues or lists grants. The only code outside the domain that
constructs a `CapabilityGrant` is a `#[cfg(test)]` helper in `api/runs.rs`.
Deny-all was therefore the only correct behaviour.

## Implemented bounded contract

`ConfiguredCapabilityPolicyEngine`
(`crates/vestrace-application/src/security/policy_engine.rs`) authorizes a fixed
set of capabilities named in deployment configuration, at any resource scope,
subject to a configured risk ceiling.

A grant-based engine could not serve this purpose: `CapabilityGrant` matching is
exact on capability, operation **and** `resource_scope`, and the HTTP layer sets
`resource_scope` to the concrete request path. Static grants would therefore
authorize `/v1/runs` but not `/v1/runs/{id}`, producing a console that works on
list pages and fails on detail pages.

Selection is explicit configuration, `[policy] engine`, defaulting to
`deny-all`:

- `deny-all` (default) — unchanged behaviour;
- `configured-capabilities` — requires a non-empty `policy.capabilities`;
  selecting it with an empty list is rejected at config load, because that would
  be the deny-all engine under a misleading name.

Decisions carry the new `PolicyDecisionReason::ConfiguredAllowance`, never
`GrantMatched`, and always with `matched_grant_id: None`. The server logs a
`WARN` at startup naming the policy version, capabilities and ceiling.

`PolicyInputState::from_request` became public so engines outside the domain
emit decisions carrying the same evidence as grant-based ones.

## Explicit non-claims

This is a stopgap, not governance. The configured engine provides **no**:

- per-subject scoping — any principal in any configured workspace is treated
  alike;
- validity window or expiry;
- revocation;
- budget or condition enforcement;
- grant identity, so decisions cannot be traced to an issued authority.

The only constraint it keeps is the risk ceiling, so a read-oriented allowance
cannot authorize a critical operation. It does not implement, replace or
partially satisfy the G-phase capability-grant work; grant persistence and an
issuance path remain unimplemented. A deployment that needs governed
authorization must not select this engine.

Workspace isolation is unaffected: RLS and the workspace-scoped request context
still apply to every query regardless of the engine.

## Evidence

- `tests/configured_policy.rs` — 8 tests: allowance at any scope, capability
  outside the set denied, risk ceiling enforced, empty set denies everything,
  unknown capability and blank version rejected, shared-port usability,
  configuration whitespace tolerated;
- `crates/vestrace-infrastructure/src/config.rs` — 3 tests: deny-all default,
  configured engine requires capabilities, environment list parsing;
- `crates/vestrace-cli/tests/command_contract.rs` — deny-all remains the
  default and the configured engine warns at startup.

## Deployment note

`docker-compose.yml` selects `configured-capabilities` with read-only
capabilities and a `medium` risk ceiling for local development only, with the
limits stated inline. The default for any other deployment remains deny-all.

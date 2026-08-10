# ADR-0008: v1.0 Trust Is a Qualification Contract

**Status:** Accepted  
**Date:** 2026-08-10

## Context

Feature-complete labels are weak maturity criteria for a system that governs memory, autonomous actions, recovery and sensitive data. A build can expose many features while violating critical security/recovery invariants.

## Decision

Vestrace v1.0 is defined by successful qualification against the `TRUSTED` profile for a specific build/configuration/environment, not by raw feature count.

A v1.0 trust claim requires:

- dependency closure of lower profiles;
- security/governance hard gates;
- deterministic fault/recovery scenarios;
- revalidation semantics;
- crypto/data-governance requirements;
- adapter qualification;
- known limitations;
- reproducible QualificationBundle.

Qualification is not permanent certification. Material environment/provider/backend/policy/schema changes may make it stale or invalid.

## Consequences

- release readiness becomes evidence-based;
- benchmarks cannot compensate for security failures;
- deployment-specific limitations remain explicit;
- v1.0 scope is stricter but more meaningful.

## Rejected alternatives

1. v1.0 after all roadmap features exist.
2. v1.0 when unit/integration CI is green.
3. One global compliance boolean independent of environment.
4. Quality score that averages away hard-gate failures.

## Normative references

- Qualification / Conformance Specification;
- Version Roadmap;
- `QUAL-001..018`.

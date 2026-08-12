# Documentation Gap Delta — Q23 Fault-Process Runtime

**Date:** 2026-08-12
**Scope:** isolated subprocess runtime for deterministic external-effect fault points
**Status:** implemented as a bounded additive slice; not a v1.0 qualification claim

## Implemented

- `ProcessFaultInjectionRuntime` starts an explicitly configured helper process
  for a configured non-production target.
- The child receives the exact target digest, named fault point, and isolation
  environment through dedicated environment variables.
- The parent clears inherited environment state; Windows retains only the
  minimal system variables required to start the configured executable.
- The child must emit one strict JSON observation. Non-zero exit, timeout,
  malformed JSON, unknown point/status, target mismatch, and point mismatch
  fail closed.
- Child processes are terminated when the runtime future is dropped or times
  out.

## Evidence

- Focused runtime tests: 9/9 passed.
- Coverage includes exact target/point forwarding, successful isolated child
  observation, non-zero exit, timeout, malformed output, invalid configuration,
  and the existing configured-executor boundaries.

## Explicit non-claims

The runtime supplies a subprocess boundary, not a provider-specific destructive
adapter or a Docker/container isolation proof. It does not run a release
qualification command, restore trust, or approve a v1.0 deployment. The
configured helper remains responsible for the scenario-specific observable
effect and must be selected only for an explicitly designated non-production
target.

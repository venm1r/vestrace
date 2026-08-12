# Documentation Gap Delta — Q21 Fault-Runtime Wiring

**Date:** 2026-08-12
**Scope:** configured non-production runtime wiring for the deterministic fault suite
**Status:** implemented as a bounded additive slice; not a v1.0 qualification claim

## Implemented

- `FaultInjectionSettings` requires a nonblank exact target digest and an
  explicit non-production isolation environment.
- `FaultInjectionRuntime` is the runtime hook for an actual scenario driver;
  `ConfiguredEffectFaultScenarioExecutor` adapts it to the existing fault-suite
  executor port.
- Disabled injection fails before runtime invocation.
- Enabled execution carries the exact target and requested fault point, rejects
  a point-mismatched observation, and propagates runtime failures.

## Evidence

- Focused runtime-wiring tests: 4/4 passed.
- Q20 gate-adapter tests: 4/4 passed.
- Q19 admission tests: 4/4 passed.
- Q18 orchestration tests: 3/3 passed.
- Full workspace library/no-run/format verification is rerun for the Q21
  commit.

## Explicit non-claims

The hook is a production-wiring boundary, not proof that the process was
crashed or that a provider-side destructive scenario executed. Process
termination, provider-specific fault drivers, isolation enforcement outside
the configuration contract, complete `QualificationBundle` assembly, trust
restoration, and release approval remain open.
